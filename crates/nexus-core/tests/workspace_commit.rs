//! Durable workspace-commit authority proofs (v1.190 P3-T2).
//!
//! This target owns the behavior the daemon's L2 proof protected before the
//! commit authority moved into `nexus-core`. It is the acceptance anchor for
//! the move: the durable file application must finalize exactly once after an
//! interruption, a changed digest must conflict, and a recovery conflict must
//! block subsequent writes.
//!
//! Every test is `#[serial]`: the crash/rendezvous seams are process-global
//! (`execution::test_hooks`), so parallel execution would let one test's armed
//! crash point fire inside another's commit.

#![cfg(feature = "execution")]
#![allow(clippy::unwrap_used)]

use base64::Engine;
use nexus_core::execution::commit_fs::hash_bytes;
use nexus_core::execution::session::{
    ChangeEntry, ChangeOp, SessionError, SessionId, WorkspaceSessionManager,
};
use nexus_core::execution::test_hooks::{
    OwnerGate, set_after_delete_capture_hook, set_crash_point, set_owner_gate,
};
use nexus_orchestration::capability::WorkspaceExecutor;
use serial_test::serial;
use std::sync::Arc;

/// An engine-admitted pool over a fresh temp DB.
///
/// The commit protocol's statements call the `nexus_writer_protocol` SQL
/// function, which only `init_engine_pool` installs — a bare
/// `SqlitePool::connect` would abort with `no such function`. Two connections
/// is the documented budget the original proof used.
async fn fresh_pool() -> (Arc<sqlx::SqlitePool>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("state.db");
    let pool = nexus_local_db::writer_protocol::init_engine_pool(
        &db_path,
        nexus_local_db::BOOTSTRAP_CREATOR_ID,
        nexus_local_db::GuardedPoolOptions {
            max_connections: 2,
            ..Default::default()
        },
    )
    .await
    .expect("engine-admitted pool")
    .clone_pool();
    (Arc::new(pool), dir)
}

fn recoverable_mgr(
    pool: Arc<sqlx::SqlitePool>,
    db_dir: &tempfile::TempDir,
) -> Arc<WorkspaceSessionManager> {
    Arc::new(
        WorkspaceSessionManager::new_recoverable(pool, db_dir.path().join("state.db"))
            .expect("recoverable"),
    )
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn create_change(path: &str, content: &[u8]) -> ChangeEntry {
    ChangeEntry {
        path: path.to_string(),
        op: ChangeOp::Create,
        expected_hash: None,
        content_base64: Some(b64(content)),
    }
}

fn modify_change(path: &str, expected: &str, content: &[u8]) -> ChangeEntry {
    ChangeEntry {
        path: path.to_string(),
        op: ChangeOp::Modify,
        expected_hash: Some(expected.to_string()),
        content_base64: Some(b64(content)),
    }
}

async fn intent_state(pool: &sqlx::SqlitePool, revision: &str) -> String {
    sqlx::query_scalar("SELECT state FROM workspace_commit_intents WHERE revision = ?")
        .bind(revision)
        .fetch_one(pool)
        .await
        .expect("intent row")
}

/// ACCEPTANCE ANCHOR (ported from `workspace_commit_l2_proof.rs`).
///
/// A crash after the file apply leaves the applied bytes in place; startup
/// recovery must complete the intent — the durable file application finalizes
/// exactly once rather than being rolled back or re-applied.
#[tokio::test]
#[serial]
async fn simulated_crash_after_file_apply_then_recovery_completes() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");
    set_crash_point(Some("after_file_apply"));
    let err = mgr
        .commit_session_durable(&session, &[create_change("applied.txt", b"payload")], &root)
        .await
        .unwrap_err();
    set_crash_point(None);
    assert!(matches!(err, SessionError::Internal(_)));
    assert!(ws.path().join("applied.txt").exists());
    mgr.startup_recovery().await.expect("recovery");
    assert_eq!(
        std::fs::read(ws.path().join("applied.txt")).unwrap(),
        b"payload"
    );
}

/// A crash before the apply leaves no file, and recovery rolls the intent back
/// rather than inventing the missing bytes.
#[tokio::test]
#[serial]
async fn simulated_crash_after_entries_then_startup_recovery() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");
    set_crash_point(Some("after_entries_persisted"));
    let err = mgr
        .commit_session_durable(&session, &[create_change("crash.txt", b"payload")], &root)
        .await
        .unwrap_err();
    set_crash_point(None);
    assert!(matches!(err, SessionError::Internal(_)));
    assert!(!ws.path().join("crash.txt").exists());
    mgr.startup_recovery().await.expect("recovery");
    assert!(!ws.path().join("crash.txt").exists());
    let state: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(state, "rolled_back");
}

/// A different digest on an already-committed session must conflict and must
/// NOT disturb the durable committed intent.
#[tokio::test]
#[serial]
async fn different_digest_rejected_after_success() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    mgr.startup_recovery().await.expect("startup");
    let session = mgr.open_session(&root, "", true).await.expect("open");
    let first = mgr
        .commit_session_durable(&session, &[create_change("a.txt", b"one")], &root)
        .await
        .expect("first");
    let err = mgr
        .commit_session_durable(&session, &[create_change("b.txt", b"two")], &root)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::AlreadyCommitted(_)));
    assert_eq!(
        intent_state(mgr.pool().as_ref(), &first.revision).await,
        "committed"
    );
}

/// A recovery conflict on the intent log must block a subsequent write rather
/// than silently committing past the unresolved intent.
#[tokio::test]
#[serial]
async fn recovery_conflict_blocks_subsequent_write() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");
    // A corrupt (unparsable) intent row is an unresolvable recovery state.
    sqlx::query(
        "INSERT INTO workspace_commit_intents (session_id, workspace_root, revision, request_digest, state, entries_json) \
         VALUES (?, ?, 'rev_bad', 'dig', 'applying', 'not-json')",
    )
    .bind(session.to_string())
    .bind(&root)
    .execute(mgr.pool().as_ref())
    .await
    .unwrap();
    let err = mgr.startup_recovery().await.unwrap_err();
    assert!(matches!(err, SessionError::RecoveryConflict(_)));
    // The next write is fenced by the unresolved intent, not committed.
    let err = mgr
        .commit_session_durable(&session, &[create_change("blocked.txt", b"x")], &root)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::RecoveryConflict(_)));
    assert!(!ws.path().join("blocked.txt").exists());
}

/// The retained commit owner finishes the commit even when the awaiting caller
/// is dropped mid-flight (client disconnect / shutdown).
#[tokio::test]
#[serial]
async fn retained_owner_survives_caller_drop() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    mgr.startup_recovery().await.expect("startup");
    let session = mgr.open_session(&root, "", true).await.expect("open");

    // Deterministic rendezvous at the owner's admission boundary — no sleeps.
    let gate = Arc::new(OwnerGate::for_session(session.to_string()));
    set_owner_gate(Some(Arc::clone(&gate)));

    let mgr2 = Arc::clone(&mgr);
    let session_id = session.clone();
    let changes = vec![create_change("owner.txt", b"owned")];
    let owner_root = root.clone();
    let caller = tokio::spawn(async move {
        WorkspaceSessionManager::commit_session_durable_owned(mgr2, session_id, changes, owner_root)
            .await
    });

    // The owner task has been admitted (claim held) and is parked.
    gate.admitted.notified().await;
    // Cancel the awaiting caller, then release the owner.
    caller.abort();
    gate.proceed.notify_one();
    // The retained owner settles on its own: no polling window.
    gate.settled.notified().await;
    set_owner_gate(None);

    assert_eq!(std::fs::read(ws.path().join("owner.txt")).unwrap(), b"owned");
    let state: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(state, "committed");
}

/// The OCC pre-image check rejects a modify whose expected hash no longer
/// matches the on-disk content.
#[tokio::test]
#[serial]
async fn occ_hash_conflict_on_stale_preimage() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    mgr.startup_recovery().await.expect("startup");
    let session = mgr.open_session(&root, "", true).await.expect("open");
    mgr.commit_session_durable(&session, &[create_change("occ.txt", b"v1")], &root)
        .await
        .expect("seed");

    let session2 = mgr.open_session(&root, "", true).await.expect("open2");
    let changed = "deadbeef".repeat(8);
    let err = mgr
        .commit_session_durable(&session2, &[modify_change("occ.txt", &changed, b"v2")], &root)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::HashConflict { .. }));
    assert_eq!(std::fs::read(ws.path().join("occ.txt")).unwrap(), b"v1");
}

/// A target re-created by an external writer between the delete's capture and
/// its finalize must never finalize the delete; the writer's bytes survive.
#[tokio::test]
#[serial]
async fn delete_detects_target_recreated_after_capture() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    std::fs::write(ws.path().join("d.txt"), b"original").unwrap();
    mgr.startup_recovery().await.expect("startup");
    let session = mgr.open_session(&root, "", true).await.expect("open");

    // Deterministic injection EXACTLY in the capture/finalize window: the hook
    // fires after the delete captured the original and before it finalizes.
    let target = ws.path().join("d.txt");
    set_after_delete_capture_hook(Some(Arc::new(move || {
        std::fs::write(&target, b"recreated-by-external-writer").expect("external write");
    })));

    let err = mgr
        .commit_session_durable(
            &session,
            &[ChangeEntry {
                path: "d.txt".to_string(),
                op: ChangeOp::Delete,
                expected_hash: Some(hash_bytes(b"original")),
                content_base64: None,
            }],
            &root,
        )
        .await
        .unwrap_err();
    set_after_delete_capture_hook(None);

    assert!(
        matches!(err, SessionError::Internal(_) | SessionError::Io(_)),
        "a re-created target after capture must never finalize a delete, got {err:?}"
    );
    // The external writer's bytes survive untouched.
    assert_eq!(
        std::fs::read(ws.path().join("d.txt")).unwrap(),
        b"recreated-by-external-writer"
    );
}

/// The orchestration `WorkspaceExecutor` adapter commits through the SAME
/// manager authority the direct commit path uses.
#[tokio::test]
#[serial]
async fn executor_matches_manager_authority() {
    use nexus_core::execution::executor::WorkspaceCommitExecutor;
    use nexus_contracts::local::orchestration::{
        WorkspaceChangeEntry, WorkspaceChangeOp, WorkspaceCommitInput, WorkspaceOpenInput,
    };

    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    std::fs::create_dir_all(ws.path().join("pkg")).unwrap();
    mgr.startup_recovery().await.expect("startup");
    let exec = WorkspaceCommitExecutor::new(Arc::clone(&mgr), root.clone());
    let opened = exec
        .open(WorkspaceOpenInput {
            path: "pkg".to_string(),
        })
        .await
        .expect("open");
    let committed = exec
        .commit(WorkspaceCommitInput {
            session_id: opened.session_id,
            changes: vec![WorkspaceChangeEntry {
                path: "cap.txt".to_string(),
                op: WorkspaceChangeOp::Create,
                expected_hash: None,
                content_base64: Some(b64(b"via-cap")),
            }],
        })
        .await
        .expect("commit");
    assert!(committed.revision.starts_with("rev_"));
    assert_eq!(
        std::fs::read(ws.path().join("pkg/cap.txt")).unwrap(),
        b"via-cap"
    );
    assert_eq!(
        intent_state(mgr.pool().as_ref(), &committed.revision).await,
        "committed"
    );
}

/// An empty manifest is a manifest validation refusal, not a silent no-op
/// commit.
#[tokio::test]
#[serial]
async fn empty_manifest_rejected() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");
    let err = mgr
        .commit_session_durable(&session, &[], &root)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::ManifestInvalid(_)));
}

/// A session opened under a foreign workspace root cannot commit through this
/// authority.
#[tokio::test]
#[serial]
async fn foreign_workspace_root_is_refused() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws_a = tempfile::tempdir().unwrap();
    let ws_b = tempfile::tempdir().unwrap();
    let root_a = ws_a.path().to_string_lossy().to_string();
    let root_b = ws_b.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root_a, "", true).await.expect("open");
    let err = mgr
        .commit_session_durable(&session, &[create_change("x.txt", b"x")], &root_b)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::ActiveWorkspaceMismatch { .. }));
    assert!(!ws_b.path().join("x.txt").exists());
}

/// `SessionId` round-trips through its display form (the wire cursor).
#[test]
fn session_id_display_round_trips() {
    let id = SessionId("sess-123".to_string());
    assert_eq!(id.to_string(), "sess-123");
}
