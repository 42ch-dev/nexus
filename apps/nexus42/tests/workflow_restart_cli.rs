//! v1.186 P0 Task 3 — real SQLite/process reopen acceptance for durable
//! terminal / wait / interrupted truth.
//!
//! P0 fixture-only proof (no live omp, no public schedule admission, no
//! Host/provider change): a REAL `orchestration_sessions` SQLite file inside
//! a temp workspace is written by one "process" (pool A), fully closed
//! (`pool.close` + drop), then reopened by a fresh pool/engine (the daemon
//! restart) and by the real `nexus42 ops inspect` binary (separate OS
//! process, hermetic `$HOME`). Fixtures seed/drive the v1 durable rows
//! internally — that is not public-admission proof.
//!
//! Scenarios (`cargo test -p nexus42 --test workflow_restart_cli durable_status`):
//! 1. terminals stay terminal across reopen, are never rewritten to
//!    `running`, and inspect reports `recovery_class: terminal`
//!    (`completed` / `failed` / `cancelled`, even with stale interrupted
//!    evidence in the durable state — A7 rule 1).
//! 2. human wait survives reopen: the A4 wait token is byte-preserved,
//!    the daemon resume skips (never stepped or approved), and inspect
//!    reports `recovery_class: human_wait` + `wait_id`; a token-bearing
//!    wait with OLD scheduler join keys still classifies human wait
//!    (token beats join keys, never auto-approved).
//! 3. interrupted/ambiguous work is reported non-replayable and never
//!    retried after reopen (explicit `interrupted` status AND
//!    running-with-in-flight/cancel marks), verdict `no` /
//!    rule `interrupted` / "never auto-retried", zero steps consumed,
//!    rows not rewritten.
//! 4. shipped converge/merge resume still works after reopen: a v1
//!    tokenless row parked at the join with live join keys re-drives
//!    through the real A7 gate (`ConvergeMerge` → `ReDriven`), the join
//!    is re-checked exactly once, and no already-completed instrumented
//!    edge re-fires.

#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::too_many_lines)] // reopen-acceptance scenarios hold setup+assertions linear
#![allow(clippy::used_underscore_binding)] // fixture channels with intentionally unused tx

mod common;

use assert_cmd::Command;
use async_trait::async_trait;
use common::LiveDaemon;
use nexus_agent_host::capability::model::{
    CapabilityDescriptor, CreateSessionRequest, FinishReason, HostContentBlock, HostEvent,
    HostEventStream, HostHealth, HostOperation, HostStartConfig, OperationFinishedEvent,
    OperationStartedEvent, ProtocolKind, ProviderHealth, TextDeltaEvent,
};
use nexus_agent_host::{
    DiscoverySource, HostError, HostOperationId, HostResult, HostSession, HostSessionId,
    LaunchStrategy, ProviderCatalog, ProviderCatalogEntry, SessionState, TrustLevel,
};
use nexus_daemon_runtime::preset_run::{
    resume_driven_sessions, PresetRunConfig, PresetRunOutcome, ResumeDecision,
};
use nexus_daemon_runtime::test_utils;
use nexus_orchestration::capability::DaemonToolDispatch;
use nexus_orchestration::engine::{SessionId, SessionStatus, SessionSummary};
use nexus_orchestration::preset::load_preset_from_str;
use nexus_orchestration::preset::loader::build_wired_outer_graph;
use nexus_orchestration::run_state::WorkflowStateStore;
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use nexus_orchestration::{
    CapabilityError, CapabilityRegistry, CapabilityRegistryHolder, GraphFlowEngine,
    OrchestrationEngine, PresetSourceIdentity,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// The REAL `nexus42 ops inspect` binary over the hermetic tmp HOME whose
/// seeded `config.toml` resolves the SAME `state.db` the pools write to.
fn nexus42(home: &Path) -> Command {
    let mut cmd = Command::cargo_bin("nexus42").expect("nexus42 binary");
    cmd.env("HOME", home);
    cmd
}

/// Valid frozen `RunDescriptorV1` (A2) — the same shape the engine writes
/// via `start_run` (embedded source identity). Required on every v1 row.
const V1_DESCRIPTOR: &[u8] = br#"{"creator_id":"test_creator","work_id":null,"workspace_root":"/tmp/ws",
     "preset_id":"e2e-converge","preset_version":3,
     "source":{"Embedded":{"preset_id":"e2e-converge","content_hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}},
     "input":{},"agent_bindings":{},"parent_session_id":null,"graph_name":null}"#;

/// Frozen child `RunDescriptorV1` (A2/A4) — the shape the production
/// `spawn_child_session_internal` writes: the child inherits the trusted
/// root identity (creator, preset, version, source) and names its parent
/// session and inner graph. The nested-child restart fixture must seed this
/// exact shape so descriptor identity preservation is actually proven.
const CHILD_DESCRIPTOR: &[u8] = br#"{"creator_id":"test_creator","work_id":null,"workspace_root":"/tmp/ws",
     "preset_id":"e2e-converge","preset_version":3,
     "source":{"Embedded":{"preset_id":"e2e-converge","content_hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}},
     "input":{},"agent_bindings":{},"parent_session_id":"nested:parent","graph_name":"manual_wait_graph"}"#;

/// Seed an authoritative v1 row (A2 columns) directly into the real SQLite
/// file — the "previous process" checkpoint state. `run_state` is the
/// serialized [`nexus_orchestration::run_state::RunStateV1`] blob (fixture
/// shape mirrored from the daemon A7 tests).
#[allow(clippy::too_many_arguments)]
async fn seed_v1_row(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    status: &str,
    current_task_id: Option<&str>,
    context: &[u8],
    run_state: &[u8],
    state_revision: i64,
) {
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES (?, 'test_creator', 'e2e-converge', 3, ?, ?, ?, 1_756_990_000, 1_756_990_300,
                 1, ?, ?, ?)",
    )
    .bind(session_id)
    .bind(status)
    .bind(current_task_id)
    .bind(context)
    .bind(state_revision)
    .bind(run_state)
    .bind(V1_DESCRIPTOR)
    .execute(pool)
    .await
    .expect("seed v1 row");
}

/// Serialized durable v1 run state (A2 `RunStateV1`), fixture-shaped.
/// `wait` selects a durable A4 human-wait token (`wait-tok-1`); the other
/// flags model interrupted evidence (in-flight prompt, cancel, step mark).
fn run_state_v1(
    wait: bool,
    step_in_flight: Option<&str>,
    in_flight: bool,
    cancel_requested: bool,
) -> Vec<u8> {
    json!({
        "wait": wait.then(|| json!({
            "wait_id": "wait-tok-1", "task_id": "task_7",
            "child_session_id": null, "child_task_id": null, "kind": "manual"
        })),
        "step_in_flight": step_in_flight,
        "in_flight": in_flight.then(|| json!({
            "attempt_id": "attempt-9", "task_id": "task_3", "phase": "active",
            "host_session_id": "host-s1", "operation_id": "op-1", "process_identity": null
        })),
        "failure": null,
        "cancel_requested": cancel_requested,
    })
    .to_string()
    .into_bytes()
}

/// Plain context (no join keys).
fn plain_context() -> Vec<u8> {
    json!({"data": {"_creator_id": "test_creator"}, "chat_history": {"messages": [], "max_messages": 1000}})
        .to_string()
        .into_bytes()
}

/// Live converge/merge chain context (the persisted join park).
fn chain_context() -> Vec<u8> {
    json!({"data": {
        "_converge_arrivals_j1": ["src-a"],
        "_join_wait_start_j1": 1
    }, "chat_history": {"messages": [], "max_messages": 1000}})
    .to_string()
    .into_bytes()
}

/// Reopen the REAL SQLite file with a fresh pool + storage + workflow store
/// — the daemon-restart boundary (previous pool fully closed by the caller).
async fn reopen(
    db_path: &Path,
) -> (
    sqlx::SqlitePool,
    Arc<dyn graph_flow::SessionStorage>,
    Arc<dyn WorkflowStateStore>,
) {
    let pool = nexus_local_db::open_pool(db_path)
        .await
        .expect("reopen pool");
    let sqlite: Arc<SqliteSessionStorage> =
        Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
    let dyn_storage: Arc<dyn graph_flow::SessionStorage> = sqlite.clone();
    let store: Arc<dyn WorkflowStateStore> = sqlite;
    (pool, dyn_storage, store)
}

/// Counting daemon-tool dispatch: any `dispatch_tool` call increments a
/// shared counter — "edges already executed before the restart must not
/// re-fire" instrumentation (zero fires expected on a converge re-drive
/// that only re-checks the parked join).
#[derive(Clone)]
struct CountingDispatch {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl DaemonToolDispatch for CountingDispatch {
    async fn dispatch_tool(
        &self,
        _tool_name: &str,
        _args: &Value,
        _request_id: &str,
    ) -> Result<Value, CapabilityError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(json!({ "ok": true }))
    }
}

/// Converge chain with instrumented host-tool edges on `start` and
/// `branch_a`; `branch_b` is the hanging upstream (never walked, never
/// arrives); a very long join deadline (300s) so a re-step at the parked
/// join deterministically parks again (1/2 arrivals) instead of racing the
/// fallback timer.
const CONVERGE_YAML: &str = r#"
preset:
  id: e2e-restart-converge
  version: 1
  kind: creator
  description: "v1.186 P0 T3 — reopen converge/merge resume with instrumented edges"
  requires_capabilities: []
  initial: start
  terminal: done
states:
    - id: start
      enter:
        - kind: host_tool
          tool_name: test.instrument
          args: { edge: start }
      next: branch_a
    - id: branch_a
      enter:
        - kind: host_tool
          tool_name: test.instrument
          args: { edge: branch_a }
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
"#;

// ---------------------------------------------------------------------------
// Scenario 1 — terminals stay terminal across reopen, never revert to running
// ---------------------------------------------------------------------------

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn restart_durable_status_terminals_stay_terminal_and_never_revert() {
    let (tmp, _nexus_home, db_path) = test_utils::create_test_workspace().await;
    let user_home = tmp.path();

    // "Process A": seed completed/failed/cancelled v1 rows, each carrying
    // STALE interrupted evidence (step_in_flight + cancel_requested + active
    // in-flight prompt) — terminal status must still win (A7 rule 1).
    {
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        for (id, status) in [
            ("done:completed", "completed"),
            ("done:failed", "failed"),
            ("done:cancelled", "cancelled"),
        ] {
            let state = run_state_v1(false, Some("task_3"), true, true);
            seed_v1_row(
                &pool,
                id,
                status,
                Some("task_9"),
                &plain_context(),
                &state,
                5,
            )
            .await;
        }
        pool.close().await;
    }

    // "Daemon restart": a fresh pool reads the SAME file; the durable
    // statuses must still be terminal (not disguised as running).
    let (pool_b, dyn_storage, store) = reopen(&db_path).await;
    for (id, expected) in [
        ("done:completed", SessionStatus::Completed),
        ("done:failed", SessionStatus::Failed),
        ("done:cancelled", SessionStatus::Cancelled),
    ] {
        let record = store
            .load_run(&SessionId(id.to_string()))
            .await
            .expect("load_run")
            .expect("row exists");
        assert_eq!(record.status, expected, "{id} must survive reopen terminal");
        assert_eq!(record.execution_version, 1);
        assert_eq!(record.state_revision, 5, "{id} must not be rewritten");
    }

    // Resume with STALE boot summaries (running) — the durable terminal
    // record must win and skip immediately: no re-drive, no rewrite.
    let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
        dyn_storage.clone(),
        store.clone(),
        CapabilityRegistryHolder::with_registry(Arc::new(CapabilityRegistry::with_builtins())),
    );
    let summaries: Vec<SessionSummary> = ["done:completed", "done:failed", "done:cancelled"]
        .iter()
        .map(|id| SessionSummary {
            session_id: SessionId(id.to_string()),
            creator_id: "test_creator".to_string(),
            preset_id: "e2e-converge".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("task_9".to_string()),
        })
        .collect();
    let decisions = resume_driven_sessions(
        &engine,
        &dyn_storage,
        Some(&store),
        &summaries,
        &PresetRunConfig::default(),
        None,
    )
    .await;
    assert_eq!(
        decisions,
        vec![
            ResumeDecision::SkippedTerminal {
                session_id: SessionId("done:completed".to_string())
            },
            ResumeDecision::SkippedTerminal {
                session_id: SessionId("done:failed".to_string())
            },
            ResumeDecision::SkippedTerminal {
                session_id: SessionId("done:cancelled".to_string())
            },
        ],
        "terminal rows must skip immediately after reopen, never re-drive"
    );

    // Terminal rows never revert to running: raw DB status unchanged.
    let rows: Vec<String> =
        sqlx::query_scalar("SELECT status FROM orchestration_sessions ORDER BY session_id")
            .fetch_all(&pool_b)
            .await
            .expect("status rows");
    assert_eq!(
        rows,
        vec![
            "cancelled".to_string(),
            "completed".to_string(),
            "failed".to_string()
        ],
        "DB status column must still carry the terminal statuses after resume"
    );

    // Scoped public evidence: the REAL binary over the reopened file.
    for (id, status) in [
        ("done:completed", "completed"),
        ("done:failed", "failed"),
        ("done:cancelled", "cancelled"),
    ] {
        let output = nexus42(user_home)
            .args(["ops", "inspect", id, "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let parsed: Value = serde_json::from_slice(&output).expect("valid inspect json");
        assert_eq!(parsed["session_id"], json!(id));
        assert_eq!(parsed["db_status"], json!(status), "{id} db_status");
        assert_eq!(parsed["recovery_class"], json!("terminal"), "{id}");
        assert_eq!(parsed["execution_version"], json!(1));
        assert_eq!(parsed["resumable"]["verdict"], json!("no"));
        assert_eq!(parsed["resumable"]["rule"], json!("terminal_status"));
        assert!(
            parsed.get("wait_id").is_none(),
            "terminal rows must not advertise a wait token: {parsed}"
        );
    }
    // Terminal lookup renders the class in the human view too.
    let human = nexus42(user_home)
        .args(["ops", "inspect", "done:completed"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(human).unwrap();
    assert!(human.contains("recovery_class: terminal"), "{human}");
    assert!(human.contains("status:         completed"), "{human}");

    pool_b.close().await;
    drop(tmp);
}

// ---------------------------------------------------------------------------
// Scenario 2 — human wait survives reopen; token preserved, never approved
// ---------------------------------------------------------------------------

#[tokio::test]
async fn restart_durable_status_human_wait_token_survives_not_approved() {
    let (tmp, _nexus_home, db_path) = test_utils::create_test_workspace().await;
    let user_home = tmp.path();
    let wait_state = run_state_v1(true, None, false, false);

    // "Process A": a plain human wait AND a token-bearing wait carrying OLD
    // scheduler join keys (must stay human wait — the A4 token beats old
    // join keys, never silently advanced to a join re-drive). Round-4 adds
    // a third row: a stale CURRENT-GATE marker for a DIFFERENT task
    // (`_gate_park_other`) alongside broad keys — historical gate markers
    // must never demote a live human wait either.
    {
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        seed_v1_row(
            &pool,
            "wait:plain",
            "waiting_for_input",
            Some("task_7"),
            &plain_context(),
            &wait_state,
            6,
        )
        .await;
        seed_v1_row(
            &pool,
            "wait:old-joins",
            "waiting_for_input",
            Some("task_7"),
            &chain_context(),
            &wait_state,
            7,
        )
        .await;
        let stale_marker_context = serde_json::json!({"data": {
            "_converge_arrivals_j1": ["a"],
            "_join_wait_start_j1": 1,
            "_gate_park_other": true
        }, "chat_history": {"messages": [], "max_messages": 1000}})
        .to_string()
        .into_bytes();
        seed_v1_row(
            &pool,
            "wait:stale-marker",
            "waiting_for_input",
            Some("task_7"),
            &stale_marker_context,
            &wait_state,
            8,
        )
        .await;
        pool.close().await;
    }

    // "Daemon restart": fresh pool reopens the file.
    let (pool_b, dyn_storage, store) = reopen(&db_path).await;

    // The durable wait token is byte-preserved after reopen for ALL rows.
    for id in ["wait:plain", "wait:old-joins", "wait:stale-marker"] {
        let record = store
            .load_run(&SessionId(id.to_string()))
            .await
            .expect("load_run")
            .expect("row exists");
        assert_eq!(record.status, SessionStatus::WaitingForInput, "{id}");
        assert_eq!(
            record.state.clone().and_then(|s| s.wait).map(|w| w.wait_id),
            Some("wait-tok-1".to_string()),
            "the A4 wait token must survive reopen untouched: {id}"
        );
    }

    // Boot resume must skip all three (never stepped, never auto-approved).
    let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
        dyn_storage.clone(),
        store.clone(),
        CapabilityRegistryHolder::with_registry(Arc::new(CapabilityRegistry::with_builtins())),
    );
    let summaries: Vec<SessionSummary> = ["wait:plain", "wait:old-joins", "wait:stale-marker"]
        .iter()
        .map(|id| SessionSummary {
            session_id: SessionId(id.to_string()),
            creator_id: "test_creator".to_string(),
            preset_id: "e2e-converge".to_string(),
            status: SessionStatus::WaitingForInput,
            current_task_id: Some("task_7".to_string()),
        })
        .collect();
    let decisions = resume_driven_sessions(
        &engine,
        &dyn_storage,
        Some(&store),
        &summaries,
        &PresetRunConfig::default(),
        None,
    )
    .await;
    assert_eq!(
        decisions,
        vec![
            ResumeDecision::SkippedHumanWait {
                session_id: SessionId("wait:plain".to_string())
            },
            ResumeDecision::SkippedHumanWait {
                session_id: SessionId("wait:old-joins".to_string())
            },
            ResumeDecision::SkippedHumanWait {
                session_id: SessionId("wait:stale-marker".to_string())
            },
        ],
        "token-bearing waits (even with old join keys or a stale gate marker \
         for another task) are never stepped at boot"
    );

    // The token is STILL the same after the resumed boot (no implicit
    // approval, no regeneration, no rewrite).
    for id in ["wait:plain", "wait:old-joins", "wait:stale-marker"] {
        let record = store
            .load_run(&SessionId(id.to_string()))
            .await
            .expect("load_run")
            .expect("row exists");
        assert_eq!(
            record.state.clone().and_then(|s| s.wait).map(|w| w.wait_id),
            Some("wait-tok-1".to_string()),
            "the wait token must survive the resumed boot: {id}"
        );
        assert_eq!(record.status, SessionStatus::WaitingForInput, "{id}");
    }

    // Scoped public evidence: inspect agrees on the reopened file for all
    // three human-wait rows (including the one with a stale gate marker for
    // a different task plus broad join keys).
    for id in ["wait:plain", "wait:old-joins", "wait:stale-marker"] {
        let output = nexus42(user_home)
            .args(["ops", "inspect", id, "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let parsed: Value = serde_json::from_slice(&output).expect("valid inspect json");
        assert_eq!(parsed["db_status"], json!("waiting_for_input"), "{id}");
        assert_eq!(parsed["recovery_class"], json!("human_wait"), "{id}");
        assert_eq!(parsed["wait_id"], json!("wait-tok-1"), "{id}");
        assert_eq!(parsed["resumable"]["verdict"], json!("no"), "{id}");
        assert_eq!(parsed["resumable"]["rule"], json!("human_wait"), "{id}");
    }
    // Human view names the class and the preserved token context.
    let human = nexus42(user_home)
        .args(["ops", "inspect", "wait:plain"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(human).unwrap();
    assert!(human.contains("recovery_class: human_wait"), "{human}");
    assert!(
        human.contains("resumable:      no — human wait (A4) token preserved"),
        "{human}"
    );

    pool_b.close().await;
    drop(tmp);
}

// ---------------------------------------------------------------------------
// Scenario 3 — interrupted is reported non-replayable and never retried
// ---------------------------------------------------------------------------

#[tokio::test]
async fn restart_durable_status_interrupted_not_replayed_and_never_retried() {
    let (tmp, _nexus_home, db_path) = test_utils::create_test_workspace().await;
    let user_home = tmp.path();

    // "Process A": two interrupted shapes — (a) explicit `interrupted`
    // status with an unfinished step mark; (b) running with a dispatching
    // prompt + cancel requested (crash-after-effect, never "safe to replay").
    {
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        seed_v1_row(
            &pool,
            "int:explicit",
            "interrupted",
            Some("task_3"),
            &plain_context(),
            &run_state_v1(false, Some("task_3"), false, false),
            4,
        )
        .await;
        seed_v1_row(
            &pool,
            "int:crash",
            "running",
            Some("task_3"),
            &chain_context(),
            &run_state_v1(false, Some("task_3"), true, true),
            5,
        )
        .await;
        pool.close().await;
    }

    // "Daemon restart": resume with stale Running summaries — interrupted
    // evidence must win over any join keys and be skipped, never retried.
    let (pool_b, dyn_storage, store) = reopen(&db_path).await;
    let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
        dyn_storage.clone(),
        store.clone(),
        CapabilityRegistryHolder::with_registry(Arc::new(CapabilityRegistry::with_builtins())),
    );
    let summaries: Vec<SessionSummary> = ["int:explicit", "int:crash"]
        .iter()
        .map(|id| SessionSummary {
            session_id: SessionId(id.to_string()),
            creator_id: "test_creator".to_string(),
            preset_id: "e2e-converge".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("task_3".to_string()),
        })
        .collect();
    let decisions = resume_driven_sessions(
        &engine,
        &dyn_storage,
        Some(&store),
        &summaries,
        &PresetRunConfig::default(),
        None,
    )
    .await;
    assert_eq!(
        decisions,
        vec![
            ResumeDecision::SkippedInterrupted {
                session_id: SessionId("int:explicit".to_string())
            },
            ResumeDecision::SkippedInterrupted {
                session_id: SessionId("int:crash".to_string())
            },
        ],
        "interrupted work is never retried after reopen (no ReDriven)"
    );

    // No row was rewritten: status + revision + interrupted marks intact.
    for (id, status) in [
        ("int:explicit", SessionStatus::Interrupted),
        ("int:crash", SessionStatus::Running),
    ] {
        let record = store
            .load_run(&SessionId(id.to_string()))
            .await
            .expect("load_run")
            .expect("row exists");
        assert_eq!(record.status, status, "{id} must not be rewritten");
        assert_eq!(
            record.state_revision,
            if id == "int:explicit" { 4 } else { 5 },
            "{id}"
        );
        let state = record.state.expect("state present");
        assert!(
            state.step_in_flight.is_some(),
            "{id} interrupted marks must survive reopen"
        );
    }

    // Scoped public evidence: inspect reports interrupted and non-replayable.
    for id in ["int:explicit", "int:crash"] {
        let output = nexus42(user_home)
            .args(["ops", "inspect", id, "--json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let parsed: Value = serde_json::from_slice(&output).expect("valid inspect json");
        assert_eq!(parsed["recovery_class"], json!("interrupted"), "{id}");
        assert_eq!(parsed["resumable"]["verdict"], json!("no"), "{id}");
        assert_eq!(parsed["resumable"]["rule"], json!("interrupted"), "{id}");
        let explanation = parsed["resumable"]["explanation"].as_str().unwrap();
        assert!(
            explanation.contains("never auto-retried"),
            "interrupted must be reported non-replayable: {explanation}"
        );
        assert!(
            parsed.get("wait_id").is_none(),
            "interrupted rows must not advertise a wait token: {parsed}"
        );
    }
    // Human view: explicit never-retried wording.
    let human = nexus42(user_home)
        .args(["ops", "inspect", "int:explicit"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(human).unwrap();
    assert!(human.contains("recovery_class: interrupted"), "{human}");
    assert!(human.contains("never auto-retried"), "{human}");

    pool_b.close().await;
    drop(tmp);
}

// ---------------------------------------------------------------------------
// Scenario 4 — shipped converge/merge resume still works after reopen
// ---------------------------------------------------------------------------

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn restart_durable_status_converge_merge_redrives_after_reopen() {
    let (tmp, _nexus_home, db_path) = test_utils::create_test_workspace().await;
    let user_home = tmp.path();
    let sid = "conv:parked-join";

    // "Process A": v1 row parked at the converge join — tokenless (A7 rule
    // 5), running, exact current-gate park marker, live join keys, one arrival
    // of two. The instrumented start/branch_a edges already fired (fixture).
    {
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let context = serde_json::json!({"data": {
            "_converge_arrivals_join": ["branch_a"],
            "_join_wait_start_join": chrono::Utc::now().timestamp_millis(),
            "_gate_park_join": true
        }, "chat_history": {"messages": [], "max_messages": 1000}})
        .to_string()
        .into_bytes();
        let state = run_state_v1(false, None, false, false);
        seed_v1_row(&pool, sid, "running", Some("join"), &context, &state, 5).await;
        pool.close().await;
    }

    // "Daemon restart": reopen + rebuild the engine over the SAME store,
    // reconstruct the runner for the parked session (mirrors
    // `recover_sessions` → `reconstruct_runner`), then resume with
    // `resume_waiting: true` — the bounded join re-drive posture.
    let (pool_b, dyn_storage, store) = reopen(&db_path).await;
    let dispatch = Arc::new(CountingDispatch {
        calls: Arc::new(AtomicUsize::new(0)),
    });
    let caps = Arc::new(CapabilityRegistry::with_builtins());
    let loaded = load_preset_from_str(CONVERGE_YAML, &caps).expect("test preset loads");
    let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
        dyn_storage.clone(),
        store.clone(),
        CapabilityRegistryHolder::with_registry(caps.clone()),
    );
    engine.set_daemon_tool_dispatch(dispatch.clone());
    let engine = Arc::new(engine);
    let engine_ref: Arc<dyn OrchestrationEngine> = engine.clone();
    let wired = build_wired_outer_graph(
        &loaded,
        &engine_ref,
        &caps,
        Some(dispatch.clone()),
        None,
        engine.shared_state().session_cancels.clone(),
    );
    let runner = Arc::new(graph_flow::FlowRunner::new(
        Arc::new(wired),
        dyn_storage.clone(),
    ));
    engine
        .shared_state()
        .runners
        .write()
        .await
        .insert(sid.to_string(), runner.clone());
    let summary = SessionSummary {
        session_id: SessionId(sid.to_string()),
        creator_id: "test_creator".to_string(),
        preset_id: loaded.id.clone(),
        status: SessionStatus::Running,
        current_task_id: Some("join".to_string()),
    };
    engine
        .shared_state()
        .sessions
        .write()
        .await
        .push(summary.clone());
    let config = PresetRunConfig {
        resume_waiting: true,
        ..PresetRunConfig::default()
    };
    let decisions = resume_driven_sessions(
        engine_ref.as_ref(),
        &dyn_storage,
        Some(&store),
        &[summary],
        &config,
        None,
    )
    .await;
    assert_eq!(decisions.len(), 1, "exactly one recovered session");
    match &decisions[0] {
        ResumeDecision::ReDriven {
            session_id: decision_sid,
            outcome,
        } => {
            assert_eq!(decision_sid, &SessionId(sid.to_string()));
            assert_eq!(
                outcome,
                &PresetRunOutcome::WaitingForInput { steps: 1 },
                "the reopened converge join is re-checked exactly once and \
                 parks again (1/2 arrivals, deadline not fired)"
            );
        }
        other => panic!("expected ReDriven for the parked converge join, got {other:?}"),
    }
    // No already-completed instrumented edge re-fires during the re-drive.
    assert_eq!(
        dispatch.calls.load(Ordering::SeqCst),
        0,
        "completed edges must not re-execute across kill/reopen"
    );
    // The session stayed non-terminal (parked at the join, not done). With
    // the writer fix the engine's in-memory status for a scheduler join park
    // is `Paused` — the same shape it persisted durably (A2; previously it
    // mirrored the graph-flow WaitingForInput outcome).
    assert_eq!(
        engine_ref
            .get_status(&SessionId(sid.to_string()))
            .await
            .expect("status"),
        SessionStatus::Paused,
        "the resumed converge chain must remain parked, not terminal"
    );

    // Scoped public evidence on the reopened file. The production re-drive
    // re-parked the join; the engine writer now persists the A2 scheduler
    // shape — `paused` + live join keys and NO human-wait token — so the
    // canonical A7 classifier reports `converge_merge` (rule 5), never
    // `human_wait` (a scheduler join is represented without a wait token).
    let output = nexus42(user_home)
        .args(["ops", "inspect", sid, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid inspect json");
    assert_eq!(parsed["session_id"], json!(sid));
    assert_eq!(parsed["recovery_class"], json!("converge_merge"));
    assert_eq!(parsed["resumable"]["verdict"], json!("yes"));
    assert_eq!(parsed["resumable"]["rule"], json!("chain_class_no_failure"));
    assert!(
        parsed.get("wait_id").is_none(),
        "a scheduler join park must not advertise a wait token: {parsed}"
    );
    assert_eq!(
        parsed["live_join_keys"],
        json!(["_converge_arrivals_join", "_join_wait_start_join"]),
        "the live join keys must persist on the reopened row"
    );

    // Real process-equivalent boundary: finish the first restart's proof,
    // drop the first engine/storage/runner references, and close pool_b
    // BEFORE the second restart opens the same file — a genuine shutdown
    // followed by a fresh process-equivalent startup (a live pool must not
    // mask connection-lifetime or SQLite-locking bugs).
    drop(runner);
    drop(engine_ref);
    drop(engine);
    drop(caps);
    drop(loaded);
    drop(dyn_storage);
    drop(store);
    pool_b.close().await;

    // "Second restart": a fresh process reopens the SAME file. Because the
    // re-park is now `paused` + tokenless (A2), rule 4 cannot fire — the
    // parked converge join classifies `ConvergeMerge` (rule 5) again and
    // the bounded join re-drive re-checks its deadline on EVERY later boot
    // (the shipped converge_timeout fallback stays reachable). This pins
    // the acceptance criterion "converge/merge resume still works" across
    // arbitrary restarts.
    let (pool_c, dyn_storage_c, store_c) = reopen(&db_path).await;
    let caps_c = Arc::new(CapabilityRegistry::with_builtins());
    let loaded_c = load_preset_from_str(CONVERGE_YAML, &caps_c).expect("test preset loads");
    let mut engine_c = GraphFlowEngine::new_with_storage_and_workflow_store(
        dyn_storage_c.clone(),
        store_c.clone(),
        CapabilityRegistryHolder::with_registry(caps_c.clone()),
    );
    engine_c.set_daemon_tool_dispatch(dispatch.clone());
    let engine_c = Arc::new(engine_c);
    let engine_c_ref: Arc<dyn OrchestrationEngine> = engine_c.clone();
    let wired_c = build_wired_outer_graph(
        &loaded_c,
        &engine_c_ref,
        &caps_c,
        Some(dispatch.clone()),
        None,
        engine_c.shared_state().session_cancels.clone(),
    );
    let runner_c = Arc::new(graph_flow::FlowRunner::new(
        Arc::new(wired_c),
        dyn_storage_c.clone(),
    ));
    engine_c
        .shared_state()
        .runners
        .write()
        .await
        .insert(sid.to_string(), runner_c);
    let summary_c = SessionSummary {
        session_id: SessionId(sid.to_string()),
        creator_id: "test_creator".to_string(),
        preset_id: loaded_c.id.clone(),
        status: SessionStatus::Paused,
        current_task_id: Some("join".to_string()),
    };
    engine_c
        .shared_state()
        .sessions
        .write()
        .await
        .push(summary_c.clone());
    let decisions_c = resume_driven_sessions(
        engine_c_ref.as_ref(),
        &dyn_storage_c,
        Some(&store_c),
        &[summary_c],
        &config,
        None,
    )
    .await;
    assert_eq!(
        decisions_c,
        vec![ResumeDecision::ReDriven {
            session_id: SessionId(sid.to_string()),
            outcome: PresetRunOutcome::WaitingForInput { steps: 1 },
        }],
        "the second restart must re-check the parked converge join (deadline \
         re-check); actual decisions: {decisions_c:?}"
    );
    // Row truth after the second restart: still a tokenless paused park with
    // live join keys — the join deadline stays re-checkable on every boot.
    let record_c = store_c
        .load_run(&SessionId(sid.to_string()))
        .await
        .expect("load_run")
        .expect("row exists");
    assert_eq!(record_c.status, SessionStatus::Paused);
    assert!(
        record_c.state.clone().and_then(|s| s.wait).is_none(),
        "an engine re-park must never write a human-wait token (A2)"
    );
    let context_c = dyn_storage_c
        .get(sid)
        .await
        .expect("storage get")
        .expect("row exists");
    let context_value = serde_json::to_value(&context_c.context).expect("context json");
    let live_join_keys = nexus_orchestration::resume_rules::live_join_keys(
        nexus_orchestration::resume_rules::context_data(&context_value).expect("data map"),
    );
    assert_eq!(
        live_join_keys,
        vec![
            "_converge_arrivals_join".to_string(),
            "_join_wait_start_join".to_string()
        ],
        "the join keys must survive the second restart"
    );

    pool_c.close().await;
    drop(tmp);
}

/// CONTRACT test: a converge-join park state with the ENGINE writer's durable
/// shape (`paused` + an exact current-task `_gate_park_*` marker + live join
/// keys + NO human-wait token) must be re-checked on every later boot so the
/// bounded join deadline can fire ("converge/merge resume still works").
/// `classify_recovery` uses that exact marker rather than broad join keys,
/// preventing a labeled-routed manual wait with stale keys from auto-driving.
#[tokio::test]
async fn restart_durable_status_engine_parked_join_redrives_after_reopen() {
    let (tmp, _nexus_home, db_path) = test_utils::create_test_workspace().await;
    let user_home = tmp.path();
    let sid = "conv:engine-parked";

    // "Process A" (real engine parking shape): `paused` at the converge join,
    // exact current-gate marker, live join keys (1/2 arrivals, deadline not
    // yet elapsed), and NO human-wait token.
    {
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let context = serde_json::json!({"data": {
            "_converge_arrivals_join": ["branch_a"],
            "_join_wait_start_join": chrono::Utc::now().timestamp_millis(),
            "_gate_park_join": true
        }, "chat_history": {"messages": [], "max_messages": 1000}})
        .to_string()
        .into_bytes();
        let state = run_state_v1(false, None, false, false);
        seed_v1_row(&pool, sid, "paused", Some("join"), &context, &state, 6).await;
        pool.close().await;
    }

    // "Daemon restart" over the SAME file: the bounded join re-drive must
    // re-check the deadline (Acceptance: converge/merge resume still works).
    let (pool_b, dyn_storage, store) = reopen(&db_path).await;
    let caps = Arc::new(CapabilityRegistry::with_builtins());
    let loaded = load_preset_from_str(CONVERGE_YAML, &caps).expect("test preset loads");
    let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
        dyn_storage.clone(),
        store.clone(),
        CapabilityRegistryHolder::with_registry(caps.clone()),
    );
    engine.set_daemon_tool_dispatch(Arc::new(CountingDispatch {
        calls: Arc::new(AtomicUsize::new(0)),
    }));
    let engine = Arc::new(engine);
    let engine_ref: Arc<dyn OrchestrationEngine> = engine.clone();
    let wired = build_wired_outer_graph(
        &loaded,
        &engine_ref,
        &caps,
        None,
        None,
        engine.shared_state().session_cancels.clone(),
    );
    let runner = Arc::new(graph_flow::FlowRunner::new(
        Arc::new(wired),
        dyn_storage.clone(),
    ));
    engine
        .shared_state()
        .runners
        .write()
        .await
        .insert(sid.to_string(), runner);
    let summary = SessionSummary {
        session_id: SessionId(sid.to_string()),
        creator_id: "test_creator".to_string(),
        preset_id: loaded.id.clone(),
        status: SessionStatus::Paused,
        current_task_id: Some("join".to_string()),
    };
    engine
        .shared_state()
        .sessions
        .write()
        .await
        .push(summary.clone());
    let config = PresetRunConfig {
        resume_waiting: true,
        ..PresetRunConfig::default()
    };
    let decisions = resume_driven_sessions(
        engine_ref.as_ref(),
        &dyn_storage,
        Some(&store),
        &[summary],
        &config,
        None,
    )
    .await;
    // CONTRACT: the parked converge join re-checks its deadline on this
    // boot (ReDriven with a single re-step that re-parks — the tokenless
    // `paused` shape routes rule 5, never `SkippedHumanWait`).
    assert_eq!(
        decisions,
        vec![ResumeDecision::ReDriven {
            session_id: SessionId(sid.to_string()),
            outcome: PresetRunOutcome::WaitingForInput { steps: 1 },
        }],
        "converge/merge resume must survive restart for an engine-parked \
         join (deadline re-check); actual decisions: {decisions:?}"
    );

    // Scoped public evidence on the reopened file (truthful DTO): the row
    // stays a tokenless converge/merge park with live join keys.
    let output = nexus42(user_home)
        .args(["ops", "inspect", sid, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid inspect json");
    assert_eq!(parsed["recovery_class"], json!("converge_merge"));
    assert_eq!(
        parsed["live_join_keys"],
        json!(["_converge_arrivals_join", "_join_wait_start_join"])
    );
    assert!(
        parsed.get("wait_id").is_none(),
        "an engine-parked join must never advertise a wait token: {parsed}"
    );

    pool_b.close().await;
    drop(tmp);
}

// ---------------------------------------------------------------------------
// Scenario 6 — nested child human wait survives restart: parent/root wait
// names the exact waiting child, the nested token is preserved (never
// auto-consumed/cleared by boot), and boot skips the wait entirely.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn restart_durable_status_nested_child_wait_preserved_and_never_auto_resumed() {
    let (tmp, _nexus_home, db_path) = test_utils::create_test_workspace().await;
    let user_home = tmp.path();

    // "Process A": a real SQLite seed of the ENGINE-PRODUCED nested shape:
    //   parent row: waiting_for_input + root WaitRecord naming the child
    //   (child_session_id + child_task_id + its own fresh token);
    //   child row: waiting_for_input + its own durable token (the child
    //   parked exactly once — the auto-resume loop is forbidden).
    {
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let parent_state = serde_json::json!({
            "wait": {
                "wait_id": "parent-tok-9", "task_id": "parent_state",
                "child_session_id": "nested:parent::child:aaa",
                "child_task_id": "manual_wait_task", "kind": "manual"
            },
            "step_in_flight": null, "in_flight": null, "failure": null,
            "cancel_requested": false,
        })
        .to_string()
        .into_bytes();
        let child_state = serde_json::json!({
            "wait": {
                "wait_id": "child-tok-9", "task_id": "manual_wait_task",
                "child_session_id": null, "child_task_id": null, "kind": "manual"
            },
            "step_in_flight": null, "in_flight": null, "failure": null,
            "cancel_requested": false,
        })
        .to_string()
        .into_bytes();
        let ctx = plain_context();
        // Parent row.
        seed_v1_row(
            &pool,
            "nested:parent",
            "waiting_for_input",
            Some("parent_state"),
            &ctx,
            &parent_state,
            8,
        )
        .await;
        // Child row: same schema with parent_session_id column.
        sqlx::query(
            "INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version, parent_session_id,
                 current_task_id, status, context_json, created_at, updated_at,
                 execution_version, state_revision, run_state_json, run_descriptor_json)
             VALUES (?, 'test_creator', 'e2e-converge', 3, ?, 'manual_wait_task',
                     'waiting_for_input', ?, 1_756_990_000, 1_756_990_300,
                     1, 3, ?, ?)",
        )
        .bind("nested:parent::child:aaa")
        .bind("nested:parent")
        .bind(&ctx)
        .bind(&child_state)
        .bind(CHILD_DESCRIPTOR)
        .execute(&pool)
        .await
        .expect("seed nested child row");
        pool.close().await;
    }

    // "Daemon restart": boot must skip the nested wait (HumanWait — never
    // stepped), the parent token AND the nested child token must be
    // byte-preserved.
    let (pool_b, dyn_storage, store) = reopen(&db_path).await;

    let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
        dyn_storage.clone(),
        store.clone(),
        CapabilityRegistryHolder::with_registry(Arc::new(CapabilityRegistry::with_builtins())),
    );
    let summary = SessionSummary {
        session_id: SessionId("nested:parent".to_string()),
        creator_id: "test_creator".to_string(),
        preset_id: "e2e-converge".to_string(),
        status: SessionStatus::WaitingForInput,
        current_task_id: Some("parent_state".to_string()),
    };
    let decisions = resume_driven_sessions(
        &engine,
        &dyn_storage,
        Some(&store),
        &[summary],
        &PresetRunConfig::default(),
        None,
    )
    .await;
    assert_eq!(
        decisions,
        vec![ResumeDecision::SkippedHumanWait {
            session_id: SessionId("nested:parent".to_string())
        }],
        "a nested child wait must be skipped at boot — never auto-advanced"
    );

    // Parent token + child identity survive byte-identical.
    let parent = store
        .load_run(&SessionId("nested:parent".to_string()))
        .await
        .expect("load_run parent")
        .expect("parent present");
    assert_eq!(parent.status, SessionStatus::WaitingForInput);
    let pw = parent
        .state
        .as_ref()
        .and_then(|s| s.wait.as_ref())
        .expect("parent wait");
    assert_eq!(pw.wait_id, "parent-tok-9", "parent token preserved");
    assert_eq!(
        pw.child_session_id.as_deref(),
        Some("nested:parent::child:aaa"),
        "parent wait names the exact waiting child"
    );
    assert_eq!(
        pw.child_task_id.as_deref(),
        Some("manual_wait_task"),
        "parent wait carries the child cursor"
    );

    // The nested child token is untouched (no boot consume/clear).
    let child = store
        .load_run(&SessionId("nested:parent::child:aaa".to_string()))
        .await
        .expect("load_run child")
        .expect("child present");
    assert_eq!(
        child.status,
        SessionStatus::WaitingForInput,
        "child still waiting"
    );
    assert_eq!(
        child
            .state
            .as_ref()
            .and_then(|s| s.wait.as_ref())
            .map(|w| w.wait_id.as_str()),
        Some("child-tok-9"),
        "the nested child wait token survives restart untouched — no auto-resume"
    );

    // The child's persisted `RunDescriptorV1` survives reopen through the
    // real storage load path with its inherited identity intact (A2/A4):
    // creator, preset id/version, embedded source, parent session, and the
    // named inner graph — the reconstructible child identity the production
    // `spawn_child_session_internal` writes.
    let child_descriptor = child
        .descriptor
        .as_ref()
        .expect("v1 child row must carry a frozen descriptor");
    assert_eq!(
        child_descriptor.creator_id, "test_creator",
        "child must inherit the root creator identity"
    );
    assert_eq!(
        child_descriptor.preset_id, "e2e-converge",
        "child must inherit the root preset identity"
    );
    assert_eq!(
        child_descriptor.preset_version, 3,
        "child must inherit the root preset version"
    );
    assert_eq!(
        child_descriptor.source,
        PresetSourceIdentity::Embedded {
            preset_id: "e2e-converge".to_string(),
            content_hash: [0u8; 32],
        },
        "child must inherit the root embedded source identity"
    );
    assert_eq!(
        child_descriptor
            .parent_session_id
            .as_ref()
            .map(|s| s.0.as_str()),
        Some("nested:parent"),
        "child must name its parent session"
    );
    assert_eq!(
        child_descriptor.graph_name.as_deref(),
        Some("manual_wait_graph"),
        "child must name its inner graph"
    );

    // Scoped public evidence: inspect reports human_wait with the preserved
    // wait identity on the reopened file.
    let output = nexus42(user_home)
        .args(["ops", "inspect", "nested:parent", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid inspect json");
    assert_eq!(parsed["db_status"], json!("waiting_for_input"));
    assert_eq!(parsed["recovery_class"], json!("human_wait"));
    assert_eq!(parsed["wait_id"], json!("parent-tok-9"));
    assert_eq!(parsed["resumable"]["verdict"], json!("no"));

    pool_b.close().await;
    drop(tmp);
}

// ---------------------------------------------------------------------------
// P3 T1 — daemon-level restart matrix (A7): production boot/attach path over
// the SAME DB/HOME, driven through `LiveDaemon::restart()` (abort drives →
// republish bundle → run_boot_recovery). Source-verified reattachment: the
// frozen source identity (manifest + referenced template bytes) is verified
// at reconstruction; a changed/missing user preset preserves the human wait
// and makes continue return `reconstruction_unavailable` (cancel-only).
// ---------------------------------------------------------------------------

/// Deterministic non-echo Host fixture (mirrors the P2 MockHost): every
/// prompt returns a transformed string, records the rendered prompt, and
/// advertises `mock-provider` so admission's binding gate validates.
struct RestartMockHost {
    prompts: Mutex<Vec<String>>,
    execs: AtomicUsize,
}

impl RestartMockHost {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            prompts: Mutex::new(Vec::new()),
            execs: AtomicUsize::new(0),
        })
    }
}

#[async_trait::async_trait]
impl nexus_agent_host::HostFacade for RestartMockHost {
    async fn start(&self, _config: HostStartConfig) -> HostResult<()> {
        Ok(())
    }
    async fn create_session(&self, request: CreateSessionRequest) -> HostResult<HostSession> {
        Ok(HostSession {
            id: HostSessionId::new(),
            provider_id: request.provider_id,
            state: SessionState::Ready,
            created_at: chrono::Utc::now(),
            active_op_id: None,
            negotiated_capabilities: CapabilityDescriptor::native_cli_limited(),
            owner: request.owner,
            process_identity: None,
        })
    }
    async fn exec(
        &self,
        session_id: HostSessionId,
        op: HostOperation,
    ) -> HostResult<HostEventStream> {
        self.execs.fetch_add(1, Ordering::SeqCst);
        let op_id = match op {
            HostOperation::Prompt { op_id, content, .. } => {
                let text = match content.as_slice() {
                    [HostContentBlock::Text { text }] => text.clone(),
                    other => format!("unexpected content {other:?}"),
                };
                self.prompts.lock().expect("prompts").push(text);
                op_id
            }
            other => {
                return Err(HostError::internal(format!(
                    "unexpected operation {other:?}"
                )));
            }
        };
        let started = HostEvent::OpStarted(OperationStartedEvent {
            op_id: op_id.clone(),
            session_id: session_id.clone(),
        });
        let delta = HostEvent::MessageDelta(TextDeltaEvent {
            session_id: session_id.clone(),
            op_id: op_id.clone(),
            text: "transformed:restart-output".to_string(),
        });
        let finished = HostEvent::OpFinished(OperationFinishedEvent {
            session_id,
            op_id,
            reason: FinishReason::EndTurn,
        });
        Ok(Box::pin(futures_util::stream::iter(vec![
            Ok(started),
            Ok(delta),
            Ok(finished),
        ])))
    }
    async fn cancel(&self, _op_id: HostOperationId) -> HostResult<()> {
        Ok(())
    }
    async fn health(&self) -> HostResult<HostHealth> {
        Ok(HostHealth {
            running: true,
            active_sessions: 0,
            active_operations: 0,
        })
    }
    async fn shutdown(&self) -> HostResult<()> {
        Ok(())
    }
    async fn shutdown_session(&self, _session_id: HostSessionId) -> HostResult<()> {
        Ok(())
    }
    async fn list_sessions(&self) -> HostResult<Vec<HostSession>> {
        Ok(Vec::new())
    }
    async fn provider_catalog(&self) -> HostResult<ProviderCatalog> {
        Ok(ProviderCatalog {
            entries: vec![ProviderCatalogEntry {
                provider_id: nexus_agent_host::ProviderId::new("mock-provider"),
                display_name: "mock-provider".to_string(),
                protocol_kind: ProtocolKind::NativeCli,
                launch: LaunchStrategy::NativeCli {
                    command: "mock".to_string(),
                    args: vec![],
                    env: HashMap::new(),
                },
                source: DiscoverySource::Config,
                trust: TrustLevel::Explicit,
                capabilities: CapabilityDescriptor::native_cli_limited(),
                health: ProviderHealth {
                    provider_id: nexus_agent_host::ProviderId::new("mock-provider"),
                    available: true,
                    latency_ms: None,
                    message: None,
                },
            }],
        })
    }
    fn subscribe_events(
        &self,
        _session_id: HostSessionId,
    ) -> tokio::sync::broadcast::Receiver<HostEvent> {
        // The daemon's prompt executor drains its own event stream; an
        // empty channel is sufficient for this fixture.
        let (_tx, rx): (
            tokio::sync::broadcast::Sender<HostEvent>,
            tokio::sync::broadcast::Receiver<HostEvent>,
        ) = tokio::sync::broadcast::channel(16);
        let _ = _tx;
        rx
    }
}

/// Public schedule add through the real HTTP surface (P3 fixture).
async fn post_schedule(
    daemon: &LiveDaemon,
    preset_id: &str,
    label: &str,
) -> (reqwest::StatusCode, String, String) {
    let resp = reqwest::Client::new()
        .post(format!(
            "{}/v1/daemon/orchestration/schedules",
            daemon.http_url
        ))
        .json(&json!({
            "creator_id": "test_creator",
            "preset_id": preset_id,
            "label": label,
            "seed": "p3-restart",
            "input": { "topic": "p3-restart" },
            "agent_bindings": { "default": { "provider_id": "mock-provider" } }
        }))
        .send()
        .await
        .expect("POST schedule");
    let status = resp.status();
    let body: Value = resp.json().await.expect("schedule json");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    (
        status,
        schedule_id,
        body["status"].as_str().unwrap_or("").to_string(),
    )
}

/// Read the durable v1 run state (status + wait token) for a session.
async fn durable_run_state(pool: &sqlx::SqlitePool, session_id: &str) -> (String, Option<Value>) {
    let (status, state): (String, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT status, run_state_json FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .expect("load run state");
    let state: Option<Value> = state.map(|b| serde_json::from_slice(&b).expect("run state json"));
    (status, state)
}

/// Wait until a schedule owns a session and the session reaches `expected`.
async fn wait_owned_status(
    daemon: &LiveDaemon,
    schedule_id: &str,
    expected: &str,
    timeout_secs: u64,
) -> String {
    tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
        loop {
            let (_, current_session_id): (String, Option<String>) = sqlx::query_as(
                "SELECT status, current_session_id FROM creator_schedules WHERE schedule_id = ?",
            )
            .bind(schedule_id)
            .fetch_one(&daemon.pool)
            .await
            .expect("schedule row");
            if let Some(sid) = current_session_id {
                let (run_status, _) = durable_run_state(&daemon.pool, &sid).await;
                if run_status == expected {
                    return sid;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("owned session reached expected status")
}

/// Write a user preset bundle (manifest + referenced template) under
/// `$HOME/.nexus42/presets/<id>/` — the A7 directory-source shape.
fn write_user_wait_preset(home: &Path, template_body: &str) {
    let bundle = home
        .join(".nexus42")
        .join("presets")
        .join("p3-restart-wait");
    std::fs::create_dir_all(bundle.join("prompts")).expect("bundle dirs");
    std::fs::write(
        bundle.join("preset.yaml"),
        r#"preset:
  id: p3-restart-wait
  version: 1
  kind: creator
  description: "P3 restart matrix — inner prompt then human wait"
  requires_capabilities: []
  initial: gate
  terminal: done
inner_graphs:
  gen:
    nodes:
      - id: n1
        kind: acp_prompt
        template_file: prompts/gen.md
        tool_policy: auto_grant_read_only
states:
  - id: gate
    enter:
      - kind: inner_graph
        name: gen
    exit_when:
      kind: graph_complete
    next: wait
  - id: wait
    exit_when:
      kind: manual
    next: done
  - id: done
    terminal: true
"#,
    )
    .expect("preset.yaml");
    std::fs::write(bundle.join("prompts").join("gen.md"), template_body).expect("gen.md");
}

/// A7 rule 4 at the daemon level: a user-directory-preset human wait survives
/// a daemon-level restart over the SAME DB/HOME (production boot/attach
/// path). The wait token is preserved byte-identical, boot never steps it,
/// and a matching continue after restart reattaches the runner from the
/// FROZEN source and completes WITHOUT replaying the completed inner graph
/// prompt. No auto-approval.
#[tokio::test]
async fn daemon_restart_user_wait_reextracts_and_continues() {
    let host = RestartMockHost::new();
    let mut daemon = LiveDaemon::start_with_host_provider_and_dispatch(
        host.clone(),
        Arc::new(CountingDispatch {
            calls: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .await;
    write_user_wait_preset(daemon.home.path(), "template v1 body");
    let (status, schedule_id, _) = post_schedule(&daemon, "p3-restart-wait", "p3-wait").await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "schedule admitted");

    // The run drives gate → inner prompt (Host) → manual wait.
    let sid = wait_owned_status(&daemon, &schedule_id, "waiting_for_input", 20).await;
    let (_, state) = durable_run_state(&daemon.pool, &sid).await;
    let wait_id = state
        .as_ref()
        .and_then(|s| s["wait"]["wait_id"].as_str())
        .expect("durable wait token")
        .to_string();
    let effects_before = host.execs.load(Ordering::SeqCst);
    // The inner poller steps the single-node child until its bound; the
    // durable wait parks only after the gate advances. The exact count is
    // harness-shaped — what matters is the NO-REPLAY guarantee below.
    assert!(effects_before >= 1, "the inner prompt must have executed");

    // Daemon-level restart over the same DB/HOME: production boot/attach.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    daemon.restart().await;

    // The wait SURVIVES untouched: no auto-approval, no consume.
    let (run_status, state_after) = durable_run_state(&daemon.pool, &sid).await;
    assert_eq!(
        run_status, "waiting_for_input",
        "still waiting after restart"
    );
    assert_eq!(
        state_after
            .as_ref()
            .and_then(|s| s["wait"]["wait_id"].as_str()),
        Some(wait_id.as_str()),
        "the durable A4 wait token survives the daemon-level restart"
    );

    // A matching continue reattaches the runner from the frozen source and
    // completes. The re-drive re-attaches the terminal child (never
    // re-steps the completed prompt) — one more manual-exit step to done.
    let resp = reqwest::Client::new()
        .post(format!(
            "{}/v1/daemon/orchestration/sessions/{sid}/signal",
            daemon.http_url
        ))
        .json(&json!({ "signal": "continue", "waitId": wait_id }))
        .send()
        .await
        .expect("continue");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "authorized continue"
    );
    // The re-drive runs in the background (single-flight owner); wait for
    // the durable terminal.
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let (run_status, _) = durable_run_state(&daemon.pool, &sid).await;
            if run_status == "completed" {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("continued run completes");
    assert_eq!(
        host.execs.load(Ordering::SeqCst),
        effects_before,
        "the completed inner-graph prompt must NOT replay across restart+continue"
    );
}

/// A7 rule 4 changed/missing source at the daemon level: after the wait parks
/// with a frozen directory identity, the referenced TEMPLATE is changed; the
/// daemon-level restart preserves the wait, and a matching continue returns
/// `reconstruction_unavailable` (409, cancel-only) instead of driving a
/// session whose frozen source no longer matches.
#[tokio::test]
async fn daemon_restart_changed_template_continue_refuses_cancel_only() {
    let host = RestartMockHost::new();
    let mut daemon = LiveDaemon::start_with_host_provider_and_dispatch(
        host.clone(),
        Arc::new(CountingDispatch {
            calls: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .await;
    write_user_wait_preset(daemon.home.path(), "template v1 body");
    let (_status, schedule_id, _) = post_schedule(&daemon, "p3-restart-wait", "p3-changed").await;

    let sid = wait_owned_status(&daemon, &schedule_id, "waiting_for_input", 20).await;
    let (_, state) = durable_run_state(&daemon.pool, &sid).await;
    let wait_id = state
        .as_ref()
        .and_then(|s| s["wait"]["wait_id"].as_str())
        .expect("durable wait token")
        .to_string();

    // Change the referenced template bytes AFTER the frozen identity was
    // stamped at admission: the source hash no longer matches.
    write_user_wait_preset(daemon.home.path(), "CHANGED template body");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    daemon.restart().await;

    // The wait survives (source identity is only verified ON DEMAND, at a
    // matching continue — boot never steps a human wait).
    let (run_status, state_after) = durable_run_state(&daemon.pool, &sid).await;
    assert_eq!(
        run_status, "waiting_for_input",
        "wait preserved under changed source"
    );
    assert_eq!(
        state_after
            .as_ref()
            .and_then(|s| s["wait"]["wait_id"].as_str()),
        Some(wait_id.as_str()),
        "wait token preserved under changed source"
    );

    // Matching continue: on-demand reattachment verifies the frozen hash and
    // refuses with reconstruction_unavailable (409), cancel-only. No drive,
    // no mutation, no new effects.
    let effects_before = host.execs.load(Ordering::SeqCst);
    let resp = reqwest::Client::new()
        .post(format!(
            "{}/v1/daemon/orchestration/sessions/{sid}/signal",
            daemon.http_url
        ))
        .json(&json!({ "signal": "continue", "waitId": wait_id }))
        .send()
        .await
        .expect("continue");
    assert_eq!(resp.status(), reqwest::StatusCode::CONFLICT, "409 conflict");
    let body: Value = resp.json().await.expect("error json");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("reconstruction_unavailable"),
        "{body}"
    );
    let actions = body["error"]["details"]["allowed_actions"]
        .as_array()
        .expect("details");
    assert_eq!(
        serde_json::Value::Array(actions.clone()),
        json!(["cancel", "new_run"]),
        "cancel-only actions"
    );
    let (run_status, state_now) = durable_run_state(&daemon.pool, &sid).await;
    assert_eq!(
        run_status, "waiting_for_input",
        "wait still preserved after refused continue"
    );
    assert_eq!(
        state_now
            .as_ref()
            .and_then(|s| s["wait"]["wait_id"].as_str()),
        Some(wait_id.as_str()),
        "token untouched by the refused continue"
    );
    assert_eq!(
        host.execs.load(Ordering::SeqCst),
        effects_before,
        "no effects"
    );
}

/// A7 rule 4 missing source at the daemon level: deleting the user preset
/// bundle after admission preserves the wait and continue refuses with
/// `reconstruction_unavailable` (cancel-only) — never a fallback to current
/// embedded/other bytes.
#[tokio::test]
async fn daemon_restart_missing_source_continue_refuses_cancel_only() {
    let host = RestartMockHost::new();
    let mut daemon = LiveDaemon::start_with_host_provider_and_dispatch(
        host.clone(),
        Arc::new(CountingDispatch {
            calls: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .await;
    write_user_wait_preset(daemon.home.path(), "template v1 body");
    let (_status, schedule_id, _) = post_schedule(&daemon, "p3-restart-wait", "p3-missing").await;

    let sid = wait_owned_status(&daemon, &schedule_id, "waiting_for_input", 20).await;
    let (_, state) = durable_run_state(&daemon.pool, &sid).await;
    let wait_id = state
        .as_ref()
        .and_then(|s| s["wait"]["wait_id"].as_str())
        .expect("durable wait token")
        .to_string();

    // Delete the user bundle (source moved/gone).
    let bundle = daemon
        .home
        .path()
        .join(".nexus42")
        .join("presets")
        .join("p3-restart-wait");
    std::fs::remove_dir_all(&bundle).expect("remove bundle");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    daemon.restart().await;

    // Wait preserved; continue → reconstruction_unavailable, cancel-only.
    let (run_status, state_after) = durable_run_state(&daemon.pool, &sid).await;
    assert_eq!(run_status, "waiting_for_input");
    assert_eq!(
        state_after
            .as_ref()
            .and_then(|s| s["wait"]["wait_id"].as_str()),
        Some(wait_id.as_str())
    );
    let resp = reqwest::Client::new()
        .post(format!(
            "{}/v1/daemon/orchestration/sessions/{sid}/signal",
            daemon.http_url
        ))
        .json(&json!({ "signal": "continue", "waitId": wait_id }))
        .send()
        .await
        .expect("continue");
    assert_eq!(resp.status(), reqwest::StatusCode::CONFLICT);
    let body: Value = resp.json().await.expect("error json");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("reconstruction_unavailable"),
        "{body}"
    );
    assert_eq!(
        body["error"]["details"]["allowed_actions"],
        json!(["cancel", "new_run"]),
        "cancel-only actions"
    );
}

// ---------------------------------------------------------------------------
// P3 T1 remaining daemon-level restart scenarios (plan matrix):
//   (a) corrupt child identity → wait preserved, continue reconstruction_unavailable
//   (b) nested/multiple waits preserved, no auto-approval, no new IDs
//   (c) dispatch-before-output crash → interrupted, no retry, refuse signals
//   (d) final-checkpoint-before-settlement crash → schedule settles terminal
// ---------------------------------------------------------------------------

/// `RunDescriptorV1` for the REAL embedded `memory-augmented` preset (A7
/// frozen identity that actually resolves at reconstruction — never a zero
/// hash). `parent` selects the nested-child shape (parent session + inner
/// graph name).
fn memory_augmented_descriptor(parent: Option<(&str, &str)>) -> Vec<u8> {
    let source = nexus_orchestration::preset::embedded_source_identity("memory-augmented")
        .expect("memory-augmented embedded source identity");
    let (parent_session_id, graph_name) = match parent {
        Some((parent_sid, graph)) => (Some(parent_sid.to_string()), Some(graph.to_string())),
        None => (None, None),
    };
    serde_json::to_vec(&json!({
        "creator_id": "test_creator",
        "work_id": null,
        "workspace_root": "",
        "preset_id": "memory-augmented",
        "preset_version": 1,
        "source": source,
        "input": {},
        "agent_bindings": {},
        "parent_session_id": parent_session_id,
        "graph_name": graph_name
    }))
    .expect("memory-augmented descriptor json")
}

/// The same frozen descriptor with the sanctioned `default` role bound to
/// the fixture provider — a re-driven run needs a resolvable binding or the
/// first prompt step refuses (N-9).
fn memory_augmented_descriptor_bound() -> Vec<u8> {
    let mut value: Value =
        serde_json::from_slice(&memory_augmented_descriptor(None)).expect("descriptor json");
    value["agent_bindings"] = json!({"default": {"provider_id": "mock-provider", "model": null}});
    serde_json::to_vec(&value).expect("bound descriptor json")
}

/// Seed a durable v1 session row with an explicit descriptor and optional
/// parent (daemon-level restart fixtures).
#[allow(clippy::too_many_arguments)]
async fn seed_daemon_session(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    status: &str,
    current_task_id: Option<&str>,
    context: &[u8],
    run_state: &[u8],
    state_revision: i64,
    descriptor: &[u8],
    parent_session_id: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, parent_session_id,
             current_task_id, status, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES (?, 'test_creator', 'memory-augmented', 1, ?, ?, ?, ?, 1_756_990_000, 1_756_990_300,
                 1, ?, ?, ?)",
    )
    .bind(session_id)
    .bind(parent_session_id)
    .bind(current_task_id)
    .bind(status)
    .bind(context)
    .bind(state_revision)
    .bind(run_state)
    .bind(descriptor)
    .execute(pool)
    .await
    .expect("seed daemon session");
}

/// Durable `RunStateV1` with a human-wait record carrying the exact token
/// (A4 shape; optional waiting-child cursor for nested waits).
fn wait_state_with_token(
    token: &str,
    task: &str,
    child_session: Option<&str>,
    child_task: Option<&str>,
) -> Vec<u8> {
    json!({
        "wait": {
            "wait_id": token, "task_id": task,
            "child_session_id": child_session, "child_task_id": child_task, "kind": "manual"
        },
        "step_in_flight": null, "in_flight": null, "failure": null, "cancel_requested": false
    })
    .to_string()
    .into_bytes()
}

/// Durable `RunStateV1` for a dispatch-before-output crash (A2): the prompt
/// was persisted `dispatching`/`active` before the Host effect completed and
/// the step never checkpointed past it.
fn dispatch_crash_state() -> Vec<u8> {
    json!({
        "wait": null,
        "step_in_flight": "generate",
        "in_flight": {
            "attempt_id": "attempt-crash", "task_id": "generate", "phase": "active",
            "host_session_id": "host-crash", "operation_id": "op-crash", "process_identity": null
        },
        "failure": null,
        "cancel_requested": false
    })
    .to_string()
    .into_bytes()
}

/// Post a session control signal over the live HTTP surface and return
/// (status, parsed body).
async fn post_session_signal(
    daemon: &LiveDaemon,
    session_id: &str,
    payload: Value,
) -> (reqwest::StatusCode, Value) {
    let resp = reqwest::Client::new()
        .post(format!(
            "{}/v1/daemon/orchestration/sessions/{session_id}/signal",
            daemon.http_url
        ))
        .json(&payload)
        .send()
        .await
        .expect("session signal");
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(json!({}));
    (status, body)
}

/// (b) Nested/multiple waits at the daemon level: a parent with TWO waiting
/// children survives the production boot/attach restart with every wait
/// token preserved, boot never auto-approves either child, and no new
/// session/child IDs are minted (existing IDs reattached).
#[tokio::test]
async fn daemon_restart_nested_multiple_waits_preserved_no_auto_approval() {
    let host = RestartMockHost::new();
    let mut daemon = LiveDaemon::start_with_host_provider_and_dispatch(
        host.clone(),
        Arc::new(CountingDispatch {
            calls: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .await;

    let parent = "daemon:nested-parent";
    let child_a = "daemon:nested-parent::child:a";
    let child_b = "daemon:nested-parent::child:b";
    let descriptor = memory_augmented_descriptor(None);
    let child_descriptor = memory_augmented_descriptor(Some((parent, "generate_graph")));
    seed_daemon_session(
        &daemon.pool,
        parent,
        "waiting_for_input",
        Some("persist"),
        &plain_context(),
        &wait_state_with_token("parent-tok-p3", "persist", Some(child_a), Some("persist")),
        8,
        &descriptor,
        None,
    )
    .await;
    seed_daemon_session(
        &daemon.pool,
        child_a,
        "waiting_for_input",
        Some("persist"),
        &plain_context(),
        &wait_state_with_token("child-a-tok", "persist", None, None),
        3,
        &child_descriptor,
        Some(parent),
    )
    .await;
    seed_daemon_session(
        &daemon.pool,
        child_b,
        "waiting_for_input",
        Some("persist"),
        &plain_context(),
        &wait_state_with_token("child-b-tok", "persist", None, None),
        3,
        &child_descriptor,
        Some(parent),
    )
    .await;
    let sessions_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions")
        .fetch_one(&daemon.pool)
        .await
        .expect("session count before");

    daemon.restart().await;

    // Every wait token survives; all three rows still waiting; no new IDs.
    for (sid, tok) in [
        (parent, "parent-tok-p3"),
        (child_a, "child-a-tok"),
        (child_b, "child-b-tok"),
    ] {
        let (status, state) = durable_run_state(&daemon.pool, sid).await;
        assert_eq!(status, "waiting_for_input", "{sid} still waiting");
        assert_eq!(
            state.and_then(|s| s["wait"]["wait_id"].as_str().map(str::to_string)),
            Some(tok.to_string()),
            "{sid} token preserved"
        );
    }
    let sessions_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions")
        .fetch_one(&daemon.pool)
        .await
        .expect("session count after");
    assert_eq!(
        sessions_after, sessions_before,
        "daemon restart must not mint new session/child IDs"
    );
    assert_eq!(
        host.execs.load(Ordering::SeqCst),
        0,
        "boot never steps nested/multiple waits"
    );
}

/// (a) Corrupt child identity at the daemon level: a persisted child with an
/// unsupported execution version blocks the parent's owned-descendant
/// reconstruction. The parent's human wait is preserved; a matching continue
/// refuses with `reconstruction_unavailable` (409, cancel-only); no new
/// effects are created and the corrupt child row is preserved verbatim.
#[tokio::test]
async fn daemon_restart_corrupt_child_identity_wait_preserved_continue_refuses() {
    let host = RestartMockHost::new();
    let mut daemon = LiveDaemon::start_with_host_provider_and_dispatch(
        host.clone(),
        Arc::new(CountingDispatch {
            calls: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .await;

    let parent = "daemon:corrupt-parent";
    let child = "daemon:corrupt-parent::child:bad";
    seed_daemon_session(
        &daemon.pool,
        parent,
        "waiting_for_input",
        Some("persist"),
        &plain_context(),
        &wait_state_with_token("parent-tok-corr", "persist", Some(child), Some("persist")),
        8,
        &memory_augmented_descriptor(None),
        None,
    )
    .await;
    // Corrupt child: unsupported execution_version 2 → load_children refuses
    // (A7 non-replayable).
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, parent_session_id,
             current_task_id, status, context_json, created_at, updated_at,
             execution_version, state_revision, run_state_json, run_descriptor_json)
         VALUES (?, 'test_creator', 'memory-augmented', 1, ?, 'persist', 'waiting_for_input',
                 ?, 1_756_990_000, 1_756_990_300, 2, 3, ?, ?)",
    )
    .bind(child)
    .bind(parent)
    .bind(plain_context())
    .bind(wait_state_with_token(
        "child-bad-tok",
        "persist",
        None,
        None,
    ))
    .bind(memory_augmented_descriptor(Some((
        parent,
        "generate_graph",
    ))))
    .execute(&daemon.pool)
    .await
    .expect("seed corrupt child");

    daemon.restart().await;

    // The wait is preserved (recovery surfaces the corrupt child as
    // non-replayable but never steps the human wait).
    let (status, state) = durable_run_state(&daemon.pool, parent).await;
    assert_eq!(status, "waiting_for_input", "parent wait preserved");
    assert_eq!(
        state.as_ref().and_then(|s| s["wait"]["wait_id"].as_str()),
        Some("parent-tok-corr"),
        "parent token preserved under corrupt child identity"
    );
    // The corrupt child row is preserved verbatim (never reinterpreted).
    let (child_status, _) = durable_run_state(&daemon.pool, child).await;
    assert_eq!(
        child_status, "waiting_for_input",
        "corrupt child row preserved"
    );

    // Matching continue: the on-demand reattachment hydrates the owned
    // closure, hits the corrupt child, and refuses reconstruction_unavailable
    // (cancel-only) — no mutation, no effects.
    let effects_before = host.execs.load(Ordering::SeqCst);
    let (code, body) = post_session_signal(
        &daemon,
        parent,
        json!({ "signal": "continue", "waitId": "parent-tok-corr" }),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("reconstruction_unavailable"),
        "{body}"
    );
    assert_eq!(
        body["error"]["details"]["allowed_actions"],
        json!(["cancel", "new_run"]),
        "cancel-only actions"
    );
    let (status, state_now) = durable_run_state(&daemon.pool, parent).await;
    assert_eq!(
        status, "waiting_for_input",
        "wait untouched by refused continue"
    );
    assert_eq!(
        state_now
            .as_ref()
            .and_then(|s| s["wait"]["wait_id"].as_str()),
        Some("parent-tok-corr"),
        "token untouched"
    );
    assert_eq!(
        host.execs.load(Ordering::SeqCst),
        effects_before,
        "no effects"
    );
}

/// (c) Dispatch-before-output crash at the daemon level: the durable record
/// carries a dispatching/active prompt with an unfinished step mark (the
/// crash happened after the external effect but before the result
/// checkpoint). The restart classifies interrupted, NEVER re-drives (zero
/// Host effects), never rewrites the row, and refuses control signals with
/// `workflow_state_conflict`.
#[tokio::test]
async fn daemon_restart_dispatch_before_output_crash_interrupted_no_retry() {
    let host = RestartMockHost::new();
    let mut daemon = LiveDaemon::start_with_host_provider_and_dispatch(
        host.clone(),
        Arc::new(CountingDispatch {
            calls: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .await;

    let sid = "daemon:dispatch-crash";
    seed_daemon_session(
        &daemon.pool,
        sid,
        "running",
        Some("generate"),
        &plain_context(),
        &dispatch_crash_state(),
        5,
        &memory_augmented_descriptor(None),
        None,
    )
    .await;

    daemon.restart().await;

    // Never retried: the durable row is untouched (status + interrupted
    // marks), zero Host effects even after a grace window.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let (status, state) = durable_run_state(&daemon.pool, sid).await;
    assert_eq!(status, "running", "durable row never rewritten by recovery");
    let state = state.expect("durable state present");
    assert_eq!(
        state["step_in_flight"],
        json!("generate"),
        "step mark preserved"
    );
    assert!(
        state["in_flight"].is_object(),
        "the unfinished prompt attempt is preserved (never cleared/retried)"
    );
    assert_eq!(
        host.execs.load(Ordering::SeqCst),
        0,
        "interrupted in-flight work must NOT retry across daemon restart"
    );

    // Advance cannot resurrect the interrupted run: 409 workflow_state_conflict.
    let (code, body) = post_session_signal(&daemon, sid, json!({ "signal": "advance" })).await;
    assert_eq!(code, reqwest::StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("workflow_state_conflict"),
        "{body}"
    );

    // Scoped public evidence: inspect projects the interrupted recovery class
    // (never a retryable verdict).
    let output = nexus42(daemon.home.path())
        .args(["ops", "inspect", sid, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&output).expect("valid inspect json");
    assert_eq!(parsed["recovery_class"], json!("interrupted"));
    assert_eq!(parsed["resumable"]["verdict"], json!("no"));
}

/// (d) Final-checkpoint-before-settlement crash at the daemon level: the
/// durable session is terminal `completed` but the schedule row still reads
/// `running` (the settlement crashed before the flip). The restart's
/// production bundle publication reconciles the schedule from the durable
/// terminal session (settles to completed), and recovery never re-drives the
/// completed session — zero effects, one session, owned-identity preserved.
#[tokio::test]
async fn daemon_restart_final_checkpoint_before_settlement_settles_terminal() {
    let host = RestartMockHost::new();
    let mut daemon = LiveDaemon::start_with_host_provider_and_dispatch(
        host.clone(),
        Arc::new(CountingDispatch {
            calls: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .await;

    let sid = "daemon:settle-crash";
    seed_daemon_session(
        &daemon.pool,
        sid,
        "completed",
        Some("done"),
        &plain_context(),
        &run_state_v1(false, None, false, false),
        6,
        &memory_augmented_descriptor(None),
        None,
    )
    .await;
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id, execution_policy,
            execution_descriptor_json, current_session_id)
           VALUES ('SCHSETTLECRASH', 'test_creator', 'memory-augmented', 1, 'running',
                   'serial', 0, 'p3-final-checkpoint', ?, ?, NULL, 'driven_v1', ?, ?)",
    )
    .bind(now)
    .bind(now)
    .bind(memory_augmented_descriptor(None))
    .bind(sid)
    .execute(&daemon.pool)
    .await
    .expect("seed pre-settlement schedule row");

    daemon.restart().await;

    // The production bundle publication reconciles the schedule from the
    // durable terminal session BEFORE recovery runs; the terminal session is
    // never re-driven.
    let (status, current_sid): (String, Option<String>) = sqlx::query_as(
        "SELECT status, current_session_id FROM creator_schedules \
         WHERE schedule_id = 'SCHSETTLECRASH'",
    )
    .fetch_one(&daemon.pool)
    .await
    .expect("settled schedule row");
    assert_eq!(
        status, "completed",
        "schedule settles from the durable terminal"
    );
    assert_eq!(
        current_sid.as_deref(),
        Some(sid),
        "schedule keeps its owned run identity"
    );
    let (run_status, _) = durable_run_state(&daemon.pool, sid).await;
    assert_eq!(
        run_status, "completed",
        "the terminal session stays terminal"
    );
    assert_eq!(
        host.execs.load(Ordering::SeqCst),
        0,
        "a settled terminal run is never re-driven after restart"
    );
}

/// A7 rule 6 (P3 T1 rereview P1): a durable v1 run parked at a fully
/// committed safe boundary (no wait / in-flight / step mark / cancel) is
/// reconstructed from its frozen source identity and re-driven through the
/// production recovery seam — never stranded by the explicit
/// `SkippedSafeBoundary` skip that previously dropped it.
#[tokio::test]
async fn daemon_restart_safe_boundary_is_reconstructed_and_redriven() {
    let host = RestartMockHost::new();
    let dispatch = Arc::new(CountingDispatch {
        calls: Arc::new(AtomicUsize::new(0)),
    });
    let daemon =
        LiveDaemon::start_with_host_provider_and_dispatch(host.clone(), dispatch.clone()).await;

    let sid = "daemon:safe-boundary";
    // Running at a committed boundary: plain context (no join keys), no
    // durable wait, no in-flight/step/cancel evidence → SafeBoundary.
    seed_daemon_session(
        &daemon.pool,
        sid,
        "running",
        Some("generate"),
        &plain_context(),
        &run_state_v1(false, None, false, false),
        4,
        &memory_augmented_descriptor_bound(),
        None,
    )
    .await;

    // The exact production recovery seam boot/restart runs
    // (`run_boot_recovery` → `recover_persisted`): reconstruction from the
    // frozen source, classification, then the single-owner re-drive.
    let coordinator = daemon.state.run_coordinator().expect("coordinator wired");
    let sqlite = Arc::new(SqliteSessionStorage::new(Arc::new(daemon.pool.clone())));
    let decisions = coordinator.recover_persisted(&sqlite, None).await;

    let redriven = decisions.iter().any(|d| match d {
        ResumeDecision::ReDriven { session_id, .. } => session_id.0 == sid,
        _ => false,
    });
    assert!(
        redriven,
        "a safe-boundary v1 run must be reconstructed and re-driven (A7 rule 6), \
         got decisions {decisions:?}"
    );
}
