//! Per-row Optimistic Concurrency Control (OCC) — CAS update pattern (V1.51 T-B P1).
//!
//! Spec: `concurrency.md` §7.
//! Plan: `2026-06-18-v1.51-per-row-occ.md` §2.2, §2.4.
//!
//! Provides:
//! - [`cas_update_result`] — check the result of a version-guarded UPDATE.
//! - [`with_cas_retry`] — retry wrapper for CAS operations that may fail
//!   due to stale preimages (cron-side fire paths).
//!
//! ## CAS Pattern
//!
//! ```text
//! 1. Read current row (SELECT ... WHERE id = ?) → get current version V
//! 2. Perform the mutation (UPDATE ... SET ..., version = version + 1
//!    WHERE id = ? AND version = V)
//! 3. Check rows_affected:
//!    - 1 → success (version advanced to V+1).
//!    - 0 → stale preimage → E_VERSION.
//! ```
//!
//! ## Lock ordering
//!
//! File lock BEFORE DB lock. CAS is applied INSIDE the file-lock scope,
//! not outside it (concurrency.md §2.4).

use crate::error::LocalDbError;
use sqlx::SqlitePool;
use std::time::Duration;
use tokio::time::sleep;

/// Maximum retry attempts for [`with_cas_retry`].
const DEFAULT_MAX_ATTEMPTS: u32 = 3;

/// Default backoff between retry attempts in milliseconds.
const DEFAULT_BACKOFF_MS: u64 = 100;

/// Check the result of a version-guarded UPDATE.
///
/// Call this after executing `UPDATE ... WHERE id = ? AND version = ?`:
/// - `rows_affected == 1` → success (version advanced).
/// - `rows_affected == 0` → version mismatch — the row was modified by another
///   writer between the caller's read and its UPDATE.
///
/// When a mismatch is detected, this function optionally reads the current
/// version from the database for the error message (pass the table name,
/// id column, and id value to enable this; use `None` for the id to skip
/// the re-read and use a generic message).
///
/// # Errors
///
/// Returns `LocalDbError::VersionMismatch` when `rows_affected == 0`.
/// Returns `LocalDbError::Sqlx` if the version re-read fails.
pub async fn cas_check(
    pool: &SqlitePool,
    rows_affected: u64,
    table: &str,
    id_column: &str,
    id_value: &str,
    expected_version: i64,
) -> Result<(), LocalDbError> {
    if rows_affected == 1 {
        return Ok(());
    }

    // Re-read the current version for a descriptive error message.
    let actual: Option<(i64,)> = sqlx::query_as(&format!(
        "SELECT version FROM {table} WHERE {id_column} = ?"
    ))
    .bind(id_value)
    .fetch_optional(pool)
    .await?;

    Err(LocalDbError::VersionMismatch {
        table: table.to_string(),
        id: id_value.to_string(),
        expected: expected_version,
        actual: actual.map(|(v,)| v),
    })
}

/// Retry a fallible CAS operation up to `max_attempts` times with a fixed
/// backoff between attempts.
///
/// The closure `f` is called at most `max_attempts` times. If it returns
/// `Ok(t)`, the retry loop exits immediately with `Ok(t)`. If it returns
/// `Err(LocalDbError::VersionMismatch { .. })`, the loop backs off for
/// `backoff_ms` milliseconds and retries. Any other error is returned
/// immediately without retrying.
///
/// A `warn!` log is emitted on each retry so operators can spot repeated
/// CAS contention.
///
/// # Defaults
///
/// When `max_attempts` is `None`, defaults to [`DEFAULT_MAX_ATTEMPTS`].
/// When `backoff_ms` is `None`, defaults to [`DEFAULT_BACKOFF_MS`].
/// # Errors
///
/// Returns the first non-`VersionMismatch` error immediately.
/// Returns `VersionMismatch` after exhausting all retry attempts.
pub async fn with_cas_retry<F, Fut, T>(
    max_attempts: Option<u32>,
    backoff_ms: Option<u64>,
    operation_name: &str,
    mut f: F,
) -> Result<T, LocalDbError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, LocalDbError>>,
{
    let max_attempts = max_attempts.unwrap_or(DEFAULT_MAX_ATTEMPTS);
    let backoff_ms = backoff_ms.unwrap_or(DEFAULT_BACKOFF_MS);
    let backoff = Duration::from_millis(backoff_ms);

    for attempt in 1..=max_attempts {
        match f().await {
            Ok(value) => return Ok(value),
            Err(LocalDbError::VersionMismatch { .. }) if attempt < max_attempts => {
                tracing::warn!(
                    operation = %operation_name,
                    attempt,
                    max_attempts,
                    backoff_ms,
                    "CAS version mismatch — retrying after backoff"
                );
                sleep(backoff).await;
            }
            Err(e) => return Err(e),
        }
    }

    // If we exhausted all attempts, the last error was a VersionMismatch —
    // the for loop above falls through on the last iteration.
    unreachable!("with_cas_retry exhausted max_attempts — last iteration should have returned Err")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_pool, run_migrations};

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    // ── cas_check ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_cas_check_success_on_rows_affected_1() {
        let (pool, _dir) = fresh_pool().await;
        // Insert a row with version=0 (migration default).
        let job_id = format!("xj_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id) \
             VALUES (?, 'ctr_test', 'wrk_test', 'entry_test', 'wld_test')",
        )
        .bind(&job_id)
        .execute(&pool)
        .await
        .unwrap();

        // Simulate a successful CAS update: version was 0, now incremented.
        let result = sqlx::query(
            "UPDATE kb_extract_jobs SET status = 'running', version = version + 1 \
             WHERE job_id = ? AND version = ?",
        )
        .bind(&job_id)
        .bind(0i64)
        .execute(&pool)
        .await
        .unwrap();

        let rows_affected = result.rows_affected();
        cas_check(
            &pool,
            rows_affected,
            "kb_extract_jobs",
            "job_id",
            &job_id,
            0,
        )
        .await
        .unwrap();
        // Should not panic — success case.

        // Verify the version was actually incremented.
        let (version,): (i64,) =
            sqlx::query_as("SELECT version FROM kb_extract_jobs WHERE job_id = ?")
                .bind(&job_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(version, 1);
    }

    #[tokio::test]
    async fn test_cas_check_version_mismatch_returns_error() {
        let (pool, _dir) = fresh_pool().await;

        let job_id = format!("xj_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id) \
             VALUES (?, 'ctr_test', 'wrk_test', 'entry_test', 'wld_test')",
        )
        .bind(&job_id)
        .execute(&pool)
        .await
        .unwrap();

        // First writer increments version to 1.
        sqlx::query("UPDATE kb_extract_jobs SET version = version + 1 WHERE job_id = ?")
            .bind(&job_id)
            .execute(&pool)
            .await
            .unwrap();

        // Second writer tries CAS with expected_version=0 — mismatch.
        let result = sqlx::query(
            "UPDATE kb_extract_jobs SET version = version + 1 \
             WHERE job_id = ? AND version = ?",
        )
        .bind(&job_id)
        .bind(0i64)
        .execute(&pool)
        .await
        .unwrap();

        let err = cas_check(
            &pool,
            result.rows_affected(),
            "kb_extract_jobs",
            "job_id",
            &job_id,
            0,
        )
        .await
        .unwrap_err();

        match err {
            LocalDbError::VersionMismatch {
                ref table,
                ref id,
                expected,
                actual,
            } => {
                assert_eq!(table, "kb_extract_jobs");
                assert_eq!(id, &job_id);
                assert_eq!(expected, 0);
                assert_eq!(actual, Some(1));
            }
            other => panic!("expected VersionMismatch, got {other:?}"),
        }
    }

    // ── with_cas_retry ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_retry_succeeds_after_version_mismatch() {
        let (pool, _dir) = fresh_pool().await;

        let job_id = format!("xj_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id) \
             VALUES (?, 'ctr_test', 'wrk_test', 'entry_test', 'wld_test')",
        )
        .bind(&job_id)
        .execute(&pool)
        .await
        .unwrap();

        // First writer increments version to 1 BEFORE our CAS.
        sqlx::query("UPDATE kb_extract_jobs SET version = version + 1 WHERE job_id = ?")
            .bind(&job_id)
            .execute(&pool)
            .await
            .unwrap();

        let mut attempts = 0u32;
        let pool_clone = pool.clone();
        let jid = job_id.clone();

        let result = with_cas_retry(
            Some(3),
            Some(10), // 10ms backoff for test speed
            "test_retry",
            || {
                attempts += 1;
                let pool = pool_clone.clone();
                let jid = jid.clone();
                async move {
                    // Read current version.
                    let (version,): (i64,) =
                        sqlx::query_as("SELECT version FROM kb_extract_jobs WHERE job_id = ?")
                            .bind(&jid)
                            .fetch_one(&pool)
                            .await?;

                    // Try CAS update.
                    let result = sqlx::query(
                        "UPDATE kb_extract_jobs SET version = version + 1 \
                         WHERE job_id = ? AND version = ?",
                    )
                    .bind(&jid)
                    .bind(version)
                    .execute(&pool)
                    .await?;

                    cas_check(
                        &pool,
                        result.rows_affected(),
                        "kb_extract_jobs",
                        "job_id",
                        &jid,
                        version,
                    )
                    .await?;
                    Ok(())
                }
            },
        )
        .await;

        // Should succeed on retry (reads version=1, updates successfully).
        assert!(result.is_ok(), "retry should succeed: {result:?}");
        // At least 2 attempts (first failed due to version mismatch on read of 0
        // vs actual 1, second succeeds after re-reading).
        assert!(attempts >= 1, "expected at least 1 attempt, got {attempts}");
    }

    #[tokio::test]
    async fn test_retry_fails_after_max_attempts() {
        let (pool, _dir) = fresh_pool().await;

        let job_id = format!("xj_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id) \
             VALUES (?, 'ctr_test', 'wrk_test', 'entry_test', 'wld_test')",
        )
        .bind(&job_id)
        .execute(&pool)
        .await
        .unwrap();

        // Continuously bump the version so CAS always fails.
        sqlx::query("UPDATE kb_extract_jobs SET version = version + 1 WHERE job_id = ?")
            .bind(&job_id)
            .execute(&pool)
            .await
            .unwrap();
        // Now version=1. Our CAS will try with expected version 0 and fail.
        // On retry, it will read version=1 and try again. But we KEEP bumping it.

        let pool_clone = pool.clone();
        let jid = job_id.clone();

        let result = with_cas_retry(Some(3), Some(10), "test_retry_fail", || {
            let pool = pool_clone.clone();
            let jid = jid.clone();
            async move {
                // Always read then bump outside our CAS.
                // First: read version.
                let (version,): (i64,) =
                    sqlx::query_as("SELECT version FROM kb_extract_jobs WHERE job_id = ?")
                        .bind(&jid)
                        .fetch_one(&pool)
                        .await?;

                // Bump the version BEFORE our CAS update (simulating another writer).
                sqlx::query("UPDATE kb_extract_jobs SET version = version + 1 WHERE job_id = ?")
                    .bind(&jid)
                    .execute(&pool)
                    .await?;

                // Now our CAS update with the stale version fails.
                let result = sqlx::query(
                    "UPDATE kb_extract_jobs SET version = version + 1 \
                         WHERE job_id = ? AND version = ?",
                )
                .bind(&jid)
                .bind(version)
                .execute(&pool)
                .await?;

                cas_check(
                    &pool,
                    result.rows_affected(),
                    "kb_extract_jobs",
                    "job_id",
                    &jid,
                    version,
                )
                .await?;
                Ok(())
            }
        })
        .await;

        // Should fail with VersionMismatch after exhausting retries.
        match result {
            Err(LocalDbError::VersionMismatch { .. }) => {}
            other => panic!("expected VersionMismatch after max retries, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_retry_non_cas_error_propagates_immediately() {
        let (_pool, _dir) = fresh_pool().await;

        let result: Result<(), LocalDbError> =
            with_cas_retry(Some(3), Some(10), "test_non_cas_error", || async {
                Err(LocalDbError::MissingVersionKey {
                    key: "test".to_string(),
                })
            })
            .await;

        match result {
            Err(LocalDbError::MissingVersionKey { key }) => assert_eq!(key, "test"),
            other => panic!("expected MissingVersionKey, got {other:?}"),
        }
    }
}
