//! Production `RelationPort` impl — routes `kb_relationships` storage
//! through spoke's port surface (spec §7.4).
//!
//! # Wire ↔ row mapping
//!
//! There is no second conversion seam for `Relation` analogous to the
//! V1.139 `WorldKbEntry ↔ KnowledgeEntry` pair — spoke's `Relation`
//! wire type maps directly onto the nexus `kb_relationships` row at
//! this boundary:
//!
//! | Spoke `Relation` field | Nexus `kb_relationships` column |
//! |------------------------|---------------------------------|
//! | `relation_id`          | `relationship_id`               |
//! | `from_id`              | `source_entity_id`              |
//! | `to_id`                | `target_entity_id`              |
//! | `relation_type`        | `relation_type`                 |
//! | `label`                | `custom_label`                  |
//! | `metadata`             | `metadata`                      |
//! | `created_at`           | `created_at`                    |
//! | `updated_at`           | `updated_at`                    |
//! | `extensions.nexus.world_id` | `world_id` (required FK)  |
//!
//! Nexus-specific columns with no spoke equivalent default at insert:
//! `symmetric = false`, `confidence = NULL`, `source_anchor_ids = '[]'`,
//! `revision = 0`, `needs_review = false`, `source = 'manual'`. These
//! match the V1.76 manual-author add path — spoke `Relation`s are
//! first-class author assertions, not extraction suggestions (the
//! V1.76 `upsert_extraction_relationship` helper is the extraction
//! path; it is unrelated to this port).
//!
//! # World id provenance
//!
//! `kb_relationships.world_id` is a required FK to `narrative_worlds`.
//! The spoke `Relation` has no first-class world field; the adapter
//! extracts it from `extensions.nexus.world_id` (the same namespace
//! pattern used by the V1.139 `KnowledgeEntry` conversion seam).
//! When absent, the put rejects with `INVALID_INPUT` — the spoke port
//! surface cannot persist a relation without a world scope.

use super::NexusBaselineAdapter;
use crate::kb_relationships::{insert_relationship_in_tx, InsertRelationshipParams, SOURCE_MANUAL};
use nexus_spoke_adapter::{
    Relation, RelationExtensionsKey, RelationPort, SpokeReject, SpokeRejectCode, SpokeResult,
};
use serde_json::{json, Map, Value};

impl RelationPort for NexusBaselineAdapter {
    fn put_relation(&self, relation: Relation) -> SpokeResult<Relation> {
        let pool = self.pool.clone();
        self.block_on(async move {
            let relation_id = relation.relation_id.clone();
            let from_id = relation.from_id.clone();
            let to_id = relation.to_id.clone();
            let relation_type = relation.relation_type.clone();
            let custom_label = relation.label.clone();
            let metadata_value = if relation.metadata.is_empty() {
                None
            } else {
                Some(Value::Object(relation.metadata.clone()))
            };
            let created_at_src = relation.created_at;
            let updated_at_src = relation.updated_at;
            let ext_world_id = extract_world_id(&relation).map(String::from);

            let Some(world_id) = ext_world_id else {
                return reject(
                    SpokeRejectCode::InvalidInput,
                    format!(
                        "Relation is missing required extensions.nexus.world_id: {relation_id}"
                    ),
                    json!({
                        "relation_id": relation_id,
                        "missing": ["extensions.nexus.world_id"],
                    }),
                );
            };

            let now = chrono::Utc::now().to_rfc3339();
            let created_at = created_at_src.map_or_else(|| now.clone(), |dt| dt.to_rfc3339());
            let updated_at = updated_at_src.map_or_else(|| now.clone(), |dt| dt.to_rfc3339());

            let mut tx = match pool.begin().await {
                Ok(tx) => tx,
                Err(e) => {
                    return reject(
                        SpokeRejectCode::InvalidInput,
                        format!("storage error on tx begin: {e}"),
                        json!({ "relation_id": relation_id }),
                    );
                }
            };

            let params = InsertRelationshipParams {
                relationship_id: relation_id.clone(),
                world_id,
                source_entity_id: from_id,
                target_entity_id: to_id,
                relation_type,
                custom_label,
                symmetric: false,
                confidence: None,
                source_anchor_ids: Vec::new(),
                metadata: metadata_value,
                created_at: created_at.clone(),
                updated_at: updated_at.clone(),
                needs_review: false,
                source: SOURCE_MANUAL.to_string(),
            };

            if let Err(e) = insert_relationship_in_tx(&mut tx, &params).await {
                return reject(
                    SpokeRejectCode::InvalidInput,
                    format!("storage error on relation insert: {e}"),
                    json!({ "relation_id": params.relationship_id }),
                );
            }

            if let Err(e) = tx.commit().await {
                return reject(
                    SpokeRejectCode::InvalidInput,
                    format!("storage error on tx commit: {e}"),
                    json!({ "relation_id": params.relationship_id }),
                );
            }

            // Reflect adapter-assigned timestamps back onto the returned
            // relation so callers see what was actually persisted (spoke
            // `Relation` has no revision field, so there is no DB-assigned
            // revision to project).
            let mut result = relation;
            if result.created_at.is_none() {
                result.created_at = parse_rfc3339(&created_at);
            }
            if result.updated_at.is_none() {
                result.updated_at = parse_rfc3339(&updated_at);
            }
            SpokeResult::Ok(result)
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Borrow the `extensions.nexus.world_id` string from a spoke `Relation`,
/// or `None` when the namespace/key is absent. The `"nexus"` literal
/// always satisfies the `RelationExtensionsKey` regex — the conversion
/// is infallible at runtime (mirrors the V1.139
/// `KnowledgeEntryExtensionsKey` pattern).
fn extract_world_id(relation: &Relation) -> Option<&str> {
    let key = RelationExtensionsKey::try_from("nexus").ok()?;
    relation
        .extensions
        .get(&key)
        .and_then(|ns| ns.get("world_id"))
        .and_then(Value::as_str)
}

/// Parse an RFC 3339 timestamp string back into a UTC `DateTime`, or
/// `None` on parse failure. Used only to reflect adapter-assigned
/// timestamps back onto the returned `Relation` when the caller omitted
/// them — parse failure is benign (the column was persisted; only the
/// returned wire value keeps its `None`).
fn parse_rfc3339(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

/// Construct a `SpokeResult::Reject` (mirrors the helper in
/// `knowledge_entry_port.rs`).
fn reject<T>(code: SpokeRejectCode, message: impl Into<String>, details: Value) -> SpokeResult<T> {
    let details_map = match details {
        Value::Object(map) => Some(map),
        other => {
            let mut map = Map::new();
            map.insert("detail".to_string(), other);
            Some(map)
        }
    };
    SpokeResult::Reject(SpokeReject {
        code,
        message: message.into(),
        details: details_map,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kb_relationships::{get_relationship, list_relationships_for_world};
    use crate::{open_pool, run_migrations};
    use nexus_spoke_adapter::RelationPort;
    use serde_json::json;

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_world_and_endpoints(pool: &sqlx::SqlitePool) {
        // SAFETY: test-only static INSERTs with bind params; mirrors
        // the kb_relationships test fixture (creators + world + two
        // kb_key_blocks rows that act as endpoints).
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
             VALUES ('wld_rel', 'wrk_test', 'ctr_test', 'Test World', 'test-world', 'active', 'private', 'manual', '{}')",
        )
        .execute(pool)
        .await
        .unwrap();
        for id in ["kb_src", "kb_dst"] {
            sqlx::query(
                "INSERT INTO kb_key_blocks \
                 (key_block_id, world_id, block_type, canonical_name, status) \
                 VALUES (?, 'wld_rel', 'character', ?, 'confirmed')",
            )
            .bind(id)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    /// Build a spoke `Relation` fixture with `extensions.nexus.world_id`
    /// set, so the adapter can persist it (`world_id` is a required FK).
    fn spoke_relation(relation_id: &str, from_id: &str, to_id: &str) -> Relation {
        serde_json::from_value(json!({
            "schema_version": 1,
            "relation_id": relation_id,
            "from_id": from_id,
            "to_id": to_id,
            "relation_type": "allied_with",
            "label": "Alice ↔ Bob",
            "metadata": { "confidence": "high" },
            "extensions": {
                "nexus": {
                    "world_id": "wld_rel"
                }
            }
        }))
        .expect("valid spoke Relation fixture")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_happy_path_persists_row() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool.clone());
        let relation = spoke_relation("rel_happy", "kb_src", "kb_dst");

        let result = adapter.put_relation(relation);
        let returned = match result {
            SpokeResult::Ok(r) => r,
            SpokeResult::Reject(err) => panic!("expected ok, got reject: {err:?}"),
        };
        assert_eq!(returned.relation_id, "rel_happy");
        assert_eq!(returned.from_id, "kb_src");
        assert_eq!(returned.to_id, "kb_dst");
        assert_eq!(returned.relation_type, "allied_with");

        // Verify the row landed with the expected nexus column mapping.
        let row = get_relationship(&pool, "rel_happy")
            .await
            .expect("row persisted");
        assert_eq!(row.relationship_id, "rel_happy");
        assert_eq!(row.world_id, "wld_rel");
        assert_eq!(row.source_entity_id, "kb_src");
        assert_eq!(row.target_entity_id, "kb_dst");
        assert_eq!(row.relation_type, "allied_with");
        assert_eq!(
            row.custom_label.as_deref(),
            Some("Alice ↔ Bob"),
            "spoke `label` maps to nexus `custom_label`"
        );
        assert_eq!(
            row.symmetric, 0,
            "spoke Relation has no symmetric field — defaults to false"
        );
        assert_eq!(
            row.source, "manual",
            "spoke Relation ports through the manual-author path"
        );
        assert_eq!(row.revision, 0, "initial revision is 0");
        assert!(
            row.metadata.is_some(),
            "spoke `metadata` is persisted to the nexus `metadata` column"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_missing_world_id_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        // Build a relation without extensions.nexus.world_id.
        let relation: Relation = serde_json::from_value(json!({
            "schema_version": 1,
            "relation_id": "rel_no_world",
            "from_id": "kb_src",
            "to_id": "kb_dst",
            "relation_type": "allied_with",
            "extensions": {}
        }))
        .expect("valid minimal Relation");

        match adapter.put_relation(relation) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "missing world_id must reject with INVALID_INPUT"
                );
                assert_eq!(
                    r.details.as_ref().and_then(|d| d.get("relation_id")),
                    Some(&json!("rel_no_world"))
                );
            }
            SpokeResult::Ok(_) => panic!("expected INVALID_INPUT reject"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_unknown_endpoint_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool.clone());
        let relation = spoke_relation("rel_bad_endpoint", "kb_src", "kb_nonexistent");

        match adapter.put_relation(relation) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "FK violation on target endpoint must surface as INVALID_INPUT"
                );
            }
            SpokeResult::Ok(_) => panic!("expected INVALID_INPUT reject"),
        }

        // The transaction must have rolled back: no row exists.
        let rows = list_relationships_for_world(&pool, "wld_rel", true, 100)
            .await
            .unwrap();
        assert!(rows.is_empty(), "tx rolled back on FK violation");
    }
}
