//! Presence-preserving deserialization for tri-state nullable PATCH fields.
//!
//! A plain `Option<T>` field erases the difference between *absent* and
//! explicit `null` (serde maps both to `None`). The `x-nexus-tri-state`
//! schema marker (`tooling/codegen/rust-gen`) injects
//! [`deserialize_presence`] into exactly those generated fields, so the
//! carrier distinguishes all three wire states:
//!
//! - field absent → `None` (keep the stored column);
//! - field `null` → `Some(Value::Null)` (clear the column to SQL NULL);
//! - field value  → `Some(Value::String(…))` (set it).
//!
//! The field type stays the typify-emitted `Option<serde_json::Value>`; only
//! the attribute is generated, so unmarked optional fields are untouched.
//!
//! [`presence_string`] is the second half of the same contract: the ONE
//! projection from that carrier onto the typed `Option<Option<String>>` the
//! stored layers use. Every consumer of a tri-state string field projects
//! through it — the native adapters (`crates/nexus-core-node`) and the core
//! findings authority — so the three-state grammar exists once, next to the
//! carrier that preserves it.

use serde::Deserialize;

/// Deserialize a JSON value as "present", preserving explicit `null`.
///
/// Always yields `Some(…)`: the `Option` layer is the *presence* layer
/// (`#[serde(default)]` supplies `None` for an absent field), and the inner
/// [`serde_json::Value`] carries the wire value verbatim — including `null`.
///
/// # Errors
///
/// Returns the deserializer's own error when the wire value is not valid JSON
/// for [`serde_json::Value`] (malformed input, or a visitor failure raised by
/// the underlying format).
pub fn deserialize_presence<'de, D>(deserializer: D) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(serde_json::Value::deserialize(deserializer)?))
}

/// Project a presence-preserving string carrier onto the typed three-state
/// form the stored layers use.
///
/// - absent → `Ok(None)` (do not touch the stored column);
/// - `null` → `Ok(Some(None))` (clear it to SQL NULL);
/// - string → `Ok(Some(Some(text)))` (set it).
///
/// # Errors
///
/// The refusal *reason* for any other JSON value — the caller names the field
/// in its own refusal shape (`invalid <field>: <reason>` at the N-API
/// boundary; `CoreError::InvalidInput { field, reason }` inside core), so the
/// three-state grammar itself stays in this one place.
pub fn presence_string(value: Option<serde_json::Value>) -> Result<Option<Option<String>>, String> {
    match value {
        None => Ok(None),
        Some(serde_json::Value::Null) => Ok(Some(None)),
        Some(serde_json::Value::String(text)) => Ok(Some(Some(text))),
        Some(other) => Err(format!(
            "expected a string, null, or omission, got {}",
            json_kind(&other)
        )),
    }
}

/// The JSON kind of a wire value, for refusal messages.
const fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::deserialize_presence;
    use serde_json::json;

    #[derive(Debug, serde::Deserialize, serde::Serialize)]
    #[serde(deny_unknown_fields)]
    struct Probe {
        #[serde(
            default,
            deserialize_with = "deserialize_presence",
            skip_serializing_if = "Option::is_none"
        )]
        rule_suggestion: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    }

    #[test]
    fn absent_is_none() {
        let probe: Probe = serde_json::from_str(r#"{"status":"triaged"}"#).expect("parses");
        assert_eq!(probe.rule_suggestion, None);
        assert_eq!(probe.status.as_deref(), Some("triaged"));
    }

    #[test]
    fn explicit_null_is_present_null() {
        let probe: Probe = serde_json::from_str(r#"{"rule_suggestion":null}"#).expect("parses");
        assert_eq!(probe.rule_suggestion, Some(json!(null)));
    }

    #[test]
    fn value_is_present_value() {
        let probe: Probe =
            serde_json::from_str(r#"{"rule_suggestion":"prefer scene breaks"}"#).expect("parses");
        assert_eq!(probe.rule_suggestion, Some(json!("prefer scene breaks")));
    }

    #[test]
    fn serialization_round_trips_all_three_states() {
        let keep: Probe = serde_json::from_str("{}").expect("parses");
        assert_eq!(serde_json::to_string(&keep).expect("keep"), "{}");
        let clear: Probe = serde_json::from_str(r#"{"rule_suggestion":null}"#).expect("parses");
        assert_eq!(
            serde_json::to_string(&clear).expect("clear"),
            r#"{"rule_suggestion":null}"#
        );
        let set: Probe = serde_json::from_str(r#"{"rule_suggestion":"v2"}"#).expect("parses");
        assert_eq!(
            serde_json::to_string(&set).expect("set"),
            r#"{"rule_suggestion":"v2"}"#
        );
    }

    /// The typed projection every tri-state string consumer shares: the
    /// carrier's three states become `None` / `Some(None)` / `Some(Some(_))`,
    /// and no other JSON value is ever accepted as a state.
    #[test]
    fn presence_string_projects_all_three_states_and_refuses_the_rest() {
        assert_eq!(super::presence_string(None), Ok(None));
        assert_eq!(
            super::presence_string(Some(json!(null))),
            Ok(Some(None)),
            "explicit null clears the column"
        );
        assert_eq!(
            super::presence_string(Some(json!("prefer scene breaks"))),
            Ok(Some(Some("prefer scene breaks".to_string())))
        );
        assert_eq!(
            super::presence_string(Some(json!(7))),
            Err("expected a string, null, or omission, got a number".to_string())
        );
        assert_eq!(
            super::presence_string(Some(json!({ "not": "a string" }))),
            Err("expected a string, null, or omission, got an object".to_string())
        );
    }
}
