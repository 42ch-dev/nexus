//! v1.188 P3 L2 proof tests — scope, idempotency, conflicts, recovery, cancellation.

#![allow(clippy::unwrap_used)]

use base64::Engine;
use nexus_contracts::local::orchestration::{
    WorkspaceChangeEntry, WorkspaceChangeOp, WorkspaceCommitInput, WorkspaceOpenInput,
};
use nexus_daemon_runtime::workspace::executor::DaemonWorkspaceExecutor;
use nexus_daemon_runtime::workspace::session::{
    ChangeEntry, ChangeOp, SessionError, WorkspaceSessionManager,
};
use nexus_daemon_runtime::workspace::session_commit::set_test_crash_point;
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
    let mgr2 = Arc::clone(&mgr);
    let session_id = session.clone();
    let changes = vec![create_change("owner.txt", b"owned")];
    let caller = tokio::spawn(async move {
        WorkspaceSessionManager::commit_session_durable_owned(mgr2, session_id, changes).await
    });
    // Let the admitted owner start before cancelling the awaiting caller.
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    caller.abort();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        if ws.path().join("owner.txt").exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        std::fs::read(ws.path().join("owner.txt")).unwrap(),
        b"owned"
    );
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
