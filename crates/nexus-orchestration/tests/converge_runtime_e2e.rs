//! V1.56 P2 fix-wave (H-001/W-002): Converge runtime integration tests.
//!
//! Tests the converge (merge-point) gate in `StateCompositeTask::run()`.
//! Verifies wait_for_all, first_completed, any strategies, and edge cases.

use graph_flow::{Context, NextAction, Task};
use nexus_orchestration::preset::manifest::{ConvergeConfig, ConvergeStrategy, NextTarget};
use nexus_orchestration::tasks::StateCompositeTask;
use std::collections::HashSet;

/// Build a converge task with the given strategy and no exit condition.
fn make_converge_task(
    id: &str,
    strategy: ConvergeStrategy,
    predecessors: &[&str],
) -> StateCompositeTask {
    let pred_set: HashSet<String> = predecessors.iter().map(|s| s.to_string()).collect();
    StateCompositeTask::from_manifest(&nexus_orchestration::preset::manifest::StateDefinition {
        id: id.to_string(),
        description: None,
        enter: vec![],
        exit_when: None,
        next: Some(NextTarget::Linear("done".to_string())),
        terminal: false,
        context_update: None,
        merge: None,
        converge: Some(ConvergeConfig { strategy }),
    })
    .with_converge_predecessors(pred_set)
}

/// Record an arrival from a predecessor in the converge tracking key.
fn record_arrival(ctx: &Context, target_id: &str) {
    let key = format!("_converge_arrivals_{target_id}");
    let mut arrived: Vec<String> = ctx.get_sync(&key).unwrap_or_default();
    arrived.push("arrived".to_string());
    ctx.set_sync(&key, arrived);
}

// ── wait_for_all tests ────────────────────────────────────────────────

#[tokio::test]
async fn converge_wait_for_all_two_way_both_arrive_advances() {
    let task = make_converge_task("merge_2", ConvergeStrategy::WaitForAll, &["a", "b"]);
    let ctx = Context::new();

    // No arrivals → should wait.
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::WaitForInput),
        "with 0 arrivals, should WaitForInput; got {:?}",
        result.next_action
    );

    // 1 of 2 arrivals → should still wait.
    record_arrival(&ctx, "merge_2");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::WaitForInput),
        "with 1/2 arrivals, should WaitForInput; got {:?}",
        result.next_action
    );

    // 2 of 2 arrivals → should advance.
    record_arrival(&ctx, "merge_2");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::Continue),
        "with 2/2 arrivals, should Continue; got {:?}",
        result.next_action
    );
}

#[tokio::test]
async fn converge_wait_for_all_three_way_advances_when_all_arrive() {
    let task = make_converge_task("merge_3", ConvergeStrategy::WaitForAll, &["a", "b", "c"]);
    let ctx = Context::new();

    // 2 of 3 → wait.
    record_arrival(&ctx, "merge_3");
    record_arrival(&ctx, "merge_3");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(matches!(result.next_action, NextAction::WaitForInput));

    // 3 of 3 → advance.
    record_arrival(&ctx, "merge_3");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(matches!(result.next_action, NextAction::Continue));
}

// ── first_completed tests ─────────────────────────────────────────────

#[tokio::test]
async fn converge_first_completed_advances_on_first_arrival() {
    let task = make_converge_task("merge_fc", ConvergeStrategy::FirstCompleted, &["a", "b"]);
    let ctx = Context::new();

    // 1 arrival → should advance immediately.
    record_arrival(&ctx, "merge_fc");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::Continue),
        "first_completed: 1 arrival should advance; got {:?}",
        result.next_action
    );
}

#[tokio::test]
async fn converge_first_completed_zero_arrivals_waits() {
    let task = make_converge_task("merge_fc2", ConvergeStrategy::FirstCompleted, &["a", "b"]);
    let ctx = Context::new();

    // 0 arrivals → should wait.
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::WaitForInput),
        "first_completed: 0 arrivals should wait; got {:?}",
        result.next_action
    );
}

// ── any strategy tests ─────────────────────────────────────────────────

#[tokio::test]
async fn converge_any_advances_on_first_arrival() {
    let task = make_converge_task("merge_any", ConvergeStrategy::Any, &["a", "b"]);
    let ctx = Context::new();

    // 1 arrival → should advance.
    record_arrival(&ctx, "merge_any");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::Continue),
        "any: 1 arrival should advance; got {:?}",
        result.next_action
    );
}

#[tokio::test]
async fn converge_any_idempotent_second_run_resumes() {
    // After converge gate passes, the resume path advances immediately.
    let task = make_converge_task("merge_idem", ConvergeStrategy::Any, &["a", "b"]);
    let ctx = Context::new();

    // First pass: 1 arrival → advance.
    record_arrival(&ctx, "merge_idem");
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::Continue),
        "any: 1 arrival should advance; got {:?}",
        result.next_action
    );

    // Second pass (resumed via resume_key): advances immediately.
    // The resume path skips the gate entirely.
    ctx.set_sync("_state_merge_idem_resumed", true);
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::Continue),
        "resumed run: should advance; got {:?}",
        result.next_action
    );
}

// ── Edge cases ─────────────────────────────────────────────────────────

#[tokio::test]
async fn converge_no_predecessors_skips_gate() {
    // When a converge state has 0 predecessors, the gate is skipped.
    let task = make_converge_task("merge_orphan", ConvergeStrategy::WaitForAll, &[]);
    let ctx = Context::new();

    // Should advance immediately (gate skipped).
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::Continue),
        "no predecessors: should skip gate and continue; got {:?}",
        result.next_action
    );
}

#[tokio::test]
async fn converge_non_converge_state_skips_gate() {
    // A state without converge config should not be affected.
    let task = StateCompositeTask::from_manifest(
        &nexus_orchestration::preset::manifest::StateDefinition {
            id: "normal_state".to_string(),
            description: None,
            enter: vec![],
            exit_when: None,
            next: Some(NextTarget::Linear("done".to_string())),
            terminal: false,
            context_update: None,
            merge: None,
            converge: None,
        },
    );
    let ctx = Context::new();
    let result = task.run(ctx.clone()).await.unwrap();
    assert!(
        matches!(result.next_action, NextAction::Continue),
        "non-converge state should continue normally; got {:?}",
        result.next_action
    );
}

// ── Reachability regression ────────────────────────────────────────────

#[tokio::test]
async fn reachability_existing_preset_loading_still_works() {
    // Regression: loading a preset with labeled edges and merge still loads
    // successfully (proves converge changes don't break existing paths).
    let yaml = r#"
preset:
  id: regression-test
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: done
states:
  - id: a
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "test template"
    next:
      - label: x
        target: merged
  - id: b
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "test template"
    next:
      - label: y
        target: merged
  - id: merged
    merge:
      kind: all
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;
    let registry = nexus_orchestration::capability::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_preset_from_str(yaml, &registry).unwrap();
    assert_eq!(loaded.id, "regression-test");
    assert!(loaded.outer_graph.get_task("a").is_some());
    assert!(loaded.outer_graph.get_task("merged").is_some());
}
