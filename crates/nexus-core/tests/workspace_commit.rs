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
use nexus_core::execution::executor::WorkspaceCommitExecutor;
use nexus_core::execution::session::{
    compute_content_hashes, occ_conflict_total, ChangeEntry, ChangeOp, SessionError, SessionId,
    WorkspaceSessionManager,
};
use nexus_core::execution::state_provider::CoreWorkspaceStateProvider;
use nexus_core::execution::test_hooks::{
    set_after_delete_capture_hook, set_crash_point, set_owner_gate, OwnerGate,
};
use nexus_orchestration::capability::{
    CapabilityRegistry, CapabilityRegistryHolder, CapabilityRuntimeDeps, WorkspaceExecutor,
};
use nexus_orchestration::engine::GraphFlowEngine;
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use nexus_orchestration::OrchestrationEngine;
use nexus_preset::load_preset_from_str;
use serde_json::json;
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
        .commit_session_durable(
            &session2,
            &[modify_change("occ.txt", &changed, b"v2")],
            &root,
        )
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
    use nexus_contracts::local::orchestration::{
        WorkspaceChangeEntry, WorkspaceChangeOp, WorkspaceCommitInput, WorkspaceOpenInput,
    };
    use nexus_core::execution::executor::WorkspaceCommitExecutor;

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

/// v1.191 P0 T4 (#318): the manifest-body base64 boundary on the bumped
/// `base64 0.23.1`. Malformed bodies stay refusals of the commit authority and
/// a well-formed body still round-trips through the same call — the assertions
/// pin accept/reject behaviour, never the decoder's error rendering.
#[tokio::test]
#[serial]
async fn malformed_manifest_base64_bodies_are_refused() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);
    let ws = tempfile::tempdir().unwrap();
    let root = ws.path().to_string_lossy().to_string();
    let session = mgr.open_session(&root, "", true).await.expect("open");

    // Malformed bodies: truncated quad, wrong/embedded padding, non-canonical
    // trailing bits, interior whitespace, and URL-safe symbols that the
    // standard alphabet does not accept. None may create a file.
    for malformed in [
        "A",    // truncated quad
        "AB=",  // wrong padding count
        "AA=A", // padding inside the quad
        "AB==", // non-canonical trailing bits in the last symbol
        "AB C", // interior whitespace is not a symbol
        "AB*",  // non-alphabet character
        "AA--", // URL-safe symbol in the standard alphabet
        "AB_",  // URL-safe symbol in the standard alphabet
    ] {
        let path = format!("malformed_{}.txt", malformed.replace(['/', ' '], "_"));
        let change = ChangeEntry {
            path: path.clone(),
            op: ChangeOp::Create,
            expected_hash: None,
            content_base64: Some(malformed.to_string()),
        };
        let err = mgr
            .commit_session_durable(&session, std::slice::from_ref(&change), &root)
            .await
            .unwrap_err();
        match err {
            SessionError::ManifestInvalid(message) => assert!(
                message.contains("base64"),
                "{malformed:?} must be refused at the base64 boundary: {message}"
            ),
            other => panic!("{malformed:?} must be a manifest refusal, got {other:?}"),
        }
        assert!(
            !ws.path().join(&path).exists(),
            "{malformed:?} must not create its target file"
        );
    }

    // Control on the same session: the identical call path accepts a
    // well-formed body, so the refusals above are the decode boundary.
    let content = b"v1.191 base64 boundary";
    mgr.commit_session_durable(&session, &[create_change("accepted.txt", content)], &root)
        .await
        .expect("a well-formed body still commits");
    assert_eq!(
        std::fs::read(ws.path().join("accepted.txt")).expect("committed file"),
        content
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Concurrent session consumption
// ─────────────────────────────────────────────────────────────────────────────

/// MIGRATED from `crates/nexus-daemon-runtime/tests/workspace_occ_concurrent.rs`
/// `concurrent_consume_single_winner` and `consume_after_commit_is_stale`.
///
/// The workspace session is single-consumer: two concurrent consumers of one
/// session produce exactly one winner (the loser gets `AlreadyCommitted` and
/// bumps the OCC conflict counter), and a later re-entry on the consumed
/// session is stale for the same reason.
#[tokio::test]
#[serial]
async fn concurrent_consume_has_one_winner_and_a_stale_reentry() {
    let (pool, _dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new(pool);

    let ws_dir = tempfile::tempdir().unwrap();
    let ws_root = ws_dir.path().to_string_lossy().to_string();
    let session = mgr
        .open_session(&ws_root, "", false)
        .await
        .expect("open_session");

    let before = occ_conflict_total();
    let first = {
        let mgr = mgr.clone();
        let session = session.clone();
        tokio::spawn(async move { mgr.consume_session(&session).await })
    };
    let second = {
        let mgr = mgr.clone();
        let session = session.clone();
        tokio::spawn(async move { mgr.consume_session(&session).await })
    };
    let first = first.await.unwrap();
    let second = second.await.unwrap();

    assert_eq!(
        usize::from(first.is_ok()) + usize::from(second.is_ok()),
        1,
        "exactly one concurrent consumer must win: {first:?} / {second:?}"
    );
    assert_eq!(
        usize::from(matches!(first, Err(SessionError::AlreadyCommitted(_))))
            + usize::from(matches!(second, Err(SessionError::AlreadyCommitted(_)))),
        1,
        "exactly one concurrent consumer must observe AlreadyCommitted"
    );
    assert_eq!(
        occ_conflict_total(),
        before + 1,
        "the losing consumer must bump the OCC conflict counter exactly once"
    );

    let stale = mgr.consume_session(&session).await;
    assert!(
        matches!(stale, Err(SessionError::AlreadyCommitted(_))),
        "a consumed session must stay stale: {stale:?}"
    );
    assert_eq!(
        occ_conflict_total(),
        before + 2,
        "the stale re-entry must bump the counter too"
    );
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/workspace_occ_concurrent.rs`
/// `concurrent_commit_session_single_winner`.
///
/// The combined validate + consume `commit_session` path keeps the same
/// single-consumer guarantee.
#[tokio::test]
#[serial]
async fn concurrent_commit_session_has_one_winner() {
    let (pool, _dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new(pool);

    let ws_dir = tempfile::tempdir().unwrap();
    let ws_root = ws_dir.path().to_string_lossy().to_string();
    let session = mgr
        .open_session(&ws_root, "", true)
        .await
        .expect("open_session");
    let changes: Vec<ChangeEntry> = Vec::new();

    let before = occ_conflict_total();
    let first = {
        let mgr = mgr.clone();
        let session = session.clone();
        let changes = changes.clone();
        let root = ws_root.clone();
        tokio::spawn(async move { mgr.commit_session(&session, &changes, &root).await })
    };
    let second = {
        let mgr = mgr.clone();
        let session = session.clone();
        let changes = changes.clone();
        let root = ws_root.clone();
        tokio::spawn(async move { mgr.commit_session(&session, &changes, &root).await })
    };
    let first = first.await.unwrap();
    let second = second.await.unwrap();

    assert_eq!(
        usize::from(first.is_ok()) + usize::from(second.is_ok()),
        1,
        "exactly one concurrent commit must win: {first:?} / {second:?}"
    );
    assert_eq!(
        usize::from(matches!(first, Err(SessionError::AlreadyCommitted(_))))
            + usize::from(matches!(second, Err(SessionError::AlreadyCommitted(_)))),
        1,
        "the losing commit must observe AlreadyCommitted"
    );
    assert!(
        occ_conflict_total() > before,
        "the losing commit must bump the OCC conflict counter"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Manifest validation
// ─────────────────────────────────────────────────────────────────────────────

/// MIGRATED from `crates/nexus-daemon-runtime/tests/workspace_occ_concurrent.rs`
/// `validate_changes_manifest_rejects_symlink_in_modify_path`.
///
/// A symlink swapped in at a `Modify` path must be refused BEFORE the OCC hash
/// is computed: following it would hash content outside the workspace root and
/// defeat the boundary check (QC2 H-2).
#[cfg(unix)]
#[tokio::test]
#[serial]
async fn manifest_validation_rejects_a_symlink_in_a_modify_path() {
    use std::os::unix::fs::symlink;

    let (pool, _dir) = fresh_pool().await;
    let mgr = WorkspaceSessionManager::new(pool);

    let ws_dir = tempfile::tempdir().unwrap();
    std::fs::write(ws_dir.path().join("real.txt"), b"real").unwrap();
    let ws_root = ws_dir.path().to_string_lossy().to_string();
    let session = mgr
        .open_session(&ws_root, "", true)
        .await
        .expect("open_session");

    let stored = compute_content_hashes(ws_dir.path())
        .await
        .expect("content hashes");
    let stored_hash = stored.hashes.get("real.txt").cloned().expect("tracked");

    // Swap the tracked file for a symlink pointing OUTSIDE the workspace root.
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), b"secret-outside").unwrap();
    std::fs::remove_file(ws_dir.path().join("real.txt")).unwrap();
    symlink(
        outside.path().join("secret.txt"),
        ws_dir.path().join("real.txt"),
    )
    .expect("symlink");

    let changes = vec![ChangeEntry {
        path: "real.txt".to_string(),
        op: ChangeOp::Modify,
        expected_hash: Some(stored_hash),
        content_base64: Some(b64(b"placeholder")),
    }];
    let err = mgr
        .validate_contract_manifest(&session, &changes)
        .await
        .expect_err("a symlink at a modify path must be refused");
    assert!(
        matches!(err, SessionError::PathEscape { .. }),
        "expected PathEscape for the symlink, got {err:?}"
    );
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/workspace_occ_concurrent.rs`
/// `recoverable_create_modify_delete_and_idempotent_retry`.
///
/// The durable commit authority applies the full create/modify/delete
/// lifecycle, and re-committing the same manifest on a consumed session is an
/// idempotent retry (same revision), never a second write.
#[tokio::test]
#[serial]
async fn recoverable_create_modify_delete_and_idempotent_retry() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(pool, &db_dir);

    let ws_dir = tempfile::tempdir().unwrap();
    let ws_root = ws_dir.path().to_string_lossy().to_string();
    let session = mgr.open_session(&ws_root, "", true).await.expect("open");

    let create_bytes = b"hello durable";
    let create_b64 = b64(create_bytes);
    let create = vec![ChangeEntry {
        path: "note.txt".to_string(),
        op: ChangeOp::Create,
        expected_hash: None,
        content_base64: Some(create_b64.clone()),
    }];
    let first = mgr
        .commit_session_durable(&session, &create, &ws_root)
        .await
        .expect("create commit");
    assert!(first.committed);
    assert!(first.revision.starts_with("rev_"));
    assert_eq!(
        std::fs::read(ws_dir.path().join("note.txt")).unwrap(),
        create_bytes
    );

    let session2 = mgr.open_session(&ws_root, "", true).await.expect("reopen");
    let hashes = compute_content_hashes(ws_dir.path()).await.expect("hash");
    let pre = hashes.hashes.get("note.txt").cloned().expect("tracked");
    let modify_bytes = b"hello modified";
    let modify = vec![ChangeEntry {
        path: "note.txt".to_string(),
        op: ChangeOp::Modify,
        expected_hash: Some(pre),
        content_base64: Some(b64(modify_bytes)),
    }];
    let second = mgr
        .commit_session_durable(&session2, &modify, &ws_root)
        .await
        .expect("modify commit");
    assert_ne!(second.revision, first.revision);

    let session3 = mgr.open_session(&ws_root, "", true).await.expect("reopen");
    let hashes = compute_content_hashes(ws_dir.path()).await.expect("hash");
    let pre = hashes.hashes.get("note.txt").cloned().expect("tracked");
    let delete = vec![ChangeEntry {
        path: "note.txt".to_string(),
        op: ChangeOp::Delete,
        expected_hash: Some(pre),
        content_base64: None,
    }];
    mgr.commit_session_durable(&session3, &delete, &ws_root)
        .await
        .expect("delete commit");
    assert!(!ws_dir.path().join("note.txt").exists());

    let session4 = mgr.open_session(&ws_root, "", true).await.expect("reopen");
    let recreate = vec![ChangeEntry {
        path: "note.txt".to_string(),
        op: ChangeOp::Create,
        expected_hash: None,
        content_base64: Some(create_b64),
    }];
    let committed = mgr
        .commit_session_durable(&session4, &recreate, &ws_root)
        .await
        .expect("recreate");
    let retry = mgr
        .commit_session_durable(&session4, &recreate, &ws_root)
        .await
        .expect("idempotent retry on a consumed session");
    assert_eq!(
        retry.revision, committed.revision,
        "re-committing the same manifest must be the same revision, not a second write"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Preset workflow over live workspace state
// ─────────────────────────────────────────────────────────────────────────────

/// A preset whose `check` state branches on live workspace state
/// (MIGRATED from
/// `crates/nexus-daemon-runtime/tests/workspace_preset_workflow.rs`).
const WORKSPACE_BRANCH_PRESET: &str = r#"
preset:
  id: retained-workspace-proof
  version: 1
  kind: creator
  description: retained workspace state wiring proof
  requires_capabilities:
    - workspace.open
    - workspace.commit
  initial: open_scope
  terminal: done
states:
  - id: open_scope
    enter:
      - kind: capability
        name: workspace.open
        args:
          path: pkg
    exit_when: { kind: rule }
    next: check
  - id: check
    enter: []
    exit_when: { kind: rule }
    next:
      kind: conditional
      rules:
        - when: "_context.workspace.committed"
          target: committed_state
      default: pending_state
  - id: committed_state
    enter: []
    exit_when: { kind: rule }
    next: done
  - id: pending_state
    enter: []
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
"#;

/// Drive the preset until the branch target is reached, returning it.
async fn routed_state(
    engine: &GraphFlowEngine,
    registry: &Arc<CapabilityRegistry>,
    label: &str,
) -> String {
    let loaded = load_preset_from_str(WORKSPACE_BRANCH_PRESET, registry).expect("preset loads");
    let session = engine
        .start_session_with_preset(&loaded)
        .await
        .expect("preset session starts");

    for _ in 0..8 {
        let current = engine
            .get_current_task_id(&session)
            .await
            .expect("cursor readable");
        if let Some(state) = current {
            if state == "committed_state" || state == "pending_state" {
                return state;
            }
            assert_ne!(
                state, "done",
                "{label}: reached terminal without a recorded branch target"
            );
        }
        engine.run_step(&session).await.expect("step succeeds");
    }
    panic!("{label}: preset never reached a branch target");
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/workspace_preset_workflow.rs`
/// `production_preset_workflow_branches_on_live_workspace_state`.
///
/// The production wiring is exercised end to end: `workspace.open` /
/// `workspace.commit` route through `WorkspaceCommitExecutor` into the shared
/// session manager, and a preset conditional resolves
/// `_context.workspace.committed` live from `CoreWorkspaceStateProvider` over
/// that same manager — so the branch flips only after a real durable commit.
#[tokio::test]
#[serial]
async fn preset_graph_branches_on_live_committed_workspace_state() {
    let (pool, db_dir) = fresh_pool().await;
    let mgr = recoverable_mgr(Arc::clone(&pool), &db_dir);
    mgr.startup_recovery().await.expect("startup recovery");

    let ws_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(ws_dir.path().join("pkg")).expect("workspace root");
    let root = std::fs::canonicalize(ws_dir.path())
        .expect("canonicalize")
        .display()
        .to_string();

    let deps = CapabilityRuntimeDeps {
        pool: Some(pool.as_ref().clone()),
        prompt_executor: None,
        session_cancels: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        daemon_tool_dispatch: None,
        cdn_config: None,
        workspace_executor: Some(Arc::new(WorkspaceCommitExecutor::new(
            Arc::clone(&mgr),
            root.clone(),
        ))),
    };
    let registry = Arc::new(CapabilityRegistry::with_runtime_deps(&deps));

    let mut engine = GraphFlowEngine::new_with_storage(
        Arc::new(SqliteSessionStorage::new(Arc::clone(&pool))),
        CapabilityRegistryHolder::with_registry(Arc::clone(&registry)),
    );
    engine.set_workspace_state_provider(Arc::new(CoreWorkspaceStateProvider::new(
        Arc::clone(&mgr),
        root.clone(),
    )));

    // Phase 1: nothing committed for this workspace -> the branch must resolve
    // to `pending_state`, proving the state is not a constant.
    assert_eq!(
        routed_state(&engine, &registry, "pre-commit").await,
        "pending_state"
    );

    // Phase 2: commit through the production capability path (registry ->
    // WorkspaceCommitExecutor -> shared manager -> DB + files).
    let opened = registry
        .get("workspace.open")
        .expect("workspace.open registered")
        .run(json!({ "path": "pkg" }))
        .await
        .expect("workspace.open runs");
    let session_id = opened["sessionId"]
        .as_str()
        .expect("sessionId present")
        .to_string();
    let committed = registry
        .get("workspace.commit")
        .expect("workspace.commit registered")
        .run(json!({
            "sessionId": session_id,
            "changes": [{
                "path": "wired.txt",
                "op": "create",
                "contentBase64": b64(b"wired-through-core"),
            }],
        }))
        .await
        .expect("workspace.commit runs");
    let revision = committed["revision"]
        .as_str()
        .expect("revision")
        .to_string();
    assert!(revision.starts_with("rev_"), "got {revision}");
    assert_eq!(
        std::fs::read(ws_dir.path().join("pkg/wired.txt")).expect("committed file on disk"),
        b"wired-through-core"
    );
    let intent = nexus_local_db::latest_committed_intent_for_root(&mgr.pool(), &root)
        .await
        .expect("intent lookup")
        .expect("committed intent row");
    assert_eq!(intent.revision, revision);
    assert_eq!(intent.state, nexus_local_db::IntentState::Committed);

    // Phase 3: the same preset now observes the committed state and branches
    // the other way — resolved live from the shared manager, not a fixture.
    assert_eq!(
        routed_state(&engine, &registry, "post-commit").await,
        "committed_state"
    );
}
