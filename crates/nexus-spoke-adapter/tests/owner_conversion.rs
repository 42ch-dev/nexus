//! v1.184 P1 Task 2 — owner-aware conversion seam proofs.
//!
//! Covers the typed Nexus extension projection at the sole SPOKE conversion
//! seam (`nexus_spoke_adapter::conversion`):
//! - World owners emit/read `extensions.nexus.world_id` (legacy golden);
//! - Character/binding owners emit their typed owner key and NEVER a
//!   fabricated `world_id`;
//! - the reverse conversion fails closed when no canonical owner key exists
//!   (no fabricated World owner);
//! - `creator_only` round-trips as Nexus metadata;
//! - unknown `extensions.nexus` keys survive the round-trip for every owner.

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{
    KnowledgeEntryBody, KnowledgeEntryRecord, KnowledgeOwnerRef,
};
use nexus_spoke_adapter::conversion::{knowledge_record_to_spoke, spoke_to_knowledge_record};
use spoke_schemas::knowledge_entry::KnowledgeEntryExtensionsKey;

fn nexus_ns(entry: &spoke_schemas::KnowledgeEntry) -> serde_json::Map<String, serde_json::Value> {
    let key = KnowledgeEntryExtensionsKey::try_from("nexus").unwrap();
    entry.extensions.get(&key).cloned().unwrap_or_default()
}

fn record_for(owner: &KnowledgeOwnerRef, name: &str) -> KnowledgeEntryRecord {
    let mut rec = match owner {
        KnowledgeOwnerRef::World(id) => KnowledgeEntryRecord::new(id, BlockType::Character, name),
        KnowledgeOwnerRef::Character(id) => {
            KnowledgeEntryRecord::for_character(id, BlockType::Character, name)
        }
        KnowledgeOwnerRef::ActorWorldBinding(id) => {
            KnowledgeEntryRecord::for_binding(id, BlockType::Character, name)
        }
    };
    rec.body = Some(KnowledgeEntryBody {
        summary: Some(format!("{name} summary")),
        ..KnowledgeEntryBody::default()
    });
    rec
}

/// World golden: a World-owned record emits `world_id` and round-trips
/// byte-identically through the seam (legacy behavior preserved).
#[test]
fn world_owner_emits_world_id_and_round_trips() {
    let rec = record_for(&KnowledgeOwnerRef::world("wld_golden"), "Aria");
    let spoke = knowledge_record_to_spoke(&rec);
    let ns = nexus_ns(&spoke);
    assert_eq!(
        ns.get("world_id").and_then(|v| v.as_str()),
        Some("wld_golden")
    );
    assert!(!ns.contains_key("character_id"));
    assert!(!ns.contains_key("actor_world_binding_id"));
    assert!(!ns.contains_key("creator_only"));

    let back = spoke_to_knowledge_record(spoke).unwrap();
    assert_eq!(back.owner, KnowledgeOwnerRef::world("wld_golden"));
    assert!(!back.creator_only);
    assert_eq!(back.entry_id, rec.entry_id);
    assert_eq!(back.body, rec.body);
}

/// Character owner: emits `character_id`, never a fabricated `world_id`.
#[test]
fn character_owner_emits_character_id_never_world_id() {
    let rec = record_for(&KnowledgeOwnerRef::character("chr_abc"), "Shared lore");
    let spoke = knowledge_record_to_spoke(&rec);
    let ns = nexus_ns(&spoke);
    assert_eq!(ns.get("character_id").and_then(|v| v.as_str()), Some("chr_abc"));
    assert!(
        !ns.contains_key("world_id"),
        "character-owned entry must not carry world_id: {ns:?}"
    );
    assert!(!ns.contains_key("actor_world_binding_id"));

    let back = spoke_to_knowledge_record(spoke).unwrap();
    assert_eq!(back.owner, KnowledgeOwnerRef::character("chr_abc"));
    assert_eq!(back.world_id(), None);
}

/// Binding owner: emits `actor_world_binding_id`, never a fabricated
/// `world_id`.
#[test]
fn binding_owner_emits_binding_id_never_world_id() {
    let rec = record_for(&KnowledgeOwnerRef::actor_world_binding("awb_abc"), "Private note");
    let spoke = knowledge_record_to_spoke(&rec);
    let ns = nexus_ns(&spoke);
    assert_eq!(
        ns.get("actor_world_binding_id").and_then(|v| v.as_str()),
        Some("awb_abc")
    );
    assert!(
        !ns.contains_key("world_id"),
        "binding-owned entry must not carry world_id: {ns:?}"
    );
    assert!(!ns.contains_key("character_id"));

    let back = spoke_to_knowledge_record(spoke).unwrap();
    assert_eq!(back.owner, KnowledgeOwnerRef::actor_world_binding("awb_abc"));
}

/// Fail closed: a spoke entry with NO canonical owner key must not gain a
/// fabricated World owner — the reverse conversion errors instead of
/// defaulting `world_id` to an empty string (the pre-v1.184 behavior).
#[test]
fn reverse_conversion_without_owner_fails_closed() {
    let rec = record_for(&KnowledgeOwnerRef::world("wld_x"), "Orphan");
    let mut spoke = knowledge_record_to_spoke(&rec);
    // Strip every owner key, simulating a malformed/foreign spoke entry.
    let key = KnowledgeEntryExtensionsKey::try_from("nexus").unwrap();
    if let Some(ns) = spoke.extensions.get_mut(&key) {
        ns.remove("world_id");
        ns.remove("character_id");
        ns.remove("actor_world_binding_id");
    }
    assert!(spoke_to_knowledge_record(spoke).is_err());
}

/// `creator_only` round-trips as Nexus metadata on World-owned entries.
#[test]
fn creator_only_round_trips_as_nexus_metadata() {
    let mut rec = record_for(&KnowledgeOwnerRef::world("wld_x"), "Creator lore");
    rec.creator_only = true;
    let spoke = knowledge_record_to_spoke(&rec);
    let ns = nexus_ns(&spoke);
    assert_eq!(ns.get("creator_only").and_then(|v| v.as_bool()), Some(true));

    let back = spoke_to_knowledge_record(spoke).unwrap();
    assert!(back.creator_only);
    assert_eq!(back.owner, KnowledgeOwnerRef::world("wld_x"));
}

/// Unknown `extensions.nexus` keys survive the round-trip verbatim for every
/// owner kind; the typed owner keys never leak into the extras bag.
#[test]
fn unknown_nexus_keys_round_trip_for_every_owner() {
    for owner in [
        KnowledgeOwnerRef::world("wld_x"),
        KnowledgeOwnerRef::character("chr_x"),
        KnowledgeOwnerRef::actor_world_binding("awb_x"),
    ] {
        let mut rec = record_for(&owner, "Extras");
        rec.extensions_nexus_extras = Some(serde_json::json!({"custom_flag": "keep-me"}));
        let spoke = knowledge_record_to_spoke(&rec);
        let ns = nexus_ns(&spoke);
        assert_eq!(
            ns.get("custom_flag").and_then(|v| v.as_str()),
            Some("keep-me"),
            "unknown key lost on forward conversion for {owner:?}"
        );
        let back = spoke_to_knowledge_record(spoke).unwrap();
        assert_eq!(back.owner, owner);
        assert_eq!(
            back.extensions_nexus_extras,
            Some(serde_json::json!({"custom_flag": "keep-me"})),
            "unknown key lost on reverse conversion for {owner:?}"
        );
    }
}
