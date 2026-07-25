//! Typed accessors over the `extensions.nexus` namespace on a spoke
//! [`KnowledgeEntry`].
//!
//! The `extensions.nexus` namespace carries the nexus-local fields that
//! spoke deliberately keeps out of its core `KnowledgeEntry` schema (tracked
//! spec §2.1). The namespace key is the literal string `"nexus"` (lowercase;
//! matches spoke's `^[a-z][a-z0-9_-]*$` namespace convention).
//!
//! ## Field inventory (5)
//!
//! | Field | JSON type | Required |
//! |-------|-----------|----------|
//! | `world_id` | string | yes |
//! | `created_from_command_id` | string | no |
//! | `source_work_id` | string | no |
//! | `source_chapter` | integer | no |
//! | `source_provenance_kind` | string | no |
//!
//! ## Round-trip preservation
//!
//! - Unknown namespaces in `extensions` are preserved verbatim on
//!   read→modify→write cycles — the accessors only ever touch the `"nexus"`
//!   namespace key.
//! - Unknown keys inside `extensions.nexus` are preserved verbatim — the
//!   typed setters only insert/remove the 5 known keys listed above.
//! - Empty `extensions.nexus` (`{}`) is valid and is not dropped.
//!
//! See tracked spec §2.2 for the full round-trip contract.

use serde_json::{Map, Value};
use spoke_operations::ExtensionMap;
use spoke_schemas::knowledge_entry::KnowledgeEntryExtensionsKey;
use spoke_schemas::KnowledgeEntry;

/// The `extensions.nexus` namespace key (lowercase, matches the
/// `^[a-z][a-z0-9_-]*$` namespace convention).
const NAMESPACE: &str = "nexus";

/// Construct the typed namespace lookup key for the `"nexus"` namespace.
///
/// `KnowledgeEntry.extensions` is keyed by the newtype
/// `KnowledgeEntryExtensionsKey` (typify-generated, regex-validated). The
/// literal `"nexus"` always satisfies the regex, so construction is
/// infallible at runtime. The type does not implement `Borrow<str>`, so a
/// `HashMap::get("nexus")` lookup does not compile — this helper bridges
/// that gap.
fn nexus_key() -> KnowledgeEntryExtensionsKey {
    KnowledgeEntryExtensionsKey::try_from(NAMESPACE)
        .expect("\"nexus\" matches the ^[a-z][a-z0-9_-]*$ namespace regex")
}

/// Borrow the `extensions.nexus` namespace object from a [`KnowledgeEntry`],
/// or `None` if the namespace is absent.
fn nexus_namespace(entry: &KnowledgeEntry) -> Option<&Map<String, Value>> {
    entry.extensions.get(&nexus_key())
}

/// Read `extensions.nexus.world_id` from a [`KnowledgeEntry`].
#[must_use]
pub fn get_world_id(entry: &KnowledgeEntry) -> Option<&str> {
    nexus_namespace(entry)
        .and_then(|ns| ns.get("world_id"))
        .and_then(Value::as_str)
}

/// Set `extensions.nexus.world_id` on a [`KnowledgeEntry`].
///
/// Mutates the entry in place. Preserves unknown keys already present in
/// `extensions.nexus` and any sibling namespaces under `extensions`.
pub fn set_world_id(entry: &mut KnowledgeEntry, world_id: String) {
    let ns = entry.extensions.entry(nexus_key()).or_default();
    ns.insert("world_id".into(), Value::String(world_id));
}

/// Read `extensions.nexus.created_from_command_id`.
#[must_use]
pub fn get_created_from_command_id(entry: &KnowledgeEntry) -> Option<&str> {
    nexus_namespace(entry)
        .and_then(|ns| ns.get("created_from_command_id"))
        .and_then(Value::as_str)
}

/// Set `extensions.nexus.created_from_command_id`.
///
/// Mutates the entry in place. Pass `None` to remove the key. Preserves
/// unknown keys already present in `extensions.nexus`.
pub fn set_created_from_command_id(entry: &mut KnowledgeEntry, command_id: Option<String>) {
    let ns = entry.extensions.entry(nexus_key()).or_default();
    match command_id {
        Some(value) => ns.insert("created_from_command_id".into(), Value::String(value)),
        None => ns.remove("created_from_command_id"),
    };
}

/// Read provenance fields as `(source_work_id, source_chapter, source_provenance_kind)`.
#[must_use]
pub fn get_provenance(entry: &KnowledgeEntry) -> (Option<&str>, Option<i64>, Option<&str>) {
    let ns = nexus_namespace(entry);
    (
        ns.and_then(|m| m.get("source_work_id"))
            .and_then(Value::as_str),
        ns.and_then(|m| m.get("source_chapter"))
            .and_then(Value::as_i64),
        ns.and_then(|m| m.get("source_provenance_kind"))
            .and_then(Value::as_str),
    )
}

/// Set the three provenance fields together.
///
/// Mutates the entry in place. Each `None` removes the corresponding key.
/// Preserves unknown keys already present in `extensions.nexus`.
pub fn set_provenance(
    entry: &mut KnowledgeEntry,
    source_work_id: Option<String>,
    source_chapter: Option<i64>,
    source_provenance_kind: Option<String>,
) {
    let ns = entry.extensions.entry(nexus_key()).or_default();
    match source_work_id {
        Some(value) => ns.insert("source_work_id".into(), Value::String(value)),
        None => ns.remove("source_work_id"),
    };
    match source_chapter {
        Some(value) => ns.insert("source_chapter".into(), Value::Number(value.into())),
        None => ns.remove("source_chapter"),
    };
    match source_provenance_kind {
        Some(value) => ns.insert("source_provenance_kind".into(), Value::String(value)),
        None => ns.remove("source_provenance_kind"),
    };
}

/// Build the `extensions.nexus` namespace object from typed nexus fields.
///
/// Returns the namespace value as a [`serde_json::Value`] (always an object
/// when serialized). The 5 typed fields are written authoritatively:
///
/// - `world_id` is always inserted (required).
/// - Each optional field is inserted when `Some`, removed when `None`.
///
/// Unknown keys already present under the `"nexus"` namespace of
/// `existing_extensions` are preserved verbatim — they are carried over
/// before the typed fields are applied. This is the round-trip guarantee
/// from tracked spec §2.2 rule 2.
///
/// Operates on the wire-neutral [`ExtensionMap`] shape (plain `String`
/// namespace keys). The caller is responsible for any
/// `KnowledgeEntryExtensionsKey` ↔ `String` conversion at the storage
/// boundary.
#[must_use]
pub fn build_extensions_nexus(
    world_id: &str,
    created_from_command_id: Option<&str>,
    source_work_id: Option<&str>,
    source_chapter: Option<i64>,
    source_provenance_kind: Option<&str>,
    existing_extensions: &ExtensionMap,
) -> Value {
    let mut nexus = existing_extensions
        .get(NAMESPACE)
        .cloned()
        .unwrap_or_default();

    nexus.insert("world_id".into(), Value::String(world_id.to_owned()));
    insert_opt_string(
        &mut nexus,
        "created_from_command_id",
        created_from_command_id,
    );
    insert_opt_string(&mut nexus, "source_work_id", source_work_id);
    insert_opt_i64(&mut nexus, "source_chapter", source_chapter);
    insert_opt_string(&mut nexus, "source_provenance_kind", source_provenance_kind);

    Value::Object(nexus)
}

/// Insert a string field when `Some(value)`, remove it when `None`.
fn insert_opt_string(nexus: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    match value {
        Some(v) => nexus.insert(key.into(), Value::String(v.to_owned())),
        None => nexus.remove(key),
    };
}

/// Insert an integer field when `Some(value)`, remove it when `None`.
fn insert_opt_i64(nexus: &mut Map<String, Value>, key: &str, value: Option<i64>) {
    match value {
        Some(v) => nexus.insert(key.into(), Value::Number(v.into())),
        None => nexus.remove(key),
    };
}
