//! Four-family structured-rule evaluator (V1.166 PD-1 / AR-2 / AR-4, DR-64).
//!
//! Composed beside the mental pair by [`super::run_all`] (AR-4). **Pure**:
//! no store access — world = `input.request.scope.scope_id`, data =
//! `input.entries` / `input.events`, rules = the already-world-scoped
//! `input.rules` (the AR-1 seam guarantees world ownership before
//! orchestration at both production callers).
//!
//! `statement` is **never parsed** (PD-1 — the human summary only).
//! Machine evaluation reads the AR-2 constraint carrier
//! (`Rule.extensions["nexus"]["constraint"]`) via
//! [`constraint_from_rule`]; a rule without a parseable carrier is skipped
//! (lenient by design, AR-2).
//!
//! # Families (Finding `kind` = family — PD-1, never `Rule.kind`)
//!
//! | Family | Matching entries/events | Match = emit finding |
//! |--------|--------------------------|-----------------------|
//! | `module_presence` | entries | `modules.<module_key>` key absent |
//! | `module_absence` | entries | `modules.<module_key>` key present |
//! | `required_field` | entries | named field missing / empty per AR-2 operators |
//! | `observer_cardinality` | events with a **recorded** observation | observer count outside `[min, max]` |
//!
//! Targeting: `Rule.target_entry_types` filters **entries** for the three
//! entry families (empty = all in scope); it is inapplicable to
//! `observer_cardinality` (events carry no `entry_type` — the CLI rejects
//! `--entry-type` alongside that carrier, AR-2).
//!
//! PD-9 (V1.164, honored via the shared [`super::observation_observers`]
//! parse): absent/malformed `modules.observation` **never matches** —
//! unrecorded ≠ nobody.
//!
//! One finding per violating entry/event (AR-4). `severity` =
//! `severity_hint` verbatim else the uniform `warning` default (AR-4 — no
//! family overrides). `title` = `{family}: {canonical_name}`;
//! `target_entry_id` = the violating entry/event id; descriptions are one
//! deterministic English line naming the rule `canonical_name` + family /
//! operator specifics — never quoting `statement` (PD-1).

use crate::check::{finding, observation_observers};
use nexus_spoke_adapter::constraint::{
    constraint_from_rule, Constraint, EntryField, RequiredFieldTarget,
};
use nexus_spoke_adapter::{
    CheckRunInput, Finding, KnowledgeEntry, Rule, SpokeResult, TimelineEvent,
};
use serde_json::Value;

/// The uniform severity default for all four families (AR-4 — spoke
/// vocabulary; `severity_hint` verbatim wins when set).
const DEFAULT_SEVERITY: &str = "warning";

/// Run the structured-rule evaluator over a scoped `orchestrate_check`
/// input.
///
/// One finding per violating entry/event; a rule with no parseable carrier
/// is skipped; an empty rule set is an emergent no-op (AR-4 fast path — the
/// mental pair always runs, this module just emits nothing).
#[must_use]
pub fn run_check(input: &CheckRunInput, creator_id: &str) -> SpokeResult<Vec<Finding>> {
    let world_id = &input.request.scope.scope_id;
    let mut findings: Vec<Finding> = Vec::new();

    for rule in &input.rules {
        let Some(constraint) = constraint_from_rule(rule) else {
            // Unparseable / absent carrier → skip the rule (AR-2 lenient read).
            continue;
        };
        let family = constraint.family();
        let severity = rule
            .severity_hint
            .clone()
            .unwrap_or_else(|| DEFAULT_SEVERITY.to_string());

        // One finding per violating entry/event, family dispatch (AR-4).
        findings.extend(match constraint {
            Constraint::ModulePresence { module_key } => check_module_presence(
                rule,
                family,
                &severity,
                &input.entries,
                world_id,
                creator_id,
                &module_key,
            ),
            Constraint::ModuleAbsence { module_key } => check_module_absence(
                rule,
                family,
                &severity,
                &input.entries,
                world_id,
                creator_id,
                &module_key,
            ),
            Constraint::RequiredField { target } => check_required_field(
                rule,
                family,
                &severity,
                &input.entries,
                world_id,
                creator_id,
                &target,
            ),
            Constraint::ObserverCardinality { min, max } => check_observer_cardinality(
                rule,
                family,
                &severity,
                &input.events,
                world_id,
                creator_id,
                (min, max),
            ),
        });
    }

    SpokeResult::Ok(findings)
}

/// `module_presence`: matching entries MUST carry `modules.<module_key>` —
/// key absent → one finding on the entry.
fn check_module_presence(
    rule: &Rule,
    family: &str,
    severity: &str,
    entries: &[KnowledgeEntry],
    world_id: &str,
    creator_id: &str,
    module_key: &str,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for entry in matching_entries(rule, entries) {
        if !carries_module(entry, module_key) {
            findings.push(entry_finding(
                family,
                severity,
                entry,
                world_id,
                creator_id,
                format!(
                    "rule '{}' ({family} {module_key}): entry {} does not carry modules.{module_key}",
                    rule.canonical_name.as_str(),
                    entry.entry_id
                ),
            ));
        }
    }
    findings
}

/// `module_absence`: matching entries MUST NOT carry `modules.<module_key>`
/// — key present → one finding on the entry.
fn check_module_absence(
    rule: &Rule,
    family: &str,
    severity: &str,
    entries: &[KnowledgeEntry],
    world_id: &str,
    creator_id: &str,
    module_key: &str,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for entry in matching_entries(rule, entries) {
        if carries_module(entry, module_key) {
            findings.push(entry_finding(
                family,
                severity,
                entry,
                world_id,
                creator_id,
                format!(
                    "rule '{}' ({family} {module_key}): entry {} carries modules.{module_key}",
                    rule.canonical_name.as_str(),
                    entry.entry_id
                ),
            ));
        }
    }
    findings
}

/// `required_field`: matching entries MUST have a named field populated
/// (AR-2 operators — entry-level closed set or module-row-level).
fn check_required_field(
    rule: &Rule,
    family: &str,
    severity: &str,
    entries: &[KnowledgeEntry],
    world_id: &str,
    creator_id: &str,
    target: &RequiredFieldTarget,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for entry in matching_entries(rule, entries) {
        let missing = match target {
            // AR-2: populated = `Some` with non-whitespace content.
            RequiredFieldTarget::Entry(EntryField::BodySummary) => entry
                .body
                .summary
                .as_deref()
                .is_none_or(|summary| summary.trim().is_empty()),
            // AR-2: populated = non-empty vec.
            RequiredFieldTarget::Entry(EntryField::BodyTags) => entry.body.tags.is_empty(),
            // AR-2: every object row of the module (array form) must carry
            // the field populated (string non-empty after trim; any other
            // type non-null); absent/object-form/zero rows ⇒ vacuous pass.
            RequiredFieldTarget::ModuleRow { module_key, field } => {
                module_rows_missing_field(entry, module_key, field)
            }
        };
        if missing {
            // One deterministic English line per AR-4: rule canonical_name +
            // operator specifics. Entry-level = the field itself is missing;
            // row-level = a module row lacks the field (absent module / zero
            // rows are a vacuous pass).
            let description = match target {
                RequiredFieldTarget::Entry(EntryField::BodySummary) => format!(
                    "rule '{}' (required_field body.summary): entry {} has no populated body.summary",
                    rule.canonical_name.as_str(),
                    entry.entry_id
                ),
                RequiredFieldTarget::Entry(EntryField::BodyTags) => format!(
                    "rule '{}' (required_field body.tags): entry {} has no populated body.tags",
                    rule.canonical_name.as_str(),
                    entry.entry_id
                ),
                RequiredFieldTarget::ModuleRow { module_key, field } => format!(
                    "rule '{}' (required_field {module_key}.{field}): entry {} has a {module_key} row without populated {field}",
                    rule.canonical_name.as_str(),
                    entry.entry_id
                ),
            };
            findings.push(entry_finding(
                family,
                severity,
                entry,
                world_id,
                creator_id,
                description,
            ));
        }
    }
    findings
}

/// `observer_cardinality`: timeline events with a **recorded** observation
/// MUST have observer count in `[min, max]` — count outside the range →
/// one finding on the event. PD-9 via the shared parse: absent/malformed
/// observation never matches (unrecorded ≠ nobody).
fn check_observer_cardinality(
    rule: &Rule,
    family: &str,
    severity: &str,
    events: &[TimelineEvent],
    world_id: &str,
    creator_id: &str,
    bounds: (Option<u64>, Option<u64>),
) -> Vec<Finding> {
    let (min, max) = bounds;
    let mut findings = Vec::new();
    for event in events {
        let Some(observers) = observation_observers(event) else {
            continue;
        };
        let count = observers.len() as u64;
        let inside = min.is_none_or(|lo| count >= lo) && max.is_none_or(|hi| count <= hi);
        if !inside {
            findings.push(event_finding(
                family,
                severity,
                event,
                world_id,
                creator_id,
                format!(
                    "rule '{}' (observer_cardinality [{}..{}]): event {} has {} recorded observer(s) — outside the allowed range",
                    rule.canonical_name.as_str(),
                    min.map_or_else(|| "0".to_string(), |lo| lo.to_string()),
                    max.map_or_else(|| "unbounded".to_string(), |hi| hi.to_string()),
                    event.timeline_event_id,
                    count
                ),
            ));
        }
    }
    findings
}

/// The entries a rule targets: `target_entry_types` empty ⇒ all entries in
/// the check scope; otherwise `entry.entry_type` must be in the set (AR-2
/// targeting axis).
fn matching_entries<'a>(rule: &Rule, entries: &'a [KnowledgeEntry]) -> Vec<&'a KnowledgeEntry> {
    if rule.target_entry_types.is_empty() {
        entries.iter().collect()
    } else {
        entries
            .iter()
            .filter(|entry| {
                rule.target_entry_types
                    .iter()
                    .any(|t| t == &entry.entry_type)
            })
            .collect()
    }
}

/// Whether the entry carries `modules.<module_key>` (key present — on the
/// typed wire `KnowledgeEntryModulesValue` is exactly object-or-array, so
/// key-present == carried, AR-2).
fn carries_module(entry: &KnowledgeEntry, module_key: &str) -> bool {
    let Ok(Value::Object(map)) = serde_json::to_value(&entry.modules) else {
        return false;
    };
    map.contains_key(module_key)
}

/// Row-level `required_field` check (AR-2): every **object** row of
/// `modules.<module_key>` (array form) must carry `field` populated —
/// string: non-empty after trim; any other JSON type: non-null. Non-object
/// rows are skipped (mental.rs `belief_rows` parse precedent). Absent
/// module / object-form module / zero rows ⇒ vacuous pass (presence is
/// `module_presence`'s job).
fn module_rows_missing_field(entry: &KnowledgeEntry, module_key: &str, field: &str) -> bool {
    let Ok(Value::Object(map)) = serde_json::to_value(&entry.modules) else {
        return false;
    };
    let Some(Value::Array(rows)) = map.get(module_key) else {
        return false; // absent module or object-form module → vacuous pass
    };
    rows.iter().any(|row| {
        let Value::Object(row_map) = row else {
            return false; // non-object rows skipped
        };
        !field_populated(row_map.get(field))
    })
}

/// AR-2 populated semantics for a row-level field value: string → non-empty
/// after trim; any other JSON type → non-null.
fn field_populated(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(s)) => !s.trim().is_empty(),
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

/// Stamp one finding on a violating entry (shared AR-4 builder; title label
/// = `canonical_name`).
fn entry_finding(
    family: &str,
    severity: &str,
    entry: &KnowledgeEntry,
    world_id: &str,
    creator_id: &str,
    description: String,
) -> Finding {
    finding(
        family,
        severity,
        &entry.entry_id,
        entry.canonical_name.as_str(),
        world_id,
        creator_id,
        description,
    )
}

/// Stamp one finding on a violating event (shared AR-4 builder; title label
/// = `canonical_name`).
fn event_finding(
    family: &str,
    severity: &str,
    event: &TimelineEvent,
    world_id: &str,
    creator_id: &str,
    description: String,
) -> Finding {
    finding(
        family,
        severity,
        &event.timeline_event_id,
        event.canonical_name.as_str(),
        world_id,
        creator_id,
        description,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_spoke_adapter::constraint::Constraint;
    use nexus_spoke_adapter::Rule;
    use serde_json::{json, Value};

    // ── Fixtures ──────────────────────────────────────────────────────
    //
    // Fixtures deserialize the spoke wire types from JSON (the typify
    // newtypes are not re-exported across the adapter boundary) — exactly
    // the wire shape `CheckRunInput` carries.

    fn entry(
        id: &str,
        name: &str,
        entry_type: &str,
        body: &Value,
        modules: &Value,
    ) -> KnowledgeEntry {
        serde_json::from_value(json!({
            "body": body,
            "canonical_name": name,
            "entry_id": id,
            "entry_type": entry_type,
            "extensions": {},
            "status": "confirmed",
            "schema_version": 1,
            "modules": modules,
        }))
        .expect("fixture KnowledgeEntry is well-formed")
    }

    fn event(id: &str, name: &str, modules: &Value) -> TimelineEvent {
        serde_json::from_value(json!({
            "canonical_name": name,
            "timeline_event_id": id,
            "extensions": {},
            "schema_version": 1,
            "sort_key": "1",
            "modules": modules,
        }))
        .expect("fixture TimelineEvent is well-formed")
    }

    fn rule(
        id: &str,
        canonical_name: &str,
        severity_hint: Option<&str>,
        target_entry_types: &[&str],
        carrier: &Value,
    ) -> Rule {
        serde_json::from_value(json!({
            "schema_version": 1,
            "rule_id": id,
            "canonical_name": canonical_name,
            "kind": "rule",
            "extensions": { "nexus": { "constraint": carrier } },
            "severity_hint": severity_hint,
            "status": "active",
            "target_entry_types": target_entry_types,
        }))
        .expect("fixture Rule is well-formed")
    }

    fn check_input(
        rules: Vec<Rule>,
        entries: Vec<KnowledgeEntry>,
        events: Vec<TimelineEvent>,
    ) -> CheckRunInput {
        CheckRunInput {
            request: serde_json::from_value(json!({
                "scope": { "scope_id": "wld_fixture" },
            }))
            .expect("fixture CheckRequest is well-formed"),
            entries,
            events,
            rules,
        }
    }

    /// Run the evaluator and return `(kind, severity, target_entry_id)` rows
    /// (ordered as emitted).
    fn rows(findings: &[Finding]) -> Vec<(&str, &str, &str)> {
        findings
            .iter()
            .map(|f| {
                (
                    f.kind.as_deref().unwrap_or("(none)"),
                    f.severity.as_str(),
                    f.target_entry_id.as_deref().unwrap_or("(none)"),
                )
            })
            .collect()
    }

    fn run(
        rules: Vec<Rule>,
        entries: Vec<KnowledgeEntry>,
        events: Vec<TimelineEvent>,
    ) -> Vec<Finding> {
        match run_check(&check_input(rules, entries, events), "ctr_test") {
            SpokeResult::Ok(findings) => findings,
            SpokeResult::Reject(reject) => panic!("evaluator must not reject: {reject:?}"),
        }
    }

    /// The canonical no-carrier rule: skip (AR-2 lenient read).
    fn rule_without_carrier() -> Rule {
        serde_json::from_value(json!({
            "schema_version": 1,
            "rule_id": "rul_none",
            "canonical_name": "No carrier",
            "kind": "rule",
            "extensions": {},
            "status": "active",
            "target_entry_types": [],
        }))
        .expect("fixture Rule is well-formed")
    }

    // ── module_presence ───────────────────────────────────────────────

    #[test]
    fn module_presence_matches_missing_key_one_finding_per_entry() {
        let rules = vec![rule(
            "rul_mp",
            "Must carry characters",
            None,
            &["character"],
            &json!({ "family": "module_presence", "module_key": "characters" }),
        )];
        let entries = vec![
            entry(
                "kb_a",
                "A",
                "character",
                &json!({}),
                &json!({ "characters": {} }),
            ),
            entry(
                "kb_b",
                "B",
                "character",
                &json!({}),
                &json!({ "belief": [] }),
            ),
            entry(
                "kb_c",
                "C",
                "info_point",
                &json!({}),
                &json!({ "belief": [] }),
            ),
        ];
        let findings = run(rules, entries, vec![]);

        assert_eq!(
            rows(&findings),
            vec![("module_presence", "warning", "kb_b")],
            "only the targeted character without modules.characters matches; \
             info_point is outside target_entry_types"
        );
        assert_eq!(findings[0].title, "module_presence: B");
        assert!(
            findings[0].description.contains("Must carry characters"),
            "description names the rule canonical_name: {}",
            findings[0].description
        );
        assert!(
            !findings[0].description.contains("statement"),
            "description never parses/quotes statement"
        );
        assert_eq!(findings[0].status, "open");
        assert_eq!(findings[0].schema_version.get(), 1);
        assert!(findings[0].suggested_fix.is_none());
        assert!(findings[0].source_anchor.is_none());
        let nexus = findings[0]
            .extensions
            .get(&nexus_spoke_adapter::FindingExtensionsKey::try_from("nexus").expect("nexus key"));
        assert!(nexus.is_some(), "extensions.nexus must be stamped");
        assert_eq!(nexus.unwrap()["world_id"], "wld_fixture");
        assert_eq!(nexus.unwrap()["creator_id"], "ctr_test");
    }

    #[test]
    fn module_presence_empty_targets_all_types() {
        let rules = vec![rule(
            "rul_mp",
            "Everything needs characters",
            None,
            &[],
            &json!({ "family": "module_presence", "module_key": "characters" }),
        )];
        let entries = vec![
            entry(
                "kb_a",
                "A",
                "character",
                &json!({}),
                &json!({ "characters": {} }),
            ),
            entry("kb_b", "B", "info_point", &json!({}), &json!({})),
        ];
        let findings = run(rules, entries, vec![]);

        assert_eq!(
            rows(&findings),
            vec![("module_presence", "warning", "kb_b")],
            "empty target_entry_types ⇒ all entry types in scope"
        );
    }

    // ── module_absence ────────────────────────────────────────────────

    #[test]
    fn module_absence_matches_present_key() {
        let rules = vec![rule(
            "rul_ma",
            "No forbidden module",
            Some("error"),
            &[],
            &json!({ "family": "module_absence", "module_key": "forbidden" }),
        )];
        let entries = vec![
            entry(
                "kb_a",
                "A",
                "character",
                &json!({}),
                &json!({ "forbidden": {} }),
            ),
            entry(
                "kb_b",
                "B",
                "character",
                &json!({}),
                &json!({ "belief": [] }),
            ),
        ];
        let findings = run(rules, entries, vec![]);

        assert_eq!(
            rows(&findings),
            vec![("module_absence", "error", "kb_a")],
            "severity_hint 'error' verbatim; only the entry carrying the key matches"
        );
    }

    // ── required_field (entry-level) ───────────────────────────────────

    #[test]
    fn required_field_body_summary_matches_empty_summary() {
        let rules = vec![rule(
            "rul_rf",
            "Characters need summaries",
            Some("error"),
            &["character"],
            &json!({ "family": "required_field", "field": "body.summary" }),
        )];
        let entries = vec![
            // Populated (non-whitespace) → no match.
            entry(
                "kb_a",
                "A",
                "character",
                &json!({ "summary": "A hero" }),
                &json!({}),
            ),
            // Absent summary → match.
            entry("kb_b", "B", "character", &json!({}), &json!({})),
            // Whitespace-only summary → match (AR-2: non-whitespace content).
            entry(
                "kb_c",
                "C",
                "character",
                &json!({ "summary": "   " }),
                &json!({}),
            ),
            // Populated but outside targeting → no match.
            entry("kb_d", "D", "info_point", &json!({}), &json!({})),
        ];
        let findings = run(rules, entries, vec![]);

        assert_eq!(
            rows(&findings),
            vec![
                ("required_field", "error", "kb_b"),
                ("required_field", "error", "kb_c"),
            ],
            "missing + whitespace-only summaries match; populated and out-of-target do not"
        );
    }

    #[test]
    fn required_field_body_tags_matches_empty_tags() {
        let rules = vec![rule(
            "rul_rf_tags",
            "Entries need tags",
            None,
            &[],
            &json!({ "family": "required_field", "field": "body.tags" }),
        )];
        let entries = vec![
            entry(
                "kb_a",
                "A",
                "character",
                &json!({ "tags": ["pov"] }),
                &json!({}),
            ),
            entry("kb_b", "B", "character", &json!({ "tags": [] }), &json!({})),
            entry("kb_c", "C", "character", &json!({}), &json!({})),
        ];
        let findings = run(rules, entries, vec![]);

        assert_eq!(
            rows(&findings),
            vec![
                ("required_field", "warning", "kb_b"),
                ("required_field", "warning", "kb_c"),
            ],
            "empty and absent tags match; non-empty does not"
        );
    }

    // ── required_field (module-row-level) ──────────────────────────────

    #[test]
    fn required_field_module_row_violation_matches() {
        let rules = vec![rule(
            "rul_row",
            "Rows need lineage",
            None,
            &[],
            &json!({ "family": "required_field", "module_key": "belief", "field": "source" }),
        )];
        let entries = vec![
            // All rows populated → no match.
            entry(
                "kb_a",
                "A",
                "character",
                &json!({}),
                &json!({ "belief": [{ "source": "Perception" }, { "source": "Gossip" }] }),
            ),
            // One object row missing the field → match (one finding per entry).
            entry(
                "kb_b",
                "B",
                "character",
                &json!({}),
                &json!({ "belief": [{ "source": "Perception" }, { "holder": "world" }] }),
            ),
            // Non-object rows skipped; all object rows populated → no match.
            entry(
                "kb_c",
                "C",
                "character",
                &json!({}),
                &json!({ "belief": ["a string row", { "source": "Perception" }] }),
            ),
            // Object-form module (not array) → vacuous pass.
            entry(
                "kb_d",
                "D",
                "character",
                &json!({}),
                &json!({ "belief": {} }),
            ),
            // Absent module → vacuous pass (module_presence's job).
            entry("kb_e", "E", "character", &json!({}), &json!({})),
            // Zero rows → vacuous pass.
            entry(
                "kb_f",
                "F",
                "character",
                &json!({}),
                &json!({ "belief": [] }),
            ),
        ];
        let findings = run(rules, entries, vec![]);

        assert_eq!(
            rows(&findings),
            vec![("required_field", "warning", "kb_b")],
            "only the entry with an object row missing the field matches"
        );
    }

    #[test]
    fn required_field_module_row_non_string_populated_and_null() {
        let rules = vec![rule(
            "rul_row2",
            "Rows need depth",
            None,
            &[],
            &json!({ "family": "required_field", "module_key": "belief", "field": "depth" }),
        )];
        let entries = vec![
            // Numeric value → populated (non-null, non-string).
            entry(
                "kb_a",
                "A",
                "character",
                &json!({}),
                &json!({ "belief": [{ "depth": 3 }] }),
            ),
            // Explicit null → NOT populated.
            entry(
                "kb_b",
                "B",
                "character",
                &json!({}),
                &json!({ "belief": [{ "depth": null }] }),
            ),
            // Empty-string → NOT populated (string trimmed empty).
            entry(
                "kb_c",
                "C",
                "character",
                &json!({}),
                &json!({ "belief": [{ "depth": "  " }] }),
            ),
        ];
        let findings = run(rules, entries, vec![]);

        assert_eq!(
            rows(&findings),
            vec![
                ("required_field", "warning", "kb_b"),
                ("required_field", "warning", "kb_c"),
            ]
        );
    }

    // ── observer_cardinality ───────────────────────────────────────────

    #[test]
    fn observer_cardinality_matches_out_of_range_recorded_observation() {
        let rules = vec![rule(
            "rul_obs",
            "Observer cap",
            None,
            &["character"], // ignored for observer_cardinality
            &json!({ "family": "observer_cardinality", "min": 0, "max": 1 }),
        )];
        let events = vec![
            event(
                "evt_a",
                "Two observers",
                &json!({ "observation": { "observers": ["kb_a", "kb_b"] } }),
            ),
            event(
                "evt_b",
                "One observer",
                &json!({ "observation": { "observers": ["kb_a"] } }),
            ),
        ];
        let findings = run(vec![], vec![], events.clone());
        // sanity: no rules → nothing
        assert!(findings.is_empty(), "empty rules ⇒ no findings");
        let findings = run(rules, vec![], events);

        assert_eq!(
            rows(&findings),
            vec![("observer_cardinality", "warning", "evt_a")],
            "2 observers outside [0,1] matches; 1 observer inside does not"
        );
        assert_eq!(findings[0].title, "observer_cardinality: Two observers");
    }

    #[test]
    fn observer_cardinality_unbounded_bounds_and_min_only() {
        // min-only (no max): count >= 2 required.
        let rules = vec![rule(
            "rul_obs_min",
            "At least two",
            None,
            &[],
            &json!({ "family": "observer_cardinality", "min": 2 }),
        )];
        let events = vec![
            event(
                "evt_a",
                "One",
                &json!({ "observation": { "observers": ["kb_a"] } }),
            ),
            event(
                "evt_b",
                "Two",
                &json!({ "observation": { "observers": ["kb_a", "kb_b"] } }),
            ),
        ];
        let findings = run(rules, vec![], events);

        assert_eq!(
            rows(&findings),
            vec![("observer_cardinality", "warning", "evt_a")],
            "min-only bound: one observer violates"
        );
    }

    #[test]
    fn observer_cardinality_absent_observation_never_matches() {
        // PD-9 via the shared parse: absent/malformed observation is not a
        // candidate — unrecorded ≠ nobody.
        let rules = vec![rule(
            "rul_obs",
            "Max zero",
            None,
            &[],
            &json!({ "family": "observer_cardinality", "min": 0, "max": 0 }),
        )];
        let events = vec![
            event("evt_a", "No modules", &json!({})),
            event(
                "evt_b",
                "Observation without observers",
                &json!({ "observation": {} }),
            ),
            event(
                "evt_c",
                "Malformed observers",
                &json!({ "observation": { "observers": ["kb_a", 42] } }),
            ),
        ];
        let findings = run(rules, vec![], events);

        assert!(
            findings.is_empty(),
            "absent/malformed observation never matches even against max 0: {findings:?}"
        );
    }

    // ── severity mapping + identity ────────────────────────────────────

    #[test]
    fn severity_hint_verbatim_else_uniform_warning() {
        let rules = vec![
            rule(
                "rul_sev",
                "Sev error",
                Some("info"),
                &[],
                &json!({ "family": "module_presence", "module_key": "x" }),
            ),
            rule(
                "rul_nosev",
                "No sev",
                None,
                &[],
                &json!({ "family": "module_presence", "module_key": "x" }),
            ),
        ];
        let entries = vec![
            entry("kb_a", "A", "character", &json!({}), &json!({})),
            entry("kb_b", "B", "character", &json!({}), &json!({})),
        ];
        let findings = run(rules, entries, vec![]);

        assert_eq!(
            rows(&findings),
            vec![
                ("module_presence", "info", "kb_a"),
                ("module_presence", "info", "kb_b"),
                ("module_presence", "warning", "kb_a"),
                ("module_presence", "warning", "kb_b"),
            ],
            "severity_hint verbatim ('info' — spoke vocabulary, not Work vocab); \
             absent hint → uniform 'warning'"
        );
    }

    #[test]
    fn finding_ids_are_fnd_family_uuid_simple() {
        let rules = vec![rule(
            "rul_id",
            "Id shape",
            None,
            &[],
            &json!({ "family": "module_presence", "module_key": "x" }),
        )];
        let findings = run(
            rules,
            vec![entry("kb_a", "A", "character", &json!({}), &json!({}))],
            vec![],
        );

        assert_eq!(findings.len(), 1);
        let id = &findings[0].finding_id;
        assert!(
            id.starts_with("fnd_module_presence_"),
            "finding_id prefix: {id}"
        );
        assert_eq!(
            id.len(),
            "fnd_module_presence_".len() + 32,
            "uuid v4 simple: {id}"
        );
        let hex = &id["fnd_module_presence_".len()..];
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "uuid simple is hex-only: {id}"
        );
    }

    // ── carrier skip + empty rules ─────────────────────────────────────

    #[test]
    fn rule_without_carrier_is_skipped() {
        let findings = run(
            vec![rule_without_carrier()],
            vec![entry("kb_a", "A", "character", &json!({}), &json!({}))],
            vec![],
        );
        assert!(findings.is_empty(), "no carrier ⇒ rule skipped");
    }

    #[test]
    fn malformed_carrier_rule_is_skipped() {
        // Unknown-family carrier → `constraint_from_rule` yields None (the
        // CLI gate rejects at authoring; the evaluator read is lenient).
        let r = serde_json::from_value(json!({
            "schema_version": 1,
            "rule_id": "rul_bad",
            "canonical_name": "Bad carrier",
            "kind": "rule",
            "extensions": { "nexus": { "constraint": { "family": "tone", "tone": "grim" } } },
            "status": "active",
            "target_entry_types": [],
        }))
        .expect("fixture Rule is well-formed");
        let findings = run(
            vec![r],
            vec![entry("kb_a", "A", "character", &json!({}), &json!({}))],
            vec![],
        );
        assert!(
            findings.is_empty(),
            "malformed carrier ⇒ rule skipped (lenient read, AR-2)"
        );
    }

    #[test]
    fn empty_rule_set_is_emergent_noop() {
        let findings = run(
            vec![],
            vec![entry("kb_a", "A", "character", &json!({}), &json!({}))],
            vec![event(
                "evt_a",
                "E",
                &json!({ "observation": { "observers": ["kb_a"] } }),
            )],
        );
        assert!(findings.is_empty(), "empty rules ⇒ no rule findings");
    }

    /// The `Constraint` type + `family()` round-trip (carrier semantics
    /// shared with the CLI gate).
    #[test]
    fn family_names_are_the_closed_pd1_set() {
        for (carrier, expected) in [
            (
                json!({ "family": "module_presence", "module_key": "a" }),
                "module_presence",
            ),
            (
                json!({ "family": "module_absence", "module_key": "a" }),
                "module_absence",
            ),
            (
                json!({ "family": "required_field", "field": "body.summary" }),
                "required_field",
            ),
            (
                json!({ "family": "observer_cardinality", "min": 0 }),
                "observer_cardinality",
            ),
        ] {
            let c: Constraint = nexus_spoke_adapter::constraint::parse_carrier_json(&carrier)
                .unwrap_or_else(|e| panic!("carrier {carrier} must parse: {e}"));
            assert_eq!(c.family(), expected);
        }
    }
}
