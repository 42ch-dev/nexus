//! Durable run-state regression tests (v1.186 P0, A2).
//!
//! Verifies the authoritative run-transition store against a real SQLite
//! database:
//!   1. Reopen truth — terminal/wait status survives restart, not disguised
//!      as `running`.
//!   2. Stale-save / CAS races — a stale graph save cannot overwrite a
//!      terminal/wait row; `commit_transition` with a wrong expected revision
//!      fails.
//!   3. Descriptor / child identity — `RunDescriptorV1` is reconstructible
//!      and child checkpoint identity is preserved.
//!   4. Unchanged legacy / corrupt evidence — v0 rows stay legacy/unverified;
//!      corrupt blobs are preserved, not reinterpreted.
//!   5. Upgrade of an old migrated DB — a DB migrated before the new migration
//!      upgrades cleanly (idempotent `run_migrations`).
//!
//! Run: `SQLX_OFFLINE=true cargo test -p nexus-orchestration --test workflow_run_state`

use graph_flow::{Session, SessionStorage};
use nexus_orchestration::engine::{SessionId, SessionStatus};
use nexus_orchestration::run_state::{
    AgentBinding, ChildCheckpoint, PresetSourceIdentity, RunCheckpoint, RunDescriptorV1,
    RunStateV1, WorkflowStateStore,
};
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Open a fresh on-disk temp SQLite pool with migrations applied.
async fn fresh_pool() -> (Arc<sqlx::SqlitePool>, tempfile::NamedTempFile) {
    let db = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(db.path())
        .await
        .expect("open pool");
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("run migrations");
    (Arc::new(pool), db)
}

/// Build a frozen v1 descriptor for tests.
fn test_descriptor(_session_id: &str) -> RunDescriptorV1 {
    let mut agent_bindings = HashMap::new();
    agent_bindings.insert(
        "default".to_string(),
        AgentBinding {
            provider_id: "test-provider".to_string(),
            model: Some("test-model".to_string()),
        },
    );
    RunDescriptorV1 {
        creator_id: "ctr_test".to_string(),
        work_id: Some("work-1".to_string()),
        workspace_root: PathBuf::from("/tmp/ws"),
        preset_id: "test-preset".to_string(),
        preset_version: 3,
        source: PresetSourceIdentity::Embedded {
            preset_id: "test-preset".to_string(),
            content_hash: [7u8; 32],
        },
        input: serde_json::json!({"topic": "durable"}).as_object().unwrap().clone(),
        agent_bindings,
        parent_session_id: None,
        graph_name: None,
    }
}

/// Build a root session snapshot.
async fn root_session(session_id: &str, task: &str) -> Session {
    let s = Session::new_from_task(session_id.to_string(), task);
    s.context.set("_session_id", session_id.to_string()).await;
    s
}

/// Build a child checkpoint.
async fn child_checkpoint(child_id: &str, task: &str) -> ChildCheckpoint {
    let s = Session::new_from_task(child_id.to_string(), task);
    s.context.set("_session_id", child_id.to_string()).await;
    ChildCheckpoint {
        session: s,
        status: SessionStatus::Running,
        state: RunStateV1::default(),
    }
}

// ---------------------------------------------------------------------------
// 1. Reopen truth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn terminal_status_survives_reopen_not_disguised_as_running() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let session_id = SessionId("sess-reopen".to_string());

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (first)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (first)");
        let storage = SqliteSessionStorage::new(Arc::new(pool));

        // Start a v1 run.
        let root = root_session(&session_id.0, "task_a").await;
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .start_run(&session_id, &test_descriptor(&session_id.0), checkpoint, &RunStateV1::default())
            .await
            .expect("start_run");

        // Commit a terminal transition (completed).
        let root = root_session(&session_id.0, "task_z").await;
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .commit_transition(
                &session_id,
                1,
                checkpoint,
                SessionStatus::Completed,
                &RunStateV1::default(),
            )
            .await
            .expect("commit completed");
    } // pool drops — simulates daemon shutdown

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (second)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (second) — idempotent");
        let storage = SqliteSessionStorage::new(Arc::new(pool));

        let record = storage
            .load_run(&session_id)
            .await
            .expect("load_run")
            .expect("run present");
        assert_eq!(
            record.status,
            SessionStatus::Completed,
            "terminal status must survive restart, not be disguised as running"
        );
        assert_eq!(record.execution_version, 1);
        assert_eq!(record.state_revision, 2, "revision advanced on transition");
        assert!(record.descriptor.is_some(), "v1 descriptor reconstructible");
    }
}

#[tokio::test]
async fn wait_status_survives_reopen() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let session_id = SessionId("sess-wait-reopen".to_string());

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (first)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (first)");
        let storage = SqliteSessionStorage::new(Arc::new(pool));

        let root = root_session(&session_id.0, "wait_task").await;
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .start_run(&session_id, &test_descriptor(&session_id.0), checkpoint, &RunStateV1::default())
            .await
            .expect("start_run");

        // Commit a human-wait transition with a durable wait record.
        let wait_state = RunStateV1 {
            wait: Some(nexus_orchestration::run_state::WaitRecord {
                wait_id: "wait-abc".to_string(),
                task_id: "wait_task".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: nexus_orchestration::run_state::WaitKind::Manual,
            }),
            ..RunStateV1::default()
        };
        let root = root_session(&session_id.0, "wait_task").await;
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .commit_transition(
                &session_id,
                1,
                checkpoint,
                SessionStatus::WaitingForInput,
                &wait_state,
            )
            .await
            .expect("commit wait");
    }

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (second)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (second)");
        let storage = SqliteSessionStorage::new(Arc::new(pool));

        let record = storage
            .load_run(&session_id)
            .await
            .expect("load_run")
            .expect("run present");
        assert_eq!(
            record.status,
            SessionStatus::WaitingForInput,
            "wait must survive restart, not be presented as runnable-without-continue"
        );
        let state = record.state.expect("v1 state present");
        let wait = state.wait.expect("wait record retained through restart");
        assert_eq!(wait.wait_id, "wait-abc", "wait token retained");
        assert_eq!(wait.task_id, "wait_task");
    }
}

// ---------------------------------------------------------------------------
// 2. Stale-save / CAS races
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stale_graph_save_cannot_overwrite_terminal_status() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-stale".to_string());

    // Start a v1 run.
    let root = root_session(&session_id.0, "task_a").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&session_id, &test_descriptor(&session_id.0), checkpoint, &RunStateV1::default())
        .await
        .expect("start_run");

    // Commit a terminal transition.
    let root = root_session(&session_id.0, "task_z").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .commit_transition(
            &session_id,
            1,
            checkpoint,
            SessionStatus::Completed,
            &RunStateV1::default(),
        )
        .await
        .expect("commit completed");

    // A stale graph save (position-only) must NOT flip the row back to
    // running or overwrite the terminal status.
    let stale = root_session(&session_id.0, "task_stale").await;
    storage
        .save(stale)
        .await
        .expect("stale save is best-effort, not an error");

    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(
        record.status,
        SessionStatus::Completed,
        "stale save must not overwrite terminal status"
    );
    assert_eq!(record.state_revision, 2, "stale save must not advance revision");
}

#[tokio::test]
async fn commit_transition_with_wrong_revision_fails() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool);
    let session_id = SessionId("sess-cas".to_string());

    let root = root_session(&session_id.0, "task_a").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&session_id, &test_descriptor(&session_id.0), checkpoint, &RunStateV1::default())
        .await
        .expect("start_run");

    // Commit with a stale expected revision (0 instead of 1).
    let root = root_session(&session_id.0, "task_b").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    let err = storage
        .commit_transition(
            &session_id,
            0,
            checkpoint,
            SessionStatus::Completed,
            &RunStateV1::default(),
        )
        .await
        .expect_err("wrong expected revision must fail the CAS");

    match err {
        nexus_orchestration::engine::EngineError::RevisionMismatch {
            session_id: sid,
            expected,
            found,
        } => {
            assert_eq!(sid, "sess-cas");
            assert_eq!(expected, 0);
            assert_eq!(found, 1);
        }
        other => panic!("expected RevisionMismatch, got {other:?}"),
    }

    // The row must be unchanged (still running, revision 1).
    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(record.status, SessionStatus::Running);
    assert_eq!(record.state_revision, 1);
}

#[tokio::test]
async fn commit_transition_after_terminal_fails() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool);
    let session_id = SessionId("sess-after-terminal".to_string());

    let root = root_session(&session_id.0, "task_a").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&session_id, &test_descriptor(&session_id.0), checkpoint, &RunStateV1::default())
        .await
        .expect("start_run");

    // Commit terminal.
    let root = root_session(&session_id.0, "task_z").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .commit_transition(
            &session_id,
            1,
            checkpoint,
            SessionStatus::Completed,
            &RunStateV1::default(),
        )
        .await
        .expect("commit completed");

    // A second transition (even with the correct revision) must be refused.
    let root = root_session(&session_id.0, "task_zz").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    let err = storage
        .commit_transition(
            &session_id,
            2,
            checkpoint,
            SessionStatus::Running,
            &RunStateV1::default(),
        )
        .await
        .expect_err("transition after terminal must be refused");

    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::TerminalState(_)
        ),
        "expected TerminalState, got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// 3. Descriptor / child identity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn descriptor_and_child_identity_preserved() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool);
    let session_id = SessionId("sess-child".to_string());

    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "parent_task").await;
    let child = child_checkpoint("sess-child:child:1", "child_task").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[child],
    };
    storage
        .start_run(&session_id, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start_run with child");

    // Reopen and verify descriptor + child identity.
    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("run present");
    let loaded_descriptor = record.descriptor.expect("descriptor present");
    assert_eq!(loaded_descriptor.creator_id, "ctr_test");
    assert_eq!(loaded_descriptor.preset_id, "test-preset");
    assert_eq!(loaded_descriptor.preset_version, 3);
    assert_eq!(loaded_descriptor.work_id.as_deref(), Some("work-1"));
    assert_eq!(
        loaded_descriptor.agent_bindings.get("default").unwrap().provider_id,
        "test-provider"
    );
    assert_eq!(
        loaded_descriptor.source,
        PresetSourceIdentity::Embedded {
            preset_id: "test-preset".to_string(),
            content_hash: [7u8; 32],
        }
    );

    // Child checkpoint identity preserved in the DB.
    let child_row = storage
        .get("sess-child:child:1")
        .await
        .expect("child session present")
        .expect("child row");
    assert_eq!(child_row.current_task_id, "child_task");
}

// ---------------------------------------------------------------------------
// 4. Unchanged legacy / corrupt evidence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v0_legacy_rows_stay_legacy_unverified() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());

    // Seed a v0 row (execution_version defaults to 0) with a running status.
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at)
         VALUES ('sess-v0', 'ctr_t', 'preset_t', 7, 'running', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300)",
    )
    .execute(&*pool)
    .await
    .expect("seed v0 row");

    let record = storage
        .load_run(&SessionId("sess-v0".to_string()))
        .await
        .expect("load_run")
        .expect("v0 row present");
    assert_eq!(
        record.execution_version, 0,
        "v0 rows are legacy evidence markers"
    );
    assert!(
        record.descriptor.is_none(),
        "v0 rows carry no descriptor (legacy/unverified)"
    );
    assert!(
        record.state.is_none(),
        "v0 rows carry no durable state (legacy/unverified)"
    );
    // Status is diagnostic only for v0 — not reinterpreted.
    assert_eq!(record.status, SessionStatus::Running);
}

#[tokio::test]
async fn corrupt_blobs_preserved_not_reinterpreted() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());

    // Seed a v1 row with a corrupt run_state_json blob.
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES ('sess-corrupt', 'ctr_t', 'preset_t', 7, 'running', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 1, 1, X'6E6F742D6A736F6E', X'616C736F2D6E6F742D6A736F6E')",
    )
    .execute(&*pool)
    .await
    .expect("seed corrupt v1 row");

    // load_run must surface the corruption as an error, not reinterpret it.
    let err = storage
        .load_run(&SessionId("sess-corrupt".to_string()))
        .await
        .expect_err("corrupt blob must not be silently reinterpreted");

    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::GraphFlow(_)
        ),
        "expected GraphFlow error for corrupt blob, got {err:?}"
    );

    // The original blob bytes must be preserved (not rewritten).
    let raw: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT run_state_json FROM orchestration_sessions WHERE session_id = 'sess-corrupt'",
    )
    .fetch_one(&*pool)
    .await
    .expect("read raw blob");
    assert_eq!(
        raw.as_deref(),
        Some(b"not-json".as_slice()),
        "corrupt blob preserved verbatim"
    );
}

// ---------------------------------------------------------------------------
// 5. Upgrade of an old migrated DB
// ---------------------------------------------------------------------------

#[tokio::test]
async fn old_migrated_db_upgrades_cleanly() {
    // Build a DB with only the pre-v1.186 migrations (exclude the new
    // workflow_execution_state migration), then run the full migration set
    // to verify the upgrade path is clean and idempotent.
    let db = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(db.path())
        .await
        .expect("open pool");

    // Copy all migrations except the new one into a temp dir and apply them
    // as the "old" schema.
    let old_migrations_dir = tempfile::tempdir().unwrap();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/nexus-local-db/migrations");
    for entry in std::fs::read_dir(&src).expect("read migrations dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "20260907000001_workflow_execution_state.sql" {
            continue;
        }
        std::fs::copy(entry.path(), old_migrations_dir.path().join(name)).expect("copy migration");
    }
    let old_migrator = sqlx::migrate::Migrator::new(old_migrations_dir.path())
        .await
        .expect("build old migrator");
    old_migrator
        .run(&pool)
        .await
        .expect("apply old migrations");

    // The new columns must NOT exist yet.
    let has_new_col: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('orchestration_sessions')
         WHERE name = 'execution_version'",
    )
    .fetch_one(&pool)
    .await
    .expect("check column");
    assert_eq!(has_new_col, 0, "old DB must lack the new column");

    // Upgrade: run the full migration set (idempotent — the new migration
    // applies, already-applied ones are skipped).
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("upgrade migrations");

    // The new columns must now be present and usable.
    let storage = SqliteSessionStorage::new(Arc::new(pool));
    let session_id = SessionId("sess-upgrade".to_string());
    let root = root_session(&session_id.0, "task_a").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&session_id, &test_descriptor(&session_id.0), checkpoint, &RunStateV1::default())
        .await
        .expect("start_run after upgrade");
    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(record.execution_version, 1);
    assert_eq!(record.status, SessionStatus::Running);
}
