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

/// Judge response verdicts that count as "go" (first-token match only).
const GO_WORDS: &[&str] = &["go", "continue", "yes", "proceed", "pass", "approve", "ok"];

/// Judge response verdicts that count as "no-go" (first-token match only).
const NOGO_WORDS: &[&str] = &[
    "nogo", "stop", "no", "revise", "wait", "reject", "deny", "fail",
];

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

    // Identity fields ("_creator_id", "_session_id") are injected by
    // orchestration context, NOT accepted from user input (security:
    // prevents cross-creator IPC routing — SEC-V131-01).
    fn input_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": { "type": "string", "description": "The evaluation prompt for the judge" }
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
/// Uses **first-token matching**: extracts the first whitespace/punctuation-
/// delimited token from the trimmed, lowercased response and checks it
/// against explicit GO and NOGO word sets. NOGO is checked first to avoid
/// false positives (e.g. "nogo" containing "go").
///
/// Returns `(result, reason)` where result is true for GO, false for NOGO
/// or ambiguous. Ambiguous responses default to NOGO (safe default) with a
/// warning log.
pub fn parse_judge_response(text: &str) -> (bool, String) {
    let lower = text.trim().to_lowercase();

    // Extract the first token (delimited by whitespace or punctuation).
    let first_token = lower
        .split(|c: char| {
            c.is_whitespace() || c == '.' || c == '!' || c == ',' || c == ';' || c == ':'
        })
        .next()
        .unwrap_or("")
        .trim();

    // Check NOGO first to avoid "nogo" matching "go".
    for word in NOGO_WORDS {
        if first_token == *word {
            return (
                false,
                format!("judge.llm: nogo (first-token matched '{word}')"),
            );
        }
    }

    for word in GO_WORDS {
        if first_token == *word {
            return (
                true,
                format!("judge.llm: go (first-token matched '{word}')"),
            );
        }
    }

    tracing::warn!(
        first_token = %first_token,
        raw_response = %&lower[..lower.len().min(80)],
        "judge.llm: ambiguous first token — defaulting to NOGO"
    );

    (
        false,
        format!(
            "judge.llm: ambiguous LLM response (first token: '{}') — defaulting to NOGO",
            &first_token[..first_token.len().min(30)]
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
        assert!(parse_judge_response("go").0);
        assert!(parse_judge_response("Continue with the plan").0);
    }

    #[test]
    fn parse_nogo_response() {
        assert!(!parse_judge_response("No, wait for more input").0);
        assert!(!parse_judge_response("Stop here").0);
        // R-V133P3-01: "nogo" must NOT match "go" — first-token parse.
        assert!(!parse_judge_response("NOGO because of missing data").0);
        assert!(!parse_judge_response("nogo").0);
        assert!(!parse_judge_response("I think we should NOGO this.").0);
        assert!(!parse_judge_response("Revise the draft first").0);
    }

    #[test]
    fn parse_ambiguous_response() {
        let (result, reason) = parse_judge_response("maybe we should think about it");
        assert!(!result);
        assert!(reason.contains("ambiguous"));
    }

    #[test]
    fn parse_go_with_reason() {
        let (result, reason) = parse_judge_response("Go — the evaluation passes all checks");
        assert!(result);
        assert!(reason.contains("go"));
    }

    #[test]
    fn parse_nogo_with_reason() {
        let (result, reason) = parse_judge_response("NOGO — insufficient evidence");
        assert!(!result);
        assert!(reason.contains("nogo"));
    }

    #[test]
    fn parse_bare_go() {
        assert!(parse_judge_response("go").0);
    }

    #[test]
    fn parse_bare_nogo() {
        assert!(!parse_judge_response("nogo").0);
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

    /// Mock provider that records the creator_id it was called with,
    /// enabling identity-spoof regression tests.
    struct MockGoProvider {
        captured_creator_id: std::sync::Mutex<String>,
    }

    impl MockGoProvider {
        fn new() -> Self {
            Self {
                captured_creator_id: std::sync::Mutex::new(String::new()),
            }
        }
    }

    #[async_trait]
    impl WorkerHandleProvider for MockGoProvider {
        async fn call_acp_prompt(
            &self,
            creator_id: &str,
            _session_id: &str,
            _prompt: String,
            _tool_policy: &str,
        ) -> Result<Value, CapabilityError> {
            *self.captured_creator_id.lock().unwrap() = creator_id.to_string();
            Ok(json!({ "full_text": "GO — the evaluation passes." }))
        }
    }

    #[tokio::test]
    async fn judge_llm_with_mock_worker_go() {
        let cap = JudgeLlm::with_worker_provider(Arc::new(MockGoProvider::new()));
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
        let cap = JudgeLlm::with_worker_provider(Arc::new(MockNogoProvider));
        let input = json!({});
        let result = cap.run(input).await;
        assert!(result.is_err());
    }

    // ── SEC-V131-01: Identity boundary regression tests ────────────────

    /// Proves that raw `creator_id` / `session_id` from preset args are
    /// NOT forwarded to the worker — only context-injected `_creator_id`
    /// and `_session_id` are trusted.
    #[tokio::test]
    async fn judge_llm_raw_creator_id_ignored_on_spoof_attempt() {
        let provider = Arc::new(MockGoProvider::new());
        let cap = JudgeLlm::with_worker_provider(provider.clone());
        let input = json!({
            "prompt": "evaluate",
            // Spoof attempt: raw preset args should be ignored
            "creator_id": "spoofed_creator",
            "session_id": "spoofed_session"
        });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["result"], true);
        // The worker must have received "default", NOT "spoofed_creator"
        let captured = provider.captured_creator_id.lock().unwrap();
        assert_eq!(
            *captured, "default",
            "SEC-V131-01: raw creator_id leaked through"
        );
    }

    /// Proves context-injected `_creator_id` / `_session_id` are used.
    #[tokio::test]
    async fn judge_llm_context_injected_identity_trusted() {
        let provider = Arc::new(MockGoProvider::new());
        let cap = JudgeLlm::with_worker_provider(provider.clone());
        let input = json!({
            "prompt": "evaluate",
            "_creator_id": "legit_creator",
            "_session_id": "legit_session"
        });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["result"], true);
        let captured = provider.captured_creator_id.lock().unwrap();
        assert_eq!(*captured, "legit_creator");
    }

    /// Proves context-injected identity wins even when raw args are present.
    #[tokio::test]
    async fn judge_llm_context_identity_overrides_raw_spoof() {
        let provider = Arc::new(MockGoProvider::new());
        let cap = JudgeLlm::with_worker_provider(provider.clone());
        let input = json!({
            "prompt": "evaluate",
            "creator_id": "spoofed_creator",
            "session_id": "spoofed_session",
            "_creator_id": "legit_creator",
            "_session_id": "legit_session"
        });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["result"], true);
        let captured = provider.captured_creator_id.lock().unwrap();
        assert_eq!(
            *captured, "legit_creator",
            "SEC-V131-01: context ID must win over raw spoof"
        );
    }
}
