//! v1.188 P3 L2 proof tests — scope coordinates, idempotency, conflicts, recovery hooks.

#![allow(clippy::unwrap_used)]

use base64::Engine;
use nexus_daemon_runtime::workspace::session::{
    ChangeEntry, ChangeOp, SessionError, WorkspaceSessionManager,
};
use nexus_local_db as db;
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

#[tokio::test]
#[serial]
async fn subdirectory_scope_create_bytes() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new_recoverable(pool, db_dir.path().join("state.db"))
        .expect("recoverable");
    let ws = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(ws.path().join("pkg")).unwrap();

    let session = mgr
        .open_session(&ws.path().to_string_lossy(), "pkg", true)
        .await
        .expect("open scope");
    let bytes = b"scoped";
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let changes = vec![ChangeEntry {
        path: "note.txt".to_string(),
        op: ChangeOp::Create,
        expected_hash: None,
        content_base64: Some(b64),
    }];
    mgr.commit_session_durable(&session, &changes, &ws.path().to_string_lossy())
        .await
        .expect("commit");
    assert_eq!(
        std::fs::read(ws.path().join("pkg/note.txt")).unwrap(),
        bytes
    );
}

#[tokio::test]
#[serial]
async fn corrupt_snapshot_rejected() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new_recoverable(pool, db_dir.path().join("state.db"))
        .expect("recoverable");
    let ws = tempfile::tempdir().unwrap();
    let session = mgr
        .open_session(&ws.path().to_string_lossy(), "", true)
        .await
        .expect("open");
    sqlx::query("UPDATE workspace_sessions SET file_hashes_json = 'not-json' WHERE session_id = ?")
        .bind(session.to_string())
        .execute(mgr.pool().as_ref())
        .await
        .unwrap();
    let changes = vec![ChangeEntry {
        path: "x.txt".to_string(),
        op: ChangeOp::Create,
        expected_hash: None,
        content_base64: Some(base64::engine::general_purpose::STANDARD.encode(b"x")),
    }];
    let err = mgr
        .validate_contract_manifest(&session, &changes)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::CorruptSnapshot(_)));
}

#[tokio::test]
#[serial]
async fn empty_manifest_rejected() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new_recoverable(pool, db_dir.path().join("state.db"))
        .expect("recoverable");
    let ws = tempfile::tempdir().unwrap();
    let session = mgr
        .open_session(&ws.path().to_string_lossy(), "", true)
        .await
        .expect("open");
    let err = mgr
        .commit_session_durable(&session, &[], &ws.path().to_string_lossy())
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::ManifestInvalid(_)));
}

#[tokio::test]
#[serial]
async fn different_digest_rejected_after_success() {
    use base64::Engine;
    let (pool, db_dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new_recoverable(pool, db_dir.path().join("state.db"))
        .expect("recoverable");
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");
    let b64 = base64::engine::general_purpose::STANDARD.encode(b"a");
    let changes = vec![ChangeEntry {
        path: "f.txt".to_string(),
        op: ChangeOp::Create,
        expected_hash: None,
        content_base64: Some(b64),
    }];
    mgr.commit_session_durable(&session, &changes, &root)
        .await
        .expect("first");
    let other = vec![ChangeEntry {
        path: "f.txt".to_string(),
        op: ChangeOp::Create,
        expected_hash: None,
        content_base64: Some(base64::engine::general_purpose::STANDARD.encode(b"b")),
    }];
    let err = mgr
        .commit_session_durable(&session, &other, &root)
        .await
        .unwrap_err();
    assert!(matches!(err, SessionError::AlreadyCommitted(_)));
}

#[tokio::test]
#[serial]
async fn simulated_crash_after_entries_then_startup_recovery() {
    use base64::Engine;
    use nexus_daemon_runtime::workspace::session_commit::set_test_crash_point;
    let (pool, db_dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new_recoverable(pool, db_dir.path().join("state.db"))
        .expect("recoverable");
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");
    let b64 = base64::engine::general_purpose::STANDARD.encode(b"payload");
    let changes = vec![ChangeEntry {
        path: "crash.txt".to_string(),
        op: ChangeOp::Create,
        expected_hash: None,
        content_base64: Some(b64),
    }];
    set_test_crash_point(Some("after_entries_persisted"));
    let err = mgr
        .commit_session_durable(&session, &changes, &root)
        .await
        .unwrap_err();
    set_test_crash_point(None);
    assert!(matches!(err, SessionError::Internal(_)));
    assert!(!ws.path().join("crash.txt").exists());
    mgr.startup_recovery().await.expect("recovery");
    assert!(!ws.path().join("crash.txt").exists());
}

#[tokio::test]
#[serial]
async fn executor_matches_manager_authority() {
    use base64::Engine;
    use nexus_contracts::local::orchestration::{
        WorkspaceCommitInput, WorkspaceOpenInput,
    };
    use nexus_daemon_runtime::workspace::executor::DaemonWorkspaceExecutor;
    use nexus_orchestration::capability::WorkspaceExecutor;
    let (pool, db_dir) = fresh_pool().await;
    let mgr = Arc::new(
        WorkspaceSessionManager::new_recoverable(pool, db_dir.path().join("state.db"))
            .expect("recoverable"),
    );
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
    let b64 = base64::engine::general_purpose::STANDARD.encode(b"via-cap");
    let committed = exec
        .commit(WorkspaceCommitInput {
            session_id: opened.session_id,
            changes: vec![nexus_contracts::local::orchestration::WorkspaceChangeEntry {
                path: "cap.txt".to_string(),
                op: nexus_contracts::local::orchestration::WorkspaceChangeOp::Create,
                expected_hash: None,
                content_base64: Some(b64),
            }],
        })
        .await
        .expect("commit");
    assert!(committed.revision.starts_with("rev_"));
    assert_eq!(
        std::fs::read(ws.path().join("pkg/cap.txt")).unwrap(),
        b"via-cap"
    );
}
