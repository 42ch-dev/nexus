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
//!
//! V1.182 P1 BL-04 (Task 3): the interrupted checkpoint is ALSO exercised
//! through the real `nexus42 ops inspect` binary surface (daemon-free,
//! read-only) — human + `--json` assertions per the inspect contract — and
//! the no-mutation obligation is proven: inspect-then-resume behaves
//! identically to resume-without-inspect (same completed set, no stage
//! re-executed, store rows + db file bytes untouched by the inspect runs).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
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
use serde_json::{json, Value};
use std::path::Path;
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

/// Snapshot the persistent checkpoint rows relevant to the no-mutation
/// proof: position + raw context + timestamps per session.
async fn checkpoint_rows(pool: &sqlx::SqlitePool) -> Vec<(String, Option<String>, Vec<u8>, i64)> {
    let rows = sqlx::query_as::<_, (String, Option<String>, Vec<u8>, i64)>(
        "SELECT session_id, current_task_id, context_json, updated_at
         FROM orchestration_sessions ORDER BY session_id",
    )
    .fetch_all(pool)
    .await
    .expect("snapshot checkpoint rows");
    rows
}

/// The REAL `nexus42 ops inspect <id>` binary surface with `HOME` pointed at
/// the hermetic tmp root (whose seeded `config.toml` resolves the SAME
/// `state.db` the engine persists to).
fn nexus42(home: &Path) -> Command {
    let mut cmd = Command::cargo_bin("nexus42").expect("nexus42 binary");
    cmd.env("HOME", home);
    cmd
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

/// Interrupt → checkpoint → inspect → boot resume, real binary surface.
///
/// V1.182 P1 BL-04 Task 3 evidence chain:
/// 1. Drive the converge chain to the parked join (instrumented edges fire
///    exactly once each), then drop the in-memory engine — the sqlite
///    checkpoint persists while the pool stays open (WAL).
/// 2. Invoke `nexus42 ops inspect <sid>` (human) and
///    `nexus42 ops inspect <sid> --json` — the REAL binary over the SAME
///    db file — and assert both against the inspect contract: run id, the
///    persisted row fields, live join keys, resumable yes /
///    `chain_class_no_failure` / `runner_check` `boot_time` caveat (rule 4
///    never folded into the verdict), position = parked join, timestamps
///    present.
///    Also list mode: the checkpoint appears as a candidate with verdict
///    `yes`.
/// 3. No-mutation proof: checkpoint rows + raw db file bytes captured
///    BEFORE the first inspect pass and AFTER the last (all six
///    invocations inside the window) are identical.
/// 4. Park past the join deadline — the same explicit DOWNTIME sleep as
///    the baseline restart test (the inspect passes run inside it).
/// 5. Resume re-drive completes 3 steps, the completed instrumented edges
///    did NOT re-fire (count still 2), and the elapsed check still
///    includes the downtime — byte-identical to resume-without-inspect.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn inspect_after_interrupt_is_side_effect_free_and_resume_matches_baseline() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let user_home = tmp.path();
    let state = WorkspaceState::new_for_testing(nexus_home, db_path.clone(), None).await;
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

    // Kill the in-memory engine (the open pool Arc keeps the WAL db writable
    // — exactly the interrupted-run state a boot would observe on restart).
    drop(engine1);

    // Downtime: park past the join deadline — the same explicit DOWNTIME
    // sleep as the baseline restart test, with the inspect passes running
    // inside the window.
    tokio::time::sleep(DOWNTIME).await;

    // ---- Inspect (real binary surface) over the interrupted checkpoint ----

    // Side-effect-free proof - baseline captured BEFORE the first inspect
    // pass: checkpoint rows + raw db file bytes, while the daemon-side pool
    // is the only open handle. Every inspect pass below must leave both
    // identical (contract §7 no-mutation obligation) - a mutation from
    // any pass would flip the post-window comparison.
    let rows_before = checkpoint_rows(&pool).await;
    let bytes_before = std::fs::read(&db_path).expect("db bytes before inspect");

    // Human detail view.
    let human = nexus42(user_home)
        .args(["ops", "inspect", &sid.0])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let human = String::from_utf8(human).unwrap();
    assert!(human.contains(&format!("session:        {}", sid.0)), "{human}");
    assert!(human.contains("creator:        test_creator"), "{human}");
    // The checkpoint stores POSITION ONLY (contract §5): preset metadata is
    // inferred by `SqliteSessionStorage::save` from context keys, and the
    // engine start path seeds only `_session_id`/`_creator_id` — so the
    // persisted row carries save()'s documented defaults ("default"/0). The
    // CLI reports the DB verbatim and never fabricates: pin that honest
    // negative here.
    assert!(
        human.contains("preset:         default"),
        "checkpoint must carry the persisted preset id: {human}"
    );
    assert!(human.contains("preset_version: 0"), "{human}");
    assert!(
        human.contains("status:         running"),
        "raw DB status column (not authoritative): {human}"
    );
    assert!(human.contains("position:       join"), "{human}");
    // Contract §2: human detail renders both persisted timestamps as
    // `YYYY-MM-DD HH:MM:SS UTC` - presence/format-light assertion
    // complementing the JSON numeric checks below.
    for key in ["created_at", "updated_at"] {
        let line = human
            .lines()
            .find(|l| l.starts_with(key))
            .unwrap_or_else(|| panic!("human detail must render {key}: {human}"));
        let date = line.split_whitespace().nth(1).unwrap_or("");
        assert!(
            date.len() == 10 && date.split('-').count() == 3,
            "{key} timestamp must render as YYYY-MM-DD: {line}"
        );
    }
    assert!(
        human.contains("join state:     2 live join key(s):"),
        "parked join must expose both live join keys: {human}"
    );
    assert!(
        human.contains("_converge_arrivals_join")
            && human.contains("_join_wait_start_join"),
        "both live join keys listed: {human}"
    );
    assert!(
        human.contains("resumable:      yes — candidate for re-drive on next boot"),
        "verdict yes with the boot-time runner caveat: {human}"
    );
    assert!(
        human.contains("run record:     (no typed run record)"),
        "no failure record: {human}"
    );

    // JSON detail view — field-by-field per the inspect contract.
    let json_out = nexus42(user_home)
        .args(["ops", "inspect", &sid.0, "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let obj: Value = serde_json::from_slice(&json_out).expect("valid inspect json");
    assert_eq!(obj["session_id"], json!(sid.0));
    assert_eq!(obj["creator_id"], json!("test_creator"));
    assert_eq!(
        obj["preset_id"], json!("default"),
        "persisted checkpoint truth, not the in-memory preset id"
    );
    assert_eq!(obj["preset_version"], json!(0));
    assert_eq!(obj["db_status"], json!("running"));
    assert_eq!(obj["current_task_id"], json!("join"));
    assert_eq!(obj["run_failure"], Value::Null);
    assert_eq!(
        obj["live_join_keys"],
        json!(["_converge_arrivals_join", "_join_wait_start_join"]),
        "live join keys must match the persisted checkpoint"
    );
    assert_eq!(obj["resumable"]["verdict"], json!("yes"));
    assert_eq!(obj["resumable"]["rule"], json!("chain_class_no_failure"));
    assert_eq!(obj["resumable"]["runner_check"], json!("boot_time"));
    let explanation = obj["resumable"]["explanation"].as_str().unwrap();
    assert!(
        explanation.contains("boot"),
        "verdict:yes explanation must state the boot-time runner caveat: {explanation}"
    );
    assert!(
        obj.get("context_readable").is_none(),
        "context_readable must be absent on readable rows"
    );
    let created_at = obj["created_at"].as_i64().expect("created_at unix secs");
    let updated_at = obj["updated_at"].as_i64().expect("updated_at unix secs");
    assert!(
        created_at >= 1_600_000_000 && updated_at >= created_at,
        "timestamps must be real persisted unix seconds: created {created_at}, updated {updated_at}"
    );

    // List mode: the interrupted run is a non-terminal candidate with the
    // same verdict; count line present.
    let list_out = nexus42(user_home)
        .args(["ops", "inspect"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list = String::from_utf8(list_out).unwrap();
    assert!(list.contains(&sid.0), "{list}");
    // Column semantics (contract §2 list layout):
    // `<session_id>  <preset_id>  <status>  <task>  <updated_at>  <verdict>`.
    // The YAML preset id `e2e-resume-reroute` appears only as the
    // session_id PREFIX (`{preset_id}:{millis}`, engine.rs:715) - the
    // preset COLUMN of this checkpoint is `default` (persisted DB truth).
    // Pin that honest negative on the actual column, not a sid-prefix grep.
    assert!(
        list.lines().any(|l| {
            let cols: Vec<&str> = l.split_whitespace().collect();
            cols.len() >= 6 && cols[0] == sid.0.as_str() && cols[1] == "default"
        }),
        "list row must show the persisted preset column `default`: {list}"
    );
    assert!(list.contains("yes"), "list verdict word: {list}");
    assert!(list.contains("checkpointed session(s)."), "{list}");

    // ---- Side-effect-free proof: post-window snapshot ----
    // (Baseline was captured before the first inspect pass above; this
    // second human/json/list triple keeps all six invocations inside the
    // proof window.)
    nexus42(user_home)
        .args(["ops", "inspect", &sid.0])
        .assert()
        .success();
    nexus42(user_home)
        .args(["ops", "inspect", &sid.0, "--json"])
        .assert()
        .success();
    nexus42(user_home).args(["ops", "inspect"]).assert().success();
    let rows_after = checkpoint_rows(&pool).await;
    let bytes_after = std::fs::read(&db_path).expect("db bytes after inspect");
    assert_eq!(rows_before, rows_after, "inspect must not mutate rows");
    assert_eq!(
        bytes_before, bytes_after,
        "inspect must not write the db file"
    );

    // ---- Phase 2: boot resume AFTER the inspect passes ----
    let (engine2, storage2) = build_engine(&pool, dispatch.clone());
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

    // Completed edges did NOT re-fire: identical to the baseline (count
    // still 2) — inspect did not create a second completed set.
    assert_eq!(
        dispatch.calls.load(Ordering::SeqCst),
        2,
        "inspect-then-resume must not re-execute completed edges"
    );

    let ctx = engine2.get_context(&sid).await.expect("context");
    let note = ctx
        .get::<String>("_join_timeout_note")
        .await
        .expect("reroute must write _join_timeout_note");
    let elapsed = elapsed_ms_from_note(&note);
    assert!(
        elapsed >= DOWNTIME_MS,
        "elapsed must include the downtime (no re-baseline): {note}"
    );
    assert!(
        elapsed >= JOIN_TIMEOUT_MS,
        "the deadline must have fired: {note}"
    );
    assert_eq!(
        engine2.get_status(&sid).await.expect("status"),
        SessionStatus::Completed
    );

    drop(state);
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
