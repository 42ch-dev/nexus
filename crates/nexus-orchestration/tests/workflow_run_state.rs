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

        let root = root_session(&session_id.0, "task_a").await;
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .start_run(&session_id, &test_descriptor(&session_id.0), checkpoint, &RunStateV1::default())
            .await
            .expect("start_run");

        // Commit a paused transition.
        let root = root_session(&session_id.0, "task_b").await;
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

    let graph = Arc::new(graph_flow::Graph::new("test-graph"));
    graph.add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask));
    let sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start_session");

    // The run must be a v1 authoritative record, not a bare save.
    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(record.execution_version, 1, "engine start must create a v1 run");
    assert_eq!(record.status, SessionStatus::Running);
    assert!(
        record.descriptor.is_some(),
        "v1 run must carry a frozen descriptor"
    );
    assert_eq!(record.descriptor.as_ref().unwrap().preset_id, "novel-writing");
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

    let graph = Arc::new(graph_flow::Graph::new("test-graph"));
    graph.add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask));
    let sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start_session");

    // Commit a terminal transition directly on the store.
    let root = root_session(&sid.0, "task_z").await;
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

    let graph = Arc::new(graph_flow::Graph::new("test-graph"));
    graph.add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask));
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
    let wait = state.wait.expect("engine-path wait must carry a durable WaitRecord");
    assert!(
        !wait.wait_id.is_empty(),
        "wait token must be a fresh UUID, not empty"
    );
    assert_eq!(wait.kind, nexus_orchestration::run_state::WaitKind::Manual);
}

// Important 1 + 2: child identity is reconstructible and revision-fenced.
#[tokio::test]
async fn child_identity_and_revision_fence() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-fix".to_string());

    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "parent_task").await;
    let child = child_checkpoint("sess-child-fix:child:1", "child_task").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[child],
    };
    storage
        .start_run(&session_id, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start_run with child");

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
        child_descriptor.parent_session_id.as_ref().map(|s| s.0.as_str()),
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
    let root2 = root_session(&session_id.0, "parent_task2").await;
    let child2 = child_checkpoint("sess-child-fix:child:1", "child_task2").await;
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
        let root = root_session(&session_id.0, "task_a").await;
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .start_run(&session_id, &test_descriptor(&session_id.0), checkpoint, &RunStateV1::default())
            .await
            .expect("start_run");

        let root = root_session(&session_id.0, "task_wait").await;
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        storage
            .commit_transition(&session_id, 1, checkpoint, status.clone(), &RunStateV1::default())
            .await
            .expect("commit");

        // A stale graph save must NOT flip the row back to running.
        let stale = root_session(&session_id.0, "task_stale").await;
        storage.save(stale).await.expect("stale save is best-effort");

        let record = storage
            .load_run(&session_id)
            .await
            .expect("load_run")
            .expect("run present");
        assert_eq!(
            record.status, status,
            "stale save must not overwrite {status:?}"
        );
        assert_eq!(record.state_revision, 2, "stale save must not advance revision");
    }
}

// Important 4: commit_transition promotes a v0 row WITH a descriptor to v1
// and returns the authoritative descriptor; a v0 row WITHOUT a descriptor
// stays legacy (cannot be laundered into a valid v1).
#[tokio::test]
async fn commit_transition_promotes_v0_with_descriptor() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());

    // Seed a v0 row WITH a descriptor (execution_version=0 but descriptor
    // present — a row that was started before the v1 migration but carries
    // enough identity to be promoted).
    let descriptor = test_descriptor("sess-v0-promote");
    let descriptor_bytes = serde_json::to_vec(&descriptor).expect("serialize descriptor");
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, run_descriptor_json)
         VALUES ('sess-v0-promote', 'ctr_t', 'preset_t', 7, 'running', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 0, 0, ?)",
    )
    .bind(descriptor_bytes)
    .execute(&*pool)
    .await
    .expect("seed v0 row with descriptor");

    // Commit a transition on the v0 row.
    let root = root_session("sess-v0-promote", "task_z").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    let record = storage
        .commit_transition(
            &SessionId("sess-v0-promote".to_string()),
            0,
            checkpoint,
            SessionStatus::Completed,
            &RunStateV1::default(),
        )
        .await
        .expect("commit on v0 row");

    // The returned record must be authoritative and reconstructible.
    assert_eq!(record.execution_version, 1, "v0 row with descriptor promoted to v1");
    assert_eq!(record.status, SessionStatus::Completed);
    assert_eq!(record.state_revision, 1);
    assert!(
        record.descriptor.is_some(),
        "promoted v1 row must return its descriptor"
    );

    // The persisted row must now be v1 and reloadable.
    let reloaded = storage
        .load_run(&SessionId("sess-v0-promote".to_string()))
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(reloaded.execution_version, 1, "row promoted to v1");
    assert_eq!(reloaded.status, SessionStatus::Completed);
    assert!(
        reloaded.descriptor.is_some(),
        "promoted v1 row must be reconstructible"
    );
}

// Important 4: a v0 row WITHOUT a descriptor stays legacy on transition —
// it cannot be laundered into a valid v1 (A2/A7).
#[tokio::test]
async fn commit_transition_keeps_descriptorless_v0_legacy() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());

    // Seed a v0 row with NO descriptor.
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at)
         VALUES ('sess-v0-nodesc', 'ctr_t', 'preset_t', 7, 'running', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300)",
    )
    .execute(&*pool)
    .await
    .expect("seed v0 row without descriptor");

    // Commit a transition on the v0 row.
    let root = root_session("sess-v0-nodesc", "task_z").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    let record = storage
        .commit_transition(
            &SessionId("sess-v0-nodesc".to_string()),
            0,
            checkpoint,
            SessionStatus::Completed,
            &RunStateV1::default(),
        )
        .await
        .expect("commit on v0 row");

    // Without a descriptor the row cannot be promoted to a valid v1; it
    // stays legacy (execution_version=0) with the terminal status written.
    assert_eq!(record.execution_version, 0, "descriptorless v0 row stays legacy");
    assert_eq!(record.status, SessionStatus::Completed);
    assert!(record.descriptor.is_none());

    // The persisted row stays v0 (legacy evidence preserved, not laundered).
    let reloaded = storage
        .load_run(&SessionId("sess-v0-nodesc".to_string()))
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(reloaded.execution_version, 0, "row stays legacy");
    assert_eq!(reloaded.status, SessionStatus::Completed);
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
        matches!(
            err,
            nexus_orchestration::engine::EngineError::GraphFlow(_)
        ),
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
        matches!(
            err,
            nexus_orchestration::engine::EngineError::GraphFlow(_)
        ),
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
    let root = root_session("sess-legacy", "task_new").await;
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

    let root = root_session(&session_id.0, "task_a").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    storage
        .start_run(&session_id, &test_descriptor(&session_id.0), checkpoint, &RunStateV1::default())
        .await
        .expect("first start_run");

    // A second start_run on the same id must report RunAlreadyExists with
    // execution_version=1 (not a misleading TerminalState).
    let root = root_session(&session_id.0, "task_b").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[],
    };
    let err = storage
        .start_run(&session_id, &test_descriptor(&session_id.0), checkpoint, &RunStateV1::default())
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
    let root = root_session(&session_id.0, "parent_task").await;
    let child = child_checkpoint("sess-child-stale:child:1", "child_task").await;
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
    let root2 = root_session(&session_id.0, "parent_task2").await;
    let child2 = child_checkpoint("sess-child-stale:child:1", "child_task2").await;
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

// Important 1: a child CAS that matches zero rows because the child is
// already terminal must roll back the root transition with TerminalState.
#[tokio::test]
async fn child_cas_terminal_child_rolls_back_root() {
    let (pool, _db) = fresh_pool().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-term".to_string());

    // Start a root run with a child at revision 0.
    let descriptor = test_descriptor(&session_id.0);
    let root = root_session(&session_id.0, "parent_task").await;
    let child = child_checkpoint("sess-child-term:child:1", "child_task").await;
    let checkpoint = RunCheckpoint {
        root: &root,
        children: &[child],
    };
    storage
        .start_run(&session_id, &descriptor, checkpoint, &RunStateV1::default())
        .await
        .expect("start_run with child");

    // Mark the child terminal (completed) — a late root transition must not
    // overwrite a terminal child.
    sqlx::query(
        "UPDATE orchestration_sessions SET status = 'completed'
         WHERE session_id = 'sess-child-term:child:1'",
    )
    .execute(&*pool)
    .await
    .expect("mark child terminal");

    // Commit a root transition with a child checkpoint — the child CAS must
    // fail (terminal fence) and roll back the root.
    let root2 = root_session(&session_id.0, "parent_task2").await;
    let child2 = child_checkpoint("sess-child-term:child:1", "child_task2").await;
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
        .expect_err("terminal child must fail the root transition");

    assert!(
        matches!(
            err,
            nexus_orchestration::engine::EngineError::TerminalState(_)
        ),
        "expected TerminalState for terminal child, got {err:?}"
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

    // The child row must remain terminal.
    let child_after = storage
        .load_run(&SessionId("sess-child-term:child:1".to_string()))
        .await
        .expect("load_run")
        .expect("child present");
    assert_eq!(child_after.status, SessionStatus::Completed);
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
    let graph = Arc::new(graph_flow::Graph::new("test-graph"));
    graph.add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask));
    let parent_sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start parent session");

    // Spawn a child session via the production path.
    let inner_graph = Arc::new(graph_flow::Graph::new("inner_graph"));
    inner_graph.add_task(Arc::new(nexus_orchestration::tasks::InnerGraphNodeTask::new("n1")));
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
        child_descriptor.parent_session_id.as_ref().map(|s| s.0.as_str()),
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
        child_after.descriptor.as_ref().unwrap().graph_name.as_deref(),
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
    let graph = Arc::new(graph_flow::Graph::new("test-graph"));
    graph.add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask));
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
        matches!(
            err,
            nexus_orchestration::engine::EngineError::GraphFlow(_)
        ),
        "expected GraphFlow error for unknown v0 status, got {err:?}"
    );

    // The row must be preserved (not rewritten).
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT status FROM orchestration_sessions WHERE session_id = 'sess-v0-unknown'",
    )
    .fetch_one(&*pool)
    .await
    .expect("read raw status");
    assert_eq!(raw.as_deref(), Some("bogus_status"), "row preserved verbatim");
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
        matches!(
            err,
            nexus_orchestration::engine::EngineError::GraphFlow(_)
        ),
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
