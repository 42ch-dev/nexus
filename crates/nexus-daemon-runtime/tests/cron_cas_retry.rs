//! V1.51 T-B P1 — Cron-side CAS retry integration tests.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.51-per-row-occ.md`
//! Spec: `concurrency.md` §7.4
//!
//! Verifies that the cron-side fire path handles CAS version conflicts
//! correctly when using `with_cas_retry` + `mark_confirmed_in_tx_with_cas`:
//! - Happy path: CAS succeeds on the first attempt.
//! - Retry path: CAS fails on the first attempt (version changed between
//!   read and write), but succeeds on retry after re-reading the version.
//! - Exhaustion: all retry attempts fail (version keeps changing),
//!   returns the `VersionMismatch` error unmodified.
//!
//! Run with: cargo test -p nexus-daemon-runtime --test cron_cas_retry

#![allow(clippy::unwrap_used)]

use nexus_local_db::cas::with_cas_retry;
use nexus_local_db::kb_extract_job::{insert_pending_with_llm, mark_confirmed_in_tx_with_cas};
use nexus_local_db::LocalDbError;
use sqlx::SqlitePool;

const OWNER: &str = "ctr_cron_cas_v151";
const WORLD: &str = "wld_cron_cas_v151";
const WORK_ID: &str = "wrk_cron_cas_v151";

async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    // Seed a world so foreign-key constraints are satisfied.
    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        OWNER,
        "Cron CAS Test World",
        "cron-cas-test",
        "private",
        "manual",
    )
    .await;
    (pool, dir)
}

/// Insert a pending extract job for adoption.
async fn seed_pending(pool: &SqlitePool) -> String {
    let payload = serde_json::json!({
        "summary": "cron CAS test character",
        "attributes": {"novel_category": "character", "aliases": []},
        "tags": ["novel"],
        "block_type": "character",
        "canonical_name": "Cron CAS Test Character",
    })
    .to_string();
    let row = insert_pending_with_llm(
        pool,
        OWNER,
        "ws",
        WORLD,
        Some(WORK_ID),
        Some(1),
        "character",
        "Cron CAS Test Character",
        &payload,
        None,
        None,
    )
    .await
    .unwrap();
    row.job_id
}

// ── Happy path: CAS succeeds first try ─────────────────────────────────

#[tokio::test]
async fn test_cron_cas_happy_path() {
    let (pool, _dir) = fresh_pool().await;
    let job_id = seed_pending(&pool).await;

    let pool_clone = pool.clone();
    let jid = job_id.clone();

    let result = with_cas_retry(Some(3), Some(10), "cron_cas_happy", || {
        let pool = pool_clone.clone();
        let jid = jid.clone();
        async move {
            // Read current version (cron-side pattern: read first).
            let (version,): (i64,) =
                sqlx::query_as("SELECT version FROM kb_extract_jobs WHERE job_id = ?")
                    .bind(&jid)
                    .fetch_one(&pool)
                    .await?;

            let mut tx = pool.begin().await?;
            let flipped = mark_confirmed_in_tx_with_cas(&mut tx, &jid, version).await?;
            tx.commit().await?;
            Ok(flipped)
        }
    })
    .await;

    assert!(result.is_ok(), "happy path should succeed: {result:?}");
    assert!(
        result.unwrap(),
        "flipped should be true for pending → confirmed transition"
    );

    // Verify the row is now confirmed.
    let (status,): (String,) =
        sqlx::query_as("SELECT promotion_status FROM kb_extract_jobs WHERE job_id = ?")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "confirmed");
}

// ── Retry path: CAS fails first, succeeds on retry ─────────────────────

#[tokio::test]
async fn test_cron_cas_retry_succeeds_after_version_mismatch() {
    let (pool, _dir) = fresh_pool().await;
    let job_id = seed_pending(&pool).await;

    // Bump the version BEFORE the first CAS attempt so the first read
    // gets the old version, but the CAS check sees the new version.
    sqlx::query("UPDATE kb_extract_jobs SET version = version + 1 WHERE job_id = ?")
        .bind(&job_id)
        .execute(&pool)
        .await
        .unwrap();
    // Now version is 1, but the closure will read it fresh on each attempt.

    let pool_clone = pool.clone();
    let jid = job_id.clone();
    let mut attempts = 0u32;

    let result = with_cas_retry(Some(3), Some(10), "cron_cas_retry", || {
        attempts += 1;
        let pool = pool_clone.clone();
        let jid = jid.clone();
        async move {
            let (version,): (i64,) =
                sqlx::query_as("SELECT version FROM kb_extract_jobs WHERE job_id = ?")
                    .bind(&jid)
                    .fetch_one(&pool)
                    .await?;

            // On the first attempt, simulate a concurrent writer by
            // bumping the version AFTER our read but BEFORE our CAS.
            // This causes a version mismatch on the first try.
            if attempts == 1 {
                sqlx::query("UPDATE kb_extract_jobs SET version = version + 1 WHERE job_id = ?")
                    .bind(&jid)
                    .execute(&pool)
                    .await?;
                // Now version is 2, but we pass expected_version=1 → mismatch.
            }

            let mut tx = pool.begin().await?;
            let flipped = mark_confirmed_in_tx_with_cas(&mut tx, &jid, version).await?;
            tx.commit().await?;
            Ok(flipped)
        }
    })
    .await;

    assert!(result.is_ok(), "retry should succeed: {result:?}");
    assert!(
        attempts >= 2,
        "expected at least 2 attempts (1 fail + 1 success), got {attempts}"
    );

    // Verify the row is now confirmed.
    let (status,): (String,) =
        sqlx::query_as("SELECT promotion_status FROM kb_extract_jobs WHERE job_id = ?")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "confirmed");
}

// ── Exhaustion: all retries fail ───────────────────────────────────────

#[tokio::test]
async fn test_cron_cas_exhaustion_returns_version_mismatch() {
    let (pool, _dir) = fresh_pool().await;
    let job_id = seed_pending(&pool).await;

    // Bump the version BEFORE the first read so the first CAS will also
    // need to fight against continuous bumps.
    sqlx::query("UPDATE kb_extract_jobs SET version = version + 1 WHERE job_id = ?")
        .bind(&job_id)
        .execute(&pool)
        .await
        .unwrap();
    // Now version is 1.

    let pool_clone = pool.clone();
    let jid = job_id.clone();

    let result: Result<bool, LocalDbError> =
        with_cas_retry(Some(3), Some(10), "cron_cas_exhaustion", || {
            let pool = pool_clone.clone();
            let jid = jid.clone();
            async move {
                let (version,): (i64,) =
                    sqlx::query_as("SELECT version FROM kb_extract_jobs WHERE job_id = ?")
                        .bind(&jid)
                        .fetch_one(&pool)
                        .await?;

                // Bump the version BEFORE our CAS to ensure a mismatch.
                sqlx::query("UPDATE kb_extract_jobs SET version = version + 1 WHERE job_id = ?")
                    .bind(&jid)
                    .execute(&pool)
                    .await?;

                let mut tx = pool.begin().await?;
                let result = mark_confirmed_in_tx_with_cas(&mut tx, &jid, version).await;
                // Roll back regardless — the CAS already failed.
                let _ = tx.rollback().await;
                result
            }
        })
        .await;

    match result {
        Err(LocalDbError::VersionMismatch { actual, .. }) => {
            assert!(
                actual.is_some(),
                "actual_version should be populated on exhaustion"
            );
        }
        other => panic!("expected VersionMismatch after max retries, got {other:?}"),
    }
}
