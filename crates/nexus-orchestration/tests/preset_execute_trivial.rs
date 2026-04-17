//! T3 test: Linear two-state preset with ManualWait executes to terminal.
//!
//! Validates:
//! - §8.2 mapping: capability→CapabilityTask, manual→ManualWaitTask, terminal→End
//! - `start_session_with_graph` trait method
//! - `run_step` correctly drives the state machine
//! - `signal(Resume)` unblocks WaitForInput

use nexus_orchestration::engine::StepOutcome;
use nexus_orchestration::{CapabilityRegistry, GraphFlowEngine, OrchestrationEngine};
use std::sync::Arc;

/// Minimal two-state preset: `start` → `end`.
///
/// - `start`: exit_when=manual (WaitForInput), next=end
/// - `end`: terminal (NextAction::End)
const TWO_STATE_MANUAL_YAML: &str = r#"
preset:
  id: trivial
  version: 1
  kind: creator
  description: "two-state manual preset for T3"
  requires_capabilities: []
  initial: start
  terminal: end
states:
  - id: start
    enter: []
    exit_when:
      kind: manual
    next: end
  - id: end
    terminal: true
"#;

#[tokio::test]
async fn linear_two_state_preset_executes_to_terminal() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded =
        nexus_orchestration::preset::load_preset_from_str(TWO_STATE_MANUAL_YAML, &caps).unwrap();

    assert_eq!(loaded.id, "trivial");

    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let engine = GraphFlowEngine::new_with_storage(storage, Arc::new(caps));

    let sid = engine
        .start_session_with_graph("trivial", loaded.outer_graph.clone())
        .await
        .expect("start_session_with_graph");

    // Step 1: should reach `start` which has ManualWait → WaitForInput.
    let out = engine.run_step(&sid).await.expect("run_step 1");
    assert!(
        out.is_waiting_for_input(),
        "first step should WaitForInput (manual exit_when): {:?}",
        out
    );

    // Signal Resume to unblock the manual wait.
    engine
        .signal(&sid, nexus_orchestration::engine::EngineSignal::Resume)
        .await
        .expect("signal resume");

    // Step 2+: engine advances to `end` (terminal) → Completed.
    // The graph-flow engine may need a couple of steps to traverse:
    // - After resume, re-run `start` → Continue → moves to `end` (Paused)
    // - Next step runs `end` → End → Completed
    for _ in 0..16 {
        let o = engine.run_step(&sid).await.expect("run_step loop");
        if matches!(o, StepOutcome::Completed { .. }) {
            break;
        }
    }

    let final_status = engine.get_status(&sid).await.expect("get_status");
    assert!(
        final_status.is_completed(),
        "preset should reach terminal state: {:?}",
        final_status
    );
}

/// Test that a capability-enter state with a valid built-in capability
/// runs successfully through the composite task.
const CAPABILITY_ENTER_YAML: &str = r#"
preset:
  id: cap-enter
  version: 1
  kind: creator
  description: "capability enter test"
  requires_capabilities:
    - workspace.open
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: capability
        name: workspace.open
    exit_when:
      kind: manual
    next: b
  - id: b
    terminal: true
"#;

#[tokio::test]
async fn capability_enter_state_composites_correctly() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded =
        nexus_orchestration::preset::load_preset_from_str(CAPABILITY_ENTER_YAML, &caps).unwrap();

    assert_eq!(loaded.id, "cap-enter");

    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let engine = GraphFlowEngine::new_with_storage(storage, Arc::new(caps));

    let sid = engine
        .start_session_with_graph("cap-enter", loaded.outer_graph.clone())
        .await
        .expect("start_session_with_graph");

    // Step 1: should reach WaitForInput (manual exit_when).
    let out = engine.run_step(&sid).await.expect("run_step 1");
    assert!(
        out.is_waiting_for_input(),
        "first step should WaitForInput: {:?}",
        out
    );

    // Resume to unblock.
    engine
        .signal(&sid, nexus_orchestration::engine::EngineSignal::Resume)
        .await
        .expect("signal resume");

    // Run steps until terminal or error.
    for _ in 0..16 {
        let o = engine.run_step(&sid).await.expect("run_step loop");
        if matches!(o, StepOutcome::Completed { .. }) {
            break;
        }
    }

    let final_status = engine.get_status(&sid).await.expect("get_status");
    assert!(
        final_status.is_completed(),
        "capability-enter preset should complete: {:?}",
        final_status
    );
}

/// Test that the outer graph has the correct §8.2 mapping structure.
#[test]
fn outer_graph_tasks_have_state_ids() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded =
        nexus_orchestration::preset::load_preset_from_str(TWO_STATE_MANUAL_YAML, &caps).unwrap();

    assert!(loaded.outer_graph.get_task("start").is_some());
    assert!(loaded.outer_graph.get_task("end").is_some());
}

/// Test that inner graph nodes are created per §8.2.
#[test]
fn inner_graph_nodes_created_per_mapping() {
    let yaml = r#"
preset:
  id: ig-map
  version: 1
  kind: creator
  description: "inner graph mapping test"
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: my_graph
    exit_when:
      kind: graph_complete
    next: b
  - id: b
    terminal: true
inner_graphs:
  my_graph:
    nodes:
      - id: n1
        kind: acp_prompt
      - id: n2
        kind: acp_prompt
        depends_on: [n1]
    output_binding: n2.text
"#;
    let caps = CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_preset_from_str(yaml, &caps).unwrap();

    assert!(loaded.inner_graphs.contains_key("my_graph"));
    let ig = &loaded.inner_graphs["my_graph"];
    assert!(ig.get_task("n1").is_some());
    assert!(ig.get_task("n2").is_some());
}
