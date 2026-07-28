//! # Spoke 0.4.1 timeline helper adoption (test-adoption, compass AC-I2)
//!
//! This is a **test-adoption**: it proves the spoke 0.4.1 timeline beat-assist
//! helpers are reachable from `nexus-narrative` through the
//! `nexus-spoke-adapter` boundary, and that they behave per spoke 0.4.1
//! semantics (cross-checked against `spoke-operations`' own `timeline.rs`
//! test cases).
//!
//! It does **not** change runtime behavior. The `TimelineEvent` instances used
//! here are spoke wire types (`nexus_spoke_adapter::TimelineEvent`, re-exported
//! from `spoke_schemas`), constructed for this test via `serde_json::from_value`
//! with fixture-style JSON mirroring spoke's `toy-world` event fixtures.
//!
//! `nexus-narrative` still has its **own** `TimelineEvent` aggregate, which is
//! deliberately untouched here. Full production migration to the spoke
//! `TimelineEvent` wire type is deferred to **T3b** (a separate, larger plan
//! that requires nexus-narrative ↔ spoke `TimelineEvent` wire-type unification).
//! See the V1.142 P0 plan's T3a/T3b split and the T3b deferral note.

use nexus_spoke_adapter::{
    filter_timeline_events_by_moment_scale, order_timeline_events_by_ids, SpokeResult,
    TimelineEvent,
};
use serde_json::json;

/// Build a spoke `TimelineEvent` from fixture-style JSON, mirroring the shape of
/// spoke's `toy-world` event fixtures (`evt_tw_harbor_*.json`) and the
/// `make_timeline_event` helper in spoke's `timeline.rs` tests.
///
/// `schema_version`, `timeline_event_id`, `canonical_name`, and `extensions`
/// are required by the wire schema; other optional fields default to `None` /
/// empty when absent, matching spoke's own test constructor.
fn event(id: &str, scale: Option<&str>) -> TimelineEvent {
    let mut value = json!({
        "schema_version": 1,
        "timeline_event_id": id,
        "canonical_name": "Adoption event",
        "extensions": {},
    });
    if let Some(scale) = scale {
        value["timeline_scale"] = json!(scale);
    }
    serde_json::from_value(value)
        .unwrap_or_else(|error| panic!("failed to deserialize fixture TimelineEvent {id}: {error}"))
}

/// Mirrors spoke's `order_by_ids_orders_explicit_list_and_appends_stable_tail`:
/// events listed in `ordered_ids` come first (in that order); the remaining
/// events are appended in input order.
#[test]
fn order_by_ids_orders_explicit_list_and_appends_stable_tail() {
    let events = [
        event("evt_a", None),
        event("evt_b", None),
        event("evt_c", None),
    ];

    let result = order_timeline_events_by_ids(&events, &["evt_c".into(), "evt_a".into()]);
    let SpokeResult::Ok(ordered) = result else {
        panic!("expected ok result, got {result:?}");
    };
    assert_eq!(
        ordered
            .iter()
            .map(|event| event.timeline_event_id.as_str())
            .collect::<Vec<_>>(),
        vec!["evt_c", "evt_a", "evt_b"]
    );
}

/// Mirrors spoke's `order_by_ids_rejects_unknown_timeline_event_ids`: an
/// `ordered_ids` entry absent from the events is rejected with
/// `InvalidInput` and the unknown ids surfaced in `details`.
#[test]
fn order_by_ids_rejects_unknown_timeline_event_ids() {
    let events = [event("evt_a", None)];
    let result = order_timeline_events_by_ids(&events, &["evt_a".into(), "evt_missing".into()]);
    let SpokeResult::Reject(reject) = result else {
        panic!("expected reject result, got {result:?}");
    };
    assert_eq!(
        reject.code,
        nexus_spoke_adapter::SpokeRejectCode::InvalidInput
    );
    assert_eq!(
        reject
            .details
            .as_ref()
            .and_then(|details| details.get("unknown_timeline_event_ids")),
        Some(&json!(["evt_missing"]))
    );
}

/// Mirrors spoke's `filter_keeps_only_moment_scale_events_in_input_order`:
/// only events whose `timeline_scale` is exactly `"moment"` are kept, in input
/// order; `None` and other scales are dropped.
#[test]
fn filter_keeps_only_moment_scale_events_in_input_order() {
    let events = [
        event("evt_1", Some("moment")),
        event("evt_2", Some("narrative")),
        event("evt_3", Some("moment")),
        event("evt_4", None),
    ];

    let filtered = filter_timeline_events_by_moment_scale(&events);
    assert_eq!(
        filtered
            .iter()
            .map(|event| event.timeline_event_id.as_str())
            .collect::<Vec<_>>(),
        vec!["evt_1", "evt_3"]
    );
}

/// Mirrors spoke's `filter_returns_empty_for_empty_input`.
#[test]
fn filter_returns_empty_for_empty_input() {
    assert!(filter_timeline_events_by_moment_scale(&[]).is_empty());
}
