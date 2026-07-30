//! `kb_relationships` row → spoke [`Relation`] conversion seam.
//!
//! This is the **sole reverse-mapping seam** between the nexus-local-db
//! `KbRelationshipRow` storage type and the spoke standard `Relation` wire
//! type (analogous to [`super::world_kb_to_spoke`] for `WorldKbEntry`). The
//! production [`RelationPort`](crate::RelationPort) impl in
//! `adapter/relation_port.rs` uses this same function for get / create-return
//! / update-return, and V1.146 P3 T2's CLI pack exporter uses it to convert
//! bulk-fetched rows into pack atoms — no second hand-rolled mapping.
//!
//! # Wire ↔ row mapping
//!
//! See `adapter/relation_port.rs` module docs for the field-by-field table.
//!
//! # `extensions.nexus`
//!
//! The nexus-local columns (`world_id`, `symmetric`, `confidence`,
//! `source_anchor_ids`, `needs_review`, `source`) ride under the `nexus`
//! namespace on the spoke type, consistent with the `WorldKbEntry` seam.
//!
//! V1.146 P5 T2: the `extensions_nexus_json` column preserves unknown
//! `extensions.nexus` keys across the `SQLite` round-trip. On read, the JSON
//! is merged underneath the 6 typed columns (which remain authoritative); any
//! key in the JSON not among the 6 known nexus-locals survives as-is on the
//! spoke `Relation.extensions["nexus"]` namespace.

use std::collections::HashMap;
use std::num::NonZeroU64;

use nexus_local_db::kb_relationships::KbRelationshipRow;
use serde_json::{Map, Value};
use spoke_schemas::relation::RelationExtensionsKey;
use spoke_schemas::Relation;

/// Build the merged `extensions.nexus` namespace for a `kb_relationships` row.
///
/// V1.146 P5 T2: the `extensions_nexus_json` column preserves unknown keys
/// across the `SQLite` round-trip. On read, the JSON (if present) is the base;
/// the 6 typed columns are then overlaid so they remain authoritative. Any key
/// in the JSON that is not among the 6 known nexus-locals survives as-is.
fn build_relation_nexus_ns(row: &KbRelationshipRow) -> Map<String, Value> {
    // Start from the stored JSON carrier (unknown keys + a snapshot of known
    // keys from the last write). If absent, start empty.
    let mut ns = row
        .extensions_nexus_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<Map<String, Value>>(s).ok())
        .unwrap_or_default();

    // Overlay the 6 typed columns (authoritative).
    ns.insert("world_id".to_string(), Value::String(row.world_id.clone()));
    ns.insert("symmetric".to_string(), Value::Bool(row.symmetric != 0));
    if let Some(c) = row.confidence {
        let v = serde_json::Number::from_f64(c).map_or(Value::Null, Value::Number);
        ns.insert("confidence".to_string(), v);
    } else {
        ns.remove("confidence");
    }
    ns.insert(
        "source_anchor_ids".to_string(),
        Value::Array(
            parse_anchor_ids(row.source_anchor_ids.as_deref())
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    ns.insert(
        "needs_review".to_string(),
        Value::Bool(row.needs_review != 0),
    );
    ns.insert("source".to_string(), Value::String(row.source.clone()));

    ns
}

/// Project a `kb_relationships` row onto a spoke [`Relation`].
///
/// `schema_version` is set to the spoke 0.5.0 relation schema version (1).
/// Timestamps (`created_at`, `updated_at`) are best-effort parsed into UTC
/// `DateTime<Utc>`; unparseable strings are left as `None` rather than
/// rejecting the row (rows may carry legacy-format strings from pre-migration
/// data, and losing the timestamp is less harmful than dropping the relation
/// from exports).
///
/// # `extensions.nexus` merge (V1.146 P5 T2)
///
/// The `extensions_nexus_json` column is merged underneath the 6 typed
/// columns (which stay authoritative). Unknown keys in the JSON survive
/// as-is, so a create→get round-trip preserves any key outside the 6
/// nexus-locals.
///
/// # Panics
///
/// Panics if the literal string `"nexus"` ever fails the
/// `RelationExtensionsKey` regex — impossible in practice (the regex
/// accepts any `^[a-z][a-z0-9_-]*$` string).
#[must_use]
pub fn kb_relationship_row_to_spoke(row: &KbRelationshipRow) -> Relation {
    let metadata = row
        .metadata
        .as_deref()
        .and_then(|s| serde_json::from_str::<Map<String, Value>>(s).ok())
        .unwrap_or_default();

    let created_at = row
        .created_at
        .parse::<chrono::DateTime<chrono::FixedOffset>>()
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let updated_at = row
        .updated_at
        .parse::<chrono::DateTime<chrono::FixedOffset>>()
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let nexus_ns = build_relation_nexus_ns(row);

    let mut extensions = HashMap::new();
    // `"nexus"` always satisfies the `RelationExtensionsKey` regex — the
    // conversion is infallible at runtime (mirrors the V1.139
    // `KnowledgeEntryExtensionsKey` pattern).
    let key = RelationExtensionsKey::try_from("nexus")
        .expect("\"nexus\" matches the extensions-key regex");
    extensions.insert(key, nexus_ns);

    Relation {
        schema_version: NonZeroU64::new(1).expect("1 is non-zero"),
        relation_id: row.relationship_id.clone(),
        from_id: row.source_entity_id.clone(),
        to_id: row.target_entity_id.clone(),
        relation_type: row.relation_type.clone(),
        label: row.custom_label.clone(),
        metadata,
        revision: Some(u64::try_from(row.revision).unwrap_or(0)),
        created_at,
        updated_at,
        extensions,
    }
}

/// Parse the stored `source_anchor_ids` JSON-array column back into a
/// `Vec<String>`; empty when the column is NULL or unparseable.
fn parse_anchor_ids(stored: Option<&str>) -> Vec<String> {
    stored
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}
