//! V1.58 P0 T3 (R-V156P0-M003) — Concurrent OCC integration tests.
//!
//! Verifies the workspace session single-consumer guarantee:
//! - Two concurrent `consume_session` calls on the same session ID cannot
//!   both succeed — exactly one wins, the other gets `AlreadyCommitted`.
//! - The OCC conflict counter (`occ_conflict_total`) increments on the
//!   losing side (V1.58 P0 T6 — R-V156P0-M006).
//! - The full `commit_session` path (validate + consume transaction guard,
//!   V1.58 P0 T5 — R-V156P0-M005) also honors single-consumer semantics.
//!
//! Run with: `cargo test -p nexus-daemon-runtime --test workspace_occ_concurrent`
//!
//! Note: the V1.58 plan's verification command referenced
//! `cargo test -p nexus-orchestration --test workspace_session`, but the
//! workspace session manager lives in `nexus-daemon-runtime` (orchestration
//! does not depend on it). This is the correct home for the test.

#![allow(clippy::unwrap_used)]

use nexus_daemon_runtime::workspace::session::{
    occ_conflict_total, ChangeEntry, ChangeOp, SessionError, SessionId,
    WorkspaceSessionManager,
};
use nexus_local_db as db;
use std::sync::Arc;
use tokio::task::JoinHandle;

const OWNER: &str = "ctr_occ_v158";

async fn fresh_pool() -> (Arc<sqlx::SqlitePool>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    (Arc::new(pool), dir)
}

/// Open a session directly via the DB layer (no file scanning) so the test
/// is deterministic and does not depend on workspace layout.
async fn open_bare_session(mgr: &WorkspaceSessionManager, workspace_root: &str) -> SessionId {
    mgr.open_session(workspace_root, "", false)
        .await
        .expect("open_session")
}

#[tokio::test]
#[serial_test::serial]
async fn concurrent_consume_single_winner() {
    let (pool, _dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new(pool);

    // Create a workspace dir so canonicalization succeeds.
    let ws_dir = tempfile::tempdir().unwrap();
    let ws_root = ws_dir.path().to_string_lossy().to_string();

    let session_id = open_bare_session(&mgr, &ws_root).await;

    let before = occ_conflict_total();

    // Spawn two concurrent consumers on the SAME session.
    let mgr_a = clone_mgr(&mgr);
    let sid_a = session_id.clone();
    let h1: JoinHandle<Result<db::WorkspaceSessionRow, SessionError>> =
        tokio::spawn(async move { mgr_a.consume_session(&sid_a).await });

    let mgr_b = clone_mgr(&mgr);
    let sid_b = session_id.clone();
    let h2: JoinHandle<Result<db::WorkspaceSessionRow, SessionError>> =
        tokio::spawn(async move { mgr_b.consume_session(&sid_b).await });

    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();

    // Exactly one must succeed, the other must get AlreadyCommitted.
    let wins = matches!(r1, Ok(_)) as u32 + matches!(r2, Ok(_)) as u32;
    assert_eq!(wins, 1, "exactly one consumer must win; got r1={r1:?} r2={r2:?}");

    let conflicts = matches!(r1, Err(SessionError::AlreadyCommitted(_))) as u32
        + matches!(r2, Err(SessionError::AlreadyCommitted(_))) as u32;
    assert_eq!(
        conflicts, 1,
        "exactly one consumer must get AlreadyCommitted"
    );

    // V1.58 P0 T6: the OCC conflict counter must have incremented by exactly 1.
    let after = occ_conflict_total();
    assert_eq!(
        after,
        before + 1,
        "OCC conflict counter must increment by 1 on the losing consumer"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn concurrent_commit_session_single_winner() {
    // V1.58 P0 T5: the combined commit_session (validate + consume) must
    // also honor single-consumer semantics.
    let (pool, _dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new(pool);

    let ws_dir = tempfile::tempdir().unwrap();
    let ws_root = ws_dir.path().to_string_lossy().to_string();

    // Open a session on an empty workspace (no tracked files).
    let session_id = open_bare_session(&mgr, &ws_root).await;
    let changes: Vec<ChangeEntry> = Vec::new();

    let before = occ_conflict_total();

    let mgr_a = clone_mgr(&mgr);
    let sid_a = session_id.clone();
    let changes_a = changes.clone();
    let ws_a = ws_root.clone();
    let h1 = tokio::spawn(async move { mgr_a.commit_session(&sid_a, &changes_a, &ws_a).await });

    let mgr_b = clone_mgr(&mgr);
    let sid_b = session_id.clone();
    let changes_b = changes.clone();
    let ws_b = ws_root.clone();
    let h2 = tokio::spawn(async move { mgr_b.commit_session(&sid_b, &changes_b, &ws_b).await });

    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();

    let wins = matches!(r1, Ok(_)) as u32 + matches!(r2, Ok(_)) as u32;
    assert_eq!(
        wins, 1,
        "commit_session: exactly one writer must win; got r1={r1:?} r2={r2:?}"
    );

    // The loser must surface a stable OCC error (AlreadyCommitted or Expired —
    // both are Conflict-class). AlreadyCommitted is the expected race outcome.
    let occ_conflicts = matches!(r1, Err(SessionError::AlreadyCommitted(_))) as u32
        + matches!(r2, Err(SessionError::AlreadyCommitted(_))) as u32;
    assert_eq!(
        occ_conflicts, 1,
        "commit_session: loser must get AlreadyCommitted"
    );

    let after = occ_conflict_total();
    assert!(
        after >= before + 1,
        "OCC conflict counter must increment; before={before} after={after}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn consume_after_commit_is_stale() {
    // Sequential sanity: after a successful consume, a second consume on the
    // same session returns AlreadyCommitted and bumps the counter.
    let (pool, _dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new(pool);

    let ws_dir = tempfile::tempdir().unwrap();
    let ws_root = ws_dir.path().to_string_lossy().to_string();
    let session_id = open_bare_session(&mgr, &ws_root).await;

    let before = occ_conflict_total();

    let first = mgr.consume_session(&session_id).await;
    let second = mgr.consume_session(&session_id).await;

    assert!(first.is_ok(), "first consume must succeed: {first:?}");
    assert!(
        matches!(second, Err(SessionError::AlreadyCommitted(_))),
        "second consume must be stale; got {second:?}"
    );

    assert_eq!(
        occ_conflict_total(),
        before + 1,
        "counter must bump on stale consume"
    );
}

/// Clone the `Arc<SqlitePool>` out of a manager so each spawned task gets
/// its own manager handle pointing at the same DB.
fn clone_mgr(mgr: &WorkspaceSessionManager) -> WorkspaceSessionManager {
    WorkspaceSessionManager::new(mgr.pool())
}
