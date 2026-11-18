//! Consumes the packed `nexus-contracts` archive: builds a generated wire
//! object, serde round-trips it, and asserts field identity field by field —
//! a length or key-count check would not notice a swapped value.

use nexus_contracts::generated::core::narrative::narrative_world_state::NarrativeWorldState;

/// Every field populated, so the round-trip has something to lose.
fn sample() -> NarrativeWorldState {
    NarrativeWorldState {
        canon_revision: Some(3),
        created_at: "2026-11-18T00:00:00Z".to_string(),
        current_time_pointer: Some("evt_42".to_string()),
        current_timeline_head_id: Some("evt_42".to_string()),
        fork_branch_id: None,
        forked_from_event_id: None,
        is_fork: false,
        parent_world_id: None,
        slug: "ashen-vale".to_string(),
        status: "active".to_string(),
        title: "The Ashen Vale".to_string(),
        world_id: "wld_0123456789".to_string(),
    }
}

fn main() {
    let original = sample();

    let encoded = serde_json::to_string(&original).expect("generated type serializes");
    let decoded: NarrativeWorldState =
        serde_json::from_str(&encoded).expect("generated type deserializes its own encoding");

    // Field-level identity: every value survives the wire, not just the shape.
    assert_eq!(decoded.canon_revision, Some(3));
    assert_eq!(decoded.created_at, "2026-11-18T00:00:00Z");
    assert_eq!(decoded.current_time_pointer.as_deref(), Some("evt_42"));
    assert_eq!(decoded.current_timeline_head_id.as_deref(), Some("evt_42"));
    assert_eq!(decoded.fork_branch_id, None);
    assert_eq!(decoded.forked_from_event_id, None);
    assert!(!decoded.is_fork);
    assert_eq!(decoded.parent_world_id, None);
    assert_eq!(decoded.slug, "ashen-vale");
    assert_eq!(decoded.status, "active");
    assert_eq!(decoded.title, "The Ashen Vale");
    assert_eq!(decoded.world_id, "wld_0123456789");

    // `skip_serializing_if` is a wire contract, not decoration: an absent
    // optional field must vanish from the object while its siblings stay put.
    let mut sparse = sample();
    sparse.current_time_pointer = None;
    let sparse_json: serde_json::Value =
        serde_json::to_value(&sparse).expect("generated type converts to JSON");
    assert_eq!(
        sparse_json.get("current_time_pointer"),
        None,
        "an absent optional field must not be emitted"
    );
    assert_eq!(sparse_json["world_id"], "wld_0123456789");
    assert_eq!(sparse_json["title"], "The Ashen Vale");

    // `deny_unknown_fields` proves the archive's field set is the contract: a
    // payload carrying an extra key is refused instead of silently kept.
    let mut with_unknown = serde_json::to_value(&original).expect("generated type converts to JSON");
    with_unknown["unexpected_field"] = serde_json::Value::Bool(true);
    assert!(
        serde_json::from_value::<NarrativeWorldState>(with_unknown).is_err(),
        "an unknown field must be refused by the generated contract"
    );

    println!(
        "nexus-contracts round-tripped {} ({} bytes)",
        original.world_id,
        encoded.len()
    );
}
