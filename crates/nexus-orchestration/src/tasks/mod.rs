//! Standard `Task` implementations for the orchestration engine.
//!
//! Design: `.agents/plans/knowledge/orchestration-engine-v1.md` §4.4.

use crate::capability::{CapabilityError, CapabilityRegistry};
use crate::preset::manifest::{
    EnterAction, ExitWhen, StateDefinition,
};
use async_trait::async_trait;
use graph_flow::{NextAction, Task, TaskResult};
use serde_json::Value;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from Task execution.
#[derive(Error, Debug)]
pub enum TaskExecError {
    #[error("capability not found: {0}")]
    CapabilityNotFound(String),
    #[error("capability execution failed: {0}")]
    CapabilityFailed(#[from] CapabilityError),
    #[error("feature not wired: {feature} (since {since})")]
    WsUnwired { feature: String, since: String },
    #[error("invalid input for task: {0}")]
    InvalidInput(String),
}

// ---------------------------------------------------------------------------
// CapabilityTask
// ---------------------------------------------------------------------------

/// Resolves a capability by name, runs it, and stores the result.
///
/// Input (via Context):
/// - `_capability_name` (String): dot-separated capability name
/// - `_capability_input` (Value): input JSON for the capability
pub struct CapabilityTask {
    pub registry: std::sync::Arc<CapabilityRegistry>,
}

#[async_trait]
impl Task for CapabilityTask {
    fn id(&self) -> &str {
        "capability_task"
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        let name: String = context.get("_capability_name").await.unwrap_or_default();
        let input: Value = context
            .get("_capability_input")
            .await
            .unwrap_or(Value::Null);

        let cap = self.registry.get(&name).ok_or_else(|| {
            graph_flow::GraphError::TaskExecutionFailed(format!("capability not found: {name}"))
        })?;

        match cap.run(input).await {
            Ok(output) => {
                context.set("_capability_output", output).await;
                Ok(TaskResult::new(
                    Some("capability executed".to_string()),
                    NextAction::Continue,
                ))
            }
            Err(e) => {
                context.set("_capability_error", format!("{e}")).await;
                Ok(TaskResult::new_with_status(
                    Some(format!("capability error: {e}")),
                    NextAction::Continue,
                    Some(format!("capability '{name}' failed: {e}")),
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RuleCheckTask
// ---------------------------------------------------------------------------

/// Pure function evaluation over Context.
///
/// Reads `_rule` from context, evaluates a simple condition, and returns
/// `NextAction::Continue` if true or `NextAction::WaitForInput` if false.
pub struct RuleCheckTask;

#[async_trait]
impl Task for RuleCheckTask {
    fn id(&self) -> &str {
        "rule_check_task"
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        let rule: String = context.get("_rule").await.unwrap_or_default();

        let (passes, reason) = match rule.as_str() {
            "always_true" => (true, "rule: always_true → pass".to_string()),
            "always_false" => (false, "rule: always_false → fail".to_string()),
            other => (false, format!("unsupported rule: '{other}'")),
        };

        context.set("_rule_result", passes).await;
        context.set("_rule_reason", reason).await;

        let next_action = if passes {
            NextAction::Continue
        } else {
            NextAction::WaitForInput
        };

        Ok(TaskResult::new(
            Some(if passes {
                "rule check passed".to_string()
            } else {
                "rule check failed — waiting for input".to_string()
            }),
            next_action,
        ))
    }
}

// ---------------------------------------------------------------------------
// ManualWaitTask
// ---------------------------------------------------------------------------

/// Returns `NextAction::WaitForInput`. CLI `advance` resumes.
pub struct ManualWaitTask;

#[async_trait]
impl Task for ManualWaitTask {
    fn id(&self) -> &str {
        "manual_wait_task"
    }

    async fn run(
        &self,
        _context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        Ok(TaskResult::new(
            Some("waiting for manual input".to_string()),
            NextAction::WaitForInput,
        ))
    }
}

// ---------------------------------------------------------------------------
// InnerGraphTask
// ---------------------------------------------------------------------------

/// Launches a child Session over a named inner graph.
///
/// **WS2 stub**: returns a typed `WsUnwired` error indicating that inner graph
/// execution is not available until WS3. Does NOT use `todo!()`.
pub struct InnerGraphTask;

#[async_trait]
impl Task for InnerGraphTask {
    fn id(&self) -> &str {
        "inner_graph_task"
    }

    async fn run(
        &self,
        _context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        Err(graph_flow::GraphError::TaskExecutionFailed(
            TaskExecError::WsUnwired {
                feature: "inner_graph".to_string(),
                since: "WS3".to_string(),
            }
            .to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// JudgeTask (rule-only stub)
// ---------------------------------------------------------------------------

/// Evaluates a judge rule. WS2 supports `judge.rule` only (pure function);
/// `judge.llm` is deferred to WS3.
pub struct JudgeTask;

#[async_trait]
impl Task for JudgeTask {
    fn id(&self) -> &str {
        "judge_task"
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        let rule: String = context.get("_judge_rule").await.unwrap_or_default();
        let _context_data: Value = context
            .get("_judge_context_data")
            .await
            .unwrap_or(Value::Null);

        // Use a simple stub evaluator for now.
        let (result, reason) = match rule.as_str() {
            "always_true" => (true, "judge.rule stub: always_true → go".to_string()),
            "always_false" => (false, "judge.rule stub: always_false → nogo".to_string()),
            other => (false, format!("unsupported judge rule: '{other}'")),
        };

        context.set("_judge_result", result).await;
        context.set("_judge_reason", reason.clone()).await;

        let next_action = if result {
            NextAction::Continue
        } else {
            NextAction::WaitForInput
        };

        Ok(TaskResult::new(
            Some(format!("judge: {reason}")),
            next_action,
        ))
    }
}

// ---------------------------------------------------------------------------
// StateCompositeTask (outer graph — per §8.2)
// ---------------------------------------------------------------------------

/// Composite task for an outer-graph state node.
///
/// Encodes the full lifecycle of one state:
/// 1. Run enter actions (capability calls, inner graph launch).
/// 2. Evaluate exit_when condition.
/// 3. Return appropriate NextAction.
///
/// §8.2 mapping:
/// - `enter[*].kind=capability` → CapabilityTask (delegated internally).
/// - `enter[*].kind=inner_graph` → InnerGraphTask (stub WsUnwired until T5).
/// - `exit_when.kind=manual` → ManualWaitTask (returns WaitForInput).
/// - `exit_when.kind=rule` → RuleCheckTask.
/// - `exit_when.kind=llm_judge` → JudgeTask.
/// - `exit_when.kind=graph_complete` → Continue (inner graph handles it).
/// - `terminal: true` → End.
pub struct StateCompositeTask {
    id: String,
    terminal: bool,
    enter_actions: Vec<EnterAction>,
    exit_when: Option<ExitWhen>,
}

impl StateCompositeTask {
    /// Build a composite task from a manifest state definition.
    pub fn from_manifest(state: &StateDefinition) -> Self {
        Self {
            id: state.id.clone(),
            terminal: state.terminal,
            enter_actions: state.enter.clone(),
            exit_when: state.exit_when.clone(),
        }
    }
}

#[async_trait]
impl Task for StateCompositeTask {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        // Check if this is a re-execution after resume.
        // After ManualWait returns WaitForInput, the engine signals Resume
        // and re-runs this task. On the second run, we should skip the wait
        // and just Continue.
        // Use a state-specific key to avoid leaking across state transitions.
        let resume_key = format!("_state_{}_resumed", self.id);
        let resumed: bool = context.get(&resume_key).await.unwrap_or(false);

        if resumed {
            // Already went through the full lifecycle before the wait.
            // Just continue to the next state.
            let response = Some(format!("state '{}': resumed, continuing", self.id));
            tracing::debug!(state_id = %self.id, terminal = self.terminal, "state resumed");
            return Ok(TaskResult::new(response, NextAction::Continue));
        }

        // 1. Process enter actions.
        for action in &self.enter_actions {
            match action {
                EnterAction::Capability { name, args } => {
                    context
                        .set(
                            "_capability_name",
                            name.clone(),
                        )
                        .await;
                    context
                        .set(
                            "_capability_input",
                            args.clone().unwrap_or(Value::Null),
                        )
                        .await;
                    let cap_task = CapabilityTask {
                        registry: std::sync::Arc::new(CapabilityRegistry::with_builtins()),
                    };
                    let cap_result = cap_task.run(context.clone()).await?;
                    // If capability task errored, propagate but still continue
                    // so the state machine doesn't get stuck.
                    if let Some(status_msg) = &cap_result.status_message {
                        context.set("_enter_error", status_msg.clone()).await;
                    }
                }
                EnterAction::InnerGraph { name } => {
                    // T5 implements full InnerGraphTask; for T3 we store the name
                    // and return WsUnwired. The loader already creates InnerGraphTask
                    // stub nodes in the inner graphs themselves.
                    context.set("_inner_graph_name", name.clone()).await;
                    let inner_task = InnerGraphTask;
                    let _ = inner_task.run(context.clone()).await;
                    // For T3: if the inner graph is stub, we still continue to
                    // evaluate exit_when. In T5, this will properly await completion.
                }
            }
        }

        // 2. Evaluate exit_when.
        let next_action = match &self.exit_when {
            None => {
                // No exit condition — terminal state or just ends.
                if self.terminal {
                    NextAction::End
                } else {
                    NextAction::Continue
                }
            }
            Some(ExitWhen::Manual) => {
                // Mark that enter actions have been processed; next run after
                // resume will skip straight to Continue.
                context.set(resume_key, true).await;
                NextAction::WaitForInput
            }
            Some(ExitWhen::Rule) => {
                // Run rule check inline.
                let rule_task = RuleCheckTask;
                let result = rule_task.run(context.clone()).await?;
                result.next_action
            }
            Some(ExitWhen::LlmJudge { .. }) => {
                // Run judge task inline.
                let judge_task = JudgeTask;
                let result = judge_task.run(context.clone()).await?;
                result.next_action
            }
            Some(ExitWhen::GraphComplete) => {
                // Inner graph completion propagates Continue.
                // (InnerGraphTask handles the actual child session; here we just
                // continue since the inner graph ran as part of enter actions.)
                NextAction::Continue
            }
            Some(ExitWhen::Timer { .. }) => {
                // Timer not yet implemented for V1.4; treat as manual wait.
                context.set(resume_key, true).await;
                NextAction::WaitForInput
            }
        };

        // 3. Terminal override — always End regardless of exit_when.
        let final_action = if self.terminal {
            NextAction::End
        } else {
            next_action
        };

        let response = if self.terminal {
            Some(format!("state '{}' completed (terminal)", self.id))
        } else {
            Some(format!("state '{}': {:?}", self.id, final_action))
        };

        Ok(TaskResult::new(response, final_action))
    }
}

// ---------------------------------------------------------------------------
// InnerGraphNodeTask (inner graph nodes — per §8.2)
// ---------------------------------------------------------------------------

/// A task for a node within an inner graph.
///
/// §8.2 mapping:
/// - `kind=acp_prompt` → AcpPromptTask (full in T4; T3 stub that stores a placeholder).
pub struct InnerGraphNodeTask {
    id: String,
}

impl InnerGraphNodeTask {
    /// Create a new inner graph node task.
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string() }
    }
}

#[async_trait]
impl Task for InnerGraphNodeTask {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        // T4 will replace this stub with the full AcpPromptTask behavior.
        // For T3, we just store a placeholder output and continue.
        let output = format!("inner_node:{}:stub_output", self.id);
        context.set(format!("nodes.{}.text", self.id), output.clone()).await;
        context.set(format!("nodes.{}.output", self.id), output.clone()).await;
        Ok(TaskResult::new(
            Some(output),
            NextAction::Continue,
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn manual_wait_returns_wait_for_input() {
        let task = ManualWaitTask;
        let ctx = graph_flow::Context::new();
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::WaitForInput));
    }

    #[tokio::test]
    async fn inner_graph_returns_typed_error_not_todo() {
        let task = InnerGraphTask;
        let ctx = graph_flow::Context::new();
        let result = task.run(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("inner_graph"),
            "error message should mention inner_graph: {err}"
        );
        assert!(
            err.contains("WS3"),
            "error message should mention WS3: {err}"
        );
    }

    #[tokio::test]
    async fn rule_check_true_continues() {
        let task = RuleCheckTask;
        let ctx = graph_flow::Context::new();
        ctx.set("_rule", "always_true").await;
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::Continue));
    }

    #[tokio::test]
    async fn rule_check_false_waits() {
        let task = RuleCheckTask;
        let ctx = graph_flow::Context::new();
        ctx.set("_rule", "always_false").await;
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::WaitForInput));
    }

    #[tokio::test]
    async fn judge_task_rule_only_stub() {
        let task = JudgeTask;
        let ctx = graph_flow::Context::new();
        ctx.set("_judge_rule", "always_true").await;
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::Continue));
    }

    #[tokio::test]
    async fn capability_task_runs_workspace_open() {
        let reg = Arc::new(CapabilityRegistry::with_builtins());
        let task = CapabilityTask { registry: reg };
        let ctx = graph_flow::Context::new();
        ctx.set("_capability_name", "workspace.open").await;
        ctx.set("_capability_input", serde_json::json!({})).await;
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::Continue));
    }

    #[tokio::test]
    async fn capability_task_returns_error_for_missing() {
        let reg = Arc::new(CapabilityRegistry::with_builtins());
        let task = CapabilityTask { registry: reg };
        let ctx = graph_flow::Context::new();
        ctx.set("_capability_name", "nonexistent.capability").await;
        let result = task.run(ctx).await;
        assert!(result.is_err());
    }
}
