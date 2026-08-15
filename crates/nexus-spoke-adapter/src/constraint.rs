//! AR-2 constraint carrier — the nexus product grammar for structured rules.
//!
//! V1.166 (DR-64): a rule's machine-evaluated semantics live in
//! `Rule.extensions["nexus"]["constraint"]` as a typed JSON object
//! discriminated by `family`. This module owns the carrier types and the
//! strict validator; it is the **sole consumer** of the carrier grammar at
//! the spoke-adapter boundary (per the plan's sole-consumer rule, the
//! evaluator logic itself is daemon-side, T4).
//!
//! The six carrier shapes (verbatim AR-2 — the entire operator set):
//!
//! ```json
//! { "family": "module_presence",  "module_key": "<non-empty string>" }
//! { "family": "module_absence",   "module_key": "<non-empty string>" }
//! { "family": "required_field",   "field": "body.summary" }
//! { "family": "required_field",   "field": "body.tags" }
//! { "family": "required_field",   "module_key": "<non-empty string>", "field": "<non-empty string>" }
//! { "family": "observer_cardinality", "min": 0, "max": 3 }
//! ```
//!
//! Shapes are **closed** — unknown members are rejected (the carrier is not
//! an open bag), and [`parse_carrier_json`] errors name the offending member.
//! The CLI `creator world rule add` is the validation gate (fail early); the
//! evaluator read path ([`constraint_from_rule`]) is lenient by design:
//! absent or unparseable carriers yield `None` and the rule is skipped.

use crate::Rule;
use serde_json::{Map, Value};
use spoke_schemas::data::rule::RuleExtensionsKey;

/// The `extensions.nexus` namespace key (lowercase, matches the
/// `^[a-z][a-z0-9_-]*$` namespace convention — same literal as
/// [`crate::extensions`]).
const NAMESPACE: &str = "nexus";

/// The `extensions.nexus.constraint` key carrying the AR-2 carrier.
const CONSTRAINT_KEY: &str = "constraint";

/// A parsed, validated structured constraint (AR-2 carrier types — verbatim).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    ModulePresence { module_key: String },
    ModuleAbsence { module_key: String },
    RequiredField { target: RequiredFieldTarget },
    ObserverCardinality { min: Option<u64>, max: Option<u64> },
}

/// The `required_field` operand target: an entry-level field (closed set) or
/// a module-row-level field (free-form).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequiredFieldTarget {
    /// `"body.summary"` | `"body.tags"` — the closed entry-level set.
    Entry(EntryField),
    /// Row-level: every object row of `modules.<module_key>` must carry
    /// `field` populated.
    ModuleRow { module_key: String, field: String },
}

/// The closed set of entry-level `required_field` fields (AR-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryField {
    BodySummary,
    BodyTags,
}

impl Constraint {
    /// The constraint family — the closed PD-1 set; also the emitted
    /// `Finding.kind` (AR-4: `module_presence` | `module_absence` |
    /// `required_field` | `observer_cardinality`).
    #[must_use]
    pub const fn family(&self) -> &'static str {
        match self {
            Self::ModulePresence { .. } => "module_presence",
            Self::ModuleAbsence { .. } => "module_absence",
            Self::RequiredField { .. } => "required_field",
            Self::ObserverCardinality { .. } => "observer_cardinality",
        }
    }
}

/// Strictly parse a carrier [`Value`] into a [`Constraint`] (AR-2 shapes).
///
/// This is the **CLI-only validation gate** (`creator world rule add`, fail
/// early). The carrier must be a JSON **object** matching exactly one of the
/// six locked shapes: `family` is required and must be one of the four closed
/// families; unknown extra members are rejected; operand requirements are
/// enforced per family.
///
/// # Errors
///
/// Returns a human message **naming the offending member** (e.g.
/// `unknown family "tone"; expected module_presence | module_absence |
/// required_field | observer_cardinality`) for any shape violation. The
/// caller (CLI) prefixes it with `--constraint:`.
pub fn parse_carrier_json(value: &Value) -> Result<Constraint, String> {
    let obj = value.as_object().ok_or_else(|| {
        format!(
            "constraint must be a JSON object, got {}",
            json_kind_name(value)
        )
    })?;

    let family = match obj.get("family") {
        Some(Value::String(s)) => s.as_str(),
        Some(_) => {
            return Err("\"family\" must be a string".to_string());
        }
        None => return Err("missing required member \"family\"".to_string()),
    };

    let constraint = match family {
        "module_presence" | "module_absence" => {
            reject_unknown_members(obj, &["family", "module_key"])?;
            let module_key = require_non_empty_string(obj, "module_key")?;
            if family == "module_presence" {
                Constraint::ModulePresence { module_key }
            } else {
                Constraint::ModuleAbsence { module_key }
            }
        }
        "required_field" => {
            reject_unknown_members(obj, &["family", "module_key", "field"])?;
            let field = require_non_empty_string(obj, "field")?;
            if obj.contains_key("module_key") {
                // Module-row operand form: module_key + free-form field.
                let module_key = require_non_empty_string(obj, "module_key")?;
                if matches!(field.as_str(), "body.summary" | "body.tags") {
                    return Err(format!(
                        "unknown \"field\" value \"{field}\"; \"{field}\" is the \
                         entry-level operand, which cannot be combined with \
                         \"module_key\" — required_field takes exactly one operand \
                         form (entry-level field OR module-row module_key + field)"
                    ));
                }
                Constraint::RequiredField {
                    target: RequiredFieldTarget::ModuleRow { module_key, field },
                }
            } else {
                // Entry-level operand form: field ∈ closed set.
                let entry = match field.as_str() {
                    "body.summary" => EntryField::BodySummary,
                    "body.tags" => EntryField::BodyTags,
                    other => {
                        return Err(format!(
                            "unknown \"field\" value \"{other}\"; entry-level \
                             required_field expects \"body.summary\" | \"body.tags\""
                        ));
                    }
                };
                Constraint::RequiredField {
                    target: RequiredFieldTarget::Entry(entry),
                }
            }
        }
        "observer_cardinality" => {
            reject_unknown_members(obj, &["family", "min", "max"])?;
            let min = match obj.get("min") {
                None => None,
                Some(v) => Some(require_u64(v, "min")?),
            };
            let max = match obj.get("max") {
                None => None,
                Some(v) => Some(require_u64(v, "max")?),
            };
            match (min, max) {
                (None, None) => {
                    return Err(
                        "observer_cardinality requires at least one of \"min\" or \"max\""
                            .to_string(),
                    );
                }
                (Some(min), Some(max)) if min > max => {
                    return Err(format!("\"min\" ({min}) must not exceed \"max\" ({max})"));
                }
                (min, max) => Constraint::ObserverCardinality { min, max },
            }
        }
        other => {
            return Err(format!(
                "unknown family \"{other}\"; expected module_presence | module_absence \
                 | required_field | observer_cardinality"
            ));
        }
    };

    Ok(constraint)
}

/// Evaluator read path: extract the AR-2 carrier from a spoke [`Rule`].
///
/// Reads `extensions["nexus"]["constraint"]` and parses it strictly.
/// Absent namespace / absent constraint / malformed carrier → `None` (the
/// evaluator skips the rule — lenient by design, AR-2). The carrier is the
/// only nexus key rules carry today (AR-2: the namespace is written fresh at
/// create).
#[must_use]
pub fn constraint_from_rule(rule: &Rule) -> Option<Constraint> {
    let nexus_key = RuleExtensionsKey::try_from(NAMESPACE).ok()?;
    let constraint = rule.extensions.get(&nexus_key)?.get(CONSTRAINT_KEY)?;
    parse_carrier_json(constraint).ok()
}

/// Require `key` in `obj` be a non-empty string; error names `key`.
fn require_non_empty_string(obj: &Map<String, Value>, key: &str) -> Result<String, String> {
    match obj.get(key) {
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.clone()),
        Some(Value::String(_)) => Err(format!("\"{key}\" must be a non-empty string")),
        Some(_) => Err(format!("\"{key}\" must be a string")),
        None => Err(format!("missing required member \"{key}\"")),
    }
}

/// Require `key` in `obj` be a `u64 ≥ 0`; error names `key`.
fn require_u64(value: &Value, key: &str) -> Result<u64, String> {
    match value {
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| format!("\"{key}\" must be a non-negative integer, got {n}")),
        _ => Err(format!("\"{key}\" must be a non-negative integer")),
    }
}

/// Reject closed-shape violations: any member outside `allowed` names itself.
fn reject_unknown_members(obj: &Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unknown member \"{key}\" in constraint"));
        }
    }
    Ok(())
}

/// Human name of a non-object JSON value kind (for the non-object reject).
const fn json_kind_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use spoke_schemas::data::rule::RuleCanonicalName;
    use std::collections::HashMap;
    use std::num::NonZeroU64;

    // ── parse_carrier_json: valid six shapes ─────────────────────────

    #[test]
    fn parses_module_presence_and_absence() {
        assert_eq!(
            parse_carrier_json(&json!({"family": "module_presence", "module_key": "characters"})),
            Ok(Constraint::ModulePresence {
                module_key: "characters".to_string()
            })
        );
        assert_eq!(
            parse_carrier_json(&json!({"family": "module_absence", "module_key": "lore"})),
            Ok(Constraint::ModuleAbsence {
                module_key: "lore".to_string()
            })
        );
    }

    #[test]
    fn parses_required_field_entry_and_module_row_forms() {
        assert_eq!(
            parse_carrier_json(&json!({"family": "required_field", "field": "body.summary"})),
            Ok(Constraint::RequiredField {
                target: RequiredFieldTarget::Entry(EntryField::BodySummary)
            })
        );
        assert_eq!(
            parse_carrier_json(&json!({"family": "required_field", "field": "body.tags"})),
            Ok(Constraint::RequiredField {
                target: RequiredFieldTarget::Entry(EntryField::BodyTags)
            })
        );
        assert_eq!(
            parse_carrier_json(&json!({
                "family": "required_field",
                "module_key": "characters",
                "field": "eye_color"
            })),
            Ok(Constraint::RequiredField {
                target: RequiredFieldTarget::ModuleRow {
                    module_key: "characters".to_string(),
                    field: "eye_color".to_string()
                }
            })
        );
        // Free-form module-row field may be any non-empty string — including
        // dotted names other than the two reserved entry-level values.
        assert!(parse_carrier_json(&json!({
            "family": "required_field",
            "module_key": "characters",
            "field": "body.height"
        }))
        .is_ok());
    }

    #[test]
    fn parses_observer_cardinality_min_max_combinations() {
        assert_eq!(
            parse_carrier_json(&json!({"family": "observer_cardinality", "min": 0, "max": 3})),
            Ok(Constraint::ObserverCardinality {
                min: Some(0),
                max: Some(3)
            })
        );
        assert_eq!(
            parse_carrier_json(&json!({"family": "observer_cardinality", "min": 2})),
            Ok(Constraint::ObserverCardinality {
                min: Some(2),
                max: None
            })
        );
        assert_eq!(
            parse_carrier_json(&json!({"family": "observer_cardinality", "max": 3})),
            Ok(Constraint::ObserverCardinality {
                min: None,
                max: Some(3)
            })
        );
    }

    #[test]
    fn family_names_match_pd1_closed_set() {
        assert_eq!(
            parse_carrier_json(&json!({"family": "module_presence", "module_key": "x"}))
                .unwrap()
                .family(),
            "module_presence"
        );
        assert_eq!(
            parse_carrier_json(&json!({"family": "module_absence", "module_key": "x"}))
                .unwrap()
                .family(),
            "module_absence"
        );
        assert_eq!(
            parse_carrier_json(&json!({"family": "required_field", "field": "body.tags"}))
                .unwrap()
                .family(),
            "required_field"
        );
        assert_eq!(
            parse_carrier_json(&json!({"family": "observer_cardinality", "min": 1}))
                .unwrap()
                .family(),
            "observer_cardinality"
        );
    }

    // ── parse_carrier_json: malformed carriers name the member ───────

    #[test]
    fn rejects_non_object_values() {
        for (bad, kind) in [
            (json!(null), "null"),
            (json!(true), "a boolean"),
            (json!(42), "a number"),
            (json!("tone"), "a string"),
            (json!([1, 2, 3]), "an array"),
        ] {
            let err = parse_carrier_json(&bad).unwrap_err();
            assert!(
                err.contains("constraint must be a JSON object") && err.contains(kind),
                "got: {err}"
            );
        }
    }

    #[test]
    fn rejects_unknown_family_naming_family() {
        let err = parse_carrier_json(&json!({"family": "tone"})).unwrap_err();
        assert_eq!(
            err,
            "unknown family \"tone\"; expected module_presence | module_absence \
             | required_field | observer_cardinality"
        );
    }

    #[test]
    fn rejects_missing_and_non_string_family() {
        let err = parse_carrier_json(&json!({"module_key": "x"})).unwrap_err();
        assert_eq!(err, "missing required member \"family\"");
        let err = parse_carrier_json(&json!({"family": 3})).unwrap_err();
        assert_eq!(err, "\"family\" must be a string");
    }

    #[test]
    fn rejects_entry_level_field_outside_closed_set() {
        let err = parse_carrier_json(&json!({
            "family": "required_field",
            "field": "body.plot"
        }))
        .unwrap_err();
        assert!(
            err.contains("unknown \"field\" value \"body.plot\"") && err.contains("body.summary"),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_required_field_with_none_operand_forms() {
        let err = parse_carrier_json(&json!({"family": "required_field"})).unwrap_err();
        assert_eq!(err, "missing required member \"field\"");
    }

    #[test]
    fn rejects_required_field_with_both_operand_forms() {
        let err = parse_carrier_json(&json!({
            "family": "required_field",
            "field": "body.summary",
            "module_key": "characters"
        }))
        .unwrap_err();
        assert!(
            err.contains("entry-level") && err.contains("module_key"),
            "both operand forms must be rejected naming the conflict, got: {err}"
        );
    }

    #[test]
    fn rejects_min_gt_max() {
        let err = parse_carrier_json(&json!({
            "family": "observer_cardinality",
            "min": 5,
            "max": 3
        }))
        .unwrap_err();
        assert_eq!(err, "\"min\" (5) must not exceed \"max\" (3)");
    }

    #[test]
    fn rejects_observer_cardinality_without_operands() {
        let err = parse_carrier_json(&json!({"family": "observer_cardinality"})).unwrap_err();
        assert_eq!(
            err,
            "observer_cardinality requires at least one of \"min\" or \"max\""
        );
    }

    #[test]
    fn rejects_negative_and_non_integer_bounds() {
        let err = parse_carrier_json(&json!({
            "family": "observer_cardinality",
            "min": -1
        }))
        .unwrap_err();
        assert!(err.contains("\"min\""), "got: {err}");
        let err = parse_carrier_json(&json!({
            "family": "observer_cardinality",
            "max": "three"
        }))
        .unwrap_err();
        assert!(err.contains("\"max\""), "got: {err}");
    }

    #[test]
    fn rejects_empty_and_missing_module_key() {
        for bad in [
            json!({"family": "module_presence"}),
            json!({"family": "module_presence", "module_key": ""}),
            json!({"family": "module_presence", "module_key": "   "}),
        ] {
            let err = parse_carrier_json(&bad).unwrap_err();
            assert!(
                err.contains("module_key"),
                "empty/missing module_key must name the member, got: {err}"
            );
        }
        let err = parse_carrier_json(&json!({
            "family": "required_field",
            "module_key": "",
            "field": "eye_color"
        }))
        .unwrap_err();
        assert_eq!(err, "\"module_key\" must be a non-empty string");
    }

    #[test]
    fn rejects_unknown_extra_members_closed_shapes() {
        for bad in [
            json!({"family": "module_presence", "module_key": "x", "bogus": 1}),
            json!({"family": "module_absence", "module_key": "x", "extra": true}),
            json!({"family": "required_field", "field": "body.summary", "min": 0}),
            json!({"family": "observer_cardinality", "min": 0, "max": 3, "field": "x"}),
        ] {
            let err = parse_carrier_json(&bad).unwrap_err();
            assert!(
                err.starts_with("unknown member \"") && err.contains("in constraint"),
                "closed shapes must name the extra member, got: {err}"
            );
        }
    }

    // ── constraint_from_rule: evaluator read path ────────────────────

    fn rule_with_extensions(extensions_json: &str) -> Rule {
        let extensions: HashMap<RuleExtensionsKey, Map<String, Value>> =
            serde_json::from_str(extensions_json).unwrap();
        Rule {
            canonical_name: RuleCanonicalName::try_from("characters need summaries".to_string())
                .unwrap(),
            created_at: None,
            description: None,
            extensions,
            kind: "rule".to_string(),
            rule_id: "rul_test".to_string(),
            schema_version: NonZeroU64::new(1).unwrap(),
            severity_hint: Some("warning".to_string()),
            source_anchor: None,
            statement: Some("Every character needs a summary".to_string()),
            status: Some("active".to_string()),
            target_entry_types: vec![],
            updated_at: None,
        }
    }

    #[test]
    fn constraint_from_rule_extracts_carrier() {
        let rule = rule_with_extensions(
            r#"{"nexus": {"constraint": {"family": "required_field", "field": "body.summary"}}}"#,
        );
        assert_eq!(
            constraint_from_rule(&rule),
            Some(Constraint::RequiredField {
                target: RequiredFieldTarget::Entry(EntryField::BodySummary)
            })
        );
    }

    #[test]
    fn constraint_from_rule_absent_or_malformed_is_none() {
        // Missing constraint key entirely.
        let no_constraint = rule_with_extensions(r#"{"nexus": {"other": 1}}"#);
        assert_eq!(constraint_from_rule(&no_constraint), None);

        // Missing nexus namespace.
        let no_namespace = rule_with_extensions(r#"{"other": {"constraint": {}}}"#);
        assert_eq!(constraint_from_rule(&no_namespace), None);

        // Empty extensions bag.
        let empty = rule_with_extensions(r"{}");
        assert_eq!(constraint_from_rule(&empty), None);

        // Malformed carrier (unknown family) → None (lenient evaluator read).
        let malformed = rule_with_extensions(
            r#"{"nexus": {"constraint": {"family": "tone", "module_key": "x"}}}"#,
        );
        assert_eq!(constraint_from_rule(&malformed), None);

        // Non-object constraint → None.
        let non_object = rule_with_extensions(r#"{"nexus": {"constraint": [1, 2, 3]}}"#);
        assert_eq!(constraint_from_rule(&non_object), None);
    }
}
