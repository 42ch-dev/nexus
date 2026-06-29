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

/// Row type matching the `kb_relationships` DDL (V1.74 + V1.76 columns).
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

    sqlx::query!(
        r#"INSERT INTO kb_relationships
           (relationship_id, world_id, source_entity_id, target_entity_id,
            relation_type, custom_label, symmetric, confidence,
            source_anchor_ids, metadata, created_at, updated_at, revision,
            needs_review, source)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
    })
}

/// CAS-update a `kb_relationships` row inside a caller-managed transaction.
///
/// The update only applies when `revision = expected_revision`. On mismatch,
/// returns [`LocalDbError::VersionMismatch`] with the actual current revision.
/// On success the revision is bumped to `expected_revision + 1` and the
/// updated row is returned so callers can project it without a post-commit
/// re-read.
///
/// `existing` supplies the immutable columns (`world_id`, `source_entity_id`,
/// `target_entity_id`, `created_at`) that are not part of the update payload.
///
/// # Errors
///
/// Returns [`LocalDbError::VersionMismatch`] on stale OCC, or
/// [`LocalDbError::Sqlx`] on database failure.
pub async fn update_relationship_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    relationship_id: &str,
    params: &UpdateRelationshipParams,
    expected_revision: i64,
    existing: &KbRelationshipRow,
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

    let result = sqlx::query!(
        r#"UPDATE kb_relationships SET
             relation_type = ?,
             custom_label = ?,
             symmetric = ?,
             confidence = ?,
             source_anchor_ids = ?,
             metadata = ?,
             needs_review = ?,
             updated_at = ?,
             revision = ?
           WHERE relationship_id = ? AND revision = ?"#,
        params.relation_type,
        custom_label_ref,
        symmetric_i64,
        params.confidence,
        source_anchor_json,
        metadata_json,
        needs_review_i64,
        params.updated_at,
        new_revision,
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
             source
           FROM kb_relationships
           WHERE relationship_id = ?"#,
        relationship_id,
    )
    .fetch_optional(pool)
    .await?;

    row.ok_or_else(|| LocalDbError::Sqlx(sqlx::Error::RowNotFound))
}

/// List all relationships in a world, ordered by `updated_at` descending.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn list_relationships_for_world(
    pool: &SqlitePool,
    world_id: &str,
) -> Result<Vec<KbRelationshipRow>, LocalDbError> {
    let rows = sqlx::query_as!(
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
             source
           FROM kb_relationships
           WHERE world_id = ?
           ORDER BY updated_at DESC"#,
        world_id,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

// ── V1.76 extraction-suggestion support ───────────────────────────────

/// Resolve a non-deleted `kb_key_blocks` id by `canonical_name` for one world.
///
/// Used by the extraction pipeline to resolve relationship endpoints before
/// persisting a suggestion (entity-scope-model §5.6 extraction ordering).
///
/// - `block_type = Some(bt)`: resolve by `(world_id, block_type, canonical_name)`
///   against non-deleted `KeyBlocks`. Returns `None` when no row matches.
/// - `block_type = None`: resolve case-insensitively by
///   `(world_id, canonical_name)` and require **exactly one** non-deleted
///   `KeyBlock` to match. Returns `None` when zero or more than one match
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
/// endpoint entity ids to existing non-deleted `KeyBlocks` via
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
            },
            0,
            &existing,
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
            },
            99,
            &existing,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, LocalDbError::VersionMismatch { .. }));
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
                },
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();

        let rows = list_relationships_for_world(&pool, &world_id)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
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

        let resolved =
            resolve_entity_by_canonical_name(&pool, &world_id, "KB_SOURCE", None)
                .await
                .unwrap();
        assert_eq!(resolved.as_deref(), Some(source_id.as_str()));
    }

    #[tokio::test]
    async fn resolve_entity_missing_returns_none() {
        let (pool, _dir) = fresh_pool().await;
        let (world_id, _source_id, _target_id) = seed_world_and_entities(&pool).await;

        let resolved =
            resolve_entity_by_canonical_name(&pool, &world_id, "nonexistent", None)
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
        let rows = list_relationships_for_world(&pool, &world_id)
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
            &pool, &world_id, &source_id, &target_id, "custom", Some("bond"), true, None, None, &now,
        )
        .await
        .unwrap();
        let second = upsert_extraction_relationship(
            &pool, &world_id, &source_id, &target_id, "custom", Some("oath"), true, None, None, &now,
        )
        .await
        .unwrap();
        assert!(
            second,
            "different custom_label is a distinct suggestion (not deduped)"
        );
    }
}
