//! Golden-fixture round-trip tests (V1.170 P0, AR-7 drift-guard layer 1).
//!
//! `fixtures/compute-input.golden.json` + `compute-output.golden.json` are
//! extracted from the canonical envelope samples (compute-module-abi.md §4/§5,
//! which mirror the `schemas/daemon-api/compute/*.schema.json` contracts).
//!
//! Each test: parses the fixture into the typed envelope, asserts typed-field
//! survival, reserializes, and compares the result value-equal to the original
//! JSON. A serialization that drops, renames, or reorders a wire field — or a
//! typed field that stops deserializing — fails here. The structural
//! mirror-gap gate (`tooling/check-module-sdk-drift.sh`) covers wire fields
//! the SDK has not yet mirrored; this test covers the SDK's own round-trip.

use nexus_module_sdk::{ComputeInput, ComputeOutput, DeltaOp};
use serde_json::Value;

const INPUT_GOLDEN: &str = include_str!("fixtures/compute-input.golden.json");
const OUTPUT_GOLDEN: &str = include_str!("fixtures/compute-output.golden.json");

/// Parse a golden fixture to a typed value and assert the reserialized JSON
/// is value-equal to the original (no field loss, rename, or reorder).
fn assert_round_trip<T>(golden: &str) -> T
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let original: Value = serde_json::from_str(golden).expect("golden fixture is valid JSON");
    let typed: T = serde_json::from_str(golden).expect("golden fixture parses as typed envelope");
    let reserialized: Value =
        serde_json::from_str(&serde_json::to_string(&typed).expect("typed envelope serializes"))
            .expect("reserialized output is valid JSON");
    assert_eq!(
        reserialized, original,
        "reserialized envelope must be value-equal to the golden fixture"
    );
    typed
}

#[test]
fn compute_input_golden_round_trips_and_typed_fields_survive() {
    let input: ComputeInput = assert_round_trip(INPUT_GOLDEN);

    // Typed-field survival (AR-3): the envelope skeleton is typed, the
    // high-churn parts stay opaque `Value` passthroughs.
    assert_eq!(input.schema_version, 1);
    assert_eq!(input.world_ref.world_id.as_deref(), Some("w_abc123"));
    assert_eq!(input.world_ref.branch_id.as_deref(), Some("root"));
    assert_eq!(
        input.world_ref.timeline_head_event_id.as_deref(),
        Some("evt_xyz789")
    );
    assert_eq!(input.key_blocks.len(), 1);
    assert_eq!(input.key_blocks[0]["key_block_id"], "kb-def");
    assert_eq!(input.narrative_state["current_chapter"], "ch3");
    assert_eq!(input.invocation["attacker_id"], "kb-atk");
}

#[test]
fn compute_output_golden_round_trips_with_delta_op_enum() {
    let output: ComputeOutput = assert_round_trip(OUTPUT_GOLDEN);

    // Typed-field survival (AR-3): state_delta items are typed
    // (`DeltaOp` serde-renamed to the wire `add`/`sub`/`set`).
    assert_eq!(output.schema_version, 1);
    assert_eq!(output.state_delta.len(), 1);
    assert_eq!(output.state_delta[0].op, DeltaOp::Sub);
    assert_eq!(output.state_delta[0].path, "character.current_hp");
    assert_eq!(
        output.state_delta[0].target_key_block_id.as_deref(),
        Some("kb-def")
    );
    assert_eq!(output.state_delta[0].value, Some(serde_json::json!(15)));

    assert_eq!(output.timeline_events.len(), 1);
    assert_eq!(output.timeline_events[0]["event_type"], "state_update");
    assert!(output.new_key_blocks.is_empty());
    assert_eq!(output.battle_report["kind"], "combat");
}
