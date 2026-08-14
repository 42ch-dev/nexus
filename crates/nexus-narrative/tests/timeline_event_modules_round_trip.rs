//! AC-V1164-3: bidirectional `TimelineEvent.modules` passthrough round-trip.
//!
//! Nexus `TimelineEvent.modules` (`Option<serde_json::Value>`) must survive
//! the nexus → spoke → nexus conversion seam verbatim — including unknown keys
//! inside `modules.observation` (the AR-5 non-lossy contract).
//!
//! Spoke 0.10.0 models modules as a typed map
//! `HashMap<TimelineEventModulesKey, TimelineEventModulesValue>` where:
//! - keys are a transparent newtype over `String`, regex-validated on
//!   deserialize (`^[a-z][a-z0-9_-]*$`), stored verbatim (no normalization);
//! - values are an untagged enum of object (`Map<String, Value>`) or array
//!   (`Vec<Value>`), so object/array content serializes losslessly.
//!
//! Content that fits those shapes therefore round-trips byte-for-byte; both
//! sides serialize through the same `serde_json::Map` (BTreeMap-backed, no
//! `preserve_order`), so identical content yields identical JSON text.

use nexus_narrative::timeline_event::{SpokeTimelineEvent, TimelineEvent, TimelineEventType};
use serde_json::json;

#[test]
fn modules_round_trip_byte_for_byte_with_unknown_key() {
    let mut event = TimelineEvent::new("wld_1", "fbk_root", TimelineEventType::StoryAdvance, 1);
    event.modules = Some(json!({
        "observation": {
            "observers": ["kb_char_1"],
            "access": {
                "line_of_sight": true,
                "hearing_range": true,
                "modality": ["visual", "auditory"]
            },
            // Unknown key: must survive the typed-map seam (AR-5 round-trip).
            "position": "in-room"
        }
    }));
    let original = event.modules.clone().unwrap();

    // Forward: nexus → spoke typed map.
    let spoke: SpokeTimelineEvent = event.into();
    assert_eq!(spoke.modules.len(), 1, "observation module must be carried");
    assert_eq!(
        spoke.modules.keys().next().map(|k| k.as_str()),
        Some("observation"),
        "module key must survive into the spoke typed map"
    );

    // Reverse: spoke typed map → nexus.
    let back: TimelineEvent = spoke.into();
    let round_tripped = back
        .modules
        .expect("modules must survive the round-trip (None would mean data loss)");

    // Byte-for-byte: identical content through the same serde_json::Map
    // normalization must serialize to identical JSON text.
    assert_eq!(
        serde_json::to_string(&original).unwrap(),
        serde_json::to_string(&round_tripped).unwrap(),
        "modules content must round-trip byte-for-byte"
    );

    // Explicit content assertions (defend against silent drops/normalization).
    assert_eq!(round_tripped["observation"]["position"], json!("in-room"));
    assert_eq!(
        round_tripped["observation"]["observers"],
        json!(["kb_char_1"])
    );
    assert_eq!(
        round_tripped["observation"]["access"]["line_of_sight"],
        json!(true)
    );
    assert_eq!(
        round_tripped["observation"]["access"]["modality"],
        json!(["visual", "auditory"])
    );
}

#[test]
fn modules_none_round_trips_to_none() {
    let event = TimelineEvent::new("wld_1", "fbk_root", TimelineEventType::StoryAdvance, 2);
    assert!(event.modules.is_none(), "precondition: no modules data");

    let spoke: SpokeTimelineEvent = event.into();
    assert!(
        spoke.modules.is_empty(),
        "nexus None must map to the spoke empty map"
    );

    let back: TimelineEvent = spoke.into();
    assert!(
        back.modules.is_none(),
        "spoke empty map must map back to nexus None (unrecorded ≡ empty)"
    );
}

// ── Malformed modules: reject-not-drop at the forward seam ─────────────────
//
// The forward conversion deserializes nexus `Option<Value>` into the spoke
// typed module map with an `expect` (timeline_event.rs ~477-480). Any valid
// JSON that is NOT a module-name → object|array map — scalar, array, null, or
// a regex-invalid key (`^[a-z][a-z0-9_-]*$`) — fails deserialization and
// panics rather than silently dropping data at the seam (plan-locked
// behavior; writers must pre-validate — P2 writer validation is
// roadmap-tracked). `json!({})` is deliberately NOT tested here: an empty
// object is a VALID empty map on the forward pass.

#[test]
#[should_panic(expected = "modules must deserialize into the spoke typed module map")]
fn modules_scalar_string_rejected_at_seam() {
    let mut event = TimelineEvent::new("wld_1", "fbk_root", TimelineEventType::StoryAdvance, 3);
    event.modules = Some(json!("not-an-object"));
    let _spoke: SpokeTimelineEvent = event.into();
}

#[test]
#[should_panic(expected = "modules must deserialize into the spoke typed module map")]
fn modules_scalar_number_rejected_at_seam() {
    let mut event = TimelineEvent::new("wld_1", "fbk_root", TimelineEventType::StoryAdvance, 4);
    event.modules = Some(json!(42));
    let _spoke: SpokeTimelineEvent = event.into();
}

#[test]
#[should_panic(expected = "modules must deserialize into the spoke typed module map")]
fn modules_scalar_bool_rejected_at_seam() {
    let mut event = TimelineEvent::new("wld_1", "fbk_root", TimelineEventType::StoryAdvance, 5);
    event.modules = Some(json!(true));
    let _spoke: SpokeTimelineEvent = event.into();
}

#[test]
#[should_panic(expected = "modules must deserialize into the spoke typed module map")]
fn modules_array_rejected_at_seam() {
    let mut event = TimelineEvent::new("wld_1", "fbk_root", TimelineEventType::StoryAdvance, 6);
    event.modules = Some(json!([1, 2]));
    let _spoke: SpokeTimelineEvent = event.into();
}

#[test]
#[should_panic(expected = "modules must deserialize into the spoke typed module map")]
fn modules_null_rejected_at_seam() {
    let mut event = TimelineEvent::new("wld_1", "fbk_root", TimelineEventType::StoryAdvance, 7);
    event.modules = Some(json!(null));
    let _spoke: SpokeTimelineEvent = event.into();
}

#[test]
#[should_panic(expected = "modules must deserialize into the spoke typed module map")]
fn modules_invalid_key_rejected_at_seam() {
    // "Bad-Key" violates the key pattern `^[a-z][a-z0-9_-]*$` (uppercase 'B')
    // and fails the key newtype's FromStr validation during map deserialize.
    let mut event = TimelineEvent::new("wld_1", "fbk_root", TimelineEventType::StoryAdvance, 8);
    event.modules = Some(json!({"Bad-Key": {}}));
    let _spoke: SpokeTimelineEvent = event.into();
}
