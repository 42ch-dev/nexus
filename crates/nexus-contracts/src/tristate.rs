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

#[cfg(test)]
mod tests {
    use super::deserialize_presence;
    use serde_json::json;

    #[derive(Debug, serde::Deserialize, serde::Serialize)]
    #[serde(deny_unknown_fields)]
    struct Probe {
        #[serde(default, deserialize_with = "deserialize_presence")]
        rule_suggestion: Option<serde_json::Value>,
        #[serde(default)]
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
}
