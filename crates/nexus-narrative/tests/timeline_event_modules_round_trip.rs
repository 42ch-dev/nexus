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
