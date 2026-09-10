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

#![allow(clippy::too_many_lines)] // deterministic run-state scenarios keep setup+assertions in one flow
#![allow(clippy::significant_drop_tightening)] // tests read state snapshots; early-drop noise without contention value

use graph_flow::{Session, SessionStorage};
use nexus_orchestration::engine::{SessionId, SessionStatus};
use nexus_orchestration::run_state::{
    AgentBinding, ChildCheckpoint, PresetSourceIdentity, RunCheckpoint, RunDescriptorV1,
    RunStateV1, WorkflowStateStore,
};
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use nexus_orchestration::OrchestrationEngine;
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
        input: serde_json::json!({"topic": "durable"})
            .as_object()
            .unwrap()
            .clone(),
        agent_bindings,
        parent_session_id: None,
        graph_name: None,
    }
}

/// Build a root session snapshot.
fn root_session(session_id: &str, task: &str) -> Session {
    let mut s = Session::new_from_task(session_id.to_string(), task);
    s.context.set("_session_id", session_id.to_string()).unwrap();
    s
}

/// Build a child checkpoint.
fn child_checkpoint(child_id: &str, task: &str) -> ChildCheckpoint {
    let mut s = Session::new_from_task(child_id.to_string(), task);
    s.context.set("_session_id", child_id.to_string()).unwrap();
    ChildCheckpoint {
        session: s,
        status: SessionStatus::Running,
        state: RunStateV1::default(),
        state_revision: 0,
        graph_name: Some("inner_graph".to_string()),
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
        let root = root_session(&session_id.0, "task_a");
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .start_run(
                &session_id,
                &test_descriptor(&session_id.0),
                checkpoint,
                &RunStateV1::default(),
            )
            .await
            .expect("start_run");

        // Commit a terminal transition (completed).
        let root = root_session(&session_id.0, "task_z");
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

        let root = root_session(&session_id.0, "wait_task");
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .start_run(
                &session_id,
                &test_descriptor(&session_id.0),
                checkpoint,
                &RunStateV1::default(),
            )
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
        let root = root_session(&session_id.0, "wait_task");
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
    let root = root_session(&session_id.0, "task_a");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(
            &session_id,
            &test_descriptor(&session_id.0),
            checkpoint,
            &RunStateV1::default(),
        )
        .await
        .expect("start_run");

    // Commit a terminal transition.
    let root = root_session(&session_id.0, "task_z");
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
    // running or overwrite the terminal status. graph-flow 0.8 makes the
    // refusal honest: the save errors with SessionConflict instead of a
    // fake-success no-op.
    let stale = root_session(&session_id.0, "task_stale");
    let err = storage
        .save(stale)
        .await
        .expect_err("stale save against terminal row must be refused");
    assert!(
        matches!(err, graph_flow::GraphError::SessionConflict(_)),
        "expected SessionConflict, got {err:?}"
    );

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
    assert_eq!(
        record.state_revision, 2,
        "stale save must not advance revision"
    );
}

#[tokio::test]
async fn commit_transition_with_wrong_revision_fails() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool);
    let session_id = SessionId("sess-cas".to_string());

    let root = root_session(&session_id.0, "task_a");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(
            &session_id,
            &test_descriptor(&session_id.0),
            checkpoint,
            &RunStateV1::default(),
        )
        .await
        .expect("start_run");

    // Commit with a stale expected revision (0 instead of 1).
    let root = root_session(&session_id.0, "task_b");
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

    let root = root_session(&session_id.0, "task_a");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(
            &session_id,
            &test_descriptor(&session_id.0),
            checkpoint,
            &RunStateV1::default(),
        )
        .await
        .expect("start_run");

    // Commit terminal.
    let root = root_session(&session_id.0, "task_z");
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
    let root = root_session(&session_id.0, "task_zz");
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
    let root = root_session(&session_id.0, "parent_task");
    let child = child_checkpoint("sess-child:child:1", "child_task");
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
        loaded_descriptor
            .agent_bindings
            .get("default")
            .unwrap()
            .provider_id,
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
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
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
    old_migrator.run(&pool).await.expect("apply old migrations");

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
    let root = root_session(&session_id.0, "task_a");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(
            &session_id,
            &test_descriptor(&session_id.0),
            checkpoint,
            &RunStateV1::default(),
        )
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

// ---------------------------------------------------------------------------
// Fix round 1 (task review): regression tests for each fixed contract
// ---------------------------------------------------------------------------

// Critical 1: paused transitions are persisted authoritatively.
#[tokio::test]
async fn paused_status_survives_reopen() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let session_id = SessionId("sess-paused-reopen".to_string());

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (first)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (first)");
        let storage = SqliteSessionStorage::new(Arc::new(pool));

        let root = root_session(&session_id.0, "task_a");
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .start_run(
                &session_id,
                &test_descriptor(&session_id.0),
                checkpoint,
                &RunStateV1::default(),
            )
            .await
            .expect("start_run");

        // Commit a paused transition.
        let root = root_session(&session_id.0, "task_b");
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .commit_transition(
                &session_id,
                1,
                checkpoint,
                SessionStatus::Paused,
                &RunStateV1::default(),
            )
            .await
            .expect("commit paused");
    } // pool drops — simulates daemon shutdown

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
            SessionStatus::Paused,
            "paused must survive restart as authoritative, not be disguised as running"
        );
        assert_eq!(record.state_revision, 2);
    }
}

// Critical 2: engine normal start path creates a v1 run via start_run.
#[tokio::test]
async fn engine_start_creates_v1_run() {
    let (pool, _db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_orchestration::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );

    let graph = Arc::new(
    graph_flow::GraphBuilder::new("test-graph")
        .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
        .build()
        .expect("test graph"),
    );
    let sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start_session");
    let suffix = sid
        .0
        .strip_prefix("novel-writing:")
        .expect("session id keeps the preset prefix");
    uuid::Uuid::parse_str(suffix).expect("root session id uses a collision-resistant UUID");

    // The run must be a v1 authoritative record, not a bare save.
    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(
        record.execution_version, 1,
        "engine start must create a v1 run"
    );
    assert_eq!(record.status, SessionStatus::Running);
    assert!(
        record.descriptor.is_some(),
        "v1 run must carry a frozen descriptor"
    );
    assert_eq!(
        record.descriptor.as_ref().unwrap().preset_id,
        "novel-writing"
    );
}

// Critical 2: the trait-level session constructor also uses start_run when a
// workflow store is configured; it must not leave a bare v0 storage row.
#[tokio::test]
async fn engine_new_session_creates_v1_run() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool);
    let sid = engine
        .new_session(
            nexus_orchestration::engine::SessionKey {
                creator_id: "ctr-new-session".to_string(),
                preset_id: "novel-writing".to_string(),
                instance_id: uuid::Uuid::new_v4().to_string(),
            },
            nexus_orchestration::engine::Context::new(),
        )
        .await
        .expect("new_session");

    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(record.execution_version, 1);
    let descriptor = record.descriptor.expect("v1 descriptor");
    assert_eq!(descriptor.creator_id, "ctr-new-session");
    assert_eq!(descriptor.preset_id, "novel-writing");
}

// Critical 3: resume must be fenced against terminal states.
#[tokio::test]
async fn resume_after_terminal_is_refused() {
    let (pool, _db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_orchestration::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );

    let graph = Arc::new(
    graph_flow::GraphBuilder::new("test-graph")
        .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
        .build()
        .expect("test graph"),
    );
    let sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start_session");

    // Commit a terminal transition directly on the store.
    let root = root_session(&sid.0, "task_z");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .commit_transition(
            &sid,
            1,
            checkpoint,
            SessionStatus::Completed,
            &RunStateV1::default(),
        )
        .await
        .expect("commit completed");

    // Resume must be refused (terminal fence).
    let err = engine
        .signal(&sid, nexus_orchestration::engine::EngineSignal::Resume)
        .await
        .expect_err("resume after terminal must be refused");
    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::TerminalState(_)
        ),
        "expected TerminalState, got {err:?}"
    );

    // The persisted status must remain terminal.
    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(record.status, SessionStatus::Completed);
}

// Critical 4: engine-path wait produces a durable wait token.
#[tokio::test]
async fn engine_wait_produces_durable_wait_token() {
    let (pool, _db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_orchestration::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );

    let graph = Arc::new(
    graph_flow::GraphBuilder::new("test-graph")
        .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
        .build()
        .expect("test graph"),
    );
    let sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start_session");

    // Step once — the ManualWaitTask produces WaitingForInput.
    let outcome = engine.run_step(&sid).await.expect("run_step");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::WaitingForInput { .. }
        ),
        "expected WaitingForInput, got {outcome:?}"
    );

    // The engine path must have persisted a durable wait record with a token.
    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(record.status, SessionStatus::WaitingForInput);
    let state = record.state.expect("v1 state present");
    let wait = state
        .wait
        .expect("engine-path wait must carry a durable WaitRecord");
    assert!(
        !wait.wait_id.is_empty(),
        "wait token must be a fresh UUID, not empty"
    );
    assert_eq!(wait.kind, nexus_orchestration::run_state::WaitKind::Manual);

    let wait_id = wait.wait_id;
    let revision = record.state_revision;
    for signal in [
        nexus_orchestration::engine::EngineSignal::Pause,
        nexus_orchestration::engine::EngineSignal::Resume,
        nexus_orchestration::engine::EngineSignal::Advance,
    ] {
        let err = engine
            .signal(&sid, signal)
            .await
            .expect_err("ordinary control signals must not consume a human wait");
        assert!(
            matches!(
                err,
                nexus_orchestration::engine::EngineError::TerminalState(_)
            ),
            "wait fence must reject the signal, got {err:?}"
        );
    }
    let after = storage
        .load_run(&sid)
        .await
        .expect("reload wait")
        .expect("run present");
    assert_eq!(after.status, SessionStatus::WaitingForInput);
    assert_eq!(
        after.state_revision, revision,
        "rejected signals do not write"
    );
    assert_eq!(
        after
            .state
            .and_then(|state| state.wait)
            .map(|wait| wait.wait_id),
        Some(wait_id),
        "only the wait-token CAS may consume the durable wait"
    );
}

#[tokio::test]
async fn in_flight_marker_fences_control_signals_and_stale_transition() {
    let (pool, _db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_orchestration::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );
    let graph = Arc::new(
    graph_flow::GraphBuilder::new("test-graph")
        .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
        .build()
        .expect("test graph"),
    );
    let sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start_session");
    let root = storage
        .get(&sid.0)
        .await
        .expect("load root")
        .expect("root present");
    let marker = RunStateV1 {
        step_in_flight: Some(root.current_task_id.clone()),
        ..RunStateV1::default()
    };
    storage
        .mark_step_in_flight(
            &sid,
            1,
            RunCheckpoint {
                root: &root,
                children: &[],
            },
            &marker,
        )
        .await
        .expect("persist marker");

    let err = engine
        .signal(&sid, nexus_orchestration::engine::EngineSignal::Pause)
        .await
        .expect_err("signal must not erase in-flight evidence");
    assert!(matches!(
        err,
        nexus_orchestration::engine::EngineError::TerminalState(_)
    ));

    let stale = storage
        .commit_transition(
            &sid,
            1,
            RunCheckpoint {
                root: &root,
                children: &[],
            },
            SessionStatus::Paused,
            &RunStateV1::default(),
        )
        .await
        .expect_err("transition loaded before the marker must lose its CAS");
    assert!(matches!(
        stale,
        nexus_orchestration::engine::EngineError::RevisionMismatch { .. }
    ));
    let after = storage
        .load_run(&sid)
        .await
        .expect("reload run")
        .expect("run present");
    assert_eq!(after.state_revision, 2);
    assert_eq!(
        after.state.and_then(|state| state.step_in_flight),
        Some(root.current_task_id),
        "in-flight evidence survives both signal orderings"
    );
}

// Important 1 + 2: child identity is reconstructible and revision-fenced.
#[tokio::test]
async fn child_identity_and_revision_fence() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-fix".to_string());

    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "parent_task");
    let child = child_checkpoint("sess-child-fix:child:1", "child_task");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[child],
    };
    storage
        .start_run(&session_id, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start_run with child");

    let boot_roots = storage
        .list_non_terminal_sessions()
        .await
        .expect("list boot roots");
    assert_eq!(
        boot_roots
            .iter()
            .map(|summary| summary.session_id.0.as_str())
            .collect::<Vec<_>>(),
        vec!["sess-child-fix"],
        "boot reconstruction lists parents only; child rows attach through their parent"
    );

    // Child identity must be reconstructible (not "unknown"/"default").
    let child_record = storage
        .load_run(&SessionId("sess-child-fix:child:1".to_string()))
        .await
        .expect("load_run")
        .expect("child run present");
    assert_eq!(child_record.execution_version, 1);
    let child_descriptor = child_record.descriptor.expect("child descriptor present");
    assert_eq!(
        child_descriptor.creator_id, "ctr_test",
        "child must inherit trusted root creator identity"
    );
    assert_eq!(
        child_descriptor.preset_id, "test-preset",
        "child must inherit trusted root preset identity"
    );
    assert_eq!(
        child_descriptor
            .parent_session_id
            .as_ref()
            .map(|s| s.0.as_str()),
        Some("sess-child-fix"),
        "child must name its parent session"
    );
    assert_eq!(
        child_descriptor.graph_name.as_deref(),
        Some("inner_graph"),
        "child must name its inner graph"
    );
    assert_eq!(
        child_descriptor.source,
        PresetSourceIdentity::Embedded {
            preset_id: "test-preset".to_string(),
            content_hash: [7u8; 32],
        },
        "child must inherit the root source identity"
    );

    // Child revision fence: a transition with a stale child revision must
    // not overwrite the child row.
    let root2 = root_session(&session_id.0, "parent_task2");
    let child2 = child_checkpoint("sess-child-fix:child:1", "child_task2");
    let checkpoint2 = RunCheckpoint {
        root: &root2,
        children: &[child2],
    };
    storage
        .commit_transition(
            &session_id,
            1,
            checkpoint2,
            SessionStatus::Running,
            &RunStateV1::default(),
        )
        .await
        .expect("commit with child");

    // The child row must have advanced its revision (fence passed with
    // matching revision 0 → 1).
    let child_after = storage
        .load_run(&SessionId("sess-child-fix:child:1".to_string()))
        .await
        .expect("load_run")
        .expect("child present");
    assert_eq!(child_after.state_revision, 1, "child revision advanced");
    assert_eq!(
        child_after.state.as_ref().unwrap().wait,
        None,
        "child state persisted"
    );
}

// Important 3: late graph saves are fenced against wait AND paused rows.
#[tokio::test]
async fn stale_save_cannot_overwrite_wait_or_paused() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());

    for (sid_str, status) in [
        ("sess-wait-fence", SessionStatus::WaitingForInput),
        ("sess-paused-fence", SessionStatus::Paused),
    ] {
        let session_id = SessionId(sid_str.to_string());
        let root = root_session(&session_id.0, "task_a");
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .start_run(
                &session_id,
                &test_descriptor(&session_id.0),
                checkpoint,
                &RunStateV1::default(),
            )
            .await
            .expect("start_run");

        let root = root_session(&session_id.0, "task_wait");
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .commit_transition(
                &session_id,
                1,
                checkpoint,
                status.clone(),
                &RunStateV1::default(),
            )
            .await
            .expect("commit");

        // A stale graph save must NOT flip the row back to running. Under
        // graph-flow 0.8 the protection is an honest SessionConflict, not a
        // silent no-op.
        let stale = root_session(&session_id.0, "task_stale");
        let err = storage
            .save(stale)
            .await
            .expect_err("stale save against protected row must be refused");
        assert!(
            matches!(err, graph_flow::GraphError::SessionConflict(_)),
            "expected SessionConflict for {status:?}, got {err:?}"
        );

        let record = storage
            .load_run(&session_id)
            .await
            .expect("load_run")
            .expect("run present");
        assert_eq!(
            record.status, status,
            "stale save must not overwrite {status:?}"
        );
        assert_eq!(
            record.state_revision, 2,
            "stale save must not advance revision"
        );
    }
}

// Important 4: commit_transition refuses every v0 row. A descriptor does not
// prove that the legacy row has a complete v1 state contract, so transition
// cannot silently promote or otherwise mutate it (A2/A7).
#[tokio::test]
async fn commit_transition_refuses_v0_rows_without_mutation() {
    for (session_id, descriptor) in [
        (
            "sess-v0-with-desc",
            Some(test_descriptor("sess-v0-with-desc")),
        ),
        ("sess-v0-no-desc", None),
    ] {
        let (pool, _db) = fresh_pool().await;
        let storage = SqliteSessionStorage::new(pool.clone());
        let descriptor_bytes = descriptor
            .as_ref()
            .map(|value| serde_json::to_vec(value).expect("serialize descriptor"));
        sqlx::query(
            "INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version, status,
                 current_task_id, context_json, created_at, updated_at,
                 execution_version, state_revision, run_descriptor_json)
             VALUES (?, 'ctr_t', 'preset_t', 7, 'running', 'task_a',
                     '{\"data\":{}}', 1756990000, 1756990300, 0, 0, ?)",
        )
        .bind(session_id)
        .bind(descriptor_bytes)
        .execute(&*pool)
        .await
        .expect("seed v0 row");

        let root = root_session(session_id, "task_z");
        let err = storage
            .commit_transition(
                &SessionId(session_id.to_string()),
                0,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                SessionStatus::Completed,
                &RunStateV1::default(),
            )
            .await
            .expect_err("legacy v0 transition must be refused");
        assert!(
            err.to_string()
                .contains("execution_version 0 is non-replayable"),
            "unexpected error: {err}"
        );

        let persisted: (i64, String, Option<Vec<u8>>) = sqlx::query_as(
            "SELECT state_revision, status, run_descriptor_json
             FROM orchestration_sessions WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_one(&*pool)
        .await
        .expect("read preserved legacy row");
        assert_eq!(persisted.0, 0, "legacy revision must remain unchanged");
        assert_eq!(
            persisted.1, "running",
            "legacy status must remain unchanged"
        );
        assert_eq!(
            persisted.2.is_some(),
            descriptor.is_some(),
            "legacy descriptor evidence must remain unchanged"
        );
    }
}

// Important 5: unknown/corrupt v1 status is non-replayable, never Running.
#[tokio::test]
async fn unknown_v1_status_is_non_replayable() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());

    // Seed a v1 row with an unknown status value.
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES ('sess-unknown-status', 'ctr_t', 'preset_t', 7, 'bogus_status', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 1, 1, '{\"wait\":null}', '{\"creator_id\":\"ctr_t\"}')",
    )
    .execute(&*pool)
    .await
    .expect("seed unknown-status v1 row");

    // load_run must surface the unknown status as an error (non-replayable).
    let err = storage
        .load_run(&SessionId("sess-unknown-status".to_string()))
        .await
        .expect_err("unknown v1 status must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error for unknown status, got {err:?}"
    );
}

// Important 5: a v1 row missing descriptor/state is non-replayable.
#[tokio::test]
async fn v1_row_missing_metadata_is_non_replayable() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());

    // Seed a v1 row with a missing run_descriptor_json.
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES ('sess-missing-desc', 'ctr_t', 'preset_t', 7, 'running', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 1, 1, '{\"wait\":null}', NULL)",
    )
    .execute(&*pool)
    .await
    .expect("seed v1 row missing descriptor");

    let err = storage
        .load_run(&SessionId("sess-missing-desc".to_string()))
        .await
        .expect_err("v1 row missing descriptor must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error, got {err:?}"
    );
}

// Important 7: WaitKind and PromptPhase encode as snake_case.
#[test]
fn wait_kind_and_prompt_phase_encode_snake_case() {
    let wait = nexus_orchestration::run_state::WaitRecord {
        wait_id: "w".to_string(),
        task_id: "t".to_string(),
        child_session_id: None,
        child_task_id: None,
        kind: nexus_orchestration::run_state::WaitKind::Manual,
    };
    let json = serde_json::to_value(&wait).expect("serialize wait");
    assert_eq!(
        json["kind"], "manual",
        "WaitKind::Manual must encode as 'manual', got {}",
        json["kind"]
    );

    let attempt = nexus_orchestration::run_state::PromptAttempt {
        attempt_id: "a".to_string(),
        task_id: "t".to_string(),
        phase: nexus_orchestration::run_state::PromptPhase::Dispatching,
        host_session_id: None,
        operation_id: None,
        process_identity: None,
    };
    let json = serde_json::to_value(&attempt).expect("serialize attempt");
    assert_eq!(
        json["phase"], "dispatching",
        "PromptPhase::Dispatching must encode as 'dispatching', got {}",
        json["phase"]
    );

    let active = nexus_orchestration::run_state::PromptAttempt {
        phase: nexus_orchestration::run_state::PromptPhase::Active,
        ..attempt
    };
    let json = serde_json::to_value(&active).expect("serialize active");
    assert_eq!(
        json["phase"], "active",
        "PromptPhase::Active must encode as 'active', got {}",
        json["phase"]
    );
}

// Important 8: legacy no-launder — start_run refuses an existing v0 row.
#[tokio::test]
async fn start_run_refuses_existing_v0_row() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());

    // Seed a v0 row (legacy evidence).
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at)
         VALUES ('sess-legacy', 'ctr_t', 'preset_t', 7, 'running', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300)",
    )
    .execute(&*pool)
    .await
    .expect("seed v0 row");

    // start_run must refuse to launder the existing v0 row in place.
    let root = root_session("sess-legacy", "task_new");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    let err = storage
        .start_run(
            &SessionId("sess-legacy".to_string()),
            &test_descriptor("sess-legacy"),
            checkpoint,
            &RunStateV1::default(),
        )
        .await
        .expect_err("start_run must refuse an existing v0 row");

    match err {
        nexus_orchestration::engine::EngineError::RunAlreadyExists {
            session_id,
            execution_version,
        } => {
            assert_eq!(session_id, "sess-legacy");
            assert_eq!(execution_version, 0, "existing row is v0 legacy");
        }
        other => panic!("expected RunAlreadyExists, got {other:?}"),
    }

    // The legacy row must be untouched (still v0, still running diagnostic).
    let record = storage
        .load_run(&SessionId("sess-legacy".to_string()))
        .await
        .expect("load_run")
        .expect("row present");
    assert_eq!(record.execution_version, 0, "legacy row not laundered");
    assert_eq!(record.status, SessionStatus::Running);
}

// Minor 1: start_run reports an accurate conflict for an already-v1 row.
#[tokio::test]
async fn start_run_reports_accurate_conflict_for_v1_row() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-already-v1".to_string());

    let root = root_session(&session_id.0, "task_a");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(
            &session_id,
            &test_descriptor(&session_id.0),
            checkpoint,
            &RunStateV1::default(),
        )
        .await
        .expect("first start_run");

    // A second start_run on the same id must report RunAlreadyExists with
    // execution_version=1 (not a misleading TerminalState).
    let root = root_session(&session_id.0, "task_b");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    let err = storage
        .start_run(
            &session_id,
            &test_descriptor(&session_id.0),
            checkpoint,
            &RunStateV1::default(),
        )
        .await
        .expect_err("second start_run must be refused");

    match err {
        nexus_orchestration::engine::EngineError::RunAlreadyExists {
            session_id: sid,
            execution_version,
        } => {
            assert_eq!(sid, "sess-already-v1");
            assert_eq!(execution_version, 1, "existing row is already v1");
        }
        other => panic!("expected RunAlreadyExists, got {other:?}"),
    }
}

// Minor 2: loader/engine integration — descriptor/source identity from an
// actual loaded preset (end-to-end, not structural only).
#[tokio::test]
async fn engine_start_with_loaded_preset_builds_source_identity() {
    let (pool, _db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_orchestration::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );

    // Load a real embedded preset (novel-writing) — exercises the actual
    // loader + source identity construction.
    let registry = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_embedded_preset("novel-writing", &registry)
        .expect("novel-writing preset loads");
    assert!(
        loaded.source_identity.is_some(),
        "embedded preset must carry a source identity"
    );

    let sid = engine
        .start_session_with_preset_for_creator(&loaded, "ctr_integration")
        .await
        .expect("start_session_with_preset_for_creator");

    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(record.execution_version, 1);
    let descriptor = record.descriptor.expect("descriptor present");
    assert_eq!(descriptor.creator_id, "ctr_integration");
    assert_eq!(descriptor.preset_id, "novel-writing");
    assert_eq!(descriptor.preset_version, loaded.version);
    assert_eq!(
        descriptor.source,
        loaded.source_identity.clone().unwrap(),
        "descriptor source identity must match the loaded preset's"
    );
}

// ---------------------------------------------------------------------------
// Fix round 2 (task re-review): regression tests for each fixed contract
// ---------------------------------------------------------------------------

// Important 1: a child CAS that matches zero rows (stale child revision)
// must roll back the root transition — never silently drop the child
// checkpoint while the root commits.
#[tokio::test]
async fn child_cas_stale_revision_rolls_back_root() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-stale".to_string());

    // Start a root run with a child at revision 0.
    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "parent_task");
    let child = child_checkpoint("sess-child-stale:child:1", "child_task");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[child],
    };
    storage
        .start_run(&session_id, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start_run with child");

    // Advance the child row to revision 1 (simulating a concurrent child
    // transition that the root does not know about).
    sqlx::query(
        "UPDATE orchestration_sessions SET state_revision = 1
         WHERE session_id = 'sess-child-stale:child:1'",
    )
    .execute(&*pool)
    .await
    .expect("advance child revision");

    // Commit a root transition with a child checkpoint still anchored at
    // revision 0 — the child CAS must fail and roll back the root.
    let root2 = root_session(&session_id.0, "parent_task2");
    let child2 = child_checkpoint("sess-child-stale:child:1", "child_task2");
    let checkpoint2 = RunCheckpoint {
        root: &root2,
        children: &[child2],
    };
    let err = storage
        .commit_transition(
            &session_id,
            1,
            checkpoint2,
            SessionStatus::Running,
            &RunStateV1::default(),
        )
        .await
        .expect_err("stale child revision must fail the root transition");

    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::RevisionMismatch { .. }
        ),
        "expected RevisionMismatch for stale child, got {err:?}"
    );

    // The root row must be unchanged (rollback): still revision 1, running.
    let root_after = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("root present");
    assert_eq!(
        root_after.state_revision, 1,
        "root transition must roll back on stale child CAS"
    );
    assert_eq!(root_after.status, SessionStatus::Running);

    // The child row must be unchanged (still revision 1, not overwritten).
    let child_after = storage
        .load_run(&SessionId("sess-child-stale:child:1".to_string()))
        .await
        .expect("load_run")
        .expect("child present");
    assert_eq!(child_after.state_revision, 1, "child row not overwritten");
}


// Unknown/corrupt child status must be a hard storage error before OCC
// classification (never RevisionMismatch or a generic CAS failure).
#[tokio::test]
async fn child_cas_unknown_status_is_storage_error_even_with_revision_mismatch() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-bogus-status".to_string());

    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "parent_task");
    let child = child_checkpoint("sess-child-bogus-status:child:1", "child_task");
    storage
        .start_run(
            &session_id,
            &descriptor,
            RunCheckpoint {
                root: &root,
                children: &[child],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start_run with child");

    sqlx::query(
        "UPDATE orchestration_sessions SET status = 'not-a-valid-status', state_revision = 2
         WHERE session_id = 'sess-child-bogus-status:child:1'",
    )
    .execute(&*pool)
    .await
    .expect("corrupt child status + advance revision");

    let root2 = root_session(&session_id.0, "parent_task2");
    let child2 = child_checkpoint("sess-child-bogus-status:child:1", "child_task2");
    let err = storage
        .commit_transition(
            &session_id,
            1,
            RunCheckpoint {
                root: &root2,
                children: &[child2],
            },
            SessionStatus::Running,
            &RunStateV1::default(),
        )
        .await
        .expect_err("unknown child status must fail closed");

    assert_hard_storage_error(&err, "child CAS with corrupt status");

    let root_after = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("root present");
    assert_eq!(root_after.state_revision, 1, "root transition must roll back");
}

// Important 1: a child CAS that matches zero rows because the child is
// already terminal must roll back the root transition with TerminalState.
#[tokio::test]
async fn child_cas_terminal_child_rolls_back_root() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-term".to_string());

    // Start a root run with a child at revision 0.
    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "parent_task");
    let child = child_checkpoint("sess-child-term:child:1", "child_task");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[child],
    };
    storage
        .start_run(&session_id, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start_run with child");

    // Mark the child terminal (completed) AND advance its revision to 1 —
    // a late root transition anchored at the stale revision 0 must not
    // overwrite a terminal child that has moved on.
    sqlx::query(
        "UPDATE orchestration_sessions SET status = 'completed', state_revision = 1
         WHERE session_id = 'sess-child-term:child:1'",
    )
    .execute(&*pool)
    .await
    .expect("mark child terminal + advance revision");

    // Commit a root transition with a child checkpoint anchored at revision
    // 0 — the child CAS must fail (terminal at a stale revision) and roll
    // back the root.
    let root2 = root_session(&session_id.0, "parent_task2");
    let child2 = child_checkpoint("sess-child-term:child:1", "child_task2");
    let checkpoint2 = RunCheckpoint {
        root: &root2,
        children: &[child2],
    };
    let err = storage
        .commit_transition(
            &session_id,
            1,
            checkpoint2,
            SessionStatus::Running,
            &RunStateV1::default(),
        )
        .await
        .expect_err("terminal child at a stale revision must fail the root transition");

    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::TerminalState(_)
        ),
        "expected TerminalState for terminal child at stale revision, got {err:?}"
    );

    // The root row must be unchanged (rollback).
    let root_after = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("root present");
    assert_eq!(
        root_after.state_revision, 1,
        "root transition must roll back on terminal child CAS"
    );
    assert_eq!(root_after.status, SessionStatus::Running);

    // The child row must remain terminal at revision 1.
    let child_after = storage
        .load_run(&SessionId("sess-child-term:child:1".to_string()))
        .await
        .expect("load_run")
        .expect("child present");
    assert_eq!(child_after.status, SessionStatus::Completed);
    assert_eq!(child_after.state_revision, 1);
}

// Important 1: a child that is already terminal at the EXPECTED revision is
// confirmed by the parent's checkpoint (no overwrite needed) — the parent's
// transition succeeds rather than failing the child CAS.
#[tokio::test]
async fn child_cas_terminal_at_expected_revision_is_confirmed() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-term-confirm".to_string());

    // Start a root run with a child at revision 0.
    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "parent_task");
    let child = child_checkpoint("sess-child-term-confirm:child:1", "child_task");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[child],
    };
    storage
        .start_run(&session_id, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start_run with child");

    // Mark the child terminal (completed) at the SAME revision 0 — the
    // parent's checkpoint confirms the child's final state.
    sqlx::query(
        "UPDATE orchestration_sessions SET status = 'completed'
         WHERE session_id = 'sess-child-term-confirm:child:1'",
    )
    .execute(&*pool)
    .await
    .expect("mark child terminal");

    // Commit a root transition with a child checkpoint anchored at revision
    // 0 AND carrying a matching terminal status (Completed) — the terminal
    // child at the expected revision is confirmed, so the root transition
    // succeeds (Important 6: confirmation requires a matching terminal
    // checkpoint, not just any terminal DB status at the expected revision).
    let root2 = root_session(&session_id.0, "parent_task2");
    let mut child2 = child_checkpoint("sess-child-term-confirm:child:1", "child_task2");
    child2.status = SessionStatus::Completed;
    let checkpoint2 = RunCheckpoint {
        root: &root2,
        children: &[child2],
    };
    let record = storage
        .commit_transition(
            &session_id,
            1,
            checkpoint2,
            SessionStatus::Running,
            &RunStateV1::default(),
        )
        .await
        .expect("terminal child at expected revision is confirmed, root transition succeeds");

    assert_eq!(record.state_revision, 2, "root transition advanced");

    // The child row must remain terminal (not overwritten).
    let child_after = storage
        .load_run(&SessionId("sess-child-term-confirm:child:1".to_string()))
        .await
        .expect("load_run")
        .expect("child present");
    assert_eq!(child_after.status, SessionStatus::Completed);
    assert_eq!(child_after.state_revision, 0, "child row not overwritten");
}

// Important 2: the production child-session path (EngineProxy/GraphFlowEngine
// spawn_child_session) creates a v1 child run with inherited trusted root
// identity, and run_step_internal persists the nested child checkpoint.
#[tokio::test]
async fn engine_nested_child_creates_v1_run() {
    let (pool, _db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_orchestration::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );

    // Start a parent session (v1 run).
    let graph = Arc::new(
    graph_flow::GraphBuilder::new("test-graph")
        .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
        .build()
        .expect("test graph"),
    );
    let parent_sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start parent session");

    // Spawn a child session via the production path.
    let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("inner_graph")
        .add_task(Arc::new(
        nexus_orchestration::tasks::InnerGraphNodeTask::new("n1"),
    ))
        .build()
        .expect("test graph"),
    );
    let params = nexus_orchestration::ChildSessionParams {
        parent_session_id: parent_sid.0.clone(),
        inner_graph,
        initial_context: graph_flow::Context::new(),
    };
    let child_sid = engine
        .spawn_child_session(params)
        .await
        .expect("spawn child session");

    // The child must be a v1 run with inherited trusted root identity.
    let child_record = storage
        .load_run(&child_sid)
        .await
        .expect("load_run")
        .expect("child run present");
    assert_eq!(
        child_record.execution_version, 1,
        "production child path must create a v1 run, not a bare save"
    );
    let child_descriptor = child_record.descriptor.expect("child descriptor present");
    assert_eq!(
        child_descriptor.creator_id, "",
        "child must inherit the root creator identity (empty for a no-creator start)"
    );
    assert_eq!(
        child_descriptor.preset_id, "novel-writing",
        "child must inherit trusted root preset identity"
    );
    assert_eq!(
        child_descriptor
            .parent_session_id
            .as_ref()
            .map(|s| s.0.as_str()),
        Some(parent_sid.0.as_str()),
        "child must name its parent session"
    );
    assert_eq!(
        child_descriptor.graph_name.as_deref(),
        Some("inner_graph"),
        "child must name its inner graph"
    );
    assert!(
        matches!(
            child_descriptor.source,
            PresetSourceIdentity::Embedded { .. }
        ),
        "child must inherit the root source identity"
    );

    // run_step on the parent must persist the nested child checkpoint
    // atomically (the child row is reconstructible after restart).
    let outcome = engine.run_step(&parent_sid).await.expect("run_step parent");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::WaitingForInput { .. }
        ),
        "expected WaitingForInput, got {outcome:?}"
    );

    // The child row must still be present and reconstructible.
    let child_after = storage
        .load_run(&child_sid)
        .await
        .expect("load_run")
        .expect("child present after parent step");
    assert_eq!(child_after.execution_version, 1);
    assert_eq!(
        child_after
            .descriptor
            .as_ref()
            .unwrap()
            .graph_name
            .as_deref(),
        Some("inner_graph")
    );
}

// Important 3: a directory preset that shadows an embedded preset with the
// same id must resolve to the directory source identity and carry the real
// directory preset version (A7 precedence: user → system → embedded).
#[tokio::test]
async fn shadowed_directory_preset_resolves_directory_source_and_version() {
    use std::fs;

    let (pool, _db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_orchestration::CapabilityRegistry::with_builtins(),
    ));
    let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );

    // Create a user directory preset that shadows the embedded novel-writing
    // preset (same id, different version).
    let nexus_home = tempfile::tempdir().unwrap();
    let bundle_dir = nexus_home.path().join("presets").join("novel-writing");
    fs::create_dir_all(&bundle_dir).unwrap();
    let override_yaml = r#"
preset:
  id: novel-writing
  version: 99
  kind: creator
  description: "user override of novel-writing"
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"#;
    fs::write(bundle_dir.join("preset.yaml"), override_yaml).unwrap();
    engine.set_nexus_home(nexus_home.path().to_path_buf());

    // Start a session for the shadowed preset id.
    let graph = Arc::new(
    graph_flow::GraphBuilder::new("test-graph")
        .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
        .build()
        .expect("test graph"),
    );
    let sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start_session with shadowed preset");

    // The descriptor must resolve to the DIRECTORY source (not embedded) and
    // carry the real directory preset version (99, not 0 and not embedded).
    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    let descriptor = record.descriptor.expect("descriptor present");
    assert_eq!(
        descriptor.preset_version, 99,
        "descriptor must carry the real directory preset version, not 0"
    );
    assert!(
        matches!(descriptor.source, PresetSourceIdentity::Directory { .. }),
        "shadowed directory preset must resolve to Directory source, got {:?}",
        descriptor.source
    );
}

// Important 4: a corrupt/ambiguous v0 status must remain distinct and
// non-replayable — never silently reinterpreted as `running`.
#[tokio::test]
async fn unknown_v0_status_is_non_replayable() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());

    // Seed a v0 row with an unknown status value.
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at)
         VALUES ('sess-v0-unknown', 'ctr_t', 'preset_t', 7, 'bogus_status', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300)",
    )
    .execute(&*pool)
    .await
    .expect("seed v0 row with unknown status");

    // load_run must surface the unknown v0 status as an error (non-replayable),
    // not silently reinterpret it as Running.
    let err = storage
        .load_run(&SessionId("sess-v0-unknown".to_string()))
        .await
        .expect_err("unknown v0 status must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error for unknown v0 status, got {err:?}"
    );

    // The row must be preserved (not rewritten).
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT status FROM orchestration_sessions WHERE session_id = 'sess-v0-unknown'",
    )
    .fetch_one(&*pool)
    .await
    .expect("read raw status");
    assert_eq!(
        raw.as_deref(),
        Some("bogus_status"),
        "row preserved verbatim"
    );
}

// Important 4: a negative/out-of-range integer revision on a v0 row must
// remain non-replayable — never coerced to zero.
#[tokio::test]
async fn negative_v0_revision_is_non_replayable() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());

    // Seed a v0 row with a negative state_revision.
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision)
         VALUES ('sess-v0-negrev', 'ctr_t', 'preset_t', 7, 'running', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 0, -5)",
    )
    .execute(&*pool)
    .await
    .expect("seed v0 row with negative revision");

    // load_run must surface the negative revision as an error (non-replayable),
    // not coerce it to zero.
    let err = storage
        .load_run(&SessionId("sess-v0-negrev".to_string()))
        .await
        .expect_err("negative v0 revision must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error for negative revision, got {err:?}"
    );

    // The row must be preserved (not rewritten).
    let raw: i64 = sqlx::query_scalar(
        "SELECT state_revision FROM orchestration_sessions WHERE session_id = 'sess-v0-negrev'",
    )
    .fetch_one(&*pool)
    .await
    .expect("read raw revision");
    assert_eq!(raw, -5, "row preserved verbatim");
}

// ---------------------------------------------------------------------------
// Fix round 3 (task re-review): regression tests for each fixed contract
// ---------------------------------------------------------------------------

/// A graph-flow task that completes immediately (returns `NextAction::End`).
struct EndTask;
#[async_trait::async_trait]
impl graph_flow::Task for EndTask {
    fn id(&self) -> &'static str {
        "end_task"
    }
    async fn run(&self, _ctx: graph_flow::Context) -> graph_flow::Result<graph_flow::TaskResult> {
        Ok(graph_flow::TaskResult::new(
            None,
            graph_flow::NextAction::End,
        ))
    }
}

/// A graph-flow task that advances to the next task and records a context
/// marker (used to verify root position/context restoration on failure).
struct ContinueTask;
#[async_trait::async_trait]
impl graph_flow::Task for ContinueTask {
    fn id(&self) -> &'static str {
        "continue_task"
    }
    async fn run(&self, ctx: graph_flow::Context) -> graph_flow::Result<graph_flow::TaskResult> {
        ctx.set("advanced", true).unwrap();
        Ok(graph_flow::TaskResult::new(
            None,
            graph_flow::NextAction::Continue,
        ))
    }
}

/// Build a fresh engine over a temp SQLite pool (helper for round-3 tests).
fn fresh_engine(
    pool: Arc<sqlx::SqlitePool>,
) -> (
    Arc<SqliteSessionStorage>,
    nexus_orchestration::GraphFlowEngine,
) {
    let storage = Arc::new(SqliteSessionStorage::new(pool));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_orchestration::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );
    (storage, engine)
}

// Important 1: an InnerGraphTask child actually runs to completion, and the
// parent's commit_transition succeeds (the child checkpoint revision is
// synchronized from each child transition).
#[tokio::test]
async fn engine_nested_child_runs_to_completion_then_parent_commits() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool);

    // Build a child graph that completes in one step (EndTask).
    let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("inner_graph")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
    );

    // Build a parent graph whose start task is an InnerGraphTask that spawns
    // the child and polls it to completion.
    let parent_graph = Arc::new(graph_flow::Graph::new("parent_graph"));
    let inner_task = nexus_orchestration::tasks::InnerGraphTask::new(
        Arc::new(engine.clone()),
        inner_graph.clone(),
        "parent_state",
        "_session_id",
        None,
    );
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(inner_task))
        .add_task(Arc::new(EndTask))
        .add_edge("parent_state", "end_task")
        .build()
        .expect("test graph build"),
    );

    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent session");

    // Run the parent step — the InnerGraphTask spawns the child, polls it to
    // completion, and the parent's commit_transition must succeed (the child
    // checkpoint revision is synchronized).
    let outcome = engine.run_step(&parent_sid).await.expect("run_step parent");
    // The parent graph has no outgoing edge from the InnerGraphTask, so it
    // returns Paused.
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::Paused { .. }
        ),
        "expected Paused, got {outcome:?}"
    );

    // The parent's commit_transition must have succeeded (revision advanced).
    let parent_record = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("parent present");
    assert_eq!(
        parent_record.state_revision, 3,
        "in-flight marker and parent transition both advance the revision"
    );

    // The child must be terminal (completed) and reconstructible.
    let children = storage
        .load_children(&parent_sid)
        .await
        .expect("load_children");
    assert_eq!(children.len(), 1, "one child persisted");
    assert_eq!(
        children[0].status,
        SessionStatus::Completed,
        "child completed"
    );
    assert!(
        children[0].state_revision >= 2,
        "child revision advanced past the initial 1"
    );

    engine
        .shared_state()
        .hydrate_children(&parent_sid)
        .await
        .expect("rehydrate after parent advanced");
    assert!(
        engine
            .shared_state()
            .attach_existing_child_session_internal(&parent_sid.0, inner_graph)
            .await
            .expect("attach is replayable-safe")
            .is_none(),
        "a later entry into the same inner graph must not reuse the retired terminal child"
    );
}

/// P3 T1 rereview-2 P1 (A7 rule 2): a parseable but tampered child
/// descriptor is non-replayable. Both hydration and the reattachment path
/// must fail closed instead of attaching the row to the root-built graph or
/// falling back to `spawn_child_session` (which would mint a fresh child id
/// and replay work).
#[tokio::test]
async fn tampered_child_descriptor_is_non_replayable_and_never_reattached() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());

    let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("inner_graph")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
    );

    let parent_graph = Arc::new(graph_flow::Graph::new("parent_graph"));
    let inner_task = nexus_orchestration::tasks::InnerGraphTask::new(
        Arc::new(engine.clone()),
        inner_graph.clone(),
        "parent_state",
        "_session_id",
        None,
    );
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(inner_task))
        .build()
        .expect("test graph build"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent session");

    // Spawn a live child through the production admission path (v1 row).
    let child_sid = engine
        .shared_state()
        .spawn_child_session_internal(nexus_orchestration::engine::ChildSessionParams {
            parent_session_id: parent_sid.0.clone(),
            inner_graph: inner_graph.clone(),
            initial_context: graph_flow::Context::new(),
        })
        .await
        .expect("spawn child");

    // Tamper: the descriptor stays parseable, but the frozen identity no
    // longer matches the trusted root.
    let descriptor: Vec<u8> = sqlx::query_scalar(
        "SELECT run_descriptor_json FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(&child_sid.0)
    .fetch_one(&*pool)
    .await
    .expect("child descriptor");
    let mut value: serde_json::Value =
        serde_json::from_slice(&descriptor).expect("descriptor json");
    value["preset_version"] = serde_json::json!(999);
    let tampered = serde_json::to_vec(&value).expect("tampered descriptor");
    sqlx::query("UPDATE orchestration_sessions SET run_descriptor_json = ? WHERE session_id = ?")
        .bind(tampered)
        .bind(&child_sid.0)
        .execute(&*pool)
        .await
        .expect("tamper child descriptor");

    // Reattachment must fail closed — never `Ok(None)` (which the caller
    // would treat as "no child" and spawn a fresh one).
    let err = engine
        .shared_state()
        .attach_existing_child_session_internal(&parent_sid.0, inner_graph.clone())
        .await
        .expect_err("tampered child must not be attached");
    assert!(
        err.to_string().contains("non-replayable"),
        "attachment refusal must be non-replayable, got {err}"
    );

    // Recovery hydration must also fail the owned closure.
    let err = engine
        .shared_state()
        .hydrate_children(&parent_sid)
        .await
        .expect_err("tampered child must fail hydration");
    assert!(
        err.to_string().contains("non-replayable"),
        "hydration refusal must be non-replayable, got {err}"
    );

    // Tampering never mints a second child row.
    let children = storage
        .load_children(&parent_sid)
        .await
        .expect("load children");
    assert_eq!(
        children.len(),
        1,
        "a tampered child must never be replaced by a freshly minted one"
    );
}

/// P3 T1 rereview-3 P1 (A7 rule 2): a parseable child whose `graph_name`
/// names an inner graph the frozen preset does not define is non-replayable.
/// Recovery must refuse to reconstruct the root instead of letting
/// `InnerGraphTask` miss the reattachment lookup and spawn a fresh child.
#[tokio::test]
async fn tampered_child_graph_name_is_non_replayable_on_recovery() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());

    let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("inner_graph")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
    );
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(nexus_orchestration::tasks::InnerGraphTask::new(
        Arc::new(engine.clone()),
        inner_graph.clone(),
        "parent_state",
        "_session_id",
        None,
    )))
        .build()
        .expect("test graph"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent session");
    let child_sid = engine
        .shared_state()
        .spawn_child_session_internal(nexus_orchestration::engine::ChildSessionParams {
            parent_session_id: parent_sid.0.clone(),
            inner_graph: inner_graph.clone(),
            initial_context: graph_flow::Context::new(),
        })
        .await
        .expect("spawn child");

    // Tamper only the graph_name: descriptor identity still matches.
    let descriptor: Vec<u8> = sqlx::query_scalar(
        "SELECT run_descriptor_json FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(&child_sid.0)
    .fetch_one(&*pool)
    .await
    .expect("child descriptor");
    let mut value: serde_json::Value =
        serde_json::from_slice(&descriptor).expect("descriptor json");
    value["graph_name"] = serde_json::json!("bogus_inner_graph");
    let tampered = serde_json::to_vec(&value).expect("tampered descriptor");
    sqlx::query("UPDATE orchestration_sessions SET run_descriptor_json = ? WHERE session_id = ?")
        .bind(tampered)
        .bind(&child_sid.0)
        .execute(&*pool)
        .await
        .expect("tamper graph_name");

    // Restart shape: a fresh engine over the same pool must refuse to
    // reconstruct the root runner over the tampered descendant.
    let (_storage_b, engine_b) = fresh_engine(pool.clone());
    let err = engine_b
        .ensure_recovered_runner_inner(&parent_sid)
        .await
        .expect_err("tampered graph_name must block reconstruction");
    assert!(
        err.to_string().contains("unknown inner graph"),
        "refusal must name the unknown inner graph, got {err}"
    );

    let children = storage
        .load_children(&parent_sid)
        .await
        .expect("load children");
    assert_eq!(children.len(), 1, "no replacement child may be minted");
}

/// P3 T1 rereview-3 P1 (A7 rule 2): a v1 child under a parent whose frozen
/// descriptor is absent is non-replayable — the v0-parent skip must not
/// exempt it from identity validation.
#[tokio::test]
async fn v1_child_under_parent_without_descriptor_is_non_replayable() {
    let (pool, _db) = fresh_pool().await;
    let (_storage, engine) = fresh_engine(pool.clone());

    let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("inner_graph")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
    );
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(nexus_orchestration::tasks::InnerGraphTask::new(
        Arc::new(engine.clone()),
        inner_graph.clone(),
        "parent_state",
        "_session_id",
        None,
    )))
        .build()
        .expect("test graph"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent session");
    let child_sid = engine
        .shared_state()
        .spawn_child_session_internal(nexus_orchestration::engine::ChildSessionParams {
            parent_session_id: parent_sid.0.clone(),
            inner_graph: inner_graph.clone(),
            initial_context: graph_flow::Context::new(),
        })
        .await
        .expect("spawn child");

    // Simulate a legacy-shaped parent (v0 execution version, no descriptor)
    // owning a v1 child: the child's identity cannot be verified.
    sqlx::query(
        "UPDATE orchestration_sessions
         SET execution_version = 0, run_descriptor_json = NULL
         WHERE session_id = ?",
    )
    .bind(&parent_sid.0)
    .execute(&*pool)
    .await
    .expect("degrade parent identity");

    let err = engine
        .shared_state()
        .hydrate_children(&parent_sid)
        .await
        .expect_err("v1 child under a descriptor-less parent must be non-replayable");
    assert!(
        err.to_string().contains("non-replayable"),
        "refusal must be non-replayable, got {err}"
    );
    let _ = child_sid;
}

// Important 1: a restarted parent hydrates its children map from persisted
// child rows so its next commit_transition submits the correct child revision.
#[tokio::test]
async fn restart_hydrates_child_checkpoints() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let parent_sid;
    let child_sid;

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (first)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (first)");
        let (storage, engine) = fresh_engine(Arc::new(pool));

        let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("inner_graph")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
        );
        let parent_graph = Arc::new(graph_flow::Graph::new("parent_graph"));
        let inner_task = nexus_orchestration::tasks::InnerGraphTask::new(
            Arc::new(engine.clone()),
            inner_graph,
            "parent_state",
            "_session_id",
            None,
        );
        let parent_graph = Arc::new(
        graph_flow::GraphBuilder::new("parent_graph")
            .add_task(Arc::new(inner_task))
            .build()
            .expect("test graph build"),
        );

        parent_sid = engine
            .start_session("novel-writing", parent_graph)
            .await
            .expect("start parent session");
        engine.run_step(&parent_sid).await.expect("run_step parent");

        let children = storage
            .load_children(&parent_sid)
            .await
            .expect("load_children");
        assert_eq!(children.len(), 1);
        child_sid = children[0].session_id.clone();
    } // pool drops — simulates daemon shutdown

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (second)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (second)");
        let (_storage, engine) = fresh_engine(Arc::new(pool));

        // Recover the parent session — this must hydrate the children map.
        let summary = nexus_orchestration::engine::SessionSummary {
            session_id: parent_sid.clone(),
            creator_id: String::new(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Paused,
            current_task_id: Some("parent_state".to_string()),
        };
        engine.recover_sessions(vec![summary]).await;

        // The children map must be hydrated with the child at its current
        // persisted revision.
        let shared = engine.shared_state();
        let children_map = shared.children.read().await;
        let parent_children = children_map
            .get(&parent_sid.0)
            .expect("children hydrated for recovered parent");
        assert_eq!(parent_children.len(), 1, "one child hydrated");
        assert_eq!(parent_children[0].session.id, child_sid.0);
        assert_eq!(
            parent_children[0].status,
            SessionStatus::Completed,
            "child status hydrated"
        );
        assert!(
            parent_children[0].state_revision >= 2,
            "child revision hydrated from persisted row"
        );
    }
}

// Important 2: a failed child CAS (stale child revision) must not leave a
// partially advanced root checkpoint — current_task_id/context_json are
// restored to their pre-step values.
#[tokio::test]
async fn failed_child_cas_restores_root_position() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());

    // Build a parent graph: task "continue_task" → task "end_task".
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(ContinueTask))
        .add_task(Arc::new(EndTask))
        .add_edge("continue_task", "end_task")
        .build()
        .expect("test graph"),
    );

    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent session");

    // Manually insert a child checkpoint into the parent's children map at
    // revision 1 (simulating a child the parent believes is at revision 1).
    let child_id = "parent:child:stale";
    let child_session = Session::new_from_task(child_id.to_string(), "child_task");
    let child_cp = ChildCheckpoint {
        session: child_session,
        status: SessionStatus::Running,
        state: RunStateV1::default(),
        state_revision: 1,
        graph_name: Some("inner_graph".to_string()),
    };
    engine
        .shared_state()
        .children
        .write()
        .await
        .insert(parent_sid.0.clone(), vec![child_cp]);

    // Create the child row in DB at revision 1, then advance it to 2
    // (simulating a concurrent child transition the parent does not know about).
    let child_descriptor = test_descriptor(&parent_sid.0);
    let child_root = root_session(child_id, "child_task");
    let child_checkpoint = RunCheckpoint {
        root: &child_root,
        children: &[],
    };
    storage
        .start_run(
            &SessionId(child_id.to_string()),
            &child_descriptor,
            child_checkpoint,
            &RunStateV1::default(),
        )
        .await
        .expect("start child run");
    sqlx::query("UPDATE orchestration_sessions SET state_revision = 2 WHERE session_id = ?")
        .bind(child_id)
        .execute(&*pool)
        .await
        .expect("advance child revision");

    // Capture the pre-step root position/context.
    let pre_step = storage
        .get(&parent_sid.0)
        .await
        .expect("get pre-step root")
        .expect("root present");
    let pre_task = pre_step.current_task_id.clone();
    let pre_ctx = serde_json::to_vec(&pre_step.context).expect("serialize pre-step context");

    // Run the parent step — task "continue_task" runs, advances to
    // "end_task", returns Paused. commit_transition fails (stale child
    // revision), and the root must be restored to its pre-step position.
    let err = engine
        .run_step(&parent_sid)
        .await
        .expect_err("stale child revision must fail the parent step");
    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::RevisionMismatch { .. }
        ),
        "expected RevisionMismatch, got {err:?}"
    );

    // The root position/context must be restored to pre-step.
    let after = storage
        .get(&parent_sid.0)
        .await
        .expect("get post-failure root")
        .expect("root present");
    assert_eq!(
        after.current_task_id, pre_task,
        "root current_task_id must not advance on failed child CAS"
    );
    let after_ctx = serde_json::to_vec(&after.context).expect("serialize post-failure context");
    assert_eq!(
        pre_ctx, after_ctx,
        "root context_json must not advance on failed child CAS"
    );
    // The "advanced" marker set by ContinueTask must be gone (restored).
    let advanced: Option<bool> = after.context.get("advanced");
    assert_eq!(
        advanced, None,
        "context marker set during the failed step must be restored away"
    );
}

// Minor 1: child IDs are collision-resistant — two child admissions in the
// same millisecond must not collide with start_run's existing-row rejection.
#[tokio::test]
async fn child_ids_are_collision_resistant() {
    let (pool, _db) = fresh_pool().await;
    let (_storage, engine) = fresh_engine(pool);

    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
        .build()
        .expect("test graph"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent session");

    // Spawn two children rapidly (same millisecond) — their IDs must differ.
    let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("inner_graph")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
    );
    let params1 = nexus_orchestration::ChildSessionParams {
        parent_session_id: parent_sid.0.clone(),
        inner_graph: inner_graph.clone(),
        initial_context: graph_flow::Context::new(),
    };
    let params2 = nexus_orchestration::ChildSessionParams {
        parent_session_id: parent_sid.0.clone(),
        inner_graph: inner_graph.clone(),
        initial_context: graph_flow::Context::new(),
    };
    let child1 = engine
        .spawn_child_session(params1)
        .await
        .expect("spawn child 1");
    let child2 = engine
        .spawn_child_session(params2)
        .await
        .expect("spawn child 2");
    assert_ne!(
        child1.0, child2.0,
        "child IDs must be collision-resistant (two admissions in one millisecond)"
    );
}

// ---------------------------------------------------------------------------
// Fix round 4 (task re-review): regression tests for each fixed contract
// ---------------------------------------------------------------------------

/// A graph-flow task that sets the external-effect marker (Important 3) and
/// advances to the next task. Simulates a step that may have performed an
/// external effect before a failed commit.
struct EffectTask;
#[async_trait::async_trait]
impl graph_flow::Task for EffectTask {
    fn id(&self) -> &'static str {
        "effect_task"
    }
    async fn run(&self, ctx: graph_flow::Context) -> graph_flow::Result<graph_flow::TaskResult> {
        ctx.set(nexus_orchestration::engine::EXTERNAL_EFFECT_MARKER, true).unwrap();
        Ok(graph_flow::TaskResult::new(
            None,
            graph_flow::NextAction::Continue,
        ))
    }
}

/// A graph-flow task that does NOT set the external-effect marker and advances
/// (deterministic step, Important 3).
struct DeterministicContinueTask;
#[async_trait::async_trait]
impl graph_flow::Task for DeterministicContinueTask {
    fn id(&self) -> &'static str {
        "deterministic_continue_task"
    }
    async fn run(&self, ctx: graph_flow::Context) -> graph_flow::Result<graph_flow::TaskResult> {
        ctx.set("advanced", true).unwrap();
        Ok(graph_flow::TaskResult::new(
            None,
            graph_flow::NextAction::Continue,
        ))
    }
}

// Important 1: recovery propagates the error when load_children fails
// (corrupt/unsupported child) — the parent is marked non-replayable, never
// reconstructed over an incomplete children map, and no stale checkpoint
// lingers.
#[tokio::test]
async fn corrupt_child_is_non_replayable_during_recovery() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());
    let parent_sid = SessionId("par-corrupt-child".to_string());

    // Start a parent run.
    let descriptor = test_descriptor(&parent_sid.0);
    let root = root_session(&parent_sid.0, "parent_state");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&parent_sid, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start parent");

    // Seed a child row whose run_descriptor_json is corrupt (invalid JSON).
    // load_children must fail on it, and hydrate_children must propagate.
    let child_sid = "par-corrupt-child:child:1";
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, parent_session_id,
             current_task_id, status, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES (?, 'ctr', 'test-preset', 3, ?, 'child_task', 'running',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 1, 1, '{\"wait\":null,\"step_in_flight\":null,\"in_flight\":null,\"failure\":null,\"cancel_requested\":false}',
                 'NOT VALID JSON{')",
    )
    .bind(child_sid)
    .bind(&parent_sid.0)
    .execute(&*pool)
    .await
    .expect("seed corrupt child row");

    // hydrate_children must return Err (corrupt child is non-replayable),
    // not silently continue with an empty children map (Important 1).
    let err = engine
        .shared_state()
        .hydrate_children(&parent_sid)
        .await
        .expect_err("corrupt child must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error for corrupt child, got {err:?}"
    );

    // The children map must NOT be populated (no stale checkpoint).
    let shared = engine.shared_state();
    let map = shared.children.read().await;
    assert!(
        !map.contains_key(&parent_sid.0),
        "corrupt child must not populate the children map"
    );
    drop(map);

    // Recovery must skip runner reconstruction for this parent (non-replayable)
    // and there must be no runner.
    let summary = nexus_orchestration::engine::SessionSummary {
        session_id: parent_sid.clone(),
        creator_id: String::new(),
        preset_id: "test-preset".to_string(),
        status: SessionStatus::Running,
        current_task_id: Some("parent_state".to_string()),
    };
    engine.recover_sessions(vec![summary]).await;
    assert!(
        !engine.has_runner(&parent_sid).await,
        "corrupt child parent must be non-replayable (no runner)"
    );
}

// Important 1: a child whose session snapshot is missing from storage is also
// non-replayable during recovery (hydrate_children propagates).
#[tokio::test]
async fn child_missing_session_snapshot_is_non_replayable() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());
    let parent_sid = SessionId("par-missing-child".to_string());

    let descriptor = test_descriptor(&parent_sid.0);
    let root = root_session(&parent_sid.0, "parent_state");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&parent_sid, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start parent");

    // Seed a VALID child row but do NOT create its session row in storage.
    let child_sid = "par-missing-child:child:1";
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, parent_session_id,
             current_task_id, status, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES (?, 'ctr', 'test-preset', 3, ?, 'child_task', 'running',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 1, 1,
                 '{\"wait\":null,\"step_in_flight\":null,\"in_flight\":null,\"failure\":null,\"cancel_requested\":false}',
                 ?)",
    )
    .bind(child_sid)
    .bind(&parent_sid.0)
    .bind(
        serde_json::to_vec(&test_descriptor(child_sid))
            .expect("serialize child descriptor"),
    )
    .execute(&*pool)
    .await
    .expect("seed child row");

    // hydrate_children must fail because the child's session row is missing.
    let err = engine
        .shared_state()
        .hydrate_children(&parent_sid)
        .await
        .expect_err("missing child session must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error for missing child session, got {err:?}"
    );
}

/// A graph-flow task that bumps the parent's root `state_revision` mid-step to
/// simulate a concurrent transition that wins before the outer
/// `commit_transition` (used to prove the restore is revision-fenced).
struct ConcurrentBumpRootTask {
    pool: Arc<sqlx::SqlitePool>,
    parent_sid: String,
}
#[async_trait::async_trait]
impl graph_flow::Task for ConcurrentBumpRootTask {
    fn id(&self) -> &'static str {
        "concurrent_bump_task"
    }
    async fn run(&self, _ctx: graph_flow::Context) -> graph_flow::Result<graph_flow::TaskResult> {
        sqlx::query(
            "UPDATE orchestration_sessions SET state_revision = state_revision + 1,
                    current_task_id = 'deterministic_continue_task'
             WHERE session_id = ?",
        )
        .bind(&self.parent_sid)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            graph_flow::GraphError::TaskExecutionFailed(format!("concurrent bump failed: {e}"))
        })?;
        Ok(graph_flow::TaskResult::new(
            None,
            graph_flow::NextAction::Continue,
        ))
    }
}

// Important 2: failed-child-CAS restore is revision-fenced — a concurrent
// root transition (revision advanced mid-step) is never overwritten by the
// older pre-step position.
#[tokio::test]
async fn failed_cas_restore_is_revision_fenced() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());

    // Parent graph: concurrent-bump → deterministic-continue → end.
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(ConcurrentBumpRootTask {
        pool: pool.clone(),
        parent_sid: "placeholder".to_string(),
    }))
        .add_task(Arc::new(DeterministicContinueTask))
        .add_task(Arc::new(EndTask))
        .add_edge("concurrent_bump_task", "deterministic_continue_task")
        .add_edge("deterministic_continue_task", "end_task")
        .build()
        .expect("test graph"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent");

    // Insert a stale child checkpoint into the parent's children map at
    // revision 1.
    let child_id = "parent:child:fence";
    let child_session = Session::new_from_task(child_id.to_string(), "child_task");
    let child_cp = ChildCheckpoint {
        session: child_session,
        status: SessionStatus::Running,
        state: RunStateV1::default(),
        state_revision: 1,
        graph_name: Some("inner_graph".to_string()),
    };
    engine
        .shared_state()
        .children
        .write()
        .await
        .insert(parent_sid.0.clone(), vec![child_cp]);

    // Create the child row, then advance the child revision to 2 so the
    // child CAS fails (stale) below.
    let child_descriptor = test_descriptor(&parent_sid.0);
    let child_root = root_session(child_id, "child_task");
    storage
        .start_run(
            &SessionId(child_id.to_string()),
            &child_descriptor,
            RunCheckpoint {
                root: &child_root,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start child");
    sqlx::query("UPDATE orchestration_sessions SET state_revision = 2 WHERE session_id = ?")
        .bind(child_id)
        .execute(&*pool)
        .await
        .expect("advance child revision");

    // The engine step: it will first capture expected_revision = 1, run the
    // concurrent-bump task which advances the root to revision 2, then run
    // deterministic-continue, then commit_transition(expected=1) fails with
    // RevisionMismatch. The restore must be SKIPPED because the persisted
    // revision (2) no longer matches the expected (1) — proving the restore
    // is revision-fenced (F2).
    let parent_graph2 = Arc::new(
        graph_flow::GraphBuilder::new("parent_graph")
            .add_task(Arc::new(ConcurrentBumpRootTask {
                pool: pool.clone(),
                parent_sid: parent_sid.0.clone(),
            }))
            .add_task(Arc::new(DeterministicContinueTask))
            .add_task(Arc::new(EndTask))
            .add_edge("concurrent_bump_task", "deterministic_continue_task")
            .add_edge("deterministic_continue_task", "end_task")
            .build()
            .expect("test graph build"),
    );
    let shared_state = engine.shared_state();
    let stored_runner = shared_state.runners.read().await;
    let _runner = stored_runner
        .get(&parent_sid.0)
        .expect("runner present")
        .clone();
    drop(stored_runner);
    engine.shared_state().runners.write().await.insert(
        parent_sid.0.clone(),
        std::sync::Arc::new(graph_flow::FlowRunner::new(
            parent_graph2,
            storage.clone() as Arc<dyn SessionStorage>,
        )),
    );

    // Capture the pre-step root position at the pre-step task.
    let pre_step_root = storage
        .get(&parent_sid.0)
        .await
        .expect("get pre-step root")
        .expect("root present");
    let pre_task = pre_step_root.current_task_id;

    // Run the parent step. The concurrent bump advances the root revision, so
    // the outer commit fails with RevisionMismatch AND, because the persisted
    // revision no longer equals expected, the restore is skipped.
    let err = engine
        .run_step(&parent_sid)
        .await
        .expect_err("concurrent revision bump must fail the parent step");
    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::RevisionMismatch { .. }
        ),
        "expected RevisionMismatch, got {err:?}"
    );

    // The root must NOT be restored to the pre-step (old) position, proving
    // the restore is revision-fenced.
    let after = storage
        .get(&parent_sid.0)
        .await
        .expect("get post-failure root")
        .expect("root present");
    let root_record = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("root present");
    assert_eq!(
        root_record.state_revision, 3,
        "the marker and concurrent bump must both survive the failed step"
    );
    assert_ne!(
        after.current_task_id, pre_task,
        "revision-fenced restore must NOT overwrite the concurrent root position with the older pre-step position"
    );
}

// Important 3: a failed post-effect commit persists an interrupted
// disposition (never a blind rewind), and the interrupted write is itself
// revision-fenced.
#[tokio::test]
async fn post_effect_failure_persists_interrupted() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());

    // Parent graph whose start is an EffectTask (sets the external-effect
    // marker) → end.
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(EffectTask))
        .add_task(Arc::new(EndTask))
        .add_edge("effect_task", "end_task")
        .build()
        .expect("test graph"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent");

    // Stale child so the post-effect commit fails.
    let child_id = "parent:child:interrupt";
    let child_session = Session::new_from_task(child_id.to_string(), "child_task");
    let child_cp = ChildCheckpoint {
        session: child_session,
        status: SessionStatus::Running,
        state: RunStateV1::default(),
        state_revision: 1,
        graph_name: Some("inner_graph".to_string()),
    };
    engine
        .shared_state()
        .children
        .write()
        .await
        .insert(parent_sid.0.clone(), vec![child_cp]);
    let child_descriptor = test_descriptor(&parent_sid.0);
    let child_root = root_session(child_id, "child_task");
    storage
        .start_run(
            &SessionId(child_id.to_string()),
            &child_descriptor,
            RunCheckpoint {
                root: &child_root,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start child");
    sqlx::query("UPDATE orchestration_sessions SET state_revision = 2 WHERE session_id = ?")
        .bind(child_id)
        .execute(&*pool)
        .await
        .expect("advance child revision");

    let pre_task = storage
        .get(&parent_sid.0)
        .await
        .expect("get pre-step root")
        .expect("root present")
        .current_task_id;

    // Run the parent step: EffectTask executes an external effect, then the
    // commit fails. The engine must persist an interrupted disposition, NOT
    // rewind to the pre-step boundary.
    let err = engine
        .run_step(&parent_sid)
        .await
        .expect_err("post-effect failure must surface the commit error");
    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::RevisionMismatch { .. }
        ),
        "expected RevisionMismatch, got {err:?}"
    );

    // The root must now be terminal Interrupted with an in-flight marker.
    let record = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("parent present");
    assert_eq!(
        record.status,
        SessionStatus::Interrupted,
        "post-effect failure must persist interrupted, not rewind"
    );
    let state = record.state.expect("interrupted state present");
    assert!(
        state.step_in_flight.is_some(),
        "interrupted disposition must carry a step_in_flight marker"
    );

    // The root must NOT be rewound to the pre-step boundary: the interrupted
    // disposition keeps the post-step position (here "end_task"), not the
    // pre-step "effect_task" safe boundary (Import 3 — no rewind).
    let after = storage
        .get(&parent_sid.0)
        .await
        .expect("get post-failure root")
        .expect("root present");
    assert_ne!(
        after.current_task_id, pre_task,
        "interrupted run keeps the step position (no rewind to a seemingly safe boundary)"
    );
}

// Important 4: a deterministic step (no external effect) with a failed commit
// is CAS-fenced restored to the pre-step position (acceptable rewind).
#[tokio::test]
async fn deterministic_failure_restores_pre_step_position() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());

    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(DeterministicContinueTask))
        .add_task(Arc::new(EndTask))
        .add_edge("deterministic_continue_task", "end_task")
        .build()
        .expect("test graph"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent");

    let child_id = "parent:child:det";
    let child_session = Session::new_from_task(child_id.to_string(), "child_task");
    let child_cp = ChildCheckpoint {
        session: child_session,
        status: SessionStatus::Running,
        state: RunStateV1::default(),
        state_revision: 1,
        graph_name: Some("inner_graph".to_string()),
    };
    engine
        .shared_state()
        .children
        .write()
        .await
        .insert(parent_sid.0.clone(), vec![child_cp]);
    let child_descriptor = test_descriptor(&parent_sid.0);
    let child_root = root_session(child_id, "child_task");
    storage
        .start_run(
            &SessionId(child_id.to_string()),
            &child_descriptor,
            RunCheckpoint {
                root: &child_root,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start child");
    sqlx::query("UPDATE orchestration_sessions SET state_revision = 2 WHERE session_id = ?")
        .bind(child_id)
        .execute(&*pool)
        .await
        .expect("advance child revision");

    let pre_task = storage
        .get(&parent_sid.0)
        .await
        .expect("get pre-step root")
        .expect("root present")
        .current_task_id;
    let pre_ctx = serde_json::to_vec(
        &storage
            .get(&parent_sid.0)
            .await
            .expect("get pre-step root")
            .expect("root present")
            .context,
    )
    .expect("serialize pre-step context");

    let err = engine
        .run_step(&parent_sid)
        .await
        .expect_err("stale child must fail the deterministic step");
    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::RevisionMismatch { .. }
        ),
        "expected RevisionMismatch, got {err:?}"
    );

    // Deterministic (no external effect): the root is restored to the
    // pre-step position (root revision unchanged → CAS-fenced restore).
    let after = storage
        .get(&parent_sid.0)
        .await
        .expect("get post-failure root")
        .expect("root present");
    assert_eq!(
        after.current_task_id, pre_task,
        "deterministic step restored to pre-step position"
    );
    let after_ctx = serde_json::to_vec(&after.context).expect("serialize post-failure context");
    assert_eq!(pre_ctx, after_ctx, "deterministic step context restored");
    let advanced: Option<bool> = after.context.get("advanced");
    assert_eq!(
        advanced, None,
        "context marker set during the failed deterministic step restored away"
    );
}

// Important 4: a recovered parent drives its existing inner child — the
// InnerGraphTask reattaches the persisted child (no duplicate row, cursor
// intact) and resumes it to completion.
#[tokio::test]
async fn recovered_parent_drives_existing_inner_child() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let parent_sid;
    let existing_child_sid;

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (first)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (first)");
        let (_storage, engine) = fresh_engine(Arc::new(pool));

        let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("ig")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
        );
        let parent_graph = Arc::new(graph_flow::Graph::new("parent_graph"));
        let inner_task = nexus_orchestration::tasks::InnerGraphTask::new(
            Arc::new(engine.clone()),
            inner_graph,
            "parent_state",
            "_session_id",
            None,
        );
        let parent_graph = Arc::new(
        graph_flow::GraphBuilder::new("parent_graph")
            .add_task(Arc::new(inner_task))
            .build()
            .expect("test graph build"),
        );

        parent_sid = engine
            .start_session("novel-writing", parent_graph)
            .await
            .expect("start parent");

        // Spawn the child and leave it Running (non-terminal) so recovery can
        // attach it. Do NOT step it yet.
        let ig = Arc::new(
    graph_flow::GraphBuilder::new("ig")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
        );
        let params = nexus_orchestration::ChildSessionParams {
            parent_session_id: parent_sid.0.clone(),
            inner_graph: ig,
            initial_context: graph_flow::Context::new(),
        };
        existing_child_sid = engine
            .spawn_child_session(params)
            .await
            .expect("spawn child");

        // Leave the child Running (never stepped). The in-memory children map
        // holds it, but we discard the engine below (restart).
    } // pool drops — simulates daemon shutdown

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (second)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (second)");
        let (storage, engine) = fresh_engine(Arc::new(pool));

        // Hydrate the children map from persisted child rows (the recovery
        // step that finding 4 relies on) for the Running parent.
        engine
            .shared_state()
            .hydrate_children(&parent_sid)
            .await
            .expect("hydrate children map");

        // Install the parent's FlowRunner over the custom InnerGraphTask graph
        // (mirroring what a restarting daemon does for this supported parent).
        let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("ig")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
        );
        let parent_graph = Arc::new(graph_flow::Graph::new("parent_graph"));
        let inner_task = nexus_orchestration::tasks::InnerGraphTask::new(
            Arc::new(engine.clone()),
            inner_graph,
            "parent_state",
            "_session_id",
            None,
        );
        let parent_graph = Arc::new(
        graph_flow::GraphBuilder::new("parent_graph")
            .add_task(Arc::new(inner_task))
            .build()
            .expect("test graph build"),
        );
        let shared_state = engine.shared_state();
        shared_state.runners.write().await.insert(
            parent_sid.0.clone(),
            std::sync::Arc::new(graph_flow::FlowRunner::new(
                parent_graph,
                storage.clone() as Arc<dyn SessionStorage>,
            )),
        );

        // Run the parent step — the InnerGraphTask must REATTACH the existing
        // Running child (not spawn a duplicate) and drive it to completion.
        let outcome = engine
            .run_step(&parent_sid)
            .await
            .expect("run_step parent drives existing child");
        assert!(
            matches!(
                outcome,
                nexus_orchestration::engine::StepOutcome::Paused { .. }
            ),
            "expected Paused, got {outcome:?}"
        );

        // Exactly ONE child row must exist, and it must be the SAME child we
        // spawned before restart (no duplicate row created).
        let children = storage
            .load_children(&parent_sid)
            .await
            .expect("load_children");
        assert_eq!(children.len(), 1, "no duplicate child row");
        assert_eq!(
            children[0].session_id.0, existing_child_sid.0,
            "recovered parent must drive the existing child, not a new one"
        );
        assert_eq!(
            children[0].status,
            SessionStatus::Completed,
            "existing child driven to completion"
        );
    }
}

// Important 5: directory-preset frozen-source reconstruction — a run admitted
// from a directory preset reconstructs against the persisted root and version
// (not the current embedded bytes), and a changed source is
// reconstruction_unavailable (non-replayable).
#[tokio::test]
async fn directory_preset_frozen_source_reconstruction() {
    use std::fs;

    let db = tempfile::NamedTempFile::new().unwrap();
    let parent_sid;
    let nexus_home_dir = tempfile::tempdir().unwrap();
    let bundle_dir = nexus_home_dir.path().join("presets").join("dir-preset");

    // Build the directory preset bundle.
    fs::create_dir_all(&bundle_dir).unwrap();
    let initial_yaml = r#"
preset:
  id: dir-preset
  version: 11
  kind: creator
  description: "directory frozen source"
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"#;
    fs::write(bundle_dir.join("preset.yaml"), initial_yaml).unwrap();

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (first)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (first)");
        let storage = Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
        let storage_arc: Arc<dyn SessionStorage> = storage.clone();
        let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage_arc,
            workflow_store,
            caps,
        );
        engine.set_nexus_home(nexus_home_dir.path().to_path_buf());

        // Start a session for the directory preset — the descriptor freezes
        // the Directory source + version 11.
        let graph = Arc::new(
    graph_flow::GraphBuilder::new("test-graph")
        .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
        .build()
        .expect("test graph"),
        );
        parent_sid = engine
            .start_session("dir-preset", graph)
            .await
            .expect("start directory-preset session");

        let record = storage
            .load_run(&parent_sid)
            .await
            .expect("load_run")
            .expect("run present");
        let descriptor = record.descriptor.expect("descriptor present");
        assert_eq!(descriptor.preset_version, 11);
        assert!(
            matches!(descriptor.source, PresetSourceIdentity::Directory { .. }),
            "descriptor must freeze Directory source"
        );
    }

    {
        // Second engine: recover the parent. Frozen-source reconstruction must
        // resolve the directory preset at the persisted root, verify the
        // hash/version, and reconstruct a runner.
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (second)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (second)");
        let storage = Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
        let storage_arc: Arc<dyn SessionStorage> = storage.clone();
        let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage_arc,
            workflow_store,
            caps,
        );
        engine.set_nexus_home(nexus_home_dir.path().to_path_buf());

        let summary = nexus_orchestration::engine::SessionSummary {
            session_id: parent_sid.clone(),
            creator_id: String::new(),
            preset_id: "dir-preset".to_string(),
            status: SessionStatus::Paused,
            current_task_id: Some("a".to_string()),
        };
        // Reset the runner map so reconstruction actually runs.
        engine.shared_state().runners.write().await.clear();
        engine.recover_sessions(vec![summary]).await;

        assert!(
            engine.has_runner(&parent_sid).await,
            "directory preset must reconstruct a runner from the frozen source"
        );
    }

    {
        // Third engine: change the directory preset content (version bump)
        // → the frozen source hash/version no longer matches → reconstruction
        // is unavailable (non-replayable), not a fallback to current bytes.
        let changed_yaml = initial_yaml.replace("version: 11", "version: 99");
        fs::write(bundle_dir.join("preset.yaml"), changed_yaml).unwrap();

        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (third)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (third)");
        let storage = Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
        let storage_arc: Arc<dyn SessionStorage> = storage.clone();
        let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage_arc,
            workflow_store,
            caps,
        );
        engine.set_nexus_home(nexus_home_dir.path().to_path_buf());

        let summary = nexus_orchestration::engine::SessionSummary {
            session_id: parent_sid.clone(),
            creator_id: String::new(),
            preset_id: "dir-preset".to_string(),
            status: SessionStatus::Paused,
            current_task_id: Some("a".to_string()),
        };
        engine.recover_sessions(vec![summary]).await;

        assert!(
            !engine.has_runner(&parent_sid).await,
            "changed directory preset source must be reconstruction_unavailable (non-replayable)"
        );
    }
}

// Important 6: a negative/unknown v1 state_revision is non-replayable —
// never silently coerced to zero.
#[tokio::test]
async fn negative_v1_revision_is_non_replayable() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());

    // Seed a v1 row with a negative state_revision.
    let descriptor = test_descriptor("sess-v1-negrev");
    let descriptor_bytes = serde_json::to_vec(&descriptor).expect("serialize descriptor");
    let state_bytes = serde_json::to_vec(&RunStateV1::default()).expect("serialize state");
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES ('sess-v1-negrev', 'ctr_t', 'test-preset', 3, 'running', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 1, -9, ?, ?)",
    )
    .bind(state_bytes)
    .bind(descriptor_bytes)
    .execute(&*pool)
    .await
    .expect("seed v1 row with negative revision");

    let err = storage
        .load_run(&SessionId("sess-v1-negrev".to_string()))
        .await
        .expect_err("negative v1 revision must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error for negative v1 revision, got {err:?}"
    );

    // The row must be preserved verbatim.
    let raw: i64 = sqlx::query_scalar(
        "SELECT state_revision FROM orchestration_sessions WHERE session_id = 'sess-v1-negrev'",
    )
    .fetch_one(&*pool)
    .await
    .expect("read raw revision");
    assert_eq!(raw, -9, "row preserved verbatim");
}

// Important 6: child terminal-at-expected CAS compares the submitted child
// checkpoint status — a stale Running checkpoint must NOT let a parent commit
// Completed when the child is Failed/Cancelled at that revision.
#[tokio::test]
async fn child_terminal_status_mismatch_rejects_confirmation() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-mismatch".to_string());

    // Start a root run with a child at revision 0.
    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "parent_task");
    let child = child_checkpoint("sess-child-mismatch:child:1", "child_task");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[child],
    };
    storage
        .start_run(&session_id, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start_run with child");

    // Mark the child FAILED at the SAME revision 0. The parent submits a
    // Running checkpoint — confirmation must FAIL (mismatch), never let the
    // parent commit as if the child completed.
    sqlx::query(
        "UPDATE orchestration_sessions SET status = 'failed'
         WHERE session_id = 'sess-child-mismatch:child:1'",
    )
    .execute(&*pool)
    .await
    .expect("mark child failed");

    let root2 = root_session(&session_id.0, "parent_task2");
    let child2 = child_checkpoint("sess-child-mismatch:child:1", "child_task2");
    let checkpoint2 = RunCheckpoint {
        root: &root2,
        children: &[child2],
    };
    let err = storage
        .commit_transition(
            &session_id,
            1,
            checkpoint2,
            SessionStatus::Completed,
            &RunStateV1::default(),
        )
        .await
        .expect_err("terminal child status mismatch must reject the parent's Completed commit");

    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow mismatch error, got {err:?}"
    );

    // The parent must NOT have committed Completed (root unchanged).
    let root_after = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("root present");
    assert_ne!(root_after.status, SessionStatus::Completed);
    assert_eq!(root_after.state_revision, 1, "root not advanced");
}

// ---------------------------------------------------------------------------
// Fix round 5 (task re-review): regression tests for each fixed contract
// ---------------------------------------------------------------------------

// Important 1: a persisted TERMINAL child for the current inner-graph
// position is reattached/consumed, never re-spawned — the parent does not
// replay completed prompt/effect work and does not create a duplicate child.
#[tokio::test]
async fn terminal_child_is_reattached_not_respawned() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let parent_sid;
    let child_sid;

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (first)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (first)");
        let (storage, engine) = fresh_engine(Arc::new(pool));

        let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("ig")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
        );
        let parent_graph = Arc::new(graph_flow::Graph::new("parent_graph"));
        let inner_task = nexus_orchestration::tasks::InnerGraphTask::new(
            Arc::new(engine.clone()),
            inner_graph,
            "parent_state",
            "_session_id",
            None,
        );
        let parent_graph = Arc::new(
        graph_flow::GraphBuilder::new("parent_graph")
            .add_task(Arc::new(inner_task))
            .build()
            .expect("test graph build"),
        );

        parent_sid = engine
            .start_session("novel-writing", parent_graph)
            .await
            .expect("start parent");
        // First run_step spawns the child AND drives it to completion
        // (terminal child checkpoint persisted).
        engine
            .run_step(&parent_sid)
            .await
            .expect("run_step parent completes child");
        let children = storage
            .load_children(&parent_sid)
            .await
            .expect("load_children");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].status, SessionStatus::Completed);
        child_sid = children[0].session_id.clone();
    } // pool drops — simulates a crash AFTER the child checkpoint is durable
      // but BEFORE the parent commits past this inner graph.

    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (second)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (second)");
        let (storage, engine) = fresh_engine(Arc::new(pool));

        // Hydrate the children map (recovery) — the terminal child is present.
        engine
            .shared_state()
            .hydrate_children(&parent_sid)
            .await
            .expect("hydrate children map");

        // Reconstruct the parent runner over the same InnerGraphTask graph.
        let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("ig")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
        );
        let parent_graph = Arc::new(graph_flow::Graph::new("parent_graph"));
        let inner_task = nexus_orchestration::tasks::InnerGraphTask::new(
            Arc::new(engine.clone()),
            inner_graph,
            "parent_state",
            "_session_id",
            None,
        );
        let parent_graph = Arc::new(
        graph_flow::GraphBuilder::new("parent_graph")
            .add_task(Arc::new(inner_task))
            .build()
            .expect("test graph build"),
        );
        let shared = engine.shared_state();
        shared.runners.write().await.insert(
            parent_sid.0.clone(),
            std::sync::Arc::new(graph_flow::FlowRunner::new(
                parent_graph,
                storage.clone() as Arc<dyn SessionStorage>,
            )),
        );

        // Run the parent step: the InnerGraphTask must REATTACH the terminal
        // child (no duplicate spawn, no re-step) and complete.
        let _outcome = engine
            .run_step(&parent_sid)
            .await
            .expect("parent step over terminal reattached child");

        // Exactly ONE child row — the original — no duplicate spawn.
        let children = storage
            .load_children(&parent_sid)
            .await
            .expect("load_children");
        assert_eq!(children.len(), 1, "no duplicate child spawn");
        assert_eq!(
            children[0].session_id.0, child_sid.0,
            "reattaches the persisted terminal child, not a new one"
        );
        assert_eq!(
            children[0].status,
            SessionStatus::Completed,
            "terminal child stays terminal"
        );
        // Marker + terminal commit advanced the child twice; reattachment
        // must not advance it again.
        assert_eq!(
            children[0].state_revision, 3,
            "terminal child checkpoint persisted before parent commit; not re-stepped"
        );
    }
}

// Important 1 + Minor 1: a child whose session snapshot cannot be read is
// non-replayable AND the stale children-map entry is cleared on the error
// (Minor 1: hydrate_children must not leave a stale checkpoint on error).
#[tokio::test]
async fn missing_child_session_clears_stale_children_entry() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());
    let parent_sid = SessionId("par-missing-clear".to_string());

    // Seed a parent run + a VALID child row but NO session row (missing
    // snapshot — the exact failing case from important/missing-child).
    let descriptor = test_descriptor(&parent_sid.0);
    let root = root_session(&parent_sid.0, "parent_state");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&parent_sid, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start parent");
    let child_sid = "par-missing-clear:child:1";
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, parent_session_id,
             current_task_id, status, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES (?, 'ctr', 'test-preset', 3, ?, 'child_task', 'running',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 1, 1,
                 '{\"wait\":null,\"step_in_flight\":null,\"in_flight\":null,\"failure\":null,\"cancel_requested\":false}',
                 ?)",
    )
    .bind(child_sid)
    .bind(&parent_sid.0)
    .bind(serde_json::to_vec(&test_descriptor(child_sid)).expect("child descriptor"))
    .execute(&*pool)
    .await
    .expect("seed child row");

    // Pre-populate a STALE children-map entry (as if a prior checkpoint
    // existed) so we can prove the error clears it (Minor 1).
    let stale_cp = ChildCheckpoint {
        session: Session::new_from_task(child_sid.to_string(), "child_task"),
        status: SessionStatus::Running,
        state: RunStateV1::default(),
        state_revision: 1,
        graph_name: Some("ig".to_string()),
    };
    engine
        .shared_state()
        .children
        .write()
        .await
        .insert(parent_sid.0.clone(), vec![stale_cp]);

    // hydrate_children must fail (missing session snapshot) AND clear the
    // stale entry.
    let err = engine
        .shared_state()
        .hydrate_children(&parent_sid)
        .await
        .expect_err("missing child session must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error, got {err:?}"
    );
    let shared = engine.shared_state();
    let map = shared.children.read().await;
    assert!(
        !map.contains_key(&parent_sid.0),
        "stale children-map entry cleared on hydrate error (Minor 1)"
    );
    drop(map);
}

// Important 2: a parent inner-graph step that drives a child session (which may
// run effects) propagates the child's external-effect marker to the parent, so
// a subsequent parent commit failure is Interrupted — never deterministically
// restored and replayed. We simulate by directly marking the parent context via
// the InnerGraphTask path and then failing the parent commit with a stale child.
#[tokio::test]
async fn child_effect_propagates_to_parent_interrupted_on_failed_commit() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());

    // Parent graph whose start is an InnerGraphTask (drives a child end task).
    let child_graph = Arc::new(
    graph_flow::GraphBuilder::new("child_graph")
        .add_task(Arc::new(EndTask))
        .build()
        .expect("test graph"),
    );
    let parent_graph = Arc::new(graph_flow::Graph::new("parent_graph"));
    let inner_task = nexus_orchestration::tasks::InnerGraphTask::new(
        Arc::new(engine.clone()),
        child_graph,
        "parent_state",
        "_session_id",
        None,
    );
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(inner_task))
        .build()
        .expect("test graph build"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent");

    // Insert a stale child into the parent's children map so the parent
    // commit_transition fails (the child CAS is a stale revision), triggering
    // the classification path.
    let child_id = "parent:child:prop";
    let child_session = Session::new_from_task(child_id.to_string(), "child_task");
    let child_cp = ChildCheckpoint {
        session: child_session,
        status: SessionStatus::Completed, // terminal at stale rev triggers...
        state: RunStateV1::default(),
        state_revision: 1,
        graph_name: Some("child_graph".to_string()),
    };
    engine
        .shared_state()
        .children
        .write()
        .await
        .insert(parent_sid.0.clone(), vec![child_cp]);
    // Seed the child row then advance it so the submitted (rev 1) stale.
    // The child descriptor must carry the REAL inherited identity (same
    // creator/preset/version/source as the parent, naming its parent and
    // inner graph) — a bare test descriptor is refused as non-replayable.
    let child_descriptor: RunDescriptorV1 = {
        let parent_json: Vec<u8> = sqlx::query_scalar(
            "SELECT run_descriptor_json FROM orchestration_sessions WHERE session_id = ?",
        )
        .bind(&parent_sid.0)
        .fetch_one(&*pool)
        .await
        .expect("parent descriptor");
        let mut descriptor: RunDescriptorV1 =
            serde_json::from_slice(&parent_json).expect("parent descriptor json");
        descriptor.parent_session_id = Some(SessionId(parent_sid.0.clone()));
        descriptor.graph_name = Some("child_graph".to_string());
        descriptor
    };
    let child_root = root_session(child_id, "child_task");
    storage
        .start_run(
            &SessionId(child_id.to_string()),
            &child_descriptor,
            RunCheckpoint {
                root: &child_root,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start child");
    sqlx::query("UPDATE orchestration_sessions SET state_revision = 2 WHERE session_id = ?")
        .bind(child_id)
        .execute(&*pool)
        .await
        .expect("advance child revision");

    // Run the parent step. Because the child's END task is a reattachable
    // terminal child? No — the child spawn path runs it. The InnerGraphTask
    // sets the parent effect marker; the parent commit fails on the stale
    // child, so the parent must be Interrupted, never rewound.
    let err = engine
        .run_step(&parent_sid)
        .await
        .expect_err("stale child must fail the parent commit");
    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::RevisionMismatch { .. }
        ),
        "expected RevisionMismatch, got {err:?}"
    );

    let record = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("parent present");
    assert_eq!(
        record.status,
        SessionStatus::Interrupted,
        "child effect must propagate Interrupted to the parent, not rewind/replay"
    );
}

// Important 3: the deterministic restore is ONE atomic revision-fenced storage
// operation. Simulate the concurrent interleaving: a transition wins between
// the check and the save — the restore must leave the newer row untouched.
#[tokio::test]
async fn restore_pre_step_is_single_atomic_revision_fence() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-atomic-restore".to_string());

    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "pre_task");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&session_id, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start_run");

    // Simulate a concurrent transition that advances revision 1 -> 2 with a
    // newer position before the restore runs.
    sqlx::query(
        "UPDATE orchestration_sessions SET state_revision = state_revision + 1,
                current_task_id = 'newer_post_task'
         WHERE session_id = ?",
    )
    .bind(&session_id.0)
    .execute(&*pool)
    .await
    .expect("concurrent bump to revision 2");

    // The restore anchored at expected_revision = 1 must fail RevisionMismatch
    // and leave the newer row (revision 2, newer task) untouched.
    let pre = root_session(&session_id.0, "pre_task");
    let err = storage
        .restore_pre_step(&session_id, 1, &pre)
        .await
        .expect_err("restore must refuse when a concurrent transition advanced the revision");
    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::RevisionMismatch { expected: 1, .. }
        ),
        "expected RevisionMismatch(expected=1), got {err:?}"
    );

    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("row present");
    assert_eq!(
        record.state_revision, 2,
        "concurrent revision 2 must survive — never overwritten by the older restore"
    );
    let db_task: String = sqlx::query_scalar(
        "SELECT current_task_id FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(&session_id.0)
    .fetch_one(&*pool)
    .await
    .expect("read current_task_id");
    assert_eq!(
        db_task, "newer_post_task",
        "the concurrent newer position is preserved"
    );
}

// Important 4: load_run rejects corrupt execution_version — negative values
// and forward versions (>=2) are non-replayable, not coerced to 0 or 1
// (A7), and the row/blobs are preserved verbatim.
#[tokio::test]
async fn negative_execution_version_is_non_replayable() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = "sess-neg-execver";

    // Seed a v1 row with execution_version = -3.
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES (?, 'ctr_t', 'test-preset', 3, 'running', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 -3, 1,
                 '{\"wait\":null,\"step_in_flight\":null,\"in_flight\":null,\"failure\":null,\"cancel_requested\":false}',
                 ?)",
    )
    .bind(session_id)
    .bind(serde_json::to_vec(&test_descriptor(session_id)).expect("descriptor"))
    .execute(&*pool)
    .await
    .expect("seed negative execution_version row");

    let err = storage
        .load_run(&SessionId(session_id.to_string()))
        .await
        .expect_err("negative execution_version must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error, got {err:?}"
    );

    // Row preserved verbatim (version still -3).
    let raw: i64 = sqlx::query_scalar(
        "SELECT execution_version FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(&*pool)
    .await
    .expect("read raw execution_version");
    assert_eq!(raw, -3, "row preserved verbatim");
}

#[tokio::test]
async fn unsupported_forward_execution_version_is_non_replayable() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = "sess-fwd-execver";

    // Seed a row with execution_version = 7 (forward version, no policy).
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES (?, 'ctr_t', 'test-preset', 3, 'running', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 7, 1,
                 '{\"wait\":null,\"step_in_flight\":null,\"in_flight\":null,\"failure\":null,\"cancel_requested\":false}',
                 ?)",
    )
    .bind(session_id)
    .bind(serde_json::to_vec(&test_descriptor(session_id)).expect("descriptor"))
    .execute(&*pool)
    .await
    .expect("seed forward execution_version row");

    let err = storage
        .load_run(&SessionId(session_id.to_string()))
        .await
        .expect_err("forward execution_version must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error, got {err:?}"
    );

    // Row preserved verbatim.
    let raw: i64 = sqlx::query_scalar(
        "SELECT execution_version FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(&*pool)
    .await
    .expect("read raw execution_version");
    assert_eq!(raw, 7, "row preserved verbatim");
}

// Important 5: the in-flight (step_in_flight) intent is persisted BEFORE an
// external effect (via mark_step_in_flight fenced to running + revision),
// so a crash after an effect but before the result commit leaves an
// interrupted (never replayable) run. We prove the durable row carries the
// in-flight marker after the pre-effect write, and that a running step's
// post-failure path stays non-replayable.
#[tokio::test]
async fn step_in_flight_persisted_before_effect_dispatch() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());

    // Parent graph: manual-wait start (Running) then end.
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
        .add_task(Arc::new(EndTask))
        .add_edge("manual_wait_task", "end_task")
        .build()
        .expect("test graph"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent");

    // Manually invoke the pre-effect mark path the engine uses before a step.
    let pre = storage
        .get(&parent_sid.0)
        .await
        .expect("get pre-step root")
        .expect("root present");
    let in_flight_state = RunStateV1 {
        step_in_flight: Some(pre.current_task_id.clone()),
        ..RunStateV1::default()
    };
    storage
        .mark_step_in_flight(
            &parent_sid,
            1, // start_run wrote revision 1
            RunCheckpoint {
                root: &pre,
                children: &[],
            },
            &in_flight_state,
        )
        .await
        .expect("mark_step_in_flight succeeds before the effect");

    // The durable row must carry the in-flight marker with status still
    // running and a newly advanced revision. The revision bump is the CAS
    // boundary that prevents a previously loaded signal from erasing the
    // marker.
    let record = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("row present");
    assert_eq!(record.status, SessionStatus::Running);
    assert_eq!(record.state_revision, 2, "in-flight mark advances revision");
    let state = record.state.expect("state present");
    assert_eq!(
        state.step_in_flight.as_deref(),
        Some(pre.current_task_id.as_str()),
        "step_in_flight persisted BEFORE any effect dispatch (Important 5)"
    );
}

// Important 5 (crash-after-effect): when a pre-effect in-flight mark was
// written and the engine's step performs an effect then has its commit fail,
// the row transitions to Interrupted (never replayable) — not left running.
#[tokio::test]
async fn crash_after_effect_with_pre_step_mark_lands_interrupted() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());

    // Parent graph: EffectTask (external effect) → end.
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(EffectTask))
        .add_task(Arc::new(EndTask))
        .add_edge("effect_task", "end_task")
        .build()
        .expect("test graph"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent");

    // Stale child so the post-effect commit fails — the row must go
    // Interrupted (Important 5: never replayable), even though a pre-step
    // in-flight mark was written at the start of the step.
    let child_id = "parent:child:crash";
    let child_session = Session::new_from_task(child_id.to_string(), "child_task");
    let child_cp = ChildCheckpoint {
        session: child_session,
        status: SessionStatus::Running,
        state: RunStateV1::default(),
        state_revision: 1,
        graph_name: Some("inner_graph".to_string()),
    };
    engine
        .shared_state()
        .children
        .write()
        .await
        .insert(parent_sid.0.clone(), vec![child_cp]);
    let child_descriptor = test_descriptor(&parent_sid.0);
    let child_root = root_session(child_id, "child_task");
    storage
        .start_run(
            &SessionId(child_id.to_string()),
            &child_descriptor,
            RunCheckpoint {
                root: &child_root,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start child");
    sqlx::query("UPDATE orchestration_sessions SET state_revision = 2 WHERE session_id = ?")
        .bind(child_id)
        .execute(&*pool)
        .await
        .expect("advance child revision");

    // Run the parent step. The pre-step in-flight mark is written first, the
    // EffectTask runs, then the commit fails on the stale child — the row must
    // be Interrupted, not left running/replayable.
    let _err = engine
        .run_step(&parent_sid)
        .await
        .expect_err("stale child must fail the commit");

    let record = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("parent present");
    assert_eq!(
        record.status,
        SessionStatus::Interrupted,
        "crash after effect must land Interrupted, never replayable"
    );
}

// Minor 2: a prior step's external effect must not leak into a later step —
// after a successful intermediate (Paused) effectful step, the marker is
// cleared so a later deterministic commit failure is NOT over-classified
// Interrupted.
#[tokio::test]
async fn external_effect_marker_cleared_per_step() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());

    // Parent graph: effectful continue (EffectTask → Continue, sets marker) →
    // deterministic continue (DeterministicContinueTask) → end.
    // Step 1 must be a Running (Paused) boundary with the marker present for
    // the effect classification; then the marker is cleared before step 2.
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(EffectTask))
        .add_task(Arc::new(DeterministicContinueTask))
        .add_task(Arc::new(EndTask))
        .add_edge("effect_task", "deterministic_continue_task")
        .add_edge("deterministic_continue_task", "end_task")
        .build()
        .expect("test graph"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent");

    // Step 1 (EffectTask) → the post-step context still carries the marker
    // during classification; after the committed Paused boundary the marker
    // is cleared.
    let record1 = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("parent present");
    assert_eq!(record1.state_revision, 1, "initial revision");

    // Run the first step (EffectTask → Continue → Paused at deterministic_continue).
    engine
        .run_step(&parent_sid)
        .await
        .expect("first effectful running step");

    // After the Paused boundary the persisted context must no longer carry the
    // marker (Minor 2).
    let post = storage
        .get(&parent_sid.0)
        .await
        .expect("get post-step root")
        .expect("root present");
    let marker: Option<bool> = post
        .context.get(nexus_orchestration::engine::EXTERNAL_EFFECT_MARKER);
    assert_eq!(
        marker, None,
        "the external-effect marker must be cleared after a successful Paused step commit (Minor 2)"
    );

    // Step 2 (DeterministicContinueTask → Paused at end). Give it a stale
    // child so its commit fails. Because the marker was cleared, the step is
    // deterministic and the engine restores to the pre-step position (NOT
    // Interrupted) — proving no stale pseudo-state leaks in.
    let child_id = "parent:child:minor2";
    let child_session = Session::new_from_task(child_id.to_string(), "child_task");
    let child_cp = ChildCheckpoint {
        session: child_session,
        status: SessionStatus::Running,
        state: RunStateV1::default(),
        state_revision: 1,
        graph_name: Some("inner_graph".to_string()),
    };
    engine
        .shared_state()
        .children
        .write()
        .await
        .insert(parent_sid.0.clone(), vec![child_cp]);
    let child_descriptor = test_descriptor(&parent_sid.0);
    let child_root = root_session(child_id, "child_task");
    storage
        .start_run(
            &SessionId(child_id.to_string()),
            &child_descriptor,
            RunCheckpoint {
                root: &child_root,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start child");
    sqlx::query("UPDATE orchestration_sessions SET state_revision = 2 WHERE session_id = ?")
        .bind(child_id)
        .execute(&*pool)
        .await
        .expect("advance child revision");

    // Capture the deterministic pre-step position.
    let deterministic_pre = storage
        .get(&parent_sid.0)
        .await
        .expect("get pre-step root")
        .expect("root present")
        .current_task_id;

    let err = engine
        .run_step(&parent_sid)
        .await
        .expect_err("stale child must fail the deterministic second step");
    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::RevisionMismatch { .. }
        ),
        "expected RevisionMismatch, got {err:?}"
    );

    // The row must NOT be Interrupted (the marker was cleared); it stays
    // non-terminal and is deterministically restored to the pre-step position.
    let final_record = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("parent present");
    assert!(
        final_record.status != SessionStatus::Interrupted,
        "cleared-marker deterministic failure is NOT over-classified Interrupted"
    );
    let after = storage
        .get(&parent_sid.0)
        .await
        .expect("get post-failure root")
        .expect("root present");
    assert_eq!(
        after.current_task_id, deterministic_pre,
        "deterministic failure restored to pre-step position (marker correctly cleared)"
    );
}

// ---------------------------------------------------------------------------
// Fix round 6 (task re-review): regression tests for each fixed contract
// ---------------------------------------------------------------------------

/// A graph-flow task that records a context marker proving the task's `run`
/// was actually dispatched (used by the engine-path Minor 1 test to assert an
/// effectful task never dispatches before `mark_step_in_flight` succeeds).
struct MarkerEffectTask;
#[async_trait::async_trait]
impl graph_flow::Task for MarkerEffectTask {
    fn id(&self) -> &'static str {
        "marker_effect_task"
    }
    async fn run(&self, ctx: graph_flow::Context) -> graph_flow::Result<graph_flow::TaskResult> {
        // Set both the engine effect marker AND a test-only dispatch marker so
        // the test can observe whether the task actually ran.
        ctx.set(nexus_orchestration::engine::EXTERNAL_EFFECT_MARKER, true).unwrap();
        ctx.set("dispatched", true).unwrap();
        Ok(graph_flow::TaskResult::new(
            None,
            graph_flow::NextAction::Continue,
        ))
    }
}

// Important 1: a legacy v0 child has no complete child descriptor/state
// contract. Recovery must reject the parent rather than omit or reinterpret
// the child, while preserving the legacy evidence for inspection.
#[tokio::test]
async fn v0_child_is_non_replayable_and_preserved() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let parent_sid = SessionId("par-v0-child".to_string());
    let child_id = "par-v0-child:child:1";
    let parent = root_session(&parent_sid.0, "parent_task");
    storage
        .start_run(
            &parent_sid,
            &test_descriptor(&parent_sid.0),
            RunCheckpoint {
                root: &parent,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start parent");

    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, parent_session_id,
             current_task_id, status, context_json, created_at, updated_at,
             execution_version, state_revision)
         VALUES (?, 'ctr', 'test-preset', 3, ?, 'child_task', 'running',
                 '{\"data\":{}}', 1756990000, 1756990300, 0, 4)",
    )
    .bind(child_id)
    .bind(&parent_sid.0)
    .execute(&*pool)
    .await
    .expect("seed v0 child row");

    let err = storage
        .load_children(&parent_sid)
        .await
        .expect_err("legacy child must make parent recovery non-replayable");
    assert!(
        err.to_string()
            .contains("v0 legacy child row (non-replayable)"),
        "unexpected error: {err}"
    );

    let persisted: (i64, i64, String) = sqlx::query_as(
        "SELECT execution_version, state_revision, status
         FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(child_id)
    .fetch_one(&*pool)
    .await
    .expect("read preserved v0 child");
    assert_eq!(persisted, (0, 4, "running".to_string()));
}

// Important 1: `load_children` must reject a child whose `state_revision` is
// negative (corrupt) as non-replayable — never coerced to zero. The child row
// and its blobs are preserved verbatim, exactly as `load_run` does for the
// root row (round-2/4 contract extended to child rows).
#[tokio::test]
async fn negative_child_revision_is_non_replayable() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());
    let parent_sid = SessionId("par-neg-child-rev".to_string());

    // Start a parent run.
    let descriptor = test_descriptor(&parent_sid.0);
    let root = root_session(&parent_sid.0, "parent_task");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&parent_sid, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start parent");

    // Seed a v1 child row with state_revision = -4 (corrupt).
    let child_id = "par-neg-child-rev:child:1";
    let child_descriptor = test_descriptor(child_id);
    // The child descriptor must name this parent so `load_children` finds it.
    let mut child_descriptor = child_descriptor;
    child_descriptor.parent_session_id = Some(parent_sid.clone());
    child_descriptor.graph_name = Some("inner_graph".to_string());
    let state = RunStateV1::default();
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, parent_session_id,
             current_task_id, status, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES (?, 'ctr', 'test-preset', 3, ?, 'child_task', 'running',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 1, -4, ?, ?)",
    )
    .bind(child_id)
    .bind(&parent_sid.0)
    .bind(serde_json::to_vec(&state).expect("serialize state"))
    .bind(serde_json::to_vec(&child_descriptor).expect("serialize child descriptor"))
    .execute(&*pool)
    .await
    .expect("seed negative-revision child row");

    // `load_children` must treat the negative child revision as corrupt
    // (non-replayable) rather than coerce it to zero and hydrate a valid
    // checkpoint.
    let err = storage
        .load_children(&parent_sid)
        .await
        .expect_err("negative child state_revision must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error for negative child revision, got {err:?}"
    );

    // The child row is preserved verbatim (state_revision still -4).
    let raw: i64 = sqlx::query_scalar(
        "SELECT state_revision FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(child_id)
    .fetch_one(&*pool)
    .await
    .expect("read raw child state_revision");
    assert_eq!(raw, -4, "child row preserved verbatim");

    // `hydrate_children` must also propagate the error (recovery refuses to
    // reconstruct a parent over a corrupt child) — no stale checkpoint.
    let err = engine
        .shared_state()
        .hydrate_children(&parent_sid)
        .await
        .expect_err("hydrate_children must propagate the negative child revision");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error from hydrate_children, got {err:?}"
    );
    let shared = engine.shared_state();
    let map = shared.children.read().await;
    assert!(
        !map.contains_key(&parent_sid.0),
        "negative child revision must not populate the children map"
    );
}

// Important 2: a stale `mark_step_in_flight` against a `waiting_for_input`
// row must NOT overwrite/clear the durable WaitRecord. The fence admits only
// statuses that may actually be stepped (running/paused); waiting_for_input
// (and unknown statuses) are excluded (A4 token retention).
#[tokio::test]
async fn stale_mark_step_in_flight_against_waiting_row_leaves_wait_untouched() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool);
    let session_id = SessionId("sess-wait-mark".to_string());

    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "wait_task");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&session_id, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start_run");

    // Commit a human-wait transition with a durable WaitRecord (revision 1 → 2).
    let wait_state = RunStateV1 {
        wait: Some(nexus_orchestration::run_state::WaitRecord {
            wait_id: "wait-mark-token".to_string(),
            task_id: "wait_task".to_string(),
            child_session_id: None,
            child_task_id: None,
            kind: nexus_orchestration::run_state::WaitKind::Manual,
        }),
        ..RunStateV1::default()
    };
    let root = root_session(&session_id.0, "wait_task");
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
        .expect("commit wait (revision 2)");

    // A stale step invocation at expected_revision = 2 (matching, so the CAS
    // fence passes) but on a waiting_for_input row must be refused — never
    // overwrite the WaitRecord.
    let pre = root_session(&session_id.0, "stale_effect_start");
    let in_flight_state = RunStateV1 {
        step_in_flight: Some("stale_effect_start".to_string()),
        ..RunStateV1::default()
    };
    let _err = storage
        .mark_step_in_flight(
            &session_id,
            2, // matches persisted revision
            RunCheckpoint {
                root: &pre,
                children: &[],
            },
            &in_flight_state,
        )
        .await
        .expect_err("mark_step_in_flight must refuse a human-wait row");

    // The row stayed waiting_for_input; the WaitRecord (with its token) and
    // revision must be untouched.
    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(
        record.status,
        SessionStatus::WaitingForInput,
        "stale mark must not change the wait row's status"
    );
    assert_eq!(
        record.state_revision, 2,
        "stale mark must not advance revision"
    );
    let state = record.state.expect("v1 state present");
    let wait = state.wait.expect("WaitRecord retained");
    assert_eq!(
        wait.wait_id, "wait-mark-token",
        "WaitRecord token must survive a stale mark_step_in_flight"
    );
    assert_eq!(wait.task_id, "wait_task");
    assert!(
        state.step_in_flight.is_none(),
        "stale mark must not persist an in-flight marker on the wait row"
    );
}

// Important 2: a stale `restore_pre_step` against a `waiting_for_input` row
// must NOT clear the durable WaitRecord. The fence admits only statuses that
// may actually be stepped (running/paused); waiting_for_input is excluded
// (A4 token retention until an authorized successful CAS).
#[tokio::test]
async fn stale_restore_pre_step_against_waiting_row_leaves_wait_untouched() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool);
    let session_id = SessionId("sess-wait-restore".to_string());

    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "wait_task");
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&session_id, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start_run");

    // Commit a human-wait transition with a durable WaitRecord (revision 1 → 2).
    let wait_state = RunStateV1 {
        wait: Some(nexus_orchestration::run_state::WaitRecord {
            wait_id: "wait-restore-token".to_string(),
            task_id: "wait_task".to_string(),
            child_session_id: None,
            child_task_id: None,
            kind: nexus_orchestration::run_state::WaitKind::Manual,
        }),
        ..RunStateV1::default()
    };
    let root = root_session(&session_id.0, "wait_task");
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
        .expect("commit wait (revision 2)");

    // A stale restore anchored at the matching revision must be refused on a
    // waiting_for_input row — never clear its WaitRecord.
    let pre = root_session(&session_id.0, "pre_wait_boundary");
    let err = storage
        .restore_pre_step(&session_id, 2, &pre)
        .await
        .expect_err("restore_pre_step must refuse a human-wait row");
    // Revision matches, so the refusal is TerminalState (waiting is excluded).
    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::TerminalState(_)
        ),
        "expected TerminalState for a waiting_for_input restore, got {err:?}"
    );

    // The row stayed waiting_for_input; the WaitRecord (with its token) and
    // the run_state_json (position) must be untouched.
    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(
        record.status,
        SessionStatus::WaitingForInput,
        "stale restore must not change the wait row's status"
    );
    assert_eq!(
        record.state_revision, 2,
        "stale restore must not advance revision"
    );
    let state = record.state.expect("v1 state present");
    let wait = state
        .wait
        .expect("WaitRecord retained through stale restore");
    assert_eq!(
        wait.wait_id, "wait-restore-token",
        "WaitRecord token must survive a stale restore_pre_step"
    );
    assert_eq!(wait.task_id, "wait_task");
}

// Minor 1 (engine path): the engine must NOT dispatch an effectful task
// before `mark_step_in_flight` is persisted. When the pre-step row is in a
// human-wait state (not re-steppable), the engine refuses the step before
// `runner.run` executes the task — the effect task's `run` never fires.
#[tokio::test]
async fn engine_effectful_task_does_not_dispatch_before_in_flight_mark() {
    let (pool, _db) = fresh_pool().await;
    let (storage, engine) = fresh_engine(pool.clone());

    // Parent graph: marker-effect task (external effect) → end.
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(MarkerEffectTask))
        .add_task(Arc::new(EndTask))
        .add_edge("marker_effect_task", "end_task")
        .build()
        .expect("test graph"),
    );
    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent");

    // Manually put the parent into a human-wait state (durable WaitRecord,
    // revision 1 → 2). A waiting_for_input row must NEVER be re-stepped.
    let root = storage
        .get(&parent_sid.0)
        .await
        .expect("get pre-wait root")
        .expect("root present");
    let wait_state = RunStateV1 {
        wait: Some(nexus_orchestration::run_state::WaitRecord {
            wait_id: "wait-engine-gate".to_string(),
            task_id: root.current_task_id.clone(),
            child_session_id: None,
            child_task_id: None,
            kind: nexus_orchestration::run_state::WaitKind::Manual,
        }),
        ..RunStateV1::default()
    };
    storage
        .commit_transition(
            &parent_sid,
            1,
            RunCheckpoint {
                root: &root,
                children: &[],
            },
            SessionStatus::WaitingForInput,
            &wait_state,
        )
        .await
        .expect("commit wait");

    // The engine's run_step must fail BEFORE dispatching the effectful task:
    // run_step_internal calls mark_step_in_flight, which is fenced to
    // running/paused and refuses the waiting row, so `runner.run` (which
    // fires MarkerEffectTask.run) is never reached.
    let err = engine
        .run_step(&parent_sid)
        .await
        .expect_err("engine must refuse to step a human-wait row");
    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::TerminalState(_)
        ),
        "expected TerminalState for stepping a waiting row, got {err:?}"
    );

    // Prove the effect task did NOT dispatch: the "dispatched" marker and the
    // external-effect marker must both be absent from the persisted context.
    let post = storage
        .get(&parent_sid.0)
        .await
        .expect("get post-refused root")
        .expect("root present");
    let dispatched: Option<bool> = post.context.get("dispatched");
    assert_eq!(
        dispatched, None,
        "the effectful task must NOT dispatch before the in-flight mark persists"
    );
    let effect: Option<bool> = post
        .context.get(nexus_orchestration::engine::EXTERNAL_EFFECT_MARKER);
    assert_eq!(
        effect, None,
        "no external-effect marker — no effect task ran before the mark gate"
    );

    // The durable WaitRecord and token survive untouched.
    let record = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(record.status, SessionStatus::WaitingForInput);
    let state = record.state.expect("v1 state present");
    let wait = state.wait.expect("WaitRecord retained");
    assert_eq!(
        wait.wait_id, "wait-engine-gate",
        "WaitRecord token retained when the engine refuses a stale step"
    );
    assert_eq!(record.state_revision, 2, "revision unchanged");
}

// ---------------------------------------------------------------------------
// Fix round 7 (writer × classifier contract, P0 Task 3 blocker): scheduler
// converge/merge parks must persist the A2 shape — `paused` + join keys and
// NO human-wait token — while genuine manual human waits keep persisting
// `waiting_for_input` + a fresh A4 token.
// ---------------------------------------------------------------------------

/// A converge/merge parked-join run used to prove the engine writer's
/// durable output shape (A2: scheduler joins = paused + join keys, tokenless).
/// `branch_b` is the hanging upstream (never walked, never arrives), so the
/// join deterministically parks at 1/2 arrivals with a 300s deadline.
const fn converge_preset_yaml() -> &'static str {
    r#"preset:
  id: e2e-engine-converge
  version: 1
  kind: creator
  description: "v1.186 P0 T3 fix — engine converge-join park writer shape"
  requires_capabilities: []
  initial: start
  terminal: done
states:
    - id: start
      next: branch_a
    - id: branch_a
      next:
        branches: []
        default: join
    - id: branch_b
      description: "Hanging upstream edge — never walked, never arrives"
      next: join
    - id: join
      converge: { strategy: wait_for_all }
      timeout_ms: 300000
      on_timeout: fallback
      next: done
    - id: fallback
      next: done
    - id: done
      terminal: true
"#
}

// Writer shape (a): a real engine step that parks at a converge/merge join
// must persist `paused`, NO `WaitRecord`, and the exact current-task gate-park
// marker alongside live join keys. The exact marker authorizes bounded
// re-drive after restart without confusing manual waits that retain stale
// broad join keys.
#[tokio::test]
async fn engine_join_park_persists_paused_tokenless_with_live_join_keys() {
    let (pool, db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc.clone(),
        workflow_store.clone(),
        nexus_orchestration::CapabilityRegistryHolder::with_registry(caps.clone()),
    );

    let mut loaded =
        nexus_orchestration::preset::load_preset_from_str(converge_preset_yaml(), &caps)
            .expect("converge preset loads");
    // Raw-YAML loads carry no source identity (the loader cannot know the
    // origin); a v1 run requires one, so freeze the embedded identity over
    // the manifest exactly as `load_embedded_preset` does (A2/A7).
    loaded.source_identity = Some(
        nexus_orchestration::preset::loader::preset_source_identity(
            &loaded.manifest,
            None,
            Some("e2e-engine-converge"),
        )
        .expect("embedded source identity"),
    );
    let sid = engine
        .start_session_with_preset_for_creator(&loaded, "ctr_test")
        .await
        .expect("start converge run");

    // Position the run exactly as the production park does before its next
    // boot: `current_task_id = join` with one predecessor arrival already
    // recorded (`branch_a` walked in the prior process; `branch_b` is the
    // hanging upstream — never arrives). The wire (same YAML shape as the
    // committed T3 restart fixture) arms the join gate with
    // converge_predecessors {branch_a, branch_b}, so the NEXT real engine
    // step at the join gate parks at 1/2 arrivals (deadline 300s not fired).
    let mut session = storage
        .get(&sid.0)
        .await
        .expect("get pre-step session")
        .expect("session present");
    session.current_task_id = "join".to_string();
    session
        .context.set("_converge_arrivals_join", vec!["branch_a".to_string()])
            .unwrap();
    storage.save(session).await.expect("save parked position");

    // One real engine step at the parked join → graph-flow `WaitForInput`;
    // the store-wired engine must persist the SCHEDULER shape.
    let outcome = engine
        .run_step(&sid)
        .await
        .expect("run_step parks the join");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::WaitingForInput { .. }
        ),
        "the engine surfaces the join park as WaitingForInput, got {outcome:?}"
    );

    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(
        record.status,
        SessionStatus::Paused,
        "a scheduler converge/merge park persists as `paused`, not `waiting_for_input` (A2)"
    );
    let state = record.state.expect("v1 state present");
    assert!(
        state.wait.is_none(),
        "a scheduler join park must NOT carry a human-wait token (A2/A4)"
    );

    // The join keys are live on the PERSISTED row (the bounded deadline
    // re-check evidence — never a manual-wait token). Re-read the post-step
    // row: the pre-step snapshot predates `join_timeout_tick` writing the
    // wait-start key.
    let persisted = storage
        .get(&sid.0)
        .await
        .expect("get post-step session")
        .expect("session present");
    let context_value = serde_json::to_value(&persisted.context).expect("context json");
    let map = nexus_orchestration::resume_rules::context_data(&context_value).expect("data map");
    assert_eq!(
        nexus_orchestration::resume_rules::live_join_keys(map),
        vec![
            "_converge_arrivals_join".to_string(),
            "_join_wait_start_join".to_string()
        ],
        "the parked join must retain its live join keys"
    );
    assert!(
        nexus_orchestration::resume_rules::gate_park_live(map, &persisted.current_task_id),
        "the engine writer must persist the exact current-task gate marker"
    );
    drop(context_value);

    // Reopen the SAME DB file (daemon restart boundary) — the canonical A7
    // classifier must see the exact engine gate marker and classify the park
    // as a converge/merge chain, never a human wait.
    pool.close().await;
    let pool_b = nexus_local_db::open_pool(db.path())
        .await
        .expect("reopen pool");
    let storage_b = Arc::new(SqliteSessionStorage::new(pool_b.clone().into()));
    let record_after_reopen = storage_b
        .load_run(&sid)
        .await
        .expect("load_run after reopen")
        .expect("run present");
    let context_b = storage_b
        .get(&sid.0)
        .await
        .expect("get after reopen")
        .expect("session present");
    let context_value_b = serde_json::to_value(&context_b.context).expect("context json");
    let gate_park_live = nexus_orchestration::resume_rules::context_data(&context_value_b)
        .is_some_and(|data| {
            nexus_orchestration::resume_rules::gate_park_live(data, &context_b.current_task_id)
        });
    assert_eq!(
        nexus_orchestration::resume_rules::classify_recovery(
            &record_after_reopen.status,
            record_after_reopen.state.as_ref(),
            gate_park_live,
        ),
        nexus_orchestration::resume_rules::RecoveryClass::ConvergeMerge,
        "an engine-parked join reopens as ConvergeMerge (rule 5), not HumanWait"
    );
}

// Writer shape (b): a genuine manual human wait (ManualWaitTask →
// `WaitForInput`, no live join keys) must STILL persist `waiting_for_input`
// with a FRESH durable A4 token (one per arrival), retained byte-identical
// across reopen until a successful CAS consumes it. The writer fix must not
// weaken the human-wait token path (A4 precedence in `classify_recovery`
// rule 4 keeps working unchanged).
#[tokio::test]
async fn engine_manual_wait_persists_waiting_for_input_fresh_retained_token() {
    let (pool, db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_orchestration::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );

    let graph = Arc::new(
    graph_flow::GraphBuilder::new("test-graph-manual")
        .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
        .build()
        .expect("test graph"),
    );
    let sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start_session");

    // One step at the manual wait → WaitingForInput; the store-wired engine
    // must persist the HUMAN shape: waiting_for_input + fresh token.
    let outcome = engine.run_step(&sid).await.expect("run_step");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::WaitingForInput { .. }
        ),
        "expected WaitingForInput, got {outcome:?}"
    );

    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(
        record.status,
        SessionStatus::WaitingForInput,
        "a genuine human wait persists as `waiting_for_input` (A2)"
    );
    let state = record.state.expect("v1 state present");
    let wait = state
        .wait
        .expect("human wait must carry a durable WaitRecord (A4)");
    assert!(
        !wait.wait_id.is_empty(),
        "the wait token must be a fresh UUID, not empty"
    );
    assert_eq!(wait.kind, nexus_orchestration::run_state::WaitKind::Manual);
    let token = wait.wait_id.clone();

    // Reopen the SAME DB file (daemon restart boundary): the token must
    // survive byte-identical (A4 retention until a successful CAS consumes
    // it) and the status must remain waiting_for_input.
    pool.close().await;
    let pool_b = nexus_local_db::open_pool(db.path())
        .await
        .expect("reopen pool");
    let storage_b = Arc::new(SqliteSessionStorage::new(pool_b.into()));
    let record_after = storage_b
        .load_run(&sid)
        .await
        .expect("load_run after reopen")
        .expect("run present");
    assert_eq!(record_after.status, SessionStatus::WaitingForInput);
    let wait_after = record_after
        .state
        .clone()
        .expect("state present")
        .wait
        .expect("token retained");
    assert_eq!(
        wait_after.wait_id, token,
        "the A4 wait token must be retained unchanged across reopen"
    );

    // The canonical A7 classifier still sees the HUMAN wait (rule 4),
    // never a scheduler join.
    let context = storage_b
        .get(&sid.0)
        .await
        .expect("get after reopen")
        .expect("session present");
    let context_value = serde_json::to_value(&context.context).expect("context json");
    let chain_class = nexus_orchestration::resume_rules::context_data(&context_value)
        .is_some_and(nexus_orchestration::resume_rules::is_converge_merge_chain);
    assert!(
        !chain_class,
        "a manual human wait carries no live join keys"
    );
    assert_eq!(
        nexus_orchestration::resume_rules::classify_recovery(
            &record_after.status,
            record_after.state.as_ref(),
            chain_class,
        ),
        nexus_orchestration::resume_rules::RecoveryClass::HumanWait,
        "token-bearing human wait classifies HumanWait (rule 4), unchanged"
    );
}

// ---------------------------------------------------------------------------
// Fix round 4 — Critical 1: labeled/conditional-routed manual waits keep
// their fresh token even with stale/broad join keys (current-gate marker
// is the ONLY authoritative scheduler-park evidence).
// ---------------------------------------------------------------------------

/// Judge executor returning a GO verdict whose reason contains the "go"
/// label (the `resolve_labeled_target` first-token match).
struct GoJudgeProvider;

#[async_trait::async_trait]
impl nexus_orchestration::capability::PromptExecutor for GoJudgeProvider {
    async fn execute(
        &self,
        _request: nexus_orchestration::capability::PromptRequest,
    ) -> Result<
        nexus_orchestration::capability::PromptResult,
        nexus_orchestration::capability::CapabilityError,
    > {
        Ok(nexus_orchestration::capability::PromptResult {
            full_text: "Go ahead — first-token match routes label 'go'.".to_string(),
            host_session_id: "host-sess".to_string(),
            operation_id: "op-1".to_string(),
        })
    }
}

/// Registry with the mock judge provider (deterministic labeled routing).
///
/// A1: the `judge.llm` prompt consumer resolves the run's coordinator
/// cancellation token from the SAME shared per-run map the engine registers
/// into at run admission. Production (daemon boot) shares one map between
/// the registry and the engine; the fixture must mirror that — otherwise the
/// fail-closed `resolve_session_cancellation` refuses with
/// `CancellationUnavailable` for the engine-created run id.
fn judge_registry_holder(
    session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
) -> nexus_orchestration::CapabilityRegistryHolder {
    let deps = nexus_orchestration::capability::CapabilityRuntimeDeps {
        pool: None,
        prompt_executor: Some(std::sync::Arc::new(GoJudgeProvider)),
        session_cancels,
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    nexus_orchestration::CapabilityRegistryHolder::with_registry(std::sync::Arc::new(
        nexus_orchestration::CapabilityRegistry::with_runtime_deps(&deps),
    ))
}

/// Round-4 Critical 1 (labeled route): a genuine manual wait REACHED VIA the
/// real labeled-routing task path (`resolve_labeled_target` on a judge GO
/// writes `_merge_manual_state` + `_converge_arrivals_manual_state` for the
/// routed target) plus seeded stale historical keys. The writer must NOT
/// re-classify it to `paused`: it must persist `waiting_for_input` with a
/// fresh retained A4 token, and the canonical classifier must see
/// `HumanWait` (token beats join keys), never `ConvergeMerge` (which would
/// auto-step on the next boot).
#[tokio::test]
async fn engine_labeled_routed_manual_wait_keeps_token_despite_join_keys() {
    use nexus_orchestration::preset::manifest::{
        ExitWhen, LabeledNext, NextTarget, StateDefinition,
    };
    use nexus_orchestration::tasks::StateCompositeTask;

    let (pool, db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    // A1: one SHARED per-run cancellation map. The registry's `judge.llm`
    // consumer resolves the run's coordinator token from this map; the
    // engine registers it at admission. Production (daemon boot) shares a
    // single map between the registry and the engine.
    let session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
    let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc.clone(),
        workflow_store.clone(),
        judge_registry_holder(session_cancels.clone()),
    );
    // Unify the engine's per-run map with the one the registry's judge
    // resolves from (the same contract daemon boot establishes).
    engine.set_prompt_executor(
        std::sync::Arc::new(GoJudgeProvider),
        session_cancels.clone(),
    );

    // Hand-built graph over the REAL `StateCompositeTask` labeled-routing
    // path: start (llm_judge + Labeled next) → GoTo manual_state (Manual).
    let mk_state = |id: &str, exit_when: ExitWhen, next: Option<NextTarget>| {
        StateCompositeTask::from_manifest(&StateDefinition {
            id: id.into(),
            description: None,
            enter: vec![],
            exit_when: Some(exit_when),
            next,
            terminal: false,
            context_update: None,
            merge: None,
            converge: None,
            timeout_ms: None,
            on_timeout: None,
        })
    };
    let registry = judge_registry_holder(session_cancels).get();
    let start = mk_state(
        "start",
        ExitWhen::LlmJudge {
            template_file: Some("Evaluate: is this outline acceptable?".to_string()),
            judge_capability: Some("judge.llm".to_string()),
            min_interval: None,
        },
        Some(NextTarget::Labeled(vec![LabeledNext {
            label: "go".to_string(),
            target: "manual_state".to_string(),
        }])),
    )
    .with_registry(registry.expect("judge registry holder carries the mock provider"));
    let manual = mk_state(
        "manual_state",
        ExitWhen::Manual,
        Some(NextTarget::Linear("done".to_string())),
    );

    let graph = Arc::new(
        graph_flow::GraphBuilder::new("test-labeled-manual")
            .add_task(Arc::new(start))
            .add_task(Arc::new(manual))
            .build()
            .expect("test graph build"),
    );
    let sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start session");

    // A prior (historical) cycle left stale broad join keys behind — the
    // failing fixture shape alongside the routing keys.
    {
        let session = storage
            .get(&sid.0)
            .await
            .expect("get session")
            .expect("present");
        session
            .context.set("_merge_other", serde_json::json!(["x"]))
            .unwrap();
        session
            .context.set("_join_wait_start_other", serde_json::json!(1))
            .unwrap();
        storage.save(session).await.expect("save seeded session");
    }

    // Step 1: judge GO routes via the "go" label (GoTo manual_state). The
    // routing writes `_merge_manual_state` / `_converge_arrivals_manual_state`.
    let outcome = engine.run_step(&sid).await.expect("judge step");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::Paused { .. }
        ),
        "judge route is an inter-task boundary, got {outcome:?}"
    );

    // Step 2: the manual wait state parks (WaitForInput). Its context now
    // carries the routing-written broad keys + the stale historical pair —
    // the OLD writer misclassified this as a scheduler park.
    let outcome = engine.run_step(&sid).await.expect("manual wait step");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::WaitingForInput { .. }
        ),
        "manual wait surfaces as WaitingForInput, got {outcome:?}"
    );

    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(
        record.status,
        SessionStatus::WaitingForInput,
        "a labeled-routed manual wait persists as waiting_for_input, never paused (A2)"
    );
    let wait = record
        .state
        .as_ref()
        .and_then(|s| s.wait.as_ref())
        .expect("labeled-routed manual wait must carry a fresh A4 token");
    assert!(
        !wait.wait_id.is_empty(),
        "the wait token must be a fresh UUID"
    );

    // The post-step context carries the routing-written BROAD join keys
    // (`_merge_manual_state` / `_converge_arrivals_manual_state` — written
    // by labeled ROUTING for the routed target, no gate consumes them) plus
    // the stale historical pair: exactly the evidence that must never
    // demote the wait.
    let persisted = storage
        .get(&sid.0)
        .await
        .expect("get post-step session")
        .expect("session present");
    let context_value = serde_json::to_value(&persisted.context).expect("context json");
    let map = nexus_orchestration::resume_rules::context_data(&context_value).expect("data map");
    let live = nexus_orchestration::resume_rules::live_join_keys(map);
    assert!(
        live.contains(&"_merge_manual_state".to_string())
            && live.contains(&"_converge_arrivals_manual_state".to_string())
            && live.contains(&"_merge_other".to_string())
            && live.contains(&"_join_wait_start_other".to_string()),
        "labeled-routed manual wait carries live broad join keys (the failing shape): {live:?}"
    );
    // ...but NO current-gate park marker for the manual state (the post-step
    // graph-flow cursor is at `manual_state` — WaitForInput stays there).
    assert!(
        !nexus_orchestration::resume_rules::gate_park_live(map, &persisted.current_task_id),
        "a labeled-routed manual wait carries no current-gate park marker \
         (cursor '{}')",
        persisted.current_task_id
    );

    // Reopen the SAME DB file: the canonical classifier must see HumanWait
    // (rule 4 — fresh token beats old join keys), never ConvergeMerge.
    pool.close().await;
    let pool_b = nexus_local_db::open_pool(db.path())
        .await
        .expect("reopen pool");
    let storage_b = Arc::new(SqliteSessionStorage::new(pool_b.clone().into()));
    let record_after = storage_b
        .load_run(&sid)
        .await
        .expect("load_run after reopen")
        .expect("run present");
    assert_eq!(record_after.status, SessionStatus::WaitingForInput);
    assert_eq!(
        record_after
            .state
            .as_ref()
            .and_then(|s| s.wait.as_ref())
            .map(|w| w.wait_id.as_str()),
        Some(wait.wait_id.as_str()),
        "the A4 token is retained unchanged across reopen"
    );
    let context_b = storage_b
        .get(&sid.0)
        .await
        .expect("get after reopen")
        .expect("session present");
    let context_value_b = serde_json::to_value(&context_b.context).expect("context json");
    let chain_class = nexus_orchestration::resume_rules::context_data(&context_value_b)
        .is_some_and(nexus_orchestration::resume_rules::is_converge_merge_chain);
    assert!(chain_class, "the broad join keys survive reopen");
    assert_eq!(
        nexus_orchestration::resume_rules::classify_recovery(
            &record_after.status,
            record_after.state.as_ref(),
            chain_class,
        ),
        nexus_orchestration::resume_rules::RecoveryClass::HumanWait,
        "a labeled-routed manual wait with stale join keys classifies HumanWait (rule 4)"
    );
}

/// Round-4 Critical 1 (conditional route): the same contract via conditional
/// branch routing — `resolve_expression_target` writes
/// `_converge_arrivals_manual_state` for the routed target; a stale
/// `_merge_other` / `_join_wait_start_other` pair is pre-seeded as
/// historical/broad evidence. The manual wait must still persist
/// `waiting_for_input` + fresh retained token and reopen as `HumanWait`.
#[tokio::test]
async fn engine_conditional_routed_manual_wait_keeps_token_despite_join_keys() {
    let (pool, db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc.clone(),
        workflow_store.clone(),
        nexus_orchestration::CapabilityRegistryHolder::with_registry(std::sync::Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        )),
    );

    let yaml = r#"
preset:
  id: cond-manual
  version: 1
  kind: creator
  description: "round-4 — conditional-routed manual wait keeps its token"
  requires_capabilities: []
  initial: evaluate
  terminal: done
states:
  - id: evaluate
    next:
      branches:
        - when: "_context.score > 80"
          target: manual_state
      default: manual_state
  - id: manual_state
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;
    let caps = std::sync::Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
    let mut loaded = nexus_orchestration::preset::load_preset_from_str(yaml, &caps)
        .expect("conditional preset loads");
    loaded.source_identity = Some(
        nexus_orchestration::preset::loader::preset_source_identity(
            &loaded.manifest,
            None,
            Some("e2e-cond-manual"),
        )
        .expect("embedded source identity"),
    );
    let sid = engine
        .start_session_with_preset_for_creator(&loaded, "ctr_test")
        .await
        .expect("start conditional run");

    // Seed expression input + historical/broad join keys on the persisted
    // session before stepping (a prior cycle left them behind).
    {
        let session = storage
            .get(&sid.0)
            .await
            .expect("get session")
            .expect("present");
        session.context.set("score", serde_json::json!(95)).unwrap();
        session
            .context.set("_merge_other", serde_json::json!(["x"]))
            .unwrap();
        session
            .context.set("_join_wait_start_other", serde_json::json!(1))
            .unwrap();
        storage.save(session).await.expect("save seeded session");
    }

    // Step 1: conditional branch matches → GoTo manual_state.
    let outcome = engine.run_step(&sid).await.expect("branch step");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::Paused { .. }
        ),
        "conditional route is an inter-task boundary, got {outcome:?}"
    );

    // Step 2: the manual wait parks with routing + historical join keys live.
    let outcome = engine.run_step(&sid).await.expect("manual wait step");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::WaitingForInput { .. }
        ),
        "manual wait surfaces as WaitingForInput, got {outcome:?}"
    );

    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(
        record.status,
        SessionStatus::WaitingForInput,
        "a conditional-routed manual wait persists as waiting_for_input, never paused"
    );
    let wait = record
        .state
        .as_ref()
        .and_then(|s| s.wait.as_ref())
        .expect("fresh A4 token");
    assert!(!wait.wait_id.is_empty(), "fresh UUID token");

    // The routing + historical keys are live; no current-gate marker.
    let persisted = storage
        .get(&sid.0)
        .await
        .expect("get post-step session")
        .expect("session present");
    let context_value = serde_json::to_value(&persisted.context).expect("context json");
    let map = nexus_orchestration::resume_rules::context_data(&context_value).expect("data map");
    let live = nexus_orchestration::resume_rules::live_join_keys(map);
    assert!(
        live.contains(&"_converge_arrivals_manual_state".to_string())
            && live.contains(&"_merge_other".to_string())
            && live.contains(&"_join_wait_start_other".to_string()),
        "routing + historical join keys stay live (the failing shape): {live:?}"
    );
    assert!(
        !nexus_orchestration::resume_rules::gate_park_live(map, "manual_state"),
        "no current-gate park marker for the manual state"
    );

    // Reopen: HumanWait (rule 4 token beats join keys), token retained.
    pool.close().await;
    let pool_b = nexus_local_db::open_pool(db.path())
        .await
        .expect("reopen pool");
    let storage_b = Arc::new(SqliteSessionStorage::new(pool_b.clone().into()));
    let record_after = storage_b
        .load_run(&sid)
        .await
        .expect("load_run after reopen")
        .expect("run present");
    assert_eq!(record_after.status, SessionStatus::WaitingForInput);
    assert_eq!(
        record_after
            .state
            .as_ref()
            .and_then(|s| s.wait.as_ref())
            .map(|w| w.wait_id.as_str()),
        Some(wait.wait_id.as_str()),
        "A4 token retained unchanged across reopen"
    );
    let context_b = storage_b
        .get(&sid.0)
        .await
        .expect("get after reopen")
        .expect("session present");
    let context_value_b = serde_json::to_value(&context_b.context).expect("context json");
    let chain_class = nexus_orchestration::resume_rules::context_data(&context_value_b)
        .is_some_and(nexus_orchestration::resume_rules::is_converge_merge_chain);
    assert!(chain_class);
    assert_eq!(
        nexus_orchestration::resume_rules::classify_recovery(
            &record_after.status,
            record_after.state.as_ref(),
            chain_class,
        ),
        nexus_orchestration::resume_rules::RecoveryClass::HumanWait,
        "conditional-routed manual wait with stale join keys classifies HumanWait"
    );
}

// ---------------------------------------------------------------------------
// Fix round 4 — Critical 2: a nested inner-graph child human wait is
// preserved and propagated (child WaitRecord/identity/token survive), the
// parent/root wait names the exact waiting descendant, restart keeps the
// nested token, and there is NO automatic child resume/replay.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn engine_nested_child_manual_wait_persists_and_propagates_no_auto_resume() {
    let (pool, db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_orchestration::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );

    // Child graph: a manual wait (human input required).
    let inner_graph = Arc::new(
    graph_flow::GraphBuilder::new("inner_graph")
        .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
        .build()
        .expect("test graph"),
    );

    // Parent graph: the inner-graph task polls the child.
    let inner_task = nexus_orchestration::tasks::InnerGraphTask::new(
        Arc::new(engine.clone()),
        inner_graph,
        "parent_state",
        "_session_id",
        None,
    );
    let parent_graph = Arc::new(
    graph_flow::GraphBuilder::new("parent_graph")
        .add_task(Arc::new(inner_task))
        .build()
        .expect("test graph build"),
    );

    let parent_sid = engine
        .start_session("novel-writing", parent_graph)
        .await
        .expect("start parent session");

    // Parent step: the child parks at its manual wait. The parent must NOT
    // auto-resume the child and must return WaitingForInput (never
    // Completed/Paused — the old InnerGraphTask auto-approval bypass).
    let outcome = engine.run_step(&parent_sid).await.expect("run_step parent");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::WaitingForInput { .. }
        ),
        "a child human wait must propagate to the parent as WaitingForInput, got {outcome:?}"
    );

    // The child remains waiting with exactly one step marker, one wait
    // transition, and one parent child-checkpoint confirmation after start:
    // start_run(1) → mark(2) → park(3) → parent confirmation(4). Any
    // automatic resume/replay would advance it again.
    let children = storage
        .load_children(&parent_sid)
        .await
        .expect("load_children");
    assert_eq!(children.len(), 1, "one child persisted");
    let child = &children[0];
    assert_eq!(
        child.status,
        SessionStatus::WaitingForInput,
        "the child must remain waiting — no auto-resume"
    );
    assert_eq!(
        child.state_revision, 4,
        "child parked exactly once (no auto-resume/replay loop)"
    );
    let child_wait = child
        .state
        .as_ref()
        .and_then(|s| s.wait.as_ref())
        .expect("child wait record preserved");
    assert!(
        !child_wait.wait_id.is_empty(),
        "child carries its own fresh A4 token"
    );

    // The parent row: waiting_for_input with a fresh token naming the exact
    // waiting child (A4 nested path — a parent token alone is insufficient;
    // the child cursor must be reconstructible).
    let parent_record = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("parent present");
    assert_eq!(parent_record.status, SessionStatus::WaitingForInput);
    let parent_wait = parent_record
        .state
        .as_ref()
        .and_then(|s| s.wait.as_ref())
        .expect("parent wait record present");
    assert!(!parent_wait.wait_id.is_empty(), "fresh parent UUID token");
    assert_eq!(
        parent_wait.child_session_id.as_deref(),
        Some(child.session_id.0.as_str()),
        "root WaitRecord must name the exact waiting child"
    );
    assert_eq!(
        parent_wait.child_task_id.as_deref(),
        Some("manual_wait_task"),
        "root WaitRecord must carry the waiting child's task cursor"
    );

    // Reopen the SAME DB file (daemon restart): parent token + child
    // identity/token survive; the row classifies HumanWait — never stepped
    // at boot.
    pool.close().await;
    let pool_b = nexus_local_db::open_pool(db.path())
        .await
        .expect("reopen pool");
    let storage_b = Arc::new(SqliteSessionStorage::new(pool_b.into()));
    let parent_after = storage_b
        .load_run(&parent_sid)
        .await
        .expect("load_run after reopen")
        .expect("parent present");
    assert_eq!(parent_after.status, SessionStatus::WaitingForInput);
    let wait_after = parent_after
        .state
        .as_ref()
        .and_then(|s| s.wait.as_ref())
        .expect("parent token retained after reopen");
    assert_eq!(
        wait_after.wait_id, parent_wait.wait_id,
        "parent wait token retained across reopen"
    );
    assert_eq!(
        wait_after.child_session_id.as_deref(),
        Some(child.session_id.0.as_str()),
        "child identity retained across reopen"
    );
    assert_eq!(
        wait_after.child_task_id.as_deref(),
        Some("manual_wait_task"),
        "child cursor retained across reopen"
    );

    let children_after = storage_b
        .load_children(&parent_sid)
        .await
        .expect("load_children after reopen");
    assert_eq!(children_after.len(), 1, "child row survives reopen");
    assert_eq!(
        children_after[0].status,
        SessionStatus::WaitingForInput,
        "child status survives reopen unchanged"
    );
    assert_eq!(
        children_after[0]
            .state
            .as_ref()
            .and_then(|s| s.wait.as_ref())
            .map(|w| w.wait_id.as_str()),
        Some(child_wait.wait_id.as_str()),
        "the nested child wait token survives restart untouched (no consume, no clear)"
    );

    // Recovery hydration must reattach the waiting child (not clear it),
    // and the canonical classifier must see a human wait so daemon boot
    // skips the nested wait (A7 rule 4, never auto-stepped).
    let storage_b_dyn: Arc<dyn SessionStorage> = storage_b.clone();
    let storage_b_store: Arc<dyn WorkflowStateStore> = storage_b.clone();
    let engine_after = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_b_dyn,
        storage_b_store,
        nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        )),
    );
    let summary = nexus_orchestration::engine::SessionSummary {
        session_id: parent_sid.clone(),
        creator_id: String::new(),
        preset_id: "novel-writing".to_string(),
        status: SessionStatus::WaitingForInput,
        current_task_id: Some("parent_state".to_string()),
    };
    engine_after.recover_sessions(vec![summary]).await;
    engine_after
        .shared_state()
        .hydrate_children(&parent_sid)
        .await
        .expect("hydrate children after reopen");
    let shared = engine_after.shared_state();
    let children_map = shared.children.read().await;
    let hydrated = children_map
        .get(&parent_sid.0)
        .expect("children hydrated for recovered parent");
    assert_eq!(hydrated.len(), 1, "waiting child reattached, not cleared");
    assert_eq!(
        hydrated[0].status,
        SessionStatus::WaitingForInput,
        "hydrated child stays waiting (no automatic resume)"
    );
    assert_eq!(
        hydrated[0].state.wait.as_ref().map(|w| w.wait_id.as_str()),
        Some(child_wait.wait_id.as_str()),
        "hydrated child carries the SAME nested token"
    );
    drop(children_map);
    drop(shared);

    let context_b = storage_b
        .get(&parent_sid.0)
        .await
        .expect("get parent after reopen")
        .expect("parent present");
    let context_value_b = serde_json::to_value(&context_b.context).expect("context json");
    let chain_class = nexus_orchestration::resume_rules::context_data(&context_value_b)
        .is_some_and(nexus_orchestration::resume_rules::is_converge_merge_chain);
    assert!(!chain_class, "a nested human wait carries no join keys");
    assert_eq!(
        nexus_orchestration::resume_rules::classify_recovery(
            &parent_after.status,
            parent_after.state.as_ref(),
            chain_class,
        ),
        nexus_orchestration::resume_rules::RecoveryClass::HumanWait,
        "a nested child wait reopens as HumanWait (never ConvergeMerge/auto-stepped)"
    );
}
// ---------------------------------------------------------------------------
// 6. graph-flow 0.8 graph-version OCC / clock / corruption (T1 review gaps)
// ---------------------------------------------------------------------------

/// Read the durable `graph_version` straight from the row (test observation,
/// not a storage-API addition).
async fn db_graph_version(pool: &sqlx::SqlitePool, session_id: &str) -> i64 {
    sqlx::query_scalar("SELECT graph_version FROM orchestration_sessions WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(pool)
        .await
        .expect("graph_version row")
}

/// Force a corrupt/exhausted `graph_version` directly (fixture-only write).
async fn force_db_graph_version(pool: &sqlx::SqlitePool, session_id: &str, version: i64) {
    sqlx::query("UPDATE orchestration_sessions SET graph_version = ? WHERE session_id = ?")
        .bind(version)
        .bind(session_id)
        .execute(pool)
        .await
        .expect("force graph_version");
}

/// A corrupt/exhausted graph counter is a HARD storage error, never a
/// concurrency (revision/terminal/conflict) outcome.
fn assert_hard_storage_error(err: &nexus_orchestration::engine::EngineError, what: &str) {
    match err {
        nexus_orchestration::engine::EngineError::GraphFlow(
            graph_flow::GraphError::StorageError(_),
        ) => {}
        other => panic!("{what}: expected hard GraphError::StorageError, got {other:?}"),
    }
}

/// Review gap 1: two independently loaded snapshots at version N — the first
/// save wins, the loser receives `SessionConflict`, the winner's
/// row/context/status/revision remain exact, and a reloaded snapshot at N+1
/// saves successfully.
#[tokio::test]
async fn two_writer_graph_save_occ_loser_conflicts_and_winner_row_unchanged() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-occ-two-writer".to_string());

    let root = root_session(&session_id.0, "task_a");
    storage
        .start_run(
            &session_id,
            &test_descriptor(&session_id.0),
            RunCheckpoint {
                root: &root,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start_run");
    assert_eq!(db_graph_version(&pool, &session_id.0).await, 1);

    // Two independent snapshots at version 1.
    let mut winner = storage
        .get(&session_id.0)
        .await
        .expect("get winner")
        .expect("winner row");
    let mut loser = storage
        .get(&session_id.0)
        .await
        .expect("get loser")
        .expect("loser row");
    assert_eq!(winner.version, 1);
    assert_eq!(loser.version, 1);

    // First writer wins.
    winner.current_task_id = "task_winner".to_string();
    winner.context.set("winner.marker", "won").unwrap();
    storage.save(winner).await.expect("first writer wins");
    assert_eq!(db_graph_version(&pool, &session_id.0).await, 2);

    // Loser's save against the stale preimage conflicts.
    loser.current_task_id = "task_loser".to_string();
    let err = storage.save(loser).await.expect_err("stale writer must lose");
    assert!(
        matches!(err, graph_flow::GraphError::SessionConflict(_)),
        "expected SessionConflict, got {err:?}"
    );

    // Winner's cursor/context/version/revision remain exact.
    let row = storage
        .get(&session_id.0)
        .await
        .expect("get after conflict")
        .expect("row");
    assert_eq!(row.version, 2);
    assert_eq!(row.current_task_id, "task_winner");
    assert_eq!(
        row.context.get::<String>("winner.marker").as_deref(),
        Some("won")
    );
    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("record");
    assert_eq!(
        record.state_revision, 1,
        "a graph save must not touch the workflow control revision"
    );
    assert_eq!(record.status, SessionStatus::Running);

    // A snapshot reloaded at the persisted version saves successfully.
    let mut fresh = storage
        .get(&session_id.0)
        .await
        .expect("get fresh")
        .expect("row");
    fresh.current_task_id = "task_next".to_string();
    storage.save(fresh).await.expect("reloaded save succeeds");
    assert_eq!(db_graph_version(&pool, &session_id.0).await, 3);
}

/// Review gap 2: a negative or exhausted (`i64::MAX`) stored `graph_version`
/// is a hard storage error with NO write across `save` and every
/// authoritative writer — never a revision/terminal/concurrency outcome.
#[tokio::test]
async fn negative_or_max_graph_version_is_hard_storage_error_across_writers() {
    for corrupt in [-1i64, i64::MAX] {
        let (pool, _db) = fresh_pool().await;
        let storage = SqliteSessionStorage::new(pool.clone());
        let session_id = SessionId(format!("sess-corrupt-{corrupt}"));

        let root = root_session(&session_id.0, "task_a");
        storage
            .start_run(
                &session_id,
                &test_descriptor(&session_id.0),
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                &RunStateV1::default(),
            )
            .await
            .expect("start_run");
        force_db_graph_version(&pool, &session_id.0, corrupt).await;

        // save: the corrupt counter is classified BEFORE any concurrency
        // outcome and nothing is written.
        let session = root_session(&session_id.0, "task_save_attempt");
        let err = storage.save(session).await.expect_err("save must refuse");
        assert!(
            matches!(err, graph_flow::GraphError::StorageError(_)),
            "save with corrupt graph_version {corrupt}: expected StorageError, got {err:?}"
        );
        assert_eq!(db_graph_version(&pool, &session_id.0).await, corrupt);

        // commit_transition at the correct revision: hard storage error.
        let root = root_session(&session_id.0, "task_b");
        let err = storage
            .commit_transition(
                &session_id,
                1,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                SessionStatus::Running,
                &RunStateV1::default(),
            )
            .await
            .expect_err("commit_transition must refuse");
        assert_hard_storage_error(&err, "commit_transition");

        // settle_cancelled at the correct revision: hard storage error.
        let root = root_session(&session_id.0, "task_b");
        let err = storage
            .settle_cancelled(
                &session_id,
                1,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                &RunStateV1::default(),
            )
            .await
            .expect_err("settle_cancelled must refuse");
        assert_hard_storage_error(&err, "settle_cancelled");

        // restore_pre_step at the correct revision: hard storage error.
        let pre = root_session(&session_id.0, "task_a");
        let err = storage
            .restore_pre_step(&session_id, 1, &pre)
            .await
            .expect_err("restore_pre_step must refuse");
        assert_hard_storage_error(&err, "restore_pre_step");

        // mark_step_in_flight at the correct revision: hard storage error.
        let root = root_session(&session_id.0, "task_a");
        let in_flight = RunStateV1 {
            step_in_flight: Some("task_a".to_string()),
            ..RunStateV1::default()
        };
        let err = storage
            .mark_step_in_flight(
                &session_id,
                1,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                &in_flight,
            )
            .await
            .expect_err("mark_step_in_flight must refuse");
        assert_hard_storage_error(&err, "mark_step_in_flight");

        // No writer mutated the row: corrupt version, revision, and status
        // are all exactly as seeded.
        assert_eq!(db_graph_version(&pool, &session_id.0).await, corrupt);
        let record = storage
            .load_run(&session_id)
            .await
            .expect("load_run")
            .expect("record");
        assert_eq!(record.state_revision, 1);
        assert_eq!(record.status, SessionStatus::Running);
    }
}

/// Review gap 2 (child path): a corrupt/exhausted CHILD `graph_version` makes
/// the parent commit fail closed with a hard storage error and rolls the
/// parent transition back atomically — never a partial parent advance.
#[tokio::test]
async fn corrupt_child_graph_version_fails_parent_transition_closed() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let parent = SessionId("sess-parent-corrupt-child".to_string());
    let child_id = "sess-parent-corrupt-child:child:1";

    let root = root_session(&parent.0, "task_a");
    let child = child_checkpoint(child_id, "child_task");
    storage
        .start_run(
            &parent,
            &test_descriptor(&parent.0),
            RunCheckpoint {
                root: &root,
                children: &[child],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start_run with child");
    assert_eq!(db_graph_version(&pool, child_id).await, 1);

    // Corrupt the child's graph counter, then run the parent's next
    // transition carrying the child at its correct revision.
    force_db_graph_version(&pool, child_id, i64::MAX).await;
    let root = root_session(&parent.0, "task_b");
    let mut carried = child_checkpoint(child_id, "child_task");
    carried.state_revision = 1;
    carried.session.version = 1;
    let err = storage
        .commit_transition(
            &parent,
            1,
            RunCheckpoint {
                root: &root,
                children: &[carried],
            },
            SessionStatus::Running,
            &RunStateV1::default(),
        )
        .await
        .expect_err("corrupt child counter must fail the parent transition");
    assert_hard_storage_error(&err, "commit_transition with corrupt child");

    // Atomic rollback: the parent's revision/graph clock did not advance and
    // the child's corrupt counter is untouched.
    let record = storage
        .load_run(&parent)
        .await
        .expect("load_run")
        .expect("record");
    assert_eq!(record.state_revision, 1, "failed transition must not advance the root");
    assert_eq!(db_graph_version(&pool, &parent.0).await, 1);
    assert_eq!(db_graph_version(&pool, child_id).await, i64::MAX);
}

/// Review gap 3: the graph-version clock advances exactly once per
/// authoritative cursor/context write (start_run -> mark_step_in_flight ->
/// runner save -> commit_transition -> restore_pre_step -> settle_cancelled)
/// while durable prompt-attempt writes deliberately leave it unchanged.
#[tokio::test]
async fn graph_version_clock_monotonic_across_writers_and_untouched_by_prompt_attempts() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-graph-clock".to_string());

    // start_run seeds graph_version = incoming version (0) + 1 = 1.
    let root = root_session(&session_id.0, "task_a");
    storage
        .start_run(
            &session_id,
            &test_descriptor(&session_id.0),
            RunCheckpoint {
                root: &root,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start_run");
    assert_eq!(db_graph_version(&pool, &session_id.0).await, 1, "start_run seeds 1");

    // mark_step_in_flight: revision 1->2, graph clock 1->2.
    let pre = storage
        .get(&session_id.0)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(pre.version, 1);
    let in_flight = RunStateV1 {
        step_in_flight: Some(pre.current_task_id.clone()),
        ..RunStateV1::default()
    };
    storage
        .mark_step_in_flight(
            &session_id,
            1,
            RunCheckpoint {
                root: &pre,
                children: &[],
            },
            &in_flight,
        )
        .await
        .expect("mark_step_in_flight");
    assert_eq!(db_graph_version(&pool, &session_id.0).await, 2, "mark_step_in_flight advances");

    // The runner loads AFTER the mark, then its graph save advances 2->3.
    let mut loaded = storage
        .get(&session_id.0)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(loaded.version, 2, "runner loads the post-mark version");
    loaded.current_task_id = "task_b".to_string();
    storage.save(loaded).await.expect("runner save");
    assert_eq!(db_graph_version(&pool, &session_id.0).await, 3, "runner save advances");

    // commit_transition anchored at the marker revision (2): revision 2->3,
    // graph clock 3->4.
    let root = storage
        .get(&session_id.0)
        .await
        .expect("get")
        .expect("row");
    storage
        .commit_transition(
            &session_id,
            2,
            RunCheckpoint {
                root: &root,
                children: &[],
            },
            SessionStatus::Running,
            &RunStateV1::default(),
        )
        .await
        .expect("commit_transition");
    assert_eq!(db_graph_version(&pool, &session_id.0).await, 4, "commit_transition advances");

    // Prompt-attempt writes are in-task metadata under the runner-owned
    // session: they must NOT advance the graph clock (brief 3.3).
    let attempt = nexus_orchestration::run_state::PromptAttempt {
        attempt_id: uuid::Uuid::new_v4().to_string(),
        task_id: "task_b".to_string(),
        phase: nexus_orchestration::run_state::PromptPhase::Dispatching,
        host_session_id: None,
        operation_id: None,
        process_identity: None,
    };
    storage
        .persist_prompt_attempt(&session_id, 3, None, None, &attempt)
        .await
        .expect("persist_prompt_attempt");
    assert_eq!(
        db_graph_version(&pool, &session_id.0).await,
        4,
        "prompt-attempt persist must leave the graph clock unchanged"
    );
    storage
        .clear_prompt_attempt(&session_id, 3, None, &attempt.attempt_id)
        .await
        .expect("clear_prompt_attempt");
    assert_eq!(
        db_graph_version(&pool, &session_id.0).await,
        4,
        "prompt-attempt clear must leave the graph clock unchanged"
    );

    // restore_pre_step: fenced restoration advances the graph clock (4->5)
    // while the workflow revision intentionally stays fixed at 3.
    let pre_root = root_session(&session_id.0, "task_a");
    storage
        .restore_pre_step(&session_id, 3, &pre_root)
        .await
        .expect("restore_pre_step");
    assert_eq!(db_graph_version(&pool, &session_id.0).await, 5, "restore_pre_step advances");
    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("record");
    assert_eq!(record.state_revision, 3, "restore keeps the workflow revision");

    // settle_cancelled: revision 3->4, graph clock 5->6, status cancelled.
    let root = storage
        .get(&session_id.0)
        .await
        .expect("get")
        .expect("row");
    storage
        .settle_cancelled(
            &session_id,
            3,
            RunCheckpoint {
                root: &root,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("settle_cancelled");
    assert_eq!(db_graph_version(&pool, &session_id.0).await, 6, "settle_cancelled advances");
    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("record");
    assert_eq!(record.status, SessionStatus::Cancelled);
    assert_eq!(record.state_revision, 4);
}
