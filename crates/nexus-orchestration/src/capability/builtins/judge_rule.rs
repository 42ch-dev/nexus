//! `judge.rule` capability — pure function rule evaluation (no LLM).
//!
//! Evaluates rule expressions over `contextData` JSON. Supported syntax:
//! - `"always_true"` / `"always_false"` — boolean literals
//! - `"context.<field> == <value>"` — field equality
//! - `"context.<field> != <value>"` — field inequality
//! - `"context.<field> >= <number>"` / `">"` / `"<="` / `"<"` — numeric compare
//!
//! Design: DF-32, plan J1.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use nexus_contracts::local::orchestration::{JudgeRuleInput, JudgeRuleOutput};
use serde_json::Value;

/// Evaluate a pure rule (AST over context data).
///
/// Supports boolean literals, field equality/inequality, and numeric comparison.
/// Returns `InputInvalid` for truly unsupported syntax.
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

        let (result, reason) = evaluate_rule(&input.rule, &input.context_data)?;

        let output = JudgeRuleOutput { result, reason };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Rule evaluation engine
// ---------------------------------------------------------------------------

/// Evaluate a rule expression against context data.
fn evaluate_rule(rule: &str, context_data: &Value) -> Result<(bool, String), CapabilityError> {
    let trimmed = rule.trim();

    // Boolean literals
    match trimmed {
        "always_true" => return Ok((true, "rule: always_true → go".to_string())),
        "always_false" => return Ok((false, "rule: always_false → nogo".to_string())),
        _ => {}
    }

    // Context field comparisons: "context.<field> <op> <value>"
    if let Some(rest) = trimmed.strip_prefix("context.") {
        return evaluate_context_comparison(rest, context_data);
    }

    Err(CapabilityError::InputInvalid(format!(
        "unsupported rule syntax: '{rule}'"
    )))
}

/// Parse and evaluate a context comparison like `count >= 1` or `status == "active"`.
fn evaluate_context_comparison(
    expr: &str,
    context_data: &Value,
) -> Result<(bool, String), CapabilityError> {
    // Try operators in order of longest-first to avoid partial matches.
    let operators = [">=", "<=", "!=", "==", ">", "<"];

    for op in operators {
        if let Some(pos) = expr.find(op) {
            let field = expr[..pos].trim();
            let value_str = expr[pos + op.len()..].trim();

            if field.is_empty() {
                return Err(CapabilityError::InputInvalid(format!(
                    "empty field name in rule: '{expr}'"
                )));
            }

            let field_value = context_data.get(field).ok_or_else(|| {
                CapabilityError::InputInvalid(format!("field '{field}' not found in contextData"))
            })?;

            return match op {
                "==" => return Ok(evaluate_equality(field, field_value, value_str)),
                "!=" => {
                    let (eq_result, _) = evaluate_equality(field, field_value, value_str);
                    Ok((
                        !eq_result,
                        format!("rule: context.{field} != {value_str} → {}", !eq_result),
                    ))
                }
                ">=" | "<=" | ">" | "<" => evaluate_numeric(field, field_value, value_str, op),
                _ => unreachable!("operator list is exhaustive"),
            };
        }
    }

    Err(CapabilityError::InputInvalid(format!(
        "no comparison operator found in rule: 'context.{expr}'"
    )))
}

/// Evaluate string/boolean/number equality.
fn evaluate_equality(field: &str, field_value: &Value, expected: &str) -> (bool, String) {
    // Try to parse expected as a quoted string
    let expected_val =
        if let Some(unquoted) = expected.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            serde_json::Value::String(unquoted.to_string())
        } else if expected == "true" {
            serde_json::Value::Bool(true)
        } else if expected == "false" {
            serde_json::Value::Bool(false)
        } else if let Ok(n) = expected.parse::<i64>() {
            serde_json::Value::Number(n.into())
        } else if let Ok(n) = expected.parse::<f64>() {
            // f64 is lossy but acceptable for rule evaluation
            serde_json::to_value(n).unwrap_or_default()
        } else {
            // Treat as plain string comparison against field_value's string repr
            let field_str = field_value.as_str().unwrap_or("");
            let result = field_str == expected;
            return (
                result,
                format!("rule: context.{field} == {expected} → {result}"),
            );
        };

    let result = *field_value == expected_val;
    (
        result,
        format!("rule: context.{field} == {expected} → {result}"),
    )
}

/// Evaluate numeric comparison.
fn evaluate_numeric(
    field: &str,
    field_value: &Value,
    rhs_str: &str,
    op: &str,
) -> Result<(bool, String), CapabilityError> {
    let lhs = value_to_f64(field_value).ok_or_else(|| {
        CapabilityError::InputInvalid(format!("field '{field}' is not numeric: {field_value}"))
    })?;

    let rhs: f64 = rhs_str.parse().map_err(|_| {
        CapabilityError::InputInvalid(format!("right-hand side is not a number: '{rhs_str}'"))
    })?;

    let result = match op {
        ">=" => lhs >= rhs,
        "<=" => lhs <= rhs,
        ">" => lhs > rhs,
        "<" => lhs < rhs,
        _ => unreachable!(),
    };

    Ok((
        result,
        format!("rule: context.{field} {op} {rhs_str} → {result}"),
    ))
}

/// Extract an f64 from a JSON value (Number only).
fn value_to_f64(v: &Value) -> Option<f64> {
    v.as_f64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn judge_rule_always_true() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
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
            .run(json!({
                "rule": "always_false",
                "contextData": {}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], false);
    }

    // ── J1: Field equality tests ──────────────────────────────────────

    #[tokio::test]
    async fn judge_rule_field_equals_string() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
                "rule": "context.status == \"active\"",
                "contextData": {"status": "active"}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], true);
    }

    #[tokio::test]
    async fn judge_rule_field_equals_string_mismatch() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
                "rule": "context.status == \"active\"",
                "contextData": {"status": "inactive"}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], false);
    }

    #[tokio::test]
    async fn judge_rule_field_equals_number() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
                "rule": "context.count == 0",
                "contextData": {"count": 0}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], true);
    }

    #[tokio::test]
    async fn judge_rule_field_equals_boolean() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
                "rule": "context.enabled == true",
                "contextData": {"enabled": true}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], true);
    }

    // ── J1: Field inequality tests ────────────────────────────────────

    #[tokio::test]
    async fn judge_rule_field_not_equals() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
                "rule": "context.status != \"inactive\"",
                "contextData": {"status": "active"}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], true);
    }

    // ── J1: Numeric comparison tests ──────────────────────────────────

    #[tokio::test]
    async fn judge_rule_numeric_gte() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
                "rule": "context.count >= 1",
                "contextData": {"count": 5}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], true);
    }

    #[tokio::test]
    async fn judge_rule_numeric_gte_equal() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
                "rule": "context.count >= 5",
                "contextData": {"count": 5}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], true);
    }

    #[tokio::test]
    async fn judge_rule_numeric_gte_fail() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
                "rule": "context.count >= 10",
                "contextData": {"count": 5}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], false);
    }

    #[tokio::test]
    async fn judge_rule_numeric_gt() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
                "rule": "context.score > 0.5",
                "contextData": {"score": 0.8}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], true);
    }

    #[tokio::test]
    async fn judge_rule_numeric_lt() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
                "rule": "context.errors < 3",
                "contextData": {"errors": 1}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], true);
    }

    #[tokio::test]
    async fn judge_rule_numeric_lte() {
        let cap = JudgeRule;
        let out = cap
            .run(json!({
                "rule": "context.errors <= 3",
                "contextData": {"errors": 3}
            }))
            .await
            .unwrap();
        assert_eq!(out["result"], true);
    }

    // ── J1: Error cases ───────────────────────────────────────────────

    #[tokio::test]
    async fn judge_rule_unsupported_syntax() {
        let cap = JudgeRule;
        let result = cap
            .run(json!({
                "rule": "complex_expression",
                "contextData": {"count": 42}
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unsupported rule syntax"),
            "error should mention unsupported: {err}"
        );
    }

    #[tokio::test]
    async fn judge_rule_missing_field() {
        let cap = JudgeRule;
        let result = cap
            .run(json!({
                "rule": "always_true"
                // missing contextData
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn judge_rule_field_not_in_context() {
        let cap = JudgeRule;
        let result = cap
            .run(json!({
                "rule": "context.missing_field == \"value\"",
                "contextData": {}
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found in contextData"), "error: {err}");
    }

    #[tokio::test]
    async fn judge_rule_non_numeric_field_numeric_op() {
        let cap = JudgeRule;
        let result = cap
            .run(json!({
                "rule": "context.name >= 1",
                "contextData": {"name": "Alice"}
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not numeric"), "error: {err}");
    }

    // ── Unit tests for evaluate_rule ──────────────────────────────────

    #[test]
    fn unit_evaluate_always_true() {
        let (result, reason) = evaluate_rule("always_true", &json!({})).unwrap();
        assert!(result);
        assert!(reason.contains("always_true"));
    }

    #[test]
    fn unit_evaluate_field_equality_with_string() {
        let ctx = json!({"status": "active"});
        let (result, _) = evaluate_rule("context.status == \"active\"", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn unit_evaluate_field_gte() {
        let ctx = json!({"count": 42});
        let (result, _) = evaluate_rule("context.count >= 1", &ctx).unwrap();
        assert!(result);
    }
}
