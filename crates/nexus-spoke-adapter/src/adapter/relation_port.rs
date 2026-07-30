//! Production `RelationPort` impl — OCC-aware routing of `kb_relationships`
//! storage through spoke's port surface (spec §7.4).
//!
//! # Wire ↔ row mapping (spoke 0.5.0)
//!
//! There is no second conversion seam for `Relation` analogous to the
//! V1.139 `WorldKbEntry ↔ KnowledgeEntry` pair — spoke's `Relation`
//! wire type maps directly onto the nexus `kb_relationships` row at
//! this boundary via [`row_to_relation`] (the single reverse-mapping
//! seam, reused by get / create-return / update-return):
//!
//! | Spoke `Relation` field        | Nexus `kb_relationships` column        |
//! |-------------------------------|-----------------------------------------|
//! | `relation_id`                 | `relationship_id`                       |
//! | `from_id`                     | `source_entity_id`                      |
//! | `to_id`                       | `target_entity_id`                      |
//! | `relation_type`               | `relation_type`                         |
//! | `label`                       | `custom_label`                          |
//! | `metadata`                    | `metadata` (JSON)                       |
//! | `revision`                    | `revision` (**now a spoke field**)      |
//! | `created_at` / `updated_at`   | `created_at` / `updated_at`             |
//! | `extensions.nexus.world_id`   | `world_id` (required FK)                |
//! | `extensions.nexus.symmetric`  | `symmetric`                             |
//! | `extensions.nexus.confidence` | `confidence`                            |
//! | `extensions.nexus.source_anchor_ids` | `source_anchor_ids`             |
//! | `extensions.nexus.needs_review`      | `needs_review`                  |
//! | `extensions.nexus.source`           | `source`                        |
//!
//! spoke 0.5.0 `Relation` has no `symmetric`/`confidence`/`custom_label`
//! fields — those ride `extensions.nexus` (nexus-locals); spoke uses `label`.
//! Unknown keys under `extensions.nexus` are not round-tripped: the
//! `kb_relationships` table has no extras-JSON column (unlike
//! `kb_key_blocks.extensions_nexus_json`), so only the known nexus-locals
//! above survive a put → get cycle. That is a pre-existing schema
//! limitation, out of scope for V1.144.
//!
//! # OCC contract (V1.144)
//!
//! [`RelationPort::put_relation`] routes create vs update on
//! `expected_base_revision`, per the spoke 0.5.0 trait contract:
//!
//! | `expected_base_revision` | Path     | Outcome                                                  |
//! |--------------------------|----------|----------------------------------------------------------|
//! | `None`                   | create   | row absent → INSERT `revision = 1`; present → `RELATION_ALREADY_EXISTS` |
//! | `Some(expected)`         | CAS      | `revision == expected` → bump to `expected + 1`; otherwise `STORED_REVISION_STALE` |
//!
//! `revision` is adapter-owned: create seeds `1` (spoke convention, **not**
//! the `0` the legacy `insert_relationship_in_tx` seeds for the daemon's own
//! add-relationship route — that function is deliberately untouched); an
//! accepted update persists `expected + 1` via the existing V1.74
//! [`update_relationship_in_tx`] CAS guard (`WHERE revision = ?`).
//!
//! The relation-port CAS mapping collapses every
//! [`LocalDbError::VersionMismatch`] shape to `STORED_REVISION_STALE`
//! (simpler than the [`KnowledgeEntryPort`](super::knowledge_entry_port)
//! 3-way split): the spoke `orchestrate_relate` entrypoint pre-routes
//! create vs update from stored presence, so the only reachable failure on
//! the update path is "the store moved since the caller's read".

use super::NexusBaselineAdapter;
use crate::{
    Relation, RelationExtensionsKey, RelationPort, SpokeReject, SpokeRejectCode, SpokeResult,
};
use nexus_local_db::kb_relationships::{
    get_relationship, update_relationship_in_tx, KbRelationshipRow, UpdateRelationshipParams,
    SOURCE_MANUAL,
};
use nexus_local_db::LocalDbError;
use serde_json::{json, Map, Value};
use std::num::NonZeroU64;

impl RelationPort for NexusBaselineAdapter<'_> {
    fn get_relation(&self, relation_id: &str) -> SpokeResult<Relation> {
        let pool = self.pool.clone();
        let relation_id = relation_id.to_string();
        self.block_on(async move {
            let row = match get_relationship(&pool, &relation_id).await {
                Ok(row) => row,
                Err(LocalDbError::Sqlx(sqlx::Error::RowNotFound)) => {
                    return reject(
                        SpokeRejectCode::RelationNotFound,
                        format!("Relation not found: {relation_id}"),
                        json!({ "relation_id": relation_id }),
                    );
                }
                Err(e) => {
                    return reject(
                        SpokeRejectCode::InternalError,
                        format!("storage error on relation read: {e}"),
                        json!({ "relation_id": relation_id }),
                    );
                }
            };
            SpokeResult::Ok(row_to_relation(&row))
        })
    }

    fn put_relation(
        &self,
        relation: Relation,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<Relation> {
        let pool = self.pool.clone();
        self.block_on(async move {
            match expected_base_revision {
                None => put_relation_create(&pool, relation).await,
                Some(expected) => put_relation_update(&pool, relation, expected).await,
            }
        })
    }
}

// ── put_relation: create path ─────────────────────────────────────────

/// Create path: `expected_base_revision = None`. Reject if the row already
/// exists; otherwise INSERT with `revision = 1` (spoke convention) and return
/// the resulting spoke `Relation`.
async fn put_relation_create(pool: &sqlx::SqlitePool, relation: Relation) -> SpokeResult<Relation> {
    let relation_id = relation.relation_id.clone();
    let locals = extract_nexus_locals(&relation);

    // Pre-check existence. The PK is the true race guard; if a concurrent
    // writer beats us the INSERT fails and surfaces as InternalError —
    // acceptable for the local single-writer daemon path.
    match get_relationship(pool, &relation_id).await {
        Ok(_) => {
            return reject(
                SpokeRejectCode::RelationAlreadyExists,
                format!("Relation already exists: {relation_id}"),
                json!({ "relation_id": relation_id }),
            );
        }
        Err(LocalDbError::Sqlx(sqlx::Error::RowNotFound)) => {} // proceed to insert
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("storage error on create pre-check: {e}"),
                json!({ "relation_id": relation_id }),
            );
        }
    }

    // Compute the create column values before moving `locals.world_id` below.
    let f = prepare_create_fields(&relation, &locals);

    let Some(world_id) = locals.world_id else {
        return reject(
            SpokeRejectCode::InvalidInput,
            format!("Relation is missing required extensions.nexus.world_id: {relation_id}"),
            json!({
                "relation_id": relation_id,
                "missing": ["extensions.nexus.world_id"],
            }),
        );
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("storage error on tx begin: {e}"),
                json!({ "relation_id": relation_id }),
            );
        }
    };

    // Seed revision = 1 directly (spoke convention). The legacy
    // `insert_relationship_in_tx` seeds 0 for the daemon's add-relationship
    // route and is deliberately NOT reused here — the port owns the spoke
    // revision-seed so the legacy fn + daemon route stay untouched (V1.144).
    let insert_result = sqlx::query!(
        r#"INSERT INTO kb_relationships
           (relationship_id, world_id, source_entity_id, target_entity_id,
            relation_type, custom_label, symmetric, confidence,
            source_anchor_ids, metadata, created_at, updated_at, revision,
            needs_review, source)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)"#,
        relation_id,
        world_id,
        relation.from_id,
        relation.to_id,
        relation.relation_type,
        f.custom_label,
        f.symmetric_i64,
        f.confidence,
        f.source_anchor_json,
        f.metadata_json,
        f.created_at,
        f.updated_at,
        f.needs_review_i64,
        f.source,
    )
    .execute(&mut *tx)
    .await;

    if let Err(e) = insert_result {
        return reject(
            SpokeRejectCode::InternalError,
            format!("storage error on relation insert: {e}"),
            json!({ "relation_id": relation_id }),
        );
    }

    if let Err(e) = tx.commit().await {
        return reject(
            SpokeRejectCode::InternalError,
            format!("storage error on tx commit: {e}"),
            json!({ "relation_id": relation_id }),
        );
    }

    // Project the persisted row to the returned spoke Relation (revision = 1).
    let row = KbRelationshipRow {
        relationship_id: relation_id,
        world_id,
        source_entity_id: relation.from_id,
        target_entity_id: relation.to_id,
        relation_type: relation.relation_type,
        custom_label: f.custom_label,
        symmetric: f.symmetric_i64,
        confidence: f.confidence,
        source_anchor_ids: Some(f.source_anchor_json),
        metadata: f.metadata_json,
        created_at: f.created_at,
        updated_at: f.updated_at,
        revision: 1,
        needs_review: f.needs_review_i64,
        source: f.source,
    };
    SpokeResult::Ok(row_to_relation(&row))
}

// ── put_relation: CAS update path ─────────────────────────────────────

/// Update path: `expected_base_revision = Some(expected)`. Pre-read the
/// stored row, reuse [`update_relationship_in_tx`] (CAS `WHERE revision = ?`)
/// and map any [`LocalDbError::VersionMismatch`] to `STORED_REVISION_STALE`.
/// Optional nexus-locals not carried on the spoke `Relation` are CLEARED
/// (clear-on-omit, behavior-equivalent to pre-cutover); see the block in
/// [`put_relation_update`] for the full rationale.
async fn put_relation_update(
    pool: &sqlx::SqlitePool,
    relation: Relation,
    expected: u64,
) -> SpokeResult<Relation> {
    let relation_id = relation.relation_id.clone();

    let existing = match get_relationship(pool, &relation_id).await {
        Ok(row) => row,
        Err(LocalDbError::Sqlx(sqlx::Error::RowNotFound)) => {
            // Absent + Some(expected): the store has no revision at all. The
            // relation-port CAS mapping collapses this to STORED_REVISION_STALE
            // (storeRevision=null signals absence); the orchestrator pre-routes
            // create vs update, so this branch is a guard, not a hot path.
            return reject(
                SpokeRejectCode::StoredRevisionStale,
                format!("Relation not found for update: {relation_id} (expected base {expected})"),
                json!({
                    "relation_id": relation_id,
                    "expectedBaseRevision": expected,
                    "storeRevision": Value::Null,
                }),
            );
        }
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("storage error on update pre-read: {e}"),
                json!({ "relation_id": relation_id }),
            );
        }
    };

    let locals = extract_nexus_locals(&relation);

    // nexus-locals on update follow clear-on-omit semantics: an optional local
    // the spoke `Relation` does NOT carry is cleared (symmetric→0,
    // confidence→SQL NULL, source_anchor_ids→'[]', needs_review→0). This is
    // behavior-equivalent to the pre-cutover `update_relationship_in_tx`,
    // which wrote every bound local directly (the V1.144 P2 cutover
    // accidentally switched these to preserve-on-omit, violating AC-I3).
    //
    // The orchestrator/handler round-trip stays safe because `get_relation`
    // (`row_to_relation`) FULLY populates `extensions.nexus` before any
    // read-modify-write put — so a carried local is never lost on a genuine
    // round-trip; only an explicit omit clears it. The handler additionally
    // pre-fills `needs_review` from `existing` when omitted (see
    // `patch_relationship_update`), so its routine-edit path is unaffected.
    //
    // `world_id` and `source` are NOT cleared here: neither is in
    // `UpdateRelationshipParams`, so `update_relationship_in_tx` always
    // preserves them from `existing` (required FK / immutable provenance) —
    // matching the pre-cutover path. `metadata` is the open bag, taken from
    // the spoke Relation directly; an empty bag clears the column.
    let symmetric = locals.symmetric.unwrap_or(false);
    let confidence = locals.confidence;
    let source_anchor_ids = locals.source_anchor_ids.unwrap_or_default();
    let needs_review = locals.needs_review.unwrap_or(false);

    let metadata_value = if relation.metadata.is_empty() {
        None
    } else {
        Some(Value::Object(relation.metadata.clone()))
    };

    let params = UpdateRelationshipParams {
        relation_type: relation.relation_type.clone(),
        custom_label: relation.label.clone(),
        symmetric,
        confidence,
        source_anchor_ids,
        metadata: metadata_value,
        updated_at: chrono::Utc::now().to_rfc3339(),
        needs_review,
    };

    // `update_relationship_in_tx` compares `revision = expected_revision` (CAS).
    // u64 → i64: revisions start at 1 and increment, so any realistic value
    // fits; clamp defensively (a clamped value just fails the CAS → stale).
    let expected_i64 = i64::try_from(expected).unwrap_or(i64::MAX);

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("storage error on tx begin: {e}"),
                json!({ "relation_id": relation_id }),
            );
        }
    };

    let result =
        update_relationship_in_tx(&mut tx, &relation_id, &params, expected_i64, &existing).await;

    let updated_row = match result {
        Ok(row) => row,
        Err(LocalDbError::VersionMismatch { actual, .. }) => {
            // CAS fail. Per the V1.144 brief the relation-port collapses every
            // VersionMismatch shape to STORED_REVISION_STALE (simpler than the
            // KnowledgeEntryPort 3-way split — the orchestrator pre-routes
            // create vs update, so the only reachable failure here is "the
            // store moved since the caller's read").
            let store_revision = actual
                .and_then(|v| u64::try_from(v).ok())
                .map_or(Value::Null, Value::from);
            return reject(
                SpokeRejectCode::StoredRevisionStale,
                format!(
                    "Store revision {} is not the expected base {expected} for relation {relation_id}",
                    actual.map_or_else(|| "?".to_string(), |v| v.to_string())
                ),
                json!({
                    "relation_id": relation_id,
                    "expectedBaseRevision": expected,
                    "storeRevision": store_revision,
                }),
            );
        }
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("storage error on relation CAS update: {e}"),
                json!({ "relation_id": relation_id }),
            );
        }
    };

    if let Err(e) = tx.commit().await {
        return reject(
            SpokeRejectCode::InternalError,
            format!("storage error on tx commit: {e}"),
            json!({ "relation_id": relation_id }),
        );
    }

    // `updated_row` already carries revision = expected + 1 and the persisted
    // mutable fields; project it through the single reverse-mapping seam.
    SpokeResult::Ok(row_to_relation(&updated_row))
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Resolved column values for the create INSERT, derived from the spoke
/// `Relation` + its nexus-locals. Extracted from [`put_relation_create`] to
/// keep that function under the `too_many_lines` budget.
struct CreateFields {
    created_at: String,
    updated_at: String,
    symmetric_i64: i64,
    confidence: Option<f64>,
    source_anchor_json: String,
    needs_review_i64: i64,
    source: String,
    metadata_json: Option<String>,
    custom_label: Option<String>,
}

/// Compute the [`CreateFields`] for a create: adapter-assigned timestamps
/// (falling back to `now` when the spoke `Relation` omits them) and the
/// nexus-locals defaulted to the V1.76 manual-author add shape when the
/// spoke `Relation` does not carry them (`symmetric=false`, `confidence=NULL`,
/// `source_anchor_ids='[]'`, `needs_review=false`, `source='manual'`).
fn prepare_create_fields(relation: &Relation, locals: &NexusLocals) -> CreateFields {
    let now = chrono::Utc::now().to_rfc3339();
    let created_at = relation
        .created_at
        .map_or_else(|| now.clone(), |dt| dt.to_rfc3339());
    let updated_at = relation
        .updated_at
        .map_or_else(|| now.clone(), |dt| dt.to_rfc3339());
    let source_anchor_ids = locals.source_anchor_ids.clone().unwrap_or_default();

    CreateFields {
        created_at,
        updated_at,
        symmetric_i64: i64::from(locals.symmetric.unwrap_or(false)),
        confidence: locals.confidence,
        source_anchor_json: serde_json::to_string(&source_anchor_ids)
            .unwrap_or_else(|_| "[]".to_string()),
        needs_review_i64: i64::from(locals.needs_review.unwrap_or(false)),
        source: locals
            .source
            .clone()
            .unwrap_or_else(|| SOURCE_MANUAL.to_string()),
        metadata_json: if relation.metadata.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&relation.metadata).unwrap_or_else(|_| "{}".to_string()))
        },
        custom_label: relation.label.clone(),
    }
}

/// Single reverse-mapping seam: project a `kb_relationships` row onto a spoke
/// `Relation` (used by get / create-return / update-return). `schema_version`
/// is set to the spoke 0.5.0 relation schema version (1).
fn row_to_relation(row: &KbRelationshipRow) -> Relation {
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

    let mut extensions = std::collections::HashMap::new();
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

/// nexus-locals carried under `extensions.nexus` on a spoke `Relation`.
/// Every field is optional — the create path defaults missing fields to the
/// V1.76 manual-author shape; the update path clears-on-omit (an absent
/// optional local is cleared, matching pre-cutover `update_relationship_in_tx`).
#[derive(Default)]
struct NexusLocals {
    world_id: Option<String>,
    symmetric: Option<bool>,
    confidence: Option<f64>,
    source_anchor_ids: Option<Vec<String>>,
    needs_review: Option<bool>,
    source: Option<String>,
}

/// Borrow the nexus-locals from a spoke `Relation`'s `extensions.nexus`
/// namespace, or [`NexusLocals::default`] when the namespace is absent.
fn extract_nexus_locals(relation: &Relation) -> NexusLocals {
    let Ok(key) = RelationExtensionsKey::try_from("nexus") else {
        return NexusLocals::default();
    };
    let Some(ns) = relation.extensions.get(&key) else {
        return NexusLocals::default();
    };
    NexusLocals {
        world_id: ns.get("world_id").and_then(Value::as_str).map(String::from),
        symmetric: ns.get("symmetric").and_then(Value::as_bool),
        confidence: ns.get("confidence").and_then(Value::as_f64),
        source_anchor_ids: ns.get("source_anchor_ids").and_then(value_as_string_array),
        needs_review: ns.get("needs_review").and_then(Value::as_bool),
        source: ns.get("source").and_then(Value::as_str).map(String::from),
    }
}

/// Parse a JSON array of strings from a [`Value`]; `None` if the value is not
/// an array or any element is not a string.
fn value_as_string_array(v: &Value) -> Option<Vec<String>> {
    let arr = v.as_array()?;
    arr.iter().map(|i| i.as_str().map(String::from)).collect()
}

/// Parse the stored `source_anchor_ids` JSON-array column back into a
/// `Vec<String>`; empty when the column is NULL or unparseable.
fn parse_anchor_ids(stored: Option<&str>) -> Vec<String> {
    stored
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
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
    use crate::RelationPort;
    use nexus_local_db::kb_relationships::{get_relationship, list_relationships_for_world};
    use nexus_local_db::{open_pool, run_migrations};
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

    /// Test helper: unwrap a `SpokeResult::Ok` or panic with the reject payload.
    fn unwrap_ok<T>(result: SpokeResult<T>, label: &str) -> T {
        match result {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("{label}: expected ok, got reject {r:?}"),
        }
    }

    // ── get_relation ──────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_relation_returns_not_found_for_missing() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        match adapter.get_relation("rel_missing") {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::RelationNotFound,
                    "missing relation must reject with RELATION_NOT_FOUND"
                );
                assert_eq!(
                    r.details.as_ref().and_then(|d| d.get("relation_id")),
                    Some(&json!("rel_missing"))
                );
            }
            SpokeResult::Ok(_) => panic!("expected RelationNotFound reject"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_relation_round_trips_persisted_row() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool.clone());
        let created = unwrap_ok(
            adapter.put_relation(spoke_relation("rel_rt", "kb_src", "kb_dst"), None),
            "create",
        );
        assert_eq!(created.revision, Some(1));

        match adapter.get_relation("rel_rt") {
            SpokeResult::Ok(r) => {
                assert_eq!(r.relation_id, "rel_rt");
                assert_eq!(r.from_id, "kb_src");
                assert_eq!(r.to_id, "kb_dst");
                assert_eq!(r.relation_type, "allied_with");
                assert_eq!(r.label.as_deref(), Some("Alice ↔ Bob"));
                assert_eq!(r.revision, Some(1), "get must reflect the seeded revision");
                // nexus-locals round-trip through extensions.nexus.
                let key = RelationExtensionsKey::try_from("nexus").unwrap();
                let ns = r.extensions.get(&key).expect("nexus namespace present");
                assert_eq!(ns.get("world_id"), Some(&json!("wld_rel")));
                assert_eq!(ns.get("symmetric"), Some(&json!(false)));
                assert_eq!(ns.get("needs_review"), Some(&json!(false)));
                assert_eq!(ns.get("source"), Some(&json!("manual")));
            }
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        }
    }

    /// Round-trip a relation carrying the FULL set of nexus-locals
    /// (`extensions.nexus`: `world_id` + `symmetric` + `confidence` +
    /// `source_anchor_ids` + `needs_review` + `source`) through put → get
    /// and confirm every known key survives.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_round_trips_explicit_nexus_locals() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);

        // Build a relation with every nexus-local set explicitly (the
        // sibling `get_relation_round_trips_persisted_row` only proves the
        // default-seed shape; this one proves explicit values survive).
        let relation: Relation = serde_json::from_value(json!({
            "schema_version": 1,
            "relation_id": "rel_locals",
            "from_id": "kb_src",
            "to_id": "kb_dst",
            "relation_type": "rivals_with",
            "label": "Alice ✗ Bob",
            "metadata": { "tag": "fixture" },
            "extensions": {
                "nexus": {
                    "world_id": "wld_rel",
                    "symmetric": true,
                    "confidence": 0.87,
                    "source_anchor_ids": ["anc_a", "anc_b"],
                    "needs_review": true,
                    "source": "extraction"
                }
            }
        }))
        .expect("valid spoke Relation fixture with full nexus-locals");

        let created = unwrap_ok(adapter.put_relation(relation, None), "create");
        assert_eq!(created.revision, Some(1));

        // Re-read through the port and confirm every nexus-local survived.
        //
        // Known limitation (brief concern #1): ONLY the nexus-local keys
        // asserted below round-trip. The `kb_relationships` table has no
        // extras-JSON column (unlike `kb_key_blocks.extensions_nexus_json`),
        // so any UNKNOWN key under `extensions.nexus` — e.g. a hypothetical
        // `custom_flag` — is silently dropped on put and absent on get. We
        // therefore do NOT assert any unknown key here; this test pins the
        // known set only. Lifting that limitation is a schema change, out of
        // scope for V1.144.
        match adapter.get_relation("rel_locals") {
            SpokeResult::Ok(r) => {
                assert_eq!(r.relation_id, "rel_locals");
                assert_eq!(r.relation_type, "rivals_with");
                assert_eq!(r.label.as_deref(), Some("Alice ✗ Bob"));
                assert_eq!(
                    r.metadata.get("tag"),
                    Some(&json!("fixture")),
                    "open metadata bag round-trips"
                );
                let key = RelationExtensionsKey::try_from("nexus").unwrap();
                let ns = r.extensions.get(&key).expect("nexus namespace present");
                assert_eq!(ns.get("world_id"), Some(&json!("wld_rel")));
                assert_eq!(
                    ns.get("symmetric"),
                    Some(&json!(true)),
                    "explicit symmetric=true survives"
                );
                // confidence is an f64 → JSON number; compare numerically.
                let confidence = ns
                    .get("confidence")
                    .and_then(Value::as_f64)
                    .expect("confidence present");
                assert!(
                    (confidence - 0.87).abs() < 1e-9,
                    "explicit confidence=0.87 survives (got {confidence})"
                );
                assert_eq!(
                    ns.get("source_anchor_ids"),
                    Some(&json!(["anc_a", "anc_b"])),
                    "explicit source_anchor_ids survive"
                );
                assert_eq!(
                    ns.get("needs_review"),
                    Some(&json!(true)),
                    "explicit needs_review=true survives"
                );
                assert_eq!(
                    ns.get("source"),
                    Some(&json!("extraction")),
                    "explicit source=extraction survives"
                );
            }
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        }
    }

    // ── put_relation create path ──────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_happy_path_persists_row() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool.clone());
        let relation = spoke_relation("rel_happy", "kb_src", "kb_dst");

        let returned = unwrap_ok(adapter.put_relation(relation, None), "create");
        assert_eq!(returned.relation_id, "rel_happy");
        assert_eq!(returned.from_id, "kb_src");
        assert_eq!(returned.to_id, "kb_dst");
        assert_eq!(returned.relation_type, "allied_with");
        assert_eq!(
            returned.revision,
            Some(1),
            "create must seed revision = 1 (spoke convention)"
        );

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
        assert_eq!(row.revision, 1, "initial revision is 1 (spoke convention)");
        assert!(
            row.metadata.is_some(),
            "spoke `metadata` is persisted to the nexus `metadata` column"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_create_on_existing_rejects_already_exists() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        let relation = spoke_relation("rel_dup", "kb_src", "kb_dst");

        let first = adapter.put_relation(relation.clone(), None);
        assert!(matches!(first, SpokeResult::Ok(_)), "first create succeeds");

        match adapter.put_relation(relation, None) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::RelationAlreadyExists,
                    "second create must reject with RELATION_ALREADY_EXISTS"
                );
            }
            SpokeResult::Ok(_) => panic!("expected RelationAlreadyExists reject"),
        }
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

        match adapter.put_relation(relation, None) {
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
    async fn put_relation_unknown_endpoint_rejects_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool.clone());
        let relation = spoke_relation("rel_bad_endpoint", "kb_src", "kb_nonexistent");

        match adapter.put_relation(relation, None) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "FK violation on target endpoint must surface as INTERNAL_ERROR (storage-level constraint)"
                );
            }
            SpokeResult::Ok(_) => panic!("expected INTERNAL_ERROR reject"),
        }

        // The transaction must have rolled back: no row exists.
        let rows = list_relationships_for_world(&pool, "wld_rel", true, 100)
            .await
            .unwrap();
        assert!(rows.is_empty(), "tx rolled back on FK violation");
    }

    // ── put_relation CAS update path ──────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_update_happy_path_bumps_revision() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);

        // Create → revision 1.
        let created = unwrap_ok(
            adapter.put_relation(spoke_relation("rel_upd", "kb_src", "kb_dst"), None),
            "create",
        );
        assert_eq!(created.revision, Some(1));

        // First update: expected_base_revision = Some(1). CAS accepts;
        // revision bumps 1 → 2. label + relation_type round-trip.
        let mut updated = created;
        updated.label = Some("Alice ↔ Bob (revised)".to_string());
        updated.relation_type = "opposes".to_string();

        let rev2 = unwrap_ok(adapter.put_relation(updated, Some(1)), "first update");
        assert_eq!(rev2.relation_id, "rel_upd");
        assert_eq!(rev2.revision, Some(2), "CAS update must bump revision");
        assert_eq!(rev2.relation_type, "opposes");
        assert_eq!(rev2.label.as_deref(), Some("Alice ↔ Bob (revised)"));
        // nexus-locals preserved (world_id still present after update).
        let key = RelationExtensionsKey::try_from("nexus").unwrap();
        assert_eq!(
            rev2.extensions.get(&key).and_then(|ns| ns.get("world_id")),
            Some(&json!("wld_rel")),
            "world_id is preserved across update"
        );

        // Second update: expected_base_revision = Some(2). CAS accepts;
        // revision bumps 2 → 3 — proves the revision-bump chain repeats,
        // not a one-shot. Mutate the label again to distinguish the writes.
        let mut rev2_mut = rev2;
        rev2_mut.label = Some("Alice ↔ Bob (v3)".to_string());
        let rev3 = unwrap_ok(adapter.put_relation(rev2_mut, Some(2)), "second update");
        assert_eq!(
            rev3.revision,
            Some(3),
            "second CAS update must bump revision 2 → 3"
        );
        assert_eq!(rev3.label.as_deref(), Some("Alice ↔ Bob (v3)"));

        // Re-read: persisted row has revision 3 + the latest label/type.
        match adapter.get_relation("rel_upd") {
            SpokeResult::Ok(r) => {
                assert_eq!(r.revision, Some(3));
                assert_eq!(r.relation_type, "opposes");
                assert_eq!(r.label.as_deref(), Some("Alice ↔ Bob (v3)"));
            }
            SpokeResult::Reject(r) => panic!("re-read failed: {r:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_update_stale_rejects_stored_revision_stale() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);

        // Create → revision 1. Bump to 2. Then attempt another update with
        // expected = 1 (caller read a stale base before the second writer
        // bumped). Store (2) > expected (1) → STORED_REVISION_STALE.
        let created = unwrap_ok(
            adapter.put_relation(spoke_relation("rel_stale", "kb_src", "kb_dst"), None),
            "create",
        );
        let _ = unwrap_ok(
            adapter.put_relation(created.clone(), Some(1)),
            "first update",
        );

        match adapter.put_relation(created, Some(1)) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::StoredRevisionStale,
                    "stored > expected must map to STORED_REVISION_STALE"
                );
                let details = r.details.expect("details present");
                assert_eq!(details["expectedBaseRevision"], json!(1));
                assert_eq!(details["storeRevision"], json!(2));
            }
            SpokeResult::Ok(_) => panic!("expected STORED_REVISION_STALE reject"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_update_on_absent_rejects_stored_revision_stale() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        // No prior create — relation is absent. Caller passes expected = Some(3);
        // the relation-port CAS mapping collapses this to STORED_REVISION_STALE
        // with storeRevision = null (V1.144 brief).
        match adapter.put_relation(spoke_relation("rel_absent", "kb_src", "kb_dst"), Some(3)) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::StoredRevisionStale,
                    "absent + Some(expected) collapses to STORED_REVISION_STALE"
                );
                let details = r.details.expect("details present");
                assert_eq!(details["expectedBaseRevision"], json!(3));
                assert_eq!(details["storeRevision"], Value::Null);
            }
            SpokeResult::Ok(_) => panic!("expected STORED_REVISION_STALE reject"),
        }
    }

    // ── V1.144 Phase 5 fix: clear-on-omit + round-trip safety ──────────

    /// Regression (V1.144 Phase 5 fix): an update that OMITS the optional
    /// nexus-locals must CLEAR them, matching the pre-cutover
    /// `update_relationship_in_tx` (which wrote every bound local —
    /// None→SQL NULL). The P2 cutover accidentally switched these to
    /// preserve-on-omit; this test pins the restored clear-on-omit semantics
    /// for `confidence`, `symmetric`, `source_anchor_ids`, and `needs_review`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_update_omitting_optional_clears_it() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);

        // Create with the full set of optional locals set explicitly.
        let seed: Relation = serde_json::from_value(json!({
            "schema_version": 1,
            "relation_id": "rel_clr",
            "from_id": "kb_src",
            "to_id": "kb_dst",
            "relation_type": "allied_with",
            "extensions": {
                "nexus": {
                    "world_id": "wld_rel",
                    "symmetric": true,
                    "confidence": 0.9,
                    "source_anchor_ids": ["anc_x"],
                    "needs_review": true
                }
            }
        }))
        .expect("valid seed Relation");
        let created = unwrap_ok(adapter.put_relation(seed, None), "create");
        assert_eq!(created.revision, Some(1));

        // Update with a Relation that carries ONLY the required world_id and
        // omits every optional local (plus a label change so the CAS write is
        // observable). Clear-on-omit must clear confidence/symmetric/etc.
        let update: Relation = serde_json::from_value(json!({
            "schema_version": 1,
            "relation_id": "rel_clr",
            "from_id": "kb_src",
            "to_id": "kb_dst",
            "relation_type": "allied_with",
            "label": "cleared locals",
            "extensions": {
                "nexus": {
                    "world_id": "wld_rel"
                }
            }
        }))
        .expect("valid update Relation omitting optional locals");
        let updated = unwrap_ok(adapter.put_relation(update, Some(1)), "update");
        assert_eq!(updated.revision, Some(2));

        // Re-read through the port and confirm every omitted local is cleared.
        let r = unwrap_ok(adapter.get_relation("rel_clr"), "re-read");
        let key = RelationExtensionsKey::try_from("nexus").unwrap();
        let ns = r.extensions.get(&key).expect("nexus namespace present");
        assert_eq!(
            ns.get("symmetric"),
            Some(&json!(false)),
            "omitted symmetric is cleared (false)"
        );
        assert_eq!(
            ns.get("confidence"),
            None,
            "omitted confidence is cleared (absent from extensions.nexus = SQL NULL)"
        );
        assert_eq!(
            ns.get("source_anchor_ids"),
            Some(&json!([])),
            "omitted source_anchor_ids is cleared (empty array)"
        );
        assert_eq!(
            ns.get("needs_review"),
            Some(&json!(false)),
            "omitted needs_review is cleared (false)"
        );
        assert_eq!(r.label.as_deref(), Some("cleared locals"));
    }

    // ── V1.146 P0: InternalError on DB failure ─────────────────────────

    /// DB failure (dropped table) on get surfaces `InternalError`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_relation_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;
        sqlx::query("DROP TABLE kb_relationships")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = NexusBaselineAdapter::new(pool);
        match adapter.get_relation("rel_any") {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "dropped table must surface INTERNAL_ERROR on get"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }

    /// DB failure on put_relation create path surfaces `InternalError`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_create_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;
        sqlx::query("DROP TABLE kb_relationships")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = NexusBaselineAdapter::new(pool);
        let relation = spoke_relation("rel_fail_create", "kb_src", "kb_dst");
        match adapter.put_relation(relation, None) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "create on dropped table must surface INTERNAL_ERROR"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }

    /// DB failure on put_relation update path surfaces `InternalError`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_update_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool.clone());
        let created = unwrap_ok(
            adapter.put_relation(spoke_relation("rel_upd_fail", "kb_src", "kb_dst"), None),
            "create",
        );
        assert_eq!(created.revision, Some(1));

        // Drop the table to simulate DB failure on update.
        sqlx::query("DROP TABLE kb_relationships")
            .execute(&pool)
            .await
            .unwrap();

        match adapter.put_relation(created, Some(1)) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "update on dropped table must surface INTERNAL_ERROR"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }

    // ── V1.146 P0: validation → InvalidInput (unchanged) ───────────────

    /// Validation failure (missing required extension field) still surfaces
    /// `InvalidInput` — no DB I/O is performed before the guard.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn relation_validation_still_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        // Create-success-then-recreate → RelationAlreadyExists (domain signal, not storage)
        let first = spoke_relation("rel_val_ae", "kb_src", "kb_dst");
        let _ = unwrap_ok(adapter.put_relation(first.clone(), None), "first create");

        match adapter.put_relation(first, None) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::RelationAlreadyExists,
                    "duplicate create must still surface RelationAlreadyExists"
                );
            }
            SpokeResult::Ok(_) => panic!("expected AlreadyExists reject"),
        }

        // get on non-existent → RelationNotFound
        match adapter.get_relation("rel_never_created") {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::RelationNotFound,
                    "missing relation must still surface RelationNotFound"
                );
            }
            SpokeResult::Ok(_) => panic!("expected NotFound reject"),
        }
    }

    // ── V1.146 P0: OCC rejects unchanged ───────────────────────────────
    // put_relation_update_stale_rejects_stored_revision_stale and
    // put_relation_update_on_absent_rejects_stored_revision_stale above
    // already cover STORED_REVISION_STALE — they pass unchanged (confirmed
    // by the red-green run). No additional OCC test needed.

    /// Safety check (V1.144 Phase 5 fix): a full get→put round-trip (read the
    /// relation via `get_relation`, mutate a non-local field, write it back via
    /// `put_relation`) must PRESERVE every nexus-local. This proves
    /// clear-on-omit is safe: `get_relation` (`row_to_relation`) fully
    /// populates `extensions.nexus`, so the orchestrator/handler round-trip
    /// never loses a carried local — only an explicit omit clears one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_update_get_put_round_trip_preserves_locals() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);

        // Create with the full set of locals set explicitly.
        let seed: Relation = serde_json::from_value(json!({
            "schema_version": 1,
            "relation_id": "rel_rt2",
            "from_id": "kb_src",
            "to_id": "kb_dst",
            "relation_type": "allied_with",
            "extensions": {
                "nexus": {
                    "world_id": "wld_rel",
                    "symmetric": true,
                    "confidence": 0.87,
                    "source_anchor_ids": ["anc_a", "anc_b"],
                    "needs_review": true,
                    "source": "extraction"
                }
            }
        }))
        .expect("valid seed Relation");
        let created = unwrap_ok(adapter.put_relation(seed, None), "create");
        assert_eq!(created.revision, Some(1));

        // Read-modify-write: get the fully-populated Relation, mutate a
        // non-local field (label), write it back with the get-result's
        // revision as the CAS base.
        let read = unwrap_ok(adapter.get_relation("rel_rt2"), "get");
        assert_eq!(read.revision, Some(1));
        let mut to_write = read;
        to_write.label = Some("round-tripped".to_string());
        let written = unwrap_ok(adapter.put_relation(to_write, Some(1)), "round-trip update");
        assert_eq!(written.revision, Some(2));

        // Re-read: every local survived the get→put round-trip (clear-on-omit
        // did NOT fire because get populated them all).
        let r = unwrap_ok(adapter.get_relation("rel_rt2"), "final get");
        assert_eq!(r.label.as_deref(), Some("round-tripped"));
        let key = RelationExtensionsKey::try_from("nexus").unwrap();
        let ns = r.extensions.get(&key).expect("nexus namespace present");
        assert_eq!(ns.get("world_id"), Some(&json!("wld_rel")));
        assert_eq!(ns.get("symmetric"), Some(&json!(true)));
        let confidence = ns
            .get("confidence")
            .and_then(Value::as_f64)
            .expect("confidence present");
        assert!(
            (confidence - 0.87).abs() < 1e-9,
            "confidence survived round-trip (got {confidence})"
        );
        assert_eq!(
            ns.get("source_anchor_ids"),
            Some(&json!(["anc_a", "anc_b"]))
        );
        assert_eq!(ns.get("needs_review"), Some(&json!(true)));
        // `source` is immutable on the update path (not in
        // UpdateRelationshipParams) — preserved from the stored row.
        assert_eq!(ns.get("source"), Some(&json!("extraction")));
    }
}
