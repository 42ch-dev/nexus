//! T4 test: AcpPromptTask dispatches through Worker Manager IPC.
//!
//! Uses an echo-worker fixture that responds to `worker/acp_prompt` with
//! the prompt text echoed back.

use graph_flow::{NextAction, Task};
use nexus_orchestration::tasks::{AcpPromptTask, ToolPolicy};
use nexus_orchestration::worker::WorkerManager;

#[tokio::test]
async fn acp_prompt_task_dispatches_to_worker_and_records_output() {
    let mgr = WorkerManager::new();

    let spec =
        nexus_orchestration::worker::WorkerSpec::test_stub("./tests/fixtures/echo-worker.sh");
    let handle = mgr.spawn(&spec).await.expect("spawn echo-worker");

    let task = AcpPromptTask::new_for_test(
        handle,
        "state-1",
        "hello {{core_context.version}}",
        ToolPolicy::AutoGrantReadOnly,
    );

    let ctx = graph_flow::Context::new();
    ctx.set("core_context.version", "0").await;
    let result = task.run(ctx.clone()).await.unwrap();

    // The echo worker returns the rendered prompt as full_text.
    assert!(
        result.response.as_deref().unwrap_or("").contains("hello 0"),
        "response should contain rendered prompt: {:?}",
        result.response
    );

    // Output should be stored at state.state-1.output.
    let stored: String = ctx.get("state.state-1.output").await.unwrap();
    assert!(
        stored.contains("hello 0"),
        "stored output should contain rendered prompt: {stored}"
    );
}

#[tokio::test]
async fn acp_prompt_task_no_worker_returns_stub() {
    // No worker handle — should operate in stub mode.
    // WS-E T5: session_id parameter defaults to "default" when None.
    let task = AcpPromptTask::new(
        None,
        "state-2",
        "test prompt {{name}}",
        ToolPolicy::DenyAll,
        None,
    );

    let ctx = graph_flow::Context::new();
    ctx.set("name", "world").await;
    let result = task.run(ctx).await.unwrap();

    assert!(matches!(result.next_action, NextAction::Continue));
    let response = result.response.unwrap();
    assert!(
        response.contains("[acp_prompt stub:"),
        "stub mode: {response}"
    );
}

/// Test that WorkerSpec::test_stub creates a valid spec for shell scripts.
#[test]
fn worker_spec_test_stub() {
    let spec = nexus_orchestration::worker::WorkerSpec::test_stub("echo.sh");
    assert_eq!(spec.program, "bash");
    assert!(spec.args.contains(&"echo.sh".to_string()));
}

/// Test that the three new capabilities are registered.
#[test]
fn acp_capabilities_registered() {
    let reg = nexus_orchestration::CapabilityRegistry::with_builtins();
    assert!(
        reg.get("acp.prompt").is_some(),
        "acp.prompt should be registered"
    );
    assert!(
        reg.get("acp.session_load").is_some(),
        "acp.session_load should be registered"
    );
    assert!(
        reg.get("judge.llm").is_some(),
        "judge.llm should be registered"
    );
}

/// Test that the preset loader accepts acp.prompt and judge.llm in requires_capabilities.
#[test]
fn preset_with_acp_capabilities_validates() {
    let yaml = r#"
preset:
  id: acp-test
  version: 1
  kind: creator
  description: "ACP capability validation test"
  requires_capabilities:
    - acp.prompt
    - judge.llm
    - acp.session_load
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when:
      kind: manual
    next: b
  - id: b
    terminal: true
"#;
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_preset_from_str(yaml, &caps);
    assert!(
        loaded.is_ok(),
        "preset with acp capabilities should be valid"
    );
}
