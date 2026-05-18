//! `acp.prompt` capability — send a prompt to this creator's active ACP session.
//!
//! Design: `orchestration-engine.md` §5.2.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde_json::{json, Value};

/// The `acp.prompt` capability.
///
/// Input schema: `{ prompt: string, tool_policy: enum, session_id?: string }`
/// Output schema: `{ full_text: string }`
///
/// In WS3 this capability is a thin stub that stores the prompt in context.
/// The full implementation dispatches to the Worker Manager IPC (see T4
/// `AcpPromptTask` for the actual worker call).
pub struct AcpPrompt;

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
                },
                "session_id": { "type": "string", "description": "Optional ACP session ID" }
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

        let _tool_policy = input
            .get("tool_policy")
            .and_then(|v| v.as_str())
            .unwrap_or("auto_grant_read_only");

        // In the full implementation, this would dispatch to the Worker Manager.
        // For the capability layer, we just validate and return a placeholder.
        // The actual worker dispatch happens in AcpPromptTask (tasks/mod.rs).
        Ok(json!({
            "full_text": format!("[acp.prompt stub: {}]", prompt)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_prompt_name() {
        assert_eq!(AcpPrompt.name(), "acp.prompt");
    }

    #[tokio::test]
    async fn acp_prompt_valid_input() {
        let cap = AcpPrompt;
        let input = json!({
            "prompt": "Hello, agent!",
            "tool_policy": "deny_all"
        });
        let result = cap.run(input).await.unwrap();
        assert!(result.get("full_text").is_some());
    }

    #[tokio::test]
    async fn acp_prompt_missing_prompt_errors() {
        let cap = AcpPrompt;
        let input = json!({ "tool_policy": "deny_all" });
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing 'prompt'"), "error: {err}");
    }
}
