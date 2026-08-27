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
