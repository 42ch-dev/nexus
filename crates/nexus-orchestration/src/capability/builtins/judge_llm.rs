//! `judge.llm` capability — evaluate a go/nogo prompt using a judge agent.
//!
//! Design: `orchestration-engine.md` §5.2.
//!
//! Implementation: delegates to `acp.prompt` with `tool_policy: deny_all`,
//! then parses the response as a boolean.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde_json::{json, Value};

/// The `judge.llm` capability.
///
/// Input schema: `{ prompt: string, creator_id?: string }`
/// Output schema: `{ result: boolean, reason: string }`
pub struct JudgeLlm;

/// Judge response verdicts that count as "go".
const GO_WORDS: &[&str] = &["go", "yes", "proceed", "pass", "approve", "ok"];

/// Judge response verdicts that count as "no-go".
const NOGO_WORDS: &[&str] = &["wait", "no", "stop", "reject", "deny", "fail"];

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
                "creator_id": { "type": "string", "description": "Optional creator ID for the judge agent" }
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
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CapabilityError::InputInvalid("missing 'prompt' field".into()))?;

        // In the full implementation, this would call acp.prompt with deny_all
        // tool policy and parse the response. For the capability layer stub,
        // we do a simple heuristic parse on the prompt itself (for testing).
        let (result, reason) = parse_judge_response(prompt);

        Ok(json!({
            "result": result,
            "reason": reason
        }))
    }
}

/// Parse a judge response text into a boolean verdict.
///
/// "go" / "yes" / "proceed" → true
/// "wait" / "no" → false
/// Ambiguous → error
fn parse_judge_response(text: &str) -> (bool, String) {
    let lower = text.trim().to_lowercase();

    for word in GO_WORDS {
        if lower.contains(word) {
            return (true, format!("judge.llm: go (matched '{word}')"));
        }
    }

    for word in NOGO_WORDS {
        if lower.contains(word) {
            return (false, format!("judge.llm: nogo (matched '{word}')"));
        }
    }

    (
        false,
        format!("judge.llm: ambiguous response — '{lower:.50}'"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn judge_llm_name() {
        assert_eq!(JudgeLlm.name(), "judge.llm");
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

    #[tokio::test]
    async fn judge_llm_valid_go_input() {
        let cap = JudgeLlm;
        let input = json!({ "prompt": "Yes, go ahead with the next step" });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["result"], true);
    }

    #[tokio::test]
    async fn judge_llm_valid_nogo_input() {
        let cap = JudgeLlm;
        let input = json!({ "prompt": "No, stop and wait" });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["result"], false);
    }

    #[tokio::test]
    async fn judge_llm_missing_prompt_errors() {
        let cap = JudgeLlm;
        let input = json!({});
        let result = cap.run(input).await;
        assert!(result.is_err());
    }
}
