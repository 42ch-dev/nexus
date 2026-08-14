//! Mental-layer checker pair (V1.164 P2 T3, l5-mind): `stale_belief_drift`
//! + `dramatic_irony_asymmetry`.
//!
//! Replaces the baseline no-op evaluator at the `orchestrate_check` callback
//! (the daemon check handler). One callback emits both Finding kinds from
//! the belief dialect Task 1 landed on `KnowledgeEntry.modules.belief`
//! (handbook field names — `holder` / `proposition` / `order` / `truth` /
//! `access` / `source`; never paper aliases, TL-5) and the event observation
//! dialect P1 landed on `TimelineEvent.modules.observation` (l5-mind
//! observation metadata).
//!
//! ## Detection is truth-label-driven (NOT string matching)
//!
//! The handbook worked example uses *different* proposition texts for the
//! world row ("The marble is in the basket") and the actor row ("The marble
//! is in the box") — the actor row's own `truth: "False"` label IS the
//! divergence signal. The checker therefore never matches actor rows to
//! world rows; each actor row (`holder != "world"`, `order == 1`,
//! `truth == "False"`) is a **false-belief candidate** on its own.
//!
//! ## Classification via the informing event
//!
//! V1.164 informing-event rule (documented heuristic; fixture-sized worlds):
//! the latest narrative timeline event in the check scope by `sequence_no`.
//! Tie-break (explicit): the gateway event stream sorts by `branch_id ASC,
//! sequence_no ASC` (no-branch ordering), and the checker's stable
//! `max_by_key` picks the LAST element on equal `sequence_no` — so
//! same-sequence events resolve deterministically to the later list
//! position. Relevance matching (proposition ↔ event semantics) is an
//! explicit non-goal with a durable-roadmap entry (plan roadmap).
//!
//! - Informing event **has** `modules.observation` and the actor **IS** in
//!   `observers` → **`stale_belief_drift`** (severity `warning`) — the
//!   actor should have seen the informing event, so the stale belief is a
//!   consistency bug (branch a).
//! - Informing event **has** `modules.observation` and the actor is **NOT**
//!   in `observers` → **`dramatic_irony_asymmetry`** (severity `info`) — the
//!   correct stale structure: the actor did not observe, so the divergent
//!   belief is deliberate (the box/basket thesis).
//! - Informing event **exists** but `modules.observation` is **absent** →
//!   **skip** (unrecorded ≠ nobody — PD-9, AC-V1164-11). An observation
//!   record whose `observers` list is missing/malformed is also skipped
//!   (an unknown observer set must not be treated as "nobody" — same
//!   PD-9 principle; `simplify:` candidate if the handbook ever defines a
//!   default for that shape).
//! - **No events** in the check scope → **`stale_belief_drift`** (branch b
//!   — the false belief has no recorded informational basis).
//!
//! ## Finding contract
//!
//! - Severity uses the spoke Finding vocabulary (`info` / `warning` /
//!   `error`) — never Nexus Work-finding `minor`/`major`/`blocker` (PD-15).
//! - `extensions.nexus.world_id` is the persistence routing key:
//!   `FindingPort::put_findings` discriminates on `extensions.nexus`
//!   (`finding_port.rs`): findings carrying `world_id` (no `work_id`) persist
//!   into the `world_findings` table with spoke vocabulary verbatim; findings
//!   carrying `work_id` keep the legacy `findings` work path (byte-identical,
//!   mapping unchanged). `creator_id` is still stamped (provenance — the
//!   world path has no creator column, AR-1). The world id is
//!   `request.scope.scope_id` (the handler already validated it equals
//!   `world_id`).
//! - The checker is advisory and pure (no store access): the callback input
//!   carries the scoped `entries` AND the scoped `events` (`CheckRunInput`),
//!   so classification never reads the database (verified against
//!   spoke-operations 0.10.0 `orchestrate_check` — `events` is a field of
//!   `CheckRunInput`).
//!
//! ## Informing-event ordering source
//!
//! The nexus→spoke event seam derives `TimelineEvent.sort_key` from
//! `sequence_no` (`nexus-narrative::timeline_event.rs` `From<TimelineEvent>
//! for SpokeTimelineEvent`: `sort_key: Some(sequence_no.to_string())`), and
//! the adapter boundary rule keeps this crate off `spoke-schemas` newtypes,
//! so the checker parses `sort_key` as `u64` for true numeric ordering.

use nexus_knowledge::world_kb::BeliefPropositionRaw;
use nexus_spoke_adapter::{
    CheckRunInput, Finding, FindingExtensionsKey, KnowledgeEntry, SpokeResult, TimelineEvent,
};
use serde_json::Value;
use std::collections::HashMap;

/// Finding kinds this checker emits (handbook-exact strings).
pub const KIND_STALE_BELIEF_DRIFT: &str = "stale_belief_drift";
/// Finding kinds this checker emits (handbook-exact strings).
pub const KIND_DRAMATIC_IRONY_ASYMMETRY: &str = "dramatic_irony_asymmetry";

/// Spoke Finding severities (PD-15 — never nexus Work-finding severities).
const SEVERITY_WARNING: &str = "warning";
/// Spoke Finding severities (PD-15 — never nexus Work-finding severities).
const SEVERITY_INFO: &str = "info";

/// Run the mental-layer checker over a scoped `orchestrate_check` input.
///
/// Collects `modules.belief` rows from the entry set (Task 1
/// [`BeliefPropositionRaw`] parse — handbook field names), classifies each
/// false-belief candidate via the informing event (latest by `sequence_no`),
/// and emits the checker pair findings. See the module docs for the branch
/// table and the PD-9 skip rule.
///
/// `creator_id` is the active creator (the handler already read it); it is
/// stamped onto `extensions.nexus` as provenance (the world path has no
/// creator column — see module docs; `world_id` is the routing key).
#[must_use]
pub fn run_check(input: &CheckRunInput, creator_id: &str) -> SpokeResult<Vec<Finding>> {
    let world_id = &input.request.scope.scope_id;
    let mut findings: Vec<Finding> = Vec::new();

    for entry in &input.entries {
        for row in belief_rows(entry) {
            let Some(actor) = candidate_actor(&row) else {
                continue;
            };
            for finding in classify(&row, actor, world_id, creator_id, &input.events) {
                findings.push(finding);
            }
        }
    }

    SpokeResult::Ok(findings)
}

/// Typed view of the entry's `modules.belief` rows (Task 1 parse semantics:
/// absent / non-array module → empty; non-object rows skipped).
fn belief_rows(entry: &KnowledgeEntry) -> Vec<BeliefPropositionRaw> {
    let Ok(Value::Object(map)) = serde_json::to_value(&entry.modules) else {
        return Vec::new();
    };
    let Some(Value::Array(rows)) = map.get("belief") else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| serde_json::from_value(row.clone()).ok())
        .collect()
}

/// A false-belief candidate: `holder != "world"`, `order == 1`,
/// `truth == "False"` (truth-label-driven — no world-row comparison).
/// Returns the actor entry id (`holder`) when the row is a candidate.
fn candidate_actor(row: &BeliefPropositionRaw) -> Option<&str> {
    let holder = row.holder.as_deref()?;
    if holder == "world" || row.order != Some(1) || row.truth.as_deref() != Some("False") {
        return None;
    }
    Some(holder)
}

/// The informing event: the latest event in the check scope by `sequence_no`
/// (V1.164 heuristic; relevance matching is a roadmap non-goal).
/// Events without a parseable `sequence_no` sort as earliest; when every
/// event is unparseable the last list element wins (deterministic for the
/// scope-filtered input).
fn informing_event(events: &[TimelineEvent]) -> Option<&TimelineEvent> {
    events.iter().max_by_key(|event| event_sequence_no(event))
}

/// `sequence_no` of an event — rides `sort_key` on the spoke seam (the
/// nexus→spoke conversion emits `sequence_no.to_string()`; the adapter
/// boundary keeps this crate off spoke-schemas newtypes).
fn event_sequence_no(event: &TimelineEvent) -> Option<u64> {
    event
        .sort_key
        .as_deref()
        .and_then(|key| key.parse::<u64>().ok())
}

/// The informing event's observer list (`modules.observation.observers`).
///
/// `None` when the observation module is absent OR the observer list is
/// missing/malformed — both are skipped by the classifier (PD-9: unrecorded
/// and unknown observer sets are never treated as "nobody"). A single
/// non-string element makes the whole set unknown (never partial — silently
/// dropping non-strings could flip drift ↔ irony).
fn observation_observers(event: &TimelineEvent) -> Option<Vec<String>> {
    let Ok(Value::Object(map)) = serde_json::to_value(&event.modules) else {
        return None;
    };
    let Value::Object(observation) = map.get("observation")? else {
        return None;
    };
    let Value::Array(observers) = observation.get("observers")? else {
        return None;
    };
    // PD-9: any non-string element → the observer set is unknown → the whole
    // set is malformed (collect into Option: one None makes the result None).
    observers
        .iter()
        .map(|observer| observer.as_str().map(str::to_owned))
        .collect()
}

/// Classify one false-belief candidate into zero or one Finding (the branch
/// table in the module docs).
fn classify(
    row: &BeliefPropositionRaw,
    actor: &str,
    world_id: &str,
    creator_id: &str,
    events: &[TimelineEvent],
) -> Vec<Finding> {
    let proposition = row.proposition.as_deref().unwrap_or("(unstated)");
    let Some(informing) = informing_event(events) else {
        // Branch b: no events in the check scope — the false belief has no
        // recorded informational basis (consistency bug).
        return vec![finding(
            KIND_STALE_BELIEF_DRIFT,
            SEVERITY_WARNING,
            actor,
            world_id,
            creator_id,
            format!(
                "{actor} holds the false belief '{proposition}' (truth: False) and no \
                 narrative timeline event in the check scope records an informational \
                 basis for it — stale belief drift"
            ),
        )];
    };

    let Some(observers) = observation_observers(informing) else {
        // PD-9 (AC-V1164-11): the informing event exists but the observation
        // is unrecorded — skip, never treat as nobody/everybody.
        return Vec::new();
    };

    let event_name = event_name(informing);
    if observers.iter().any(|observer| observer == actor) {
        // Branch a: the actor should have seen the informing event — the
        // stale belief is a consistency bug.
        vec![finding(
            KIND_STALE_BELIEF_DRIFT,
            SEVERITY_WARNING,
            actor,
            world_id,
            creator_id,
            format!(
                "{actor} holds the false belief '{proposition}' (truth: False) but is \
                 listed as an observer of the informing event '{event_name}' ({}) — the \
                 belief should have been revised (stale belief drift)",
                informing.timeline_event_id
            ),
        )]
    } else {
        // The correct stale structure: the actor did not observe the
        // informing event — the divergent belief is deliberate (irony).
        vec![finding(
            KIND_DRAMATIC_IRONY_ASYMMETRY,
            SEVERITY_INFO,
            actor,
            world_id,
            creator_id,
            format!(
                "{actor} retains the false belief '{proposition}' (truth: False) and was \
                 not an observer of the informing event '{event_name}' ({}) — a \
                 deliberate stale structure (dramatic irony asymmetry)",
                informing.timeline_event_id
            ),
        )]
    }
}

/// Human-readable event label for descriptions — the event's `canonical_name`.
/// The title → summary → event-id fallback chain lives at the nexus→spoke
/// seam (`nexus-narrative::timeline_event` `From<TimelineEvent>`), where
/// `canonical_name` is populated; this function performs no fallback itself.
fn event_name(event: &TimelineEvent) -> String {
    event.canonical_name.as_str().to_string()
}

/// Build a spoke `Finding` ready for `FindingPort::put_findings` (stamps
/// `extensions.nexus.world_id` — the routing key — plus `creator_id` as
/// provenance).
fn finding(
    kind: &str,
    severity: &str,
    actor: &str,
    world_id: &str,
    creator_id: &str,
    description: String,
) -> Finding {
    let mut nexus = serde_json::Map::new();
    // `world_id` is the routing key injected here — `put_findings`
    // discriminates on `extensions.nexus` (finding_port.rs): `world_id`
    // (no `work_id`) → world path. `creator_id` rides along as provenance
    // (AR-1 — no creator column on the world path).
    nexus.insert("world_id".to_string(), Value::String(world_id.to_string()));
    nexus.insert(
        "creator_id".to_string(),
        Value::String(creator_id.to_string()),
    );
    let mut extensions = HashMap::new();
    extensions.insert(
        FindingExtensionsKey::try_from("nexus")
            .expect("\"nexus\" matches the ^[a-z][a-z0-9_-]*$ namespace regex"),
        nexus,
    );
    Finding {
        created_at: None,
        description,
        extensions,
        finding_id: format!("fnd_{kind}_{}", uuid::Uuid::new_v4().simple()),
        kind: Some(kind.to_string()),
        schema_version: std::num::NonZeroU64::new(1).expect("1 is non-zero"),
        severity: severity.to_string(),
        source_anchor: None,
        status: "open".to_string(),
        suggested_fix: None,
        target_entry_id: Some(actor.to_string()),
        text_position: serde_json::Map::new(),
        title: format!("{kind}: {actor}"),
        updated_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Spoke handbook worked-example fixtures (box/basket story) ──────
    //
    // Seeded onto the designated world-state `info_point` (AR-1) carrying
    // `holder: "world"` rows, `kb_ana` / `kb_bo` character entries, and the
    // `evt_transfer` narrative timeline event. Fixture A–D map to
    // AC-V1164-9 / AC-V1164-10 / AC-V1164-11 plus the negative case.
    //
    // Fixtures deserialize the spoke wire types from JSON (the typify
    // newtypes like `KnowledgeEntryCanonicalName` are not re-exported across
    // the adapter boundary); the wire shape is exactly what the checker's
    // `CheckRunInput` carries.

    fn entry(id: &str, name: &str, entry_type: &str, modules: &Value) -> KnowledgeEntry {
        serde_json::from_value(json!({
            "body": {},
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

    /// The world-state `info_point` (AR-1): narrated facts, `holder: "world"`.
    fn world_entry() -> KnowledgeEntry {
        entry(
            "kb_world",
            "World State",
            "info_point",
            &json!({
                "belief": [
                    { "holder": "world", "proposition": "The marble is in the basket",
                      "order": 0, "truth": "True" },
                    { "holder": "world", "proposition": "Bo left the room",
                      "order": 0, "truth": "True" }
                ]
            }),
        )
    }

    /// `kb_ana`: shared true belief — never a candidate.
    fn kb_ana_entry() -> KnowledgeEntry {
        entry(
            "kb_ana",
            "Ana",
            "character",
            &json!({
                "belief": [
                    { "holder": "kb_ana", "proposition": "The marble is in the basket",
                      "order": 1, "truth": "True", "access": "Shared" }
                ]
            }),
        )
    }

    /// `kb_bo`: actor row parameterized by `truth` (fixture D flips to True).
    fn kb_bo_entry(truth: &str) -> KnowledgeEntry {
        entry(
            "kb_bo",
            "Bo",
            "character",
            &json!({
                "belief": [
                    { "holder": "kb_bo", "proposition": "The marble is in the box",
                      "order": 1, "truth": truth, "access": "Private", "source": "Perception" }
                ]
            }),
        )
    }

    /// `evt_transfer` with an arbitrary `modules` payload (for malformed
    /// observation shapes — non-string observer elements).
    fn evt_transfer_modules(sequence_no: u64, modules: &Value) -> TimelineEvent {
        serde_json::from_value(json!({
            "canonical_name": "Marble transfer",
            "timeline_event_id": "evt_transfer",
            "extensions": {},
            "schema_version": 1,
            "sort_key": sequence_no.to_string(),
            "modules": modules,
        }))
        .expect("fixture TimelineEvent is well-formed")
    }

    /// `evt_transfer`: the narrative timeline event carrying the optional
    /// observation module. `observers: None` → no `modules` at all (fixture C).
    fn evt_transfer(sequence_no: u64, observers: Option<Vec<&str>>) -> TimelineEvent {
        let modules = observers.map_or_else(
            || json!({}),
            |list| json!({ "observation": { "observers": list } }),
        );
        evt_transfer_modules(sequence_no, &modules)
    }

    fn check_input(entries: Vec<KnowledgeEntry>, events: Vec<TimelineEvent>) -> CheckRunInput {
        CheckRunInput {
            request: serde_json::from_value(json!({
                "scope": { "scope_id": "wld_fixture" },
            }))
            .expect("fixture CheckRequest is well-formed"),
            entries,
            events,
            rules: Vec::new(),
        }
    }

    fn findings_by_kind(findings: &[Finding]) -> Vec<(&str, &str)> {
        findings
            .iter()
            .map(|f| {
                (
                    f.kind.as_deref().unwrap_or("(none)"),
                    f.target_entry_id.as_deref().unwrap_or("(none)"),
                )
            })
            .collect()
    }

    /// Fixture A (irony — the thesis, AC-V1164-9): `evt_transfer` observed
    /// by `kb_ana` only → `dramatic_irony_asymmetry` on `kb_bo`; NO drift
    /// for `kb_bo` (`kb_ana`'s truth-True row is not a candidate).
    #[test]
    fn fixture_a_irony_when_actor_absent_from_observers() {
        let input = check_input(
            vec![world_entry(), kb_ana_entry(), kb_bo_entry("False")],
            vec![evt_transfer(1, Some(vec!["kb_ana"]))],
        );

        let findings = match run_check(&input, "ctr_test") {
            SpokeResult::Ok(findings) => findings,
            SpokeResult::Reject(reject) => panic!("checker must not reject: {reject:?}"),
        };

        assert_eq!(
            findings_by_kind(&findings),
            vec![(KIND_DRAMATIC_IRONY_ASYMMETRY, "kb_bo")],
            "exactly one irony finding on kb_bo, no drift"
        );
        let irony = &findings[0];
        assert_eq!(irony.severity, SEVERITY_INFO);
        assert_eq!(irony.status, "open");
        assert_eq!(irony.target_entry_id.as_deref(), Some("kb_bo"));
        assert_eq!(irony.kind.as_deref(), Some(KIND_DRAMATIC_IRONY_ASYMMETRY));
        // Description names the actor, the belief proposition, and the
        // informing event (brief step 1.4).
        assert!(
            irony.description.contains("kb_bo"),
            "description names the actor: {}",
            irony.description
        );
        assert!(
            irony.description.contains("The marble is in the box"),
            "description names the belief proposition: {}",
            irony.description
        );
        assert!(
            irony.description.contains("evt_transfer"),
            "description names the informing event: {}",
            irony.description
        );
        // The Finding must carry the nexus-required persistence fields.
        let nexus = irony
            .extensions
            .get(&FindingExtensionsKey::try_from("nexus").expect("nexus key validates"));
        assert!(
            nexus.is_some(),
            "extensions.nexus must be present for put_findings"
        );
    }

    /// Fixture B (drift — the bug, AC-V1164-10): `evt_transfer` observed by
    /// `kb_ana` AND `kb_bo` → `stale_belief_drift` on `kb_bo`; NO irony.
    #[test]
    fn fixture_b_drift_when_actor_in_observers() {
        let input = check_input(
            vec![world_entry(), kb_ana_entry(), kb_bo_entry("False")],
            vec![evt_transfer(1, Some(vec!["kb_ana", "kb_bo"]))],
        );

        let findings = match run_check(&input, "ctr_test") {
            SpokeResult::Ok(findings) => findings,
            SpokeResult::Reject(reject) => panic!("checker must not reject: {reject:?}"),
        };

        assert_eq!(
            findings_by_kind(&findings),
            vec![(KIND_STALE_BELIEF_DRIFT, "kb_bo")],
            "exactly one drift finding on kb_bo, no irony"
        );
        assert_eq!(findings[0].severity, SEVERITY_WARNING);
        assert!(
            findings[0].description.contains("observer"),
            "drift description names the observed informing event: {}",
            findings[0].description
        );
    }

    /// Fixture C (absent observation, AC-V1164-11): `evt_transfer` with NO
    /// `modules` → unrecorded ≠ nobody — skip observation-dependent Findings.
    #[test]
    fn fixture_c_absent_observation_skips() {
        let input = check_input(
            vec![world_entry(), kb_ana_entry(), kb_bo_entry("False")],
            vec![evt_transfer(1, None)],
        );

        let findings = match run_check(&input, "ctr_test") {
            SpokeResult::Ok(findings) => findings,
            SpokeResult::Reject(reject) => panic!("checker must not reject: {reject:?}"),
        };

        assert!(
            findings.is_empty(),
            "no observation-dependent finding when observation is unrecorded: {findings:?}"
        );
    }

    /// Fixture D (negative): `kb_bo`'s row is `truth: "True"` → not a
    /// candidate → no Finding of either kind.
    #[test]
    fn fixture_d_truth_true_is_not_a_candidate() {
        let input = check_input(
            vec![world_entry(), kb_ana_entry(), kb_bo_entry("True")],
            vec![evt_transfer(1, Some(vec!["kb_ana", "kb_bo"]))],
        );

        let findings = match run_check(&input, "ctr_test") {
            SpokeResult::Ok(findings) => findings,
            SpokeResult::Reject(reject) => panic!("checker must not reject: {reject:?}"),
        };

        assert!(
            findings.is_empty(),
            "a truth-True actor row is not a false-belief candidate: {findings:?}"
        );
    }

    /// Malformed observer set with a non-string element (S-1 fix):
    /// `['kb_ana', 42]` must NOT silently truncate to `['kb_ana']` (which
    /// could flip drift ↔ irony) — the whole set is unknown → skip (PD-9).
    #[test]
    fn malformed_observers_with_non_string_element_skip() {
        let input = check_input(
            vec![world_entry(), kb_ana_entry(), kb_bo_entry("False")],
            vec![evt_transfer_modules(
                1,
                &json!({ "observation": { "observers": ["kb_ana", 42] } }),
            )],
        );

        let findings = match run_check(&input, "ctr_test") {
            SpokeResult::Ok(findings) => findings,
            SpokeResult::Reject(reject) => panic!("checker must not reject: {reject:?}"),
        };

        assert!(
            findings.is_empty(),
            "a non-string observer element makes the set unknown — must skip: {findings:?}"
        );
    }

    /// All-non-string observer list — equally unknown, must skip.
    #[test]
    fn all_non_string_observers_skip() {
        let input = check_input(
            vec![world_entry(), kb_ana_entry(), kb_bo_entry("False")],
            vec![evt_transfer_modules(
                1,
                &json!({ "observation": { "observers": [42] } }),
            )],
        );

        let findings = match run_check(&input, "ctr_test") {
            SpokeResult::Ok(findings) => findings,
            SpokeResult::Reject(reject) => panic!("checker must not reject: {reject:?}"),
        };

        assert!(
            findings.is_empty(),
            "a fully non-string observer list is unknown — must skip: {findings:?}"
        );
    }

    /// Branch b (brief step 1.3): no events in the check scope → the false
    /// belief has no recorded informational basis → `stale_belief_drift`
    /// even though the actor was not observed failing anything.
    #[test]
    fn no_events_in_scope_is_drift_branch_b() {
        let input = check_input(
            vec![world_entry(), kb_ana_entry(), kb_bo_entry("False")],
            vec![],
        );

        let findings = match run_check(&input, "ctr_test") {
            SpokeResult::Ok(findings) => findings,
            SpokeResult::Reject(reject) => panic!("checker must not reject: {reject:?}"),
        };

        assert_eq!(
            findings_by_kind(&findings),
            vec![(KIND_STALE_BELIEF_DRIFT, "kb_bo")],
            "no informing event → drift (branch b)"
        );
        assert_eq!(findings[0].severity, SEVERITY_WARNING);
        assert!(
            findings[0]
                .description
                .contains("no narrative timeline event"),
            "branch-b description names the absent basis: {}",
            findings[0].description
        );
    }

    /// The informing-event rule is latest-by-`sequence_no`: a later event
    /// (whether or not it carries an observation) is the informing event.
    /// Here the later event observes `kb_bo` → drift, even though the
    /// earlier event did not observe `kb_bo` (would have been irony).
    #[test]
    fn informing_event_is_latest_by_sequence_no() {
        let input = check_input(
            vec![world_entry(), kb_ana_entry(), kb_bo_entry("False")],
            vec![
                evt_transfer(1, Some(vec!["kb_ana"])),
                evt_transfer(2, Some(vec!["kb_ana", "kb_bo"])),
            ],
        );

        let findings = match run_check(&input, "ctr_test") {
            SpokeResult::Ok(findings) => findings,
            SpokeResult::Reject(reject) => panic!("checker must not reject: {reject:?}"),
        };

        assert_eq!(
            findings_by_kind(&findings),
            vec![(KIND_STALE_BELIEF_DRIFT, "kb_bo")],
            "the latest event (sequence 2) is the informing event"
        );
    }
}
