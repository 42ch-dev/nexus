//! Product checkers invoked from the spoke `orchestrate_check` callback
//! (the daemon `POST /v1/daemon/check` surface).
//!
//! The callback receives a `CheckRunInput` (`request` + scoped `entries` +
//! scoped `events` + resolved `rules`) and returns the `Finding`s the
//! orchestrator persists via `FindingPort::put_findings` (V1.148 P2 — the
//! check op became daemon-reachable through `api::handlers::check`).
//!
//! V1.164 P2 T3 replaced the baseline no-op evaluator with the mental-layer
//! checker pair (see [`mental`]); V1.166 (DR-64, AR-4) composes the
//! four-family structured-rule evaluator (see [`rules_eval`]) beside it via
//! [`run_all`] — the handler closure both production callers pass to
//! [`orchestrate_check_world_scoped`](nexus_spoke_adapter::orchestrate_check_world_scoped).
//!
//! # Shared stamp/PD-9 single sources (AR-4)
//!
//! The finding stamp builder ([`finding`]) and the PD-9 observers parse
//! ([`observation_observers`]) live here as `pub(crate)` single sources —
//! moved verbatim from the mental checker (V1.164 PD-11 freeze: mental
//! behavior untouched, the mental suite is the regression proof). Both
//! evaluators consume them so stamp/PD-9 semantics cannot drift (one
//! semantics, two consumers).

pub mod mental;
pub mod rules_eval;

use nexus_spoke_adapter::{Finding, FindingExtensionsKey, SpokeResult, TimelineEvent};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// V1.166 AR-4: composed product checker — mental pair (V1.164, frozen)
/// ∪ rule evaluator (V1.166 PD-1) over the same `CheckRunInput`.
///
/// The mental checker runs first and its `SpokeResult` short-circuit is
/// preserved (a reject never reaches the rule evaluator); rule findings
/// extend the mental findings in rule order. (The AR-4 lock sketches this
/// with `?`; `SpokeResult` is a plain enum without `Try`, so the match
/// below is the representable form of the same semantics.)
#[must_use]
pub fn run_all(
    input: &nexus_spoke_adapter::CheckRunInput,
    creator_id: &str,
) -> nexus_spoke_adapter::SpokeResult<Vec<Finding>> {
    let mut findings = match mental::run_check(input, creator_id) {
        SpokeResult::Ok(findings) => findings,
        reject @ SpokeResult::Reject(_) => return reject,
    };
    match rules_eval::run_check(input, creator_id) {
        SpokeResult::Ok(rule_findings) => findings.extend(rule_findings),
        reject @ SpokeResult::Reject(_) => return reject,
    }
    SpokeResult::Ok(findings)
}

/// The informing event's observer list (`modules.observation.observers`) —
/// the **shared** PD-9 parse (AR-4 single source).
///
/// `None` when the observation module is absent OR the observer list is
/// missing/malformed — both are skipped by the consumers (PD-9: unrecorded
/// and unknown observer sets are never treated as "nobody"). A single
/// non-string element makes the whole set unknown (never partial — silently
/// dropping non-strings could flip drift ↔ irony in the mental classifier or
/// fabricate a cardinality match for the rule evaluator).
#[must_use]
pub(crate) fn observation_observers(event: &TimelineEvent) -> Option<Vec<String>> {
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

/// Build a spoke `Finding` ready for `FindingPort::put_findings` — the
/// **shared** stamp builder (AR-4 single source; moved from the mental
/// checker verbatim).
///
/// Stamps `extensions.nexus.world_id` (the routing key — `put_findings`
/// discriminates on `extensions.nexus` in `finding_port.rs`: `world_id`
/// without `work_id` → world path) plus `creator_id` as provenance (AR-1 —
/// no creator column on the world path).
///
/// `target_entry_id` is the violating entry/event id; `title_label` is the
/// human label used in the title (`{kind}: {label}`). The mental pair passes
/// the same value for both (the actor entry id — historical title shape);
/// the rule evaluator passes the entry/event `canonical_name` (AR-4 table).
#[must_use]
pub(crate) fn finding(
    kind: &str,
    severity: &str,
    target_entry_id: &str,
    title_label: &str,
    world_id: &str,
    creator_id: &str,
    description: String,
) -> Finding {
    let mut nexus = Map::new();
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
        target_entry_id: Some(target_entry_id.to_string()),
        text_position: Map::new(),
        title: format!("{kind}: {title_label}"),
        updated_at: None,
    }
}
