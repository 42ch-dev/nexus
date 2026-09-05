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

use nexus_knowledge::world_kb::errors::KbError;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeOwnerRef;
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

/// The 8 typed identity/owner field names managed by the accessors in this
/// module (v1.184 P1 adds the canonical owner keys + `creator_only`).
const KNOWN_NEXUS_KEYS: [&str; 8] = [
    "world_id",
    "character_id",
    "actor_world_binding_id",
    "creator_only",
    "created_from_command_id",
    "source_work_id",
    "source_chapter",
    "source_provenance_kind",
];

/// Returns `true` if `key` is one of the 8 typed `extensions.nexus` identity
/// / owner fields managed by the accessors in this module.
///
/// This is the single source of truth for the typed/unknown key boundary
/// (spec §2.2 round-trip rule 2); storage and conversion-seam callers use it
/// to separate authoritative typed columns from verbatim-carried extras.
#[must_use]
pub fn is_known_nexus_key(key: &str) -> bool {
    KNOWN_NEXUS_KEYS.contains(&key)
}

/// Read the *unknown* keys under `extensions.nexus` — everything outside the
/// 8 typed identity/owner fields. Returns an owned map; `None` when no
/// unknown keys are present (or the namespace is absent).
///
/// Used by the conversion seam reverse direction to surface product-local
/// extras onto the nexus domain type so they survive the spoke round-trip.
#[must_use]
pub fn get_nexus_extras(entry: &KnowledgeEntry) -> Option<Map<String, Value>> {
    let ns = nexus_namespace(entry)?;
    let extras: Map<String, Value> = ns
        .iter()
        .filter(|(k, _)| !is_known_nexus_key(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    (!extras.is_empty()).then_some(extras)
}

/// Insert unknown keys into `extensions.nexus`, preserving typed keys.
///
/// The 5 typed keys (already set authoritatively by [`set_world_id`] /
/// [`set_provenance`] / [`set_created_from_command_id`]) and any unknown key
/// already present are preserved; only keys outside the typed set are inserted.
///
/// Used by the conversion seam forward direction to carry product-local extras
/// onto the spoke boundary type.
pub fn set_nexus_extras(entry: &mut KnowledgeEntry, extras: &Map<String, Value>) {
    if extras.is_empty() {
        return;
    }
    let ns = entry.extensions.entry(nexus_key()).or_default();
    for (k, v) in extras {
        if !is_known_nexus_key(k) {
            ns.insert(k.clone(), v.clone());
        }
    }
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

/// Read the canonical owner from `extensions.nexus` (v1.184 P1).
///
/// The owner representation is closed: exactly **one** typed owner key
/// (`world_id` / `character_id` / `actor_world_binding_id`) must be present
/// and carry a string value. This fails closed instead of resolving by
/// precedence — a malformed entry that carries multiple typed owner keys, or
/// any owner key holding a non-string/null value, is rejected rather than
/// silently accepting one claim and discarding the other.
///
/// # Errors
/// - [`KbError::MissingOwner`] when no typed owner key is present.
/// - [`KbError::InvalidOwnerMetadata`] when more than one typed owner key is
///   present, or an owner key is present but not a string (including `null`).
pub fn get_owner(entry: &KnowledgeEntry) -> Result<KnowledgeOwnerRef, KbError> {
    let ns = nexus_namespace(entry).ok_or(KbError::MissingOwner)?;
    // Collect the present, string-typed owner claims as `(key, value)`,
    // rejecting a present-but-non-string/null owner key outright.
    let mut claims: Vec<(&str, &str)> = Vec::with_capacity(3);
    for key in ["world_id", "character_id", "actor_world_binding_id"] {
        match ns.get(key) {
            None => {}
            Some(Value::String(s)) => claims.push((key, s)),
            Some(Value::Null) => {
                return Err(KbError::InvalidOwnerMetadata(format!(
                    "extensions.nexus.{key} is null"
                )));
            }
            Some(_) => {
                return Err(KbError::InvalidOwnerMetadata(format!(
                    "extensions.nexus.{key} must be a string"
                )));
            }
        }
    }

    match claims.len() {
        0 => Err(KbError::MissingOwner),
        1 => {
            let (key, id) = claims
                .pop()
                .expect("len == 1 as matched above");
            Ok(match key {
                "world_id" => KnowledgeOwnerRef::world(id),
                "character_id" => KnowledgeOwnerRef::character(id),
                "actor_world_binding_id" => KnowledgeOwnerRef::actor_world_binding(id),
                _ => unreachable!("only the three typed owner keys are collected"),
            })
        }
        _ => Err(KbError::InvalidOwnerMetadata(
            "multiple typed owner keys present (world_id/character_id/actor_world_binding_id) \
             — the entry is ambiguous"
                .to_string(),
        )),
    }
}

/// Read the `creator_only` flag from `extensions.nexus` (default `false`).
#[must_use]
pub fn get_creator_only(entry: &KnowledgeEntry) -> bool {
    nexus_namespace(entry)
        .and_then(|m| m.get("creator_only"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Set the canonical owner on `extensions.nexus`, removing the other two
/// owner keys so the projection is unambiguous (v1.184 P1).
///
/// World owners emit `world_id`; Character owners emit `character_id`;
/// binding owners emit `actor_world_binding_id`. A non-World owner NEVER
/// carries a `world_id` key (no fabricated World id). When `creator_only` is
/// `true` the flag is emitted; otherwise the key is removed. Unknown keys
/// already present in `extensions.nexus` are preserved verbatim.
pub fn set_owner(entry: &mut KnowledgeEntry, owner: &KnowledgeOwnerRef, creator_only: bool) {
    let ns = entry.extensions.entry(nexus_key()).or_default();
    ns.remove("world_id");
    ns.remove("character_id");
    ns.remove("actor_world_binding_id");
    match owner {
        KnowledgeOwnerRef::World(id) => {
            ns.insert("world_id".into(), Value::String(id.clone()));
        }
        KnowledgeOwnerRef::Character(id) => {
            ns.insert("character_id".into(), Value::String(id.clone()));
        }
        KnowledgeOwnerRef::ActorWorldBinding(id) => {
            ns.insert("actor_world_binding_id".into(), Value::String(id.clone()));
        }
    }
    if creator_only {
        ns.insert("creator_only".into(), Value::Bool(true));
    } else {
        ns.remove("creator_only");
    }
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
/// when serialized). The owner is written authoritatively from
/// [`KnowledgeOwnerRef`] (World → `world_id`, Character → `character_id`,
/// binding → `actor_world_binding_id`); the other owner keys are removed so
/// a non-World owner never fabricates a `world_id`. `creator_only` is emitted
/// when set (World-owned only). Each optional provenance field is inserted
/// when `Some`, removed when `None`.
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
    owner: &KnowledgeOwnerRef,
    creator_only: bool,
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

    nexus.remove("world_id");
    nexus.remove("character_id");
    nexus.remove("actor_world_binding_id");
    match owner {
        KnowledgeOwnerRef::World(id) => {
            nexus.insert("world_id".into(), Value::String(id.clone()));
        }
        KnowledgeOwnerRef::Character(id) => {
            nexus.insert("character_id".into(), Value::String(id.clone()));
        }
        KnowledgeOwnerRef::ActorWorldBinding(id) => {
            nexus.insert("actor_world_binding_id".into(), Value::String(id.clone()));
        }
    }
    if creator_only {
        nexus.insert("creator_only".into(), Value::Bool(true));
    } else {
        nexus.remove("creator_only");
    }
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

/// Reserved `extensions.nexus` key carrying the full nexus `KnowledgeEntryBody`
/// losslessly across the spoke boundary.
///
/// Spoke's typed `BodyAttributeValue` only models string/number/bool
/// attribute values; null/array/object values have no spoke slot (see
/// `nexus_attr_to_spoke` in `nexus-knowledge`). The forward conversion stashes
/// the full nexus body here so the persist path (orchestrator → `put_update` →
/// reverse conversion) recovers it instead of the spoke-truncated body,
/// preserving full body fidelity. This key is reserved (`_nexus_` prefix); it
/// is consumed by [`take_nexus_body`] on the reverse path and never reaches the
/// product-local extras / the `extensions` DB column.
///
/// # Carrier boundary (HARD)
///
/// Only **two** production call sites may set this carrier:
/// - the **MCA read path** (`SpokeBackedKbStore` →
///   [`NexusAdapter::list_knowledge_entries_scoped`]), and
/// - the **persist path** (`build_spoke_upsert_request` in `nexus-daemon-runtime`,
///   so the orchestrator's `put_update` recovers the body).
///
/// The **spoke orchestrator read path** (`ScopeQueryPort::list_knowledge_entries`)
/// must NEVER carry this carrier: it returns spoke entries straight to the
/// orchestrator, so a leaked carrier would persist into the `extensions` DB
/// column. [`has_nexus_body`] backs the `debug_assert!` guard at that boundary.
const NEXUS_BODY_KEY: &str = "_nexus_body";

/// Stash the full nexus body (as JSON) under `extensions.nexus._nexus_body`.
///
/// Pass `None` to clear a previously-stashed carrier. Preserves the 5 typed
/// identity keys and any sibling unknown keys already present.
///
/// See [`NEXUS_BODY_KEY`] for the carrier-boundary contract — this must only be
/// called on the MCA read or persist paths.
pub fn set_nexus_body(entry: &mut KnowledgeEntry, body: Option<&Value>) {
    let ns = entry.extensions.entry(nexus_key()).or_default();
    match body {
        Some(v) => ns.insert(NEXUS_BODY_KEY.into(), v.clone()),
        None => ns.remove(NEXUS_BODY_KEY),
    };
}

/// Read-only check: does `entry` carry the reserved `_nexus_body` carrier?
///
/// Backs the carrier-boundary guard (see [`NEXUS_BODY_KEY`]): the spoke
/// orchestrator read path (`ScopeQueryPort::list_knowledge_entries`) asserts
/// none of its returned entries carry the carrier, so a future caller that
/// accidentally stashes one on a non-MCA path is caught at test time. Unlike
/// [`take_nexus_body`], this does not mutate.
#[must_use]
pub fn has_nexus_body(entry: &KnowledgeEntry) -> bool {
    entry
        .extensions
        .get(&nexus_key())
        .is_some_and(|ns| ns.contains_key(NEXUS_BODY_KEY))
}

/// Remove and return the reserved `_nexus_body` carrier from
/// `extensions.nexus`, if present.
///
/// Used by the conversion seam reverse direction to recover the full nexus
/// body losslessly. Taking (rather than borrowing) ensures the carrier does
/// not leak into [`get_nexus_extras`] or the persisted `extensions` column.
#[must_use]
pub fn take_nexus_body(entry: &mut KnowledgeEntry) -> Option<Value> {
    entry
        .extensions
        .get_mut(&nexus_key())?
        .remove(NEXUS_BODY_KEY)
}

/// Insert an integer field when `Some(value)`, remove it when `None`.
fn insert_opt_i64(nexus: &mut Map<String, Value>, key: &str, value: Option<i64>) {
    match value {
        Some(v) => nexus.insert(key.into(), Value::Number(v.into())),
        None => nexus.remove(key),
    };
}
