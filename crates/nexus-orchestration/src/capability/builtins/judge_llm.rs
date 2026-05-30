//! `judge.llm` capability — evaluate a go/nogo prompt using a judge agent.
//!
//! Design: `orchestration-engine.md` §5.2, DF-33, plan J2.
//!
//! Two execution modes:
//! 1. **With worker IPC**: when `WorkerHandleProvider` is injected through the
//!    registry, builds a judge prompt, calls `worker/acp_prompt` with
//!    `deny_all` tool policy, and parses the LLM response as GO/NOGO.
//! 2. **Standalone (no provider)**: returns `WorkerUnavailable` — no heuristic
//!    matching on input text.

use crate::capability::{Capability, CapabilityError, WorkerHandleProvider};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

/// Judge response verdicts that count as "go".
const GO_WORDS: &[&str] = &["go", "yes", "proceed", "pass", "approve", "ok"];

/// Judge response verdicts that count as "no-go".
const NOGO_WORDS: &[&str] = &["wait", "no", "stop", "reject", "deny", "fail"];

/// The `judge.llm` capability.
///
/// Holds an optional [`WorkerHandleProvider`] for LLM calls. When present,
/// sends the evaluation prompt via worker IPC. When absent, returns
/// [`CapabilityError::WorkerUnavailable`] (standalone/test mode).
pub struct JudgeLlm {
    workers: Option<Arc<dyn WorkerHandleProvider>>,
}

impl JudgeLlm {
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

impl Default for JudgeLlm {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for JudgeLlm {
    fn name(&self) -> &'static str {
        "judge.llm"
    }

    fn input_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": { "type": "string", "description": "The evaluation prompt for the judge" },
                "creator_id": { "type": "string", "description": "Optional creator ID for the judge agent" },
                "session_id": { "type": "string", "description": "Optional session ID" }
            }
        }"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["result", "reason"],
            "properties": {
                "result": { "type": "boolean", "description": "true = go, false = nogo" },
                "reason": { "type": "string", "description": "Human-readable explanation" }
            }
        }"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let prompt_text = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CapabilityError::InputInvalid("missing 'prompt' field".into()))?;

        let creator_id = input
            .get("creator_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        let workers = self
            .workers
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        // Build judge prompt with GO/NOGO framing.
        let judge_prompt = format!(
            "You are a judge. Evaluate the following and respond with GO or NOGO.\n\
             Respond with ONLY 'GO' or 'NOGO' followed by a brief reason.\n\n\
             {prompt_text}"
        );

        let response = workers
            .call_acp_prompt(creator_id, session_id, judge_prompt, "deny_all")
            .await?;

        let full_text = response
            .get("full_text")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let (result, reason) = parse_judge_response(full_text);

        Ok(json!({
            "result": result,
            "reason": reason
        }))
    }
}

/// Parse a judge LLM response text into a boolean verdict.
///
/// Returns `(result, reason)` where result is true for GO, false for NOGO
/// or ambiguous.
pub fn parse_judge_response(text: &str) -> (bool, String) {
    let lower = text.trim().to_lowercase();

    for word in GO_WORDS {
        if lower.contains(word) {
            return (
                true,
                format!("judge.llm: go (matched '{word}' in LLM response)"),
            );
        }
    }

    for word in NOGO_WORDS {
        if lower.contains(word) {
            return (
                false,
                format!("judge.llm: nogo (matched '{word}' in LLM response)"),
            );
        }
    }

    (
        false,
        format!(
            "judge.llm: ambiguous LLM response — '{}'",
            &lower[..lower.len().min(50)]
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn judge_llm_name() {
        let cap = JudgeLlm::new();
        assert_eq!(cap.name(), "judge.llm");
    }

    #[test]
    fn parse_go_response() {
        assert!(parse_judge_response("Yes, proceed with the next step").0);
        assert!(parse_judge_response("Go ahead!").0);
        assert!(parse_judge_response("APPROVE").0);
    }

    #[test]
    fn parse_nogo_response() {
        assert!(!parse_judge_response("No, wait for more input").0);
        assert!(!parse_judge_response("Stop here").0);
    }

    #[test]
    fn parse_ambiguous_response() {
        let (result, reason) = parse_judge_response("maybe we should think about it");
        assert!(!result);
        assert!(reason.contains("ambiguous"));
    }

    // ── J2: Standalone mode returns WorkerUnavailable ─────────────────

    #[tokio::test]
    async fn judge_llm_standalone_returns_unavailable() {
        let cap = JudgeLlm::new();
        let input = json!({ "prompt": "evaluate this" });
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("worker unavailable"),
            "expected worker unavailable, got: {err}"
        );
    }

    // ── J2: With mock provider — GO response ──────────────────────────

    /// Mock provider that returns a canned GO response.
    struct MockGoProvider;

    #[async_trait]
    impl WorkerHandleProvider for MockGoProvider {
        async fn call_acp_prompt(
            &self,
            _creator_id: &str,
            _session_id: &str,
            _prompt: String,
            _tool_policy: &str,
        ) -> Result<Value, CapabilityError> {
            Ok(json!({ "full_text": "GO — the evaluation passes." }))
        }
    }

    #[tokio::test]
    async fn judge_llm_with_mock_worker_go() {
        let cap = JudgeLlm::with_worker_provider(Arc::new(MockGoProvider));
        let input = json!({ "prompt": "Is the task complete?" });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["result"], true);
        assert!(result["reason"].as_str().unwrap().contains("go"));
    }

    // ── J2: With mock provider — NOGO response ────────────────────────

    struct MockNogoProvider;

    #[async_trait]
    impl WorkerHandleProvider for MockNogoProvider {
        async fn call_acp_prompt(
            &self,
            _creator_id: &str,
            _session_id: &str,
            _prompt: String,
            _tool_policy: &str,
        ) -> Result<Value, CapabilityError> {
            Ok(json!({ "full_text": "NO — stop and review." }))
        }
    }

    #[tokio::test]
    async fn judge_llm_with_mock_worker_nogo() {
        let cap = JudgeLlm::with_worker_provider(Arc::new(MockNogoProvider));
        let input = json!({ "prompt": "Is the task complete?" });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["result"], false);
        assert!(result["reason"].as_str().unwrap().contains("nogo"));
    }

    #[tokio::test]
    async fn judge_llm_missing_prompt_errors() {
        let cap = JudgeLlm::with_worker_provider(Arc::new(MockGoProvider));
        let input = json!({});
        let result = cap.run(input).await;
        assert!(result.is_err());
    }
}
