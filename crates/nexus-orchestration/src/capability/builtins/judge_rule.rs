//! `judge.rule` capability — pure function rule evaluation (no LLM).
//!
//! `judge.llm` is deferred to WS3.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use nexus_contracts::local::orchestration::{JudgeRuleInput, JudgeRuleOutput};
use serde_json::Value;

/// Evaluate a pure rule (AST over context data).
///
/// This is a **stub** rule evaluator that checks simple conditions.
/// The full rule evaluation engine will be implemented in a later wave.
///
/// Supported rules (stub):
/// - `"always_true"` → `result: true`
/// - `"always_false"` → `result: false`
/// - Any other string → error (rule not yet supported)
pub struct JudgeRule;

#[async_trait]
impl Capability for JudgeRule {
    fn name(&self) -> &'static str {
        "judge.rule"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"rule":{"type":"string"},"contextData":{}},"required":["rule","contextData"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"result":{"type":"boolean"},"reason":{"type":"string"}},"required":["result","reason"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let input: JudgeRuleInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("judge.rule input: {e}")))?;

        let (result, reason) = match input.rule.as_str() {
            "always_true" => (true, "stub rule: always_true evaluates to go".to_string()),
            "always_false" => (
                false,
                "stub rule: always_false evaluates to nogo".to_string(),
            ),
            other => {
                return Err(CapabilityError::InputInvalid(format!(
                    "unsupported rule: '{other}' (full rule engine deferred to later wave)"
                )));
            }
        };

        let output = JudgeRuleOutput { result, reason };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn judge_rule_always_true() {
        let cap = JudgeRule;
        let out = cap
            .run(serde_json::json!({
                "rule": "always_true",
                "contextData": {}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], true);
    }

    #[tokio::test]
    async fn judge_rule_always_false() {
        let cap = JudgeRule;
        let out = cap
            .run(serde_json::json!({
                "rule": "always_false",
                "contextData": {}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], false);
    }

    #[tokio::test]
    async fn judge_rule_unsupported() {
        let cap = JudgeRule;
        let result = cap
            .run(serde_json::json!({
                "rule": "complex_expression",
                "contextData": {"count": 42}
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn judge_rule_missing_field() {
        let cap = JudgeRule;
        let result = cap
            .run(serde_json::json!({
                "rule": "always_true"
                // missing contextData
            }))
            .await;
        assert!(result.is_err());
    }
}
