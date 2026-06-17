//! V1.50 T-B P1 — `creator world kb pending|adopt|reject` hermetic round-trip.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-kb-auto-promotion.md`
//! Spec: `.mstar/knowledge/specs/entity-scope-model.md` §5.5
//!
//! Drives `nexus42::commands::creator::world::kb` logic functions directly
//! against a fresh temp DB to exercise pending/adopt/reject without `$HOME`
//! or a daemon.
//!
//! Covers:
//! - AC3 (#3): CLI commands round-trip; cross-author attempt returns `403`
//!   with code `WORLD_KB_FORBIDDEN`.
//! - AC4 (#4): adopt inserts a new `KeyBlock` with `status='confirmed'`.
//! - AC5 (#5): reject writes a log entry; row marked `rejected`.
//!
//! Run with: cargo test -p nexus42 --test world_kb_promotion_cli

#![allow(clippy::unwrap_used)]

use nexus42::commands::creator::world::kb::{
    kb_adopt, kb_pending, kb_reject, WORLD_KB_FORBIDDEN_CODE,
};
use nexus42::db::Schema;
use nexus42::errors::CliError;
use nexus_kb::KbStore;
use nexus_local_db::kb_extract_job::{get_promotion, insert_pending};
use nexus_local_db::kb_store::SqliteKbStore;

const OWNER: &str = "ctr_owner";
const OTHER: &str = "ctr_other";
const WORLD: &str = "wld_promo";

/// Build a fresh migrated pool + seed a world owned by [`OWNER`].
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = Schema::init(&db_path).await.unwrap();
    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        OWNER,
        "Promo World",
        "promo-world",
        "private",
        "manual",
    )
    .await;
    (pool, dir)
}

/// Seed a pending candidate in [`WORLD`] owned by [`OWNER`].
async fn seed_pending(
    pool: &sqlx::SqlitePool,
    name: &str,
) -> nexus_local_db::kb_extract_job::KbExtractPromotion {
    let payload = serde_json::json!({
        "summary": format!("Candidate: {name}"),
        "attributes": {"novel_category": "character", "aliases": [name]},
        "tags": ["novel"],
    })
    .to_string();
    insert_pending(
        pool,
        OWNER,
        "ws",
        WORLD,
        Some("wrk_test"),
        Some(1),
        "character",
        name,
        &payload,
    )
    .await
    .unwrap()
}

// ── AC3: pending lists candidates; cross-author 403 ─────────────────────────

#[tokio::test]
async fn pending_lists_candidates_for_owner() {
    let (pool, _dir) = fresh_pool().await;
    seed_pending(&pool, "Lin Xia").await;
    seed_pending(&pool, "Marcus Vale").await;

    // Owner sees both.
    let pending = nexus_local_db::kb_extract_job::list_pending_for_world(&pool, WORLD, None)
        .await
        .unwrap();
    assert_eq!(pending.len(), 2);
}

#[tokio::test]
async fn pending_cross_author_returns_403() {
    let (pool, _dir) = fresh_pool().await;
    seed_pending(&pool, "Lin Xia").await;

    let err = kb_pending(&pool, OTHER, WORLD, None, false)
        .await
        .unwrap_err();
    match err {
        CliError::Api { status, message } => {
            assert_eq!(status, 403);
            assert!(
                message.contains(WORLD_KB_FORBIDDEN_CODE),
                "expected {WORLD_KB_FORBIDDEN_CODE} in: {message}"
            );
        }
        other => panic!("expected Api 403, got: {other:?}"),
    }
}

// ── AC4: adopt creates a confirmed KeyBlock ─────────────────────────────────

#[tokio::test]
async fn adopt_creates_confirmed_key_block() {
    let (pool, _dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Aria Stormblade").await;

    // Adopt.
    kb_adopt(&pool, OWNER, &candidate.job_id, None, false)
        .await
        .unwrap();

    // The promotion row is now confirmed.
    let row = get_promotion(&pool, &candidate.job_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.promotion_status, "confirmed");

    // A KeyBlock exists in the world with status=confirmed.
    let store = SqliteKbStore::new(pool.clone());
    let blocks = store.list_by_world(WORLD).await.unwrap();
    let adopted = blocks
        .iter()
        .find(|b| b.canonical_name == "Aria Stormblade")
        .unwrap_or_else(|| panic!("no KeyBlock for Aria Stormblade: {blocks:?}"));
    assert_eq!(adopted.status, "confirmed");
}

#[tokio::test]
async fn adopt_cross_author_returns_403() {
    let (pool, _dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Lin Xia").await;

    let err = kb_adopt(&pool, OTHER, &candidate.job_id, None, false)
        .await
        .unwrap_err();
    match err {
        CliError::Api { status, message } => {
            assert_eq!(status, 403);
            assert!(message.contains(WORLD_KB_FORBIDDEN_CODE));
        }
        other => panic!("expected Api 403, got: {other:?}"),
    }

    // Row is still pending (adopt was rejected).
    let row = get_promotion(&pool, &candidate.job_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.promotion_status, "pending");
}

// ── AC5: reject marks rejected + writes audit log ───────────────────────────

#[tokio::test]
async fn reject_marks_rejected_and_writes_log() {
    let (pool, dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Rejected Name").await;

    kb_reject(&pool, OWNER, &candidate.job_id, Some(dir.path()))
        .await
        .unwrap();

    // Row is rejected.
    let row = get_promotion(&pool, &candidate.job_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.promotion_status, "rejected");

    // Audit log written under Works/<work_id>/Logs/kb/rejected/.
    let log_path = dir
        .path()
        .join("Works")
        .join("wrk_test")
        .join("Logs")
        .join("kb")
        .join("rejected");
    let entries = std::fs::read_dir(&log_path).unwrap();
    let log_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(&candidate.job_id))
        .collect();
    assert!(
        !log_files.is_empty(),
        "expected a rejected log file for {}",
        candidate.job_id
    );
}

#[tokio::test]
async fn reject_cross_author_returns_403() {
    let (pool, _dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Lin Xia").await;

    let err = kb_reject(&pool, OTHER, &candidate.job_id, None)
        .await
        .unwrap_err();
    match err {
        CliError::Api { status, message } => {
            assert_eq!(status, 403);
            assert!(message.contains(WORLD_KB_FORBIDDEN_CODE));
        }
        other => panic!("expected Api 403, got: {other:?}"),
    }
}

// ── Adopt then pending-list excludes it ─────────────────────────────────────

#[tokio::test]
async fn adopted_candidate_disappears_from_pending() {
    let (pool, _dir) = fresh_pool().await;
    let a = seed_pending(&pool, "Alpha").await;
    seed_pending(&pool, "Beta").await;

    kb_adopt(&pool, OWNER, &a.job_id, None, false)
        .await
        .unwrap();

    let pending = nexus_local_db::kb_extract_job::list_pending_for_world(&pool, WORLD, None)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].canonical_name_guess.as_deref(), Some("Beta"));
}

// ── Double-adopt is rejected cleanly ────────────────────────────────────────

#[tokio::test]
async fn double_adopt_is_rejected() {
    let (pool, _dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Gamma").await;

    // First adopt succeeds.
    kb_adopt(&pool, OWNER, &candidate.job_id, None, false)
        .await
        .unwrap();

    // Second adopt fails (row no longer pending).
    let err = kb_adopt(&pool, OWNER, &candidate.job_id, None, false)
        .await
        .unwrap_err();
    match err {
        CliError::Other(msg) => {
            assert!(
                msg.contains("not pending"),
                "expected 'not pending' in: {msg}"
            );
        }
        other => panic!("expected Other error, got: {other:?}"),
    }
}
