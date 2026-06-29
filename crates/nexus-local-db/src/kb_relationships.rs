//! World KB relationship storage (V1.74 Track A).
//!
//! Provides CRUD + per-row OCC helpers for the `kb_relationships` table.
//! All writes are transaction-aware (`*_in_tx`) so they can compose atomically
//! with sibling operations if needed.

use crate::cas::cas_check_with_version_column;
use crate::LocalDbError;
use sqlx::SqlitePool;

/// Generate a new relationship id (`rel_<uuid>`).
#[must_use]
pub fn generate_relationship_id() -> String {
    format!("rel_{}", uuid::Uuid::new_v4().simple())
}

/// Row type matching the `kb_relationships` DDL.
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
            source_anchor_ids, metadata, created_at, updated_at, revision)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
             updated_at = ?,
             revision = ?
           WHERE relationship_id = ? AND revision = ?"#,
        params.relation_type,
        custom_label_ref,
        symmetric_i64,
        params.confidence,
        source_anchor_json,
        metadata_json,
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
             revision
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
             revision
           FROM kb_relationships
           WHERE world_id = ?
           ORDER BY updated_at DESC"#,
        world_id,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
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
}
