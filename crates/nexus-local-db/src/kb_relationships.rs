//! World KB relationship storage (V1.74 Track A; V1.76 Track A extraction gate).
//!
//! Provides CRUD + per-row OCC helpers for the `kb_relationships` table.
//! All writes are transaction-aware (`*_in_tx`) so they can compose atomically
//! with sibling operations if needed.
//!
//! V1.76 adds the `needs_review` extraction-suggestion gate + `source`
//! provenance column, plus [`resolve_entity_by_canonical_name`] (endpoint
//! resolution for extraction) and [`upsert_extraction_relationship`] (idempotent
//! extraction-sourced suggestion persistence).

use crate::cas::cas_check_with_version_column;
use crate::LocalDbError;
use sqlx::SqlitePool;

/// Provenance marker for a relationship row (V1.76).
///
/// `Manual` rows are author-created via the patch-relationship route;
/// `Extraction` rows are proposed by `nexus.llm.extract` and land behind the
/// `needs_review` gate until the author promotes them.
pub const SOURCE_MANUAL: &str = "manual";
pub const SOURCE_EXTRACTION: &str = "extraction";

/// Generate a new relationship id (`rel_<uuid>`).
#[must_use]
pub fn generate_relationship_id() -> String {
    format!("rel_{}", uuid::Uuid::new_v4().simple())
}

/// Row type matching the `kb_relationships` DDL (V1.74 + V1.76 + V1.146 columns).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KbRelationshipRow {
    pub relationship_id: String,
    pub world_id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relation_type: String,
    pub custom_label: Option<String>,
    pub symmetric: i64,
    pub confidence: Option<f64>,
    pub source_anchor_ids: Option<String>,
    pub metadata: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub revision: i64,
    /// V1.76: 1 = extraction suggestion (hidden from the default graph);
    /// 0 = author-confirmed. Promotion clears the flag.
    pub needs_review: i64,
    /// V1.76: provenance — [`SOURCE_MANUAL`] or [`SOURCE_EXTRACTION`].
    pub source: String,
    /// V1.146 P5 T1: full serialized `extensions.nexus` namespace (round-trip).
    /// Known identity fields stay authoritative in their typed columns above;
    /// this column preserves unknown keys when a spoke Relation transits `SQLite`.
    pub extensions_nexus_json: Option<String>,
}

/// Params for inserting a new relationship row.
#[derive(Debug, Clone)]
pub struct InsertRelationshipParams {
    pub relationship_id: String,
    pub world_id: String,
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relation_type: String,
    pub custom_label: Option<String>,
    pub symmetric: bool,
    pub confidence: Option<f64>,
    pub source_anchor_ids: Vec<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
    /// V1.76: `needs_review` gate. `false` for manual author adds; `true` for
    /// extraction-sourced suggestions.
    pub needs_review: bool,
    /// V1.76: provenance. [`SOURCE_MANUAL`] for author adds;
    /// [`SOURCE_EXTRACTION`] for extraction suggestions.
    pub source: String,
    /// V1.146 P5 T1: serialized `extensions.nexus` for round-trip preservation.
    /// `None` (NULL in DB) for rows without extra extension keys.
    pub extensions_nexus_json: Option<String>,
}

/// Params for updating an existing relationship row.
#[derive(Debug, Clone)]
pub struct UpdateRelationshipParams {
    pub relation_type: String,
    pub custom_label: Option<String>,
    pub symmetric: bool,
    pub confidence: Option<f64>,
    pub source_anchor_ids: Vec<String>,
    pub metadata: Option<serde_json::Value>,
    pub updated_at: String,
    /// V1.76: `needs_review` gate. Promotion sets this to `false`; the existing
    /// patch-relationship route carries it so no second promotion state machine
    /// is needed. `source` is immutable and not part of the update payload.
    pub needs_review: bool,
    /// V1.146 P5 T1: serialized `extensions.nexus` for round-trip preservation.
    /// `None` clears the column; `Some(...)` writes it.
    pub extensions_nexus_json: Option<String>,
}

pub(crate) fn bool_to_i64(v: bool) -> i64 {
    i64::from(v)
}

#[cfg(test)]
const fn i64_to_bool(v: i64) -> bool {
    v != 0
}

pub(crate) fn serialize_string_array(ids: &[String]) -> String {
    serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string())
}

/// Insert a new `kb_relationships` row inside a caller-managed transaction.
///
/// Returns the inserted row (revision 0) so callers can project it without a
/// post-commit re-read.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure (including FK violations).
pub async fn insert_relationship_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    params: &InsertRelationshipParams,
) -> Result<KbRelationshipRow, LocalDbError> {
    let symmetric_i64 = bool_to_i64(params.symmetric);
    let needs_review_i64 = bool_to_i64(params.needs_review);
    let source_anchor_json = serialize_string_array(&params.source_anchor_ids);
    let metadata_json = params
        .metadata
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));
    let custom_label_ref = params.custom_label.as_deref();

    let extensions_nexus_ref = params.extensions_nexus_json.as_deref();

    sqlx::query!(
        r#"INSERT INTO kb_relationships
           (relationship_id, world_id, source_entity_id, target_entity_id,
            relation_type, custom_label, symmetric, confidence,
            source_anchor_ids, metadata, created_at, updated_at, revision,
            needs_review, source, extensions_nexus_json)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        params.relationship_id,
        params.world_id,
        params.source_entity_id,
        params.target_entity_id,
        params.relation_type,
        custom_label_ref,
        symmetric_i64,
        params.confidence,
        source_anchor_json,
        metadata_json,
        params.created_at,
        params.updated_at,
        0i64,
        needs_review_i64,
        params.source,
        extensions_nexus_ref,
    )
    .execute(&mut **tx)
    .await?;

    Ok(KbRelationshipRow {
        relationship_id: params.relationship_id.clone(),
        world_id: params.world_id.clone(),
        source_entity_id: params.source_entity_id.clone(),
        target_entity_id: params.target_entity_id.clone(),
        relation_type: params.relation_type.clone(),
        custom_label: params.custom_label.clone(),
        symmetric: symmetric_i64,
        confidence: params.confidence,
        source_anchor_ids: Some(source_anchor_json),
        metadata: metadata_json,
        created_at: params.created_at.clone(),
        updated_at: params.updated_at.clone(),
        revision: 0,
        needs_review: needs_review_i64,
        source: params.source.clone(),
        extensions_nexus_json: params.extensions_nexus_json.clone(),
    })
}

/// CAS-update a `kb_relationships` row inside a caller-managed transaction.
///
/// The update only applies when `revision = expected_revision` AND the stored
/// `world_id` equals `world_id` (V1.154 P2, R3 closure, spec §3.2 LOCKED:
/// `WHERE relationship_id = ? AND revision = ? AND world_id = ?`). On a
/// version mismatch, returns [`LocalDbError::VersionMismatch`] with the
/// actual current revision; on a world mismatch (a cross-process writer moved
/// the row between the caller's verified read and the CAS), returns
/// [`LocalDbError::WorldConflict`] — never a generic OCC failure. On success
/// the revision is bumped to `expected_revision + 1` and the updated row is
/// returned so callers can project it without a post-commit re-read.
///
/// `existing` supplies the immutable columns (`world_id`, `source_entity_id`,
/// `target_entity_id`, `created_at`) that are not part of the update payload.
/// `world_id` is the stored-world expected by the request (the world the
/// caller verified on read) — it joins the CAS predicate and is NOT taken
/// from `existing` (a fresh pre-read would mask the race this predicate
/// closes).
///
/// # Errors
///
/// Returns [`LocalDbError::VersionMismatch`] on stale OCC,
/// [`LocalDbError::WorldConflict`] when the row now lives in another world,
/// or [`LocalDbError::Sqlx`] on database failure.
pub async fn update_relationship_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    relationship_id: &str,
    params: &UpdateRelationshipParams,
    expected_revision: i64,
    existing: &KbRelationshipRow,
    world_id: &str,
) -> Result<KbRelationshipRow, LocalDbError> {
    let new_revision = expected_revision + 1;
    let symmetric_i64 = bool_to_i64(params.symmetric);
    let needs_review_i64 = bool_to_i64(params.needs_review);
    let source_anchor_json = serialize_string_array(&params.source_anchor_ids);
    let metadata_json = params
        .metadata
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));
    let custom_label_ref = params.custom_label.as_deref();

    let extensions_nexus_ref = params.extensions_nexus_json.as_deref();

    let result = sqlx::query!(
        r#"UPDATE kb_relationships SET
             relation_type = ?,
             custom_label = ?,
             symmetric = ?,
             confidence = ?,
             source_anchor_ids = ?,
             metadata = ?,
             needs_review = ?,
             extensions_nexus_json = ?,
             updated_at = ?,
             revision = ?
           WHERE relationship_id = ? AND revision = ? AND world_id = ?"#,
        params.relation_type,
        custom_label_ref,
        symmetric_i64,
        params.confidence,
        source_anchor_json,
        metadata_json,
        needs_review_i64,
        extensions_nexus_ref,
        params.updated_at,
        new_revision,
        relationship_id,
        expected_revision,
        world_id,
    )
    .execute(&mut **tx)
    .await?;

    if result.rows_affected() == 0 {
        // V1.154 P2 (R3 closure): disambiguate a world move from a version
        // mismatch before falling into the generic OCC classification. The
        // caller's revision may have been perfectly valid — the WORLD moved
        // (e.g. a second Connect process / the daemon on the same workspace
        // DB). `world_id` is NOT NULL on `kb_relationships` (migration
        // 202606290001), so a present row always carries its stored world.
        let stored_world: Option<String> =
            sqlx::query_scalar("SELECT world_id FROM kb_relationships WHERE relationship_id = ?")
                .bind(relationship_id)
                .fetch_optional(&mut **tx)
                .await?;
        if let Some(actual_world) = stored_world.as_ref() {
            if actual_world != world_id {
                return Err(LocalDbError::WorldConflict {
                    table: "kb_relationships".to_string(),
                    id: relationship_id.to_string(),
                    expected_world: world_id.to_string(),
                    actual_world: actual_world.clone(),
                });
            }
        }
    }

    cas_check_with_version_column(
        &mut **tx,
        result.rows_affected(),
        "kb_relationships",
        "relationship_id",
        relationship_id,
        "revision",
        expected_revision,
    )
    .await?;

    Ok(KbRelationshipRow {
        relationship_id: relationship_id.to_string(),
        world_id: existing.world_id.clone(),
        source_entity_id: existing.source_entity_id.clone(),
        target_entity_id: existing.target_entity_id.clone(),
        relation_type: params.relation_type.clone(),
        custom_label: params.custom_label.clone(),
        symmetric: symmetric_i64,
        confidence: params.confidence,
        source_anchor_ids: Some(source_anchor_json),
        metadata: metadata_json,
        created_at: existing.created_at.clone(),
        updated_at: params.updated_at.clone(),
        revision: new_revision,
        needs_review: needs_review_i64,
        // source is immutable — preserved from the existing row.
        source: existing.source.clone(),
        extensions_nexus_json: params.extensions_nexus_json.clone(),
    })
}

/// CAS-delete a `kb_relationships` row inside a caller-managed transaction.
///
/// The delete only applies when `revision = expected_revision`. On mismatch,
/// returns [`LocalDbError::VersionMismatch`].
///
/// # Errors
///
/// Returns [`LocalDbError::VersionMismatch`] on stale OCC, or
/// [`LocalDbError::Sqlx`] on database failure.
pub async fn delete_relationship_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    relationship_id: &str,
    expected_revision: i64,
) -> Result<(), LocalDbError> {
    let result = sqlx::query!(
        "DELETE FROM kb_relationships WHERE relationship_id = ? AND revision = ?",
        relationship_id,
        expected_revision,
    )
    .execute(&mut **tx)
    .await?;

    cas_check_with_version_column(
        &mut **tx,
        result.rows_affected(),
        "kb_relationships",
        "relationship_id",
        relationship_id,
        "revision",
        expected_revision,
    )
    .await?;

    Ok(())
}

/// Read one relationship row by id.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`](sqlx::Error::RowNotFound) when the row does not
/// exist, or [`LocalDbError::Sqlx`] on other database failures.
pub async fn get_relationship(
    pool: &SqlitePool,
    relationship_id: &str,
) -> Result<KbRelationshipRow, LocalDbError> {
    let row = sqlx::query_as!(
        KbRelationshipRow,
        r#"SELECT
             relationship_id,
             world_id,
             source_entity_id,
             target_entity_id,
             relation_type,
             custom_label as "custom_label?",
             symmetric,
             confidence as "confidence?",
             source_anchor_ids as "source_anchor_ids?",
             metadata as "metadata?",
             created_at,
             updated_at,
             revision,
             needs_review,
             source,
             extensions_nexus_json as "extensions_nexus_json?"
           FROM kb_relationships
           WHERE relationship_id = ?"#,
        relationship_id,
    )
    .fetch_optional(pool)
    .await?;

    row.ok_or_else(|| LocalDbError::Sqlx(sqlx::Error::RowNotFound))
}

/// List relationships in a world, ordered by `updated_at` descending, capped
/// by `limit` at the SQL layer.
///
/// V1.76: `include_suggested` gates the `needs_review` filter at the SQL layer
/// so the default (confirmed) graph uses the `(world_id, needs_review)` index
/// and never materializes extraction-suggestion rows. Pass `false` for the
/// confirmed graph (default path); pass `true` to fetch both confirmed and
/// suggested rows (Suggested triage pane).
///
/// V1.77: `limit` is pushed into the SQL `LIMIT ?` so the hot path never
/// materializes unbounded rows (qc3 W-QC3-P1-001). Callers that need
/// truncation detection pass `cap + 1` and inspect the returned length: a
/// result of `cap + 1` rows means the world exceeded the cap. The existing
/// `(world_id, needs_review)` / `(world_id)` indexes cover the `WHERE` +
/// `ORDER BY updated_at DESC` + `LIMIT` shape; no new migration is required.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn list_relationships_for_world(
    pool: &SqlitePool,
    world_id: &str,
    include_suggested: bool,
    limit: i64,
) -> Result<Vec<KbRelationshipRow>, LocalDbError> {
    // Two compile-time-checked static queries (sqlx macros can't express a
    // conditional WHERE clause). The default (`false`) branch pushes
    // `needs_review = 0` into SQL so SQLite uses
    // `idx_kb_relationships_world_id_needs_review` and skips hidden rows. The
    // `LIMIT ?` is pushed down (qc3 W-QC3-P1-001) so the cap bounds the DB
    // read + decode, not just the final projection.
    let rows = if include_suggested {
        sqlx::query_as!(
            KbRelationshipRow,
            r#"SELECT
              relationship_id,
              world_id,
              source_entity_id,
              target_entity_id,
              relation_type,
              custom_label as "custom_label?",
              symmetric,
              confidence as "confidence?",
              source_anchor_ids as "source_anchor_ids?",
              metadata as "metadata?",
              created_at,
              updated_at,
              revision,
              needs_review,
              source,
              extensions_nexus_json as "extensions_nexus_json?"
            FROM kb_relationships
            WHERE world_id = ?
            ORDER BY updated_at DESC
            LIMIT ?"#,
            world_id,
            limit,
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as!(
            KbRelationshipRow,
            r#"SELECT
              relationship_id,
              world_id,
              source_entity_id,
              target_entity_id,
              relation_type,
              custom_label as "custom_label?",
              symmetric,
              confidence as "confidence?",
              source_anchor_ids as "source_anchor_ids?",
              metadata as "metadata?",
              created_at,
              updated_at,
              revision,
              needs_review,
              source,
              extensions_nexus_json as "extensions_nexus_json?"
            FROM kb_relationships
            WHERE world_id = ? AND needs_review = 0
            ORDER BY updated_at DESC
            LIMIT ?"#,
            world_id,
            limit,
        )
        .fetch_all(pool)
        .await?
    };

    Ok(rows)
}

/// Keyset cursor for paginated reads of a world's confirmed relation graph.
///
/// Encodes the `(updated_at, relationship_id)` of the **last** row of the
/// previous page; the next page resumes strictly after it. `updated_at` is
/// the stored RFC3339 text (BINARY collation — lexicographic order matches
/// recency for the fixed format the daemon writes), and `relationship_id`
/// (the TEXT PRIMARY KEY) is the tiebreaker that makes the ordering total.
///
/// The `(world_id, needs_review)` graph index covers the page filter, so
/// keyset pagination needs no new index or migration (architect, V1.158 P2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipCursor {
    pub updated_at: String,
    pub relationship_id: String,
}

/// Keyset-paginated read of a world's **confirmed** relation graph
/// (`needs_review = 0`), ordered by `(updated_at DESC, relationship_id DESC)`.
///
/// The only consumer is hop-edge expansion (V1.158 P2 T3,
/// `NexusAdapter::list_hop_edges_for_world_paginated`), which walks a large
/// confirmed graph page by page instead of truncating at a hard cap. Callers
/// pass `cursor = Some(...)` to resume after the previous page's last row;
/// `None` starts at the newest rows. Pass `limit + 1` to detect overflow the
/// same way [`list_relationships_for_world`] callers do — a result of
/// `limit + 1` rows means the graph continues past the page.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn list_confirmed_relationships_paginated(
    pool: &SqlitePool,
    world_id: &str,
    limit: i64,
    cursor: Option<&RelationshipCursor>,
) -> Result<Vec<KbRelationshipRow>, LocalDbError> {
    // SAFETY: runtime query — this keyset variant is not yet in the committed
    // `.sqlx/` offline cache (`cargo sqlx prepare` re-run would promote it to
    // a compile-time macro; repo convention for new queries, e.g. works.rs).
    // Static SELECT with bind params only; `KbRelationshipRow: FromRow` maps
    // the snake_case columns 1:1 (nullable columns are Option fields).
    let sql = if cursor.is_some() {
        "SELECT relationship_id, world_id, source_entity_id, target_entity_id,
                relation_type, custom_label, symmetric, confidence,
                source_anchor_ids, metadata, created_at, updated_at, revision,
                needs_review, source, extensions_nexus_json
         FROM kb_relationships
         WHERE world_id = ? AND needs_review = 0
           AND (updated_at < ? OR (updated_at = ? AND relationship_id < ?))
         ORDER BY updated_at DESC, relationship_id DESC
         LIMIT ?"
    } else {
        "SELECT relationship_id, world_id, source_entity_id, target_entity_id,
                relation_type, custom_label, symmetric, confidence,
                source_anchor_ids, metadata, created_at, updated_at, revision,
                needs_review, source, extensions_nexus_json
         FROM kb_relationships
         WHERE world_id = ? AND needs_review = 0
         ORDER BY updated_at DESC, relationship_id DESC
         LIMIT ?"
    };
    let mut q = sqlx::query_as::<_, KbRelationshipRow>(sql).bind(world_id);
    if let Some(c) = cursor {
        q = q.bind(&c.updated_at).bind(&c.updated_at).bind(&c.relationship_id);
    }
    let rows = q.bind(limit).fetch_all(pool).await?;
    Ok(rows)
}

// ── V1.76 extraction-suggestion support ───────────────────────────────

/// Resolve a non-deleted `kb_key_blocks` id by `canonical_name` for one world.
///
/// Used by the extraction pipeline to resolve relationship endpoints before
/// persisting a suggestion (entity-scope-model §5.6 extraction ordering).
///
/// - `block_type = Some(bt)`: resolve by `(world_id, block_type, canonical_name)`
///   against non-deleted `KnowledgeEntry` rows. Returns `None` when no row matches.
/// - `block_type = None`: resolve case-insensitively by
///   `(world_id, canonical_name)` and require **exactly one** non-deleted
///   `WorldKbEntry` to match. Returns `None` when zero or more than one match
///   (ambiguous → skip + log, per the architect lock).
///
/// `canonical_name` is matched case-insensitively in both branches so the LLM
/// is not penalized for casing drift.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn resolve_entity_by_canonical_name(
    pool: &SqlitePool,
    world_id: &str,
    canonical_name: &str,
    block_type: Option<&str>,
) -> Result<Option<String>, LocalDbError> {
    if let Some(bt) = block_type {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT key_block_id FROM kb_key_blocks \
             WHERE world_id = ? AND block_type = ? AND canonical_name = ? COLLATE NOCASE \
             AND status NOT IN ('deleted', 'merged', 'deprecated') \
             LIMIT 1",
        )
        .bind(world_id)
        .bind(bt)
        .bind(canonical_name)
        .fetch_optional(pool)
        .await?;
        Ok(id)
    } else {
        // SAFETY: static SELECT with bind params; case-insensitive resolve.
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT key_block_id FROM kb_key_blocks \
             WHERE world_id = ? AND canonical_name = ? COLLATE NOCASE \
             AND status NOT IN ('deleted', 'merged', 'deprecated')",
        )
        .bind(world_id)
        .bind(canonical_name)
        .fetch_all(pool)
        .await?;
        if rows.len() == 1 {
            Ok(Some(rows[0].0.clone()))
        } else {
            Ok(None)
        }
    }
}

/// Idempotent upsert of an extraction-sourced relationship suggestion.
///
/// Implements the V1.76 architect-locked dedup: a suggestion is keyed on
/// `(world_id, source_entity_id, target_entity_id, relation_type,
/// COALESCE(custom_label, ''), source = 'extraction')`. When a row with that
/// composite key already exists, this is a no-op (the suggestion is not
/// re-inserted and the revision is not bumped — rescan idempotency). Otherwise
/// a new row is inserted with `needs_review = 1`, `source = 'extraction'`, and
/// the verbatim `source_quote` carried in `metadata` for audit.
///
/// The caller (the review-time extraction hook) MUST have already resolved both
/// endpoint entity ids to existing non-deleted `KnowledgeEntry` rows via
/// [`resolve_entity_by_canonical_name`]; this function does not re-check.
///
/// Returns `Ok(true)` when a new suggestion row was inserted, `Ok(false)` when
/// the suggestion already existed (idempotent skip).
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure (including FK violations
/// if an endpoint entity id does not exist).
// Single dedicated extraction upsert; splitting into a builder adds
// indirection for one call-site, mirroring the insert_pending_with_llm allow.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_extraction_relationship(
    pool: &SqlitePool,
    world_id: &str,
    source_entity_id: &str,
    target_entity_id: &str,
    relation_type: &str,
    custom_label: Option<&str>,
    symmetric: bool,
    confidence: Option<f64>,
    source_quote: Option<&str>,
    now: &str,
) -> Result<bool, LocalDbError> {
    // SAFETY: static SELECT for the idempotency probe. NULL-safe comparison on
    // custom_label via COALESCE so a NULL and an empty suggestion key collide.
    let existing_id: Option<String> = sqlx::query_scalar(
        "SELECT relationship_id FROM kb_relationships \
         WHERE world_id = ? AND source_entity_id = ? AND target_entity_id = ? \
         AND relation_type = ? AND COALESCE(custom_label, '') = COALESCE(?, '') \
         AND source = 'extraction' LIMIT 1",
    )
    .bind(world_id)
    .bind(source_entity_id)
    .bind(target_entity_id)
    .bind(relation_type)
    .bind(custom_label)
    .fetch_optional(pool)
    .await?;

    if existing_id.is_some() {
        // Idempotent: the suggestion already exists; do not re-insert or bump
        // the revision (avoids churn on rescan).
        return Ok(false);
    }

    let metadata = source_quote.map(|q| serde_json::json!({ "source_quote": q }));
    let metadata_json = metadata
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));
    let relationship_id = generate_relationship_id();
    let symmetric_i64 = bool_to_i64(symmetric);

    sqlx::query!(
        r#"INSERT INTO kb_relationships
           (relationship_id, world_id, source_entity_id, target_entity_id,
            relation_type, custom_label, symmetric, confidence,
            source_anchor_ids, metadata, created_at, updated_at, revision,
            needs_review, source)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, '[]', ?, ?, ?, 0, 1, 'extraction')"#,
        relationship_id,
        world_id,
        source_entity_id,
        target_entity_id,
        relation_type,
        custom_label,
        symmetric_i64,
        confidence,
        metadata_json,
        now,
        now,
    )
    .execute(pool)
    .await?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_pool, run_migrations};

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_world_and_entities(pool: &SqlitePool) -> (String, String, String) {
        let world_id = "wld_test";
        let source_id = "kb_source";
        let target_id = "kb_target";

        sqlx::query!(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data)
             VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')"
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query!(
            r#"INSERT INTO narrative_worlds
               (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json)
               VALUES (?, 'wrk_test', 'ctr_test', 'Test World', 'test-world', 'active', 'private', 'manual', '{}')"#,
            world_id,
        )
        .execute(pool)
        .await
        .unwrap();

        for id in [source_id, target_id] {
            sqlx::query!(
                r#"INSERT INTO kb_key_blocks
                   (key_block_id, world_id, block_type, canonical_name, status)
                   VALUES (?, ?, 'character', ?, 'confirmed')"#,
                id,
                world_id,
                id,
            )
            .execute(pool)
            .await
            .unwrap();
        }

        (
            world_id.to_string(),
            source_id.to_string(),
            target_id.to_string(),
        )
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, source_id, target_id) = seed_world_and_entities(&pool).await;

        let mut tx = pool.begin().await.unwrap();
        let rel_id = generate_relationship_id();
        insert_relationship_in_tx(
            &mut tx,
            &InsertRelationshipParams {
                relationship_id: rel_id.clone(),
                world_id: world_id.clone(),
                source_entity_id: source_id,
                target_entity_id: target_id,
                relation_type: "allied_with".to_string(),
                custom_label: None,
                symmetric: false,
                confidence: Some(0.75),
                source_anchor_ids: vec!["sa_kb_source".to_string()],
                metadata: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                needs_review: false,
                source: SOURCE_MANUAL.to_string(),
                extensions_nexus_json: None,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let row = get_relationship(&pool, &rel_id).await.unwrap();
        assert_eq!(row.world_id, world_id);
        // symmetric was false -> stored integer should be 0
        assert!(!i64_to_bool(row.symmetric));
        assert_eq!(row.confidence, Some(0.75));
    }

    #[tokio::test]
    async fn test_update_cas_bumps_revision() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, source_id, target_id) = seed_world_and_entities(&pool).await;

        let mut tx = pool.begin().await.unwrap();
        let rel_id = generate_relationship_id();
        insert_relationship_in_tx(
            &mut tx,
            &InsertRelationshipParams {
                relationship_id: rel_id.clone(),
                world_id: world_id.clone(),
                source_entity_id: source_id,
                target_entity_id: target_id,
                relation_type: "allied_with".to_string(),
                custom_label: None,
                symmetric: false,
                confidence: None,
                source_anchor_ids: vec![],
                metadata: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                needs_review: false,
                source: SOURCE_MANUAL.to_string(),
                extensions_nexus_json: None,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let existing = get_relationship(&pool, &rel_id).await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        let row = update_relationship_in_tx(
            &mut tx,
            &rel_id,
            &UpdateRelationshipParams {
                relation_type: "opposes".to_string(),
                custom_label: None,
                symmetric: true,
                confidence: Some(0.9),
                source_anchor_ids: vec![],
                metadata: None,
                updated_at: chrono::Utc::now().to_rfc3339(),
                needs_review: false,
                extensions_nexus_json: None,
            },
            0,
            &existing,
            &world_id,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(row.revision, 1);
        assert_eq!(row.relation_type, "opposes");
        assert!(i64_to_bool(row.symmetric));
        let row = get_relationship(&pool, &rel_id).await.unwrap();
        assert_eq!(row.relation_type, "opposes");
        assert!(i64_to_bool(row.symmetric));
    }

    #[tokio::test]
    async fn test_update_cas_fails_on_stale_revision() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, source_id, target_id) = seed_world_and_entities(&pool).await;

        let mut tx = pool.begin().await.unwrap();
        let rel_id = generate_relationship_id();
        insert_relationship_in_tx(
            &mut tx,
            &InsertRelationshipParams {
                relationship_id: rel_id.clone(),
                world_id: world_id.clone(),
                source_entity_id: source_id,
                target_entity_id: target_id,
                relation_type: "allied_with".to_string(),
                custom_label: None,
                symmetric: false,
                confidence: None,
                source_anchor_ids: vec![],
                metadata: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                needs_review: false,
                source: SOURCE_MANUAL.to_string(),
                extensions_nexus_json: None,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let existing = get_relationship(&pool, &rel_id).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        let err = update_relationship_in_tx(
            &mut tx,
            &rel_id,
            &UpdateRelationshipParams {
                relation_type: "opposes".to_string(),
                custom_label: None,
                symmetric: false,
                confidence: None,
                source_anchor_ids: vec![],
                metadata: None,
                updated_at: chrono::Utc::now().to_rfc3339(),
                needs_review: false,
                extensions_nexus_json: None,
            },
            99,
            &existing,
            &world_id,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, LocalDbError::VersionMismatch { .. }));
    }

    #[tokio::test]
    async fn test_update_cas_rejects_row_in_foreign_world() {
        // R3 regression (relate side, atomic source of truth): the caller's
        // world-verified preimage (wld_test) is stale — another writer moved
        // the row to wld_other without bumping the revision, so the pre-fix
        // id+revision CAS would have succeeded. The world-aware predicate
        // must deny with WorldConflict, NOT a plain VersionMismatch.
        let (pool, _dir) = fresh_pool().await;
        let (world_id, source_id, target_id) = seed_world_and_entities(&pool).await;
        sqlx::query!(
            r#"INSERT INTO narrative_worlds
               (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json)
               VALUES ('wld_other', 'wrk_test', 'ctr_test', 'Other World', 'other-world', 'active', 'private', 'manual', '{}')"#
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        let rel_id = generate_relationship_id();
        insert_relationship_in_tx(
            &mut tx,
            &InsertRelationshipParams {
                relationship_id: rel_id.clone(),
                world_id: world_id.clone(),
                source_entity_id: source_id,
                target_entity_id: target_id,
                relation_type: "allied_with".to_string(),
                custom_label: None,
                symmetric: false,
                confidence: None,
                source_anchor_ids: vec![],
                metadata: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                needs_review: false,
                source: SOURCE_MANUAL.to_string(),
                extensions_nexus_json: None,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // The caller's preimage: read BEFORE the interleaved writer moves the
        // row (the OCC `existing` snapshot the update path would carry).
        let existing = get_relationship(&pool, &rel_id).await.unwrap();
        assert_eq!(
            existing.world_id, world_id,
            "gate-check precondition: row is in the claimed world"
        );

        // "Other writer" (Connect process ∥ daemon) moves the row across
        // worlds between the gate-check and the CAS, revision untouched.
        sqlx::query!(
            "UPDATE kb_relationships SET world_id = ? WHERE relationship_id = ?",
            "wld_other",
            rel_id,
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        let err = update_relationship_in_tx(
            &mut tx,
            &rel_id,
            &UpdateRelationshipParams {
                relation_type: "opposes".to_string(),
                custom_label: None,
                symmetric: false,
                confidence: None,
                source_anchor_ids: vec![],
                metadata: None,
                updated_at: chrono::Utc::now().to_rfc3339(),
                needs_review: false,
                extensions_nexus_json: None,
            },
            0,
            &existing,
            &world_id,
        )
        .await
        .unwrap_err();
        match err {
            LocalDbError::WorldConflict {
                table,
                id,
                expected_world,
                actual_world,
            } => {
                assert_eq!(table, "kb_relationships");
                assert_eq!(id, rel_id);
                assert_eq!(expected_world, world_id);
                assert_eq!(actual_world, "wld_other");
            }
            other => panic!("world mismatch must classify as WorldConflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_delete_cas() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, source_id, target_id) = seed_world_and_entities(&pool).await;

        let mut tx = pool.begin().await.unwrap();
        let rel_id = generate_relationship_id();
        insert_relationship_in_tx(
            &mut tx,
            &InsertRelationshipParams {
                relationship_id: rel_id.clone(),
                world_id: world_id.clone(),
                source_entity_id: source_id,
                target_entity_id: target_id,
                relation_type: "allied_with".to_string(),
                custom_label: None,
                symmetric: false,
                confidence: None,
                source_anchor_ids: vec![],
                metadata: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                needs_review: false,
                source: SOURCE_MANUAL.to_string(),
                extensions_nexus_json: None,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        delete_relationship_in_tx(&mut tx, &rel_id, 0)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert!(get_relationship(&pool, &rel_id).await.is_err());
    }

    #[tokio::test]
    async fn test_list_for_world() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, source_id, target_id) = seed_world_and_entities(&pool).await;

        let mut tx = pool.begin().await.unwrap();
        for i in 0..3 {
            insert_relationship_in_tx(
                &mut tx,
                &InsertRelationshipParams {
                    relationship_id: generate_relationship_id(),
                    world_id: world_id.clone(),
                    source_entity_id: source_id.clone(),
                    target_entity_id: target_id.clone(),
                    relation_type: "allied_with".to_string(),
                    custom_label: None,
                    symmetric: false,
                    confidence: None,
                    source_anchor_ids: vec![],
                    metadata: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    updated_at: format!("{}-{:02}", chrono::Utc::now().to_rfc3339(), i),
                    needs_review: false,
                    source: SOURCE_MANUAL.to_string(),
                    extensions_nexus_json: None,
                },
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();

        let rows = list_relationships_for_world(&pool, &world_id, true, 100)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
    }

    // ── V1.77: SQL LIMIT pushdown (qc3 W-QC3-P1-001) ─────────────────────

    #[tokio::test]
    async fn test_list_for_world_respects_sql_limit() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, source_id, target_id) = seed_world_and_entities(&pool).await;

        // Seed 3 confirmed rows.
        let mut tx = pool.begin().await.unwrap();
        for i in 0..3 {
            insert_relationship_in_tx(
                &mut tx,
                &InsertRelationshipParams {
                    relationship_id: generate_relationship_id(),
                    world_id: world_id.clone(),
                    source_entity_id: source_id.clone(),
                    target_entity_id: target_id.clone(),
                    relation_type: "allied_with".to_string(),
                    custom_label: None,
                    symmetric: false,
                    confidence: None,
                    source_anchor_ids: vec![],
                    metadata: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    updated_at: format!("{}-{:02}", chrono::Utc::now().to_rfc3339(), i),
                    needs_review: false,
                    source: SOURCE_MANUAL.to_string(),
                    extensions_nexus_json: None,
                },
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();

        // SQL LIMIT caps at the DB layer: pass `cap + 1 = 3` and observe 3
        // (no truncation), then pass `2` and observe the truncation sentinel
        // shape (rows.len() == limit).
        let rows = list_relationships_for_world(&pool, &world_id, true, 3)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3, "limit >= row count returns all rows");

        let rows = list_relationships_for_world(&pool, &world_id, true, 2)
            .await
            .unwrap();
        assert_eq!(
            rows.len(),
            2,
            "SQL LIMIT pushdown caps the materialized row count"
        );
    }

    // ── V1.158 P2 T3: keyset pagination (hop-edge walk, R-V1149P1-001) ──

    #[tokio::test]
    async fn list_confirmed_relationships_paginated_resumes_after_cursor() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, source_id, target_id) = seed_world_and_entities(&pool).await;

        // Seed 5 confirmed rows with strictly increasing RFC3339 `updated_at`
        // (lexicographic order == chronological order for the fixed format).
        let mut tx = pool.begin().await.unwrap();
        for i in 0..5 {
            let ts = format!("2026-08-{:02}T00:00:00+00:00", i + 1);
            insert_relationship_in_tx(
                &mut tx,
                &InsertRelationshipParams {
                    relationship_id: format!("rel_pg{i}"),
                    world_id: world_id.clone(),
                    source_entity_id: source_id.clone(),
                    target_entity_id: target_id.clone(),
                    relation_type: "allied_with".to_string(),
                    custom_label: None,
                    symmetric: false,
                    confidence: None,
                    source_anchor_ids: vec![],
                    metadata: None,
                    created_at: ts.clone(),
                    updated_at: ts,
                    needs_review: false,
                    source: SOURCE_MANUAL.to_string(),
                    extensions_nexus_json: None,
                },
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();

        // Page 1 (page size 2 + overflow probe): the caller passes
        // `limit + 1` per the `list_relationships_for_world` convention; a
        // result of `limit + 1` rows means the graph continues past the page.
        let rows = list_confirmed_relationships_paginated(&pool, &world_id, 3, None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3, "overflow probe returns page + 1 rows");
        assert_eq!(rows[0].relationship_id, "rel_pg4");
        assert_eq!(rows[1].relationship_id, "rel_pg3");
        assert_eq!(rows[2].relationship_id, "rel_pg2", "probe row present");

        // Resume after rel_pg3's keyset → the next two rows (plus the next
        // probe), strictly after the cursor (no overlap, no gap).
        let cursor = RelationshipCursor {
            updated_at: rows[1].updated_at.clone(),
            relationship_id: rows[1].relationship_id.clone(),
        };
        let rows2 = list_confirmed_relationships_paginated(&pool, &world_id, 3, Some(&cursor))
            .await
            .unwrap();
        assert_eq!(
            rows2.iter().map(|r| r.relationship_id.as_str()).collect::<Vec<_>>(),
            vec!["rel_pg2", "rel_pg1", "rel_pg0"],
            "keyset resumes strictly after the cursor row"
        );

        // One row remains; the final page returns it without a probe row.
        let cursor2 = RelationshipCursor {
            updated_at: rows2[1].updated_at.clone(),
            relationship_id: rows2[1].relationship_id.clone(),
        };
        let rows3 = list_confirmed_relationships_paginated(&pool, &world_id, 3, Some(&cursor2))
            .await
            .unwrap();
        assert_eq!(
            rows3.iter().map(|r| r.relationship_id.as_str()).collect::<Vec<_>>(),
            vec!["rel_pg0"],
            "final page drains the remainder"
        );
    }

    // ── V1.76: extraction resolve + idempotent upsert ─────────────────────

    #[tokio::test]
    async fn resolve_entity_by_canonical_name_with_block_type() {
        let (pool, _dir) = fresh_pool().await;
        // seed_world_and_entities inserts kb_source + kb_target as 'character'
        // with canonical_name == id.
        let (world_id, source_id, _target_id) = seed_world_and_entities(&pool).await;

        let resolved =
            resolve_entity_by_canonical_name(&pool, &world_id, "kb_source", Some("character"))
                .await
                .unwrap();
        assert_eq!(resolved.as_deref(), Some(source_id.as_str()));
    }

    #[tokio::test]
    async fn resolve_entity_case_insensitive_without_block_type() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, source_id, _target_id) = seed_world_and_entities(&pool).await;

        let resolved = resolve_entity_by_canonical_name(&pool, &world_id, "KB_SOURCE", None)
            .await
            .unwrap();
        assert_eq!(resolved.as_deref(), Some(source_id.as_str()));
    }

    #[tokio::test]
    async fn resolve_entity_missing_returns_none() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _source_id, _target_id) = seed_world_and_entities(&pool).await;

        let resolved = resolve_entity_by_canonical_name(&pool, &world_id, "nonexistent", None)
            .await
            .unwrap();
        assert!(resolved.is_none());
    }

    #[tokio::test]
    async fn upsert_extraction_relationship_inserts_then_dedup() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, source_id, target_id) = seed_world_and_entities(&pool).await;
        let now = chrono::Utc::now().to_rfc3339();

        // First call inserts a suggestion.
        let inserted = upsert_extraction_relationship(
            &pool,
            &world_id,
            &source_id,
            &target_id,
            "allied_with",
            None,
            true,
            Some(0.8),
            Some("quote"),
            &now,
        )
        .await
        .unwrap();
        assert!(inserted, "first call inserts a new suggestion");

        // Second call with the same composite key is a no-op (dedup).
        let inserted_again = upsert_extraction_relationship(
            &pool,
            &world_id,
            &source_id,
            &target_id,
            "allied_with",
            None,
            true,
            Some(0.8),
            Some("quote"),
            &now,
        )
        .await
        .unwrap();
        assert!(
            !inserted_again,
            "second call is idempotent (no duplicate, no revision bump)"
        );

        // Only one row exists.
        let rows = list_relationships_for_world(&pool, &world_id, true, 100)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].needs_review, 1);
        assert_eq!(rows[0].source, "extraction");
    }

    #[tokio::test]
    async fn upsert_extraction_different_custom_label_inserts_separate() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, source_id, target_id) = seed_world_and_entities(&pool).await;
        let now = chrono::Utc::now().to_rfc3339();

        let _ = upsert_extraction_relationship(
            &pool,
            &world_id,
            &source_id,
            &target_id,
            "custom",
            Some("bond"),
            true,
            None,
            None,
            &now,
        )
        .await
        .unwrap();
        let second = upsert_extraction_relationship(
            &pool,
            &world_id,
            &source_id,
            &target_id,
            "custom",
            Some("oath"),
            true,
            None,
            None,
            &now,
        )
        .await
        .unwrap();
        assert!(
            second,
            "different custom_label is a distinct suggestion (not deduped)"
        );
    }
}
