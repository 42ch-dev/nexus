//! v1.188 P3 L2 proof tests — scope, idempotency, conflicts, recovery, cancellation.

#![allow(clippy::unwrap_used)]

use base64::Engine;
use nexus_contracts::local::orchestration::{
    WorkspaceChangeEntry, WorkspaceChangeOp, WorkspaceCommitInput, WorkspaceOpenInput,
};
use nexus_daemon_runtime::workspace::executor::DaemonWorkspaceExecutor;
use nexus_daemon_runtime::workspace::session::{
    ChangeEntry, ChangeOp, SessionError, SessionId, WorkspaceSessionManager,
};
use nexus_daemon_runtime::workspace::commit_fs::{
    hash_bytes, set_test_after_delete_capture_hook,
};
use nexus_daemon_runtime::workspace::session_commit::{
    set_test_crash_point, set_test_owner_gate, OwnerGate,
};
use nexus_local_db as db;
use nexus_orchestration::capability::WorkspaceExecutor;
use serial_test::serial;
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;

async fn fresh_pool() -> (Arc<sqlx::SqlitePool>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("state.db");
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
        .await
        .expect("connect");
    db::run_migrations(&pool).await.expect("migrate");
    (Arc::new(pool), dir)
}

fn recoverable_mgr(pool: Arc<sqlx::SqlitePool>, db_dir: &tempfile::TempDir) -> Arc<WorkspaceSessionManager> {
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

async fn intent_row_count(pool: &sqlx::SqlitePool, session_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM workspace_commit_intents WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(pool)
        .await
        .expect("count")
}

async fn intent_state(pool: &sqlx::SqlitePool, revision: &str) -> String {
    sqlx::query_scalar("SELECT state FROM workspace_commit_intents WHERE revision = ?")
        .bind(revision)
        .fetch_one(pool)
        .await
        .expect("intent row")
}

#[tokio::test]
#[serial]
async fn subdirectory_scope_create_bytes() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    std::fs::create_dir_all(ws.path().join("pkg")).unwrap();
    mgr.startup_recovery().await.expect("startup");
    let session = mgr.open_session(&root, "pkg", true).await.expect("open");
    let outcome = mgr
        .commit_session_durable(&session, &[create_change("nested.txt", b"scoped")], &root)
        .await
        .expect("commit");
    assert!(outcome.revision.starts_with("rev_"));
    assert_eq!(
        std::fs::read(ws.path().join("pkg/nested.txt")).unwrap(),
        b"scoped"
    );
    assert_eq!(intent_state(mgr.pool().as_ref(), &outcome.revision).await, "committed");
}

#[tokio::test]
#[serial]
async fn corrupt_snapshot_rejected() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");
    sqlx::query("UPDATE workspace_sessions SET file_hashes_json = 'not-json' WHERE session_id = ?")
        .bind(session.to_string())
        .execute(mgr.pool().as_ref())
        .await
        .unwrap();
    let err = mgr
        .validate_contract_manifest(&session, &[create_change("x.txt", b"x")])
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::CorruptSnapshot(_)));
}

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
    assert_eq!(intent_state(mgr.pool().as_ref(), &first.revision).await, "committed");
}

#[tokio::test]
#[serial]
async fn simulated_crash_after_entries_then_startup_recovery() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");
    set_test_crash_point(Some("after_entries_persisted"));
    let err = mgr
        .commit_session_durable(&session, &[create_change("crash.txt", b"payload")], &root)
        .await
        .unwrap_err();
    set_test_crash_point(None);
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

#[tokio::test]
#[serial]
async fn simulated_crash_after_file_apply_then_recovery_completes() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");
    set_test_crash_point(Some("after_file_apply"));
    let err = mgr
        .commit_session_durable(&session, &[create_change("applied.txt", b"payload")], &root)
        .await
        .unwrap_err();
    set_test_crash_point(None);
    assert!(matches!(err, SessionError::Internal(_)));
    assert!(ws.path().join("applied.txt").exists());
    mgr.startup_recovery().await.expect("recovery");
    assert_eq!(
        std::fs::read(ws.path().join("applied.txt")).unwrap(),
        b"payload"
    );
}

#[tokio::test]
#[serial]
async fn corrupt_intent_blocks_startup() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");
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
    let state: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE revision = 'rev_bad'",
    )
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(state, "recovery_conflict");
}

#[tokio::test]
#[serial]
async fn executor_matches_manager_authority() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    std::fs::create_dir_all(ws.path().join("pkg")).unwrap();
    mgr.startup_recovery().await.expect("startup");
    let exec = DaemonWorkspaceExecutor::new(Arc::clone(&mgr), root.clone());
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
    let gate = Arc::new(OwnerGate::default());
    set_test_owner_gate(Some(Arc::clone(&gate)));

    let mgr2 = Arc::clone(&mgr);
    let session_id = session.clone();
    let changes = vec![create_change("owner.txt", b"owned")];
    let caller = tokio::spawn(async move {
        WorkspaceSessionManager::commit_session_durable_owned(mgr2, session_id, changes).await
    });

    // The owner task has been admitted (claim held) and is parked.
    gate.admitted.notified().await;
    // Cancel the awaiting caller, then release the owner.
    caller.abort();
    gate.proceed.notify_one();
    // The retained owner settles on its own: no polling window.
    gate.settled.notified().await;
    set_test_owner_gate(None);

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

#[tokio::test]
#[serial]
async fn recovery_conflict_blocks_subsequent_write() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    sqlx::query(
        "INSERT INTO workspace_commit_intents (session_id, workspace_root, revision, request_digest, state, entries_json, error_category) \
         VALUES ('ws_x', ?, 'rev_conflict', 'dig', 'recovery_conflict', '[]', 'manual')",
    )
    .bind(&root)
    .execute(mgr.pool().as_ref())
    .await
    .unwrap();
    let err = mgr.startup_recovery().await.unwrap_err();
    assert!(matches!(err, SessionError::RecoveryConflict(_)));
    let session = mgr.open_session(&root, "", true).await.expect("open");
    let err = mgr
        .commit_session_durable(&session, &[create_change("blocked.txt", b"x")], &root)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        SessionError::RecoveryConflict(_) | SessionError::ManifestInvalid(_)
    ));
}

#[tokio::test]
#[serial]
async fn crash_after_intent_then_startup_recovery_rolls_back() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");

    set_test_crash_point(Some("after_intent"));
    let err = mgr
        .commit_session_durable(&session, &[create_change("intent.txt", b"never")], &root)
        .await
        .unwrap_err();
    set_test_crash_point(None);
    assert!(matches!(err, SessionError::Internal(_)));

    // The admitted intent is durable and still `applying` — never dropped.
    let state: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(state, "applying");
    assert!(!ws.path().join("intent.txt").exists());

    mgr.startup_recovery().await.expect("recovery");
    let settled: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(settled, "rolled_back");
    assert!(!ws.path().join("intent.txt").exists());
}

#[tokio::test]
#[serial]
async fn crash_before_finalize_then_recovery_settles_committed() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");

    set_test_crash_point(Some("before_finalize"));
    let err = mgr
        .commit_session_durable(&session, &[create_change("kept.txt", b"durable")], &root)
        .await
        .unwrap_err();
    set_test_crash_point(None);
    assert!(matches!(err, SessionError::Internal(_)));
    // The postimage is already on disk; only the settle is missing.
    assert_eq!(
        std::fs::read(ws.path().join("kept.txt")).unwrap(),
        b"durable"
    );

    mgr.startup_recovery().await.expect("recovery");
    let state: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(state, "committed");
    assert_eq!(
        std::fs::read(ws.path().join("kept.txt")).unwrap(),
        b"durable"
    );

    // Recovery-finalized sessions are consumed: no second commit.
    let err = mgr
        .commit_session_durable(&session, &[create_change("other.txt", b"x")], &root)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::AlreadyCommitted(_)));
}

#[tokio::test]
#[serial]
async fn crash_during_rollback_then_recovery_completes_rollback() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");

    // First change applies, then the commit dies: recovery must roll back.
    set_test_crash_point(Some("after_file_apply"));
    let err = mgr
        .commit_session_durable(
            &session,
            &[
                create_change("f1.txt", b"one"),
                create_change("f2.txt", b"two"),
            ],
            &root,
        )
        .await
        .unwrap_err();
    set_test_crash_point(None);
    assert!(matches!(err, SessionError::Internal(_)));
    assert!(ws.path().join("f1.txt").exists());

    // Recovery reaches the rollback and is interrupted mid-way.
    set_test_crash_point(Some("during_rollback"));
    let err = mgr.startup_recovery().await.unwrap_err();
    set_test_crash_point(None);
    assert!(matches!(err, SessionError::Internal(_)));
    let state: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(state, "rolling_back");

    // A second recovery pass settles the rollback.
    mgr.startup_recovery().await.expect("recovery");
    let state: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(state, "rolled_back");
    assert!(!ws.path().join("f1.txt").exists());
    assert!(!ws.path().join("f2.txt").exists());
}

#[tokio::test]
#[serial]
async fn multi_file_commit_survives_new_manager_restart() {
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("state.db");
    let pool = Arc::new(
        SqlitePoolOptions::new()
            .max_connections(2)
            .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
            .await
            .expect("connect"),
    );
    db::run_migrations(&pool).await.expect("migrate");
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();

    let revision = {
        let mgr = Arc::new(
            WorkspaceSessionManager::new_recoverable(Arc::clone(&pool), db_path.clone())
                .expect("recoverable"),
        );
        mgr.startup_recovery().await.expect("startup");
        let session = mgr.open_session(&root, "", true).await.expect("open");
        let outcome = mgr
            .commit_session_durable(
                &session,
                &[
                    create_change("a.txt", b"alpha"),
                    create_change("b.txt", b"beta"),
                    create_change("c.txt", b"gamma"),
                ],
                &root,
            )
            .await
            .expect("commit");
        outcome.revision
        // `mgr` drops here, releasing the workspace authority lease.
    };

    // Brand-new manager over the same durable state (restart).
    let restarted = Arc::new(
        WorkspaceSessionManager::new_recoverable(Arc::clone(&pool), db_path.clone())
            .expect("restart recoverable"),
    );
    restarted.startup_recovery().await.expect("restart recovery");
    assert_eq!(std::fs::read(ws.path().join("a.txt")).unwrap(), b"alpha");
    assert_eq!(std::fs::read(ws.path().join("b.txt")).unwrap(), b"beta");
    assert_eq!(std::fs::read(ws.path().join("c.txt")).unwrap(), b"gamma");
    assert_eq!(
        intent_state(restarted.pool().as_ref(), &revision).await,
        "committed"
    );
}

#[tokio::test]
#[serial]
async fn expired_session_rejected() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");
    sqlx::query(
        "UPDATE workspace_sessions SET expires_at = '1970-01-01T00:00:00Z' WHERE session_id = ?",
    )
    .bind(session.to_string())
    .execute(mgr.pool().as_ref())
    .await
    .unwrap();

    let err = mgr
        .commit_session_durable(&session, &[create_change("late.txt", b"x")], &root)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::Expired(_)));
    assert!(!ws.path().join("late.txt").exists());
}

#[tokio::test]
#[serial]
async fn overlapping_and_duplicate_paths_rejected() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");

    let nested = mgr
        .commit_session_durable(
            &session,
            &[
                create_change("dir", b"file"),
                create_change("dir/child.txt", b"child"),
            ],
            &root,
        )
        .await
        .unwrap_err();
    assert!(matches!(nested, SessionError::ManifestInvalid(_)));

    let duplicate = mgr
        .commit_session_durable(
            &session,
            &[
                create_change("same.txt", b"one"),
                create_change("same.txt", b"two"),
            ],
            &root,
        )
        .await
        .unwrap_err();
    assert!(matches!(duplicate, SessionError::ManifestInvalid(_)));
    assert_eq!(
        intent_row_count(mgr.pool().as_ref(), &session.to_string()).await,
        0
    );
}

#[tokio::test]
#[serial]
async fn traversal_path_rejected() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");

    for path in ["../escape.txt", "nested/../../escape.txt", "/absolute.txt"] {
        let err = mgr
            .commit_session_durable(&session, &[create_change(path, b"x")], &root)
            .await
            .unwrap_err();
        assert!(
            matches!(err, SessionError::ManifestInvalid(_)),
            "{path} must be rejected, got {err:?}"
        );
    }
    assert!(!ws.path().parent().unwrap().join("escape.txt").exists());
}

#[tokio::test]
#[serial]
#[cfg(unix)]
async fn ancestor_symlink_rejected() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    std::os::unix::fs::symlink(outside.path(), ws.path().join("alink")).unwrap();
    let session = mgr.open_session(&root, "", true).await.expect("open");

    let err = mgr
        .commit_session_durable(&session, &[create_change("alink/evil.txt", b"x")], &root)
        .await
        .unwrap_err();
    assert!(
        matches!(err, SessionError::Io(_) | SessionError::PathEscape { .. }),
        "ancestor symlink must fail closed, got {err:?}"
    );
    assert!(!outside.path().join("evil.txt").exists());
}

#[tokio::test]
#[serial]
async fn missing_parent_rejected_without_orphan_intent() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");

    let err = mgr
        .commit_session_durable(&session, &[create_change("nope/deep.txt", b"x")], &root)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::ManifestInvalid(_)));

    // A staging failure settles the intent BEFORE releasing the claim: no
    // orphaned `rolling_back` row, and the session is immediately reusable.
    assert_eq!(
        intent_row_count(mgr.pool().as_ref(), &session.to_string()).await,
        0
    );
    let recovered = mgr
        .commit_session_durable(&session, &[create_change("ok.txt", b"fine")], &root)
        .await
        .expect("session reusable after staging failure");
    assert_eq!(
        intent_state(mgr.pool().as_ref(), &recovered.revision).await,
        "committed"
    );
}

#[tokio::test]
#[serial]
async fn external_writer_at_mutation_boundary_blocks_and_preserves_evidence() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    std::fs::write(ws.path().join("a.txt"), b"one").unwrap();
    mgr.startup_recovery().await.expect("startup");
    let session = mgr.open_session(&root, "", true).await.expect("open");
    let pre = hash_bytes(b"one");

    // Park the owner past validation, then let an external writer swap bytes.
    let gate = Arc::new(OwnerGate::default());
    set_test_owner_gate(Some(Arc::clone(&gate)));
    let mgr2 = Arc::clone(&mgr);
    let session_id = session.clone();
    let changes = vec![modify_change("a.txt", &pre, b"two")];
    let owner_root = root.clone();
    let handle = tokio::spawn(async move {
        mgr2.commit_session_durable(&session_id, &changes, &owner_root).await
    });
    gate.admitted.notified().await;
    std::fs::write(ws.path().join("a.txt"), b"tampered").unwrap();
    gate.proceed.notify_one();
    let err = handle.await.unwrap().unwrap_err();
    set_test_owner_gate(None);

    assert!(
        matches!(err, SessionError::Internal(ref m) if m.contains("recovery evidence preserved")),
        "third-state apply failure must surface the preserved-evidence path, got {err:?}"
    );
    // Evidence over truth: the external bytes are never overwritten, and the
    // conflict is durable.
    assert_eq!(std::fs::read(ws.path().join("a.txt")).unwrap(), b"tampered");
    let state: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(state, "recovery_conflict");

    // Subsequent writes to the workspace are blocked until recovered.
    let next = mgr.open_session(&root, "", true).await.expect("open2");
    let err = mgr
        .commit_session_durable(&next, &[create_change("blocked.txt", b"x")], &root)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::RecoveryConflict(_)));
    assert!(!ws.path().join("blocked.txt").exists());
}

#[tokio::test]
#[serial]
async fn oversized_entries_json_blocks_startup_before_parsing() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");

    // VALID json of the right shape, above the raw-byte cap: the cap (not
    // entry validation) is what must reject it.
    let entries: Vec<db::IntentEntryJson> = (0..128)
        .map(|_| db::IntentEntryJson {
            path: "a".repeat(4096),
            op: "delete".to_string(),
            pre_hash: Some("0".repeat(64)),
            post_hash: None,
            stage_basename: ".nexus-stage-x".to_string(),
            backup_basename: None,
            mode: None,
        })
        .collect();
    let oversized = serde_json::to_string(&entries).unwrap();
    assert!(serde_json::from_str::<Vec<db::IntentEntryJson>>(&oversized).is_ok());
    assert!(oversized.len() > nexus_local_db::MAX_ENTRIES_JSON_BYTES);

    sqlx::query(
        "INSERT INTO workspace_commit_intents (session_id, workspace_root, revision, request_digest, state, entries_json) \
         VALUES (?, ?, 'rev_big', 'dig', 'applying', ?)",
    )
    .bind(session.to_string())
    .bind(&root)
    .bind(&oversized)
    .execute(mgr.pool().as_ref())
    .await
    .unwrap();

    let err = mgr.startup_recovery().await.unwrap_err();
    assert!(matches!(err, SessionError::RecoveryConflict(_)));
    let state: String =
        sqlx::query_scalar("SELECT state FROM workspace_commit_intents WHERE revision = 'rev_big'")
            .fetch_one(mgr.pool().as_ref())
            .await
            .unwrap();
    assert_eq!(state, "recovery_conflict");

    // The write side enforces the same cap.
    let err = db::claim_session_and_insert_intent(
        mgr.pool().as_ref(),
        &session.to_string(),
        "rev_oversized_write",
        &root,
        "dig",
        &oversized,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        nexus_local_db::LocalDbError::ValidationError(_)
    ));
}

#[tokio::test]
#[serial]
async fn unknown_session_rejected() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    mgr.startup_recovery().await.expect("startup");

    let ghost = SessionId("ws_00000000-0000-0000-0000-000000000000".to_string());
    let err = mgr
        .commit_session_durable(&ghost, &[create_change("ghost.txt", b"x")], &root)
        .await
        .unwrap_err();
    assert!(
        matches!(err, SessionError::NotFound(_)),
        "unknown session must be NotFound, got {err:?}"
    );
    assert!(!ws.path().join("ghost.txt").exists());
    assert_eq!(intent_row_count(mgr.pool().as_ref(), &ghost.to_string()).await, 0);
}

#[tokio::test]
#[serial]
async fn recoverable_claim_race_has_single_winner() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    mgr.startup_recovery().await.expect("startup");
    let session = mgr.open_session(&root, "", true).await.expect("open");

    // Two concurrent durable commits on ONE session, identical manifest.
    // Exactly one claim may be admitted; the loser either replays the winner's
    // revision or is rejected as already-committed. Never two revisions.
    let changes = vec![create_change("race.txt", b"contended")];
    let mut handles = Vec::new();
    for _ in 0..2 {
        let mgr2 = Arc::clone(&mgr);
        let session_id = session.clone();
        let changes = changes.clone();
        let owner_root = root.clone();
        let _ = &owner_root;
        handles.push(tokio::spawn(async move {
            WorkspaceSessionManager::commit_session_durable_owned(mgr2, session_id, changes).await
        }));
    }

    let mut revisions: Vec<String> = Vec::new();
    let mut rejections = 0usize;
    for handle in handles {
        match handle.await.expect("join") {
            Ok(outcome) => revisions.push(outcome.revision),
            Err(SessionError::AlreadyCommitted(_)) => rejections += 1,
            Err(other) => panic!("unexpected claim-race error: {other:?}"),
        }
    }

    assert!(!revisions.is_empty(), "one contender must win");
    assert_eq!(
        revisions.iter().collect::<std::collections::HashSet<_>>().len(),
        1,
        "all winners must report the SAME revision, got {revisions:?}"
    );
    assert_eq!(revisions.len() + rejections, 2);

    // The winner's bytes are intact and exactly one intent settled committed.
    assert_eq!(
        std::fs::read(ws.path().join("race.txt")).expect("committed file"),
        b"contended"
    );
    let committed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workspace_commit_intents WHERE session_id = ? AND state = 'committed'",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(committed, 1, "exactly one intent may settle committed");
    let leftover: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workspace_commit_intents WHERE session_id = ? AND state <> 'committed'",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(leftover, 0, "no unsettled intent may survive a claim race");
}

#[tokio::test]
#[serial]
async fn external_writer_after_replace_is_detected_at_recovery() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    std::fs::write(ws.path().join("r.txt"), b"one").unwrap();
    mgr.startup_recovery().await.expect("startup");
    let session = mgr.open_session(&root, "", true).await.expect("open");

    // Apply succeeds, then the commit dies before finalize.
    set_test_crash_point(Some("after_file_apply"));
    let err = mgr
        .commit_session_durable(
            &session,
            &[modify_change("r.txt", &hash_bytes(b"one"), b"two")],
            &root,
        )
        .await
        .unwrap_err();
    set_test_crash_point(None);
    assert!(matches!(err, SessionError::Internal(_)));
    assert_eq!(std::fs::read(ws.path().join("r.txt")).unwrap(), b"two");

    // An external writer lands in the window after the mutation.
    std::fs::write(ws.path().join("r.txt"), b"third-state").unwrap();

    // Recovery must NOT settle committed, and must NOT overwrite the writer.
    let err = mgr.startup_recovery().await.unwrap_err();
    assert!(
        matches!(err, SessionError::RecoveryConflict(_) | SessionError::Io(_)),
        "post-mutation third state must block recovery, got {err:?}"
    );
    assert_eq!(
        std::fs::read(ws.path().join("r.txt")).unwrap(),
        b"third-state",
        "recovery must never overwrite external bytes"
    );
    let state: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(state, "recovery_conflict");
}

#[tokio::test]
#[serial]
async fn external_writer_after_delete_is_detected_at_recovery() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    std::fs::write(ws.path().join("d.txt"), b"doomed").unwrap();
    mgr.startup_recovery().await.expect("startup");
    let session = mgr.open_session(&root, "", true).await.expect("open");

    set_test_crash_point(Some("after_file_apply"));
    let err = mgr
        .commit_session_durable(
            &session,
            &[ChangeEntry {
                path: "d.txt".to_string(),
                op: ChangeOp::Delete,
                expected_hash: Some(hash_bytes(b"doomed")),
                content_base64: None,
            }],
            &root,
        )
        .await
        .unwrap_err();
    set_test_crash_point(None);
    assert!(matches!(err, SessionError::Internal(_)));
    assert!(
        !ws.path().join("d.txt").exists(),
        "the delete itself is atomic and effective"
    );

    // An external writer re-creates the name after our delete landed.
    std::fs::write(ws.path().join("d.txt"), b"resurrected").unwrap();

    let err = mgr.startup_recovery().await.unwrap_err();
    assert!(
        matches!(err, SessionError::RecoveryConflict(_) | SessionError::Io(_)),
        "post-delete third state must block recovery, got {err:?}"
    );
    assert_eq!(
        std::fs::read(ws.path().join("d.txt")).unwrap(),
        b"resurrected",
        "recovery must never delete bytes it did not write"
    );
    let state: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(state, "recovery_conflict");
}

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
    set_test_after_delete_capture_hook(Some(Arc::new(move || {
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
    set_test_after_delete_capture_hook(None);

    assert!(
        matches!(err, SessionError::Internal(_) | SessionError::Io(_)),
        "a re-created target after capture must never finalize a delete, got {err:?}"
    );

    // The commit is NOT recorded as committed, and the writer's bytes survive
    // untouched — the original is preserved as capture evidence.
    let state: String = sqlx::query_scalar(
        "SELECT state FROM workspace_commit_intents WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
    )
    .bind(session.to_string())
    .fetch_one(mgr.pool().as_ref())
    .await
    .unwrap();
    assert_eq!(state, "recovery_conflict");
    assert_eq!(
        std::fs::read(ws.path().join("d.txt")).unwrap(),
        b"recreated-by-external-writer",
        "the delete must not remove bytes it did not capture"
    );

    // The displaced original is preserved as evidence, not discarded.
    let preserved: Vec<_> = std::fs::read_dir(ws.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("displaced"))
        .collect();
    assert!(
        !preserved.is_empty(),
        "the captured original must survive as evidence, found: {preserved:?}"
    );

    // And the workspace stays blocked until an operator resolves the conflict.
    let next = mgr.open_session(&root, "", true).await.expect("open2");
    let err = mgr
        .commit_session_durable(&next, &[create_change("after.txt", b"x")], &root)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::RecoveryConflict(_)));
    assert!(!ws.path().join("after.txt").exists());
}
