//! Round-trip + typed-parse tests for the `modules.mental` / `modules.belief`
//! dialect (V1.164 P2 T1, l5-mind).
//!
//! Covers plan Task 1 + AC-V1164-6 / AC-V1164-7:
//! - A `WorldKbEntry` carrying `modules.mental` (nine-field subset) and
//!   `modules.belief` (handbook proposition rows) survives the spoke
//!   conversion seam verbatim — all fields + unknown keys preserved (the
//!   same unknown-key fidelity discipline as `extensions.nexus`).
//! - The typed parse lens (`MentalFieldsRaw` / `BeliefPropositionRaw` from
//!   nexus-knowledge) reads handbook field names off `modules_json`.
//! - The designated world-state `info_point` entry (AR-1 / PD-10) carries
//!   the `holder: "world"` row and round-trips.
//! - Paper-alias rejection (AC-V1164-7 / TL-5): a row using `actor` /
//!   `knowledge_access` / `mental_source` is NOT produced by the parse type
//!   and does not round-trip as-is.

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{
    BeliefPropositionRaw, MentalFieldsRaw, WorldKbEntry,
};
use nexus_spoke_adapter::conversion::{spoke_to_world_kb, world_kb_to_spoke};
use serde_json::{json, Value};

/// Character entry carrying the handbook worked-example false-belief
/// structure (Bo's stale "marble in the box" belief) plus an unknown key
/// inside the `modules.mental` bag and one inside a belief row — both must
/// survive the seam verbatim.
fn bo_entry() -> WorldKbEntry {
    let mut entry = WorldKbEntry::new("wld_test", BlockType::Character, "Bo");
    entry.entry_id = "kb_bo".to_string();
    entry.modules = Some(json!({
        "mental": {
            "identity": { "role": "harbor_master" },
            "beliefs": { "ref": "kb_bo_beliefs", "count": 12 },
            "attention": { "target": "kb_tw_dawn_dock", "modality": "visual" },
            "goals": [{ "goal": "clear the dawn berths", "status": "active" }],
            "emotions": [{ "emotion": "alert", "intensity": 0.6 }],
            "norms": ["greet arriving captains"],
            "constraints": ["cannot waive dockside law"],
            "nexus_private_note": "unknown mental key must survive"
        },
        "belief": [
            {
                "holder": "kb_bo",
                "proposition": "The marble is in the box",
                "order": 1,
                "truth": "False",
                "access": "Private",
                "representation": "Implicit",
                "content_type": "Location",
                "source": "Perception",
                "context": "Temporal",
                "author_note": "unknown belief-row key must survive"
            }
        ]
    }));
    entry
}

/// Designated world-state `info_point` entry (AR-1 / PD-10) carrying the
/// narrated world fact row (`holder: "world"`).
fn world_state_entry() -> WorldKbEntry {
    let mut entry = WorldKbEntry::new("wld_test", BlockType::InfoPoint, "World State");
    entry.entry_id = "kb_world_state".to_string();
    entry.modules = Some(json!({
        "belief": [
            {
                "holder": "world",
                "proposition": "The marble is in the basket",
                "order": 0,
                "truth": "True",
                "access": "Public",
                "representation": "Explicit",
                "content_type": "Location",
                "source": "Narration",
                "context": "Temporal"
            }
        ]
    }));
    entry
}

/// Round-trip a `WorldKbEntry` through the spoke conversion seam.
fn roundtrip(entry: &WorldKbEntry) -> WorldKbEntry {
    spoke_to_world_kb(world_kb_to_spoke(entry))
}

#[test]
fn mental_and_belief_survive_spoke_roundtrip_verbatim() {
    let entry = bo_entry();
    let original_modules = entry.modules.clone().expect("fixture has modules");

    let roundtripped = roundtrip(&entry);

    // All fields verbatim + unknown keys preserved (AC-V1164-6).
    assert_eq!(roundtripped.modules, Some(original_modules));

    // Typed parse lens reads the handbook fields off the round-tripped
    // modules_json.
    let mental: MentalFieldsRaw = roundtripped
        .parse_mental_fields()
        .expect("modules.mental parses after round-trip");
    assert_eq!(mental.identity, Some(json!({ "role": "harbor_master" })));
    assert_eq!(
        mental.beliefs,
        Some(json!({ "ref": "kb_bo_beliefs", "count": 12 }))
    );
    assert_eq!(
        mental.goals,
        Some(json!([{ "goal": "clear the dawn berths", "status": "active" }]))
    );
    assert_eq!(
        mental.emotions,
        Some(json!([{ "emotion": "alert", "intensity": 0.6 }]))
    );

    let rows = roundtripped.parse_belief_rows();
    assert_eq!(rows.len(), 1, "one actor row in the fixture");
    let row = &rows[0];
    assert_eq!(row.holder.as_deref(), Some("kb_bo"));
    assert_eq!(row.proposition.as_deref(), Some("The marble is in the box"));
    assert_eq!(row.order, Some(1));
    assert_eq!(row.truth.as_deref(), Some("False"));
    assert_eq!(row.access.as_deref(), Some("Private"));
}

#[test]
fn world_state_info_point_roundtrips_with_world_row() {
    let entry = world_state_entry();
    assert_eq!(entry.block_type, BlockType::InfoPoint);
    assert_eq!(entry.canonical_name, "World State");
    let original_modules = entry.modules.clone().expect("fixture has modules");

    let roundtripped = roundtrip(&entry);

    assert_eq!(roundtripped.modules, Some(original_modules));
    let rows = roundtripped.parse_belief_rows();
    assert_eq!(rows.len(), 1);
    let world = &rows[0];
    assert_eq!(world.holder.as_deref(), Some("world"));
    assert_eq!(
        world.proposition.as_deref(),
        Some("The marble is in the basket")
    );
    assert_eq!(world.order, Some(0));
    assert_eq!(world.truth.as_deref(), Some("True"));
    assert_eq!(world.access.as_deref(), Some("Public"));
}

#[test]
fn paper_alias_row_is_not_produced_and_does_not_round_trip_as_is() {
    // AC-V1164-7: `actor` / `knowledge_access` / `mental_source` are
    // OmniToM paper aliases, not handbook fields (TL-5 / PD-5). The parse
    // type has no fields for them, so a paper-alias row never round-trips
    // through the typed form.
    let alias_row = json!({
        "actor": "kb_bo",
        "proposition": "The marble is in the box",
        "order": 1,
        "knowledge_access": "Private",
        "mental_source": "Perception"
    });

    let parsed: BeliefPropositionRaw =
        serde_json::from_value(alias_row.clone()).expect("raw type ignores unknown keys");
    assert_eq!(parsed.holder, None, "`actor` must not map to `holder`");
    assert_eq!(
        parsed.access, None,
        "`knowledge_access` must not map to `access`"
    );
    assert_eq!(
        parsed.source, None,
        "`mental_source` must not map to `source`"
    );
    // Re-emission is handbook-names only — the aliases are gone, so the row
    // does not round-trip as-is.
    let emitted: Value = serde_json::to_value(&parsed).unwrap();
    assert_ne!(emitted, alias_row);
    assert!(emitted.get("actor").is_none());
    assert!(emitted.get("knowledge_access").is_none());
    assert!(emitted.get("mental_source").is_none());

    // Same through the entry path: an entry carrying a paper-alias row
    // parses to handbook fields unpopulated and never re-emits the aliases.
    let mut entry = bo_entry();
    entry.modules = Some(json!({ "belief": [alias_row] }));
    let rows = entry.parse_belief_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].holder, None);
    assert_eq!(rows[0].access, None);
    assert_eq!(rows[0].source, None);
}
