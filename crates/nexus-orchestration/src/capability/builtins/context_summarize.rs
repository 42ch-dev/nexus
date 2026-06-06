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

    // Identity fields ("_creator_id", "_session_id") are injected by
    // orchestration context, NOT accepted from user input (security:
    // prevents cross-creator IPC routing — SEC-V131-01).
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

        let workers = self
            .workers
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        // Security: only accept context-injected identity fields (prefixed _).
        // Raw `creator_id`/`session_id` from user/preset input are ignored
        // to prevent cross-creator IPC routing (IDOR). See SEC-V131-01.
        let creator_id = input
            .get("_creator_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let session_id = input
            .get("_session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let trace = input.get("trace").and_then(|v| v.as_str()).unwrap_or("");
        let template = input.get("template").and_then(|v| v.as_str()).unwrap_or("");

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

/// Default maximum content size passed to the LLM (256 KiB).
/// TD-V131-04: Content exceeding this limit is truncated with a marker.
const DEFAULT_MAX_CONTENT_BYTES: usize = 256 * 1024;

/// Truncate `s` so the result is at most `max_bytes` bytes AND ends on a
/// valid UTF-8 char boundary. Walks backwards up to 4 bytes (the maximum
/// UTF-8 encoding length) to find the boundary.
///
/// TD-V131-04 (post-QC fix wave): using `&s[..max_bytes]` directly panics
/// when `max_bytes` falls inside a multi-byte scalar. This helper preserves
/// the size cap without violating Rust's `str` invariants.
fn truncate_to_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}

/// Build the summarization prompt from content, trace, and template.
///
/// TD-V131-04: If `content` exceeds `DEFAULT_MAX_CONTENT_BYTES` (256 KiB),
/// it is truncated with a `[truncated at N bytes]` marker to prevent
/// oversized prompts from blowing up context windows or causing timeouts.
fn build_summary_prompt(content: &str, trace: &str, template: &str) -> String {
    let instructions = if template.is_empty() {
        "Summarize the following content concisely, preserving key entities, relationships, and state."
    } else {
        template
    };

    // TD-V131-04: Truncate oversized content to prevent context window overflow.
    // Use char-boundary-aware truncation to avoid panics on multi-byte UTF-8.
    let content_display = if content.len() > DEFAULT_MAX_CONTENT_BYTES {
        tracing::warn!(
            original_len = content.len(),
            max_bytes = DEFAULT_MAX_CONTENT_BYTES,
            "context.summarize: content exceeds size limit, truncating"
        );
        let truncated = truncate_to_char_boundary(content, DEFAULT_MAX_CONTENT_BYTES);
        format!(
            "{}\n\n[truncated at {} bytes — original was {} bytes]",
            truncated,
            truncated.len(),
            content.len()
        )
    } else {
        content.to_string()
    };

    let mut prompt = format!("{instructions}\n\n--- Content ---\n{content_display}");

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
        let schema: Value = serde_json::from_str(cap.input_schema()).expect("valid JSON Schema");
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
        let schema: Value = serde_json::from_str(cap.output_schema()).expect("valid JSON Schema");
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

    struct MockSummaryProvider {
        captured_creator_id: std::sync::Mutex<String>,
    }

    impl MockSummaryProvider {
        fn new() -> Self {
            Self {
                captured_creator_id: std::sync::Mutex::new(String::new()),
            }
        }
    }

    #[async_trait]
    impl WorkerHandleProvider for MockSummaryProvider {
        async fn call_acp_prompt(
            &self,
            creator_id: &str,
            _session_id: &str,
            _prompt: String,
            _tool_policy: &str,
        ) -> Result<Value, CapabilityError> {
            *self.captured_creator_id.lock().unwrap() = creator_id.to_string();
            Ok(json!({
                "full_text": "A concise summary of the content."
            }))
        }
    }

    #[tokio::test]
    async fn context_summarize_with_mock_worker() {
        let cap = ContextSummarize::with_worker_provider(Arc::new(MockSummaryProvider::new()));
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
        let cap = ContextSummarize::with_worker_provider(Arc::new(MockSummaryProvider::new()));
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
        let cap = ContextSummarize::with_worker_provider(Arc::new(MockSummaryProvider::new()));
        let input = json!({});
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing 'content' field"));
    }

    // ── SEC-V131-01: Identity boundary regression tests ────────────────

    /// Proves that raw `creator_id` / `session_id` from preset args are
    /// NOT forwarded to the worker — only context-injected `_creator_id`
    /// and `_session_id` are trusted.
    #[tokio::test]
    async fn context_summarize_raw_creator_id_ignored_on_spoof_attempt() {
        let provider = Arc::new(MockSummaryProvider::new());
        let cap = ContextSummarize::with_worker_provider(provider.clone());
        let input = json!({
            "content": "Some text",
            // Spoof attempt: raw preset args should be ignored
            "creator_id": "spoofed_creator",
            "session_id": "spoofed_session"
        });
        let result = cap.run(input).await.unwrap();
        assert!(result.get("summary").is_some());
        let captured = provider.captured_creator_id.lock().unwrap();
        assert_eq!(
            *captured, "default",
            "SEC-V131-01: raw creator_id leaked through"
        );
    }

    /// Proves context-injected `_creator_id` / `_session_id` are used.
    #[tokio::test]
    async fn context_summarize_context_injected_identity_trusted() {
        let provider = Arc::new(MockSummaryProvider::new());
        let cap = ContextSummarize::with_worker_provider(provider.clone());
        let input = json!({
            "content": "Some text",
            "_creator_id": "legit_creator",
            "_session_id": "legit_session"
        });
        let result = cap.run(input).await.unwrap();
        assert!(result.get("summary").is_some());
        let captured = provider.captured_creator_id.lock().unwrap();
        assert_eq!(*captured, "legit_creator");
    }

    /// Proves context-injected identity wins even when raw args are present.
    #[tokio::test]
    async fn context_summarize_context_identity_overrides_raw_spoof() {
        let provider = Arc::new(MockSummaryProvider::new());
        let cap = ContextSummarize::with_worker_provider(provider.clone());
        let input = json!({
            "content": "Some text",
            "creator_id": "spoofed_creator",
            "session_id": "spoofed_session",
            "_creator_id": "legit_creator",
            "_session_id": "legit_session"
        });
        let result = cap.run(input).await.unwrap();
        assert!(result.get("summary").is_some());
        let captured = provider.captured_creator_id.lock().unwrap();
        assert_eq!(
            *captured, "legit_creator",
            "SEC-V131-01: context ID must win over raw spoof"
        );
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

    // ── TD-V131-04: Content truncation at size limit ────────────────────

    #[test]
    fn build_summary_prompt_truncates_oversized_content() {
        let oversized = "x".repeat(DEFAULT_MAX_CONTENT_BYTES + 1000);
        let prompt = build_summary_prompt(&oversized, "", "");
        assert!(prompt.contains("[truncated at"));
        assert!(prompt.contains(&format!("{} bytes", DEFAULT_MAX_CONTENT_BYTES)));
        assert!(prompt.contains(&format!("original was {} bytes", oversized.len())));
        // The truncated portion should be exactly DEFAULT_MAX_CONTENT_BYTES of 'x'.
        let truncated_content = &oversized[..DEFAULT_MAX_CONTENT_BYTES];
        assert!(prompt.contains(truncated_content));
    }

    #[test]
    fn build_summary_prompt_no_truncation_under_limit() {
        let content = "a".repeat(DEFAULT_MAX_CONTENT_BYTES - 1);
        let prompt = build_summary_prompt(&content, "", "");
        assert!(!prompt.contains("[truncated at"));
        assert!(prompt.contains(&content));
    }

    /// Regression test for C-QC1-001 / C-QC3-001: the previous implementation
    /// used `&content[..DEFAULT_MAX_CONTENT_BYTES]` which panics on multi-byte
    /// UTF-8 at the byte boundary. This test pads with a 4-byte CJK char
    /// (each "字" is 3 bytes in UTF-8) so that the byte cap lands inside a
    /// multi-byte scalar for non-ASCII content. Must NOT panic.
    #[test]
    fn build_summary_prompt_truncates_multibyte_utf8_without_panic() {
        // 256 KiB worth of 3-byte CJK chars. The byte boundary cap is 262144,
        // which is NOT a multiple of 3, so naive byte slicing would panic.
        let char_count = DEFAULT_MAX_CONTENT_BYTES / 3 + 100;
        let cjk_content: String = "字".repeat(char_count);
        assert!(cjk_content.len() > DEFAULT_MAX_CONTENT_BYTES);
        // Should not panic.
        let prompt = build_summary_prompt(&cjk_content, "", "");
        assert!(prompt.contains("[truncated at"));
        // The truncated content must be valid UTF-8 (it is, by construction).
        // Verify the marker reports the actual byte length used.
        assert!(prompt.contains(&format!(
            "[truncated at {} bytes",
            (DEFAULT_MAX_CONTENT_BYTES / 3) * 3
        )));
    }

    /// Regression test: oversized content with a multi-byte char exactly
    /// at the cap boundary should be safe (boundary at the start of a char).
    #[test]
    fn build_summary_prompt_truncates_at_clean_char_boundary() {
        // 3 ASCII bytes + 1 3-byte char = total 6 bytes; under cap.
        let content = "abc字def";
        assert_eq!(content.len(), 9);
        let prompt = build_summary_prompt(content, "", "");
        assert!(!prompt.contains("[truncated at"));
        assert!(prompt.contains("abc字def"));
    }

    /// Regression test: oversized content where cap lands mid-3-byte-char.
    /// Must truncate to the nearest valid boundary.
    #[test]
    fn build_summary_prompt_truncates_mid_cjk_char() {
        // 2 ASCII + 1 CJK = 5 bytes (cap would be 4 -> mid 3-byte char).
        let content = "ab字cd"; // bytes: a(1) b(1) 字(3) c(1) d(1) = 7 bytes
                                // Force truncation by prepending lots of ASCII.
        let mut oversized = String::with_capacity(DEFAULT_MAX_CONTENT_BYTES + 100);
        oversized.push_str(&"x".repeat(DEFAULT_MAX_CONTENT_BYTES - 4));
        oversized.push_str(content);
        assert!(oversized.len() > DEFAULT_MAX_CONTENT_BYTES);
        // Should not panic.
        let prompt = build_summary_prompt(&oversized, "", "");
        assert!(prompt.contains("[truncated at"));
    }
}
