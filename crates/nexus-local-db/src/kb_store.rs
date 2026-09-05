//! SQLite-backed `KbStore` implementation.
//!
//! Implements the `KbStore` trait from `nexus-kb` using the workspace
//! `state.db` pool. Uses compile-time checked `sqlx` queries for all
//! static SQL.
//!
//! # Validation
//!
//! `SqliteKbStore` runs body validation on insert and update when a
//! [`ValidationMode`](nexus_knowledge::world_kb::validation::ValidationMode) is configured.
//! The default mode is `Generic` (no novel-specific checks). Set
//! `validation_mode` to [`ValidationMode::Novel`] to enforce
//! `body.attributes.novel_category` requirements per entity-scope-model.md §5.1.1.
//!
//! # Test helpers
//!
//! The [`seed`] submodule provides async functions to insert test data
//! (key blocks, source anchors) into the database for integration tests.

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::errors::ValidationError;
use nexus_knowledge::world_kb::knowledge_entry::{
    KnowledgeEntryBody, KnowledgeEntryRecord, KnowledgeOwnerRef,
};
use nexus_knowledge::world_kb::query::{KbInsertResult, KbQuery, KbQueryResult};
use nexus_knowledge::world_kb::source_anchor::SourceAnchor;
use nexus_knowledge::world_kb::store::KbStoreError;
use nexus_knowledge::world_kb::validation::{
    validate_body, validate_canonical_name, ValidationMode,
};
use nexus_knowledge::world_kb::KbStore;
// V1.145 P1b — `build_extensions_nexus` + `is_known_nexus_key` inlined as
// private local fns so `nexus-local-db` no longer depends on
// `nexus-spoke-adapter` (spec §8 dep-graph reversal). The production adapter
// (now in `nexus-spoke-adapter`) still calls the spoke-adapter
// `build_extensions_nexus` on its own write path; this local copy keeps the
// storage layer's own INSERT/UPDATE legacy wrappers spoke-unaware. The two
// implementations are behavior-equivalent (same 5 typed keys, same round-trip).
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;

use crate::LocalDbError;

/// Wire-neutral namespace map carrying an entry's `extensions.nexus` payload.
///
/// Local stand-in for spoke's `ExtensionMap` (which lives in
/// `spoke-operations`). The storage layer only needs the `"nexus"` namespace
/// key, so a plain `HashMap<String, Map<String, Value>>` is sufficient —
/// `nexus-local-db` no longer depends on `nexus-spoke-adapter` (V1.145 P1b,
/// spec §8).
type ExtensionMap = HashMap<String, serde_json::Map<String, serde_json::Value>>;

/// The 8 typed identity/owner field names carried under `extensions.nexus`.
///
/// Mirror of `nexus_spoke_adapter::extensions::KNOWN_NEXUS_KEYS`. Inlined here
/// so the storage layer can separate authoritative typed columns from
/// verbatim-carried extras without a spoke-adapter dep (V1.145 P1b). v1.184
/// P1 adds the canonical owner keys (`character_id`, `actor_world_binding_id`)
/// and the `creator_only` flag so non-World owners never fabricate a
/// `world_id`.
const KNOWN_NEXUS_KEYS: [&str; 8] = [
    "world_id",
    "character_id",
    "actor_world_binding_id",
    "creator_only",
    "created_from_command_id",
    "source_work_id",
    "source_chapter",
    "source_provenance_kind",
];

/// Returns `true` if `key` is one of the 8 typed `extensions.nexus` identity
/// / owner fields. Local mirror of `nexus_spoke_adapter::extensions::is_known_nexus_key`
/// (spec §2.2 round-trip rule 2).
fn is_known_nexus_key(key: &str) -> bool {
    KNOWN_NEXUS_KEYS.contains(&key)
}

/// Build the `extensions.nexus` namespace object from typed nexus fields.
///
/// Behavior-equivalent local copy of
/// `nexus_spoke_adapter::extensions::build_extensions_nexus` (spec §2.3 write
/// path). The canonical owner is written from [`KnowledgeOwnerRef`]: World
/// owners emit `world_id` (required pre-v1.184 key), Character owners emit
/// `character_id`, and binding owners emit `actor_world_binding_id` — a
/// non-World owner never carries a `world_id` key (no fabricated World id).
/// Each optional provenance field is inserted when `Some`, removed when
/// `None`. Unknown keys already present under the `"nexus"` namespace of
/// `existing_extensions` are preserved verbatim (spec §2.2 round-trip rule 2).
fn build_extensions_nexus(
    owner: &KnowledgeOwnerRef,
    creator_only: bool,
    created_from_command_id: Option<&str>,
    source_work_id: Option<&str>,
    source_chapter: Option<i64>,
    source_provenance_kind: Option<&str>,
    existing_extensions: &ExtensionMap,
) -> serde_json::Value {
    let mut nexus = existing_extensions
        .get("nexus")
        .cloned()
        .unwrap_or_default();

    // Canonical owner keys: exactly the owner's key is set; the other two
    // (and their stale extras) are removed so the projection is unambiguous.
    nexus.remove("world_id");
    nexus.remove("character_id");
    nexus.remove("actor_world_binding_id");
    if let Some(world_id) = owner.world_id() {
        nexus.insert("world_id".into(), serde_json::Value::String(world_id.to_owned()));
    } else if let Some(character_id) = owner.character_id() {
        nexus.insert(
            "character_id".into(),
            serde_json::Value::String(character_id.to_owned()),
        );
    } else if let Some(binding_id) = owner.actor_world_binding_id() {
        nexus.insert(
            "actor_world_binding_id".into(),
            serde_json::Value::String(binding_id.to_owned()),
        );
    }

    // `creator_only` is World-owned only (DB CHECK); round-trip as Nexus
    // metadata when set, otherwise the key is absent.
    if creator_only {
        nexus.insert("creator_only".into(), serde_json::Value::Bool(true));
    } else {
        nexus.remove("creator_only");
    }

    insert_opt_string(
        &mut nexus,
        "created_from_command_id",
        created_from_command_id,
    );
    insert_opt_string(&mut nexus, "source_work_id", source_work_id);
    insert_opt_i64(&mut nexus, "source_chapter", source_chapter);
    insert_opt_string(&mut nexus, "source_provenance_kind", source_provenance_kind);

    serde_json::Value::Object(nexus)
}

/// Insert a string field when `Some(value)`, remove it when `None`.
fn insert_opt_string(
    nexus: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<&str>,
) {
    match value {
        Some(v) => nexus.insert(key.into(), serde_json::Value::String(v.to_owned())),
        None => nexus.remove(key),
    };
}

/// Insert an integer field when `Some(value)`, remove it when `None`.
fn insert_opt_i64(
    nexus: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<i64>,
) {
    match value {
        Some(v) => nexus.insert(key.into(), serde_json::Value::Number(v.into())),
        None => nexus.remove(key),
    };
}

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
    pub async fn knowledge_entry(
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
pub const LIST_BY_WORLD_LIMIT: i64 = 500;

/// Result of [`SqliteKbStore::list_by_world_scoped`].
#[derive(Debug, Clone)]
pub struct WorldKbScopedList {
    /// Active entries matching the scope filters.
    pub entries: Vec<KnowledgeEntryRecord>,
    /// `true` when an unfiltered world listing exceeded [`LIST_BY_WORLD_LIMIT`].
    pub truncated: bool,
}

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

    /// Fetch the active [`KnowledgeEntryRecord`] for a world's unique
    /// `(block_type, canonical_name)` key.
    ///
    /// Uses the `idx_kb_key_blocks_active_unique` partial index directly —
    /// unlike [`KbStore::list_by_world`], this is not subject to the
    /// `LIST_BY_WORLD_LIMIT` window.
    ///
    /// # Errors
    ///
    /// Returns [`KbStoreError::Storage`] on database failure.
    pub async fn get_active_by_unique_key(
        &self,
        world_id: &str,
        canonical_name: &str,
        block_type: BlockType,
    ) -> Result<Option<KnowledgeEntryRecord>, KbStoreError> {
        let block_type_str = block_type_to_sql(block_type);
        // SAFETY: static SQL with vetted column names from migration
        // 202606190003_kb_key_blocks_provenance.sql. Runtime query used
        // because new provenance columns are unknown to sqlx offline mode.
        let row = sqlx::query_as::<_, KeyBlockRow>(
            r"SELECT
                key_block_id, owner_kind, world_id, character_id,
                actor_world_binding_id, creator_only,
                block_type, canonical_name, status,
                revision, body_json, source_anchor_json, created_from_command_id,
                created_at, updated_at, source_work_id, source_chapter,
                source_provenance_kind, extensions_nexus_json, modules_json
            FROM kb_key_blocks
            WHERE owner_kind = 'world'
              AND world_id = ?
              AND block_type = ?
              AND canonical_name = ?
              AND status NOT IN ('deleted', 'merged', 'deprecated')
            LIMIT 1",
        )
        .bind(world_id)
        .bind(&block_type_str)
        .bind(canonical_name)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?;

        row.as_ref().map(KeyBlockRow::to_record).transpose()
    }

    /// List active knowledge entries for a world with optional scope filters
    /// applied in SQL (V1.142 greploop R-V1142P2-002).
    ///
    /// When `entry_ids` and/or `entry_types` are non-empty, filters are
    /// pushed into the query so matching rows are not dropped by the
    /// `list_by_world` 500-row window. Unfiltered listings use
    /// `LIST_BY_WORLD_LIMIT + 1` to detect truncation; callers must reject
    /// when [`WorldKbScopedList::truncated`] is `true`.
    ///
    /// `entry_types` are spoke wire `entry_type` strings (`snake_case`), which
    /// match the `kb_key_blocks.block_type` column format.
    ///
    /// # Errors
    ///
    /// Returns [`KbStoreError::Storage`] on database failure.
    pub async fn list_by_world_scoped(
        &self,
        world_id: &str,
        entry_ids: &[String],
        entry_types: &[String],
    ) -> Result<WorldKbScopedList, KbStoreError> {
        let has_id_filter = !entry_ids.is_empty();
        let has_type_filter = !entry_types.is_empty();
        let unfiltered = !has_id_filter && !has_type_filter;

        let mut sql = String::from(
            r"SELECT
                key_block_id,
                owner_kind,
                world_id,
                character_id,
                actor_world_binding_id,
                creator_only,
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
                source_provenance_kind, extensions_nexus_json, modules_json
            FROM kb_key_blocks
            WHERE owner_kind = 'world'
              AND world_id = ?
              AND status NOT IN ('deleted', 'merged', 'deprecated')",
        );

        if has_id_filter {
            sql.push_str(" AND key_block_id IN (SELECT value FROM json_each(?))");
        }
        if has_type_filter {
            sql.push_str(" AND block_type IN (SELECT value FROM json_each(?))");
        }
        sql.push_str(" ORDER BY created_at ASC");
        if unfiltered {
            use std::fmt::Write;
            let _ = write!(sql, " LIMIT {}", LIST_BY_WORLD_LIMIT + 1);
        }

        // SAFETY: static column list; dynamic fragments are filter/limit clauses
        // with bind params only (no user-controlled SQL).
        let mut q = sqlx::query_as::<_, KeyBlockRow>(&sql).bind(world_id);
        if has_id_filter {
            q = q.bind(serde_json::to_string(entry_ids).unwrap_or_else(|_| "[]".to_string()));
        }
        if has_type_filter {
            q = q.bind(serde_json::to_string(entry_types).unwrap_or_else(|_| "[]".to_string()));
        }

        let rows = q.fetch_all(&*self.pool).await.map_err(|e| db_err(&e))?;

        let truncated =
            unfiltered && rows.len() > usize::try_from(LIST_BY_WORLD_LIMIT).unwrap_or(500);
        let kept = if truncated {
            &rows[..usize::try_from(LIST_BY_WORLD_LIMIT).unwrap_or(500)]
        } else {
            rows.as_slice()
        };

        let entries = kept
            .iter()
            .map(KeyBlockRow::to_record)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(WorldKbScopedList { entries, truncated })
    }

    /// Transaction-aware variant of [`KbStore::insert_knowledge_entry`] (R-V150KBED-03).
    ///
    /// Runs the same `canonical_name` + body validation as the trait method and
    /// issues the same INSERT, but against a caller-managed transaction so the
    /// `creator world kb adopt` path can wrap insert + promotion flip atomically.
    /// If the caller rolls back the transaction (or drops it without commit),
    /// neither the `KnowledgeEntryRecord` row nor any sibling writes in the same tx persist.
    ///
    /// **Keep in sync with `KbStore::insert_knowledge_entry`** (the trait impl on this
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
        kb: KnowledgeEntryRecord,
    ) -> Result<KbInsertResult, KbStoreError> {
        // V1.145 P0: legacy compat wrapper. Builds the `extensions.nexus` JSON
        // internally (keeps the `build_extensions_nexus` import) then delegates
        // to the spoke-unaware primitive [`insert_key_block_with_extensions_in_tx`].
        // Retained for the 5+ external callers (`apps/nexus42`,
        // `nexus-daemon-runtime`, `nexus-orchestration`) and the `KbStore` trait
        // impl. Residual R-V145P0-I1: this wrapper keeps the spoke-adapter import
        // until P1 moves the trait impls / external callers migrate to the
        // opaque primitive.
        let extensions_nexus_json = serde_json::to_string(&build_extensions_nexus(
            &kb.owner,
            kb.creator_only,
            kb.created_from_command_id.as_deref(),
            kb.source_work_id.as_deref(),
            kb.source_chapter,
            kb.source_provenance_kind.as_deref(),
            &nexus_extras_extension_map(kb.extensions_nexus_extras.as_ref()),
        ))
        .unwrap_or_default();
        self.insert_key_block_with_extensions_in_tx(tx, kb, extensions_nexus_json)
            .await
    }

    /// Insert a `kb_key_blocks` row from a **pre-built** opaque
    /// `extensions_nexus_json` string (V1.145 P0 T1 — spoke-unaware storage
    /// primitive, spec §7.4).
    ///
    /// This is the INSERT-side counterpart to
    /// [`update_key_block_auxiliary_fields_in_tx`]: the storage layer accepts
    /// the serialized `extensions.nexus` namespace as an opaque string and
    /// does **not** call `build_extensions_nexus`. The serialization boundary
    /// is owned by the caller (the spoke adapter port impl), matching the
    /// already-lifted UPDATE CAS path.
    ///
    /// Validation, column binding, and the `kb_key_blocks_active_unique`
    /// duplicate mapping are identical to [`insert_key_block_in_tx`]; the only
    /// difference is that the caller supplies `extensions_nexus_json`.
    ///
    /// # Errors
    ///
    /// Returns [`KbStoreError::Validation`] / [`KbStoreError::ValidationLegacy`]
    /// on `canonical_name` or body validation failure, [`KbStoreError::Duplicate`]
    /// on the `kb_key_blocks_active_unique` violation, or [`KbStoreError::Storage`]
    /// on database failure.
    pub async fn insert_key_block_with_extensions_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        kb: KnowledgeEntryRecord,
        extensions_nexus_json: String,
    ) -> Result<KbInsertResult, KbStoreError> {
        // Validate canonical_name format/safety (same as trait impl).
        validate_canonical_name(&kb.canonical_name).map_err(validation_err)?;

        // Validate body semantics before persisting (same as trait impl).
        validate_body(kb.block_type, kb.body.as_ref(), self.validation_mode)
            .map_err(validation_err)?;

        // v1.184 P1 fix: `creator_only` is World-only — the SQLite schema
        // CHECK is defense in depth, but the explicit check surfaces a
        // validation error and keeps the invariant identical across domain /
        // memory / conversion boundaries. Mapped to `ValidationLegacy` (not
        // the `validation_err` fallback, which would mislabel it `MissingBody`).
        nexus_knowledge::world_kb::knowledge_entry::validate_creator_only_owner(
            &kb.owner,
            kb.creator_only,
        )
        .map_err(|e| KbStoreError::ValidationLegacy(e.to_string()))?;

        let key_block_id = kb.entry_id.clone();
        let owner = kb.owner.clone();
        let created_at = kb.created_at.clone();

        let body_json = kb
            .body
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap_or_default());
        let source_anchor_json = kb
            .source_anchor
            .as_ref()
            .map(|a| serde_json::to_string(a).unwrap_or_default());
        // V1.146 P4 T1: serialize modules to JSON for the modules_json column.
        let modules_json = kb
            .modules
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());
        // Stable snake_case serialization matching wire format (not Debug)
        let block_type_str = serde_json::to_string(&kb.block_type)
            .unwrap_or_else(|_| format!("{:?}", kb.block_type));
        // Strip surrounding quotes from serde_json string output
        let block_type_str = block_type_str.trim_matches('"').to_string();
        let revision_i64 = kb.revision.map(u64::cast_signed);

        // V1.52 T-A P2 + v1.184 P1: provenance/owner columns are new; sqlx
        // compile-time verification can't resolve them until migration is
        // applied. SAFETY: static SQL with vetted column names; the owner
        // columns come from the closed [`KnowledgeOwnerRef`] (exactly one is
        // non-NULL, matching the migration's owner union CHECK).
        let owner_kind = owner.kind();
        let world_id_opt = owner.world_id();
        let character_id = owner.character_id();
        let actor_world_binding_id = owner.actor_world_binding_id();
        let creator_only_i64 = if kb.creator_only { 1 } else { 0 };
        let cname = kb.canonical_name.clone();
        let btype = kb.block_type;
        sqlx::query(
            r"INSERT INTO kb_key_blocks
                (key_block_id, owner_kind, world_id, character_id,
                 actor_world_binding_id, creator_only, block_type, canonical_name, status,
                 revision, body_json, source_anchor_json, created_from_command_id, created_at,
                 updated_at, source_work_id, source_chapter, source_provenance_kind,
                 extensions_nexus_json, modules_json)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&key_block_id)
        .bind(owner_kind)
        .bind(world_id_opt)
        .bind(character_id)
        .bind(actor_world_binding_id)
        .bind(creator_only_i64)
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
        .bind(&extensions_nexus_json)
        .bind(&modules_json)
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            // SQLite UNIQUE constraint violation (owner-scoped partial index).
            if let sqlx::Error::Database(ref db_err_inner) = e {
                if db_err_inner.code().as_deref() == Some("2067") {
                    return KbStoreError::Duplicate {
                        owner: owner.clone(),
                        name: cname,
                        block_type: btype,
                    };
                }
            }
            db_err(&e)
        })?;

        Ok(KbInsertResult {
            entry_id: key_block_id,
            owner,
            created_at,
        })
    }
}

// Row type matching the kb_key_blocks DDL.
#[derive(Debug, Clone, sqlx::FromRow)]
struct KeyBlockRow {
    key_block_id: String,
    // v1.184 P1 owner union — `world_id` is nullable now (non-World owners).
    owner_kind: String,
    world_id: Option<String>,
    character_id: Option<String>,
    actor_world_binding_id: Option<String>,
    creator_only: i64,
    block_type: String,
    canonical_name: String,
    status: String,
    revision: Option<i64>,
    body_json: Option<String>,
    source_anchor_json: Option<String>,
    created_from_command_id: Option<String>,
    created_at: String,
    updated_at: Option<String>,
    // V1.52 T-A P2: Work→KnowledgeEntryRecord provenance columns
    source_work_id: Option<String>,
    source_chapter: Option<i64>,
    source_provenance_kind: Option<String>,
    // V1.139 P1 T4: full serialized `extensions.nexus` namespace (Q7 round-trip).
    // Known identity/owner fields stay authoritative in their typed columns
    // above; this column preserves unknown keys when a spoke KnowledgeEntry
    // transits SQLite.
    extensions_nexus_json: Option<String>,
    // V1.146 P4 T1: full serialized `modules` namespace (modules durability).
    // Carries per-entry functional dialects (activation, pack, etc.) as a JSON
    // object. NULL for legacy rows; backfilled on next write cycle.
    modules_json: Option<String>,
}

impl KeyBlockRow {
    fn to_record(&self) -> Result<KnowledgeEntryRecord, KbStoreError> {
        let block_type = parse_block_type(&self.block_type)?;
        let body = self
            .body_json
            .as_ref()
            .and_then(|s| serde_json::from_str::<KnowledgeEntryBody>(s).ok());
        let source_anchor = self
            .source_anchor_json
            .as_ref()
            .and_then(|s| serde_json::from_str::<SourceAnchor>(s).ok());

        // v1.184 P1: reconstruct the closed canonical owner from the owner
        // union columns. An unknown `owner_kind`, or a missing/extra id
        // column, fails closed rather than fabricating an owner (malformed
        // owner data at the storage boundary → error, never a default World).
        let owner = match self.owner_kind.as_str() {
            "world" => KnowledgeOwnerRef::world(self.world_id.clone().ok_or_else(|| {
                KbStoreError::Storage(format!(
                    "malformed owner row {}: owner_kind='world' with NULL world_id",
                    self.key_block_id
                ))
            })?),
            "character" => KnowledgeOwnerRef::character(self.character_id.clone().ok_or_else(|| {
                KbStoreError::Storage(format!(
                    "malformed owner row {}: owner_kind='character' with NULL character_id",
                    self.key_block_id
                ))
            })?),
            "actor_world_binding" => KnowledgeOwnerRef::actor_world_binding(
                self.actor_world_binding_id.clone().ok_or_else(|| {
                    KbStoreError::Storage(format!(
                        "malformed owner row {}: owner_kind='actor_world_binding' \
                         with NULL actor_world_binding_id",
                        self.key_block_id
                    ))
                })?,
            ),
            other => {
                return Err(KbStoreError::Storage(format!(
                    "malformed owner row {}: unknown owner_kind {other:?}",
                    self.key_block_id
                )));
            }
        };

        // V1.139 P1 T4: activate the extensions.nexus round-trip (spec §2.2
        // rule 2). The 8 typed identity/owner columns below stay authoritative;
        // any *unknown* keys carried in `extensions_nexus_json` are surfaced on
        // `KnowledgeEntryRecord::extensions_nexus_extras` so they survive the
        // read-modify-write cycle and the spoke conversion seam.
        let extensions_nexus_extras = extract_nexus_extras(&self.build_merged_extensions_nexus());

        // V1.146 P4 T1: surface modules_json as KnowledgeEntryRecord.modules.
        let modules = self
            .modules_json
            .as_ref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

        Ok(KnowledgeEntryRecord {
            schema_version: 1,
            entry_id: self.key_block_id.clone(),
            owner,
            creator_only: self.creator_only != 0,
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
            extensions_nexus_extras,
            modules,
        })
    }

    /// Build the merged `extensions.nexus` namespace value (spec §2.3 read path).
    ///
    /// The 8 typed identity/owner columns are authoritative; any *unknown* keys
    /// carried in [`KeyBlockRow::extensions_nexus_json`] are preserved verbatim
    /// and merged underneath the `"nexus"` namespace. This is the canonical
    /// round-trip merge point for the `KnowledgeEntryRecord` ↔ spoke `KnowledgeEntry`
    /// conversion — [`extract_nexus_extras`] filters the result down to the
    /// unknown subset that rides on `KnowledgeEntryRecord::extensions_nexus_extras`.
    fn build_merged_extensions_nexus(&self) -> serde_json::Value {
        let mut existing = ExtensionMap::default();
        if let Some(json) = &self.extensions_nexus_json {
            if let Ok(map) =
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(json)
            {
                existing.insert("nexus".to_string(), map);
            }
        }
        // Recover the canonical owner from the typed columns (the same closed
        // `KnowledgeOwnerRef` [to_record] builds) so the merged namespace
        // carries the correct owner keys for the round-trip.
        let owner = match self.owner_kind.as_str() {
            "world" => KnowledgeOwnerRef::world(self.world_id.clone().unwrap_or_default()),
            "character" => {
                KnowledgeOwnerRef::character(self.character_id.clone().unwrap_or_default())
            }
            "actor_world_binding" => {
                KnowledgeOwnerRef::actor_world_binding(self.actor_world_binding_id.clone().unwrap_or_default())
            }
            other => {
                // Fail toward a World owner of an empty id is never correct;
                // but the extras merge only needs the namespace keys — a
                // malformed kind rejects later in `to_record`. Use an empty
                // World id so the merge is deterministic (never a fabricated
                // character/binding id).
                let _ = other;
                KnowledgeOwnerRef::world(String::new())
            }
        };
        build_extensions_nexus(
            &owner,
            self.creator_only != 0,
            self.created_from_command_id.as_deref(),
            self.source_work_id.as_deref(),
            self.source_chapter,
            self.source_provenance_kind.as_deref(),
            &existing,
        )
    }
}

/// Filter the merged `extensions.nexus` namespace down to the *unknown* keys
/// (everything outside the 5 typed identity fields). Returns `None` when no
/// unknown keys are present. This is the read-side companion to the typed-key
/// write path in [`build_extensions_nexus`] (spec §2.2 round-trip rule 2).
fn extract_nexus_extras(merged: &serde_json::Value) -> Option<serde_json::Value> {
    let map = merged.as_object()?;
    let extras: serde_json::Map<String, serde_json::Value> = map
        .iter()
        .filter(|(k, _)| !is_known_nexus_key(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    (!extras.is_empty()).then_some(serde_json::Value::Object(extras))
}

/// Build the wire-neutral [`ExtensionMap`] carrying an entry's unknown
/// `extensions.nexus` keys, ready to pass to [`build_extensions_nexus`] on the
/// write path (spec §2.3). Empty/absent extras yield an empty map so only the
/// typed identity keys are serialized. This is the write-side companion to
/// [`extract_nexus_extras`] and closes the read-modify-write round-trip.
fn nexus_extras_extension_map(extras: Option<&serde_json::Value>) -> ExtensionMap {
    let mut map = ExtensionMap::new();
    if let Some(serde_json::Value::Object(obj)) = extras {
        if !obj.is_empty() {
            map.insert("nexus".to_string(), obj.clone());
        }
    }
    map
}

/// Serialize a [`BlockType`] to the `kb_key_blocks.block_type` column format.
fn block_type_to_sql(block_type: BlockType) -> String {
    let block_type_str =
        serde_json::to_string(&block_type).unwrap_or_else(|_| format!("{block_type:?}"));
    block_type_str.trim_matches('"').to_string()
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
fn validation_err(e: nexus_knowledge::world_kb::KbError) -> KbStoreError {
    match e {
        nexus_knowledge::world_kb::KbError::Validation(ve) => KbStoreError::Validation(ve),
        nexus_knowledge::world_kb::KbError::ValidationError(msg) => {
            KbStoreError::Validation(ValidationError {
                kind: nexus_knowledge::world_kb::ValidationKind::MissingBody,
                field: None,
                message: msg,
            })
        }
        other => KbStoreError::Validation(ValidationError {
            kind: nexus_knowledge::world_kb::ValidationKind::MissingBody,
            field: None,
            message: other.to_string(),
        }),
    }
}

// SAFETY: sqlx SQLite futures borrow the connection pool internally;
// safe for single-threaded SQLite usage within our tokio runtime.
#[allow(clippy::future_not_send)]
impl KbStore for SqliteKbStore {
    async fn insert_knowledge_entry(
        &self,
        kb: KnowledgeEntryRecord,
    ) -> Result<KbInsertResult, KbStoreError> {
        let mut tx = self.pool.begin().await.map_err(|e| db_err(&e))?;
        let result = self.insert_key_block_in_tx(&mut tx, kb).await?;
        tx.commit().await.map_err(|e| db_err(&e))?;
        Ok(result)
    }

    async fn get_knowledge_entry(&self, key_block_id: &str) -> Result<KnowledgeEntryRecord, KbStoreError> {
        // SAFETY: runtime query because new provenance columns are unknown
        // to sqlx offline mode until migration 202606190003 is applied.
        let row = sqlx::query_as::<_, KeyBlockRow>(
            r"SELECT
                key_block_id, owner_kind, world_id, character_id,
                actor_world_binding_id, creator_only,
                block_type, canonical_name, status,
                revision, body_json, source_anchor_json, created_from_command_id,
                created_at, updated_at, source_work_id, source_chapter,
                source_provenance_kind, extensions_nexus_json, modules_json
            FROM kb_key_blocks
            WHERE key_block_id = ?",
        )
        .bind(key_block_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?
        .ok_or_else(|| KbStoreError::NotFound(key_block_id.to_string()))?;

        row.to_record()
    }

    async fn list_by_world(&self, world_id: &str) -> Result<Vec<KnowledgeEntryRecord>, KbStoreError> {
        // SAFETY: LIMIT is a compile-time constant; dynamic SQL needed because
        // sqlx::query_as! does not support LIMIT as bind param in SQLite offline mode.
        let rows = sqlx::query_as::<_, KeyBlockRow>(&format!(
            r"SELECT
                key_block_id,
                owner_kind,
                world_id,
                character_id,
                actor_world_binding_id,
                creator_only,
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
                source_provenance_kind, extensions_nexus_json, modules_json
            FROM kb_key_blocks
            WHERE owner_kind = 'world'
              AND world_id = ?
              AND status NOT IN ('deleted', 'merged', 'deprecated')
            ORDER BY created_at ASC
            LIMIT {LIST_BY_WORLD_LIMIT}"
        ))
        .bind(world_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?;

        rows.iter().map(KeyBlockRow::to_record).collect()
    }

    async fn query(&self, query: &KbQuery) -> Result<KbQueryResult, KbStoreError> {
        // Strategy: fetch all active blocks for the world, then apply
        // optional filters in-memory. This avoids complex dynamic SQL
        // and is efficient for per-world datasets (typically small).
        //
        // ## body_json growth and computable indexing (R-V161P0-LOW-004)
        //
        // Computable KnowledgeEntries (V1.61) embed `state` (dynamic runtime) and
        // `attributes` (immutable compute params) inside `body_json`. For
        // character KnowledgeEntries this can add several KiB of structured JSON
        // per block — the `body_json` TEXT column may grow with compute
        // usage over time.
        //
        // The `computable` filter is applied in-memory after `list_by_world`
        // (consistent with all other query filters). If per-world KnowledgeEntryRecord
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

        let filtered: Vec<KnowledgeEntryRecord> = all_active
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
        let items: Vec<KnowledgeEntryRecord> = filtered.into_iter().skip(offset).take(limit).collect();
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

    async fn update_knowledge_entry(&self, kb: KnowledgeEntryRecord) -> Result<(), KbStoreError> {
        // Validate canonical_name format/safety
        validate_canonical_name(&kb.canonical_name).map_err(validation_err)?;

        // Validate body semantics before persisting
        validate_body(kb.block_type, kb.body.as_ref(), self.validation_mode)
            .map_err(validation_err)?;

        // Verify exists
        let existing = self.get_knowledge_entry(&kb.entry_id).await?;

        // v1.184 P1: owner and `creator_only` are immutable through patch
        // APIs — moving knowledge is explicit create/copy work. Rejected here
        // (and in the in-memory store) rather than silently re-owned.
        if existing.owner != kb.owner || existing.creator_only != kb.creator_only {
            return Err(KbStoreError::ImmutableOwner(kb.entry_id.clone()));
        }

        // If name or type changed, check owner-scoped uniqueness.
        if existing.canonical_name != kb.canonical_name || existing.block_type != kb.block_type {
            // Stable snake_case serialization matching wire format
            let block_type_str = serde_json::to_string(&kb.block_type)
                .unwrap_or_else(|_| format!("{:?}", kb.block_type));
            let block_type_str = block_type_str.trim_matches('"').to_string();
            // Owner-scoped count: the owner column is chosen from the closed
            // [`KnowledgeOwnerRef`] (a fixed whitelist of owner-kinds), so the
            // SQL fragment is static — not user input. Runs as a runtime query
            // because the owner columns are new (unknown to sqlx offline mode).
            let owner_column: &str = match &kb.owner {
                KnowledgeOwnerRef::World(_) => "world_id",
                KnowledgeOwnerRef::Character(_) => "character_id",
                KnowledgeOwnerRef::ActorWorldBinding(_) => "actor_world_binding_id",
            };
            let q = format!(
                "SELECT COUNT(*) FROM kb_key_blocks \
                 WHERE {owner_column} = ? \
                   AND block_type = ? \
                   AND canonical_name = ? \
                   AND key_block_id != ? \
                   AND status NOT IN ('deleted', 'merged', 'deprecated')"
            );
            let count: i64 = sqlx::query_scalar(&q)
                .bind(kb.owner.id())
                .bind(&block_type_str)
                .bind(&kb.canonical_name)
                .bind(&kb.entry_id)
                .fetch_one(&*self.pool)
                .await
                .map_err(|e| db_err(&e))?;

            if count > 0 {
                return Err(KbStoreError::Duplicate {
                    owner: kb.owner.clone(),
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
        // V1.139 P1 T4: re-serialize the full `extensions.nexus` namespace on
        // UPDATE too, so unknown keys survive the read-modify-write cycle
        // (spec §2.3 write path; mirrors the INSERT path).
        let extensions_nexus_json = serde_json::to_string(&build_extensions_nexus(
            &kb.owner,
            kb.creator_only,
            kb.created_from_command_id.as_deref(),
            kb.source_work_id.as_deref(),
            kb.source_chapter,
            kb.source_provenance_kind.as_deref(),
            &nexus_extras_extension_map(kb.extensions_nexus_extras.as_ref()),
        ))
        .unwrap_or_default();
        // V1.146 P4 T1: serialize modules_json on UPDATE too.
        let modules_json = kb
            .modules
            .as_ref()
            .map(|m| serde_json::to_string(m).unwrap_or_default());

        // SAFETY: runtime query because `extensions_nexus_json` column is
        // unknown to sqlx offline mode (mirrors the INSERT path). Static SQL
        // with vetted column names from migration 202606190003.
        sqlx::query(
            r"UPDATE kb_key_blocks SET
                block_type = ?,
                canonical_name = ?,
                status = ?,
                revision = ?,
                body_json = ?,
                source_anchor_json = ?,
                updated_at = ?,
                extensions_nexus_json = ?,
                modules_json = ?
              WHERE key_block_id = ?",
        )
        .bind(&block_type_str)
        .bind(&kb.canonical_name)
        .bind(&kb.status)
        .bind(revision_i64)
        .bind(&body_json)
        .bind(&source_anchor_json)
        .bind(&kb.updated_at)
        .bind(&extensions_nexus_json)
        .bind(&modules_json)
        .bind(&kb.entry_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| db_err(&e))?;

        Ok(())
    }

    async fn delete_knowledge_entry(&self, key_block_id: &str) -> Result<(), KbStoreError> {
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

// ── V1.146 P3: pack-IO widened list methods (inherent, not trait) ─────────

impl SqliteKbStore {
    /// List `KnowledgeEntryRecord`s for a world **including** `deprecated` rows
    /// (still excluding `deleted` / `merged` terminal states).
    ///
    /// Used by the V1.146 P3 `creator world kb pack export --include-deprecated`
    /// CLI path. Mirrors [`KbStore::list_by_world`] but widens the status
    /// filter. Bound by the same [`LIST_BY_WORLD_LIMIT`] safety cap.
    ///
    /// # Errors
    ///
    /// Returns [`KbStoreError::Storage`] on database failure.
    pub async fn list_by_world_including_deprecated(
        &self,
        world_id: &str,
    ) -> Result<Vec<KnowledgeEntryRecord>, KbStoreError> {
        self.list_by_world_with_status_filter(world_id, true).await
    }

    /// Shared body for [`Self::list_by_world_including_deprecated`] —
    /// parameterized status clause so the two call sites don't duplicate
    /// the full SELECT shape.
    async fn list_by_world_with_status_filter(
        &self,
        world_id: &str,
        include_deprecated: bool,
    ) -> Result<Vec<KnowledgeEntryRecord>, KbStoreError> {
        // SAFETY: LIMIT is a compile-time constant; status filter is a static
        // fragment chosen from two literals (no user input). Dynamic SQL
        // needed because sqlx offline mode cannot bind LIMIT.
        let status_clause = if include_deprecated {
            "status NOT IN ('deleted', 'merged')"
        } else {
            "status NOT IN ('deleted', 'merged', 'deprecated')"
        };
        let sql = format!(
            r"SELECT
                key_block_id,
                owner_kind,
                world_id,
                character_id,
                actor_world_binding_id,
                creator_only,
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
                source_provenance_kind, extensions_nexus_json, modules_json
            FROM kb_key_blocks
            WHERE owner_kind = 'world'
              AND world_id = ?
              AND {status_clause}
            ORDER BY created_at ASC
            LIMIT {LIST_BY_WORLD_LIMIT}"
        );
        let rows = sqlx::query_as::<_, KeyBlockRow>(&sql)
            .bind(world_id)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| db_err(&e))?;

        rows.iter().map(KeyBlockRow::to_record).collect()
    }

    /// List active [`KnowledgeEntryRecord`]s owned by a canonical
    /// [`KnowledgeOwnerRef`] (v1.184 P1).
    ///
    /// The owner column is chosen from the closed owner kind (a fixed
    /// whitelist — not user input), so the SQL fragment is static. World
    /// owners return the same set as [`KbStore::list_by_world`]; Character and
    /// binding owners return their own isolated rows. Bound by the same
    /// [`LIST_BY_WORLD_LIMIT`] safety cap as `list_by_world`. `creator_only`
    /// is carried on the returned records (the view service filters it) — the
    /// store is owner-scoped, not visibility-scoped.
    ///
    /// # Errors
    ///
    /// Returns [`KbStoreError::Storage`] on database failure.
    pub async fn list_by_owner(
        &self,
        owner: &KnowledgeOwnerRef,
    ) -> Result<Vec<KnowledgeEntryRecord>, KbStoreError> {
        let owner_column = match owner {
            KnowledgeOwnerRef::World(_) => "world_id",
            KnowledgeOwnerRef::Character(_) => "character_id",
            KnowledgeOwnerRef::ActorWorldBinding(_) => "actor_world_binding_id",
        };
        let sql = format!(
            r"SELECT
                key_block_id,
                owner_kind,
                world_id,
                character_id,
                actor_world_binding_id,
                creator_only,
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
                source_provenance_kind, extensions_nexus_json, modules_json
            FROM kb_key_blocks
            WHERE {owner_column} = ?
              AND status NOT IN ('deleted', 'merged', 'deprecated')
            ORDER BY created_at ASC
            LIMIT {LIST_BY_WORLD_LIMIT}"
        );
        let rows = sqlx::query_as::<_, KeyBlockRow>(&sql)
            .bind(owner.id())
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| db_err(&e))?;

        rows.iter().map(KeyBlockRow::to_record).collect()
    }

    /// Complete owner listing for Actor KnowledgeView (v1.184 P1 T3 fix1).
    ///
    /// Unlike [`Self::list_by_owner`], this path has no silent 500-row cap.
    /// Rows are ordered by `(created_at, key_block_id)` so keyset pagination
    /// over the union is deterministic even when timestamps collide.
    ///
    /// # Errors
    ///
    /// Returns [`KbStoreError::Storage`] on database failure.
    pub async fn list_by_owner_complete(
        &self,
        owner: &KnowledgeOwnerRef,
    ) -> Result<Vec<KnowledgeEntryRecord>, KbStoreError> {
        let owner_column = match owner {
            KnowledgeOwnerRef::World(_) => "world_id",
            KnowledgeOwnerRef::Character(_) => "character_id",
            KnowledgeOwnerRef::ActorWorldBinding(_) => "actor_world_binding_id",
        };
        let sql = format!(
            r"SELECT
                key_block_id,
                owner_kind,
                world_id,
                character_id,
                actor_world_binding_id,
                creator_only,
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
                source_provenance_kind, extensions_nexus_json, modules_json
            FROM kb_key_blocks
            WHERE {owner_column} = ?
              AND status NOT IN ('deleted', 'merged', 'deprecated')
            ORDER BY created_at ASC, key_block_id ASC"
        );
        let rows = sqlx::query_as::<_, KeyBlockRow>(&sql)
            .bind(owner.id())
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| db_err(&e))?;

        rows.iter().map(KeyBlockRow::to_record).collect()
    }

    /// SQL-side owner keyset for Actor KnowledgeView (v1.184 P1 QC W2).
    ///
    /// Each component is bounded to `limit` rows (`limit` is already `page
    /// size + 1` at the call site). Chronological order uses millisecond unix
    /// time (`strftime('%s')` plus `%f` millis) matching
    /// `stored_created_at_order_millis`. Stored `created_at` bytes are not rewritten.
    ///
    /// # Errors
    ///
    /// Returns [`KbStoreError::Storage`] on database failure.
    pub async fn list_by_owner_keyset(
        &self,
        owner: &KnowledgeOwnerRef,
        after: Option<&(String, String)>,
        limit: u32,
        exclude_creator_only: bool,
    ) -> Result<Vec<KnowledgeEntryRecord>, KbStoreError> {
        let owner_column = match owner {
            KnowledgeOwnerRef::World(_) => "world_id",
            KnowledgeOwnerRef::Character(_) => "character_id",
            KnowledgeOwnerRef::ActorWorldBinding(_) => "actor_world_binding_id",
        };
        let created_key = "(CAST(strftime('%s', created_at) AS INTEGER) * 1000 + CAST(substr(strftime('%f', created_at), 4) AS INTEGER))";
        let cursor_millis = "(CAST(strftime('%s', ?) AS INTEGER) * 1000 + CAST(substr(strftime('%f', ?), 4) AS INTEGER))";
        let visibility = if exclude_creator_only {
            " AND creator_only = 0"
        } else {
            ""
        };
        let cursor_sql = if after.is_some() {
            format!(
                " AND ({created_key} > {cursor_millis} \
                   OR ({created_key} = {cursor_millis} \
                       AND key_block_id > ?))"
            )
        } else {
            String::new()
        };
        let sql = format!(
            r"SELECT
                key_block_id,
                owner_kind,
                world_id,
                character_id,
                actor_world_binding_id,
                creator_only,
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
                source_provenance_kind, extensions_nexus_json, modules_json
            FROM kb_key_blocks
            WHERE {owner_column} = ?
              AND status NOT IN ('deleted', 'merged', 'deprecated'){visibility}{cursor_sql}
            ORDER BY {created_key} ASC, key_block_id ASC
            LIMIT {limit}"
        );
        let mut query = sqlx::query_as::<_, KeyBlockRow>(&sql).bind(owner.id());
        if let Some((created_at, entry_id)) = after {
            query = query
                .bind(created_at)
                .bind(created_at)
                .bind(created_at)
                .bind(created_at)
                .bind(entry_id);
        }
        let rows = query
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| db_err(&e))?;
        rows.iter().map(KeyBlockRow::to_record).collect()
    }
}

// ── V1.73 Canvas World KB: per-row OCC CAS entity edit ──────────────────────

/// V1.73 P0: CAS-aware partial update of a `kb_key_blocks` row.
///
/// Mirrors the V1.51 `kb_extract_jobs` CAS pattern
/// ([`kb_extract_job::mark_confirmed_in_tx_with_cas`]). Adds a
/// `WHERE key_block_id = ? AND COALESCE(revision, 0) = ? AND world_id = ?`
/// guard so a stale preimage (read before another writer modified the row)
/// is rejected with [`LocalDbError::VersionMismatch`], and — V1.154 P2 (R3
/// closure) — a row moved to another world between the caller's read and the
/// UPDATE is rejected with [`LocalDbError::WorldConflict`] instead of a
/// generic version mismatch. On success the `revision` column is bumped to
/// `expected_revision + 1` and the bumped value is returned.
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
/// - `world_id` — the stored-world the caller verified on read (spec §3.1:
///   the world bind is the stored-world expected by the request). A row that
///   moved to another world fails the predicate and classifies as
///   [`LocalDbError::WorldConflict`].
///
/// # Returns
///
/// - `Ok(new_revision)` — row updated, returns the new bumped version.
/// - `Err(LocalDbError::VersionMismatch)` — the row's `revision` changed
///   between read and UPDATE (409 caller-side).
/// - `Err(LocalDbError::VersionMismatch { actual: None })` — row not found.
/// - `Err(LocalDbError::WorldConflict)` — the row now lives in another
///   world (R3: cross-process writers; wire code `world_conflict`).
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
    world_id: &str,
) -> Result<u64, LocalDbError> {
    // Build a dynamic SET clause from the supplied fields. revision is always
    // bumped; updated_at always set. SAFETY: dynamic SET built from a fixed
    // field whitelist (not user-controlled SQL); all values are bind params.
    let mut sets = vec!["revision = ?".to_string(), "updated_at = ?".to_string()];
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
    // controlled SQL); all values are bind params. COALESCE(revision, 0)
    // NULL-normalizes the revision column per the architect Phase 2b lock
    // (existing rows may have revision = NULL; treated as 0 for OCC).
    // V1.154 P2 (R3 closure, spec §3.2 LOCKED): the stored `world_id` joins
    // the CAS predicate so a row moved to another world between the caller's
    // verified read and this UPDATE cannot be rewritten cross-world.
    // v1.184 P1: the CAS lane is World-owned only — `owner_kind = 'world'`
    // joins the predicate so a non-World row (NULL `world_id`) can never be
    // patched through a world-scoped route (fails closed as a world conflict).
    let sql = format!(
        "UPDATE kb_key_blocks SET {set_clause} \
         WHERE key_block_id = ? AND COALESCE(revision, 0) = ? \
           AND owner_kind = 'world' AND world_id = ?"
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
    q = q.bind(key_block_id).bind(expected_revision).bind(world_id);
    let result = q.execute(&mut **tx).await?;

    if result.rows_affected() == 1 {
        return Ok(u64::try_from(new_revision).unwrap_or(0));
    }

    // rows_affected == 0 — disambiguate world move vs not-found vs version
    // mismatch by re-reading the row. `world_id` is NULL for non-World
    // owners (v1.184 P1), so a Character/binding row (or a row moved to
    // another world) fails the `owner_kind='world' AND world_id = ?`
    // predicate and classifies as a world conflict (fail-closed — a non-World
    // row can never be rewritten through a World-scoped CAS lane). NULL
    // revision is treated as 0.
    let current: Option<(Option<i64>, Option<String>)> =
        sqlx::query_as(
            "SELECT revision, world_id FROM kb_key_blocks WHERE key_block_id = ?",
        )
        .bind(key_block_id)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some((_, stored_world)) = current.as_ref() {
        if stored_world.as_deref() != Some(world_id) {
            // The caller's revision was valid; the WORLD moved (a cross-
            // process writer) or the row is non-World-owned. Surfaced as a
            // world_conflict, not a generic OCC version mismatch (spec §3.2).
            return Err(LocalDbError::WorldConflict {
                table: "kb_key_blocks".to_string(),
                id: key_block_id.to_string(),
                expected_world: world_id.to_string(),
                actual_world: stored_world.clone().unwrap_or_default(),
            });
        }
    }
    let actual = current.map(|(rev, _)| rev.unwrap_or(0));
    Err(LocalDbError::VersionMismatch {
        table: "kb_key_blocks".to_string(),
        id: key_block_id.to_string(),
        expected: expected_revision,
        actual,
    })
}

/// Character ToM carrier CAS: replaces `modules_json` and bumps `revision`.
///
/// Used by the v1.184 P4 Character ToM seam for Character- or binding-owned
/// carrier KnowledgeEntries that cannot use the World-scoped
/// [`cas_update_key_block_fields`] predicate.
///
/// Beyond OCC revision, the predicate revalidates inside the write
/// transaction (QC fix round 1, F-004) that the carrier is still live
/// (`status NOT IN ('deleted','merged','deprecated')` — soft-delete does not
/// bump `revision`) and still owned by the admitted Character or binding
/// (`owner_character_id` / `owner_binding_id`). A concurrent soft-delete or
/// ownership drift therefore misses the CAS and rolls back.
///
/// # Errors
///
/// Returns [`LocalDbError::VersionMismatch`] on stale OCC, lost liveness, or
/// ownership drift, and [`LocalDbError::Sqlx`] on database failure.
pub async fn cas_update_key_block_modules_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key_block_id: &str,
    modules_json: &str,
    expected_revision: i64,
    owner_character_id: &str,
    owner_binding_id: &str,
) -> Result<u64, LocalDbError> {
    let now = chrono::Utc::now().to_rfc3339();
    // Checked increment (v1.184 P4 T2 fix): an `i64::MAX` expected revision
    // must reject deterministically instead of overflowing the bump.
    let new_revision = expected_revision.checked_add(1).ok_or_else(|| {
        LocalDbError::ValidationError("expected_revision overflow: cannot bump revision".into())
    })?;
    let result = sqlx::query(
        "UPDATE kb_key_blocks SET revision = ?, updated_at = ?, modules_json = ?          WHERE key_block_id = ? AND COALESCE(revision, 0) = ?            AND status NOT IN ('deleted', 'merged', 'deprecated')            AND (character_id = ? OR actor_world_binding_id = ?)",
    )
    .bind(new_revision)
    .bind(&now)
    .bind(modules_json)
    .bind(key_block_id)
    .bind(expected_revision)
    .bind(owner_character_id)
    .bind(owner_binding_id)
    .execute(&mut **tx)
    .await?;

    if result.rows_affected() == 1 {
        return Ok(u64::try_from(new_revision).unwrap_or(0));
    }

    let current: Option<Option<i64>> =
        sqlx::query_scalar("SELECT revision FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(key_block_id)
            .fetch_optional(&mut **tx)
            .await?;
    // NULL revision normalizes to 0, matching the COALESCE(revision, 0)
    // predicate and the established cas_update_key_block_fields reporting;
    // `None` means the row itself is absent.
    let actual = current.map(|rev| rev.unwrap_or(0));
    Err(LocalDbError::VersionMismatch {
        table: "kb_key_blocks".to_string(),
        id: key_block_id.to_string(),
        expected: expected_revision,
        actual,
    })
}


/// Persist the non-CAS fields of a `kb_key_blocks` row inside a caller-owned
/// transaction.
///
/// Used after [`cas_update_key_block_fields`] on the spoke adapter update path
/// so `status`, `source_anchor_json`, `extensions_nexus_json`, and
/// `modules_json` are written in the same transaction as the revision bump.
///
/// # Errors
///
/// Returns [`sqlx::Error`] on database failure.
///
/// `source_provenance_kind` uses COALESCE semantics (V1.155 P2 T3,
/// R-V1152P0-001): the dedicated provenance column is updated only when the
/// incoming entry carries a value, so the pack-import overwrite stamp is
/// atomic with the CAS body replace while ordinary edits (which do not
/// round-trip provenance) leave the column untouched.
pub async fn update_key_block_auxiliary_fields_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key_block_id: &str,
    status: &str,
    source_anchor_json: Option<&str>,
    extensions_nexus_json: &str,
    modules_json: Option<&str>,
    source_provenance_kind: Option<&str>,
) -> Result<(), sqlx::Error> {
    // SAFETY: static SQL with vetted column names from migration
    // 202606190003_kb_key_blocks_provenance.sql.
    sqlx::query(
        r"UPDATE kb_key_blocks SET
             status = ?,
             source_anchor_json = ?,
             extensions_nexus_json = ?,
             modules_json = ?,
             source_provenance_kind = COALESCE(?, source_provenance_kind)
           WHERE key_block_id = ?",
    )
    .bind(status)
    .bind(source_anchor_json)
    .bind(extensions_nexus_json)
    .bind(modules_json)
    .bind(source_provenance_kind)
    .bind(key_block_id)
    .execute(&mut **tx)
    .await
    .map(|_| ())
}

// ── V1.147 W3: TX-aware state read/write primitives (state-delta lane) ─────
// The Accept lane's state-delta path reads/writes `kb_key_blocks` through
// these primitives so ALL SQL against the table lives in `nexus-local-db`
// (one storage write-path family). `nexus-orchestration::state_delta` owns
// the semantic merge (dot-path validation, add/sub/set, block_type state-key
// rules) and calls these inside the caller's transaction.

/// The `kb_key_blocks` storage fields a state delta needs, read inside a
/// caller-owned transaction (V1.147 W3).
#[derive(Debug, Clone)]
pub struct KbStateRow {
    /// Stored `block_type` column value (`snake_case` wire string).
    pub block_type: String,
    /// Stored `body_json` (`None` when the row has no body).
    pub body_json: Option<String>,
    /// Stored `world_id` — the caller's world-scope check compares against it.
    pub world_id: String,
}

/// Read a key block's state-relevant storage fields inside a caller-owned
/// transaction.
///
/// Compile-time checked (F-004: the Accept lane is the highest-risk write
/// path, so its SQL must be offline-validated). Returns `Ok(None)` when no
/// row with `key_block_id` exists — the caller distinguishes "not found"
/// from "found" (a foreign-world target must read the row to be detected).
///
/// # Errors
///
/// Returns [`LocalDbError`] on database failure.
pub async fn read_kb_state_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key_block_id: &str,
) -> Result<Option<KbStateRow>, LocalDbError> {
    // v1.184 P1: `world_id` is nullable since migration
    // 20260905000002_actor_knowledge_owners.sql (non-World owners store NULL).
    // The state-delta lane is World-only by construction
    // (`update_kb_state_in_tx` is world-scoped), so the `!` override asserts
    // the legacy NOT NULL contract instead of widening `KbStateRow` to
    // `Option<String>`; a non-World row here fails closed at decode time.
    let row = sqlx::query!(
        "SELECT block_type, body_json, world_id AS \"world_id!\" FROM kb_key_blocks \
         WHERE key_block_id = ?",
        key_block_id,
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|r| KbStateRow {
        block_type: r.block_type,
        body_json: r.body_json,
        world_id: r.world_id,
    }))
}

/// Transaction-aware state-only update of a `kb_key_blocks` row.
///
/// This is the storage primitive behind the state-delta Accept lane: it
/// updates ONLY `body_json` + `updated_at` inside a caller-owned transaction
/// and is **world-scoped** (`WHERE key_block_id = ? AND world_id = ?`) so a
/// target that changed worlds mid-transaction cannot be written. The caller
/// (`nexus-orchestration::state_delta`) owns the semantic merge and passes
/// the re-serialized body.
///
/// This is deliberately NOT the full [`KbStore::update_knowledge_entry`]
/// validation path (canonical name, uniqueness, extensions): the delta only
/// mutates `body.state`, so those invariants are untouched by construction.
///
/// Returns the number of affected rows (1 on success, 0 when no row with
/// `key_block_id` exists in `world_id`).
///
/// # Errors
///
/// Returns [`LocalDbError`] on database failure.
pub async fn update_kb_state_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key_block_id: &str,
    world_id: &str,
    body_json: &str,
) -> Result<u64, LocalDbError> {
    let updated_at = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query!(
        "UPDATE kb_key_blocks SET body_json = ?, updated_at = ? \
         WHERE key_block_id = ? AND world_id = ?",
        body_json,
        updated_at,
        key_block_id,
        world_id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected())
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
    use serde_json::{json, Value};

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
        let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");

        let result = store.insert_knowledge_entry(kb.clone()).await.unwrap();
        assert_eq!(result.entry_id, kb.entry_id);

        let fetched = store.get_knowledge_entry(&kb.entry_id).await.unwrap();
        assert_eq!(fetched.canonical_name, "Hero");
        assert_eq!(fetched.world_id(), Some("wld_1"));
    }

    #[tokio::test]
    async fn insert_key_block_in_tx_rollback_leaves_no_row() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool.clone());
        let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "RollbackHero");
        let entry_id = kb.entry_id.clone();

        let mut tx = pool.begin().await.unwrap();
        store
            .insert_key_block_in_tx(&mut tx, kb)
            .await
            .expect("insert in tx should succeed");
        tx.rollback().await.unwrap();

        let err = store.get_knowledge_entry(&entry_id).await.unwrap_err();
        assert!(
            matches!(err, KbStoreError::NotFound(ref id) if *id == entry_id),
            "rolled-back insert must not persist: {err:?}"
        );
    }

    // ── V1.147 W3: TX-aware state read/write primitives ──────────────────────

    fn kb_with_state(world_id: &str, canonical_name: &str, hp: i64) -> KnowledgeEntryRecord {
        let mut kb = KnowledgeEntryRecord::new(world_id, BlockType::Character, canonical_name);
        kb.body = Some(KnowledgeEntryBody {
            summary: None,
            attributes: None,
            tags: None,
            state: Some(json!({"character": {"current_hp": hp}})),
            computable: Some(true),
        });
        kb
    }

    /// Read the persisted `body_json` through the state-delta read primitive
    /// (keeps the assertion on the same lane the Accept path uses).
    async fn stored_body_json(pool: &SqlitePool, key_block_id: &str) -> Option<String> {
        let mut tx = pool.begin().await.unwrap();
        let row = read_kb_state_in_tx(&mut tx, key_block_id).await.unwrap();
        tx.rollback().await.unwrap();
        row.and_then(|r| r.body_json)
    }

    #[tokio::test]
    async fn read_kb_state_in_tx_returns_stored_fields() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool.clone());
        let kb = kb_with_state("wld_1", "ReadHero", 100);
        let entry_id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        let row = read_kb_state_in_tx(&mut tx, &entry_id)
            .await
            .expect("read in tx must succeed")
            .expect("row must exist");
        let missing = read_kb_state_in_tx(&mut tx, "kb_missing")
            .await
            .expect("read must not error");
        tx.rollback().await.unwrap();

        assert_eq!(row.block_type, "character");
        assert_eq!(row.world_id, "wld_1");
        let body: Value = serde_json::from_str(&row.body_json.expect("body present")).unwrap();
        assert_eq!(body["state"]["character"]["current_hp"], 100);

        assert!(missing.is_none(), "unknown id must read as None");
    }

    #[tokio::test]
    async fn update_kb_state_in_tx_world_scope_rejects_foreign_world() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;
        // Second world for the foreign-target case.
        sqlx::query!(
            r#"INSERT INTO narrative_worlds
                (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json)
               VALUES ('wld_2', 'wrk_test', 'ctr_test', 'Other World', 'other-world', 'active', 'private', 'manual', '{}')"#
        )
        .execute(&pool)
        .await
        .unwrap();

        let store = SqliteKbStore::new(pool.clone());
        let kb = kb_with_state("wld_1", "ScopedHero", 100);
        let entry_id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        let affected = update_kb_state_in_tx(&mut tx, &entry_id, "wld_2", "{}")
            .await
            .expect("world-scoped update must not error");
        tx.rollback().await.unwrap();
        assert_eq!(affected, 0, "foreign-world update must affect no rows");

        // Body unchanged.
        let body_json = stored_body_json(&pool, &entry_id)
            .await
            .expect("row still has body");
        let body: Value = serde_json::from_str(&body_json).unwrap();
        assert_eq!(body["state"]["character"]["current_hp"], 100);
    }

    #[tokio::test]
    async fn update_kb_state_in_tx_rollback_restores_body() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool.clone());
        let kb = kb_with_state("wld_1", "RollbackState", 100);
        let entry_id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        // Write a new body inside a TX, then roll back — the primitive's
        // write must be rolled back with the caller's transaction.
        let new_body = json!({"state": {"character": {"current_hp": 7}}});
        let mut tx = pool.begin().await.unwrap();
        let affected = update_kb_state_in_tx(&mut tx, &entry_id, "wld_1", &new_body.to_string())
            .await
            .expect("in-tx update must succeed");
        assert_eq!(affected, 1);
        tx.rollback().await.unwrap();

        let body_json = stored_body_json(&pool, &entry_id)
            .await
            .expect("row still has body");
        let body: Value = serde_json::from_str(&body_json).unwrap();
        assert_eq!(
            body["state"]["character"]["current_hp"], 100,
            "rolled-back state update must not persist"
        );
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let (pool, _dir) = fresh_pool().await;
        let store = SqliteKbStore::new(pool);
        let err = store
            .get_knowledge_entry("kb_nonexistent")
            .await
            .unwrap_err();
        assert!(matches!(err, KbStoreError::NotFound(ref s) if s == "kb_nonexistent"));
    }

    #[tokio::test]
    async fn test_list_by_world() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb1 = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");
        let kb2 = KnowledgeEntryRecord::new("wld_1", BlockType::Scene, "Forest");
        store.insert_knowledge_entry(kb1).await.unwrap();
        store.insert_knowledge_entry(kb2).await.unwrap();

        let items = store.list_by_world("wld_1").await.unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn test_list_excludes_deleted() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        store.delete_knowledge_entry(&id).await.unwrap();

        let items = store.list_by_world("wld_1").await.unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn test_get_active_by_unique_key_beyond_list_limit() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        // Fill the list_by_world window (500 oldest rows) so a newer entry is
        // absent from the scan — the retry-safe promote path must not rely on it.
        for i in 0..LIST_BY_WORLD_LIMIT {
            seed::knowledge_entry(
                &pool,
                &format!("kb_fill_{i:03}"),
                "wld_1",
                "character",
                &format!("Fill_{i:03}"),
                "confirmed",
            )
            .await;
        }
        seed::knowledge_entry(
            &pool,
            "kb_target",
            "wld_1",
            "character",
            "RetryTarget",
            "confirmed",
        )
        .await;

        let store = SqliteKbStore::new(pool);
        let listed = store.list_by_world("wld_1").await.unwrap();
        assert_eq!(listed.len(), usize::try_from(LIST_BY_WORLD_LIMIT).unwrap());
        assert!(
            !listed.iter().any(|kb| kb.canonical_name == "RetryTarget"),
            "newest entry must fall outside the list_by_world window"
        );

        let found = store
            .get_active_by_unique_key("wld_1", "RetryTarget", BlockType::Character)
            .await
            .unwrap()
            .expect("targeted lookup must find the active row");
        assert_eq!(found.entry_id, "kb_target");
    }

    #[tokio::test]
    async fn test_list_by_world_scoped_entry_id_beyond_list_window() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        for i in 0..LIST_BY_WORLD_LIMIT {
            seed::knowledge_entry(
                &pool,
                &format!("kb_fill_{i:03}"),
                "wld_1",
                "item",
                &format!("Fill_{i:03}"),
                "confirmed",
            )
            .await;
        }
        seed::knowledge_entry(
            &pool,
            "kb_scoped_target",
            "wld_1",
            "character",
            "ScopedTarget",
            "confirmed",
        )
        .await;

        let store = SqliteKbStore::new(pool);
        let scoped = store
            .list_by_world_scoped("wld_1", &["kb_scoped_target".to_string()], &[])
            .await
            .unwrap();
        assert!(!scoped.truncated);
        assert_eq!(scoped.entries.len(), 1);
        assert_eq!(scoped.entries[0].entry_id, "kb_scoped_target");
    }

    #[tokio::test]
    async fn test_list_by_world_scoped_unfiltered_detects_truncation() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        for i in 0..=LIST_BY_WORLD_LIMIT {
            seed::knowledge_entry(
                &pool,
                &format!("kb_cap_{i:03}"),
                "wld_1",
                "item",
                &format!("Cap_{i:03}"),
                "confirmed",
            )
            .await;
        }

        let store = SqliteKbStore::new(pool);
        let scoped = store.list_by_world_scoped("wld_1", &[], &[]).await.unwrap();
        assert!(scoped.truncated);
        assert_eq!(
            scoped.entries.len(),
            usize::try_from(LIST_BY_WORLD_LIMIT).unwrap()
        );
    }

    #[tokio::test]
    async fn test_list_by_owner_complete_has_no_silent_500_cap() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;
        let store = SqliteKbStore::new(pool.clone());
        let n = usize::try_from(LIST_BY_WORLD_LIMIT).unwrap() + 1;
        for i in 0..n {
            let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Item, &format!("Cap_{i:03}"));
            store.insert_knowledge_entry(kb).await.unwrap();
        }
        sqlx::query("UPDATE kb_key_blocks SET created_at = '2026-01-01T00:00:00Z'")
            .execute(&pool)
            .await
            .unwrap();

        let capped = store
            .list_by_owner(&KnowledgeOwnerRef::world("wld_1"))
            .await
            .unwrap();
        assert_eq!(capped.len(), usize::try_from(LIST_BY_WORLD_LIMIT).unwrap());

        let complete = store
            .list_by_owner_complete(&KnowledgeOwnerRef::world("wld_1"))
            .await
            .unwrap();
        assert_eq!(complete.len(), n);
        let ids: Vec<&str> = complete.iter().map(|r| r.entry_id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "equal timestamps must tie-break on key_block_id");
    }

    #[tokio::test]
    async fn test_list_by_owner_keyset_bounds_and_mixed_timestamp_order() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;
        let store = SqliteKbStore::new(pool.clone());
        let rows = [
            ("kb_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1", "2026-01-01 00:00:02"),
            ("kb_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb1", "2026-01-01T00:00:00Z"),
            ("kb_ccccccccccccccccccccccccccccccc1", "2026-01-01T00:00:01Z"),
        ];
        for (id, ts) in rows {
            let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Item, id);
            kb.entry_id = id.to_string();
            store.insert_knowledge_entry(kb).await.unwrap();
            sqlx::query("UPDATE kb_key_blocks SET created_at = ? WHERE key_block_id = ?")
                .bind(ts)
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        for i in 0..8 {
            let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Item, &format!("Pad_{i}"));
            store.insert_knowledge_entry(kb).await.unwrap();
            sqlx::query(
                "UPDATE kb_key_blocks SET created_at = '2026-01-02T00:00:00Z' WHERE canonical_name = ?",
            )
            .bind(format!("Pad_{i}"))
            .execute(&pool)
            .await
            .unwrap();
        }

        let owner = KnowledgeOwnerRef::world("wld_1");
        let first = store
            .list_by_owner_keyset(&owner, None, 3, false)
            .await
            .unwrap();
        assert_eq!(first.len(), 3, "SQL LIMIT must bound the component");
        assert_eq!(
            first.iter().map(|r| r.entry_id.as_str()).collect::<Vec<_>>(),
            vec![
                "kb_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb1",
                "kb_ccccccccccccccccccccccccccccccc1",
                "kb_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1",
            ]
        );
        let stored_space: String = sqlx::query_scalar(
            "SELECT created_at FROM kb_key_blocks WHERE key_block_id = ?",
        )
        .bind("kb_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored_space, "2026-01-01 00:00:02");

        let after = (first[1].created_at.clone(), first[1].entry_id.clone());
        let page = store
            .list_by_owner_keyset(&owner, Some(&after), 2, false)
            .await
            .unwrap();
        assert!(page.len() <= 2);
        assert_eq!(page[0].entry_id, "kb_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1");
    }

    #[tokio::test]
    async fn test_list_by_owner_keyset_same_millisecond_reverse_ids() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;
        let store = SqliteKbStore::new(pool.clone());
        let rows = [
            ("kb_m", "2026-01-01T10:00:00.123200Z"),
            ("kb_a", "2026-01-01T10:00:00.123300Z"),
            ("kb_z", "2026-01-01T10:00:01Z"),
        ];
        for (id, ts) in rows {
            let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Item, id);
            kb.entry_id = id.to_string();
            store.insert_knowledge_entry(kb).await.unwrap();
            sqlx::query("UPDATE kb_key_blocks SET created_at = ? WHERE key_block_id = ?")
                .bind(ts)
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        let owner = KnowledgeOwnerRef::world("wld_1");
        let first = store
            .list_by_owner_keyset(&owner, None, 1, false)
            .await
            .unwrap();
        assert_eq!(
            first.iter().map(|r| r.entry_id.as_str()).collect::<Vec<_>>(),
            vec!["kb_a"]
        );
        let after = (first[0].created_at.clone(), first[0].entry_id.clone());
        let page = store
            .list_by_owner_keyset(&owner, Some(&after), 2, false)
            .await
            .unwrap();
        let ids: Vec<&str> = page.iter().map(|r| r.entry_id.as_str()).collect();
        assert_eq!(ids, vec!["kb_m", "kb_z"]);
    }

    #[tokio::test]
    async fn test_uniqueness_rejects_duplicate() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb1 = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");
        store.insert_knowledge_entry(kb1).await.unwrap();

        let kb2 = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");
        let err = store.insert_knowledge_entry(kb2).await.unwrap_err();
        assert!(matches!(err, KbStoreError::Duplicate { .. }));
    }

    #[tokio::test]
    async fn test_attach_and_get_anchors() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

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
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb.clone()).await.unwrap();

        kb.canonical_name = "Superhero".to_string();
        kb.updated_at = Some(chrono::Utc::now().to_rfc3339());
        store.update_knowledge_entry(kb).await.unwrap();

        let fetched = store.get_knowledge_entry(&id).await.unwrap();
        assert_eq!(fetched.canonical_name, "Superhero");
    }

    #[tokio::test]
    async fn test_delete_key_block() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        store.delete_knowledge_entry(&id).await.unwrap();

        // Block still exists but marked deleted
        let fetched = store.get_knowledge_entry(&id).await.unwrap();
        assert_eq!(fetched.status, "deleted");
    }

    #[tokio::test]
    async fn test_deleted_allows_reinsertion() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        store.delete_knowledge_entry(&id).await.unwrap();

        // Re-insert with same canonical_name + type should succeed
        let kb2 = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");
        assert!(store.insert_knowledge_entry(kb2).await.is_ok());
    }

    #[tokio::test]
    async fn test_query_with_block_type() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        store
            .insert_knowledge_entry(KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero"))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(KnowledgeEntryRecord::new("wld_1", BlockType::Scene, "Forest"))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Villain"))
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
            .insert_knowledge_entry(KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero"))
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
    ) -> KnowledgeEntryRecord {
        let mut kb = KnowledgeEntryRecord::new(world_id, block_type, name);
        kb.body = Some(KnowledgeEntryBody {
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
        let result = store.insert_knowledge_entry(kb).await;
        assert!(result.is_ok(), "expected ok, got {:?}", result.unwrap_err());
    }

    #[tokio::test]
    async fn test_sqlite_novel_missing_category_rejected() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::with_validation_mode(pool, ValidationMode::Novel);
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "char_no_cat");
        kb.body = Some(KnowledgeEntryBody {
            summary: Some("A character without category".to_string()),
            attributes: Some(serde_json::json!({"aliases": ["NoCat"]})),
            tags: Some(vec!["novel".to_string()]),
            ..Default::default()
        });

        let err = store.insert_knowledge_entry(kb).await.unwrap_err();
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
        let err = store.insert_knowledge_entry(kb).await.unwrap_err();
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
        store.insert_knowledge_entry(kb1).await.unwrap();

        let kb2 = make_novel_block_sql("wld_1", BlockType::Character, "char_dupe", "character");
        let err = store.insert_knowledge_entry(kb2).await.unwrap_err();
        assert!(matches!(err, KbStoreError::Duplicate { .. }));
    }

    #[tokio::test]
    async fn test_sqlite_canonical_name_validation_rejects_slash() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "evil/../path");
        let err = store.insert_knowledge_entry(kb).await.unwrap_err();
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
        let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "evil;rm -rf");
        let err = store.insert_knowledge_entry(kb).await.unwrap_err();
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
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "temp");
        kb.canonical_name = String::new();
        let err = store.insert_knowledge_entry(kb).await.unwrap_err();
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
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "char_generic");
        kb.body = Some(KnowledgeEntryBody {
            summary: Some("A generic character".to_string()),
            attributes: None,
            tags: None,
            ..Default::default()
        });
        assert!(store.insert_knowledge_entry(kb).await.is_ok());
    }

    #[tokio::test]
    async fn test_sqlite_update_validates_body_in_novel_mode() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::with_validation_mode(pool, ValidationMode::Novel);
        let kb = make_novel_block_sql("wld_1", BlockType::Character, "char_hero", "character");
        let mut kb = kb;
        store.insert_knowledge_entry(kb.clone()).await.unwrap();

        // Update to body missing novel_category should fail
        kb.body = Some(KnowledgeEntryBody {
            summary: Some("updated".to_string()),
            attributes: Some(serde_json::json!({"traits": ["old"]})),
            tags: None,
            ..Default::default()
        });
        kb.updated_at = Some(chrono::Utc::now().to_rfc3339());

        let err = store.update_knowledge_entry(kb).await.unwrap_err();
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
        let kb = KnowledgeEntryRecord::new("wld_1", BlockType::InfoPoint, "test_block");
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

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

    fn make_computable_kb(
        world_id: &str,
        name: &str,
        bt: BlockType,
        computable: bool,
    ) -> KnowledgeEntryRecord {
        let mut kb = KnowledgeEntryRecord::new(world_id, bt, name);
        kb.body = Some(KnowledgeEntryBody {
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
            .insert_knowledge_entry(make_computable_kb(
                "wld_1",
                "Hero",
                BlockType::Character,
                true,
            ))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(make_computable_kb(
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
            .insert_knowledge_entry(make_computable_kb(
                "wld_1",
                "Hero",
                BlockType::Character,
                true,
            ))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(make_computable_kb(
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
            .insert_knowledge_entry(make_computable_kb(
                "wld_1",
                "Hero",
                BlockType::Character,
                true,
            ))
            .await
            .unwrap();
        store
            .insert_knowledge_entry(make_computable_kb(
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
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Legacy");
        kb.body = Some(KnowledgeEntryBody {
            summary: Some("legacy".to_string()),
            attributes: None,
            tags: None,
            ..Default::default()
        });
        store.insert_knowledge_entry(kb).await.unwrap();

        // computable=true should exclude it
        let q = KbQuery::new("wld_1").with_computable(Some(true));
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 0);

        // computable=false should include it
        let q = KbQuery::new("wld_1").with_computable(Some(false));
        let result = store.query(&q).await.unwrap();
        assert_eq!(result.total_count, 1);
    }

    // ── V1.139 P1 T4: extensions.nexus round-trip (Greptile P2) ──────
    //
    // Proves unknown `extensions.nexus` keys survive the SQLite
    // read-modify-write cycle: INSERT writes them into `extensions_nexus_json`,
    // and GET surfaces them back on `KnowledgeEntryRecord.extensions_nexus_extras`.
    // The 5 typed identity fields stay authoritative in their own columns.

    #[tokio::test]
    async fn test_sqlite_extensions_nexus_extras_roundtrip_on_insert_and_get() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");
        kb.extensions_nexus_extras =
            Some(serde_json::json!({"custom_label": "villain-arc", "priority": 7}));
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        let fetched = store.get_knowledge_entry(&id).await.unwrap();
        let extras = fetched
            .extensions_nexus_extras
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .expect("unknown extensions.nexus keys survive INSERT→GET");
        assert_eq!(extras["custom_label"], "villain-arc");
        assert_eq!(extras["priority"], 7);
        // Typed identity fields remain authoritative on their own columns.
        assert_eq!(fetched.world_id(), Some("wld_1"));
    }

    #[tokio::test]
    async fn test_insert_key_block_with_extensions_in_tx_round_trip() {
        // V1.145 P0 T3: the new opaque-JSON INSERT primitive round-trips
        // `extensions.nexus` identical to the legacy wrapper. Verifies the
        // storage primitive is behavior-equivalent when the caller owns the
        // serialization boundary (spec §7.4) — the same shape the spoke
        // adapter `put_create` path now uses (T2).
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool.clone());
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Opaque");
        kb.source_work_id = Some("wrk_src".to_string());
        kb.source_chapter = Some(7);
        kb.extensions_nexus_extras = Some(serde_json::json!({"edition": "alpha", "priority": 7}));
        let id = kb.entry_id.clone();

        // Build the opaque JSON the way the spoke adapter does (T2 path).
        let extensions_nexus_json = serde_json::to_string(&build_extensions_nexus(
            &kb.owner,
            kb.creator_only,
            kb.created_from_command_id.as_deref(),
            kb.source_work_id.as_deref(),
            kb.source_chapter,
            kb.source_provenance_kind.as_deref(),
            &nexus_extras_extension_map(kb.extensions_nexus_extras.as_ref()),
        ))
        .unwrap_or_default();

        let mut tx = pool.begin().await.unwrap();
        store
            .insert_key_block_with_extensions_in_tx(&mut tx, kb.clone(), extensions_nexus_json)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let fetched = store.get_knowledge_entry(&id).await.unwrap();
        assert_eq!(fetched.world_id(), Some("wld_1"));
        assert_eq!(fetched.source_work_id, Some("wrk_src".to_string()));
        assert_eq!(fetched.source_chapter, Some(7));
        let extras = fetched
            .extensions_nexus_extras
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .expect("unknown keys survive the opaque-JSON INSERT path");
        assert_eq!(extras["edition"], "alpha");
        assert_eq!(extras["priority"], 7);
    }

    #[tokio::test]
    async fn test_sqlite_extensions_nexus_extras_survive_update() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let mut kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Hero");
        kb.extensions_nexus_extras = Some(serde_json::json!({"edition": "alpha"}));
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb.clone()).await.unwrap();

        // RMW: read, modify the extras, write back via UPDATE.
        let mut fetched = store.get_knowledge_entry(&id).await.unwrap();
        fetched.extensions_nexus_extras =
            Some(serde_json::json!({"edition": "beta", "reviewer": "qc"}));
        fetched.updated_at = Some(chrono::Utc::now().to_rfc3339());
        store.update_knowledge_entry(fetched).await.unwrap();

        let after = store.get_knowledge_entry(&id).await.unwrap();
        let extras = after
            .extensions_nexus_extras
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .expect("unknown keys survive the UPDATE write path");
        assert_eq!(extras["edition"], "beta", "UPDATE re-serializes extras");
        assert_eq!(extras["reviewer"], "qc");
    }

    #[tokio::test]
    async fn test_sqlite_extensions_nexus_extras_none_when_absent() {
        // An entry inserted without extras has extensions_nexus_extras = None
        // on read (only the typed keys are serialized; no unknown keys present).
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let store = SqliteKbStore::new(pool);
        let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, "Plain");
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();

        let fetched = store.get_knowledge_entry(&id).await.unwrap();
        assert!(
            fetched.extensions_nexus_extras.is_none(),
            "no unknown keys → extras is None"
        );
    }

    // ── V1.154 P2 (R3 closure): world-aware CAS ──────────────────────────

    /// Seed a second world row so a test can FK-move a `kb_key_blocks` row
    /// across worlds (`world_id` is a NOT NULL FK to `narrative_worlds`).
    async fn seed_second_world(pool: &SqlitePool) {
        sqlx::query!(
            r#"INSERT INTO narrative_worlds
                (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json)
               VALUES ('wld_2', 'wrk_test', 'ctr_test', 'Second World', 'test-world-2', 'active', 'private', 'manual', '{}')"#
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// Seed a `kb_key_blocks` row in `wld_1` (revision NULL → 0 per the
    /// V1.73 NULL-normalization rule) and return its id.
    async fn seed_key_block(pool: &SqlitePool, canonical_name: &str) -> String {
        let store = SqliteKbStore::new(pool.clone());
        let kb = KnowledgeEntryRecord::new("wld_1", BlockType::Character, canonical_name);
        let id = kb.entry_id.clone();
        let mut tx = pool.begin().await.unwrap();
        store.insert_key_block_in_tx(&mut tx, kb).await.unwrap();
        tx.commit().await.unwrap();
        id
    }

    #[tokio::test]
    async fn cas_update_key_block_fields_same_world_matching_revision_succeeds() {
        // World-aware CAS happy path: the world bind matches the stored row,
        // so the CAS bumps revision exactly like the pre-R3 predicate.
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;
        let id = seed_key_block(&pool, "CasHappy").await;

        let mut tx = pool.begin().await.unwrap();
        let new_rev = cas_update_key_block_fields(
            &mut tx,
            &id,
            Some("CasHappy Renamed"),
            None,
            None,
            0,
            "wld_1",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(new_rev, 1, "CAS success bumps revision to expected + 1");
    }

    #[tokio::test]
    async fn cas_update_key_block_fields_rejects_row_in_foreign_world() {
        // R3 regression (atomic source of truth): the caller's world-verified
        // preimage (wld_1) is stale — a cross-process writer moved the row to
        // wld_2 without bumping the revision, so the pre-fix id+revision CAS
        // would have succeeded. The world-aware predicate must deny with
        // WorldConflict, NOT VersionMismatch (the caller's revision was valid;
        // the WORLD moved).
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;
        seed_second_world(&pool).await;
        let id = seed_key_block(&pool, "CasForeign").await;

        // "Other writer" (Connect process ∥ daemon) moves the row across
        // worlds between the gate-check and the CAS.
        sqlx::query!(
            "UPDATE kb_key_blocks SET world_id = ? WHERE key_block_id = ?",
            "wld_2",
            id,
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        let err =
            cas_update_key_block_fields(&mut tx, &id, Some("CasForeign"), None, None, 0, "wld_1")
                .await
                .unwrap_err();
        match err {
            LocalDbError::WorldConflict {
                table,
                id: err_id,
                expected_world,
                actual_world,
            } => {
                assert_eq!(table, "kb_key_blocks");
                assert_eq!(err_id, id);
                assert_eq!(expected_world, "wld_1");
                assert_eq!(actual_world, "wld_2");
            }
            other => panic!("world mismatch must classify as WorldConflict, got {other:?}"),
        }
    }

    async fn seed_character_key_block(pool: &SqlitePool, canonical_name: &str) -> String {
        // ToM-scoped CAS requires an admitted Character/binding owner.
        crate::ensure_creator_row(pool, "ctr_cas", "Cas").await.unwrap();
        sqlx::query(
            "INSERT OR IGNORE INTO characters \
             (character_id, owner_creator_id, display_name, status, image_uri, persona_json, \
              created_at, updated_at) \
             VALUES ('chr_cccccccccccccccccccccccccccccccc', 'ctr_cas', 'CasOwner', 'active', NULL, '{}', \
              '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        let store = SqliteKbStore::new(pool.clone());
        let kb = KnowledgeEntryRecord::for_character("chr_cccccccccccccccccccccccccccccccc", BlockType::Character, canonical_name);
        let id = kb.entry_id.clone();
        let mut tx = pool.begin().await.unwrap();
        store.insert_key_block_in_tx(&mut tx, kb).await.unwrap();
        tx.commit().await.unwrap();
        id
    }

    #[tokio::test]
    async fn cas_update_key_block_modules_in_tx_null_revision_reports_actual_zero() {
        // v1.184 P4 T1 fix (review I1): an existing row whose revision is
        // NULL (pre-bump legacy/seed shape) must report `actual: Some(0)` on
        // a stale CAS — matching the COALESCE(revision, 0) predicate and the
        // established cas_update_key_block_fields normalization — never the
        // `actual: None` "row absent" classification.
        let (pool, _dir) = fresh_pool().await;
        let id = seed_character_key_block(&pool, "NullRev").await;
        // Seed path leaves revision NULL (V1.73 NULL-normalization rule).
        let raw: Option<i64> =
            sqlx::query_scalar("SELECT revision FROM kb_key_blocks WHERE key_block_id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(raw.is_none(), "seed row must carry NULL revision");

        let mut tx = pool.begin().await.unwrap();
        let err = cas_update_key_block_modules_in_tx(
            &mut tx,
            &id,
            r#"{"belief":[]}"#,
            5,
            "chr_cccccccccccccccccccccccccccccccc",
            "awb_cas_unused",
        )
        .await
        .expect_err("stale expected revision must miss the CAS");
        let _ = tx.rollback().await;
        match err {
            LocalDbError::VersionMismatch { actual, expected, .. } => {
                assert_eq!(expected, 5);
                assert_eq!(actual, Some(0), "NULL revision normalizes to actual 0");
            }
            other => panic!("expected VersionMismatch, got {other:?}"),
        }

        // Happy path from the normalized 0 preimage succeeds.
        let mut tx = pool.begin().await.unwrap();
        let new_rev = cas_update_key_block_modules_in_tx(
            &mut tx,
            &id,
            r#"{"belief":[]}"#,
            0,
            "chr_cccccccccccccccccccccccccccccccc",
            "awb_cas_unused",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(new_rev, 1);
    }

    #[tokio::test]
    async fn cas_update_key_block_fields_same_world_stale_revision_stays_version_mismatch() {
        // A same-world stale revision keeps the existing OCC classification —
        // the world-aware predicate must not widen the WorldConflict bucket.
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;
        let id = seed_key_block(&pool, "CasStale").await;

        let mut tx = pool.begin().await.unwrap();
        let err =
            cas_update_key_block_fields(&mut tx, &id, Some("CasStale"), None, None, 5, "wld_1")
                .await
                .unwrap_err();
        assert!(
            matches!(err, LocalDbError::VersionMismatch { .. }),
            "same-world stale revision keeps VersionMismatch: {err:?}"
        );
    }
}
