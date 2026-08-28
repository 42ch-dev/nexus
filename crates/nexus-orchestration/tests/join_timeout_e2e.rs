//! DR-06 (v1.179) bounded-join timeout e2e — `timeout_ms` / `on_timeout` on
//! join states (merge AND converge gates, single `converge_timeout:`
//! discriminator).
//!
//! Scenarios (plan Task 2):
//! (a) converge reroute — deadline fires, `on_timeout` resolves → run leaves
//!     the join via `NextAction::GoTo(target)`, arrivals key cleared.
//! (b) converge error — deadline fires without `on_timeout` → typed
//!     `GraphError::TaskExecutionFailed` beginning with `converge_timeout:`
//!     naming gate/state/arrived/expected/elapsed; the join does NOT sit on
//!     `WaitForInput` past the deadline (`DeterministicClock`).
//! (c) merge reroute — same reroute semantics on a merge-gate node.
//! (d) merge error — `converge_timeout: gate=merge …` (same discriminator).
//! (e) default-None — no `timeout_ms` set → waiting behaviour byte-identical
//!     to pre-DR-06 (no wait-start tracking key ever written).
//! (f) F-001 (QC fix) — a successful join exit clears `_join_wait_start_{id}`
//!     so a same-session re-entry starts a fresh wait budget; a merge-success
//!     tick whose converge gate still waits keeps the shared state-level
//!     budget alive (§3.3.3).
//! (h) `timeout_ms: 0` — fires on the first waiting tick (reroute or typed
//!     fail), pinned as the documented §3.3.3 semantics.
//! (i) elapsed uses a saturating comparison — `u64::MAX` never wraps into a
//!     spurious deadline.
//!
//! All timing is exercised through [`DeterministicClock`] — no wall-clock
//! sleeps. Converge arrivals go through the real runtime path
//! (`StateCompositeTask::record_converge_arrival`), per the converge e2e
//! contract.

use graph_flow::{Context, NextAction, Task};
use nexus_orchestration::preset::manifest::{
    ConvergeConfig, ConvergeStrategy, NextTarget, StateDefinition,
};
use nexus_orchestration::tasks::{DeterministicClock, StateCompositeTask};
use std::collections::HashSet;
use std::sync::Arc;

/// Build a converge-gate task with bounded-join fields and an injected clock.
fn make_converge_task(
    id: &str,
    timeout_ms: Option<u64>,
    on_timeout: Option<&str>,
    clock: Arc<DeterministicClock>,
) -> StateCompositeTask {
    let preds: HashSet<String> = ["a", "b"].iter().map(|s| (*s).to_string()).collect();
    StateCompositeTask::from_manifest(&StateDefinition {
        id: id.to_string(),
        description: None,
        enter: vec![],
        exit_when: None,
        next: Some(NextTarget::Linear("done".to_string())),
        terminal: false,
        context_update: None,
        merge: None,
        converge: Some(ConvergeConfig {
            strategy: ConvergeStrategy::WaitForAll,
        }),
        timeout_ms,
        on_timeout: on_timeout.map(std::string::ToString::to_string),
    })
    .with_converge_predecessors(preds)
    .with_join_clock(clock)
}

/// Build a merge-gate task (2 expected incoming labeled edges, no explicit
/// `merge:` — default wait-all) with bounded-join fields and an injected clock.
fn make_merge_task(
    id: &str,
    timeout_ms: Option<u64>,
    on_timeout: Option<&str>,
    clock: Arc<DeterministicClock>,
) -> StateCompositeTask {
    StateCompositeTask::from_manifest(&StateDefinition {
        id: id.to_string(),
        description: None,
        enter: vec![],
        exit_when: None,
        next: Some(NextTarget::Linear("done".to_string())),
        terminal: false,
        context_update: None,
        merge: None,
        converge: None,
        timeout_ms,
        on_timeout: on_timeout.map(std::string::ToString::to_string),
    })
    .with_expected_incoming(2)
    .with_join_clock(clock)
}

/// Record a converge arrival using the real runtime path.
fn converge_arrive(ctx: &Context, target_id: &str, source_id: &str) {
    StateCompositeTask::record_converge_arrival(ctx, target_id, source_id);
}

/// Record a merge arrival (direct key write, mirroring the existing merge
/// unit-test convention — merge arrivals have no public recorder).
fn merge_arrive(ctx: &Context, target_id: &str, label: &str) {
    let key = format!("_merge_{target_id}");
    let mut arrived: Vec<String> = ctx.get_sync(&key).unwrap_or_default();
    arrived.push(label.to_string());
    ctx.set_sync(&key, arrived);
}

// ── (a) converge reroute ─────────────────────────────────────────────────

#[tokio::test]
async fn converge_timeout_reroutes_to_on_timeout_and_clears_arrivals() {
    let clock = Arc::new(DeterministicClock::new(1_000));
    let task = make_converge_task("join_a", Some(100), Some("timeout_handler"), clock.clone());
    let ctx = Context::new();

    // Partial arrival, first waiting tick: within deadline → wait.
    converge_arrive(&ctx, "join_a", "a");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::WaitForInput),
        "within deadline should still wait; got {:?}",
        result.next_action
    );
    assert!(
        ctx.get_sync::<Vec<String>>("_converge_arrivals_join_a")
            .is_some(),
        "arrival must survive while inside the deadline"
    );

    // Deadline elapses → reroute to the on_timeout state.
    clock.advance(150);
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::GoTo(ref t) if t == "timeout_handler"),
        "deadline exceeded with valid on_timeout should GoTo the target; got {:?}",
        result.next_action
    );
    // Arrivals key cleared for the next cycle + context note written.
    assert!(
        ctx.get_sync::<HashSet<String>>("_converge_arrivals_join_a")
            .is_none(),
        "arrivals key must be cleared on timeout reroute"
    );
    let note: String = ctx
        .get_sync("_join_timeout_note")
        .expect("context note written on reroute");
    assert!(
        note.contains("timeout_handler"),
        "note names the target: {note}"
    );
}

// ── (b) converge typed error ─────────────────────────────────────────────

#[tokio::test]
async fn converge_timeout_without_on_timeout_fails_typed_not_wait() {
    let clock = Arc::new(DeterministicClock::new(2_000));
    let task = make_converge_task("join_b", Some(100), None, clock.clone());
    let ctx = Context::new();

    // First waiting tick inside the deadline → still waits.
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(matches!(result.next_action, NextAction::WaitForInput));

    // Past the deadline the graph must NOT sit on WaitForInput — it fails
    // typed.
    clock.advance(101);
    let err = task
        .run(ctx.clone())
        .await
        .expect_err("deadline without on_timeout must error");
    let msg = match &err {
        graph_flow::GraphError::TaskExecutionFailed(m) => m.clone(),
        other => panic!("expected TaskExecutionFailed, got {other:?}"),
    };
    assert!(
        msg.starts_with("converge_timeout:"),
        "typed discriminator: {msg}"
    );
    assert!(msg.contains("gate=converge"), "names the gate: {msg}");
    assert!(msg.contains("state_id=join_b"), "names the state: {msg}");
    assert!(msg.contains("arrived=0"), "names arrived: {msg}");
    assert!(msg.contains("expected=2"), "names expected: {msg}");
    assert!(msg.contains("elapsed_ms=101"), "names elapsed: {msg}");
    // F-004 (qc2): the typed-fail path also clears arrivals + wait-start.
    assert!(
        ctx.get_sync::<HashSet<String>>("_converge_arrivals_join_b")
            .is_none(),
        "typed fail must clear the converge arrivals key"
    );
    assert!(
        ctx.get_sync::<u64>("_join_wait_start_join_b").is_none(),
        "typed fail must clear the wait-start key"
    );
}

// ── (c) merge reroute ────────────────────────────────────────────────────

#[tokio::test]
async fn merge_timeout_reroutes_to_on_timeout_and_clears_arrivals() {
    let clock = Arc::new(DeterministicClock::new(5_000));
    let task = make_merge_task("join_c", Some(200), Some("merge_handler"), clock.clone());
    let ctx = Context::new();

    merge_arrive(&ctx, "join_c", "label_a");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::WaitForInput),
        "1/2 arrivals within deadline should wait; got {:?}",
        result.next_action
    );

    clock.advance(250);
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::GoTo(ref t) if t == "merge_handler"),
        "merge-gate deadline with on_timeout should GoTo the target; got {:?}",
        result.next_action
    );
    assert!(
        ctx.get_sync::<Vec<String>>("_merge_join_c").is_none(),
        "merge arrivals key must be cleared on timeout reroute"
    );
}

// ── (d) merge typed error ────────────────────────────────────────────────

#[tokio::test]
async fn merge_timeout_without_on_timeout_fails_typed_with_merge_gate() {
    let clock = Arc::new(DeterministicClock::new(10_000));
    let task = make_merge_task("join_d", Some(200), None, clock.clone());
    let ctx = Context::new();

    merge_arrive(&ctx, "join_d", "label_a");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(matches!(result.next_action, NextAction::WaitForInput));

    clock.advance(300);
    let err = task
        .run(ctx.clone())
        .await
        .expect_err("merge deadline without on_timeout must error");
    let msg = match &err {
        graph_flow::GraphError::TaskExecutionFailed(m) => m.clone(),
        other => panic!("expected TaskExecutionFailed, got {other:?}"),
    };
    // Single discriminator serves BOTH gates — merge parity by construction.
    assert!(
        msg.starts_with("converge_timeout:"),
        "typed discriminator: {msg}"
    );
    assert!(msg.contains("gate=merge"), "names the merge gate: {msg}");
    assert!(msg.contains("state_id=join_d"), "names the state: {msg}");
    assert!(msg.contains("arrived=1"), "names arrived: {msg}");
    assert!(msg.contains("expected=2"), "names expected: {msg}");
    // F-004 (qc2): the typed-fail path also clears arrivals + wait-start.
    assert!(
        ctx.get_sync::<Vec<String>>("_merge_join_d").is_none(),
        "typed fail must clear the merge arrivals key"
    );
    assert!(
        ctx.get_sync::<u64>("_join_wait_start_join_d").is_none(),
        "typed fail must clear the wait-start key"
    );
}

// ── (e) default-None byte-identical behaviour ────────────────────────────

#[tokio::test]
async fn join_without_timeout_fields_keeps_legacy_waiting_and_writes_no_tracking_keys() {
    let clock = Arc::new(DeterministicClock::new(50_000));
    let task = make_converge_task("join_e", None, None, clock.clone());
    let ctx = Context::new();

    // Many ticks far past any plausible deadline — must keep waiting exactly
    // like the pre-DR-06 behaviour, with no wait-start tracking key written.
    for _ in 0..5 {
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::WaitForInput),
            "unbounded join keeps waiting; got {:?}",
            result.next_action
        );
        clock.advance(1_000_000);
    }
    assert!(
        ctx.get_sync::<u64>("_join_wait_start_join_e").is_none(),
        "no wait-start tracking key may be written when timeout_ms is absent"
    );
    assert!(
        ctx.get_sync::<String>("_join_timeout_note").is_none(),
        "no timeout note may be written when timeout_ms is absent"
    );
}

// ── deadline edge: elapsed == timeout fires, elapsed < timeout waits ─────

#[tokio::test]
async fn converge_timeout_fires_exactly_at_deadline() {
    let clock = Arc::new(DeterministicClock::new(0));
    let task = make_converge_task("join_edge", Some(500), None, clock.clone());
    let ctx = Context::new();

    // First tick records wait-start at t=0.
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(matches!(result.next_action, NextAction::WaitForInput));

    // elapsed == deadline → fires (deadline is inclusive).
    clock.advance(500);
    let err = task.run(ctx.clone()).await.unwrap_err();
    let msg = match &err {
        graph_flow::GraphError::TaskExecutionFailed(m) => m.clone(),
        other => panic!("expected TaskExecutionFailed, got {other:?}"),
    };
    assert!(
        msg.contains("elapsed_ms=500"),
        "fires at the deadline: {msg}"
    );
}

// ── (f) F-001 (v1.179 QC fix): success-leave wait-start clearing ─────────

/// F-001: a successful join exit must clear `_join_wait_start_{id}`. A
/// same-session re-entry (runtime `GoTo` loop — the DAG load only rejects
/// *static* cycles) otherwise reuses the stale timestamp and fires the
/// deadline immediately instead of starting a fresh wait budget.
#[tokio::test]
async fn join_exit_clears_wait_start_so_same_session_reentry_gets_fresh_budget() {
    let clock = Arc::new(DeterministicClock::new(1_000));
    let task = make_converge_task("join_f", Some(100), Some("timeout_handler"), clock.clone());
    let ctx = Context::new();

    // Cycle 1: partial arrival → the waiting tick starts the budget at t=1000.
    converge_arrive(&ctx, "join_f", "a");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(matches!(result.next_action, NextAction::WaitForInput));

    // Last predecessor arrives → the gate passes and the join LEAVES. The
    // wait-start key must retire with the join cycle.
    converge_arrive(&ctx, "join_f", "b");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::Continue),
        "complete join should advance; got {:?}",
        result.next_action
    );
    assert!(
        ctx.get_sync::<u64>("_join_wait_start_join_f").is_none(),
        "successful join exit must clear the wait-start key"
    );

    // Same-session re-entry far past the ORIGINAL deadline: a fresh budget
    // must start — the stale timestamp must not fire the deadline.
    clock.advance(5_000);
    converge_arrive(&ctx, "join_f", "a");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::WaitForInput),
        "re-entry must wait on a fresh budget, not fire the stale deadline; \
         got {:?}",
        result.next_action
    );

    // And the fresh budget still bounds: past the NEW deadline it fires.
    clock.advance(150);
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(&result.next_action, NextAction::GoTo(t) if t == "timeout_handler"),
        "deadline still enforced after re-entry; got {:?}",
        result.next_action
    );
}

/// F-001 corollary: `timeout_ms` is ONE state-level budget shared by BOTH
/// gates (§3.3.3) — a merge-success tick whose converge gate still has to
/// wait in the same tick must NOT clear the wait-start key.
#[tokio::test]
async fn merge_pass_keeps_state_level_wait_budget_for_converge_gate() {
    let clock = Arc::new(DeterministicClock::new(1_000));
    let preds: HashSet<String> = ["a", "b"].iter().map(|s| (*s).to_string()).collect();
    let task = StateCompositeTask::from_manifest(&StateDefinition {
        id: "join_g".to_string(),
        description: None,
        enter: vec![],
        exit_when: None,
        next: Some(NextTarget::Linear("done".to_string())),
        terminal: false,
        context_update: None,
        merge: None,
        converge: Some(ConvergeConfig {
            strategy: ConvergeStrategy::WaitForAll,
        }),
        timeout_ms: Some(100),
        on_timeout: None,
    })
    .with_expected_incoming(2)
    .with_converge_predecessors(preds)
    .with_join_clock(clock.clone());
    let ctx = Context::new();

    // Tick 1: merge gate waiting (1/2) → the waiting tick starts the budget.
    merge_arrive(&ctx, "join_g", "label_a");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(matches!(result.next_action, NextAction::WaitForInput));

    // Tick 2 (t=1050): the merge gate passes, but the converge gate still
    // waits in the same tick — the budget must survive the merge success.
    clock.advance(50);
    merge_arrive(&ctx, "join_g", "label_b");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(matches!(result.next_action, NextAction::WaitForInput));

    // Tick 3 (t=1110): elapsed measured from the ORIGINAL wait-start
    // (110 ms) fires the shared budget (whichever gate is waiting — the
    // merge key was cleared by the merge success, so the merge gate re-check
    // sees 0/2). A merge-success clear would have restarted the budget at
    // t=1050 and the deadline would NOT fire (only 60 ms would have elapsed).
    clock.advance(60);
    let err = task
        .run(ctx.clone())
        .await
        .expect_err("shared budget measured from the first waiting tick must fire");
    let msg = match &err {
        graph_flow::GraphError::TaskExecutionFailed(m) => m.clone(),
        other => panic!("expected TaskExecutionFailed, got {other:?}"),
    };
    assert!(
        msg.contains("elapsed_ms=110"),
        "deadline measured from the ORIGINAL wait-start, not the merge-pass \
         tick: {msg}"
    );
}

// ── (h) timeout_ms = 0 semantics (qc2 F-003 / qc3 S-4) ───────────────────

/// `timeout_ms: 0` means the deadline is already elapsed on the first
/// waiting tick: that tick immediately reroutes (with `on_timeout`) or
/// fails typed (without). Pinned as the documented §3.3.3 semantics.
#[tokio::test]
async fn timeout_ms_zero_fires_immediately_on_first_waiting_tick() {
    // With `on_timeout` → immediate reroute on the first waiting tick.
    let clock = Arc::new(DeterministicClock::new(500));
    let reroute = make_converge_task("join_h1", Some(0), Some("timeout_handler"), clock);
    let ctx = Context::new();
    let result = reroute.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(&result.next_action, NextAction::GoTo(t) if t == "timeout_handler"),
        "timeout_ms=0 reroutes on the first waiting tick; got {:?}",
        result.next_action
    );

    // Without `on_timeout` → typed fail on the first waiting tick, elapsed 0.
    let clock = Arc::new(DeterministicClock::new(500));
    let fail = make_converge_task("join_h2", Some(0), None, clock);
    let ctx = Context::new();
    let err = fail.run(ctx.clone()).await.unwrap_err();
    let msg = match &err {
        graph_flow::GraphError::TaskExecutionFailed(m) => m.clone(),
        other => panic!("expected TaskExecutionFailed, got {other:?}"),
    };
    assert!(
        msg.contains("elapsed_ms=0"),
        "timeout_ms=0 fires with zero elapsed: {msg}"
    );
}

// ── (i) saturating elapsed comparison (no wrap at extreme clock values) ──

/// The elapsed comparison is `now − start` (saturating) against
/// `timeout_ms` — never `start + timeout_ms` — so extreme clock values
/// cannot wrap into a spurious deadline.
#[tokio::test]
async fn timeout_ms_u64_max_never_spuriously_fires_near_clock_maximum() {
    let clock = Arc::new(DeterministicClock::new(u64::MAX - 1_000));
    let task = make_converge_task("join_i", Some(u64::MAX), None, clock.clone());
    let ctx = Context::new();

    let result = task.run(ctx.clone()).await.unwrap();
    assert!(matches!(result.next_action, NextAction::WaitForInput));

    clock.advance(500); // now = u64::MAX − 500; elapsed = 500, no wrap.
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::WaitForInput),
        "elapsed 500 < u64::MAX must keep waiting; got {:?}",
        result.next_action
    );
}
