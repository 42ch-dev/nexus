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

use assert_cmd::Command;
use async_trait::async_trait;
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
    OrchestrationEngine,
};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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
    let pool = nexus_local_db::open_pool(db_path).await.expect("reopen pool");
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
        let pool = nexus_local_db::open_pool(&db_path).await.expect("open pool");
        for (id, status) in [
            ("done:completed", "completed"),
            ("done:failed", "failed"),
            ("done:cancelled", "cancelled"),
        ] {
            let state = run_state_v1(false, Some("task_3"), true, true);
            seed_v1_row(
                &pool, id, status, Some("task_9"), &plain_context(), &state, 5,
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
    let rows: Vec<String> = sqlx::query_scalar("SELECT status FROM orchestration_sessions ORDER BY session_id")
        .fetch_all(&pool_b)
        .await
        .expect("status rows");
    assert_eq!(
        rows,
        vec!["cancelled".to_string(), "completed".to_string(), "failed".to_string()],
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
    // join keys, never silently advanced to a join re-drive).
    {
        let pool = nexus_local_db::open_pool(&db_path).await.expect("open pool");
        seed_v1_row(
            &pool, "wait:plain", "waiting_for_input", Some("task_7"),
            &plain_context(), &wait_state, 6,
        )
        .await;
        seed_v1_row(
            &pool, "wait:old-joins", "waiting_for_input", Some("task_7"),
            &chain_context(), &wait_state, 7,
        )
        .await;
        pool.close().await;
    }

    // "Daemon restart": fresh pool reopens the file.
    let (pool_b, dyn_storage, store) = reopen(&db_path).await;

    // The durable wait token is byte-preserved after reopen for BOTH rows.
    for id in ["wait:plain", "wait:old-joins"] {
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

    // Boot resume must skip both (never stepped, never auto-approved).
    let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
        dyn_storage.clone(),
        store.clone(),
        CapabilityRegistryHolder::with_registry(Arc::new(CapabilityRegistry::with_builtins())),
    );
    let summaries: Vec<SessionSummary> = ["wait:plain", "wait:old-joins"]
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
        ],
        "token-bearing waits (even with old join keys) are never stepped at boot"
    );

    // The token is STILL the same after the resumed boot (no implicit
    // approval, no regeneration, no rewrite).
    for id in ["wait:plain", "wait:old-joins"] {
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

    // Scoped public evidence: inspect agrees on the reopened file.
    for (id, waits) in [("wait:plain", true), ("wait:old-joins", true)] {
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
        assert_eq!(
            waits,
            true,
            "{id} — human wait must never be silently advanced to a join re-drive"
        );
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
    assert!(human.contains("resumable:      no — human wait (A4) token preserved"), "{human}");

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
        let pool = nexus_local_db::open_pool(&db_path).await.expect("open pool");
        seed_v1_row(
            &pool, "int:explicit", "interrupted", Some("task_3"),
            &plain_context(), &run_state_v1(false, Some("task_3"), false, false), 4,
        )
        .await;
        seed_v1_row(
            &pool, "int:crash", "running", Some("task_3"),
            &chain_context(), &run_state_v1(false, Some("task_3"), true, true), 5,
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
        assert_eq!(record.state_revision, if id == "int:explicit" { 4 } else { 5 }, "{id}");
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
    // 5), running, live join keys, one arrival of two. The instrumented
    // start/branch_a edges "already fired" in the prior process (fixture).
    {
        let pool = nexus_local_db::open_pool(&db_path).await.expect("open pool");
        let context = serde_json::json!({"data": {
            "_converge_arrivals_join": ["branch_a"],
            "_join_wait_start_join": chrono::Utc::now().timestamp_millis()
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
    let wired = build_wired_outer_graph(&loaded, &engine_ref, &caps, Some(dispatch.clone()));
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
        engine_ref.get_status(&SessionId(sid.to_string())).await.expect("status"),
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
    let wired_c = build_wired_outer_graph(&loaded_c, &engine_c_ref, &caps_c, Some(dispatch.clone()));
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
    let live_join_keys =
        nexus_orchestration::resume_rules::live_join_keys(
            nexus_orchestration::resume_rules::context_data(&context_value).expect("data map"),
        );
    assert_eq!(
        live_join_keys,
        vec!["_converge_arrivals_join".to_string(), "_join_wait_start_join".to_string()],
        "the join keys must survive the second restart"
    );

    pool_c.close().await;
    pool_b.close().await;
    drop(tmp);
}

/// CONTRACT test (was the BLOCKED pin — now GREEN with the writer-side fix):
/// a converge-join park state that the ENGINE itself writes (`paused` + live
/// join keys + NO human-wait token — the corrected `run_step_internal` /
/// `build_step_state` shape per A2, cross-checked by the reopen flow in
/// [`restart_durable_status_converge_merge_redrives_after_reopen`]) must be
/// re-checked on every later boot so the bounded join deadline can fire
/// ("converge/merge resume still works"). Because the engine no longer
/// mints a Manual wait token for scheduler parks, `classify_recovery`
/// rule 4 cannot misclassify the park as a human wait — rule 5
/// (`ConvergeMerge`) governs and the daemon re-drives the join.
#[tokio::test]
async fn restart_durable_status_converge_join_token_row_still_redrives() {
    let (tmp, _nexus_home, db_path) = test_utils::create_test_workspace().await;
    let user_home = tmp.path();
    let sid = "conv:engine-parked";

    // "Process A" (real engine parking — the FIXED durable shape): `paused`
    // at the converge join, live join keys (1/2 arrivals, deadline not yet
    // elapsed), and NO human-wait token. This is exactly what the
    // store-wired engine now persists on every park (A2).
    {
        let pool = nexus_local_db::open_pool(&db_path).await.expect("open pool");
        let context = serde_json::json!({"data": {
            "_converge_arrivals_join": ["branch_a"],
            "_join_wait_start_join": chrono::Utc::now().timestamp_millis()
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
    let wired = build_wired_outer_graph(&loaded, &engine_ref, &caps, None);
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
    assert_eq!(parsed["live_join_keys"], json!(["_converge_arrivals_join", "_join_wait_start_join"]));
    assert!(
        parsed.get("wait_id").is_none(),
        "an engine-parked join must never advertise a wait token: {parsed}"
    );

    pool_b.close().await;
    drop(tmp);
}
