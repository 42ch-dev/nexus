//! Production `RelationPort` impl — OCC-aware routing of `kb_relationships`
//! storage through spoke's port surface (spec §7.4).
//!
//! # Wire ↔ row mapping (spoke 0.5.0)
//!
//! There is no second conversion seam for `Relation` analogous to the
//! V1.139 `WorldKbEntry ↔ KnowledgeEntry` pair — spoke's `Relation`
//! wire type maps directly onto the nexus `kb_relationships` row at
//! this boundary via the single reverse-mapping seam
//! [`crate::conversion::kb_relationship_row_to_spoke`] (moved here in
//! V1.146 P3 so the CLI pack exporter and the port share one mapping):
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
//!
//! V1.146 P5 T2: the `extensions_nexus_json` column (added by T1) now
//! preserves unknown `extensions.nexus` keys across the `SQLite` round-trip.
//! On write, the full `extensions["nexus"]` namespace is serialized into the
//! column; on read, [`kb_relationship_row_to_spoke`] merges it back with the
//! 6 typed columns as authoritative.
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
//!
//! V1.154 P2 (R3 closure): a [`LocalDbError::WorldConflict`] (the stored row
//! moved to another world between the caller's verified read and the CAS) is
//! NOT collapsed — it rides the `InternalError` carrier with a
//! `world_conflict: true` details marker so hosts surface the fixed
//! `world_conflict` wire code (spec §3.2).
//!
//! # `RelationPort` read gap + hop-edge loader (V1.149 P1, spec §5; iteration
//! spec: `.mstar/iterations/v1.149/specs/fl-l-w4-activation.md` §5)
//!
//! spoke 0.8.2 `RelationPort` is **get/put only** — there is no
//! list-by-entity on the trait. Relation-hop expansion (lore activation,
//! `adapter::activation`) therefore cannot read the graph through the port:
//! [`NexusAdapter::list_hop_edges_for_world`] below is an **inherent** adapter
//! helper that reads the storage list primitive
//! `list_relationships_for_world` (confirmed graph only) and maps rows to
//! engine-local [`HopEdge`]s (`super::activation::HopEdge` — not a spoke wire
//! type). No hop/matcher logic lives in `spoke-operations`.

use super::activation::HopEdge;
use super::NexusAdapter;
use crate::{
    Relation, RelationExtensionsKey, RelationPort, SpokeReject, SpokeRejectCode, SpokeResult,
};
use async_trait::async_trait;
use nexus_local_db::kb_relationships::{
    get_relationship, list_relationships_for_world, update_relationship_in_tx, KbRelationshipRow,
    UpdateRelationshipParams, SOURCE_MANUAL,
};
use nexus_local_db::LocalDbError;
use serde_json::{json, Map, Value};

#[async_trait]
impl RelationPort for NexusAdapter<'_> {
    async fn get_relation(&self, relation_id: &str) -> SpokeResult<Relation> {
        let pool = self.pool.clone();
        let relation_id = relation_id.to_string();
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
        SpokeResult::Ok(crate::conversion::kb_relationship_row_to_spoke(&row))
    }

    async fn put_relation(
        &self,
        relation: Relation,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<Relation> {
        let pool = self.pool.clone();
        match expected_base_revision {
            None => put_relation_create(&pool, relation).await,
            Some(expected) => put_relation_update(&pool, relation, expected).await,
        }
    }
}

/// Upper bound for one hop-edge load — confirmed relation rows per world
/// (V1.149 P1, spec §5).
///
/// Generous but finite: a world whose confirmed graph exceeds this limit is
/// **truncated** to the `HOP_EDGE_LIST_LIMIT` newest rows (the `cap + 1`
/// probe row is dropped). Truncation is silent — no panic, no error — the
/// engine simply sees fewer edges. Worlds that routinely exceed the limit
/// are tracked in the V1.149 P1 plan residual (paginated / neighbor-indexed
/// read is the follow-up).
pub const HOP_EDGE_LIST_LIMIT: i64 = 10_000;

impl NexusAdapter<'_> {
    /// Load the confirmed relation edges of a world for relation-hop
    /// expansion (V1.149 P1, spec §5).
    ///
    /// # `RelationPort` gap
    ///
    /// spoke 0.8.2 [`RelationPort`] is get/put only — no list-by-entity — so
    /// hop expansion cannot use the port. This **inherent** helper (not a
    /// trait method) reads the storage list primitive
    /// [`list_relationships_for_world`] directly with
    /// `include_suggested = false` (the confirmed graph; extraction
    /// suggestions stay out of lore hops) and maps rows to engine-local
    /// [`HopEdge`]s (`adapter::activation::HopEdge` — not a spoke wire type).
    /// Matching/hop logic itself lives in the pure activation engine, never
    /// in `spoke-operations`.
    ///
    /// # Truncation
    ///
    /// At most [`HOP_EDGE_LIST_LIMIT`] edges are returned; a larger graph is
    /// silently truncated to the newest rows (no panic, no error — see the
    /// constant docs and the V1.149 P1 plan residual).
    ///
    /// # Errors
    ///
    /// Returns [`LocalDbError`] on database failure.
    pub async fn list_hop_edges_for_world(
        &self,
        world_id: &str,
    ) -> Result<Vec<HopEdge>, LocalDbError> {
        let pool = self.pool.clone();
        let world_id = world_id.to_string();
        // `cap + 1` probe per the `list_relationships_for_world` caller
        // convention: a result of `cap + 1` rows signals overflow; the
        // probe row is dropped below (truncate, no panic). The const
        // (10_000) always fits `usize`; the fallback is defensive only.
        let limit = usize::try_from(HOP_EDGE_LIST_LIMIT).unwrap_or(usize::MAX);
        let rows =
            list_relationships_for_world(&pool, &world_id, false, HOP_EDGE_LIST_LIMIT + 1).await?;
        let edges = rows
            .into_iter()
            .take(limit)
            .map(|row| HopEdge {
                relation_id: row.relationship_id,
                from_id: row.source_entity_id,
                to_id: row.target_entity_id,
                relation_type: row.relation_type,
            })
            .collect();
        Ok(edges)
    }
}

// ── put_relation: create path ─────────────────────────────────────────

/// Create path: `expected_base_revision = None`. Reject if the row already
/// exists; otherwise INSERT with `revision = 1` (spoke convention) and return
/// the resulting spoke `Relation`.
#[allow(clippy::too_many_lines)]
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

    // V1.146 P5 T2: serialize the full extensions.nexus namespace before
    // prepare_create_fields consumes `relation` (the locals extraction borrows
    // relation, so we compute the JSON string before prepare_create_fields
    // which also borrows relation).
    let extensions_nexus_json = serialize_extensions_nexus_json(&relation);

    // Compute the create column values before moving `locals.world_id` below.
    let f = prepare_create_fields(&relation, &locals, extensions_nexus_json);

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

    let extensions_nexus_ref = f.extensions_nexus_json.as_deref();

    // Seed revision = 1 directly (spoke convention). The legacy
    // `insert_relationship_in_tx` seeds 0 for the daemon's add-relationship
    // route and is deliberately NOT reused here — the port owns the spoke
    // revision-seed so the legacy fn + daemon route stay untouched (V1.144).
    let insert_result = sqlx::query!(
        r#"INSERT INTO kb_relationships
           (relationship_id, world_id, source_entity_id, target_entity_id,
            relation_type, custom_label, symmetric, confidence,
            source_anchor_ids, metadata, created_at, updated_at, revision,
            needs_review, source, extensions_nexus_json)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)"#,
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
        extensions_nexus_ref,
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
        extensions_nexus_json: f.extensions_nexus_json,
    };
    SpokeResult::Ok(crate::conversion::kb_relationship_row_to_spoke(&row))
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

    // V1.154 P2 (R3 closure, spec §3.1): the world bind is the stored-world
    // expected by the request — the relation's claimed
    // `extensions.nexus.world_id`, which the invoke gate verified against the
    // stored row. It is NOT taken from `existing` (a fresh pre-read would
    // mask the cross-process race this predicate closes). The orchestrator
    // round-trip always carries it (`get_relation` fully populates
    // `extensions.nexus`); fail closed when a direct port caller omits it.
    let Some(claimed_world) = locals.world_id.as_deref() else {
        return reject(
            SpokeRejectCode::InvalidInput,
            format!(
                "Relation is missing required extensions.nexus.world_id for update: {relation_id}"
            ),
            json!({
                "relation_id": relation_id,
                "missing": ["extensions.nexus.world_id"],
            }),
        );
    };

    // nexus-locals on update follow clear-on-omit semantics: an optional local
    // the spoke `Relation` does NOT carry is cleared (symmetric→0,
    // confidence→SQL NULL, source_anchor_ids→'[]', needs_review→0). This is
    // behavior-equivalent to the pre-cutover `update_relationship_in_tx`,
    // which wrote every bound local directly (the V1.144 P2 cutover
    // accidentally switched these to preserve-on-omit, violating AC-I3).
    //
    // The orchestrator/handler round-trip stays safe because `get_relation`
    // (`kb_relationship_row_to_spoke`) FULLY populates `extensions.nexus` before any
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

    // V1.146 P5 T2: serialize the full extensions.nexus namespace for
    // round-trip preservation. Unknown keys survive the update cycle.
    let extensions_nexus_json = serialize_extensions_nexus_json(&relation);

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
        extensions_nexus_json,
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

    let result = update_relationship_in_tx(
        &mut tx,
        &relation_id,
        &params,
        expected_i64,
        &existing,
        claimed_world,
    )
    .await;

    // Map the CAS outcome (rejects before the commit below; success
    // projects the updated row AFTER the commit).
    let updated_row = match map_relation_cas_result(result, &relation_id, expected) {
        SpokeResult::Ok(row) => row,
        SpokeResult::Reject(r) => return SpokeResult::Reject(r),
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
    SpokeResult::Ok(crate::conversion::kb_relationship_row_to_spoke(
        &updated_row,
    ))
}

/// Map the CAS update outcome for [`put_relation_update`], keeping that
/// function under the `too_many_lines` budget (same extraction pattern as
/// [`CreateFields`] for the create path):
///
/// - `VersionMismatch` → `STORED_REVISION_STALE` (the relation-port
///   collapses every mismatch shape to stale; the orchestrator pre-routes
///   create vs update, so the only reachable failure is "the store moved
///   since the caller's read").
/// - `WorldConflict` (V1.154 P2 R3, spec §3.2) → the `world_conflict`-
///   marked `InternalError` reject — a zero-row CAS caused by a world
///   mismatch must never collapse into a generic OCC failure (the caller's
///   revision was valid; the WORLD moved). Same carrier + marker as the
///   `KnowledgeEntryPort` CAS mapping; hosts remap to the fixed///   `world_conflict` wire code via
///   [`is_world_conflict_reject`](crate::is_world_conflict_reject).
/// - Any other storage error → `InternalError`.
fn map_relation_cas_result(
    result: Result<KbRelationshipRow, LocalDbError>,
    relation_id: &str,
    expected: u64,
) -> SpokeResult<KbRelationshipRow> {
    let updated_row = match result {
        Ok(row) => row,
        Err(LocalDbError::VersionMismatch { actual, .. }) => {
            // CAS fail. Per the V1.144 brief the relation-port collapses
            // every VersionMismatch shape to STORED_REVISION_STALE (simpler
            // than the KnowledgeEntryPort 3-way split — the orchestrator
            // pre-routes create vs update, so the only reachable failure
            // here is "the store moved since the caller's read").
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
        Err(LocalDbError::WorldConflict {
            table,
            id,
            expected_world,
            actual_world,
        }) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!(
                    "Relation {id} now lives in world {actual_world}, \
                     not the expected world {expected_world} (row moved between \
                     verification and CAS)"
                ),
                json!({
                    "world_conflict": true,
                    "table": table,
                    "id": id,
                    "expectedWorld": expected_world,
                    "actualWorld": actual_world,
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
    SpokeResult::Ok(updated_row)
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
    /// V1.146 P5 T2: serialized `extensions.nexus` for round-trip preservation.
    extensions_nexus_json: Option<String>,
}

/// Compute the [`CreateFields`] for a create: adapter-assigned timestamps
/// (falling back to `now` when the spoke `Relation` omits them) and the
/// nexus-locals defaulted to the V1.76 manual-author add shape when the
/// spoke `Relation` does not carry them (`symmetric=false`, `confidence=NULL`,
/// `source_anchor_ids='[]'`, `needs_review=false`, `source='manual'`).
///
/// `extensions_nexus_json` is the full serialized `extensions.nexus` namespace
/// (V1.146 P5 T2), pre-computed by the caller so `prepare_create_fields` stays
/// pure.
fn prepare_create_fields(
    relation: &Relation,
    locals: &NexusLocals,
    extensions_nexus_json: Option<String>,
) -> CreateFields {
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
        extensions_nexus_json,
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

/// Serialize the full `extensions.nexus` namespace into the
/// `extensions_nexus_json` column value (`None` when absent or empty).
///
/// V1.146 P5 T2: the full namespace (known + unknown keys) is serialized so
/// unknown keys survive the `SQLite` round-trip. On read,
/// [`crate::conversion::kb_relationship_row_to_spoke`] merges the JSON back,
/// with the 6 typed columns as authoritative.
fn serialize_extensions_nexus_json(relation: &Relation) -> Option<String> {
    let Ok(key) = RelationExtensionsKey::try_from("nexus") else {
        return None;
    };
    let ns = relation.extensions.get(&key)?;
    if ns.is_empty() {
        return None;
    }
    serde_json::to_string(ns).ok()
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

        let adapter = NexusAdapter::new(pool);
        match adapter.get_relation("rel_missing").await {
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

        let adapter = NexusAdapter::new(pool.clone());
        let created = unwrap_ok(
            adapter
                .put_relation(spoke_relation("rel_rt", "kb_src", "kb_dst"), None)
                .await,
            "create",
        );
        assert_eq!(created.revision, Some(1));

        match adapter.get_relation("rel_rt").await {
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

        let adapter = NexusAdapter::new(pool);

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

        let created = unwrap_ok(adapter.put_relation(relation, None).await, "create");
        assert_eq!(created.revision, Some(1));

        // Re-read through the port and confirm every nexus-local survived.
        //
        // V1.146 P5 T2: the `extensions_nexus_json` column now preserves
        // unknown `extensions.nexus` keys across the SQLite round-trip.
        // Known keys are verified below; unknown-key round-trip is covered
        // by the dedicated `put_relation_round_trips_unknown_nexus_key` test.
        match adapter.get_relation("rel_locals").await {
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

        let adapter = NexusAdapter::new(pool.clone());
        let relation = spoke_relation("rel_happy", "kb_src", "kb_dst");

        let returned = unwrap_ok(adapter.put_relation(relation, None).await, "create");
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

        let adapter = NexusAdapter::new(pool);
        let relation = spoke_relation("rel_dup", "kb_src", "kb_dst");

        let first = adapter.put_relation(relation.clone(), None).await;
        assert!(matches!(first, SpokeResult::Ok(_)), "first create succeeds");

        match adapter.put_relation(relation, None).await {
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

        let adapter = NexusAdapter::new(pool);
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

        match adapter.put_relation(relation, None).await {
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

        let adapter = NexusAdapter::new(pool.clone());
        let relation = spoke_relation("rel_bad_endpoint", "kb_src", "kb_nonexistent");

        match adapter.put_relation(relation, None).await {
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

        let adapter = NexusAdapter::new(pool);

        // Create → revision 1.
        let created = unwrap_ok(
            adapter
                .put_relation(spoke_relation("rel_upd", "kb_src", "kb_dst"), None)
                .await,
            "create",
        );
        assert_eq!(created.revision, Some(1));

        // First update: expected_base_revision = Some(1). CAS accepts;
        // revision bumps 1 → 2. label + relation_type round-trip.
        let mut updated = created;
        updated.label = Some("Alice ↔ Bob (revised)".to_string());
        updated.relation_type = "opposes".to_string();

        let rev2 = unwrap_ok(adapter.put_relation(updated, Some(1)).await, "first update");
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
        let rev3 = unwrap_ok(
            adapter.put_relation(rev2_mut, Some(2)).await,
            "second update",
        );
        assert_eq!(
            rev3.revision,
            Some(3),
            "second CAS update must bump revision 2 → 3"
        );
        assert_eq!(rev3.label.as_deref(), Some("Alice ↔ Bob (v3)"));

        // Re-read: persisted row has revision 3 + the latest label/type.
        match adapter.get_relation("rel_upd").await {
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

        let adapter = NexusAdapter::new(pool);

        // Create → revision 1. Bump to 2. Then attempt another update with
        // expected = 1 (caller read a stale base before the second writer
        // bumped). Store (2) > expected (1) → STORED_REVISION_STALE.
        let created = unwrap_ok(
            adapter
                .put_relation(spoke_relation("rel_stale", "kb_src", "kb_dst"), None)
                .await,
            "create",
        );
        let _ = unwrap_ok(
            adapter.put_relation(created.clone(), Some(1)).await,
            "first update",
        );

        match adapter.put_relation(created, Some(1)).await {
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

        let adapter = NexusAdapter::new(pool);
        // No prior create — relation is absent. Caller passes expected = Some(3);
        // the relation-port CAS mapping collapses this to STORED_REVISION_STALE
        // with storeRevision = null (V1.144 brief).
        match adapter
            .put_relation(spoke_relation("rel_absent", "kb_src", "kb_dst"), Some(3))
            .await
        {
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

        let adapter = NexusAdapter::new(pool);

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
        let created = unwrap_ok(adapter.put_relation(seed, None).await, "create");
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
        let updated = unwrap_ok(adapter.put_relation(update, Some(1)).await, "update");
        assert_eq!(updated.revision, Some(2));

        // Re-read through the port and confirm every omitted local is cleared.
        let r = unwrap_ok(adapter.get_relation("rel_clr").await, "re-read");
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

        let adapter = NexusAdapter::new(pool);
        match adapter.get_relation("rel_any").await {
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

        let adapter = NexusAdapter::new(pool);
        let relation = spoke_relation("rel_fail_create", "kb_src", "kb_dst");
        match adapter.put_relation(relation, None).await {
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

        let adapter = NexusAdapter::new(pool.clone());
        let created = unwrap_ok(
            adapter
                .put_relation(spoke_relation("rel_upd_fail", "kb_src", "kb_dst"), None)
                .await,
            "create",
        );
        assert_eq!(created.revision, Some(1));

        // Drop the table to simulate DB failure on update.
        sqlx::query("DROP TABLE kb_relationships")
            .execute(&pool)
            .await
            .unwrap();

        match adapter.put_relation(created, Some(1)).await {
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

        let adapter = NexusAdapter::new(pool);
        // Create-success-then-recreate → RelationAlreadyExists (domain signal, not storage)
        let first = spoke_relation("rel_val_ae", "kb_src", "kb_dst");
        let _ = unwrap_ok(
            adapter.put_relation(first.clone(), None).await,
            "first create",
        );

        match adapter.put_relation(first, None).await {
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
        match adapter.get_relation("rel_never_created").await {
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
    /// clear-on-omit is safe: `get_relation` (`kb_relationship_row_to_spoke`) fully
    /// populates `extensions.nexus`, so the orchestrator/handler round-trip
    /// never loses a carried local — only an explicit omit clears one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_update_get_put_round_trip_preserves_locals() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusAdapter::new(pool);

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
        let created = unwrap_ok(adapter.put_relation(seed, None).await, "create");
        assert_eq!(created.revision, Some(1));

        // Read-modify-write: get the fully-populated Relation, mutate a
        // non-local field (label), write it back with the get-result's
        // revision as the CAS base.
        let read = unwrap_ok(adapter.get_relation("rel_rt2").await, "get");
        assert_eq!(read.revision, Some(1));
        let mut to_write = read;
        to_write.label = Some("round-tripped".to_string());
        let written = unwrap_ok(
            adapter.put_relation(to_write, Some(1)).await,
            "round-trip update",
        );
        assert_eq!(written.revision, Some(2));

        // Re-read: every local survived the get→put round-trip (clear-on-omit
        // did NOT fire because get populated them all).
        let r = unwrap_ok(adapter.get_relation("rel_rt2").await, "final get");
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

    // ── V1.146 P5 T2: unknown extensions.nexus key round-trip ──────────

    /// Create a Relation with an unknown `extensions.nexus` key, then re-read
    /// it and confirm the unknown key survives the SQLite round-trip.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_round_trips_unknown_nexus_key() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusAdapter::new(pool);

        // Create with a known nexus-local (world_id) plus an unknown key.
        let relation: Relation = serde_json::from_value(json!({
            "schema_version": 1,
            "relation_id": "rel_unknown_key",
            "from_id": "kb_src",
            "to_id": "kb_dst",
            "relation_type": "allied_with",
            "extensions": {
                "nexus": {
                    "world_id": "wld_rel",
                    "custom_flag": "experimental",
                    "priority": 3,
                    "vendor_data": {"source": "cli", "version": 2}
                }
            }
        }))
        .expect("valid spoke Relation with unknown nexus key");

        let created = unwrap_ok(adapter.put_relation(relation, None).await, "create");
        assert_eq!(created.revision, Some(1));

        // Re-read and confirm the unknown keys survived alongside the known ones.
        let r = unwrap_ok(adapter.get_relation("rel_unknown_key").await, "get");
        let key = RelationExtensionsKey::try_from("nexus").unwrap();
        let ns = r.extensions.get(&key).expect("nexus namespace present");

        // Known key: always present from the typed column.
        assert_eq!(ns.get("world_id"), Some(&json!("wld_rel")));

        // Unknown keys: survived the round-trip via extensions_nexus_json.
        assert_eq!(
            ns.get("custom_flag"),
            Some(&json!("experimental")),
            "unknown string key survives"
        );
        assert_eq!(
            ns.get("priority"),
            Some(&json!(3)),
            "unknown number key survives"
        );
        assert_eq!(
            ns.get("vendor_data"),
            Some(&json!({"source": "cli", "version": 2})),
            "unknown object key survives"
        );
    }

    /// V1.146 P5 T2: the unknown-key round-trip also holds across updates.
    /// Create with unknown keys, update (mutating only the label), re-read —
    /// unknown keys must still be present.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_relation_update_preserves_unknown_nexus_key() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusAdapter::new(pool);

        let seed: Relation = serde_json::from_value(json!({
            "schema_version": 1,
            "relation_id": "rel_upd_unk",
            "from_id": "kb_src",
            "to_id": "kb_dst",
            "relation_type": "allied_with",
            "extensions": {
                "nexus": {
                    "world_id": "wld_rel",
                    "custom_tag": "imported",
                    "batch_id": "B42"
                }
            }
        }))
        .expect("valid seed Relation");

        let created = unwrap_ok(adapter.put_relation(seed, None).await, "create");
        assert_eq!(created.revision, Some(1));

        // Update: change only the label. The unknown keys must survive.
        let mut update = created;
        update.label = Some("updated label".to_string());
        let updated = unwrap_ok(adapter.put_relation(update, Some(1)).await, "update");
        assert_eq!(updated.revision, Some(2));

        let r = unwrap_ok(
            adapter.get_relation("rel_upd_unk").await,
            "get after update",
        );
        let key = RelationExtensionsKey::try_from("nexus").unwrap();
        let ns = r.extensions.get(&key).expect("nexus namespace present");

        assert_eq!(r.label.as_deref(), Some("updated label"));
        assert_eq!(ns.get("world_id"), Some(&json!("wld_rel")));
        assert_eq!(
            ns.get("custom_tag"),
            Some(&json!("imported")),
            "unknown key survives update cycle"
        );
        assert_eq!(
            ns.get("batch_id"),
            Some(&json!("B42")),
            "unknown key survives update cycle"
        );
    }

    // ── V1.149 P1: list_hop_edges_for_world (inherent hop-edge loader) ──

    /// Seed a confirmed + a suggested relation through the port, then load
    /// hop edges: the loader must return only the confirmed graph, mapped to
    /// engine-local `HopEdge`s (the `RelationPort` gap — get/put only — is
    /// bridged by the storage list primitive, spec §5).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_hop_edges_for_world_loads_confirmed_graph_only() {
        let (pool, _dir) = fresh_pool().await;
        seed_world_and_endpoints(&pool).await;

        let adapter = NexusAdapter::new(pool);
        // Two confirmed edges (different types) + one extraction suggestion
        // (needs_review = true — must be excluded from lore hops).
        unwrap_ok(
            adapter
                .put_relation(spoke_relation("rel_hop1", "kb_src", "kb_dst"), None)
                .await,
            "create confirmed edge 1",
        );
        unwrap_ok(
            adapter
                .put_relation(spoke_relation("rel_hop2", "kb_dst", "kb_src"), None)
                .await,
            "create confirmed edge 2",
        );
        let suggested: Relation = serde_json::from_value(json!({
            "schema_version": 1,
            "relation_id": "rel_sugg",
            "from_id": "kb_src",
            "to_id": "kb_dst",
            "relation_type": "suggests",
            "label": "extraction suggestion",
            "extensions": {
                "nexus": {
                    "world_id": "wld_rel",
                    "needs_review": true
                }
            }
        }))
        .expect("valid spoke Relation fixture (suggested)");
        unwrap_ok(
            adapter.put_relation(suggested, None).await,
            "create suggested relation",
        );

        let edges = adapter
            .list_hop_edges_for_world("wld_rel")
            .await
            .expect("hop-edge load succeeds");
        assert_eq!(edges.len(), 2, "suggested (needs_review) edge excluded");

        let by_id: std::collections::HashMap<&str, &HopEdge> =
            edges.iter().map(|e| (e.relation_id.as_str(), e)).collect();
        let e1 = by_id.get("rel_hop1").expect("rel_hop1 present");
        assert_eq!(e1.from_id, "kb_src");
        assert_eq!(e1.to_id, "kb_dst");
        assert_eq!(e1.relation_type, "allied_with");
        let e2 = by_id.get("rel_hop2").expect("rel_hop2 present");
        assert_eq!(e2.from_id, "kb_dst");
        assert_eq!(e2.to_id, "kb_src");
        assert_eq!(e2.relation_type, "allied_with");
        assert!(
            !by_id.contains_key("rel_sugg"),
            "extraction suggestion must not appear in hop edges"
        );
    }
}
