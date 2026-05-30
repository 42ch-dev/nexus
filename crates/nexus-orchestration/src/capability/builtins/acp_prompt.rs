//! `acp.prompt` capability — send a prompt to this creator's active ACP session.
//!
//! Design: `orchestration-engine.md` §5.2, DF-37, plan J4.
//!
//! Two execution modes:
//! 1. **With worker IPC**: when `WorkerHandleProvider` is injected, dispatches
//!    `worker/acp_prompt` via IPC and returns the LLM response.
//! 2. **Standalone (no provider)**: returns a structured placeholder indicating
//!    the prompt was prepared but no worker was available. Used in explicit
//!    test/standalone mode.

use crate::capability::{Capability, CapabilityError, WorkerHandleProvider};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

/// The `acp.prompt` capability.
///
/// Input schema: `{ prompt: string, tool_policy: enum }`
///
/// Identity fields (`_creator_id`, `_session_id`) are **context-injected** by
/// the orchestration task execution layer — not accepted from raw user input,
/// to prevent cross-creator IPC routing.
/// Output schema: `{ full_text: string }`
///
/// When `WorkerHandleProvider` is present, dispatches the prompt via worker
/// IPC. Otherwise returns a structured placeholder (standalone/test mode).
pub struct AcpPrompt {
    workers: Option<Arc<dyn WorkerHandleProvider>>,
}

impl AcpPrompt {
    /// Create in standalone/test mode (no worker IPC).
    #[must_use]
    pub const fn new() -> Self {
        Self { workers: None }
    }

    /// Create with a worker handle provider for production IPC.
    #[must_use]
    pub fn with_worker_provider(provider: Arc<dyn WorkerHandleProvider>) -> Self {
        Self {
            workers: Some(provider),
        }
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
                // NOT accepted from user input (security: prevents cross-creator IPC).
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
            .unwrap_or("auto_grant_read_only");

        // Security: only accept context-injected identity fields (prefixed _).
        // Raw `creator_id`/`session_id` from user/preset input are ignored
        // to prevent cross-creator IPC routing (IDOR).
        let session_id = input
            .get("_session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let creator_id = input
            .get("_creator_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        // Production path: use worker IPC when provider is present.
        if let Some(ref workers) = self.workers {
            let response = workers
                .call_acp_prompt(creator_id, session_id, prompt.to_string(), tool_policy)
                .await?;

            let full_text = response
                .get("full_text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            return Ok(json!({
                "full_text": full_text,
                "dispatched_via_ipc": true
            }));
        }

        // Standalone/test mode: return structured placeholder.
        // DF-37: stub output only in explicit standalone/test mode.
        Ok(json!({
            "full_text": format!("[acp.prompt prepared: tool_policy={tool_policy}, session_id={session_id}, prompt_len={}]", prompt.len()),
            "prompt": prompt,
            "tool_policy": tool_policy,
            "session_id": session_id,
            "dispatched_via_ipc": false
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

    // ── Standalone mode (no worker) ───────────────────────────────────

    #[tokio::test]
    async fn acp_prompt_standalone_returns_prepared_output() {
        let cap = AcpPrompt::new();
        let input = json!({
            "prompt": "Hello, agent!",
            "tool_policy": "deny_all"
        });
        let result = cap.run(input).await.unwrap();
        assert!(result.get("full_text").is_some());
        assert_eq!(result["dispatched_via_ipc"], false);
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

    // ── J4: With mock worker provider ─────────────────────────────────

    struct MockAcpProvider;

    #[async_trait]
    impl WorkerHandleProvider for MockAcpProvider {
        async fn call_acp_prompt(
            &self,
            _creator_id: &str,
            _session_id: &str,
            prompt: String,
            _tool_policy: &str,
        ) -> Result<Value, CapabilityError> {
            Ok(json!({
                "full_text": format!("Response to: {prompt}")
            }))
        }
    }

    #[tokio::test]
    async fn acp_prompt_with_worker_uses_ipc() {
        let cap = AcpPrompt::with_worker_provider(Arc::new(MockAcpProvider));
        let input = json!({
            "prompt": "Extract character info",
            "tool_policy": "auto_grant_read_only",
            "_session_id": "sess_123",
            "_creator_id": "creator_abc"
        });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["dispatched_via_ipc"], true);
        let full_text = result["full_text"].as_str().unwrap();
        assert!(
            full_text.contains("Extract character info"),
            "full_text: {full_text}"
        );
    }

    #[tokio::test]
    async fn acp_prompt_with_worker_returns_structured_output() {
        let cap = AcpPrompt::with_worker_provider(Arc::new(MockAcpProvider));
        let input = json!({
            "prompt": "test prompt",
            "tool_policy": "deny_all",
            "_session_id": "sess_456",
            "_creator_id": "creator_def"
        });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["dispatched_via_ipc"], true);
        assert!(result["full_text"]
            .as_str()
            .unwrap()
            .contains("test prompt"));
    }
}
