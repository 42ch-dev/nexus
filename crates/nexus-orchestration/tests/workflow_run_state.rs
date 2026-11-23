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
use nexus_contracts::generated::daemon_api::orchestration::workflow_document::WorkflowDocument;
use nexus_orchestration::applied_document::{AppliedDocument, AppliedDocumentSource};
use nexus_orchestration::OrchestrationEngine;
use nexus_preset::source_identity::PresetSourceIdentity;
use nexus_session_storage::run_state::{
    AgentBinding, AppliedRunIdentity, ChildCheckpoint, RunCheckpoint, RunDescriptorV1, RunStateV1,
    TerminalSettlementTarget, WorkflowStateStore,
};
use nexus_session_storage::storage::sqlite::SqliteSessionStorage;
use nexus_session_storage::StorageError as EngineError;
use nexus_session_storage::{SessionId, SessionStatus};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

mod common;

use common::arc_pool_file_first;

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
        applied: None,
        intake_mode: None,
    }
}

/// Build a root session snapshot.
fn root_session(session_id: &str, task: &str) -> Session {
    let s = Session::new_from_task(session_id.to_string(), task);
    s.context
        .set("_session_id", session_id.to_string())
        .unwrap();
    s
}

/// Build a child checkpoint.
fn child_checkpoint(child_id: &str, task: &str) -> ChildCheckpoint {
    let s = Session::new_from_task(child_id.to_string(), task);
    s.context.set("_session_id", child_id.to_string()).unwrap();
    ChildCheckpoint {
        session: s,
        status: SessionStatus::Running,
        state: RunStateV1::default(),
        state_revision: 0,
        graph_name: Some("inner_graph".to_string()),
    }
}

#[tokio::test]
async fn terminal_status_survives_reopen_not_disguised_as_running() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let session_id = SessionId("sess-reopen".to_string());

    {
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (first)");
        let pool = guarded.clone_pool();
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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (second)");
        let pool = guarded.clone_pool();
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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (first)");
        let pool = guarded.clone_pool();
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

        let wait_state = RunStateV1 {
            wait: Some(nexus_session_storage::run_state::WaitRecord {
                wait_id: "wait-abc".to_string(),
                task_id: "wait_task".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: nexus_session_storage::run_state::WaitKind::Manual,
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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (second)");
        let pool = guarded.clone_pool();
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

#[tokio::test]
async fn stale_graph_save_cannot_overwrite_terminal_status() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-stale".to_string());

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
    let (_db, pool) = arc_pool_file_first().await;
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
        EngineError::RevisionMismatch {
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
    let (_db, pool) = arc_pool_file_first().await;
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
        matches!(err, EngineError::TerminalState(_)),
        "expected TerminalState, got {err:?}"
    );
}

#[tokio::test]
async fn descriptor_and_child_identity_preserved() {
    let (_db, pool) = arc_pool_file_first().await;
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

    let child_row = storage
        .get("sess-child:child:1")
        .await
        .expect("child session present")
        .expect("child row");
    assert_eq!(child_row.current_task_id, "child_task");
}

/// The applied identity is frozen on the descriptor, and a run with no apply
/// keeps writing exactly the bytes it always did.
#[test]
fn descriptor_serializes_the_applied_identity_only_when_present() {
    let no_apply_descriptor = test_descriptor("sess");
    let no_apply = serde_json::to_value(&no_apply_descriptor).expect("descriptor serializes");
    assert!(
        no_apply.get("applied").is_none(),
        "a no-apply descriptor must not gain a key: {no_apply}"
    );
    assert_eq!(
        serde_json::to_string(&no_apply_descriptor).expect("descriptor serializes"),
        concat!(
            r#"{"creator_id":"ctr_test","work_id":"work-1","workspace_root":"/tmp/ws","#,
            r#""preset_id":"test-preset","preset_version":3,"#,
            r#""source":{"Embedded":{"preset_id":"test-preset","content_hash":"#,
            "[7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7]}},",
            r#""input":{"topic":"durable"},"#,
            r#""agent_bindings":{"default":{"provider_id":"test-provider","model":"test-model"}},"#,
            r#""parent_session_id":null,"graph_name":null}"#,
        )
    );

    let mut applied = test_descriptor("sess");
    applied.applied = Some(nexus_session_storage::run_state::AppliedRunIdentity {
        apply_id: "apl_1".to_string(),
        content_hash: "blake3:abc".to_string(),
    });
    let value = serde_json::to_value(&applied).expect("descriptor serializes");
    assert_eq!(
        value["applied"]["content_hash"],
        serde_json::json!("blake3:abc")
    );
    let back: RunDescriptorV1 = serde_json::from_value(value).expect("descriptor decodes");
    assert_eq!(back.applied, applied.applied);
}

/// The intake mode is frozen on the descriptor only when an admission recorded
/// one, the one remaining arm round-trips under its kebab-case spelling, and a
/// legacy descriptor keeps its exact bytes.
#[test]
fn descriptor_round_trips_intake_mode_and_legacy_bytes_hold() {
    use nexus_home_layout::strategy_intake::StrategyIntakeMode;

    let legacy = test_descriptor("sess");
    assert_eq!(legacy.intake_mode, None);
    let legacy_value = serde_json::to_value(&legacy).expect("descriptor serializes");
    assert!(
        legacy_value.get("intake_mode").is_none(),
        "a legacy descriptor must not gain a key: {legacy_value}"
    );

    let mut descriptor = test_descriptor("sess");
    descriptor.intake_mode = Some(StrategyIntakeMode::IrOnly);
    let value = serde_json::to_value(&descriptor).expect("descriptor serializes");
    let back: RunDescriptorV1 = serde_json::from_value(value.clone()).expect("descriptor decodes");
    assert_eq!(back.intake_mode, Some(StrategyIntakeMode::IrOnly));
    assert!(
        value.get("intake_mode").is_some(),
        "a recorded mode must serialize a key: {value}"
    );

    let json = serde_json::to_string(&descriptor).expect("descriptor serializes");
    assert!(
        json.contains(r#""intake_mode":"ir-only""#),
        "ir-only must serialize kebab-case: {json}"
    );
}

/// A run admitted while `yaml-only` was live recorded that spelling on its
/// descriptor. Recovery must refuse it by name — not as a bare unknown-variant
/// parse error, and never as `IrOnly`: mapping it would silently switch the
/// document source the run executes, which is the outcome the arm's removal
/// refuses (I-000509).
#[tokio::test]
async fn recorded_retired_intake_mode_refuses_by_name() {
    let mut recorded =
        serde_json::to_value(test_descriptor("sess")).expect("descriptor serializes");
    recorded["intake_mode"] = serde_json::json!("yaml-only");

    let error = serde_json::from_value::<RunDescriptorV1>(recorded.clone())
        .expect_err("a recorded retired intake mode must not decode");
    let message = error.to_string();
    assert!(
        message.contains("intake_mode_retired"),
        "the refusal is a named one: {message}"
    );
    assert!(
        message.contains("'yaml-only'"),
        "the refusal names the recorded value: {message}"
    );

    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES ('sess-yaml-only', 'ctr_t', 'test-preset', 3, 'running', 'task_a',
                 '{\"data\":{}}', 1756990000, 1756990300,
                 1, 0, ?, ?)",
    )
    .bind(serde_json::to_vec(&RunStateV1::default()).expect("state serializes"))
    .bind(serde_json::to_vec(&recorded).expect("descriptor serializes"))
    .execute(&*pool)
    .await
    .expect("seed a descriptor recorded under the retired arm");

    let err = storage
        .load_run(&SessionId("sess-yaml-only".to_string()))
        .await
        .expect_err("a recorded retired intake mode is not replayable");
    assert!(
        format!("{err:?}").contains("intake_mode_retired"),
        "recovery names the retired mode: {err:?}"
    );
}

#[tokio::test]
async fn v0_legacy_rows_stay_legacy_unverified() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());

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
    assert_eq!(record.status, SessionStatus::Running);
}

#[tokio::test]
async fn corrupt_blobs_preserved_not_reinterpreted() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());

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

    let err = storage
        .load_run(&SessionId("sess-corrupt".to_string()))
        .await
        .expect_err("corrupt blob must not be silently reinterpreted");

    assert!(
        matches!(err, EngineError::GraphFlow(_)),
        "expected GraphFlow error for corrupt blob, got {err:?}"
    );

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

#[tokio::test]
async fn old_migrated_db_upgrades_cleanly() {
    const RUNNER_CUTOVER_MIGRATION: &str = "20260918000001_holder_registry.sql";

    let db = tempfile::NamedTempFile::new().unwrap();
    let raw = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.path().display()))
        .await
        .expect("raw pool");

    let old_migrations_dir = tempfile::tempdir().unwrap();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/nexus-local-db/migrations");
    for entry in std::fs::read_dir(&src).expect("read migrations dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "20260907000001_workflow_execution_state.sql" || name == RUNNER_CUTOVER_MIGRATION
        {
            continue;
        }
        std::fs::copy(entry.path(), old_migrations_dir.path().join(name)).expect("copy migration");
    }
    let old_migrator = sqlx::migrate::Migrator::new(old_migrations_dir.path())
        .await
        .expect("build old migrator");
    old_migrator.run(&raw).await.expect("apply old migrations");

    let has_new_col: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('orchestration_sessions')
         WHERE name = 'execution_version'",
    )
    .fetch_one(&raw)
    .await
    .expect("check column");
    assert_eq!(has_new_col, 0, "old DB must lack the new column");
    raw.close().await;

    let guarded = nexus_local_db::init_engine_pool(db.path())
        .await
        .expect("open pool");
    let pool = guarded.clone_pool();

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

#[tokio::test]
async fn paused_status_survives_reopen() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let session_id = SessionId("sess-paused-reopen".to_string());

    {
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (first)");
        let pool = guarded.clone_pool();
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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (second)");
        let pool = guarded.clone_pool();
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

#[tokio::test]
async fn engine_start_creates_v1_run() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_capability::CapabilityRegistry::with_builtins(),
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

#[tokio::test]
async fn engine_new_session_creates_v1_run() {
    let (_db, pool) = arc_pool_file_first().await;
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

#[tokio::test]
async fn resume_after_terminal_is_refused() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_capability::CapabilityRegistry::with_builtins(),
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

    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    assert_eq!(record.status, SessionStatus::Completed);
}

#[tokio::test]
async fn engine_wait_produces_durable_wait_token() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_capability::CapabilityRegistry::with_builtins(),
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
    assert_eq!(record.status, SessionStatus::WaitingForInput);
    let state = record.state.expect("v1 state present");
    let wait = state
        .wait
        .expect("engine-path wait must carry a durable WaitRecord");
    assert!(
        !wait.wait_id.is_empty(),
        "wait token must be a fresh UUID, not empty"
    );
    assert_eq!(
        wait.kind,
        nexus_session_storage::run_state::WaitKind::Manual
    );

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
    let (_db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_capability::CapabilityRegistry::with_builtins(),
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
            None,
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
    assert!(matches!(stale, EngineError::RevisionMismatch { .. }));
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

#[tokio::test]
async fn child_identity_and_revision_fence() {
    let (_db, pool) = arc_pool_file_first().await;
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

#[tokio::test]
async fn stale_save_cannot_overwrite_wait_or_paused() {
    let (_db, pool) = arc_pool_file_first().await;
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

#[tokio::test]
async fn commit_transition_refuses_v0_rows_without_mutation() {
    for (session_id, descriptor) in [
        (
            "sess-v0-with-desc",
            Some(test_descriptor("sess-v0-with-desc")),
        ),
        ("sess-v0-no-desc", None),
    ] {
        let (_db, pool) = arc_pool_file_first().await;
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

#[tokio::test]
async fn unknown_v1_status_is_non_replayable() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());

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

    let err = storage
        .load_run(&SessionId("sess-unknown-status".to_string()))
        .await
        .expect_err("unknown v1 status must be non-replayable");
    assert!(
        matches!(err, EngineError::GraphFlow(_)),
        "expected GraphFlow error for unknown status, got {err:?}"
    );
}

#[tokio::test]
async fn v1_row_missing_metadata_is_non_replayable() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());

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
        matches!(err, EngineError::GraphFlow(_)),
        "expected GraphFlow error, got {err:?}"
    );
}

#[test]
fn wait_kind_and_prompt_phase_encode_snake_case() {
    let wait = nexus_session_storage::run_state::WaitRecord {
        wait_id: "w".to_string(),
        task_id: "t".to_string(),
        child_session_id: None,
        child_task_id: None,
        kind: nexus_session_storage::run_state::WaitKind::Manual,
    };
    let json = serde_json::to_value(&wait).expect("serialize wait");
    assert_eq!(
        json["kind"], "manual",
        "WaitKind::Manual must encode as 'manual', got {}",
        json["kind"]
    );

    let attempt = nexus_session_storage::run_state::PromptAttempt {
        attempt_id: "a".to_string(),
        task_id: "t".to_string(),
        phase: nexus_session_storage::run_state::PromptPhase::Dispatching,
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

    let active = nexus_session_storage::run_state::PromptAttempt {
        phase: nexus_session_storage::run_state::PromptPhase::Active,
        ..attempt
    };
    let json = serde_json::to_value(&active).expect("serialize active");
    assert_eq!(
        json["phase"], "active",
        "PromptPhase::Active must encode as 'active', got {}",
        json["phase"]
    );
}

#[tokio::test]
async fn start_run_refuses_existing_v0_row() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());

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
        EngineError::RunAlreadyExists {
            session_id,
            execution_version,
        } => {
            assert_eq!(session_id, "sess-legacy");
            assert_eq!(execution_version, 0, "existing row is v0 legacy");
        }
        other => panic!("expected RunAlreadyExists, got {other:?}"),
    }

    let record = storage
        .load_run(&SessionId("sess-legacy".to_string()))
        .await
        .expect("load_run")
        .expect("row present");
    assert_eq!(record.execution_version, 0, "legacy row not laundered");
    assert_eq!(record.status, SessionStatus::Running);
}

#[tokio::test]
async fn start_run_reports_accurate_conflict_for_v1_row() {
    let (_db, pool) = arc_pool_file_first().await;
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
        EngineError::RunAlreadyExists {
            session_id: sid,
            execution_version,
        } => {
            assert_eq!(sid, "sess-already-v1");
            assert_eq!(execution_version, 1, "existing row is already v1");
        }
        other => panic!("expected RunAlreadyExists, got {other:?}"),
    }
}

#[tokio::test]
async fn engine_start_with_loaded_preset_builds_source_identity() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_capability::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );

    let registry = nexus_capability::CapabilityRegistry::with_builtins();
    let loaded = nexus_preset::load_embedded_preset("novel-writing", &registry)
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

#[tokio::test]
async fn child_cas_stale_revision_rolls_back_root() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-stale".to_string());

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

    sqlx::query(
        "UPDATE orchestration_sessions SET state_revision = 1
         WHERE session_id = 'sess-child-stale:child:1'",
    )
    .execute(&*pool)
    .await
    .expect("advance child revision");

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
        matches!(err, EngineError::RevisionMismatch { .. }),
        "expected RevisionMismatch for stale child, got {err:?}"
    );

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

    let child_after = storage
        .load_run(&SessionId("sess-child-stale:child:1".to_string()))
        .await
        .expect("load_run")
        .expect("child present");
    assert_eq!(child_after.state_revision, 1, "child row not overwritten");
}

#[tokio::test]
async fn child_cas_unknown_status_is_storage_error_even_with_revision_mismatch() {
    let (_db, pool) = arc_pool_file_first().await;
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
    assert_eq!(
        root_after.state_revision, 1,
        "root transition must roll back"
    );
}

#[tokio::test]
async fn child_cas_terminal_child_rolls_back_root() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-term".to_string());

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

    sqlx::query(
        "UPDATE orchestration_sessions SET status = 'completed', state_revision = 1
         WHERE session_id = 'sess-child-term:child:1'",
    )
    .execute(&*pool)
    .await
    .expect("mark child terminal + advance revision");

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
        matches!(err, EngineError::TerminalState(_)),
        "expected TerminalState for terminal child at stale revision, got {err:?}"
    );

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

    let child_after = storage
        .load_run(&SessionId("sess-child-term:child:1".to_string()))
        .await
        .expect("load_run")
        .expect("child present");
    assert_eq!(child_after.status, SessionStatus::Completed);
    assert_eq!(child_after.state_revision, 1);
}

#[tokio::test]
async fn child_cas_terminal_at_expected_revision_is_confirmed() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-term-confirm".to_string());

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

    sqlx::query(
        "UPDATE orchestration_sessions SET status = 'completed'
         WHERE session_id = 'sess-child-term-confirm:child:1'",
    )
    .execute(&*pool)
    .await
    .expect("mark child terminal");

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

    let child_after = storage
        .load_run(&SessionId("sess-child-term-confirm:child:1".to_string()))
        .await
        .expect("load_run")
        .expect("child present");
    assert_eq!(child_after.status, SessionStatus::Completed);
    assert_eq!(child_after.state_revision, 0, "child row not overwritten");
}

#[tokio::test]
async fn engine_nested_child_creates_v1_run() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_capability::CapabilityRegistry::with_builtins(),
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
    let parent_sid = engine
        .start_session("novel-writing", graph)
        .await
        .expect("start parent session");

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

    let outcome = engine.run_step(&parent_sid).await.expect("run_step parent");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::WaitingForInput { .. }
        ),
        "expected WaitingForInput, got {outcome:?}"
    );

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

#[tokio::test]
async fn shadowed_directory_preset_resolves_directory_source_and_version() {
    use std::fs;

    let (_db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_capability::CapabilityRegistry::with_builtins(),
    ));
    let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );

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

#[tokio::test]
async fn unknown_v0_status_is_non_replayable() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());

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

    let err = storage
        .load_run(&SessionId("sess-v0-unknown".to_string()))
        .await
        .expect_err("unknown v0 status must be non-replayable");
    assert!(
        matches!(err, EngineError::GraphFlow(_)),
        "expected GraphFlow error for unknown v0 status, got {err:?}"
    );

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

#[tokio::test]
async fn negative_v0_revision_is_non_replayable() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());

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

    let err = storage
        .load_run(&SessionId("sess-v0-negrev".to_string()))
        .await
        .expect_err("negative v0 revision must be non-replayable");
    assert!(
        matches!(err, EngineError::GraphFlow(_)),
        "expected GraphFlow error for negative revision, got {err:?}"
    );

    let raw: i64 = sqlx::query_scalar(
        "SELECT state_revision FROM orchestration_sessions WHERE session_id = 'sess-v0-negrev'",
    )
    .fetch_one(&*pool)
    .await
    .expect("read raw revision");
    assert_eq!(raw, -5, "row preserved verbatim");
}

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
    let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_capability::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );
    (storage, engine)
}

/// A real graph-only save interleaved after an engine snapshot read.
struct GraphSaveAfterRead {
    inner: Arc<SqliteSessionStorage>,
    reads_until_save: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl SessionStorage for GraphSaveAfterRead {
    async fn save(&self, session: Session) -> graph_flow::Result<()> {
        self.inner.save(session).await
    }

    async fn get(&self, id: &str) -> graph_flow::Result<Option<Session>> {
        use std::sync::atomic::Ordering;
        let snapshot = self.inner.get(id).await?;
        if self
            .reads_until_save
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            == Ok(1)
        {
            let winner = self
                .inner
                .get(id)
                .await?
                .ok_or_else(|| graph_flow::GraphError::SessionNotFound(id.to_string()))?;
            winner.context.set("concurrent_graph_output", "preserved")?;
            self.inner.save(winner).await?;
        }
        Ok(snapshot)
    }

    async fn delete(&self, id: &str) -> graph_flow::Result<()> {
        self.inner.delete(id).await
    }
}

#[tokio::test]
async fn ordinary_cancel_rebases_graph_winners_at_intent_and_settlement() {
    use nexus_orchestration::engine::EngineSignal;
    use std::sync::atomic::{AtomicUsize, Ordering};
    for (boundary, snapshot_read) in [("cancel intent", 2), ("cancel settlement", 3)] {
        let (_db, pool) = arc_pool_file_first().await;
        let storage = Arc::new(SqliteSessionStorage::new(pool));
        let racing = Arc::new(GraphSaveAfterRead {
            inner: storage.clone(),
            reads_until_save: AtomicUsize::new(0),
        });
        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_capability::CapabilityRegistry::with_builtins(),
        ));
        let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            racing.clone(),
            storage.clone(),
            caps,
        );
        let graph = Arc::new(
            graph_flow::GraphBuilder::new("cancel-graph-race")
                .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let sid = engine.start_session("novel-writing", graph).await.unwrap();
        racing
            .reads_until_save
            .store(snapshot_read, Ordering::SeqCst);
        engine.signal(&sid, EngineSignal::Cancel).await.unwrap();
        let record = storage.load_run(&sid).await.unwrap().unwrap();
        assert_eq!(record.status, SessionStatus::Cancelled, "{boundary}");
        let saved = storage.get(&sid.0).await.unwrap().unwrap();
        assert_eq!(
            saved
                .context
                .get::<String>("concurrent_graph_output")
                .as_deref(),
            Some("preserved"),
            "{boundary} must retain the graph writer's output"
        );
    }
}

#[tokio::test]
async fn ordinary_pause_refuses_a_stale_graph_snapshot() {
    use nexus_orchestration::engine::{EngineError, EngineSignal};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (_db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool));
    let racing = Arc::new(GraphSaveAfterRead {
        inner: storage.clone(),
        reads_until_save: AtomicUsize::new(0),
    });
    let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_capability::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        racing.clone(),
        storage.clone(),
        caps,
    );
    let graph = Arc::new(
        graph_flow::GraphBuilder::new("pause-graph-race")
            .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
            .build()
            .expect("test graph"),
    );
    let sid = engine.start_session("novel-writing", graph).await.unwrap();
    racing.reads_until_save.store(1, Ordering::SeqCst);
    let error = engine
        .signal(&sid, EngineSignal::Pause)
        .await
        .expect_err("a non-rebasing control signal must lose a stale graph snapshot");
    assert!(matches!(error, EngineError::RevisionMismatch { .. }));
    assert_eq!(
        storage.load_run(&sid).await.unwrap().unwrap().status,
        SessionStatus::Running
    );
    assert_eq!(
        storage
            .get(&sid.0)
            .await
            .unwrap()
            .unwrap()
            .context
            .get::<String>("concurrent_graph_output")
            .as_deref(),
        Some("preserved")
    );
}

#[tokio::test]
async fn engine_nested_child_runs_to_completion_then_parent_commits() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool);

    let inner_graph = Arc::new(
        graph_flow::GraphBuilder::new("inner_graph")
            .add_task(Arc::new(EndTask))
            .build()
            .expect("test graph"),
    );

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

    let outcome = engine.run_step(&parent_sid).await.expect("run_step parent");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::Paused { .. }
        ),
        "expected Paused, got {outcome:?}"
    );

    let parent_record = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("parent present");
    assert_eq!(
        parent_record.state_revision, 3,
        "in-flight marker and parent transition both advance the revision"
    );

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
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());

    let inner_graph = Arc::new(
        graph_flow::GraphBuilder::new("inner_graph")
            .add_task(Arc::new(EndTask))
            .build()
            .expect("test graph"),
    );

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

    let child_sid = engine
        .shared_state()
        .spawn_child_session_internal(nexus_orchestration::engine::ChildSessionParams {
            parent_session_id: parent_sid.0.clone(),
            inner_graph: inner_graph.clone(),
            initial_context: graph_flow::Context::new(),
        })
        .await
        .expect("spawn child");

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

    let err = engine
        .shared_state()
        .attach_existing_child_session_internal(&parent_sid.0, inner_graph.clone())
        .await
        .expect_err("tampered child must not be attached");
    assert!(
        err.to_string().contains("non-replayable"),
        "attachment refusal must be non-replayable, got {err}"
    );

    let err = engine
        .shared_state()
        .hydrate_children(&parent_sid)
        .await
        .expect_err("tampered child must fail hydration");
    assert!(
        err.to_string().contains("non-replayable"),
        "hydration refusal must be non-replayable, got {err}"
    );

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
    let (_db, pool) = arc_pool_file_first().await;
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
    let (_db, pool) = arc_pool_file_first().await;
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

/// Seed that persisted shape. `marker_states` are the outer states whose
/// `_inner_child_session_<state>` records name the child; `child_inner_graph`
/// is the inner graph the child's frozen descriptor names.
async fn seed_abandoned_child_root(
    pool: &sqlx::SqlitePool,
    storage: &Arc<SqliteSessionStorage>,
    engine: &nexus_orchestration::GraphFlowEngine,
    marker_states: &[&str],
    child_inner_graph: &str,
) -> (SessionId, SessionId) {
    let inner_graph = Arc::new(
        graph_flow::GraphBuilder::new(child_inner_graph)
            .add_task(Arc::new(EndTask))
            .build()
            .expect("test inner graph"),
    );
    let parent_graph = Arc::new(
        graph_flow::GraphBuilder::new("parent_graph")
            .add_task(Arc::new(EndTask))
            .build()
            .expect("test parent graph"),
    );
    let root_sid = engine
        .start_session("novel-project-init", parent_graph)
        .await
        .expect("start root session");
    let child_sid = engine
        .shared_state()
        .spawn_child_session_internal(nexus_orchestration::engine::ChildSessionParams {
            parent_session_id: root_sid.0.clone(),
            inner_graph,
            initial_context: graph_flow::Context::new(),
        })
        .await
        .expect("spawn child through production admission");

    let root = storage
        .get(&root_sid.0)
        .await
        .expect("get root")
        .expect("root row");
    for state in marker_states {
        root.context
            .set(format!("_inner_child_session_{state}"), child_sid.0.clone())
            .expect("write root marker");
    }
    sqlx::query(
        "UPDATE orchestration_sessions SET context_json = ?, current_task_id = 'committing'
         WHERE session_id = ?",
    )
    .bind(serde_json::to_vec(&root.context).expect("serialize root context"))
    .bind(&root_sid.0)
    .execute(pool)
    .await
    .expect("persist root marker and advanced cursor");
    (root_sid, child_sid)
}

/// The root moved past `init_intro` (which enters `intro_graph`) and its
/// record still names the abandoned child whose descriptor names
/// `intro_graph`: recovery must accept the row, and must not mint a
/// replacement child.
#[tokio::test]
async fn recovery_child_marker_reattaches_abandoned_child_without_replacement() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());
    let (root_sid, _child_sid) =
        seed_abandoned_child_root(&pool, &storage, &engine, &["init_intro"], "intro_graph").await;

    let (_storage_b, engine_b) = fresh_engine(pool.clone());
    engine_b
        .ensure_recovered_runner_inner(&root_sid)
        .await
        .expect("a marker-named child past the root position must stay replayable");

    let children = storage
        .load_children(&root_sid)
        .await
        .expect("load children");
    assert_eq!(
        children.len(),
        1,
        "recovery must not mint a replacement child"
    );
}

/// The record's state resolves to a DIFFERENT legal inner graph than the
/// child's descriptor names, so it is not evidence of this child's
/// reattachment: recovery rejects it and mints no replacement.
#[tokio::test]
async fn recovery_child_marker_for_another_inner_graph_is_non_replayable() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());
    let (root_sid, _child_sid) = seed_abandoned_child_root(
        &pool,
        &storage,
        &engine,
        &["collecting_details"],
        "intro_graph",
    )
    .await;

    let (_storage_b, engine_b) = fresh_engine(pool.clone());
    let err = engine_b
        .ensure_recovered_runner_inner(&root_sid)
        .await
        .expect_err("a record for another inner graph must be non-replayable");
    assert!(
        err.to_string()
            .contains("not recorded by an inner-graph state"),
        "refusal must come from the marker branch, got {err}"
    );

    let children = storage
        .load_children(&root_sid)
        .await
        .expect("load children");
    assert_eq!(
        children.len(),
        1,
        "rejection must not mint a replacement child"
    );
}

/// Two records name the SAME child but resolve to two different legal inner
/// graphs, so the record set is ambiguous and counts as no evidence at all:
/// recovery rejects it and mints no replacement.
#[tokio::test]
async fn recovery_child_marker_conflict_is_non_replayable() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());
    let (root_sid, _child_sid) = seed_abandoned_child_root(
        &pool,
        &storage,
        &engine,
        &["init_intro", "collecting_details"], // intro_graph vs details_graph
        "intro_graph",
    )
    .await;

    let (_storage_b, engine_b) = fresh_engine(pool.clone());
    let err = engine_b
        .ensure_recovered_runner_inner(&root_sid)
        .await
        .expect_err("conflicting records for one child must be non-replayable");
    assert!(
        err.to_string()
            .contains("not recorded by an inner-graph state"),
        "refusal must come from the marker branch, got {err}"
    );

    let children = storage
        .load_children(&root_sid)
        .await
        .expect("load children");
    assert_eq!(
        children.len(),
        1,
        "rejection must not mint a replacement child"
    );
}

#[tokio::test]
async fn restart_hydrates_child_checkpoints() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let parent_sid;
    let child_sid;

    {
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (first)");
        let pool = guarded.clone_pool();
        let (storage, engine) = fresh_engine(Arc::new(pool));

        let inner_graph = Arc::new(
            graph_flow::GraphBuilder::new("inner_graph")
                .add_task(Arc::new(EndTask))
                .build()
                .expect("test graph"),
        );
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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (second)");
        let pool = guarded.clone_pool();
        let (_storage, engine) = fresh_engine(Arc::new(pool));

        let summary = nexus_session_storage::SessionSummary {
            session_id: parent_sid.clone(),
            creator_id: String::new(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Paused,
            current_task_id: Some("parent_state".to_string()),
        };
        engine.recover_sessions(vec![summary]).await;

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

#[tokio::test]
async fn failed_child_cas_restores_root_position() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());

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

    let pre_step = storage
        .get(&parent_sid.0)
        .await
        .expect("get pre-step root")
        .expect("root present");
    let pre_task = pre_step.current_task_id.clone();
    let pre_ctx = serde_json::to_vec(&pre_step.context).expect("serialize pre-step context");

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
    let advanced: Option<bool> = after.context.get("advanced");
    assert_eq!(
        advanced, None,
        "context marker set during the failed step must be restored away"
    );
}

#[tokio::test]
async fn child_ids_are_collision_resistant() {
    let (_db, pool) = arc_pool_file_first().await;
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
        ctx.set(nexus_orchestration::engine::EXTERNAL_EFFECT_MARKER, true)
            .unwrap();
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

#[tokio::test]
async fn corrupt_child_is_non_replayable_during_recovery() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());
    let parent_sid = SessionId("par-corrupt-child".to_string());

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

    let err = engine
        .shared_state()
        .hydrate_children(&parent_sid)
        .await
        .expect_err("corrupt child must be non-replayable");
    assert!(
        matches!(err, nexus_orchestration::engine::EngineError::GraphFlow(_)),
        "expected GraphFlow error for corrupt child, got {err:?}"
    );

    let shared = engine.shared_state();
    let map = shared.children.read().await;
    assert!(
        !map.contains_key(&parent_sid.0),
        "corrupt child must not populate the children map"
    );
    drop(map);

    let summary = nexus_session_storage::SessionSummary {
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

#[tokio::test]
async fn child_missing_session_snapshot_is_non_replayable() {
    let (_db, pool) = arc_pool_file_first().await;
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

#[tokio::test]
async fn failed_cas_restore_is_revision_fenced() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());

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

    let pre_step_root = storage
        .get(&parent_sid.0)
        .await
        .expect("get pre-step root")
        .expect("root present");
    let pre_task = pre_step_root.current_task_id;

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

#[tokio::test]
async fn post_effect_failure_persists_interrupted() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());

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

#[tokio::test]
async fn deterministic_failure_restores_pre_step_position() {
    let (_db, pool) = arc_pool_file_first().await;
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

#[tokio::test]
async fn recovered_parent_drives_existing_inner_child() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let parent_sid;
    let existing_child_sid;

    {
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (first)");
        let pool = guarded.clone_pool();
        let (_storage, engine) = fresh_engine(Arc::new(pool));

        let inner_graph = Arc::new(
            graph_flow::GraphBuilder::new("ig")
                .add_task(Arc::new(EndTask))
                .build()
                .expect("test graph"),
        );
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
    } // pool drops — simulates daemon shutdown

    {
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (second)");
        let pool = guarded.clone_pool();
        let (storage, engine) = fresh_engine(Arc::new(pool));

        engine
            .shared_state()
            .hydrate_children(&parent_sid)
            .await
            .expect("hydrate children map");

        let inner_graph = Arc::new(
            graph_flow::GraphBuilder::new("ig")
                .add_task(Arc::new(EndTask))
                .build()
                .expect("test graph"),
        );
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

#[tokio::test]
async fn directory_preset_frozen_source_reconstruction() {
    use std::fs;

    let db = tempfile::NamedTempFile::new().unwrap();
    let parent_sid;
    let nexus_home_dir = tempfile::tempdir().unwrap();
    let bundle_dir = nexus_home_dir.path().join("presets").join("dir-preset");

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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (first)");
        let pool = guarded.clone_pool();
        let storage = Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
        let storage_arc: Arc<dyn SessionStorage> = storage.clone();
        let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_capability::CapabilityRegistry::with_builtins(),
        ));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage_arc,
            workflow_store,
            caps,
        );
        engine.set_nexus_home(nexus_home_dir.path().to_path_buf());

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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (second)");
        let pool = guarded.clone_pool();
        let storage = Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
        let storage_arc: Arc<dyn SessionStorage> = storage.clone();
        let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_capability::CapabilityRegistry::with_builtins(),
        ));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage_arc,
            workflow_store,
            caps,
        );
        engine.set_nexus_home(nexus_home_dir.path().to_path_buf());

        let summary = nexus_session_storage::SessionSummary {
            session_id: parent_sid.clone(),
            creator_id: String::new(),
            preset_id: "dir-preset".to_string(),
            status: SessionStatus::Paused,
            current_task_id: Some("a".to_string()),
        };
        engine.shared_state().runners.write().await.clear();
        engine.recover_sessions(vec![summary]).await;

        assert!(
            engine.has_runner(&parent_sid).await,
            "directory preset must reconstruct a runner from the frozen source"
        );
    }

    {
        let changed_yaml = initial_yaml.replace("version: 11", "version: 99");
        fs::write(bundle_dir.join("preset.yaml"), changed_yaml).unwrap();

        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (third)");
        let pool = guarded.clone_pool();
        let storage = Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
        let storage_arc: Arc<dyn SessionStorage> = storage.clone();
        let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_capability::CapabilityRegistry::with_builtins(),
        ));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage_arc,
            workflow_store,
            caps,
        );
        engine.set_nexus_home(nexus_home_dir.path().to_path_buf());

        let summary = nexus_session_storage::SessionSummary {
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

#[tokio::test]
async fn negative_v1_revision_is_non_replayable() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());

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
        matches!(err, EngineError::GraphFlow(_)),
        "expected GraphFlow error for negative v1 revision, got {err:?}"
    );

    let raw: i64 = sqlx::query_scalar(
        "SELECT state_revision FROM orchestration_sessions WHERE session_id = 'sess-v1-negrev'",
    )
    .fetch_one(&*pool)
    .await
    .expect("read raw revision");
    assert_eq!(raw, -9, "row preserved verbatim");
}

#[tokio::test]
async fn child_terminal_status_mismatch_rejects_confirmation() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-child-mismatch".to_string());

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
        matches!(err, EngineError::GraphFlow(_)),
        "expected GraphFlow mismatch error, got {err:?}"
    );

    let root_after = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("root present");
    assert_ne!(root_after.status, SessionStatus::Completed);
    assert_eq!(root_after.state_revision, 1, "root not advanced");
}

#[tokio::test]
async fn terminal_child_is_reattached_not_respawned() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let parent_sid;
    let child_sid;

    {
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (first)");
        let pool = guarded.clone_pool();
        let (storage, engine) = fresh_engine(Arc::new(pool));

        let inner_graph = Arc::new(
            graph_flow::GraphBuilder::new("ig")
                .add_task(Arc::new(EndTask))
                .build()
                .expect("test graph"),
        );
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

    {
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool (second)");
        let pool = guarded.clone_pool();
        let (storage, engine) = fresh_engine(Arc::new(pool));

        engine
            .shared_state()
            .hydrate_children(&parent_sid)
            .await
            .expect("hydrate children map");

        let inner_graph = Arc::new(
            graph_flow::GraphBuilder::new("ig")
                .add_task(Arc::new(EndTask))
                .build()
                .expect("test graph"),
        );
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

        let _outcome = engine
            .run_step(&parent_sid)
            .await
            .expect("parent step over terminal reattached child");

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
        assert_eq!(
            children[0].state_revision, 3,
            "terminal child checkpoint persisted before parent commit; not re-stepped"
        );
    }
}

#[tokio::test]
async fn missing_child_session_clears_stale_children_entry() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());
    let parent_sid = SessionId("par-missing-clear".to_string());

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

#[tokio::test]
async fn child_effect_propagates_to_parent_interrupted_on_failed_commit() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());

    let child_graph = Arc::new(
        graph_flow::GraphBuilder::new("child_graph")
            .add_task(Arc::new(EndTask))
            .build()
            .expect("test graph"),
    );
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

#[tokio::test]
async fn restore_pre_step_is_single_atomic_revision_fence() {
    let (_db, pool) = arc_pool_file_first().await;
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

    sqlx::query(
        "UPDATE orchestration_sessions SET state_revision = state_revision + 1,
                current_task_id = 'newer_post_task'
         WHERE session_id = ?",
    )
    .bind(&session_id.0)
    .execute(&*pool)
    .await
    .expect("concurrent bump to revision 2");

    let pre = root_session(&session_id.0, "pre_task");
    let err = storage
        .restore_pre_step(&session_id, 1, &pre)
        .await
        .expect_err("restore must refuse when a concurrent transition advanced the revision");
    assert!(
        matches!(err, EngineError::RevisionMismatch { expected: 1, .. }),
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

#[tokio::test]
async fn negative_execution_version_is_non_replayable() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = "sess-neg-execver";

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
        matches!(err, EngineError::GraphFlow(_)),
        "expected GraphFlow error, got {err:?}"
    );

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
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = "sess-fwd-execver";

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
        matches!(err, EngineError::GraphFlow(_)),
        "expected GraphFlow error, got {err:?}"
    );

    let raw: i64 = sqlx::query_scalar(
        "SELECT execution_version FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(&*pool)
    .await
    .expect("read raw execution_version");
    assert_eq!(raw, 7, "row preserved verbatim");
}

#[tokio::test]
async fn step_in_flight_persisted_before_effect_dispatch() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());

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
            None,
            RunCheckpoint {
                root: &pre,
                children: &[],
            },
            &in_flight_state,
        )
        .await
        .expect("mark_step_in_flight succeeds before the effect");

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

#[tokio::test]
async fn crash_after_effect_with_pre_step_mark_lands_interrupted() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());

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

#[tokio::test]
async fn external_effect_marker_cleared_per_step() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());

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

    let record1 = storage
        .load_run(&parent_sid)
        .await
        .expect("load_run")
        .expect("parent present");
    assert_eq!(record1.state_revision, 1, "initial revision");

    engine
        .run_step(&parent_sid)
        .await
        .expect("first effectful running step");

    let post = storage
        .get(&parent_sid.0)
        .await
        .expect("get post-step root")
        .expect("root present");
    let marker: Option<bool> = post
        .context
        .get(nexus_orchestration::engine::EXTERNAL_EFFECT_MARKER);
    assert_eq!(
        marker, None,
        "the external-effect marker must be cleared after a successful Paused step commit (Minor 2)"
    );

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
        ctx.set(nexus_orchestration::engine::EXTERNAL_EFFECT_MARKER, true)
            .unwrap();
        ctx.set("dispatched", true).unwrap();
        Ok(graph_flow::TaskResult::new(
            None,
            graph_flow::NextAction::Continue,
        ))
    }
}

#[tokio::test]
async fn v0_child_is_non_replayable_and_preserved() {
    let (_db, pool) = arc_pool_file_first().await;
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

#[tokio::test]
async fn negative_child_revision_is_non_replayable() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());
    let parent_sid = SessionId("par-neg-child-rev".to_string());

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

    let child_id = "par-neg-child-rev:child:1";
    let child_descriptor = test_descriptor(child_id);
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

    let err = storage
        .load_children(&parent_sid)
        .await
        .expect_err("negative child state_revision must be non-replayable");
    assert!(
        matches!(err, EngineError::GraphFlow(_)),
        "expected GraphFlow error for negative child revision, got {err:?}"
    );

    let raw: i64 = sqlx::query_scalar(
        "SELECT state_revision FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(child_id)
    .fetch_one(&*pool)
    .await
    .expect("read raw child state_revision");
    assert_eq!(raw, -4, "child row preserved verbatim");

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

#[tokio::test]
async fn stale_mark_step_in_flight_against_waiting_row_leaves_wait_untouched() {
    let (_db, pool) = arc_pool_file_first().await;
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

    let wait_state = RunStateV1 {
        wait: Some(nexus_session_storage::run_state::WaitRecord {
            wait_id: "wait-mark-token".to_string(),
            task_id: "wait_task".to_string(),
            child_session_id: None,
            child_task_id: None,
            kind: nexus_session_storage::run_state::WaitKind::Manual,
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

    let pre = root_session(&session_id.0, "stale_effect_start");
    let in_flight_state = RunStateV1 {
        step_in_flight: Some("stale_effect_start".to_string()),
        ..RunStateV1::default()
    };
    let _err = storage
        .mark_step_in_flight(
            &session_id,
            2, // matches persisted revision
            None,
            RunCheckpoint {
                root: &pre,
                children: &[],
            },
            &in_flight_state,
        )
        .await
        .expect_err("mark_step_in_flight must refuse a human-wait row");

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

#[tokio::test]
async fn stale_restore_pre_step_against_waiting_row_leaves_wait_untouched() {
    let (_db, pool) = arc_pool_file_first().await;
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

    let wait_state = RunStateV1 {
        wait: Some(nexus_session_storage::run_state::WaitRecord {
            wait_id: "wait-restore-token".to_string(),
            task_id: "wait_task".to_string(),
            child_session_id: None,
            child_task_id: None,
            kind: nexus_session_storage::run_state::WaitKind::Manual,
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

    let pre = root_session(&session_id.0, "pre_wait_boundary");
    let err = storage
        .restore_pre_step(&session_id, 2, &pre)
        .await
        .expect_err("restore_pre_step must refuse a human-wait row");
    assert!(
        matches!(err, EngineError::TerminalState(_)),
        "expected TerminalState for a waiting_for_input restore, got {err:?}"
    );

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

#[tokio::test]
async fn engine_effectful_task_does_not_dispatch_before_in_flight_mark() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool.clone());

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

    let root = storage
        .get(&parent_sid.0)
        .await
        .expect("get pre-wait root")
        .expect("root present");
    let wait_state = RunStateV1 {
        wait: Some(nexus_session_storage::run_state::WaitRecord {
            wait_id: "wait-engine-gate".to_string(),
            task_id: root.current_task_id.clone(),
            child_session_id: None,
            child_task_id: None,
            kind: nexus_session_storage::run_state::WaitKind::Manual,
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
        .context
        .get(nexus_orchestration::engine::EXTERNAL_EFFECT_MARKER);
    assert_eq!(
        effect, None,
        "no external-effect marker — no effect task ran before the mark gate"
    );

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

#[tokio::test]
async fn engine_join_park_persists_paused_tokenless_with_live_join_keys() {
    let (db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = Arc::new(nexus_capability::CapabilityRegistry::with_builtins());
    let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc.clone(),
        workflow_store.clone(),
        nexus_capability::CapabilityRegistryHolder::with_registry(caps.clone()),
    );

    let mut loaded = nexus_preset::load_preset_from_str(converge_preset_yaml(), &caps)
        .expect("converge preset loads");
    loaded.source_identity = Some(
        nexus_preset::loader::preset_source_identity(
            &loaded.manifest,
            None,
            Some("e2e-engine-converge"),
        )
        .expect("embedded source identity"),
    );
    engine.set_applied_document_source(Arc::new(ManifestDocumentSource {
        identity: AppliedRunIdentity {
            apply_id: "apl_converge".to_string(),
            content_hash: "blake3:converge".to_string(),
        },
    }));
    let sid = engine
        .start_session_with_preset_for_creator(&loaded, "ctr_test")
        .await
        .expect("start converge run");

    let mut session = storage
        .get(&sid.0)
        .await
        .expect("get pre-step session")
        .expect("session present");
    session.current_task_id = "join".to_string();
    session
        .context
        .set("_converge_arrivals_join", vec!["branch_a".to_string()])
        .unwrap();
    storage.save(session).await.expect("save parked position");

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

    pool.close().await;
    let guarded = nexus_local_db::init_engine_pool(db.path())
        .await
        .expect("reopen pool");
    let pool_b = guarded.clone_pool();
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

#[tokio::test]
async fn engine_manual_wait_persists_waiting_for_input_fresh_retained_token() {
    let (db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_capability::CapabilityRegistry::with_builtins(),
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
    assert_eq!(
        wait.kind,
        nexus_session_storage::run_state::WaitKind::Manual
    );
    let token = wait.wait_id.clone();

    pool.close().await;
    let guarded = nexus_local_db::init_engine_pool(db.path())
        .await
        .expect("reopen pool");
    let pool_b = guarded.clone_pool();
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

/// Judge executor returning a GO verdict whose reason contains the "go"
/// label (the `resolve_labeled_target` first-token match).
struct GoJudgeProvider;

#[async_trait::async_trait]
impl nexus_capability::PromptExecutor for GoJudgeProvider {
    async fn execute(
        &self,
        _request: nexus_capability::PromptRequest,
    ) -> Result<nexus_capability::PromptResult, nexus_capability::CapabilityError> {
        Ok(nexus_capability::PromptResult {
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
) -> nexus_capability::CapabilityRegistryHolder {
    let deps = nexus_capability::CapabilityRuntimeDeps {
        pool: None,
        prompt_executor: Some(std::sync::Arc::new(GoJudgeProvider)),
        session_cancels,
        daemon_tool_dispatch: None,
        cdn_config: None,
        workspace_executor: None,
    };
    nexus_capability::CapabilityRegistryHolder::with_registry(std::sync::Arc::new(
        nexus_capability::CapabilityRegistry::with_runtime_deps(&deps),
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
    use nexus_orchestration::tasks::StateCompositeTask;
    use nexus_preset::manifest::{ExitWhen, LabeledNext, NextTarget, StateDefinition};

    let (db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
    let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc.clone(),
        workflow_store.clone(),
        judge_registry_holder(session_cancels.clone()),
    );
    engine.set_prompt_executor(
        std::sync::Arc::new(GoJudgeProvider),
        session_cancels.clone(),
    );

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

    {
        let session = storage
            .get(&sid.0)
            .await
            .expect("get session")
            .expect("present");
        session
            .context
            .set("_merge_other", serde_json::json!(["x"]))
            .unwrap();
        session
            .context
            .set("_join_wait_start_other", serde_json::json!(1))
            .unwrap();
        storage.save(session).await.expect("save seeded session");
    }

    let outcome = engine.run_step(&sid).await.expect("judge step");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::Paused { .. }
        ),
        "judge route is an inter-task boundary, got {outcome:?}"
    );

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
    assert!(
        !nexus_orchestration::resume_rules::gate_park_live(map, &persisted.current_task_id),
        "a labeled-routed manual wait carries no current-gate park marker \
         (cursor '{}')",
        persisted.current_task_id
    );

    pool.close().await;
    let guarded = nexus_local_db::init_engine_pool(db.path())
        .await
        .expect("reopen pool");
    let pool_b = guarded.clone_pool();
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
    let (db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc.clone(),
        workflow_store.clone(),
        nexus_capability::CapabilityRegistryHolder::with_registry(std::sync::Arc::new(
            nexus_capability::CapabilityRegistry::with_builtins(),
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
    let caps = std::sync::Arc::new(nexus_capability::CapabilityRegistry::with_builtins());
    let mut loaded =
        nexus_preset::load_preset_from_str(yaml, &caps).expect("conditional preset loads");
    loaded.source_identity = Some(
        nexus_preset::loader::preset_source_identity(
            &loaded.manifest,
            None,
            Some("e2e-cond-manual"),
        )
        .expect("embedded source identity"),
    );
    engine.set_applied_document_source(Arc::new(ManifestDocumentSource {
        identity: AppliedRunIdentity {
            apply_id: "apl_cond_manual".to_string(),
            content_hash: "blake3:cond_manual".to_string(),
        },
    }));
    let sid = engine
        .start_session_with_preset_for_creator(&loaded, "ctr_test")
        .await
        .expect("start conditional run");

    {
        let session = storage
            .get(&sid.0)
            .await
            .expect("get session")
            .expect("present");
        session.context.set("score", serde_json::json!(95)).unwrap();
        session
            .context
            .set("_merge_other", serde_json::json!(["x"]))
            .unwrap();
        session
            .context
            .set("_join_wait_start_other", serde_json::json!(1))
            .unwrap();
        storage.save(session).await.expect("save seeded session");
    }

    let outcome = engine.run_step(&sid).await.expect("branch step");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::Paused { .. }
        ),
        "conditional route is an inter-task boundary, got {outcome:?}"
    );

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

    pool.close().await;
    let guarded = nexus_local_db::init_engine_pool(db.path())
        .await
        .expect("reopen pool");
    let pool_b = guarded.clone_pool();
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

#[tokio::test]
async fn engine_nested_child_manual_wait_persists_and_propagates_no_auto_resume() {
    let (db, pool) = arc_pool_file_first().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool.clone()));
    let storage_arc: Arc<dyn SessionStorage> = storage.clone();
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_capability::CapabilityRegistry::with_builtins(),
    ));
    let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc,
        workflow_store,
        caps,
    );

    let inner_graph = Arc::new(
        graph_flow::GraphBuilder::new("inner_graph")
            .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
            .build()
            .expect("test graph"),
    );

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

    let outcome = engine.run_step(&parent_sid).await.expect("run_step parent");
    assert!(
        matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::WaitingForInput { .. }
        ),
        "a child human wait must propagate to the parent as WaitingForInput, got {outcome:?}"
    );

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

    pool.close().await;
    let guarded = nexus_local_db::init_engine_pool(db.path())
        .await
        .expect("reopen pool");
    let pool_b = guarded.clone_pool();
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

    let storage_b_dyn: Arc<dyn SessionStorage> = storage_b.clone();
    let storage_b_store: Arc<dyn WorkflowStateStore> = storage_b.clone();
    let engine_after = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_b_dyn,
        storage_b_store,
        nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_capability::CapabilityRegistry::with_builtins(),
        )),
    );
    let summary = nexus_session_storage::SessionSummary {
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
fn assert_hard_storage_error(err: &nexus_session_storage::StorageError, what: &str) {
    match err {
        nexus_session_storage::StorageError::GraphFlow(graph_flow::GraphError::StorageError(_)) => {
        }
        other => panic!("{what}: expected hard GraphError::StorageError, got {other:?}"),
    }
}

/// Review gap 1: two independently loaded snapshots at version N — the first
/// save wins, the loser receives `SessionConflict`, the winner's
/// row/context/status/revision remain exact, and a reloaded snapshot at N+1
/// saves successfully.
#[tokio::test]
async fn two_writer_graph_save_occ_loser_conflicts_and_winner_row_unchanged() {
    let (_db, pool) = arc_pool_file_first().await;
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

    winner.current_task_id = "task_winner".to_string();
    winner.context.set("winner.marker", "won").unwrap();
    storage.save(winner).await.expect("first writer wins");
    assert_eq!(db_graph_version(&pool, &session_id.0).await, 2);

    loser.current_task_id = "task_loser".to_string();
    let err = storage
        .save(loser)
        .await
        .expect_err("stale writer must lose");
    assert!(
        matches!(err, graph_flow::GraphError::SessionConflict(_)),
        "expected SessionConflict, got {err:?}"
    );

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
        let (_db, pool) = arc_pool_file_first().await;
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

        let session = root_session(&session_id.0, "task_save_attempt");
        let err = storage.save(session).await.expect_err("save must refuse");
        assert!(
            matches!(err, graph_flow::GraphError::StorageError(_)),
            "save with corrupt graph_version {corrupt}: expected StorageError, got {err:?}"
        );
        assert_eq!(db_graph_version(&pool, &session_id.0).await, corrupt);

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

        let root = root_session(&session_id.0, "task_b");
        let err = storage
            .settle_run(
                &session_id,
                1,
                None,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                &RunStateV1::default(),
                TerminalSettlementTarget::Cancelled,
            )
            .await
            .expect_err("settle_cancelled must refuse");
        assert_hard_storage_error(&err, "settle_cancelled");

        let pre = root_session(&session_id.0, "task_a");
        let err = storage
            .restore_pre_step(&session_id, 1, &pre)
            .await
            .expect_err("restore_pre_step must refuse");
        assert_hard_storage_error(&err, "restore_pre_step");

        let root = root_session(&session_id.0, "task_a");
        let in_flight = RunStateV1 {
            step_in_flight: Some("task_a".to_string()),
            ..RunStateV1::default()
        };
        let err = storage
            .mark_step_in_flight(
                &session_id,
                1,
                None,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                &in_flight,
            )
            .await
            .expect_err("mark_step_in_flight must refuse");
        assert_hard_storage_error(&err, "mark_step_in_flight");

        assert_eq!(db_graph_version(&pool, &session_id.0).await, corrupt);
        let (revision, status): (i64, String) = sqlx::query_as(
            "SELECT state_revision, status FROM orchestration_sessions WHERE session_id = ?",
        )
        .bind(&session_id.0)
        .fetch_one(&*pool)
        .await
        .expect("read corrupt row");
        assert_eq!(revision, 1);
        assert_eq!(status, "running");
    }
}

/// Review gap 2 (child path): a corrupt/exhausted CHILD `graph_version` makes
/// the parent commit fail closed with a hard storage error and rolls the
/// parent transition back atomically — never a partial parent advance.
#[tokio::test]
async fn corrupt_child_graph_version_fails_parent_transition_closed() {
    let (_db, pool) = arc_pool_file_first().await;
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

    let record = storage
        .load_run(&parent)
        .await
        .expect("load_run")
        .expect("record");
    assert_eq!(
        record.state_revision, 1,
        "failed transition must not advance the root"
    );
    assert_eq!(db_graph_version(&pool, &parent.0).await, 1);
    assert_eq!(db_graph_version(&pool, child_id).await, i64::MAX);
}

/// Review gap 3: the graph-version clock advances exactly once per
/// authoritative cursor/context write (`start_run` -> `mark_step_in_flight` ->
/// runner save -> `commit_transition` -> `restore_pre_step` -> `settle_cancelled`)
/// while durable prompt-attempt writes deliberately leave it unchanged.
#[tokio::test]
async fn graph_version_clock_monotonic_across_writers_and_untouched_by_prompt_attempts() {
    let (_db, pool) = arc_pool_file_first().await;
    let storage = SqliteSessionStorage::new(pool.clone());
    let session_id = SessionId("sess-graph-clock".to_string());

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
    assert_eq!(
        db_graph_version(&pool, &session_id.0).await,
        1,
        "start_run seeds 1"
    );

    let pre = storage.get(&session_id.0).await.expect("get").expect("row");
    assert_eq!(pre.version, 1);
    let in_flight = RunStateV1 {
        step_in_flight: Some(pre.current_task_id.clone()),
        ..RunStateV1::default()
    };
    storage
        .mark_step_in_flight(
            &session_id,
            1,
            None,
            RunCheckpoint {
                root: &pre,
                children: &[],
            },
            &in_flight,
        )
        .await
        .expect("mark_step_in_flight");
    assert_eq!(
        db_graph_version(&pool, &session_id.0).await,
        2,
        "mark_step_in_flight advances"
    );

    let mut loaded = storage.get(&session_id.0).await.expect("get").expect("row");
    assert_eq!(loaded.version, 2, "runner loads the post-mark version");
    loaded.current_task_id = "task_b".to_string();
    storage.save(loaded).await.expect("runner save");
    assert_eq!(
        db_graph_version(&pool, &session_id.0).await,
        3,
        "runner save advances"
    );

    let root = storage.get(&session_id.0).await.expect("get").expect("row");
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
    assert_eq!(
        db_graph_version(&pool, &session_id.0).await,
        4,
        "commit_transition advances"
    );

    let attempt = nexus_session_storage::run_state::PromptAttempt {
        attempt_id: uuid::Uuid::new_v4().to_string(),
        task_id: "task_b".to_string(),
        phase: nexus_session_storage::run_state::PromptPhase::Dispatching,
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

    let pre_root = root_session(&session_id.0, "task_a");
    storage
        .restore_pre_step(&session_id, 3, &pre_root)
        .await
        .expect("restore_pre_step");
    assert_eq!(
        db_graph_version(&pool, &session_id.0).await,
        5,
        "restore_pre_step advances"
    );
    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("record");
    assert_eq!(
        record.state_revision, 3,
        "restore keeps the workflow revision"
    );

    let root = storage.get(&session_id.0).await.expect("get").expect("row");
    storage
        .settle_run(
            &session_id,
            3,
            None,
            RunCheckpoint {
                root: &root,
                children: &[],
            },
            &RunStateV1::default(),
            TerminalSettlementTarget::Cancelled,
        )
        .await
        .expect("settle_cancelled");
    assert_eq!(
        db_graph_version(&pool, &session_id.0).await,
        6,
        "settle_cancelled advances"
    );
    let record = storage
        .load_run(&session_id)
        .await
        .expect("load_run")
        .expect("record");
    assert_eq!(record.status, SessionStatus::Cancelled);
    assert_eq!(record.state_revision, 4);
}

/// The applied fixture document's entry node — a node id no loaded bundle
/// declares, so the run's persisted position names the graph that ran.
const APPLIED_ENTRY_PROBE: &str = "applied_entry_probe";
/// The applied fixture document's manual wait. Never reached here (no
/// coordinator drives the entry), but it keeps the document runnable.
const APPLIED_ONLY_PROBE: &str = "applied_only_probe";

/// An applied document: entry → applied-only wait → done. It shares no node id
/// with the bundle the loaded preset carries, so a run started on it can only
/// be positioned on `applied_entry_probe`.
fn applied_probe_document(id: &str) -> WorkflowDocument {
    serde_json::from_str(&format!(
        r#"{{
  "ir_version": 0,
  "id": "{id}",
  "version": 1,
  "revision": 0,
  "state": "candidate",
  "meta": {{
    "description": "applied entry fixture",
    "kind": "creator",
    "run_intents": [],
    "requires_capabilities": []
  }},
  "entry": "{APPLIED_ENTRY_PROBE}",
  "terminal": ["done"],
  "nodes": [
    {{"id": "{APPLIED_ENTRY_PROBE}", "kind": "outer_state", "effect_class": "pure",
     "exit_when": {{"kind": "rule"}}}},
    {{"id": "{APPLIED_ONLY_PROBE}", "kind": "outer_state", "effect_class": "pure",
     "exit_when": {{"kind": "manual"}}}},
    {{"id": "done", "kind": "terminal", "effect_class": "pure"}}
  ],
  "edges": [
    {{"id": "{APPLIED_ENTRY_PROBE}->{APPLIED_ONLY_PROBE}", "from": "{APPLIED_ENTRY_PROBE}",
     "to": "{APPLIED_ONLY_PROBE}", "kind": "linear"}},
    {{"id": "{APPLIED_ONLY_PROBE}->done", "from": "{APPLIED_ONLY_PROBE}",
     "to": "done", "kind": "linear"}}
  ],
  "nests": [],
  "assets": [],
  "signals": [],
  "roles": [],
  "effect_summary": "pure",
  "provenance": {{"source": "compose"}}
}}"#
    ))
    .expect("the applied fixture document decodes")
}

/// An applied source answering for exactly one strategy id.
struct FakeAppliedDocuments {
    preset_id: &'static str,
    identity: AppliedRunIdentity,
    document: WorkflowDocument,
}

#[async_trait::async_trait]
impl AppliedDocumentSource for FakeAppliedDocuments {
    async fn applied_document(
        &self,
        preset_id: &str,
        _loaded: Option<&nexus_preset::LoadedPreset>,
    ) -> Result<Option<AppliedDocument>, String> {
        Ok((preset_id == self.preset_id).then(|| AppliedDocument {
            identity: self.identity.clone(),
            document: self.document.clone(),
        }))
    }
}

/// A source answering the document its loaded preset's manifest projects.
///
/// Some fixtures freeze an embedded source identity over a raw-YAML manifest
/// (the identity only exists to satisfy the v1 descriptor), and such a preset
/// has no compiled-in bundle to resolve — this source answers the document
/// those bytes project instead.
struct ManifestDocumentSource {
    identity: AppliedRunIdentity,
}

#[async_trait::async_trait]
impl AppliedDocumentSource for ManifestDocumentSource {
    async fn applied_document(
        &self,
        _preset_id: &str,
        loaded: Option<&nexus_preset::LoadedPreset>,
    ) -> Result<Option<AppliedDocument>, String> {
        let loaded = loaded.ok_or("the fixture supplies its loaded preset")?;
        let document = nexus_preset::workflow_ir::import_preset_manifest(&loaded.manifest)
            .map_err(|error| error.to_string())?;
        Ok(Some(AppliedDocument {
            identity: self.identity.clone(),
            document,
        }))
    }
}

/// Assert the entry admitted the applied graph and froze its identity.
async fn assert_applied_entry(
    storage: &Arc<SqliteSessionStorage>,
    sid: &SessionId,
    identity: &AppliedRunIdentity,
) {
    let record = storage
        .load_run(sid)
        .await
        .expect("load_run")
        .expect("run present");
    let descriptor = record.descriptor.expect("v1 descriptor");
    assert_eq!(
        descriptor.applied.as_ref(),
        Some(identity),
        "the descriptor must freeze the apply the run executes"
    );
    let session = storage
        .get(&sid.0)
        .await
        .expect("session read")
        .expect("row");
    assert_eq!(
        session.current_task_id, APPLIED_ENTRY_PROBE,
        "the run must be built from the applied document, not the bundle"
    );
}

/// Both preset trait entries resolve through the `AppliedDocumentSource`: they
/// persist the identity and select the applied graph.
///
/// Mutation this fails on: either entry building the YAML graph unconditionally
/// and leaving `applied: None` — then the position is the bundle's initial
/// state and a restart rebuilds a document the run never ran.
#[tokio::test]
async fn preset_entries_freeze_and_run_the_strategy_apply() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, mut engine) = fresh_engine(pool);
    let registry = nexus_capability::CapabilityRegistry::with_builtins();
    let loaded =
        nexus_preset::load_embedded_preset("novel-writing", &registry).expect("preset loads");
    let identity = AppliedRunIdentity {
        apply_id: "apl_entry".to_string(),
        content_hash: "blake3:entry".to_string(),
    };
    engine.set_applied_document_source(Arc::new(FakeAppliedDocuments {
        preset_id: "novel-writing",
        identity: identity.clone(),
        document: applied_probe_document("novel-writing"),
    }));

    let sid = engine
        .start_session_with_preset(&loaded)
        .await
        .expect("start_session_with_preset");
    assert_applied_entry(&storage, &sid, &identity).await;

    let sid = engine
        .start_session_with_preset_for_creator(&loaded, "ctr_entry")
        .await
        .expect("start_session_with_preset_for_creator");
    assert_applied_entry(&storage, &sid, &identity).await;
}

/// A restart during an applied run re-enters through the applied identity, not
/// the strategy bundle: with the bundle deleted between admission and restart
/// the run still recovers, because the graph it executes is the applied
/// document.
///
/// Mutation this fails on: reloading the bundle and enforcing its frozen-source
/// identity/version guards before resolving the applied arm — the deleted
/// bundle then reports `reconstruction_unavailable` for a run that needs nothing
/// from it. One step past recovery lands on `applied_only_probe`, a node id the
/// admitted bundle never declared, so survival alone is not the assertion.
#[tokio::test]
async fn applied_run_recovers_after_its_bundle_is_deleted() {
    let (_db, pool) = arc_pool_file_first().await;
    let home = tempfile::tempdir().unwrap();
    let loaded = user_bundle_preset(home.path(), "applied-restart");
    let identity = AppliedRunIdentity {
        apply_id: "apl_restart".to_string(),
        content_hash: "blake3:restart".to_string(),
    };

    let (storage, mut engine) = fresh_engine(pool.clone());
    engine.set_nexus_home(home.path().to_path_buf());
    engine.set_applied_document_source(Arc::new(FakeAppliedDocuments {
        preset_id: "applied-restart",
        identity: identity.clone(),
        document: applied_probe_document("applied-restart"),
    }));
    let sid = engine
        .start_session_with_preset(&loaded)
        .await
        .expect("start_session_with_preset");
    assert_applied_entry(&storage, &sid, &identity).await;

    std::fs::remove_dir_all(home.path().join("presets").join("applied-restart"))
        .expect("delete the strategy bundle");

    let (_, mut restarted) = fresh_engine(pool.clone());
    restarted.set_nexus_home(home.path().to_path_buf());
    restarted.set_applied_document_source(Arc::new(FakeAppliedDocuments {
        preset_id: "applied-restart",
        identity: identity.clone(),
        document: applied_probe_document("applied-restart"),
    }));
    restarted
        .ensure_recovered_runner_inner(&sid)
        .await
        .expect("an applied run must recover without the bundle it never ran");
    restarted
        .run_step(&sid)
        .await
        .expect("run_step on the recovered runner");
    let session = storage
        .get(&sid.0)
        .await
        .expect("session read")
        .expect("row");
    assert_eq!(
        session.current_task_id, APPLIED_ONLY_PROBE,
        "the recovered graph must be the applied document's, not the deleted bundle's"
    );
}

/// Write a minimal valid user bundle under `home` and resolve it, asserting the
/// loader classified it as a directory source — the only source intake gates.
fn user_bundle_preset(home: &std::path::Path, preset_id: &str) -> nexus_preset::LoadedPreset {
    let bundle = home.join("presets").join(preset_id);
    std::fs::create_dir_all(&bundle).expect("bundle dir");
    std::fs::write(
        bundle.join("preset.yaml"),
        format!(
            "preset:\n  id: {preset_id}\n  version: 3\n  kind: creator\n  description: \
             \"intake fixture\"\n  requires_capabilities: []\n  initial: a\n  terminal: b\n\
             states:\n  - id: a\n    enter: []\n    exit_when: {{ kind: manual }}\n    next: b\n  \
             - id: b\n    terminal: true\n"
        ),
    )
    .expect("bundle yaml");
    let registry = nexus_capability::CapabilityRegistry::with_builtins();
    let loaded = nexus_preset::resolve_preset(preset_id, home, &registry).expect("bundle loads");
    assert!(
        matches!(
            loaded.source_identity,
            Some(PresetSourceIdentity::Directory { .. })
        ),
        "the fixture must resolve to a user directory bundle, got {:?}",
        loaded.source_identity
    );
    loaded
}

/// Intake at the trait entry: a user bundle with no current apply refuses under
/// the default `ir-only`, before any graph is built or any row is written.
///
/// Mutation this fails on: dropping the `ir-only` no-apply arm from
/// `start_preset_entry` — the entry then runs the YAML bundle the gate exists
/// to refuse, and the run is admitted as a document it never applied.
#[tokio::test]
async fn preset_entry_refuses_a_no_apply_user_bundle_under_ir_only() {
    let (_db, pool) = arc_pool_file_first().await;
    let (_, mut engine) = fresh_engine(pool.clone());
    let home = tempfile::tempdir().unwrap();
    let loaded = user_bundle_preset(home.path(), "ir-only-probe");
    engine.set_nexus_home(home.path().to_path_buf());
    engine.set_applied_document_source(Arc::new(FakeAppliedDocuments {
        preset_id: "some-other-strategy",
        identity: AppliedRunIdentity {
            apply_id: "apl_unused".to_string(),
            content_hash: "blake3:unused".to_string(),
        },
        document: applied_probe_document("some-other-strategy"),
    }));

    let refused = engine
        .start_session_with_preset(&loaded)
        .await
        .expect_err("ir-only must refuse a user bundle with no current apply");
    let reason = refused.to_string();
    assert!(
        reason.contains("yaml_intake_disabled"),
        "the refusal must name the intake refusal: {reason}"
    );
    assert!(
        reason.contains("ir-only"),
        "the refusal must name the mode that refused: {reason}"
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions")
        .fetch_one(&*pool)
        .await
        .expect("count rows");
    assert_eq!(rows, 0, "the refusal must happen before a run row exists");
}

/// The intake key governs user bundles only: an embedded preset starts under a
/// config value that would refuse a user bundle at the same entry, and records
/// no intake mode.
///
/// Mutation this fails on: reading the key (or applying the `ir-only` no-apply
/// arm) without the `is_user_strategy_bundle` classification — every embedded
/// run would then be gated on an operator key that does not name it.
#[tokio::test]
async fn preset_entry_does_not_gate_an_embedded_preset() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, mut engine) = fresh_engine(pool);
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("config.toml"),
        "[strategy_intake]\nmode = \"bogus\"\n",
    )
    .expect("home config");
    engine.set_nexus_home(home.path().to_path_buf());
    let registry = nexus_capability::CapabilityRegistry::with_builtins();
    let loaded =
        nexus_preset::load_embedded_preset("novel-writing", &registry).expect("preset loads");

    let sid = engine
        .start_session_with_preset(&loaded)
        .await
        .expect("an embedded preset never consults the intake key");
    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    let descriptor = record.descriptor.expect("v1 descriptor");
    assert!(
        descriptor.intake_mode.is_none(),
        "an embedded run records no intake mode, got {:?}",
        descriptor.intake_mode
    );
}

/// An engine with no configured home has no operator key to read, so it takes
/// the documented missing-file default instead of probing the process CWD.
///
/// Mutation this fails on: passing `Path::new("")` for a missing home — the
/// read then joins onto the process CWD, where an unrelated `config.toml` can
/// refuse with `intake_config_invalid` (the `yaml-only` value below no longer
/// parses) naming a file the operator never set.
#[tokio::test]
async fn preset_entry_without_a_home_takes_ir_only() {
    let (_db, pool) = arc_pool_file_first().await;
    let (_, engine) = fresh_engine(pool);
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("config.toml"),
        "[strategy_intake]\nmode = \"yaml-only\"\n",
    )
    .expect("home config");
    let loaded = user_bundle_preset(home.path(), "home-less-probe");

    let refused = engine
        .start_session_with_preset(&loaded)
        .await
        .expect_err("a home-less engine must take the ir-only default");
    let reason = refused.to_string();
    assert!(
        reason.contains("yaml_intake_disabled"),
        "the absent home must not fall through to the YAML arm: {reason}"
    );
}

/// Restart recovery over a trait-entry run: a successor apply is refused, the
/// admitted apply is rebuilt.
///
/// The run was admitted through `start_session_with_preset`, so its descriptor
/// froze apply A and its position is the applied document's entry node. A
/// restarted engine whose source answers with a successor apply B must keep the
/// run non-replayable (the `(descriptor.applied.as_ref(), applied)` match in
/// `reconstruct_runner`) and leave the durable position
/// alone. A restarted engine whose source still answers A rebuilds the applied
/// document — proven by one step landing on `applied_only_probe`, a node id the
/// loaded bundle does not declare.
#[tokio::test]
async fn preset_entry_recovery_refuses_a_successor_and_rebuilds_the_admitted_apply() {
    let (_db, pool) = arc_pool_file_first().await;
    let registry = nexus_capability::CapabilityRegistry::with_builtins();
    let loaded =
        nexus_preset::load_embedded_preset("novel-writing", &registry).expect("preset loads");
    let admitted = AppliedRunIdentity {
        apply_id: "apl_entry".to_string(),
        content_hash: "blake3:entry".to_string(),
    };
    let successor = AppliedRunIdentity {
        apply_id: "apl_entry_successor".to_string(),
        content_hash: "blake3:successor".to_string(),
    };

    let (storage, mut engine) = fresh_engine(pool.clone());
    engine.set_applied_document_source(Arc::new(FakeAppliedDocuments {
        preset_id: "novel-writing",
        identity: admitted.clone(),
        document: applied_probe_document("novel-writing"),
    }));
    let sid = engine
        .start_session_with_preset(&loaded)
        .await
        .expect("start_session_with_preset");

    let (_, mut restarted) = fresh_engine(pool.clone());
    restarted.set_applied_document_source(Arc::new(FakeAppliedDocuments {
        preset_id: "novel-writing",
        identity: successor.clone(),
        document: applied_probe_document("novel-writing"),
    }));
    let refused = restarted
        .ensure_recovered_runner_inner(&sid)
        .await
        .expect_err("a run whose admitted apply gained a successor must not be recovered");
    let reason = refused.to_string();
    assert!(
        reason.contains(&admitted.apply_id),
        "the refusal must name the apply the run was admitted under: {reason}"
    );
    assert!(
        reason.contains(&successor.apply_id),
        "the refusal must name the apply that replaced it: {reason}"
    );
    assert_applied_entry(&storage, &sid, &admitted).await;

    let (_, mut recovered) = fresh_engine(pool.clone());
    recovered.set_applied_document_source(Arc::new(FakeAppliedDocuments {
        preset_id: "novel-writing",
        identity: admitted.clone(),
        document: applied_probe_document("novel-writing"),
    }));
    recovered
        .ensure_recovered_runner_inner(&sid)
        .await
        .expect("the admitted apply is still current, so the run must be rebuilt");
    recovered
        .run_step(&sid)
        .await
        .expect("the rebuilt run must execute the applied document's entry node");
    let session = storage
        .get(&sid.0)
        .await
        .expect("session read")
        .expect("row");
    assert_eq!(
        session.current_task_id, APPLIED_ONLY_PROBE,
        "the rebuilt graph must carry the applied-only node; got {:?}",
        session.current_task_id
    );
}

/// An embedded preset admitted with NO configured applied source still executes
/// its document: the entry resolves the preset's own compiled-in bundle under
/// the compass D10 identity and freezes it on the descriptor.
///
/// Mutation this fails on: `start_preset_entry` handing the embedded preset to
/// the YAML builder with `applied: None` — the descriptor then freezes no
/// identity, so neither a restart nor an operator can tell which bytes the run
/// executed, and recovery cannot refuse a changed embed.
#[tokio::test]
async fn embedded_entry_freezes_the_d10_identity_without_a_configured_source() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, engine) = fresh_engine(pool);
    let registry = nexus_capability::CapabilityRegistry::with_builtins();
    let loaded =
        nexus_preset::load_embedded_preset("novel-writing", &registry).expect("preset loads");

    let sid = engine
        .start_session_with_preset(&loaded)
        .await
        .expect("an embedded preset starts without a configured source");
    let document =
        nexus_preset::embedded_workflow_document(&loaded).expect("embedded document constructs");
    let (apply_id, content_hash) =
        nexus_preset::embedded_applied_identity(&loaded.id, &document).expect("D10 identity");
    assert_eq!(apply_id, format!("embedded:{}", loaded.id));
    let record = storage
        .load_run(&sid)
        .await
        .expect("load_run")
        .expect("run present");
    let descriptor = record.descriptor.expect("v1 descriptor");
    assert_eq!(
        descriptor.applied,
        Some(AppliedRunIdentity {
            apply_id,
            content_hash,
        }),
        "the entry must freeze the identity of the document the run executes; \
         `applied: None` means it executed the YAML manifest instead"
    );
}

/// Legacy (no-store) recovery rebuilds an embedded session from the applied
/// document, not from the preset's manifest: the source answers with a document
/// whose node ids the bundle never declares, and the recovered runner continues
/// on them.
///
/// Mutation this fails on: `reconstruct_runner_legacy` calling
/// `build_wired_outer_graph(&loaded, ..)` directly — the rebuilt graph then has
/// no `applied_entry_probe` task, so the run cannot resume where it parked.
#[tokio::test]
async fn legacy_recovery_rebuilds_an_embedded_session_from_the_document() {
    let registry = nexus_capability::CapabilityRegistry::with_builtins();
    let loaded =
        nexus_preset::load_embedded_preset("novel-writing", &registry).expect("preset loads");
    let identity = AppliedRunIdentity {
        apply_id: "apl_legacy".to_string(),
        content_hash: "blake3:legacy".to_string(),
    };
    let storage: Arc<dyn SessionStorage> = Arc::new(graph_flow::InMemorySessionStorage::new());
    let mut admitted = nexus_orchestration::GraphFlowEngine::new_with_storage(
        storage.clone(),
        nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_capability::CapabilityRegistry::with_builtins(),
        )),
    );
    admitted.set_applied_document_source(Arc::new(FakeAppliedDocuments {
        preset_id: "novel-writing",
        identity: identity.clone(),
        document: applied_probe_document("novel-writing"),
    }));
    let sid = admitted
        .start_session_with_preset(&loaded)
        .await
        .expect("start_session_with_preset");
    let summaries = admitted
        .list_active(nexus_orchestration::engine::SessionFilter::default())
        .await
        .expect("list_active");

    let mut recovered = nexus_orchestration::GraphFlowEngine::new_with_storage(
        storage.clone(),
        nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_capability::CapabilityRegistry::with_builtins(),
        )),
    );
    recovered.set_applied_document_source(Arc::new(FakeAppliedDocuments {
        preset_id: "novel-writing",
        identity,
        document: applied_probe_document("novel-writing"),
    }));
    recovered.recover_sessions_inner(summaries).await;
    recovered
        .run_step(&sid)
        .await
        .expect("the recovered runner steps the document's graph");
    let session = storage
        .get(&sid.0)
        .await
        .expect("session read")
        .expect("row");
    assert_eq!(
        session.current_task_id, APPLIED_ONLY_PROBE,
        "legacy recovery must rebuild the document's graph; the bundle has no \
         `{APPLIED_ENTRY_PROBE}` task to resume at (got {:?})",
        session.current_task_id
    );
}

/// A store-backed pre-upgrade row froze an embedded source but no applied
/// identity (the field did not exist yet). A configured source that answers
/// "no document" for it must leave the run non-replayable, not rebuild the
/// YAML manifest: a run admitted after D10 executes its document, so the
/// manifest is a graph this row never ran.
///
/// Mutation this fails on: `resolve_applied` answering `Ok(None)` for an
/// embedded preset — recovery then reaches `build_wired_outer_graph(&loaded, ..)`
/// for a descriptor with no recorded apply.
#[tokio::test]
async fn store_recovery_refuses_a_legacy_embedded_row_the_source_cannot_resolve() {
    let (_db, pool) = arc_pool_file_first().await;
    let (storage, mut engine) = fresh_engine(pool);
    let registry = nexus_capability::CapabilityRegistry::with_builtins();
    let loaded =
        nexus_preset::load_embedded_preset("novel-writing", &registry).expect("preset loads");
    let session_id = SessionId("sess-legacy-embedded".to_string());

    let mut descriptor = test_descriptor(&session_id.0);
    descriptor.preset_id = loaded.id.clone();
    descriptor.preset_version = loaded.version;
    descriptor.source = loaded
        .source_identity
        .clone()
        .expect("an embedded preset carries its source identity");
    assert!(
        descriptor.applied.is_none(),
        "the fixture must be a legacy row: no applied identity"
    );
    let root = root_session(&session_id.0, "task_a");
    storage
        .start_run(
            &session_id,
            &descriptor,
            RunCheckpoint {
                root: &root,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start_run");

    engine.set_applied_document_source(Arc::new(FakeAppliedDocuments {
        preset_id: "some-other-strategy",
        identity: AppliedRunIdentity {
            apply_id: "apl_unused".to_string(),
            content_hash: "blake3:unused".to_string(),
        },
        document: applied_probe_document("some-other-strategy"),
    }));

    let refused = engine
        .ensure_recovered_runner_inner(&session_id)
        .await
        .expect_err("a legacy embedded row with no resolvable document must not be rebuilt");
    let reason = refused.to_string();
    assert!(
        reason.contains("embedded_document_unavailable"),
        "the refusal must name the missing embedded document: {reason}"
    );
    assert!(
        reason.contains("non-replayable"),
        "the run must stay non-replayable, not resume on its manifest: {reason}"
    );
}
