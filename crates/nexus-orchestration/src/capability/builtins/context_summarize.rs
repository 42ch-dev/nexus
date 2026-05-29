//! `context.summarize` capability — LLM-driven `core_context` summarization.
//!
//! Takes the current `core_context` content and optionally a state execution
//! trace, invokes an LLM to produce a condensed summary, and returns the
//! result for the engine to write as a new `core_context_versions` row with
//! `derivation_kind = 'llm_summarize'`.
//!
//! Design: `creator-schedule-and-core-context.md` §11, DF-34, plan J3.
//!
//! Two execution modes:
//! 1. **With worker IPC**: when `WorkerHandleProvider` is injected, builds a
//!    summarization prompt, calls `worker/acp_prompt`, returns summary text
//!    and blake3 prompt hash.
//! 2. **Standalone (no provider)**: returns `WorkerUnavailable` — no
//!    truncation or `SUMMARIZE_STUB` fallback in production path.

use crate::capability::{Capability, CapabilityError, WorkerHandleProvider};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

/// The `context.summarize` capability.
///
/// Holds an optional [`WorkerHandleProvider`] for LLM calls. When present,
/// sends the summarization prompt via worker IPC. When absent, returns
/// [`CapabilityError::WorkerUnavailable`] (standalone/test mode).
pub struct ContextSummarize {
    workers: Option<Arc<dyn WorkerHandleProvider>>,
}

impl ContextSummarize {
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

impl Default for ContextSummarize {
    fn default() -> Self {
        Self::new()
    }
}

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
                },
                "creator_id": {
                    "type": "string",
                    "description": "Optional creator ID"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID"
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

        let workers = self
            .workers
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        let creator_id = input
            .get("creator_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let trace = input.get("trace").and_then(|v| v.as_str()).unwrap_or("");
        let template = input
            .get("template")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Build summarization prompt.
        let prompt = build_summary_prompt(content, trace, template);

        // Compute blake3 hash of the prompt actually sent.
        let prompt_hash = blake3::hash(prompt.as_bytes()).to_hex().to_string();

        // Call worker IPC.
        let response = workers
            .call_acp_prompt(creator_id, session_id, prompt, "deny_all")
            .await?;

        let summary = response
            .get("full_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(json!({
            "summary": summary,
            "prompt_hash": prompt_hash
        }))
    }
}

/// Build the summarization prompt from content, trace, and template.
fn build_summary_prompt(content: &str, trace: &str, template: &str) -> String {
    let instructions = if template.is_empty() {
        "Summarize the following content concisely, preserving key entities, relationships, and state."
    } else {
        template
    };

    let mut prompt = format!("{instructions}\n\n--- Content ---\n{content}");

    if !trace.is_empty() {
        prompt.push_str("\n\n--- Execution Trace ---\n");
        prompt.push_str(trace);
    }

    prompt.push_str("\n\n---\nProvide the summary now:");
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_summarize_name() {
        let cap = ContextSummarize::new();
        assert_eq!(cap.name(), "context.summarize");
    }

    #[test]
    fn context_summarize_input_schema_valid() {
        let cap = ContextSummarize::new();
        let schema: Value =
            serde_json::from_str(cap.input_schema()).expect("valid JSON Schema");
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
        let cap = ContextSummarize::new();
        let schema: Value =
            serde_json::from_str(cap.output_schema()).expect("valid JSON Schema");
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("summary")));
        assert!(required.contains(&json!("prompt_hash")));
    }

    // ── J3: Standalone mode returns WorkerUnavailable ─────────────────

    #[tokio::test]
    async fn context_summarize_standalone_returns_unavailable() {
        let cap = ContextSummarize::new();
        let input = json!({ "content": "Some text" });
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("worker unavailable"),
            "expected worker unavailable, got: {err}"
        );
    }

    // ── J3: With mock provider ────────────────────────────────────────

    struct MockSummaryProvider;

    #[async_trait]
    impl WorkerHandleProvider for MockSummaryProvider {
        async fn call_acp_prompt(
            &self,
            _creator_id: &str,
            _session_id: &str,
            _prompt: String,
            _tool_policy: &str,
        ) -> Result<Value, CapabilityError> {
            Ok(json!({
                "full_text": "A concise summary of the content."
            }))
        }
    }

    #[tokio::test]
    async fn context_summarize_with_mock_worker() {
        let cap = ContextSummarize::with_worker_provider(Arc::new(MockSummaryProvider));
        let input = json!({
            "content": "The story is about a brave knight who saves the kingdom from a dragon."
        });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["summary"], "A concise summary of the content.");
        // Verify prompt_hash is a valid blake3 hex string.
        let hash = result["prompt_hash"].as_str().unwrap();
        assert_eq!(hash.len(), 64);
    }

    #[tokio::test]
    async fn context_summarize_with_trace_and_template() {
        let cap = ContextSummarize::with_worker_provider(Arc::new(MockSummaryProvider));
        let input = json!({
            "content": "Some context",
            "trace": "User entered state X, performed action Y",
            "template": "Summarize focusing on character development."
        });
        let result = cap.run(input).await.unwrap();
        assert!(result.get("summary").is_some());
        assert!(result.get("prompt_hash").is_some());
    }

    #[tokio::test]
    async fn context_summarize_missing_content_errors() {
        let cap = ContextSummarize::with_worker_provider(Arc::new(MockSummaryProvider));
        let input = json!({});
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing 'content' field"));
    }

    // ── J3: Prompt builder unit tests ─────────────────────────────────

    #[test]
    fn build_summary_prompt_basic() {
        let prompt = build_summary_prompt("Hello world", "", "");
        assert!(prompt.contains("Hello world"));
        assert!(prompt.contains("Summarize the following"));
    }

    #[test]
    fn build_summary_prompt_with_trace() {
        let prompt = build_summary_prompt("Content", "trace data", "");
        assert!(prompt.contains("trace data"));
        assert!(prompt.contains("Execution Trace"));
    }

    #[test]
    fn build_summary_prompt_with_template() {
        let prompt = build_summary_prompt("Content", "", "Focus on characters.");
        assert!(prompt.contains("Focus on characters."));
        assert!(!prompt.contains("Summarize the following"));
    }
}
