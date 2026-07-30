//! Round-trip preservation tests for the `extensions.nexus` typed accessors.
//!
//! Covers tracked spec §2.2:
//! - All 5 typed fields populate and read back.
//! - Unknown namespaces in `extensions` survive a nexus-accessor round-trip.
//! - Unknown keys inside `extensions.nexus` survive a nexus-accessor
//!   round-trip.
//! - Empty `extensions.nexus` is valid.

use std::collections::HashMap;
use std::num::NonZeroU64;

use nexus_spoke_adapter::extensions::{
    build_extensions_nexus, get_created_from_command_id, get_provenance, get_world_id,
    set_created_from_command_id, set_provenance, set_world_id,
};
use nexus_spoke_adapter::ExtensionMap;
use serde_json::{json, Map, Value};
use spoke_schemas::knowledge_entry::{
    KnowledgeEntryBody, KnowledgeEntryCanonicalName, KnowledgeEntryExtensionsKey,
};
use spoke_schemas::KnowledgeEntry;

/// Construct a [`KnowledgeEntry`] with empty extensions and a minimal valid body.
fn make_entry() -> KnowledgeEntry {
    KnowledgeEntry {
        body: KnowledgeEntryBody::default(),
        canonical_name: KnowledgeEntryCanonicalName::try_from("Mira Vale".to_owned())
            .expect("non-empty canonical name"),
        created_at: None,
        entry_id: "kb_test".into(),
        entry_type: "character".into(),
        extensions: HashMap::new(),
        modules: HashMap::new(),
        revision: None,
        schema_version: NonZeroU64::new(1).expect("1 is non-zero"),
        source_anchor: None,
        status: "provisional".into(),
        updated_at: None,
    }
}

#[test]
fn sets_and_reads_all_five_typed_fields() {
    let mut entry = make_entry();

    set_world_id(&mut entry, "wld_abc".into());
    set_created_from_command_id(&mut entry, Some("cmd_xyz".into()));
    set_provenance(
        &mut entry,
        Some("wrk_def".into()),
        Some(3),
        Some("review_time_extract".into()),
    );

    assert_eq!(get_world_id(&entry), Some("wld_abc"));
    assert_eq!(get_created_from_command_id(&entry), Some("cmd_xyz"));
    assert_eq!(
        get_provenance(&entry),
        (Some("wrk_def"), Some(3), Some("review_time_extract"))
    );
}

#[test]
fn optional_fields_default_to_none_when_namespace_absent() {
    let entry = make_entry();

    assert_eq!(get_world_id(&entry), None);
    assert_eq!(get_created_from_command_id(&entry), None);
    assert_eq!(get_provenance(&entry), (None, None, None));
}

#[test]
fn optional_fields_read_none_when_not_set_but_namespace_present() {
    let mut entry = make_entry();
    // Setting only world_id creates the nexus namespace, but the optional
    // provenance/command fields should still read None.
    set_world_id(&mut entry, "wld_only".into());

    assert_eq!(get_world_id(&entry), Some("wld_only"));
    assert_eq!(get_created_from_command_id(&entry), None);
    assert_eq!(get_provenance(&entry), (None, None, None));
}

#[test]
fn setting_optional_to_none_removes_the_key() {
    let mut entry = make_entry();
    set_created_from_command_id(&mut entry, Some("cmd_1".into()));
    set_provenance(
        &mut entry,
        Some("wrk_1".into()),
        Some(7),
        Some("manual".into()),
    );
    assert_eq!(get_created_from_command_id(&entry), Some("cmd_1"));

    // Clear them again.
    set_created_from_command_id(&mut entry, None);
    set_provenance(&mut entry, None, None, None);

    assert_eq!(get_created_from_command_id(&entry), None);
    assert_eq!(get_provenance(&entry), (None, None, None));
}

#[test]
fn preserves_unknown_keys_inside_extensions_nexus() {
    let mut entry = make_entry();

    // First, populate the typed fields.
    set_world_id(&mut entry, "wld_abc".into());
    set_provenance(
        &mut entry,
        Some("wrk_def".into()),
        Some(3),
        Some("manual".into()),
    );

    // Inject an unknown key directly into the nexus namespace — simulates a
    // future/forward-compatible field that nexus does not yet know about.
    let nexus_key =
        KnowledgeEntryExtensionsKey::try_from("nexus").expect("nexus matches namespace regex");
    let ns = entry.extensions.entry(nexus_key).or_default();
    ns.insert("future_field".into(), json!("keep-me"));

    // Touch the typed fields again.
    set_world_id(&mut entry, "wld_updated".into());

    // Unknown key is preserved verbatim; typed fields are updated.
    let ns = entry
        .extensions
        .get(&KnowledgeEntryExtensionsKey::try_from("nexus").unwrap())
        .expect("nexus namespace present");
    assert_eq!(ns.get("future_field"), Some(&json!("keep-me")));
    assert_eq!(ns.get("world_id"), Some(&json!("wld_updated")));
    assert_eq!(
        get_provenance(&entry),
        (Some("wrk_def"), Some(3), Some("manual"))
    );
}

#[test]
fn preserves_unknown_namespaces_alongside_nexus() {
    let mut entry = make_entry();
    set_world_id(&mut entry, "wld_abc".into());

    // Inject a sibling namespace that nexus must never touch.
    let other_key = KnowledgeEntryExtensionsKey::try_from("other_product")
        .expect("other_product matches namespace regex");
    let mut other_ns = Map::new();
    other_ns.insert("book_id".into(), json!("bk_1"));
    entry.extensions.insert(other_key, other_ns);

    // Round-trip through a nexus accessor.
    set_world_id(&mut entry, "wld_updated".into());

    // Sibling namespace survives untouched.
    let other = entry
        .extensions
        .get(&KnowledgeEntryExtensionsKey::try_from("other_product").unwrap())
        .expect("other_product namespace preserved");
    assert_eq!(other.get("book_id"), Some(&json!("bk_1")));
    assert_eq!(get_world_id(&entry), Some("wld_updated"));
}

#[test]
fn empty_extensions_nexus_is_valid_after_set_then_clear() {
    let mut entry = make_entry();
    set_world_id(&mut entry, "wld_temp".into());
    set_created_from_command_id(&mut entry, Some("cmd_temp".into()));

    // Clear all optional fields; world_id stays (required, no setter takes None).
    set_created_from_command_id(&mut entry, None);
    set_provenance(&mut entry, None, None, None);

    // The nexus namespace is still present and non-null (world_id remains).
    let nexus_key = KnowledgeEntryExtensionsKey::try_from("nexus").unwrap();
    let ns = entry.extensions.get(&nexus_key);
    assert!(
        ns.is_some(),
        "empty-but-present nexus namespace is not dropped"
    );
    let ns = ns.expect("namespace present");
    assert_eq!(ns.len(), 1, "only world_id remains");
    assert_eq!(ns.get("world_id"), Some(&json!("wld_temp")));
}

// ── build_extensions_nexus ─────────────────────────────────────────────

fn ext_map(from: &Value) -> ExtensionMap {
    from.as_object()
        .expect("extension map root is an object")
        .iter()
        .map(|(namespace, fields)| {
            let inner: Map<String, Value> = fields
                .as_object()
                .expect("namespace value is an object")
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            (namespace.clone(), inner)
        })
        .collect()
}

#[test]
fn build_extensions_nexus_writes_typed_fields_and_preserves_unknown_keys() {
    // existing_extensions carries an unknown key in the nexus namespace and
    // a sibling namespace that must be carried through untouched by callers.
    let existing = ext_map(&json!({
        "nexus": { "future_field": "keep-me", "world_id": "STALE" },
        "other_product": { "book_id": "bk_1" }
    }));

    let value = build_extensions_nexus(
        "wld_abc",
        Some("cmd_xyz"),
        Some("wrk_def"),
        Some(3),
        Some("review_time_extract"),
        &existing,
    );

    let ns = value.as_object().expect("nexus namespace is an object");
    assert_eq!(
        ns.get("world_id"),
        Some(&json!("wld_abc")),
        "typed world_id overrides stale"
    );
    assert_eq!(ns.get("created_from_command_id"), Some(&json!("cmd_xyz")));
    assert_eq!(ns.get("source_work_id"), Some(&json!("wrk_def")));
    assert_eq!(ns.get("source_chapter"), Some(&json!(3)));
    assert_eq!(
        ns.get("source_provenance_kind"),
        Some(&json!("review_time_extract"))
    );
    assert_eq!(
        ns.get("future_field"),
        Some(&json!("keep-me")),
        "unknown nexus key preserved verbatim"
    );
}

#[test]
fn build_extensions_nexus_omits_optional_fields_when_none_and_removes_stale() {
    let existing = ext_map(&json!({
        "nexus": {
            "source_work_id": "STALE",
            "source_chapter": 99,
            "future_field": "keep-me"
        }
    }));

    let value = build_extensions_nexus("wld_abc", None, None, None, None, &existing);

    let ns = value.as_object().expect("nexus namespace is an object");
    assert_eq!(
        ns.get("world_id"),
        Some(&json!("wld_abc")),
        "world_id always inserted"
    );
    assert!(
        ns.get("created_from_command_id").is_none(),
        "None removes/omits known key"
    );
    assert!(
        ns.get("source_work_id").is_none(),
        "None removes stale known value (typed fields are authoritative)"
    );
    assert!(ns.get("source_chapter").is_none());
    assert!(ns.get("source_provenance_kind").is_none());
    assert_eq!(
        ns.get("future_field"),
        Some(&json!("keep-me")),
        "unknown key still preserved"
    );
}

#[test]
fn build_extensions_nexus_handles_missing_namespace() {
    let existing = ExtensionMap::new();

    let value = build_extensions_nexus(
        "wld_abc",
        Some("cmd_xyz"),
        None,
        None,
        Some("manual"),
        &existing,
    );

    let ns = value.as_object().expect("nexus namespace is an object");
    assert_eq!(ns.get("world_id"), Some(&json!("wld_abc")));
    assert_eq!(ns.get("created_from_command_id"), Some(&json!("cmd_xyz")));
    assert_eq!(ns.get("source_provenance_kind"), Some(&json!("manual")));
}
