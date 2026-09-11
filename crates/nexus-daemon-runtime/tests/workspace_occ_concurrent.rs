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
use base64::Engine;

use nexus_daemon_runtime::workspace::session::{
    compute_content_hashes, occ_conflict_total, ChangeEntry, ChangeOp, SessionError, SessionId,
    WorkspaceSessionManager,
};
use nexus_local_db as db;
use std::sync::Arc;
use tokio::task::JoinHandle;

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
    let wins = u32::from(r1.is_ok()) + u32::from(r2.is_ok());
    assert_eq!(
        wins, 1,
        "exactly one consumer must win; got r1={r1:?} r2={r2:?}"
    );

    let conflicts = u32::from(matches!(r1, Err(SessionError::AlreadyCommitted(_))))
        + u32::from(matches!(r2, Err(SessionError::AlreadyCommitted(_))));
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

    let wins = u32::from(r1.is_ok()) + u32::from(r2.is_ok());
    assert_eq!(
        wins, 1,
        "commit_session: exactly one writer must win; got r1={r1:?} r2={r2:?}"
    );

    // The loser must surface a stable OCC error (AlreadyCommitted or Expired —
    // both are Conflict-class). AlreadyCommitted is the expected race outcome.
    let occ_conflicts = u32::from(matches!(r1, Err(SessionError::AlreadyCommitted(_))))
        + u32::from(matches!(r2, Err(SessionError::AlreadyCommitted(_))));
    assert_eq!(
        occ_conflicts, 1,
        "commit_session: loser must get AlreadyCommitted"
    );

    let after = occ_conflict_total();
    assert!(
        after > before,
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

// ── V1.58 P0 T2 (QC2 H-2 regression): symlink rejection in Modify path ────

#[tokio::test]
#[serial_test::serial]
#[cfg(unix)]
async fn validate_changes_manifest_rejects_symlink_in_modify_path() {
    // V1.58 P0 T2 (QC2 H-2 regression): `validate_changes_manifest` must
    // reject a symlink introduced at a Modify change path BEFORE opening the
    // file for hashing. Without the `symlink_metadata` defense, a symlink
    // pointing outside the workspace root would be followed by `File::open`
    // inside `compute_single_file_hash`, allowing the hash of outside
    // content to be used for OCC comparison (TOCTOU between boundary check
    // and hash read). Mirrors the `compute_content_hashes_inner` defense.
    use std::os::unix::fs::symlink;

    let (pool, _dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new(pool);

    // Workspace with one tracked file.
    let ws_dir = tempfile::tempdir().unwrap();
    std::fs::write(ws_dir.path().join("real.txt"), b"real").unwrap();

    // Open a session on the workspace root (scans real.txt → snapshot).
    let ws_root = ws_dir.path().to_string_lossy().to_string();
    let session_id = mgr
        .open_session(&ws_root, "", true)
        .await
        .expect("open_session");

    // Compute the stored hash so the Modify change has a matching content_hash.
    let stored_hashes = compute_content_hashes(ws_dir.path())
        .await
        .expect("compute");
    let stored_hash = stored_hashes
        .hashes
        .get("real.txt")
        .cloned()
        .expect("real.txt tracked");

    // Swap real.txt for a symlink pointing OUTSIDE the workspace root.
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), b"secret-outside").unwrap();
    std::fs::remove_file(ws_dir.path().join("real.txt")).unwrap();
    symlink(
        outside.path().join("secret.txt"),
        ws_dir.path().join("real.txt"),
    )
    .expect("symlink");

    // The commit-time validation must reject the symlink with `PathEscape` —
    // it must NOT compute the hash of the outside file (which would be a
    // successful symlink escape).
    let changes = vec![ChangeEntry {
        path: "real.txt".to_string(),
        op: ChangeOp::Modify,
        expected_hash: Some(stored_hash),
        content_base64: Some(
            base64::engine::general_purpose::STANDARD.encode(b"placeholder"),
        ),
    }];
    let err = mgr
        .validate_contract_manifest(&session_id, &changes)
        .await
        .expect_err("symlink must be rejected");
    assert!(
        matches!(err, SessionError::PathEscape { .. }),
        "expected PathEscape for symlink in Modify path, got {err:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn recoverable_create_modify_delete_and_idempotent_retry() {
    use base64::Engine;
    let (pool, db_dir) = fresh_pool().await;
    let db_path = db_dir.path().join("state.db");
    let mgr = WorkspaceSessionManager::new_recoverable(pool, db_path).expect("recoverable mgr");

    let ws_dir = tempfile::tempdir().unwrap();
    let ws_root = ws_dir.path().to_string_lossy().to_string();

    let session_id = mgr.open_session(&ws_root, "", true).await.expect("open");
    let create_bytes = b"hello durable";
    let create_b64 = base64::engine::general_purpose::STANDARD.encode(create_bytes);
    let create_changes = vec![ChangeEntry {
        path: "note.txt".to_string(),
        op: ChangeOp::Create,
        expected_hash: None,
        content_base64: Some(create_b64.clone()),
    }];
    let first = mgr
        .commit_session_durable(&session_id, &create_changes, &ws_root)
        .await
        .expect("create commit");
    assert!(first.committed);
    assert!(first.revision.starts_with("rev_"));
    assert_eq!(std::fs::read(ws_dir.path().join("note.txt")).unwrap(), create_bytes);

    let session2 = mgr.open_session(&ws_root, "", true).await.expect("reopen");
    let hashes = compute_content_hashes(ws_dir.path()).await.expect("hash");
    let pre = hashes.hashes.get("note.txt").cloned().expect("tracked");
    let modify_bytes = b"hello modified";
    let modify_b64 = base64::engine::general_purpose::STANDARD.encode(modify_bytes);
    let modify_changes = vec![ChangeEntry {
        path: "note.txt".to_string(),
        op: ChangeOp::Modify,
        expected_hash: Some(pre),
        content_base64: Some(modify_b64),
    }];
    let second = mgr
        .commit_session_durable(&session2, &modify_changes, &ws_root)
        .await
        .expect("modify commit");
    assert_ne!(second.revision, first.revision);

    let session3 = mgr.open_session(&ws_root, "", true).await.expect("reopen3");
    let hashes2 = compute_content_hashes(ws_dir.path()).await.expect("hash2");
    let pre2 = hashes2.hashes.get("note.txt").cloned().expect("tracked2");
    let delete_changes = vec![ChangeEntry {
        path: "note.txt".to_string(),
        op: ChangeOp::Delete,
        expected_hash: Some(pre2),
        content_base64: None,
    }];
    mgr.commit_session_durable(&session3, &delete_changes, &ws_root)
        .await
        .expect("delete commit");
    assert!(!ws_dir.path().join("note.txt").exists());

    let session4 = mgr.open_session(&ws_root, "", true).await.expect("reopen4");
    let recreate = vec![ChangeEntry {
        path: "note.txt".to_string(),
        op: ChangeOp::Create,
        expected_hash: None,
        content_base64: Some(create_b64),
    }];
    let a = mgr
        .commit_session_durable(&session4, &recreate, &ws_root)
        .await
        .expect("recreate");
    let retry = mgr
        .commit_session_durable(&session4, &recreate, &ws_root)
        .await
        .expect("idempotent retry after consumed session");
    assert_eq!(retry.revision, a.revision);
}
