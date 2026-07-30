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
//! Unknown extension keys are not carried — the `kb_relationships` table has
//! no extras-JSON column (pre-existing schema limitation, out of scope).

use std::collections::HashMap;
use std::num::NonZeroU64;

use nexus_local_db::kb_relationships::KbRelationshipRow;
use serde_json::{Map, Value};
use spoke_schemas::relation::RelationExtensionsKey;
use spoke_schemas::Relation;

/// Project a `kb_relationships` row onto a spoke [`Relation`].
///
/// `schema_version` is set to the spoke 0.5.0 relation schema version (1).
/// Timestamps (`created_at`, `updated_at`) are best-effort parsed into UTC
/// `DateTime<Utc>`; unparseable strings are left as `None` rather than
/// rejecting the row (rows may carry legacy-format strings from pre-migration
/// data, and losing the timestamp is less harmful than dropping the relation
/// from exports).
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

    let mut nexus_ns = Map::new();
    nexus_ns.insert("world_id".to_string(), Value::String(row.world_id.clone()));
    nexus_ns.insert("symmetric".to_string(), Value::Bool(row.symmetric != 0));
    if let Some(c) = row.confidence {
        let v = serde_json::Number::from_f64(c).map_or(Value::Null, Value::Number);
        nexus_ns.insert("confidence".to_string(), v);
    }
    nexus_ns.insert(
        "source_anchor_ids".to_string(),
        Value::Array(
            parse_anchor_ids(row.source_anchor_ids.as_deref())
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    nexus_ns.insert(
        "needs_review".to_string(),
        Value::Bool(row.needs_review != 0),
    );
    nexus_ns.insert("source".to_string(), Value::String(row.source.clone()));

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
