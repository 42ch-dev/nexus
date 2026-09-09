//! T4 test: `AcpPromptTask` dispatches through the production prompt executor
//! seam (A1).
//!
//! Uses a mock `PromptExecutor` returning a deterministic non-echo
//! transformation, proving the task records the agent output — never the
//! prompt echo.

use graph_flow::Task;
use nexus_orchestration::capability::{
    CapabilityError, PromptExecutor, PromptRequest, PromptResult,
};
use nexus_orchestration::tasks::{AcpPromptTask, ToolPolicy};

/// Mock executor returning a deterministic non-echo transformation.
struct MockExecutor;

#[async_trait::async_trait]
impl PromptExecutor for MockExecutor {
    async fn execute(&self, request: PromptRequest) -> Result<PromptResult, CapabilityError> {
        Ok(PromptResult {
            full_text: format!("transformed:{}", request.prompt),
            host_session_id: "host-sess".to_string(),
            operation_id: "op-1".to_string(),
        })
    }
}

fn empty_session_cancels() -> std::sync::Arc<
    std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
> {
    std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()))
}

#[tokio::test]
async fn acp_prompt_task_dispatches_to_executor_and_records_output() {
    // `new_for_test` defaults the session id to "default" and constructs an
    // empty cancellation map; register the run's coordinator token so the
    // task's fail-closed token resolution succeeds (production registers at
    // run admission).
    let cancels = empty_session_cancels();
    cancels
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            "default".to_string(),
            tokio_util::sync::CancellationToken::new(),
        );
    let task = AcpPromptTask::new(
        Some(std::sync::Arc::new(MockExecutor)),
        cancels,
        "state-1",
        "hello {{core_context.version}}",
        ToolPolicy::AutoGrantReadOnly,
        Some("default".to_string()),
    );

    let ctx = graph_flow::Context::new();
    ctx.set("core_context.version", "0").await;
    let result = task.run(ctx.clone()).await.unwrap();

    // The executor returns the non-echo transformation as full_text.
    assert_eq!(
        result.response.as_deref().unwrap_or(""),
        "transformed:hello 0",
        "response should be the agent transformation, not the prompt echo: {:?}",
        result.response
    );

    // Output should be stored at state.state-1.output.
    let stored: String = ctx.get("state.state-1.output").await.unwrap();
    assert_eq!(stored, "transformed:hello 0");
}

#[tokio::test]
async fn acp_prompt_task_no_executor_refuses() {
    // No executor — must refuse with a typed failure, never a placeholder
    // success (A1: no echo/worker fallback).
    let task = AcpPromptTask::new(
        None,
        empty_session_cancels(),
        "state-2",
        "test prompt {{name}}",
        ToolPolicy::DenyAll,
        None,
    );

    let ctx = graph_flow::Context::new();
    ctx.set("name", "world").await;
    let result = task.run(ctx).await;
    assert!(result.is_err(), "no executor must refuse: {result:?}");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("no prompt executor"),
        "typed refusal expected: {err}"
    );
}

/// Test that the ACP prompt and judge capabilities are registered.
#[test]
fn acp_capabilities_registered() {
    let reg = nexus_orchestration::CapabilityRegistry::with_builtins();
    assert!(
        reg.get("acp.prompt").is_some(),
        "acp.prompt should be registered"
    );
    assert!(
        reg.get("judge.llm").is_some(),
        "judge.llm should be registered"
    );
}

/// Test that the preset loader accepts acp.prompt and judge.llm in `requires_capabilities`.
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
  initial: start
  terminal: start
  initial_action:
    kind: seed_direct
states:
  - id: start
    terminal: true
"#;
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_preset_from_str(yaml, &caps);
    assert!(
        loaded.is_ok(),
        "preset with acp.prompt + judge.llm must validate: {loaded:?}"
    );
}
