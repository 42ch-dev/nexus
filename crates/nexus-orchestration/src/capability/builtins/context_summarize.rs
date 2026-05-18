//! `context.summarize` capability — LLM-driven `core_context` summarization.
//!
//! Takes the current `core_context` content and optionally a state execution
//! trace, invokes an LLM to produce a condensed summary, and returns the
//! result for the engine to write as a new `core_context_versions` row with
//! `derivation_kind = 'llm_summarize'`.
//!
//! Design: `creator-schedule-and-core-context.md` §11.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde_json::{json, Value};

/// The `context.summarize` capability.
///
/// Input schema: `{ content: string, trace?: string, template?: string }`
/// Output schema: `{ summary: string, prompt_hash: string }`
pub struct ContextSummarize;

#[async_trait]
impl Capability for ContextSummarize {
    fn name(&self) -> &'static str {
        "context.summarize"
    }

    fn input_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["content"],
            "properties": {
                "content": {
                    "type": "string",
                    "description": "Current core_context text to summarize"
                },
                "trace": {
                    "type": "string",
                    "description": "Optional state execution trace for context"
                },
                "template": {
                    "type": "string",
                    "description": "Optional summarization template/instructions"
                }
            }
        }"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["summary", "prompt_hash"],
            "properties": {
                "summary": {
                    "type": "string",
                    "description": "LLM-generated summary of the core_context"
                },
                "prompt_hash": {
                    "type": "string",
                    "description": "blake3 hash of the prompt sent to the LLM"
                }
            }
        }"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CapabilityError::InputInvalid("missing 'content' field".into()))?;

        let _trace = input.get("trace").and_then(|v| v.as_str()).unwrap_or("");

        let _template = input.get("template").and_then(|v| v.as_str()).unwrap_or("");

        // In the full implementation, this would invoke an LLM via acp.prompt.
        // For the V1.5 capability stub, we use a simple heuristic: if the
        // content contains a special marker "[SUMMARIZE_STUB]", we return
        // a canned summary for testing. Otherwise, we return a truncated
        // version of the content as a naive summary.
        //
        // This allows tests to drive the capability deterministically.
        let (summary, prompt_hash) = if content.contains("[SUMMARIZE_STUB]") {
            // Test hook: return canned response with known hash
            let canned = "[LLM SUMMARY] This is a test summary from the mock LLM.";
            let hash = blake3::hash(b"test-prompt").to_hex().to_string();
            (canned.to_string(), hash)
        } else {
            // Naive fallback: return content as-is (pre-1.0 acceptable)
            // In production this would be replaced by actual LLM invocation.
            let prompt_text = content;
            let hash = blake3::hash(prompt_text.as_bytes()).to_hex().to_string();
            // Truncate to a reasonable summary length if needed
            let summary = if content.len() > 500 {
                format!("{}...", &content[..500])
            } else {
                content.to_string()
            };
            (summary, hash)
        };

        Ok(json!({
            "summary": summary,
            "prompt_hash": prompt_hash
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_summarize_name() {
        assert_eq!(ContextSummarize.name(), "context.summarize");
    }

    #[test]
    fn context_summarize_input_schema_valid() {
        let schema: Value =
            serde_json::from_str(ContextSummarize.input_schema()).expect("valid JSON Schema");
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(schema["type"], "object");
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("content")));
    }

    #[test]
    fn context_summarize_output_schema_valid() {
        let schema: Value =
            serde_json::from_str(ContextSummarize.output_schema()).expect("valid JSON Schema");
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("summary")));
        assert!(required.contains(&json!("prompt_hash")));
    }

    #[tokio::test]
    async fn context_summarize_valid_input_returns_summary() {
        let cap = ContextSummarize;
        let input = json!({
            "content": "The story is about a brave knight who saves the kingdom from a dragon."
        });
        let result = cap.run(input).await.unwrap();
        assert!(result.get("summary").is_some());
        assert!(result.get("prompt_hash").is_some());
        let summary = result["summary"].as_str().unwrap();
        assert!(summary.contains("knight"));
    }

    #[tokio::test]
    async fn context_summarize_stub_marker_returns_canned_response() {
        let cap = ContextSummarize;
        let input = json!({
            "content": "[SUMMARIZE_STUB] test content"
        });
        let result = cap.run(input).await.unwrap();
        assert_eq!(
            result["summary"],
            "[LLM SUMMARY] This is a test summary from the mock LLM."
        );
        // Verify prompt_hash is a valid hex string
        let hash = result["prompt_hash"].as_str().unwrap();
        assert_eq!(hash.len(), 64); // blake3 hex = 64 chars
    }

    #[tokio::test]
    async fn context_summarize_missing_content_errors() {
        let cap = ContextSummarize;
        let input = json!({});
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing 'content' field"));
    }

    #[tokio::test]
    async fn context_summarize_with_optional_trace() {
        let cap = ContextSummarize;
        let input = json!({
            "content": "Some context",
            "trace": "User entered state X, performed action Y"
        });
        let result = cap.run(input).await.unwrap();
        assert!(result.get("summary").is_some());
    }

    #[tokio::test]
    async fn context_summarize_long_content_truncated() {
        let cap = ContextSummarize;
        let long_content = "A".repeat(1000);
        let input = json!({ "content": long_content });
        let result = cap.run(input).await.unwrap();
        let summary = result["summary"].as_str().unwrap();
        assert!(summary.ends_with("..."));
        assert!(summary.len() < 1000);
    }
}
