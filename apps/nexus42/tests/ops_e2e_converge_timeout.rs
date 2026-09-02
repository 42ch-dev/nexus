//! DR-06 ops E2E — real daemon session + daemon-local preset-run driver
//! (V1.180 P2 T1). Closes the V1.179 QA carried note qc3 S-7 (automated
//! load→run timeout e2e "both gates × reroute/error via daemon").
//!
//! Exercises the converge-gate bounded-join surface in a REAL daemon
//! process: a live HTTP router over the daemon `WorkspaceState`, a real
//! `orchestration_sessions` SQLite persistence, and the daemon engine the
//! routes serve. The preset-run driver
//! ([`nexus_daemon_runtime::preset_run`]) steps the session; the failure
//! record asserts the typed `converge_timeout:` discriminator, the
//! `on_timeout` reroute (`_join_timeout_note` + reroute target), and the
//! absent-`on_timeout` typed-failure (never `WaitForInput` forever).
//!
//! ## Wiring choices (declared, per the brief)
//!
//! The test presets are NOT embedded (no test artifact shipped in the
//! production binary), so the session is started on the daemon engine
//! same-process via `start_session_with_preset_for_creator` instead of
//! `POST /v1/daemon/orchestration/sessions` (which loads embedded presets
//! only). The session lives in the SAME engine the HTTP router serves, so
//! `GET /v1/daemon/orchestration/sessions/:id` observes it. The driver is
//! likewise called same-process (the HTTP surface has no step route and the
//! brief allows same-process driving).

mod common;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use common::LiveDaemon;
use nexus_daemon_runtime::preset_run::{drive_preset_run, PresetRunConfig, PresetRunOutcome};
use nexus_orchestration::engine::{SessionId, SessionStatus};
use nexus_orchestration::CapabilityRegistry;

// ---------------------------------------------------------------------------
// Test presets (converge-gate shapes, no capabilities — hermetic, no network)
// ---------------------------------------------------------------------------

/// Happy path: a single-predecessor converge joins immediately when its one
/// upstream arrives; the bounded-join fields stay inert on the passing path
/// (F-001 success-leave clear of `_join_wait_start_*`).
const HAPPY_PATH_YAML: &str = r#"
preset:
  id: e2e-converge-happy
  version: 1
  kind: creator
  description: "DR-06 ops E2E — converge join happy path through the preset-run driver"
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
    - id: join
      converge: { strategy: wait_for_all }
      timeout_ms: 5000
      next: done
    - id: done
      terminal: true
"#;

/// Timeout + reroute: the converge join waits for TWO predecessors, but the
/// walk only ever delivers `branch_a` — `branch_b` is the hanging upstream
/// edge (never walked, never arrives). When the deadline fires the join
/// reroutes to `fallback` and the run completes.
const TIMEOUT_REROUTE_YAML: &str = r#"
preset:
  id: e2e-converge-timeout-reroute
  version: 1
  kind: creator
  description: "DR-06 ops E2E — hanging upstream + timeout_ms reroutes via on_timeout"
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
      description: "Hanging upstream edge — never walked in this run, never arrives"
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

/// Timeout + typed failure: identical to the reroute shape but WITHOUT
/// `on_timeout` — the deadline must fail the run with the typed
/// `converge_timeout:` error instead of waiting forever.
const TIMEOUT_ERROR_YAML: &str = r#"
preset:
  id: e2e-converge-timeout-error
  version: 1
  kind: creator
  description: "DR-06 ops E2E — absent on_timeout fails typed at the deadline"
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
      description: "Hanging upstream edge — never walked in this run, never arrives"
      next: join
    - id: join
      converge: { strategy: wait_for_all }
      timeout_ms: 120
      next: done
    - id: done
      terminal: true
"#;

/// Join deadline (ms) + how long the test parks past it before re-driving.
const JOIN_TIMEOUT_MS: u64 = 120;
const PAST_DEADLINE_SLEEP: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Start a session on the DAEMON engine from a YAML preset string.
async fn start_preset_session(d: &LiveDaemon, yaml: &str) -> SessionId {
    let caps = Arc::new(CapabilityRegistry::with_builtins());
    let loaded = nexus_orchestration::preset::load_preset_from_str(yaml, &caps)
        .unwrap_or_else(|e| panic!("test preset must load: {e}"));
    d.engine
        .start_session_with_preset_for_creator(&loaded, "test_creator")
        .await
        .expect("start preset session on the daemon engine")
}

/// Drive the session through the daemon-local preset-run driver.
async fn drive(d: &LiveDaemon, sid: &SessionId, resume_waiting: bool) -> PresetRunOutcome {
    let config = PresetRunConfig {
        resume_waiting,
        ..PresetRunConfig::default()
    };
    drive_preset_run(
        d.engine.as_ref(),
        Some(&d.session_storage),
        sid,
        &config,
        None,
    )
    .await
}

/// `GET /v1/daemon/orchestration/sessions/{id}` → (HTTP status, status
/// string). Terminal sessions drop out of the engine's ACTIVE list, so
/// after completion/failure the route answers 404 — that active-monitor
/// semantic is asserted, not changed, by this task.
async fn http_session_status(d: &LiveDaemon, sid: &SessionId) -> (u16, String) {
    let url = format!("{}/v1/daemon/orchestration/sessions/{}", d.http_url, sid.0);
    let resp = reqwest::get(&url).await.expect("GET daemon session");
    let http_status = resp.status().as_u16();
    let body = resp.text().await.expect("read GET response");
    let status = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v["session"]["status"].as_str().map(str::to_string))
        .unwrap_or_default();
    (http_status, status)
}

/// Extract the `elapsed_ms=` payload from a `converge_timeout:` record.
fn elapsed_ms_from_error(error: &str) -> u64 {
    error
        .split("elapsed_ms=")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .and_then(|s| {
            s.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .ok()
        })
        .unwrap_or_else(|| panic!("error must carry a numeric elapsed_ms: {error}"))
}

// ---------------------------------------------------------------------------
// Step 1: driver + happy-path skeleton
// ---------------------------------------------------------------------------

/// Real daemon session: the driver steps a converging preset run to
/// completion; the join passes when its (single) arrival lands, the
/// bounded-join fields stay inert, and the F-001 success-leave clear
/// removes the wait-start key.
#[tokio::test]
async fn driver_steps_happy_path_preset_run_to_completion() {
    let d = LiveDaemon::start().await;
    let sid = start_preset_session(&d, HAPPY_PATH_YAML).await;

    // Daemon-observable: the session is listed by the live HTTP surface.
    let (http_status, status) = http_session_status(&d, &sid).await;
    assert_eq!(http_status, 200, "session must be daemon-visible at start");
    assert_eq!(status, "running");

    let outcome = drive(&d, &sid, false).await;
    assert_eq!(
        outcome,
        PresetRunOutcome::Completed { steps: 4 },
        "happy path: start -> branch_a -> join -> done = 4 steps"
    );

    // The daemon engine reports the session terminal.
    assert_eq!(
        d.engine.get_status(&sid).await.expect("status"),
        SessionStatus::Completed
    );

    // Terminal sessions leave the ACTIVE monitor (engine `list_active`
    // semantics) — the route answers 404 without a wire change.
    let (http_status, _) = http_session_status(&d, &sid).await;
    assert_eq!(http_status, 404, "terminal session leaves the active list");

    // F-001: the passing join cleared its tracking keys.
    let ctx = d.engine.get_context(&sid).await.expect("context");
    assert!(
        ctx.get::<HashSet<String>>("_converge_arrivals_join")
            .await
            .is_none(),
        "join leave clears the converge arrivals key"
    );
    assert!(
        ctx.get::<u64>("_join_wait_start_join").await.is_none(),
        "join leave clears the wait-start key (F-001 success-leave clear)"
    );
    assert!(
        ctx.get::<String>("_join_timeout_note").await.is_none(),
        "a passing join writes no timeout note"
    );
}

// ---------------------------------------------------------------------------
// Step 2a: timeout path with `on_timeout` → reroute
// ---------------------------------------------------------------------------

/// Hanging upstream + `timeout_ms` + `on_timeout`: the first drive parks the
/// join at 1/2 arrivals (`waiting_for_input` — daemon-visible over HTTP);
/// once the deadline passes, the next drive re-steps the join, which
/// reroutes to `fallback` (writing `_join_timeout_note`) and completes.
#[tokio::test]
async fn hanging_upstream_with_timeout_reroutes_via_on_timeout() {
    let d = LiveDaemon::start().await;
    let sid = start_preset_session(&d, TIMEOUT_REROUTE_YAML).await;

    // Drive 1: start -> branch_a -> join (parks at 1/2 arrivals).
    let first = drive(&d, &sid, false).await;
    assert_eq!(
        first,
        PresetRunOutcome::WaitingForInput { steps: 3 },
        "the join waits for the never-arriving upstream"
    );

    // Daemon-observable: HTTP shows the parked join waiting for input
    // (this is exactly the state DR-06 was about — a join that sits here
    // forever without a driver).
    let (http_status, status) = http_session_status(&d, &sid).await;
    assert_eq!(http_status, 200);
    assert_eq!(status, "waiting_for_input", "HTTP surfaces the join wait");

    // Park past the deadline, then re-drive: the join re-checks its
    // deadline when stepped and reroutes to `fallback`.
    tokio::time::sleep(PAST_DEADLINE_SLEEP).await;
    let second = drive(&d, &sid, true).await;
    assert_eq!(
        second,
        PresetRunOutcome::Completed { steps: 3 },
        "reroute: join -> fallback -> done = 3 steps on the second drive"
    );

    // Reroute evidence in the persisted session context (`_join_timeout_note`
    // names the join and the reroute target; the join keys are cleared for
    // the next cycle).
    let ctx = d.engine.get_context(&sid).await.expect("context");
    let note = ctx
        .get::<String>("_join_timeout_note")
        .await
        .expect("reroute must write _join_timeout_note");
    assert!(
        note.contains("join timeout at 'join'"),
        "note names the join state: {note}"
    );
    assert!(
        note.contains("rerouting to 'fallback'"),
        "note names the reroute target: {note}"
    );
    assert!(
        note.contains("gate=converge"),
        "note names the gate: {note}"
    );
    let elapsed = elapsed_ms_from_error(&note);
    assert!(
        elapsed >= JOIN_TIMEOUT_MS,
        "deadline elapsed must exceed the join timeout: {note}"
    );
    assert!(
        ctx.get::<HashSet<String>>("_converge_arrivals_join")
            .await
            .is_none(),
        "reroute clears the converge arrivals key"
    );
    assert!(
        ctx.get::<u64>("_join_wait_start_join").await.is_none(),
        "reroute clears the wait-start key"
    );

    // Terminal: daemon engine reports Completed; HTTP active-list 404.
    assert_eq!(
        d.engine.get_status(&sid).await.expect("status"),
        SessionStatus::Completed
    );
    let (http_status, _) = http_session_status(&d, &sid).await;
    assert_eq!(http_status, 404);
}

// ---------------------------------------------------------------------------
// Step 2b: timeout path without `on_timeout` → typed failure
// ---------------------------------------------------------------------------

/// Hanging upstream + `timeout_ms`, NO `on_timeout`: at the deadline the
/// join must fail with the typed `converge_timeout:` discriminator — never
/// sit at `WaitForInput` forever. The failure record is persisted by the
/// driver (`_run_status` / `_run_error` in the session context) and the
/// daemon-visible tracker status flips to `failed`.
#[tokio::test]
async fn hanging_upstream_without_on_timeout_fails_typed_not_waiting_forever() {
    let d = LiveDaemon::start().await;
    let sid = start_preset_session(&d, TIMEOUT_ERROR_YAML).await;

    // Drive 1: park the join at 1/2 arrivals.
    let first = drive(&d, &sid, false).await;
    assert_eq!(first, PresetRunOutcome::WaitingForInput { steps: 3 });

    // Park past the deadline, then re-drive.
    tokio::time::sleep(PAST_DEADLINE_SLEEP).await;
    let second = drive(&d, &sid, true).await;

    let error = match second {
        PresetRunOutcome::Failed { steps, error } => {
            assert_eq!(steps, 1, "the deadline-firing tick is one step");
            error
        }
        other => panic!(
            "absent on_timeout must FAIL TYPED at the deadline, not {}",
            match other {
                PresetRunOutcome::WaitingForInput { .. } => "WaitForInput forever".to_string(),
                o => format!("{o:?}"),
            }
        ),
    };

    // Typed discriminator + per-field payload (gate/state/arrived/expected).
    assert!(
        error.contains("converge_timeout:"),
        "typed discriminator must surface: {error}"
    );
    assert!(error.contains("gate=converge"), "gate field: {error}");
    assert!(error.contains("state_id=join"), "state field: {error}");
    assert!(
        error.contains("arrived=1"),
        "arrived field (branch_a only): {error}"
    );
    assert!(
        error.contains("expected=2"),
        "expected field (branch_a + hanging branch_b): {error}"
    );
    let elapsed = elapsed_ms_from_error(&error);
    assert!(
        elapsed >= JOIN_TIMEOUT_MS,
        "deadline elapsed must exceed the join timeout: {error}"
    );

    // Daemon-observable tracker: the driver flipped the session to failed.
    assert_eq!(
        d.engine.get_status(&sid).await.expect("status"),
        SessionStatus::Failed,
        "daemon-visible status must be failed after the typed failure"
    );

    // Driver-landed failure record in the persisted session context.
    let ctx = d.engine.get_context(&sid).await.expect("context");
    assert_eq!(
        ctx.get::<String>("_run_status").await.as_deref(),
        Some("failed"),
        "driver persists _run_status=failed"
    );
    let run_error = ctx
        .get::<String>("_run_error")
        .await
        .expect("driver persists _run_error");
    assert!(
        run_error.contains("converge_timeout:") && run_error.contains("state_id=join"),
        "persisted failure record carries the typed discriminator: {run_error}"
    );
    assert!(
        ctx.get::<String>("_join_timeout_note").await.is_none(),
        "typed-fail path writes no reroute note"
    );

    // Terminal: HTTP active-list 404.
    let (http_status, _) = http_session_status(&d, &sid).await;
    assert_eq!(http_status, 404);
}
