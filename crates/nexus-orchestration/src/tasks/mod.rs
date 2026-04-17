//! Standard `Task` implementations for the orchestration engine.
//!
//! Design: `.agents/plans/knowledge/orchestration-engine-v1.md` §4.4.

use crate::capability::{CapabilityError, CapabilityRegistry};
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

    async fn run(&self, context: graph_flow::Context) -> Result<TaskResult, graph_flow::GraphError> {
        let name: String = context.get("_capability_name").await.unwrap_or_default();
        let input: Value = context.get("_capability_input").await.unwrap_or(Value::Null);

        let cap = self
            .registry
            .get(&name)
            .ok_or_else(|| {
                graph_flow::GraphError::TaskExecutionFailed(format!(
                    "capability not found: {name}"
                ))
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

    async fn run(&self, context: graph_flow::Context) -> Result<TaskResult, graph_flow::GraphError> {
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

    async fn run(&self, _context: graph_flow::Context) -> Result<TaskResult, graph_flow::GraphError> {
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

    async fn run(&self, _context: graph_flow::Context) -> Result<TaskResult, graph_flow::GraphError> {
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

    async fn run(&self, context: graph_flow::Context) -> Result<TaskResult, graph_flow::GraphError> {
        let rule: String = context.get("_judge_rule").await.unwrap_or_default();
        let _context_data: Value =
            context.get("_judge_context_data").await.unwrap_or(Value::Null);

        // Use a simple stub evaluator for now.
        let (result, reason) = match rule.as_str() {
            "always_true" => (
                true,
                "judge.rule stub: always_true → go".to_string(),
            ),
            "always_false" => (
                false,
                "judge.rule stub: always_false → nogo".to_string(),
            ),
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
