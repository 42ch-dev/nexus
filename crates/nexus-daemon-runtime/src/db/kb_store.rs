//! SQLite-backed `KbStore` implementation.
//!
//! Implements the `KbStore` trait from `nexus-kb` using the workspace
//! `state.db` pool. Uses compile-time checked `sqlx` queries for all
//! static SQL.
//!
//! # Test helpers
//!
//! The [`seed`] submodule provides async functions to insert test data
//! (key blocks, source anchors) into the database for integration tests.

use nexus_contracts::BlockType;
use nexus_kb::key_block::{KeyBlock, KeyBlockBody};
use nexus_kb::query::{KbInsertResult, KbQuery, KbQueryResult};
use nexus_kb::source_anchor::SourceAnchor;
use nexus_kb::store::KbStoreError;
use nexus_kb::KbStore;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Test helpers for seeding KB data into the database.
///
/// These functions are intended for tests and development fixtures only.
/// They create the necessary FK parent rows (e.g. creators, worlds) if missing.
#[cfg(test)]
pub mod seed {
    use sqlx::SqlitePool;

    /// Seed a test world row (also seeds a minimal creator for FK).
    ///
    /// Reuses the same pattern as `narrative_gateway::seed::world`.
    pub async fn world(
        pool: &SqlitePool,
        world_id: &str,
        owner_creator_id: &str,
        title: &str,
        slug: &str,
        visibility: &str,
        time_policy: &str,
    ) {
        sqlx::query!(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, ?, 'active', datetime('now'), '{}')",
            owner_creator_id,
            owner_creator_id,
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query!(
            r#"INSERT INTO narrative_worlds
                (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json)
               VALUES (?, 'wrk_test', ?, ?, ?, 'active', ?, ?, '{}')"#,
            world_id,
            owner_creator_id,
            title,
            slug,
            visibility,
            time_policy,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// Seed a test key block row into `kb_key_blocks`.
    pub async fn key_block(
        pool: &SqlitePool,
        key_block_id: &str,
        world_id: &str,
        block_type: &str,
        canonical_name: &str,
        status: &str,
    ) {
        sqlx::query!(
            r#"INSERT INTO kb_key_blocks
                (key_block_id, world_id, block_type, canonical_name, status)
               VALUES (?, ?, ?, ?, ?)"#,
            key_block_id,
            world_id,
            block_type,
            canonical_name,
            status,
        )
        .execute(pool)
        .await
        .unwrap();
    }
}

/// SQLite-backed KB store.
///
/// Holds an `Arc<SqlitePool>` shared per active workspace. Construct once
/// at daemon/CLI boot and inject as `Arc<dyn KbStore>`.
pub struct SqliteKbStore {
    pool: Arc<SqlitePool>,
}

impl SqliteKbStore {
    /// Create a new store backed by the given pool.
    ///
    /// The pool is wrapped in `Arc` for cheap cloning if needed.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }
}

// Row type matching the kb_key_blocks DDL.
#[derive(Debug, Clone, sqlx::FromRow)]
struct KeyBlockRow {
    key_block_id: String,
    world_id: String,
    block_type: String,
    canonical_name: String,
    status: String,
    revision: Option<i64>,
    body_json: Option<String>,
    source_anchor_json: Option<String>,
    created_from_command_id: Option<String>,
    created_at: String,
    updated_at: Option<String>,
}

impl KeyBlockRow {
    fn to_key_block(&self) -> Result<KeyBlock, KbStoreError> {
        let block_type = parse_block_type(&self.block_type)?;
        let body = self
            .body_json
            .as_ref()
            .and_then(|s| serde_json::from_str::<KeyBlockBody>(s).ok());
        let source_anchor = self
            .source_anchor_json
            .as_ref()
            .and_then(|s| serde_json::from_str::<SourceAnchor>(s).ok());

        Ok(KeyBlock {
            schema_version: 1,
            key_block_id: self.key_block_id.clone(),
            world_id: self.world_id.clone(),
            block_type,
            canonical_name: self.canonical_name.clone(),
            status: self.status.clone(),
            revision: self.revision.map(i64::cast_unsigned),
            body,
            source_anchor,
            created_from_command_id: self.created_from_command_id.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        })
    }
}

/// Parse a `block_type` string into `BlockType`.
fn parse_block_type(s: &str) -> Result<BlockType, KbStoreError> {
    match s {
        "Character" => Ok(BlockType::Character),
        "Ability" => Ok(BlockType::Ability),
        "Scene" => Ok(BlockType::Scene),
        "Organization" => Ok(BlockType::Organization),
        "Item" => Ok(BlockType::Item),
        "Conflict" => Ok(BlockType::Conflict),
        "InfoPoint" => Ok(BlockType::InfoPoint),
        "Event" => Ok(BlockType::Event),
        _ => Err(KbStoreError::Storage(format!("unknown block_type: {s}"))),
    }
}

/// Convert a sqlx error into a `KbStoreError`.
fn db_err(err: &sqlx::Error) -> KbStoreError {
    KbStoreError::Storage(format!("database error: {err}"))
}

#[allow(clippy::future_not_send)]
impl KbStore for SqliteKbStore {
    async fn insert_key_block(&self, kb: KeyBlock) -> Result<KbInsertResult, KbStoreError> {
        let key_block_id = kb.key_block_id.clone();
        let world_id = kb.world_id.clone();
        let created_at = kb.created_at.clone();

        let body_json = kb
            .body
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap_or_default());
        let source_anchor_json = kb
            .source_anchor
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap_or_default());
        let block_type_str = format!("{:?}", kb.block_type);
        let revision_i64 = kb.revision.map(u64::cast_signed);

        sqlx::query!(
            r#"INSERT INTO kb_key_blocks
                (key_block_id, world_id, block_type, canonical_name, status, revision,
                 body_json, source_anchor_json, created_from_command_id, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            key_block_id,
            kb.world_id,
            block_type_str,
            kb.canonical_name,
            kb.status,
            revision_i64,
            body_json,
            source_anchor_json,
            kb.created_from_command_id,
            kb.created_at,
            kb.updated_at,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            // SQLite UNIQUE constraint violation
            if let sqlx::Error::Database(ref db_err_inner) = e {
                if db_err_inner.code().as_deref() == Some("2067") {
                    return KbStoreError::Duplicate {
                        world_id: kb.world_id.clone(),
                        name: kb.canonical_name.clone(),
                        block_type: kb.block_type,
                    };
                }
            }
            db_err(&e)
        })?;

        Ok(KbInsertResult {
            key_block_id,
            world_id,
            created_at,
        })
    }

    async fn get_key_block(&self, key_block_id: &str) -> Result<KeyBlock, KbStoreError> {
        let row = sqlx::query_as!(
            KeyBlockRow,
            r#"SELECT
                key_block_id as "key_block_id!",
                world_id as "world_id!",
                block_type as "block_type!",
                canonical_name as "canonical_name!",
                status as "status!",
                revision,
                body_json,
                source_anchor_json,
                created_from_command_id,
                created_at as "created_at!",
                updated_at
            FROM kb_key_blocks
            WHERE key_block_id = ?"#,
            key_block_id
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?
        .ok_or_else(|| KbStoreError::NotFound(key_block_id.to_string()))?;

        row.to_key_block()
    }

    async fn list_by_world(&self, world_id: &str) -> Result<Vec<KeyBlock>, KbStoreError> {
        let rows = sqlx::query_as!(
            KeyBlockRow,
            r#"SELECT
                key_block_id as "key_block_id!",
                world_id as "world_id!",
                block_type as "block_type!",
                canonical_name as "canonical_name!",
                status as "status!",
                revision,
                body_json,
                source_anchor_json,
                created_from_command_id,
                created_at as "created_at!",
                updated_at
            FROM kb_key_blocks
            WHERE world_id = ?
              AND status NOT IN ('deleted', 'merged', 'deprecated')
            ORDER BY created_at ASC"#,
            world_id
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?;

        rows.iter().map(KeyBlockRow::to_key_block).collect()
    }

    async fn query(&self, query: &KbQuery) -> Result<KbQueryResult, KbStoreError> {
        // Strategy: fetch all active blocks for the world, then apply
        // optional filters in-memory. This avoids complex dynamic SQL
        // and is efficient for per-world datasets (typically small).
        let all_active = self.list_by_world(&query.world_id).await?;

        let text_lower = query.text_search.as_deref().map(str::to_lowercase);

        let filtered: Vec<KeyBlock> = all_active
            .into_iter()
            .filter(|kb| {
                if let Some(bt) = query.block_type {
                    if kb.block_type != bt {
                        return false;
                    }
                }
                if let Some(ref name) = query.canonical_name {
                    if kb.canonical_name != *name {
                        return false;
                    }
                }
                if let Some(ref lower) = text_lower {
                    let hit_name = kb.canonical_name.to_lowercase().contains(lower);
                    let hit_summary = kb
                        .body
                        .as_ref()
                        .and_then(|b| b.summary.as_ref())
                        .is_some_and(|s| s.to_lowercase().contains(lower));
                    let hit_tags = kb
                        .body
                        .as_ref()
                        .and_then(|b| b.tags.as_ref())
                        .is_some_and(|tags| tags.iter().any(|t| t.to_lowercase().contains(lower)));
                    if !hit_name && !hit_summary && !hit_tags {
                        return false;
                    }
                }
                true
            })
            .collect();

        let total_count = filtered.len();
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(usize::MAX);
        let items: Vec<KeyBlock> = filtered.into_iter().skip(offset).take(limit).collect();
        let fetched = items.len();
        let has_more = offset + fetched < total_count;

        Ok(KbQueryResult {
            items,
            total_count,
            has_more,
        })
    }

    async fn attach_source_anchor(
        &self,
        key_block_id: &str,
        anchor: SourceAnchor,
    ) -> Result<(), KbStoreError> {
        // Verify block exists
        let exists: i64 = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM kb_key_blocks WHERE key_block_id = ?) as "exists!""#,
            key_block_id
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?;

        if exists == 0 {
            return Err(KbStoreError::NotFound(key_block_id.to_string()));
        }

        // Get next ordinal
        let max_ordinal: Option<i64> = sqlx::query_scalar!(
            r#"SELECT MAX(anchor_ordinal) as "max_ordinal: _" FROM kb_source_anchors WHERE key_block_id = ?"#,
            key_block_id
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?
        .flatten();

        let next_ordinal = max_ordinal.unwrap_or(-1) + 1;
        let anchor_json = serde_json::to_string(&anchor).unwrap_or_default();

        sqlx::query!(
            r#"INSERT INTO kb_source_anchors (key_block_id, anchor_ordinal, source_anchor_json)
               VALUES (?, ?, ?)"#,
            key_block_id,
            next_ordinal,
            anchor_json,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?;

        Ok(())
    }

    async fn get_anchors(&self, key_block_id: &str) -> Result<Vec<SourceAnchor>, KbStoreError> {
        let rows = sqlx::query!(
            r#"SELECT source_anchor_json as "source_anchor_json!"
               FROM kb_source_anchors
               WHERE key_block_id = ?
               ORDER BY anchor_ordinal ASC"#,
            key_block_id
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?;

        Ok(rows
            .iter()
            .filter_map(|r| serde_json::from_str::<SourceAnchor>(&r.source_anchor_json).ok())
            .collect())
    }

    async fn update_key_block(&self, kb: KeyBlock) -> Result<(), KbStoreError> {
        // Verify exists
        let existing = self.get_key_block(&kb.key_block_id).await?;

        // If name or type changed, check uniqueness
        if existing.canonical_name != kb.canonical_name || existing.block_type != kb.block_type {
            // Check for active duplicate (excluding self)
            let block_type_str = format!("{:?}", kb.block_type);
            let count: i64 = sqlx::query_scalar!(
                r#"SELECT COUNT(*) as count FROM kb_key_blocks
                   WHERE world_id = ?
                     AND block_type = ?
                     AND canonical_name = ?
                     AND key_block_id != ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')"#,
                kb.world_id,
                block_type_str,
                kb.canonical_name,
                kb.key_block_id,
            )
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| db_err(&e))?;

            if count > 0 {
                return Err(KbStoreError::Duplicate {
                    world_id: kb.world_id.clone(),
                    name: kb.canonical_name.clone(),
                    block_type: kb.block_type,
                });
            }
        }

        let body_json = kb
            .body
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap_or_default());
        let source_anchor_json = kb
            .source_anchor
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap_or_default());
        let block_type_str = format!("{:?}", kb.block_type);
        let revision_i64 = kb.revision.map(u64::cast_signed);

        sqlx::query!(
            r#"UPDATE kb_key_blocks SET
                block_type = ?,
                canonical_name = ?,
                status = ?,
                revision = ?,
                body_json = ?,
                source_anchor_json = ?,
                updated_at = ?
              WHERE key_block_id = ?"#,
            block_type_str,
            kb.canonical_name,
            kb.status,
            revision_i64,
            body_json,
            source_anchor_json,
            kb.updated_at,
            kb.key_block_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?;

        Ok(())
    }

    async fn delete_key_block(&self, key_block_id: &str) -> Result<(), KbStoreError> {
        let now = chrono::Utc::now().to_rfc3339();

        let result = sqlx::query!(
            r#"UPDATE kb_key_blocks SET status = 'deleted', updated_at = ?
               WHERE key_block_id = ?"#,
            now,
            key_block_id,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?;

        if result.rows_affected() == 0 {
            return Err(KbStoreError::NotFound(key_block_id.to_string()));
        }

        Ok(())
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_local_db::{open_pool, run_migrations};

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_world(pool: &SqlitePool) {
        // Seed creator + world for FK satisfaction
        sqlx::query!(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')"
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query!(
            r#"INSERT INTO narrative_worlds
                (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json)
               VALUES ('wld_1', 'wrk_test', 'ctr_test', 'Test World', 'test-world', 'active', 'private', 'manual', '{}')"#
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_insert_and_get() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KeyBlock::new("wld_1", BlockType::Character, "Hero");

        let result = store.insert_key_block(kb.clone()).await.unwrap();
        assert_eq!(result.key_block_id, kb.key_block_id);

        let fetched = store.get_key_block(&kb.key_block_id).await.unwrap();
        assert_eq!(fetched.canonical_name, "Hero");
        assert_eq!(fetched.world_id, "wld_1");
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let (pool, _dir) = fresh_pool().await;
        let store = SqliteKbStore::new(pool);
        let err = store.get_key_block("kb_nonexistent").await.unwrap_err();
        assert!(matches!(err, KbStoreError::NotFound(ref s) if s == "kb_nonexistent"));
    }

    #[tokio::test]
    async fn test_list_by_world() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb1 = KeyBlock::new("wld_1", BlockType::Character, "Hero");
        let kb2 = KeyBlock::new("wld_1", BlockType::Scene, "Forest");
        store.insert_key_block(kb1).await.unwrap();
        store.insert_key_block(kb2).await.unwrap();

        let items = store.list_by_world("wld_1").await.unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn test_list_excludes_deleted() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KeyBlock::new("wld_1", BlockType::Character, "Hero");
        let id = kb.key_block_id.clone();
        store.insert_key_block(kb).await.unwrap();

        store.delete_key_block(&id).await.unwrap();

        let items = store.list_by_world("wld_1").await.unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn test_uniqueness_rejects_duplicate() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb1 = KeyBlock::new("wld_1", BlockType::Character, "Hero");
        store.insert_key_block(kb1).await.unwrap();

        let kb2 = KeyBlock::new("wld_1", BlockType::Character, "Hero");
        let err = store.insert_key_block(kb2).await.unwrap_err();
        assert!(matches!(err, KbStoreError::Duplicate { .. }));
    }

    #[tokio::test]
    async fn test_attach_and_get_anchors() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KeyBlock::new("wld_1", BlockType::Character, "Hero");
        let id = kb.key_block_id.clone();
        store.insert_key_block(kb).await.unwrap();

        let anchor = SourceAnchor::new("stm_1", "sum_1", None);
        store.attach_source_anchor(&id, anchor).await.unwrap();

        let anchors = store.get_anchors(&id).await.unwrap();
        assert_eq!(anchors.len(), 1);
    }

    #[tokio::test]
    async fn test_update_key_block() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let mut kb = KeyBlock::new("wld_1", BlockType::Character, "Hero");
        let id = kb.key_block_id.clone();
        store.insert_key_block(kb.clone()).await.unwrap();

        kb.canonical_name = "Superhero".to_string();
        kb.updated_at = Some(chrono::Utc::now().to_rfc3339());
        store.update_key_block(kb).await.unwrap();

        let fetched = store.get_key_block(&id).await.unwrap();
        assert_eq!(fetched.canonical_name, "Superhero");
    }

    #[tokio::test]
    async fn test_delete_key_block() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KeyBlock::new("wld_1", BlockType::Character, "Hero");
        let id = kb.key_block_id.clone();
        store.insert_key_block(kb).await.unwrap();

        store.delete_key_block(&id).await.unwrap();

        // Block still exists but marked deleted
        let fetched = store.get_key_block(&id).await.unwrap();
        assert_eq!(fetched.status, "deleted");
    }

    #[tokio::test]
    async fn test_deleted_allows_reinsertion() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KeyBlock::new("wld_1", BlockType::Character, "Hero");
        let id = kb.key_block_id.clone();
        store.insert_key_block(kb).await.unwrap();

        store.delete_key_block(&id).await.unwrap();

        // Re-insert with same canonical_name + type should succeed
        let kb2 = KeyBlock::new("wld_1", BlockType::Character, "Hero");
        assert!(store.insert_key_block(kb2).await.is_ok());
    }

    #[tokio::test]
    async fn test_query_with_block_type() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        store
            .insert_key_block(KeyBlock::new("wld_1", BlockType::Character, "Hero"))
            .await
            .unwrap();
        store
            .insert_key_block(KeyBlock::new("wld_1", BlockType::Scene, "Forest"))
            .await
            .unwrap();
        store
            .insert_key_block(KeyBlock::new("wld_1", BlockType::Character, "Villain"))
            .await
            .unwrap();

        let result = store
            .query(&KbQuery::new("wld_1").with_block_type(BlockType::Character))
            .await
            .unwrap();
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.total_count, 2);
    }

    #[tokio::test]
    async fn test_query_world_isolation() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        // Seed second world
        sqlx::query!(
            r#"INSERT INTO narrative_worlds
                (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json)
               VALUES ('wld_2', 'wrk_test', 'ctr_test', 'World Two', 'world-two', 'active', 'private', 'manual', '{}')"#
        )
        .execute(&pool)
        .await
        .unwrap();

        let store = SqliteKbStore::new(pool);
        store
            .insert_key_block(KeyBlock::new("wld_1", BlockType::Character, "Hero"))
            .await
            .unwrap();

        let result = store.query(&KbQuery::new("wld_2")).await.unwrap();
        assert!(result.items.is_empty());
    }
}
