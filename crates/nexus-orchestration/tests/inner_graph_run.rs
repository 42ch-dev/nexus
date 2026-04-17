//! T5 test: Outer state with inner_graph runs to completion and exports output.
//!
//! Validates:
//! - InnerGraphTask spawns a child session
//! - Child session polls to completion
//! - output_binding is read from child context and written to parent
//! - `engine.spawn_child_session` + `engine.get_context` work on trait + impl

use nexus_orchestration::engine::StepOutcome;
use nexus_orchestration::{CapabilityRegistry, GraphFlowEngine, OrchestrationEngine};
use std::sync::Arc;

/// Preset with one outer state A that enters an inner_graph `my_graph`.
/// The inner graph has three nodes: n1 -> n2 -> n3 (rule-only, no ACP).
/// output_binding: n3.text
const INNER_GRAPH_PRESET_YAML: &str = r#"
preset:
  id: inner-graph-test
  version: 1
  kind: creator
  description: "inner graph execution test for T5"
  requires_capabilities: []
  initial: A
  terminal: B
states:
  - id: A
    enter:
      - kind: inner_graph
        name: my_graph
    exit_when:
      kind: graph_complete
    next: B
  - id: B
    terminal: true
inner_graphs:
  my_graph:
    nodes:
      - id: n1
        kind: acp_prompt
      - id: n2
        kind: acp_prompt
        depends_on: [n1]
      - id: n3
        kind: acp_prompt
        depends_on: [n2]
    output_binding: n3.text
"#;

#[tokio::test]
async fn inner_graph_runs_to_completion_and_exports_output() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_preset_from_str(INNER_GRAPH_PRESET_YAML, &caps)
        .expect("load preset");

    assert_eq!(loaded.id, "inner-graph-test");
    assert!(loaded.inner_graphs.contains_key("my_graph"));
    assert_eq!(loaded.output_bindings.get("my_graph").unwrap(), "n3.text");

    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let engine = Arc::new(GraphFlowEngine::new_with_storage(storage));

    // Use start_session_with_preset which wires engine + inner graphs.
    let sid = engine
        .start_session_with_preset(&loaded)
        .await
        .expect("start_session_with_preset");

    // Run steps until terminal or error (max 32 steps).
    let mut steps = 0;
    loop {
        let out = engine.run_step(&sid).await.expect("run_step");
        steps += 1;

        match out {
            StepOutcome::Completed { .. } => break,
            StepOutcome::Error(e) => panic!("engine error after {steps} steps: {e}"),
            StepOutcome::WaitingForInput { .. } => {
                // Inner graphs shouldn't wait for input; resume.
                engine
                    .signal(&sid, nexus_orchestration::engine::EngineSignal::Resume)
                    .await
                    .expect("signal resume");
            }
            StepOutcome::Paused { .. } => {
                // Resume paused sessions (shouldn't happen for rule-only inner graphs).
                engine
                    .signal(&sid, nexus_orchestration::engine::EngineSignal::Resume)
                    .await
                    .expect("signal resume");
            }
        }

        if steps > 32 {
            panic!("did not reach terminal state after {steps} steps");
        }
    }

    // Verify completion.
    let final_status = engine.get_status(&sid).await.expect("get_status");
    assert!(
        final_status.is_completed(),
        "preset should be completed: {final_status:?}"
    );

    // Verify output was exported from inner graph.
    let ctx = engine.get_context(&sid).await.expect("get_context");
    let exported: String = ctx.get("state.A.output").await.unwrap_or_default();
    assert!(
        !exported.is_empty(),
        "state.A.output should be populated from inner graph output_binding"
    );
    // The InnerGraphNodeTask stub produces "inner_node:<id>:stub_output",
    // so n3.text would be stored as nodes.n3.text.
    // The output_binding is "n3.text", and InnerGraphTask reads both
    // "n3.text" and "nodes.n3.text" from the child context.
    assert!(
        exported.contains("n3"),
        "exported output should reference n3: {exported}"
    );
}

/// Test that spawn_child_session and get_context work via the trait.
#[tokio::test]
async fn spawn_child_and_get_context() {
    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let engine = Arc::new(GraphFlowEngine::new_with_storage(storage));

    // Create a simple inner graph.
    let inner = graph_flow::Graph::new("test_child");
    inner.add_task(std::sync::Arc::new(
        nexus_orchestration::tasks::InnerGraphNodeTask::new("x"),
    ));

    let params = nexus_orchestration::engine::ChildSessionParams {
        parent_session_id: "parent-1".to_string(),
        inner_graph: Arc::new(inner),
        initial_context: graph_flow::Context::new(),
    };

    let child_sid = engine
        .spawn_child_session(params)
        .await
        .expect("spawn_child_session");

    // Run the child to completion.
    for _ in 0..8 {
        let out = engine.run_step(&child_sid).await.expect("run_step child");
        if matches!(out, StepOutcome::Completed { .. }) {
            break;
        }
    }

    // Get child context.
    let child_ctx = engine.get_context(&child_sid).await.expect("get_context");
    let output: String = child_ctx.get("nodes.x.output").await.unwrap_or_default();
    assert!(
        !output.is_empty(),
        "child should have produced output: {output}"
    );
}

/// Test that the loaded preset has the correct structure for novel-writing.
#[test]
fn inner_graph_preset_structure() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded =
        nexus_orchestration::preset::load_preset_from_str(INNER_GRAPH_PRESET_YAML, &caps).unwrap();

    assert!(loaded.inner_graphs.contains_key("my_graph"));
    let ig = &loaded.inner_graphs["my_graph"];
    assert!(ig.get_task("n1").is_some());
    assert!(ig.get_task("n2").is_some());
    assert!(ig.get_task("n3").is_some());
    assert_eq!(loaded.output_bindings.get("my_graph").unwrap(), "n3.text");
}
