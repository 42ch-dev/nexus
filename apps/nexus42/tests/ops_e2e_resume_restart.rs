//! BL-04 checkpoint minimal slice — restart-resume E2E (V1.180 P2 T2).
//!
//! A real `orchestration_sessions` SQLite persistence + a real
//! `GraphFlowEngine` (production-shaped, mirroring `boot.rs` wiring) drive a
//! converge/merge chain preset whose `start`/`branch_a` edges carry
//! instrumented `host_tool` enter actions. The test:
//!
//! 1. Drives the chain to the parked join (3 steps; the instrumented edges
//!    fire exactly once each).
//! 2. Kills the in-memory engine (the sqlite file persists) and parks past
//!    the join deadline — simulated downtime.
//! 3. Restarts: a fresh engine over the SAME pool, runner reconstructed for
//!    the existing session (mirrors `recover_sessions` →
//!    `reconstruct_runner` for a non-embedded preset), then
//!    [`resume_driven_sessions`] re-drives from the persisted position.
//! 4. Asserts: the run completes (join → fallback → done = 3 steps), the
//!    instrumented edges did NOT re-fire (count still 2), and
//!    `_join_wait_start_*` elapsed INCLUDES the downtime (the persisted
//!    wall-clock wait-start is compared against the wall clock on the first
//!    re-step — no re-baseline).
//!
//! A second test pins the boot filters: a typed-failed session
//! (`_run_error` in context — the DB `status` column stays `running` after
//! a typed failure because `SqliteSessionStorage::save` ON CONFLICT never
//! updates it) is NOT re-driven, and a session without join-tracking keys
//! (not of the converge/merge chain class) is NOT re-driven — byte-identical
//! to pre-T2 boot (tracked-but-not-driven).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use async_trait::async_trait;
use nexus_daemon_runtime::preset_run::{
    drive_preset_run, resume_driven_sessions, PresetRunConfig, PresetRunOutcome, ResumeDecision,
};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::capability::DaemonToolDispatch;
use nexus_orchestration::engine::{SessionId, SessionStatus, SessionSummary};
use nexus_orchestration::preset::load_preset_from_str;
use nexus_orchestration::preset::loader::build_wired_outer_graph;
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use nexus_orchestration::{
    CapabilityError, CapabilityRegistry, CapabilityRegistryHolder, GraphFlowEngine,
    OrchestrationEngine,
};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Counting daemon-tool dispatch: every `dispatch_tool` call increments a
/// shared counter — the "completed edges must not re-fire" instrumentation.
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
        Ok(serde_json::json!({ "ok": true }))
    }
}

/// Converge chain with instrumented host-tool edges on `start` and
/// `branch_a`; `branch_b` is the hanging upstream (never walked, never
/// arrives); the join reroutes to `fallback` when its deadline fires.
const RESUME_REROUTE_YAML: &str = r#"
preset:
  id: e2e-resume-reroute
  version: 1
  kind: creator
  description: "BL-04 resume E2E — converge join with instrumented host-tool edges"
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
      timeout_ms: 120
      on_timeout: fallback
      next: done
    - id: fallback
      next: done
    - id: done
      terminal: true
"#;

/// Join deadline (ms) + how long the test parks past it (simulated
/// downtime between kill and restart).
const JOIN_TIMEOUT_MS: u64 = 120;
const DOWNTIME_MS: u64 = 500;
const DOWNTIME: Duration = Duration::from_millis(DOWNTIME_MS);

/// Build a production-shaped engine over the given pool (mirrors `boot.rs`:
/// `SqliteSessionStorage` + `GraphFlowEngine` + daemon tool dispatch).
fn build_engine(
    pool: &sqlx::SqlitePool,
    dispatch: Arc<dyn DaemonToolDispatch>,
) -> (Arc<GraphFlowEngine>, Arc<dyn graph_flow::SessionStorage>) {
    let storage: Arc<dyn graph_flow::SessionStorage> =
        Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
    let holder =
        CapabilityRegistryHolder::with_registry(Arc::new(CapabilityRegistry::with_builtins()));
    let mut engine = GraphFlowEngine::new_with_storage(storage.clone(), holder);
    engine.set_daemon_tool_dispatch(dispatch);
    (Arc::new(engine), storage)
}

/// Extract the `elapsed_ms=` payload from a `_join_timeout_note`.
fn elapsed_ms_from_note(note: &str) -> u64 {
    note.split("elapsed_ms=")
        .nth(1)
        .and_then(|s| s.split(')').next())
        .and_then(|s| {
            s.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .ok()
        })
        .unwrap_or_else(|| panic!("note must carry a numeric elapsed_ms: {note}"))
}

/// Kill/restart mid-chain: the resume re-drive skips completed edges.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn restart_mid_chain_resumes_without_re_executing_completed_edges() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().expect("pool").clone();
    test_utils::seed_test_creator_and_world(&pool).await;

    let dispatch = Arc::new(CountingDispatch {
        calls: Arc::new(AtomicUsize::new(0)),
    });
    let (engine1, storage) = build_engine(&pool, dispatch.clone());

    // Phase 1: start the converge chain and drive to the parked join.
    let caps = Arc::new(CapabilityRegistry::with_builtins());
    let loaded = load_preset_from_str(RESUME_REROUTE_YAML, &caps).expect("test preset loads");
    let sid = engine1
        .start_session_with_preset_for_creator(&loaded, "test_creator")
        .await
        .expect("start preset session on the daemon engine");

    let first = drive_preset_run(
        engine1.as_ref(),
        Some(&storage),
        &sid,
        &PresetRunConfig::default(),
        None,
    )
    .await;
    assert_eq!(
        first,
        PresetRunOutcome::WaitingForInput { steps: 3 },
        "start -> branch_a -> join (parks at 1/2 arrivals)"
    );
    assert_eq!(
        dispatch.calls.load(Ordering::SeqCst),
        2,
        "start + branch_a host-tool edges fired exactly once before the kill"
    );

    // Kill: drop the in-memory engine + workspace state (the sqlite file
    // persists — the pool Arc survives through the test's own clone).
    drop(engine1);
    drop(state);

    // Downtime: park past the join deadline.
    tokio::time::sleep(DOWNTIME).await;

    // Phase 2: restart — a fresh engine over the SAME pool/storage.
    let (engine2, storage2) = build_engine(&pool, dispatch.clone());

    // Reconstruct the runner for the existing session (mirrors
    // `recover_sessions` → `reconstruct_runner`; the test preset is not
    // embedded, so the test wires the graph itself).
    let engine_ref: Arc<dyn OrchestrationEngine> = engine2.clone();
    let wired = build_wired_outer_graph(&loaded, &engine_ref, &caps, Some(dispatch.clone()));
    let runner = Arc::new(graph_flow::FlowRunner::new(
        Arc::new(wired),
        storage2.clone(),
    ));
    engine2
        .shared_state()
        .runners
        .write()
        .await
        .insert(sid.0.clone(), runner);
    let summary = SessionSummary {
        session_id: sid.clone(),
        creator_id: "test_creator".to_string(),
        preset_id: loaded.id.clone(),
        status: SessionStatus::WaitingForInput,
        current_task_id: Some("join".to_string()),
    };
    engine2
        .shared_state()
        .sessions
        .write()
        .await
        .push(summary.clone());

    // Resume re-drive from the persisted position (resume_waiting: true —
    // the parked join re-checks its deadline on the first re-step).
    let config = PresetRunConfig {
        resume_waiting: true,
        ..PresetRunConfig::default()
    };
    let decisions =
        resume_driven_sessions(engine2.as_ref(), &storage2, &[summary], &config, None).await;
    assert_eq!(decisions.len(), 1, "exactly one recovered session");
    match &decisions[0] {
        ResumeDecision::ReDriven {
            session_id,
            outcome,
        } => {
            assert_eq!(session_id, &sid);
            assert_eq!(
                outcome,
                &PresetRunOutcome::Completed { steps: 3 },
                "resume re-drives join -> fallback -> done = 3 steps from the persisted position"
            );
        }
        other => panic!("expected ReDriven, got {other:?}"),
    }

    // Completed edges did NOT re-fire: the instrumented host-tool count is
    // still 2 (start + branch_a), not 4.
    assert_eq!(
        dispatch.calls.load(Ordering::SeqCst),
        2,
        "completed edges must not re-execute across kill/restart"
    );

    // `_join_wait_start_*` semantics pinned: elapsed INCLUDES downtime
    // (the persisted wall-clock wait-start is compared against the wall
    // clock on the first re-step — no re-baseline).
    let ctx = engine2.get_context(&sid).await.expect("context");
    let note = ctx
        .get::<String>("_join_timeout_note")
        .await
        .expect("reroute must write _join_timeout_note");
    let elapsed = elapsed_ms_from_note(&note);
    // The wait-start was persisted BEFORE the kill; the re-step compares it
    // against the wall clock AFTER the downtime. Elapsed therefore includes
    // the full downtime (>= DOWNTIME) — a re-baseline at resume would have
    // produced ~0ms and the 120ms deadline would NOT have fired on the
    // first re-step (the join would have parked again).
    assert!(
        elapsed >= DOWNTIME_MS,
        "elapsed must include the downtime (no re-baseline): {note}"
    );
    assert!(
        elapsed >= JOIN_TIMEOUT_MS,
        "the deadline must have fired: {note}"
    );

    // Terminal: the daemon engine reports Completed.
    assert_eq!(
        engine2.get_status(&sid).await.expect("status"),
        SessionStatus::Completed
    );

    drop(tmp);
}

/// Boot filters pinned: a typed-failed session and a non-class session are
/// NOT re-driven (no-checkpoint default equivalence + T1 review constraint).
#[tokio::test]
async fn resume_skips_typed_failed_and_non_class_sessions() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().expect("pool").clone();

    let dispatch = Arc::new(CountingDispatch {
        calls: Arc::new(AtomicUsize::new(0)),
    });
    let (engine, storage) = build_engine(&pool, dispatch.clone());

    // Session A: typed-failed join — the context carries the failure record
    // (`_run_status`/`_run_error`); the DB `status` column would read
    // `running` (save ON CONFLICT never updates it). Must NOT be re-driven.
    let typed_failed = graph_flow::Session::new_from_task("test:typed-failed".to_string(), "join");
    typed_failed
        .context
        .set("_converge_arrivals_join", serde_json::json!(["branch_a"]))
        .await;
    typed_failed
        .context
        .set("_join_wait_start_join", serde_json::json!(1000u64))
        .await;
    typed_failed
        .context
        .set("_run_status", "failed".to_string())
        .await;
    typed_failed
        .context
        .set("_run_error", "converge_timeout: gate=converge".to_string())
        .await;
    storage.save(typed_failed).await.unwrap();

    // Session B: no join-tracking keys — not of the converge/merge chain
    // class. Must behave byte-identically to pre-T2 boot (tracked but not
    // driven).
    let linear = graph_flow::Session::new_from_task("test:linear".to_string(), "mid");
    storage.save(linear).await.unwrap();

    let summaries = vec![
        SessionSummary {
            session_id: SessionId("test:typed-failed".to_string()),
            creator_id: "test_creator".to_string(),
            preset_id: "e2e".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("join".to_string()),
        },
        SessionSummary {
            session_id: SessionId("test:linear".to_string()),
            creator_id: "test_creator".to_string(),
            preset_id: "e2e".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("mid".to_string()),
        },
    ];

    let decisions = resume_driven_sessions(
        engine.as_ref(),
        &storage,
        &summaries,
        &PresetRunConfig::default(),
        None,
    )
    .await;
    assert_eq!(decisions.len(), 2);
    assert!(
        matches!(
            &decisions[0],
            ResumeDecision::SkippedTypedFailed { session_id }
                if session_id.0 == "test:typed-failed"
        ),
        "typed-failed session must not be re-driven: {:?}",
        decisions[0]
    );
    assert!(
        matches!(
            &decisions[1],
            ResumeDecision::SkippedNotConvergeMergeClass { session_id }
                if session_id.0 == "test:linear"
        ),
        "non-class session must not be re-driven: {:?}",
        decisions[1]
    );
    assert_eq!(
        dispatch.calls.load(Ordering::SeqCst),
        0,
        "no tool may fire for skipped sessions"
    );

    drop(state);
    drop(tmp);
}
