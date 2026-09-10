//! `acp.prompt` capability — send a prompt to this creator's active ACP session.
//!
//! Design: `orchestration-engine.md` §5.2, DF-37, plan J4.
//!
//! Two execution modes:
//! 1. **With prompt executor**: when `PromptExecutor` is injected, dispatches
//!    the prompt through the daemon Host plane and returns the agent output.
//! 2. **Standalone (no executor)**: returns `WorkerUnavailable` — no echo or
//!    placeholder success in production.

use crate::capability::{Capability, CapabilityError, PromptExecutor, PromptRequest, ToolPolicy};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

/// The `acp.prompt` capability.
///
/// Input schema: `{ prompt: string, tool_policy: enum }`
///
/// Identity fields (`_creator_id`, `_session_id`) are **context-injected** by
/// the orchestration task execution layer — not accepted from raw user input,
/// to prevent cross-creator routing.
/// Output schema: `{ full_text: string }`
///
/// When `PromptExecutor` is present, dispatches the prompt via the Host
/// plane. Otherwise returns `WorkerUnavailable` (standalone/test mode).
pub struct AcpPrompt {
    executor: Option<Arc<dyn PromptExecutor>>,
    /// Per-run coordinator cancellation tokens (A1): the executor listens to
    /// the token for the current run concurrently with the Host stream.
    session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
}

impl AcpPrompt {
    /// Create in standalone/test mode (no prompt executor).
    #[must_use]
    pub fn new() -> Self {
        Self {
            executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    /// Create with a prompt executor for production Host dispatch.
    #[must_use]
    pub fn with_prompt_executor(executor: Arc<dyn PromptExecutor>) -> Self {
        Self {
            executor: Some(executor),
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
        }
    }

    /// Builder-style per-run coordinator cancellation tokens (A1).
    #[must_use]
    pub fn with_session_cancels(
        mut self,
        session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        >,
    ) -> Self {
        self.session_cancels = session_cancels;
        self
    }
}

impl Default for AcpPrompt {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for AcpPrompt {
    fn name(&self) -> &'static str {
        "acp.prompt"
    }

    fn input_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": { "type": "string", "description": "The prompt text to send to the ACP agent" },
                "tool_policy": {
                    "type": "string",
                    "enum": ["auto_grant_all", "auto_grant_read_only", "deny_all", "request_policy"],
                    "default": "auto_grant_read_only",
                    "description": "Tool permission policy for this prompt"
                }
                // "_creator_id" and "_session_id" are injected by orchestration context,
                // NOT accepted from user input (security: prevents cross-creator routing).
            }
        }"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["full_text"],
            "properties": {
                "full_text": { "type": "string", "description": "The full response text from the ACP agent" }
            }
        }"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CapabilityError::InputInvalid("missing 'prompt' field".into()))?;

        let tool_policy = input
            .get("tool_policy")
            .and_then(|v| v.as_str())
            .map_or_else(
                || Ok(ToolPolicy::AutoGrantReadOnly),
                |s| std::str::FromStr::from_str(s).map_err(CapabilityError::InputInvalid),
            )?;

        // Security: only accept context-injected identity fields (prefixed _).
        // Raw `creator_id`/`session_id` from user/preset input are ignored
        // to prevent cross-creator routing (IDOR).
        //
        // M-002: a missing trusted `_session_id` refuses with a typed error
        // — never a magic `default` run id. The orchestration engine seeds
        // the trusted `_session_id` at run admission; its absence means the
        // capability is being invoked outside a trusted run context.
        let session_id = input
            .get("_session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CapabilityError::Forbidden(
                    "missing trusted _session_id: orchestration context must inject the run identity"
                        .to_string(),
                )
            })?;

        let executor = self
            .executor
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        // A1: resolve the coordinator cancellation token for this run from
        // the shared per-run map. FAIL-CLOSED: a run with no registered
        // token refuses with `CancellationUnavailable` — a fresh token would
        // be uncancellable by any coordinator (never mint one here). The run
        // admission path (engine start/spawn/recovery) registers the token.
        let cancellation =
            crate::capability::resolve_session_cancellation(&self.session_cancels, session_id)?;

        let result = executor
            .execute(PromptRequest {
                run_id: session_id.to_string(),
                task_id: "acp.prompt".to_string(),
                agent_ref: None,
                prompt: prompt.to_string(),
                tool_policy,
                cancellation,
            })
            .await?;

        Ok(json!({
            "full_text": result.full_text,
            "host_session_id": result.host_session_id,
            "operation_id": result.operation_id,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_prompt_name() {
        let cap = AcpPrompt::new();
        assert_eq!(cap.name(), "acp.prompt");
    }

    // ── Standalone mode (no executor) ─────────────────────────────────

    #[tokio::test]
    async fn acp_prompt_standalone_returns_unavailable() {
        let cap = AcpPrompt::new();
        let input = json!({
            "prompt": "Hello, agent!",
            "tool_policy": "deny_all",
            "_session_id": "sess"
        });
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("worker unavailable"),
            "standalone acp.prompt must refuse, got: {err}"
        );
    }

    #[tokio::test]
    async fn acp_prompt_missing_prompt_errors() {
        let cap = AcpPrompt::new();
        let input = json!({ "tool_policy": "deny_all" });
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing 'prompt'"), "error: {err}");
    }

    // ── With mock prompt executor ─────────────────────────────────────

    struct MockAcpExecutor {
        captured: std::sync::Mutex<Option<PromptRequest>>,
    }

    #[async_trait]
    impl PromptExecutor for MockAcpExecutor {
        async fn execute(
            &self,
            request: PromptRequest,
        ) -> Result<crate::capability::PromptResult, CapabilityError> {
            *self.captured.lock().expect("capture lock") = Some(request);
            Ok(crate::capability::PromptResult {
                full_text: "transformed:hello".to_string(),
                host_session_id: "host-sess".to_string(),
                operation_id: "op-1".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn acp_prompt_with_executor_returns_agent_output() {
        let executor = Arc::new(MockAcpExecutor {
            captured: std::sync::Mutex::new(None),
        });
        // Seed the run's coordinator token (fail-closed contract: unregistered
        // runs refuse CancellationUnavailable; production registers at admission).
        let cancels = Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        cancels
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                "sess".to_string(),
                tokio_util::sync::CancellationToken::new(),
            );
        let cap = AcpPrompt::with_prompt_executor(executor.clone()).with_session_cancels(cancels);
        let input = json!({
            "prompt": "hello",
            "tool_policy": "deny_all",
            "_creator_id": "ctr",
            "_session_id": "sess"
        });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["full_text"], "transformed:hello");
        assert_eq!(result["host_session_id"], "host-sess");
        let captured = executor
            .captured
            .lock()
            .expect("capture lock")
            .clone()
            .unwrap();
        assert_eq!(captured.run_id, "sess");
        assert_eq!(captured.tool_policy, ToolPolicy::DenyAll);
    }
}
