//! SQLite-backed `KbStore` implementation.
//!
//! Implements the `KbStore` trait from `nexus-kb` using the workspace
//! `state.db` pool. Uses compile-time checked `sqlx` queries for all
//! static SQL.
//!
//! # Validation
//!
//! `SqliteKbStore` runs body validation on insert and update when a
//! [`ValidationMode`](nexus_kb::validation::ValidationMode) is configured.
//! The default mode is `Generic` (no novel-specific checks). Set
//! `validation_mode` to [`ValidationMode::Novel`] to enforce
//! `body.attributes.novel_category` requirements per entity-scope-model.md §5.1.1.
//!
//! # Test helpers
//!
//! The [`seed`] submodule provides async functions to insert test data
//! (key blocks, source anchors) into the database for integration tests.

use nexus_contracts::BlockType;
use nexus_kb::errors::ValidationError;
use nexus_kb::key_block::{KeyBlock, KeyBlockBody};
use nexus_kb::query::{KbInsertResult, KbQuery, KbQueryResult};
use nexus_kb::source_anchor::SourceAnchor;
use nexus_kb::store::KbStoreError;
use nexus_kb::validation::{validate_body, validate_canonical_name, ValidationMode};
use nexus_kb::KbStore;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::LocalDbError;

/// Test helpers for seeding KB data into the database.
///
/// These functions are intended for tests and development fixtures only.
/// They create the necessary FK parent rows (e.g. creators, worlds) if missing.
pub mod seed {
    use super::super::seed_shared;
    use sqlx::SqlitePool;

    /// Seed a test world row (also seeds a minimal creator for FK).
    ///
    /// Delegates to the shared `seed_shared::world` helper.
    pub async fn world(
        pool: &SqlitePool,
        world_id: &str,
        owner_creator_id: &str,
        title: &str,
        slug: &str,
        visibility: &str,
        time_policy: &str,
    ) {
        seed_shared::world(
            pool,
            world_id,
            owner_creator_id,
            title,
            slug,
            visibility,
            time_policy,
        )
        .await;
    }

    /// Seed a test key block row into `kb_key_blocks`.
    ///
    /// # Panics
    ///
    /// Panics if the database insert fails (e.g., FK violation).
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

/// Maximum number of key blocks returned by `list_by_world` (safety cap, R9).
///
/// Prevents unbounded memory usage on large worlds. The `query()` method applies
/// its own pagination on top of this.
const LIST_BY_WORLD_LIMIT: i64 = 500;

/// SQLite-backed KB store.
///
/// Holds an `Arc<SqlitePool>` shared per active workspace. Construct once
/// at daemon/CLI boot and inject as `Arc<dyn KbStore>`.
pub struct SqliteKbStore {
    pool: Arc<SqlitePool>,
    validation_mode: ValidationMode,
}

impl SqliteKbStore {
    /// Create a new store backed by the given pool with `Generic` validation.
    ///
    /// The pool is wrapped in `Arc` for cheap cloning if needed.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool: Arc::new(pool),
            validation_mode: ValidationMode::Generic,
        }
    }

    /// Create a new store backed by the given pool with the given validation mode.
    #[must_use]
    pub fn with_validation_mode(pool: SqlitePool, mode: ValidationMode) -> Self {
        Self {
            pool: Arc::new(pool),
            validation_mode: mode,
        }
    }

    /// Transaction-aware variant of [`KbStore::insert_key_block`] (R-V150KBED-03).
    ///
    /// Runs the same `canonical_name` + body validation as the trait method and
    /// issues the same INSERT, but against a caller-managed transaction so the
    /// `creator world kb adopt` path can wrap insert + promotion flip atomically.
    /// If the caller rolls back the transaction (or drops it without commit),
    /// neither the `KeyBlock` row nor any sibling writes in the same tx persist.
    ///
    /// **Keep in sync with `KbStore::insert_key_block`** (the trait impl on this
    /// type): validation, serialization, and the INSERT statement must stay
    /// identical. Both paths use `ValidationMode::Novel` for the adopt path.
    ///
    /// # Errors
    ///
    /// Returns [`KbStoreError::Validation`] / [`KbStoreError::ValidationLegacy`]
    /// on `canonical_name` or body validation failure, [`KbStoreError::Duplicate`]
    /// on the `kb_key_blocks_active_unique` violation, or [`KbStoreError::Storage`]
    /// on database failure.
    pub async fn insert_key_block_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        kb: KeyBlock,
    ) -> Result<KbInsertResult, KbStoreError> {
        // Validate canonical_name format/safety (same as trait impl).
        validate_canonical_name(&kb.canonical_name).map_err(validation_err)?;

        // Validate body semantics before persisting (same as trait impl).
        validate_body(kb.block_type, kb.body.as_ref(), self.validation_mode)
            .map_err(validation_err)?;

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
        // Stable snake_case serialization matching wire format (not Debug)
        let block_type_str = serde_json::to_string(&kb.block_type)
            .unwrap_or_else(|_| format!("{:?}", kb.block_type));
        // Strip surrounding quotes from serde_json string output
        let block_type_str = block_type_str.trim_matches('"').to_string();
        let revision_i64 = kb.revision.map(u64::cast_signed);

        // V1.52 T-A P2: provenance columns are new; sqlx compile-time
        // verification can't resolve them until migration is applied.
        // SAFETY: static SQL with vetted column names from migration
        // 202606190003_kb_key_blocks_provenance.sql.
        let wld_id = kb.world_id.clone();
        let cname = kb.canonical_name.clone();
        let btype = kb.block_type;
        sqlx::query(
            r"INSERT INTO kb_key_blocks
                (key_block_id, world_id, block_type, canonical_name, status, revision,
                 body_json, source_anchor_json, created_from_command_id, created_at, updated_at,
                 source_work_id, source_chapter, source_provenance_kind)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&key_block_id)
        .bind(&wld_id)
        .bind(&block_type_str)
        .bind(&cname)
        .bind(&kb.status)
        .bind(revision_i64)
        .bind(&body_json)
        .bind(&source_anchor_json)
        .bind(&kb.created_from_command_id)
        .bind(&kb.created_at)
        .bind(&kb.updated_at)
        .bind(&kb.source_work_id)
        .bind(kb.source_chapter)
        .bind(&kb.source_provenance_kind)
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            // SQLite UNIQUE constraint violation
            if let sqlx::Error::Database(ref db_err_inner) = e {
                if db_err_inner.code().as_deref() == Some("2067") {
                    return KbStoreError::Duplicate {
                        world_id: wld_id,
                        name: cname,
                        block_type: btype,
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
    // V1.52 T-A P2: Work→KeyBlock provenance columns
    source_work_id: Option<String>,
    source_chapter: Option<i64>,
    source_provenance_kind: Option<String>,
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
            source_work_id: self.source_work_id.clone(),
            source_chapter: self.source_chapter,
            source_provenance_kind: self.source_provenance_kind.clone(),
        })
    }
}

/// Parse a `block_type` string into `BlockType`.
///
/// Accepts both `snake_case` (wire format via serde) and `PascalCase` (legacy DB).
fn parse_block_type(s: &str) -> Result<BlockType, KbStoreError> {
    // Try serde (snake_case) first — matches wire format
    if let Ok(bt) = serde_json::from_value::<BlockType>(serde_json::Value::String(s.to_string())) {
        return Ok(bt);
    }
    // Fallback: legacy PascalCase stored by prior versions via Debug format
    match s {
        "Character" => Ok(BlockType::Character),
        "Ability" => Ok(BlockType::Ability),
        "Scene" => Ok(BlockType::Scene),
        "Organization" => Ok(BlockType::Organization),
        "Item" => Ok(BlockType::Item),
        "Conflict" => Ok(BlockType::Conflict),
        "InfoPoint" => Ok(BlockType::InfoPoint),
        "Event" => Ok(BlockType::Event),
        // V1.54 P1: game-bible BlockType variants (legacy PascalCase fallback)
        "Species" => Ok(BlockType::Species),
        "Faction" => Ok(BlockType::Faction),
        "MagicSystem" => Ok(BlockType::MagicSystem),
        "Technology" => Ok(BlockType::Technology),
        "Deity" => Ok(BlockType::Deity),
        "Level" => Ok(BlockType::Level),
        "EconomyTier" => Ok(BlockType::EconomyTier),
        _ => Err(KbStoreError::Storage(format!("unknown block_type: {s}"))),
    }
}

/// Convert a sqlx error into a `KbStoreError`.
fn db_err(err: &sqlx::Error) -> KbStoreError {
    KbStoreError::Storage(format!("database error: {err}"))
}

/// Convert a `KbError` from validation into a `KbStoreError`.
fn validation_err(e: nexus_kb::KbError) -> KbStoreError {
    match e {
        nexus_kb::KbError::Validation(ve) => KbStoreError::Validation(ve),
        nexus_kb::KbError::ValidationError(msg) => KbStoreError::Validation(ValidationError {
            kind: nexus_kb::ValidationKind::MissingBody,
            field: None,
            message: msg,
        }),
        other => KbStoreError::Validation(ValidationError {
            kind: nexus_kb::ValidationKind::MissingBody,
            field: None,
            message: other.to_string(),
        }),
    }
}

// SAFETY: sqlx SQLite futures borrow the connection pool internally;
// safe for single-threaded SQLite usage within our tokio runtime.
#[allow(clippy::future_not_send)]
impl KbStore for SqliteKbStore {
    async fn insert_key_block(&self, kb: KeyBlock) -> Result<KbInsertResult, KbStoreError> {
        // Validate canonical_name format/safety
        validate_canonical_name(&kb.canonical_name).map_err(validation_err)?;

        // Validate body semantics before persisting
        validate_body(kb.block_type, kb.body.as_ref(), self.validation_mode)
            .map_err(validation_err)?;

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
        // Stable snake_case serialization matching wire format (not Debug)
        let block_type_str = serde_json::to_string(&kb.block_type)
            .unwrap_or_else(|_| format!("{:?}", kb.block_type));
        // Strip surrounding quotes from serde_json string output
        let block_type_str = block_type_str.trim_matches('"').to_string();
        let revision_i64 = kb.revision.map(u64::cast_signed);

        // SAFETY: static SQL with vetted column names from migration
        // 202606190003_kb_key_blocks_provenance.sql. Runtime query used
        // because new provenance columns are unknown to sqlx offline mode.
        let wld_id = kb.world_id.clone();
        let cname = kb.canonical_name.clone();
        let btype = kb.block_type;
        sqlx::query(
            r"INSERT INTO kb_key_blocks
                (key_block_id, world_id, block_type, canonical_name, status, revision,
                 body_json, source_anchor_json, created_from_command_id, created_at, updated_at,
                 source_work_id, source_chapter, source_provenance_kind)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&key_block_id)
        .bind(&wld_id)
        .bind(&block_type_str)
        .bind(&cname)
        .bind(&kb.status)
        .bind(revision_i64)
        .bind(&body_json)
        .bind(&source_anchor_json)
        .bind(&kb.created_from_command_id)
        .bind(&kb.created_at)
        .bind(&kb.updated_at)
        .bind(&kb.source_work_id)
        .bind(kb.source_chapter)
        .bind(&kb.source_provenance_kind)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            // SQLite UNIQUE constraint violation
            if let sqlx::Error::Database(ref db_err_inner) = e {
                if db_err_inner.code().as_deref() == Some("2067") {
                    return KbStoreError::Duplicate {
                        world_id: wld_id,
                        name: cname,
                        block_type: btype,
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
        // SAFETY: runtime query because new provenance columns are unknown
        // to sqlx offline mode until migration 202606190003 is applied.
        let row = sqlx::query_as::<_, KeyBlockRow>(
            r"SELECT
                key_block_id, world_id, block_type, canonical_name, status,
                revision, body_json, source_anchor_json, created_from_command_id,
                created_at, updated_at, source_work_id, source_chapter,
                source_provenance_kind
            FROM kb_key_blocks
            WHERE key_block_id = ?",
        )
        .bind(key_block_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?
        .ok_or_else(|| KbStoreError::NotFound(key_block_id.to_string()))?;

        row.to_key_block()
    }

    async fn list_by_world(&self, world_id: &str) -> Result<Vec<KeyBlock>, KbStoreError> {
        // SAFETY: LIMIT is a compile-time constant; dynamic SQL needed because
        // sqlx::query_as! does not support LIMIT as bind param in SQLite offline mode.
        let rows = sqlx::query_as::<_, KeyBlockRow>(&format!(
            r"SELECT
                key_block_id,
                world_id,
                block_type,
                canonical_name,
                status,
                revision,
                body_json,
                source_anchor_json,
                created_from_command_id,
                created_at,
                updated_at,
                source_work_id,
                source_chapter,
                source_provenance_kind
            FROM kb_key_blocks
            WHERE world_id = ?
              AND status NOT IN ('deleted', 'merged', 'deprecated')
            ORDER BY created_at ASC
            LIMIT {LIST_BY_WORLD_LIMIT}"
        ))
        .bind(world_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?;

        rows.iter().map(KeyBlockRow::to_key_block).collect()
    }

    async fn query(&self, query: &KbQuery) -> Result<KbQueryResult, KbStoreError> {
        // Strategy: fetch all active blocks for the world, then apply
        // optional filters in-memory. This avoids complex dynamic SQL
        // and is efficient for per-world datasets (typically small).
        //
        // ## body_json growth and computable indexing (R-V161P0-LOW-004)
        //
        // Computable KeyBlocks (V1.61) embed `state` (dynamic runtime) and
        // `attributes` (immutable compute params) inside `body_json`. For
        // character KeyBlocks this can add several KiB of structured JSON
        // per block — the `body_json` TEXT column may grow with compute
        // usage over time.
        //
        // The `computable` filter is applied in-memory after `list_by_world`
        // (consistent with all other query filters). If per-world KeyBlock
        // counts grow to thousands, a SQLite expression index on
        // `json_extract(body_json, '$.computable')` would accelerate the
        // filter at the storage layer:
        //
        // ```sql
        // CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_computable
        //   ON kb_key_blocks(json_extract(body_json, '$.computable'));
        // ```
        //
        // This is deferred to a future iteration — V1.61 worlds are small
        // enough that in-memory filtering is sufficient. No migration needed.
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
                // V1.61 P1: filter by computable flag
                if let Some(want) = query.computable {
                    let is_computable =
                        kb.body.as_ref().and_then(|b| b.computable).unwrap_or(false);
                    if is_computable != want {
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
        // Validate canonical_name format/safety
        validate_canonical_name(&kb.canonical_name).map_err(validation_err)?;

        // Validate body semantics before persisting
        validate_body(kb.block_type, kb.body.as_ref(), self.validation_mode)
            .map_err(validation_err)?;

        // Verify exists
        let existing = self.get_key_block(&kb.key_block_id).await?;

        // If name or type changed, check uniqueness
        if existing.canonical_name != kb.canonical_name || existing.block_type != kb.block_type {
            // Stable snake_case serialization matching wire format
            let block_type_str = serde_json::to_string(&kb.block_type)
                .unwrap_or_else(|_| format!("{:?}", kb.block_type));
            let block_type_str = block_type_str.trim_matches('"').to_string();
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
        // Stable snake_case serialization matching wire format (not Debug)
        let block_type_str = serde_json::to_string(&kb.block_type)
            .unwrap_or_else(|_| format!("{:?}", kb.block_type));
        let block_type_str = block_type_str.trim_matches('"').to_string();
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

// ── V1.73 Canvas World KB: per-row OCC CAS entity edit ──────────────────────

/// V1.73 P0: CAS-aware partial update of a `kb_key_blocks` row.
///
/// Mirrors the V1.51 `kb_extract_jobs` CAS pattern
/// ([`kb_extract_job::mark_confirmed_in_tx_with_cas`]). Adds a
/// `WHERE key_block_id = ? AND revision = ?` guard so a stale preimage
/// (read before another writer modified the row) is rejected with
/// [`LocalDbError::VersionMismatch`]. On success the `revision` column is
/// bumped to `expected_revision + 1` and the bumped value is returned.
///
/// Only the fields supplied as `Some(..)` are mutated; `None` fields keep
/// their current DB value. `revision` is NULL-normalized to 0 by this
/// function (the architect Phase 2b lock: existing rows may have
/// `revision = NULL`; the first successful patch sets it to 1).
///
/// # Arguments
///
/// - `tx` — caller-owned transaction (so the entity edit can be composed
///   atomically with sibling writes if needed).
/// - `key_block_id` — target row PK.
/// - `canonical_name` / `block_type` / `body_json` — optional replacement
///   values (JSON strings for `body_json`).
/// - `expected_revision` — the per-row version the caller observed on read
///   (NULL-normalized to 0; this is the OCC precondition).
///
/// # Returns
///
/// - `Ok(new_revision)` — row updated, returns the new bumped version.
/// - `Err(LocalDbError::VersionMismatch)` — the row's `revision` changed
///   between read and UPDATE (409 caller-side).
/// - `Err(LocalDbError::VersionMismatch { actual: None })` — row not found.
/// - `Err(LocalDbError::Sqlx)` — database failure.
///
/// # Errors
///
/// See above.
pub async fn cas_update_key_block_fields(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key_block_id: &str,
    canonical_name: Option<&str>,
    block_type: Option<&str>,
    body_json: Option<&str>,
    expected_revision: i64,
) -> Result<u64, LocalDbError> {
    // Build a dynamic SET clause from the supplied fields. revision is always
    // bumped; updated_at always set. SAFETY: dynamic SET built from a fixed
    // field whitelist (not user-controlled SQL); all values are bind params.
    let mut sets = vec![
        "revision = ?".to_string(),
        "updated_at = ?".to_string(),
    ];
    if canonical_name.is_some() {
        sets.push("canonical_name = ?".to_string());
    }
    if block_type.is_some() {
        sets.push("block_type = ?".to_string());
    }
    if body_json.is_some() {
        sets.push("body_json = ?".to_string());
    }
    let set_clause = sets.join(", ");
    let now = chrono::Utc::now().to_rfc3339();
    let new_revision = expected_revision + 1;
    // SAFETY: dynamic SET built from a fixed field whitelist (not user-
    // controlled SQL); all values are bind params. revision IS ? matches
    // NULL revisions too (NULL-normalized to 0 on read by the caller).
    let sql = format!(
        "UPDATE kb_key_blocks SET {set_clause} \
         WHERE key_block_id = ? AND revision IS ?"
    );

    let mut q = sqlx::query(&sql);
    q = q.bind(new_revision).bind(now);
    if let Some(v) = canonical_name {
        q = q.bind(v);
    }
    if let Some(v) = block_type {
        q = q.bind(v);
    }
    if let Some(v) = body_json {
        q = q.bind(v);
    }
    q = q.bind(key_block_id).bind(expected_revision);
    let result = q.execute(&mut **tx).await?;

    if result.rows_affected() == 1 {
        return Ok(u64::try_from(new_revision).unwrap_or(0));
    }

    // rows_affected == 0 — disambiguate not-found vs version mismatch by
    // re-reading the row. NULL revision is treated as 0.
    let current: Option<Option<i64>> =
        sqlx::query_scalar("SELECT revision FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(key_block_id)
            .fetch_optional(&mut **tx)
            .await?;
    let actual = current.map(|rev| rev.unwrap_or(0));
    Err(LocalDbError::VersionMismatch {
        table: "kb_key_blocks".to_string(),
        id: key_block_id.to_string(),
        expected: expected_revision,
        actual,
    })
}

/// V1.73 P0: read the per-row OCC version of a `kb_key_blocks` row,
/// NULL-normalized to 0. Returns `None` when the row does not exist.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn read_key_block_revision(
    pool: &SqlitePool,
    key_block_id: &str,
) -> Result<Option<u64>, LocalDbError> {
    // SAFETY: static SELECT by PK with bind param.
    let row: Option<Option<i64>> =
        sqlx::query_scalar("SELECT revision FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(key_block_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|rev| rev.unwrap_or(0).max(0).cast_unsigned()))
}

// ── Tests ───────────────────────────────────────────────────────────

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

    // ── Validation tests (QC1 C-001 / QC2 C1 + QC2 W2 + QC2 W3) ──

    fn make_novel_block_sql(
        world_id: &str,
        block_type: BlockType,
        name: &str,
        novel_category: &str,
    ) -> KeyBlock {
        let mut kb = KeyBlock::new(world_id, block_type, name);
        kb.body = Some(KeyBlockBody {
            summary: Some(format!("{novel_category}: {name}")),
            attributes: Some(serde_json::json!({
                "novel_category": novel_category,
                "traits": ["test"]
            })),
            tags: Some(vec!["novel".to_string()]),
            ..Default::default()
        });
        kb
    }

    #[tokio::test]
    async fn test_sqlite_novel_valid_category_succeeds() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::with_validation_mode(pool, ValidationMode::Novel);
        let kb = make_novel_block_sql("wld_1", BlockType::Character, "char_lin_xia", "character");
        let result = store.insert_key_block(kb).await;
        assert!(result.is_ok(), "expected ok, got {:?}", result.unwrap_err());
    }

    #[tokio::test]
    async fn test_sqlite_novel_missing_category_rejected() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::with_validation_mode(pool, ValidationMode::Novel);
        let mut kb = KeyBlock::new("wld_1", BlockType::Character, "char_no_cat");
        kb.body = Some(KeyBlockBody {
            summary: Some("A character without category".to_string()),
            attributes: Some(serde_json::json!({"aliases": ["NoCat"]})),
            tags: Some(vec!["novel".to_string()]),
            ..Default::default()
        });

        let err = store.insert_key_block(kb).await.unwrap_err();
        match err {
            KbStoreError::Validation(ve) => {
                assert!(ve.message.contains("novel_category is required"));
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[tokio::test]
    async fn test_sqlite_novel_invalid_category_rejected() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::with_validation_mode(pool, ValidationMode::Novel);
        let kb = make_novel_block_sql("wld_1", BlockType::Character, "char_bad", "invalid_cat");
        let err = store.insert_key_block(kb).await.unwrap_err();
        match err {
            KbStoreError::Validation(ve) => {
                assert!(ve.message.contains("invalid novel_category"));
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[tokio::test]
    async fn test_sqlite_uniqueness_preserved_with_validation() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::with_validation_mode(pool, ValidationMode::Novel);
        let kb1 = make_novel_block_sql("wld_1", BlockType::Character, "char_dupe", "character");
        store.insert_key_block(kb1).await.unwrap();

        let kb2 = make_novel_block_sql("wld_1", BlockType::Character, "char_dupe", "character");
        let err = store.insert_key_block(kb2).await.unwrap_err();
        assert!(matches!(err, KbStoreError::Duplicate { .. }));
    }

    #[tokio::test]
    async fn test_sqlite_canonical_name_validation_rejects_slash() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KeyBlock::new("wld_1", BlockType::Character, "evil/../path");
        let err = store.insert_key_block(kb).await.unwrap_err();
        match err {
            KbStoreError::Validation(ve) => {
                assert!(ve.message.contains("forbidden character"));
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[tokio::test]
    async fn test_sqlite_canonical_name_validation_rejects_shell_meta() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KeyBlock::new("wld_1", BlockType::Character, "evil;rm -rf");
        let err = store.insert_key_block(kb).await.unwrap_err();
        match err {
            KbStoreError::Validation(ve) => {
                assert!(ve.message.contains("forbidden character"));
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[tokio::test]
    async fn test_sqlite_canonical_name_validation_rejects_empty() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let mut kb = KeyBlock::new("wld_1", BlockType::Character, "temp");
        kb.canonical_name = String::new();
        let err = store.insert_key_block(kb).await.unwrap_err();
        match err {
            KbStoreError::Validation(ve) => {
                assert!(ve.message.contains("must not be empty"));
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[tokio::test]
    async fn test_sqlite_generic_mode_accepts_body_without_novel_category() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool); // Generic mode by default
        let mut kb = KeyBlock::new("wld_1", BlockType::Character, "char_generic");
        kb.body = Some(KeyBlockBody {
            summary: Some("A generic character".to_string()),
            attributes: None,
            tags: None,
            ..Default::default()
        });
        assert!(store.insert_key_block(kb).await.is_ok());
    }

    #[tokio::test]
    async fn test_sqlite_update_validates_body_in_novel_mode() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::with_validation_mode(pool, ValidationMode::Novel);
        let kb = make_novel_block_sql("wld_1", BlockType::Character, "char_hero", "character");
        let mut kb = kb;
        store.insert_key_block(kb.clone()).await.unwrap();

        // Update to body missing novel_category should fail
        kb.body = Some(KeyBlockBody {
            summary: Some("updated".to_string()),
            attributes: Some(serde_json::json!({"traits": ["old"]})),
            tags: None,
            ..Default::default()
        });
        kb.updated_at = Some(chrono::Utc::now().to_rfc3339());

        let err = store.update_key_block(kb).await.unwrap_err();
        match err {
            KbStoreError::Validation(ve) => {
                assert!(ve.message.contains("novel_category is required"));
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[tokio::test]
    async fn test_sqlite_stores_block_type_snake_case() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KeyBlock::new("wld_1", BlockType::InfoPoint, "test_block");
        let id = kb.key_block_id.clone();
        store.insert_key_block(kb).await.unwrap();

        // Verify DB contains snake_case "info_point" (not Debug "InfoPoint")
        let row: (String,) =
            sqlx::query_as("SELECT block_type FROM kb_key_blocks WHERE key_block_id = ?")
                .bind(&id)
                .fetch_one(&*store.pool)
                .await
                .unwrap();
        assert_eq!(row.0, "info_point");
    }

    // ── Computable query filter (V1.61 P1) ─────────────────────────

    fn make_computable_kb(world_id: &str, name: &str, bt: BlockType, computable: bool) -> KeyBlock {
        let mut kb = KeyBlock::new(world_id, bt, name);
        kb.body = Some(KeyBlockBody {
            summary: Some(format!("{name} summary")),
            attributes: if computable {
                Some(serde_json::json!({"max_hp": 100}))
            } else {
                None
            },
            tags: None,
            computable: Some(computable),
            state: if computable {
                Some(serde_json::json!({"character": {"current_hp": 80}}))
            } else {
                None
            },
        });
        kb
    }

    #[tokio::test]
    async fn test_sqlite_query_computable_true() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool.clone());
        store
            .insert_key_block(make_computable_kb(
                "wld_1",
                "Hero",
                BlockType::Character,
                true,
            ))
            .await
            .unwrap();
        store
            .insert_key_block(make_computable_kb(
                "wld_1",
                "NPC",
                BlockType::Character,
                false,
            ))
            .await
            .unwrap();

        let q = KbQuery::new("wld_1").with_computable(Some(true));
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.items[0].canonical_name, "Hero");
        assert_eq!(
            result.items[0].body.as_ref().unwrap().computable,
            Some(true)
        );
    }

    #[tokio::test]
    async fn test_sqlite_query_computable_false() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool.clone());
        store
            .insert_key_block(make_computable_kb(
                "wld_1",
                "Hero",
                BlockType::Character,
                true,
            ))
            .await
            .unwrap();
        store
            .insert_key_block(make_computable_kb(
                "wld_1",
                "NPC",
                BlockType::Character,
                false,
            ))
            .await
            .unwrap();

        let q = KbQuery::new("wld_1").with_computable(Some(false));
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.items[0].canonical_name, "NPC");
    }

    #[tokio::test]
    async fn test_sqlite_query_computable_none_returns_all() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool.clone());
        store
            .insert_key_block(make_computable_kb(
                "wld_1",
                "Hero",
                BlockType::Character,
                true,
            ))
            .await
            .unwrap();
        store
            .insert_key_block(make_computable_kb(
                "wld_1",
                "NPC",
                BlockType::Character,
                false,
            ))
            .await
            .unwrap();

        // No computable filter → should return both
        let q = KbQuery::new("wld_1");
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 2);
    }

    #[tokio::test]
    async fn test_sqlite_query_computable_legacy_block() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool.clone());
        // Legacy block with no computable field
        let mut kb = KeyBlock::new("wld_1", BlockType::Character, "Legacy");
        kb.body = Some(KeyBlockBody {
            summary: Some("legacy".to_string()),
            attributes: None,
            tags: None,
            ..Default::default()
        });
        store.insert_key_block(kb).await.unwrap();

        // computable=true should exclude it
        let q = KbQuery::new("wld_1").with_computable(Some(true));
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 0);

        // computable=false should include it
        let q = KbQuery::new("wld_1").with_computable(Some(false));
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 1);
    }
}
