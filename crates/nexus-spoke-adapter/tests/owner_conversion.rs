//! v1.184 P1 Task 2 — owner-aware conversion seam proofs.
//!
//! Covers the typed Nexus extension projection at the sole SPOKE conversion
//! seam (`nexus_spoke_adapter::conversion`):
//! - World owners emit/read `extensions.nexus.world_id` (legacy golden);
//! - Character/binding owners emit their typed owner key and NEVER a
//!   fabricated `world_id`;
//! - the reverse conversion fails closed when no canonical owner key exists
//!   (no fabricated World owner);
//! - native `holder_entry_id` / `disclosure` map exactly onto the wire
//!   `owner` / `disclosure` fields and back (v1.191 P1 T8);
//! - the retired legacy `creator_only` key is refused, not carried;
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
    assert_eq!(
        (back.holder_entry_id, back.disclosure),
        (None, None),
        "a shared record carries no governance on either side"
    );
    assert_eq!(back.entry_id, rec.entry_id);
    assert_eq!(back.body, rec.body);
}

/// Character owner: emits `character_id`, never a fabricated `world_id`.
#[test]
fn character_owner_emits_character_id_never_world_id() {
    let rec = record_for(&KnowledgeOwnerRef::character("chr_abc"), "Shared lore");
    let spoke = knowledge_record_to_spoke(&rec);
    let ns = nexus_ns(&spoke);
    assert_eq!(
        ns.get("character_id").and_then(|v| v.as_str()),
        Some("chr_abc")
    );
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
    let rec = record_for(
        &KnowledgeOwnerRef::actor_world_binding("awb_abc"),
        "Private note",
    );
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
    assert_eq!(
        back.owner,
        KnowledgeOwnerRef::actor_world_binding("awb_abc")
    );
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

/// v1.191 P1 T8: the native governance pair maps exactly onto the wire
/// `owner` / `disclosure` fields in both directions, and the holder axis is
/// never confused with the narrative container (`extensions.nexus.world_id`).
#[test]
fn holder_and_disclosure_map_exactly_onto_the_wire_fields() {
    let mut rec = record_for(&KnowledgeOwnerRef::world("wld_x"), "Creator lore");
    rec.holder_entry_id = Some("hld_creator".to_string());
    rec.disclosure = Some("owner-private".to_string());

    let spoke = knowledge_record_to_spoke(&rec);
    assert_eq!(
        spoke.owner.as_ref().map(|o| o.as_str()),
        Some("hld_creator"),
        "native holder_entry_id is the wire owner"
    );
    assert_eq!(
        spoke.disclosure.as_ref().map(|d| d.as_str()),
        Some("owner-private")
    );
    let ns = nexus_ns(&spoke);
    assert_eq!(
        ns.get("world_id").and_then(|v| v.as_str()),
        Some("wld_x"),
        "the container keeps riding extensions.nexus, never wire owner"
    );
    assert!(!ns.contains_key("holder_entry_id"));
    assert!(!ns.contains_key("disclosure"));

    let back = spoke_to_knowledge_record(spoke).unwrap();
    assert_eq!(back.holder_entry_id.as_deref(), Some("hld_creator"));
    assert_eq!(back.disclosure.as_deref(), Some("owner-private"));
    assert_eq!(back.owner, KnowledgeOwnerRef::world("wld_x"));
}

/// v1.191 P1 T8: an unknown (open-domain) disclosure value is carried verbatim
/// by the seam — normalization to "shared" is a storage-boundary decision, not
/// a conversion one, so an adopted foreign row keeps its vocabulary.
#[test]
fn unknown_disclosure_vocabulary_round_trips_verbatim() {
    let mut rec = record_for(&KnowledgeOwnerRef::world("wld_x"), "Foreign lore");
    rec.holder_entry_id = Some("hld_foreign".to_string());
    rec.disclosure = Some("party-only".to_string());

    let back = spoke_to_knowledge_record(knowledge_record_to_spoke(&rec)).unwrap();
    assert_eq!(back.holder_entry_id.as_deref(), Some("hld_foreign"));
    assert_eq!(back.disclosure.as_deref(), Some("party-only"));
}

/// v1.191 P1 T8: holder-only rows (an adopted foreign owner without a
/// disclosure) keep their holder and stay shared.
#[test]
fn holder_without_disclosure_round_trips_as_shared() {
    let mut rec = record_for(&KnowledgeOwnerRef::world("wld_x"), "Adopted lore");
    rec.holder_entry_id = Some("hld_adopted".to_string());

    let spoke = knowledge_record_to_spoke(&rec);
    assert!(spoke.disclosure.is_none(), "no disclosure is authored");
    let back = spoke_to_knowledge_record(spoke).unwrap();
    assert_eq!(back.holder_entry_id.as_deref(), Some("hld_adopted"));
    assert_eq!(back.disclosure, None);
}

/// Build a spoke entry whose `extensions.nexus` JSON is exactly the supplied
/// map (bypassing the typed setters) — used to synthesize malformed wire
/// payloads the typed setter would never emit.
fn spoke_with_nexus(json_map: &serde_json::Value) -> spoke_schemas::KnowledgeEntry {
    let rec = record_for(&KnowledgeOwnerRef::world("wld_x"), "Wire");
    let mut spoke = knowledge_record_to_spoke(&rec);
    let key = KnowledgeEntryExtensionsKey::try_from("nexus").unwrap();
    // The `extensions` map holds the namespace as a JSON object (the same
    // shape `nexus_namespace` reads back), not a wrapped `Value`.
    let obj = json_map
        .as_object()
        .expect("nexus payload is an object")
        .clone();
    spoke.extensions.insert(key, obj);
    spoke
}

/// Ambiguous owner metadata must fail closed (v1.184 P1 fix): every pair (and
/// the triple) of typed owner keys present at once is rejected — precedence is
/// not an ambiguity check.
#[test]
fn ambiguous_owner_keys_reject_both_pairs_and_triple() {
    for ns in [
        serde_json::json!({"world_id": "wld_1", "character_id": "chr_1"}),
        serde_json::json!({"world_id": "wld_1", "actor_world_binding_id": "awb_1"}),
        serde_json::json!({"character_id": "chr_1", "actor_world_binding_id": "awb_1"}),
        serde_json::json!({
            "world_id": "wld_1",
            "character_id": "chr_1",
            "actor_world_binding_id": "awb_1"
        }),
    ] {
        let spoke = spoke_with_nexus(&ns);
        let err = spoke_to_knowledge_record(spoke).unwrap_err();
        assert!(
            matches!(
                err,
                nexus_knowledge::world_kb::errors::KbError::InvalidOwnerMetadata(_)
            ),
            "ambiguous owner metadata must reject, got {err:?}"
        );
    }
}

/// A present owner key carrying a non-string value (or `null`) must fail
/// closed, never be silently ignored (v1.184 P1 fix).
#[test]
fn wrong_typed_or_null_owner_key_rejects() {
    for ns in [
        serde_json::json!({"world_id": 12345}),
        serde_json::json!({"world_id": null}),
        serde_json::json!({"character_id": ["not", "a", "string"]}),
        serde_json::json!({"actor_world_binding_id": {"id": "nested"}}),
    ] {
        let spoke = spoke_with_nexus(&ns);
        let err = spoke_to_knowledge_record(spoke).unwrap_err();
        assert!(
            matches!(
                err,
                nexus_knowledge::world_kb::errors::KbError::InvalidOwnerMetadata(_)
            ),
            "malformed owner key must reject, got {err:?}"
        );
    }
}

/// v1.191 P1 T8: the retired legacy `extensions.nexus.creator_only` key is
/// refused at the conversion seam with the stable reason — `false` included —
/// and never carried as an unknown extra. The native governance pair is the
/// only governance channel.
#[test]
fn legacy_creator_only_key_is_refused_not_carried() {
    for legacy in [
        serde_json::Value::Bool(true),
        serde_json::Value::Bool(false),
    ] {
        let spoke = spoke_with_nexus(&serde_json::json!({
            "world_id": "wld_x",
            "creator_only": legacy,
        }));
        let err = spoke_to_knowledge_record(spoke).unwrap_err();
        assert!(
            err.to_string().contains("legacy_creator_only_unsupported"),
            "the stable reason must ride the refusal, got {err:?}"
        );
    }
}

/// A wire `schema_version` exceeding `u32::MAX` must not be silently
/// normalized to `1` (v1.184 P1 fix).
#[test]
fn schema_version_overflow_rejects_not_normalizes() {
    let rec = record_for(&KnowledgeOwnerRef::world("wld_x"), "Overflow");
    let mut spoke = knowledge_record_to_spoke(&rec);
    spoke.schema_version = std::num::NonZero::new(u64::from(u32::MAX) + 1).expect("non-zero");
    let err = spoke_to_knowledge_record(spoke).unwrap_err();
    assert!(
        matches!(
            &err,
            nexus_knowledge::world_kb::errors::KbError::UnsupportedSchemaVersion(n) if *n == u64::from(u32::MAX) + 1
        ),
        "schema_version overflow must reject, got {err:?}"
    );
}

/// An unknown spoke `entry_type` must not silently normalize to the default
/// block type (v1.184 P1 fix).
#[test]
fn unknown_entry_type_rejects_not_normalizes() {
    let rec = record_for(&KnowledgeOwnerRef::world("wld_x"), "UnknownType");
    let mut spoke = knowledge_record_to_spoke(&rec);
    spoke.entry_type = "not_a_real_block_type".to_string();
    let err = spoke_to_knowledge_record(spoke).unwrap_err();
    assert!(
        matches!(
            &err,
            nexus_knowledge::world_kb::errors::KbError::UnknownEntryType(t) if t == "not_a_real_block_type"
        ),
        "unknown entry_type must reject, got {err:?}"
    );
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
