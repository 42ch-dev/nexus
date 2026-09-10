//! `judge.llm` capability — evaluate a go/nogo prompt using a judge agent.
//!
//! Design: `orchestration-engine.md` §5.2, DF-33, plan J2.
//!
//! Two execution modes:
//! 1. **With prompt executor**: when `PromptExecutor` is injected through the
//!    registry, builds a judge prompt, executes it with `deny_all` tool
//!    policy through the Host plane, and parses the agent response as GO/NOGO.
//! 2. **Standalone (no executor)**: returns `WorkerUnavailable` — no heuristic
//!    matching on input text.

use crate::capability::{Capability, CapabilityError, PromptExecutor, PromptRequest, ToolPolicy};
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
/// Holds an optional [`PromptExecutor`] for agent calls. When present, sends
/// the evaluation prompt via the Host plane. When absent, returns
/// [`CapabilityError::WorkerUnavailable`] (standalone/test mode).
pub struct JudgeLlm {
    executor: Option<Arc<dyn PromptExecutor>>,
    /// Per-run coordinator cancellation tokens (A1): the executor listens to
    /// the token for the current run concurrently with the Host stream.
    session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
}

impl JudgeLlm {
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
    // prevents cross-creator routing — SEC-V131-01).
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
        // to prevent cross-creator routing (IDOR). See SEC-V131-01.
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

        // Build judge prompt with GO/NOGO framing.
        let judge_prompt = format!(
            "You are a judge. Evaluate the following and respond with GO or NOGO.\n\
             Respond with ONLY 'GO' or 'NOGO' followed by a brief reason.\n\n\
             {prompt_text}"
        );

        let result = executor
            .execute(PromptRequest {
                run_id: session_id.to_string(),
                task_id: "judge.llm".to_string(),
                agent_ref: None,
                prompt: judge_prompt,
                tool_policy: ToolPolicy::DenyAll,
                cancellation,
            })
            .await?;

        let (result, reason) = parse_judge_response(&result.full_text);

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

    /// Shared cancellation map with coordinator tokens registered for the
    /// given run ids (fail-closed contract; production registers at run
    /// admission).
    fn cancels_with(
        ids: &[&str],
    ) -> std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    > {
        let map: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        {
            let mut guard = map
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for id in ids {
                guard.insert(id.to_string(), tokio_util::sync::CancellationToken::new());
            }
        }
        map
    }

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
        let input = json!({ "prompt": "evaluate this", "_session_id": "sess" });
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("worker unavailable"),
            "expected worker unavailable, got: {err}"
        );
    }

    // ── J2: With mock executor — GO response ──────────────────────────

    /// Mock executor that records the `run_id` it was called with,
    /// enabling identity-spoof regression tests.
    struct MockGoExecutor {
        captured_run_id: std::sync::Mutex<String>,
    }

    impl MockGoExecutor {
        fn new() -> Self {
            Self {
                captured_run_id: std::sync::Mutex::new(String::new()),
            }
        }
    }

    #[async_trait]
    impl PromptExecutor for MockGoExecutor {
        async fn execute(
            &self,
            request: PromptRequest,
        ) -> Result<crate::capability::PromptResult, CapabilityError> {
            *self.captured_run_id.lock().expect("capture lock") = request.run_id;
            Ok(crate::capability::PromptResult {
                full_text: "GO — the evaluation passes.".to_string(),
                host_session_id: "host-sess".to_string(),
                operation_id: "op-1".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn judge_llm_with_mock_executor_go() {
        let cap = JudgeLlm::with_prompt_executor(Arc::new(MockGoExecutor::new()))
            .with_session_cancels(cancels_with(&["default"]));
        let input = json!({ "prompt": "Is the task complete?", "_session_id": "default" });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["result"], true);
        assert!(result["reason"].as_str().unwrap().contains("go"));
    }

    // ── J2: With mock executor — NOGO response ────────────────────────

    struct MockNogoExecutor;

    #[async_trait]
    impl PromptExecutor for MockNogoExecutor {
        async fn execute(
            &self,
            _request: PromptRequest,
        ) -> Result<crate::capability::PromptResult, CapabilityError> {
            Ok(crate::capability::PromptResult {
                full_text: "NO — stop and review.".to_string(),
                host_session_id: "host-sess".to_string(),
                operation_id: "op-1".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn judge_llm_with_mock_executor_nogo() {
        let cap = JudgeLlm::with_prompt_executor(Arc::new(MockNogoExecutor))
            .with_session_cancels(cancels_with(&["default"]));
        let input = json!({ "prompt": "Is the task complete?", "_session_id": "default" });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["result"], false);
        assert!(result["reason"].as_str().unwrap().contains("nogo"));
    }

    #[tokio::test]
    async fn judge_llm_missing_prompt_errors() {
        let cap = JudgeLlm::with_prompt_executor(Arc::new(MockNogoExecutor))
            .with_session_cancels(cancels_with(&["default"]));
        let input = json!({});
        let result = cap.run(input).await;
        assert!(result.is_err());
    }

    // ── SEC-V131-01: Identity boundary regression tests ────────────────

    /// Proves that raw `creator_id` / `session_id` from preset args are
    /// NOT forwarded to the executor — only context-injected `_session_id`
    /// is trusted as the run identity. M-002: with no trusted `_session_id`
    /// the capability now fails closed (Forbidden) instead of falling back
    /// to a magic `default` run id.
    #[tokio::test]
    async fn judge_llm_raw_creator_id_ignored_on_spoof_attempt() {
        let executor = Arc::new(MockGoExecutor::new());
        let cap = JudgeLlm::with_prompt_executor(executor.clone())
            .with_session_cancels(cancels_with(&["default", "legit_session"]));
        let input = json!({
            "prompt": "evaluate",
            // Spoof attempt: raw preset args should be ignored
            "creator_id": "spoofed_creator",
            "session_id": "spoofed_session"
        });
        let result = cap.run(input).await;
        assert!(
            result.is_err(),
            "SEC-V131-01/M-002: raw session_id must never be trusted; missing trusted _session_id must refuse"
        );
        // The executor must never have been reached with a spoofed id.
        assert_eq!(
            *executor.captured_run_id.lock().expect("capture lock"),
            "",
            "SEC-V131-01: raw session_id leaked through"
        );
    }

    /// Proves context-injected `_session_id` is used as the run identity.
    #[tokio::test]
    async fn judge_llm_context_injected_identity_trusted() {
        let executor = Arc::new(MockGoExecutor::new());
        let cap = JudgeLlm::with_prompt_executor(executor.clone())
            .with_session_cancels(cancels_with(&["default", "legit_session"]));
        let input = json!({
            "prompt": "evaluate",
            "_creator_id": "legit_creator",
            "_session_id": "legit_session"
        });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["result"], true);
        assert_eq!(
            *executor.captured_run_id.lock().expect("capture lock"),
            "legit_session"
        );
    }

    /// Proves context-injected identity wins even when raw args are present.
    #[tokio::test]
    async fn judge_llm_context_identity_overrides_raw_spoof() {
        let executor = Arc::new(MockGoExecutor::new());
        let cap = JudgeLlm::with_prompt_executor(executor.clone())
            .with_session_cancels(cancels_with(&["default", "legit_session"]));
        let input = json!({
            "prompt": "evaluate",
            "creator_id": "spoofed_creator",
            "session_id": "spoofed_session",
            "_creator_id": "legit_creator",
            "_session_id": "legit_session"
        });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["result"], true);
        assert_eq!(
            *executor.captured_run_id.lock().expect("capture lock"),
            "legit_session",
            "SEC-V131-01: context ID must win over raw spoof"
        );
    }
}
