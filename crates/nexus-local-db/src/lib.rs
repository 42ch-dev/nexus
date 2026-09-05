//! Nexus Local Database Module
//!
//! Single ownership of local `SQLite` (`state.db`) capabilities.
//! Provides unified API for CLI and daemon to initialize, migrate, and query local DB.
//!
//! ## Version Lines (Decoupled)
//!
//! - `db_schema_version`: Local `SQLite` structure version (managed by migrations)
//! - `schema_version`: Contract schema version (from nexus-contracts, network compatibility)
//!
//! See `.mstar/archived/knowledge/local-db-refactor-legacy.md` for design baseline.

// V1.153 P2 T2: cas is pure SQL (OCC helpers — no unix APIs); the former
// `#[cfg(unix)]` gate was wrong and broke `kb_relationships` (which imports
// `crate::cas`) on the Windows x64 build.
pub mod actor_world_binding;
pub mod cas;
pub mod character;
pub mod character_memory_fragment;
pub mod character_pending_review;
pub mod character_soul_meta;
pub mod character_soul_narrative;
pub mod compute_runs;
pub mod compute_session;
pub mod creators;
#[cfg(unix)]
pub mod file_lock;
pub mod findings;
pub mod force_gates_audit;
pub mod identity;
pub mod inspiration_items;
pub mod kb_extract_job;
pub mod kb_relationships;
pub mod kb_store;
pub mod knowledge_store;
pub mod memory_fragment;
pub mod mind_state_store;
pub mod moment_directive;
pub mod narrative_gateway;
pub mod narrative_write;
pub mod novel_pool_entries;
pub mod peer_hosts;
pub mod pending_review;
pub mod prompt_injection;
pub mod reading;
pub mod reference_source;
pub mod runtime_lock;
pub mod soul_meta;
pub mod soul_narrative;
pub mod spoke_rules;
pub mod work_chapters;
pub mod works;
pub mod workspace_session;
pub mod world_findings;
pub mod world_stories;

mod error;
mod seed_shared;
mod version;

// Test-only tracing-capture helpers shared by DAO mutation-path tests
// (R-V146P4-QC1-S1 / R-V146P4-QC3-S1). Compiled only under `cfg(test)`.
#[cfg(test)]
mod test_tracing;

use std::future::Future;

// Re-export version constants
pub use version::{DB_SCHEMA_VERSION, SCHEMA_VERSION};

// Re-export error types
pub use error::{ActorContractConflict, LocalDbError};

pub use actor_world_binding::{
    add_actor_world_binding, count_bindings_for_world_tx, list_bindings_for_character,
    mint_binding_id, remove_binding, ActorWorldBindingRecord, CreateBindingParams,
};
pub use character::{
    create_character_with_initial_binding, get_character, list_characters, mint_character_id,
    CharacterRecord, CreateCharacterParams, CreateCharacterResult,
};

// Re-export sqlx pool type for consumers
pub use sqlx::SqlitePool;

// Re-export identity types
pub use identity::{
    create_local_identity, delete_local_identity, get_local_identity, link_to_platform,
    list_local_identities, unlink_from_platform, LocalIdentityRow,
};

// Re-export creators types (V1.167 P2 T2)
pub use creators::ensure_creator_row;

// Re-export soul_meta types
pub use soul_meta::{
    delete as delete_soul_meta, get as get_soul_meta, upsert as upsert_soul_meta, SoulMeta,
};

// Re-export pending_review types
pub use pending_review::{
    count_pending_reviews, create_pending_review, delete_pending_review,
    delete_pending_review_in_tx, get_pending_review, list_pending_reviews, PendingReviewRecord,
};

// Re-export memory_fragment types
pub use memory_fragment::{
    count_fragments, create_fragment, delete_fragment, get_all_keywords, list_fragments,
    list_fragments_by_session, list_fragments_filtered, list_fragments_limited,
    MemoryFragmentRecord,
};

// Re-export soul_narrative types
pub use soul_narrative::{
    build_stats_fingerprint, get_soul_narrative, soul_narrative_fragment_stats,
    upsert_soul_narrative, SoulNarrativeFragmentStats, SoulNarrativeRecord,
};

// Re-export character memory types (v1.184 P3 Task 1)
pub use character_memory_fragment::{
    create_character_fragment, create_character_fragment_in_tx, delete_character_fragment,
    get_character_fragment, list_character_fragments, promote_character_fragment_to_shared,
    CharacterMemoryFragmentRecord, NewCharacterMemoryFragment,
};
pub use character_pending_review::{
    count_character_pending_reviews, create_character_pending_review,
    delete_character_pending_review, delete_character_pending_review_in_tx,
    get_character_pending_review, list_character_pending_reviews, CharacterPendingReviewRecord,
};
pub use character_soul_meta::{
    delete_character_soul_meta, get_character_soul_meta, upsert_character_soul_meta,
    CharacterSoulMeta,
};
pub use character_soul_narrative::{
    character_soul_narrative_fragment_stats, get_character_soul_narrative,
    upsert_character_soul_narrative, CharacterSoulNarrativeRecord,
};

/// Hard upper bound for Character memory list page sizes.
///
/// Every `list_*` read on the Character memory repositories clamps its `limit`
/// to `1..=MAX_CHARACTER_MEMORY_LIST_LIMIT` before issuing SQL, so a caller
/// cannot force unbounded materialization (SQLite treats `LIMIT -1` as
/// "no limit"). Follows the local-db pagination convention (clamp, as in
/// [`reference_source`](crate::reference_source)).
pub const MAX_CHARACTER_MEMORY_LIST_LIMIT: i64 = 500;

// Re-export mind_state_store types (V1.164 P2, l5-mind when-axis storage)
pub use mind_state_store::{
    delete_mind_state, get_mind_state, insert_mind_state, list_mind_states_by_holder, MindStateRow,
};

// Re-export world_stories types
pub use world_stories::{
    create_world_story, delete_world_story, generate_id as generate_world_story_id, get_by_id,
    list_by_world, update_chapters, update_status as update_world_story_status, WorldStory,
};

// Re-export narrative_write types
pub use narrative_write::{
    append_event, create_world, create_world_tx, AppendEventResult, CreateWorldResult,
    NarrativeWriteError,
};

// Re-export knowledge_store types
pub use knowledge_store::SqliteKnowledgeStore;

// Re-export kb_relationships types (V1.74 A2)
pub use kb_relationships::{
    delete_relationship_in_tx, generate_relationship_id, get_relationship,
    insert_relationship_in_tx, list_confirmed_relationships_paginated,
    list_relationships_for_world, update_relationship_in_tx, InsertRelationshipParams,
    KbRelationshipRow, RelationshipCursor, UpdateRelationshipParams,
};

// Re-export reference_source types
pub use reference_source::{
    find_by_id_for_creator as find_reference_by_id_for_creator, get_by_id as get_reference_by_id,
    list as list_references, register as register_reference, ReferenceSourceRow, RegisterParams,
    SourceMutability,
};

// Re-export kb_extract_job types
pub use kb_extract_job::{
    claim_job as claim_extract_job, enqueue as enqueue_extract_job,
    enqueue_with_artifact as enqueue_extract_job_with_artifact, get as get_extract_job,
    get_promotion as get_extract_promotion, insert_pending as insert_pending_extract,
    is_idempotent as is_extract_idempotent, list_by_creator as list_extract_jobs,
    list_pending_for_world as list_pending_extracts_for_world,
    mark_confirmed as mark_extract_confirmed, mark_done as mark_extract_job_done,
    mark_failed as mark_extract_job_failed, mark_rejected as mark_extract_rejected,
    mark_running as mark_extract_job_running, next_queued as next_queued_extract_job, KbExtractJob,
    KbExtractPromotion,
};

// Re-export prompt_injection types
pub use prompt_injection::{
    claim_prompt_injections, enqueue_prompt_injection, mark_prompt_injections_consumed,
    NewPromptInjection, PromptInjectionRow,
};

// Re-export peer_hosts types (V1.155 P0, N-C3 multi-host production)
pub use peer_hosts::{
    list_peer_manifests, record_peer_manifest, PeerHostRow, MAX_HOST_ID_CHARS,
    MAX_MANIFEST_JSON_BYTES, MAX_PEER_ID_CHARS,
};

// Re-export moment_directive types (V1.150 P1, DF-75)
pub use moment_directive::{
    clear as clear_moment_directive,
    clear_on_scene_change as clear_moment_directive_on_scene_change,
    decrement_ttl as decrement_moment_directive_ttl, get_active_for_work, get_active_for_world,
    get_by_id as get_moment_directive_by_id, replace_active as replace_moment_directive,
    set_active as set_moment_directive,
    update_lifecycle_anchor as update_moment_directive_lifecycle_anchor, MomentDirectiveRow,
    NewMomentDirective,
};

// Re-export findings types
pub use findings::{
    count_open_findings_by_severity, count_resolved_findings_older_than, create_finding,
    create_finding_from_review, create_finding_from_review_tx, delete_finding, get_finding,
    is_valid_status, is_valid_transition, list_findings, prune_resolved_findings_older_than,
    update_finding, Finding, FindingListFilters, FindingPatch, ReviewVerdictFinding, SeverityCount,
    ACTIONABLE_FINDING_STATUSES, RETENTION_DEFAULT_DAYS, VALID_STATUSES,
};

// Re-export works types
pub use works::{
    advance_work_stage_atomic, append_inspiration, count_works, create_work,
    find_work_by_client_request_id, get_work, has_active_fl_e_schedule, is_essay_profile,
    is_game_bible_profile, is_novel_profile, is_script_profile, list_works, patch_work,
    record_idempotency, InspirationLogEntry, WorkListFilters, WorkPatch, WorkRecord,
};

// Re-export novel_pool_entries types
pub use novel_pool_entries::{
    archive_pool_entry, count_pool_entries, get_active_pool_entry, get_pool_entry,
    get_pool_entry_by_work, list_pool_entries, mark_pool_entry_completed,
    mark_pool_entry_completed_for_work, promote_to_active, PoolEntry,
};

// Re-export inspiration_items types
pub use inspiration_items::{
    archive_inspiration, count_inspiration, create_inspiration_row,
    create_inspiration_with_scaffold, get_inspiration, inspiration_promote_atomic,
    list_inspiration, promote_inspiration, title_to_slug, InspirationItem,
};

// Re-export work_chapters types
pub use work_chapters::{
    apply_reconcile_diff, compute_reconcile_diff, count_chapters, get_chapter, insert_chapter,
    is_essay_complete, is_game_bible_design_complete, is_script_complete, is_work_completed,
    next_chapter, next_chapter_volume_aware, reconcile_from_filesystem, seed_chapters,
    seed_chapters_multi_volume, seed_chapters_multi_volume_tx, update_paths, update_status,
    InsertChapterParams, ReconcileDiff, ReconcileOp, ReconcileReport, WorkChapterRecord,
};

// Re-export force_gates_audit types
pub use force_gates_audit::{
    insert_force_gates_audit, list_force_gates_audit, prune_force_gates_audit_before,
    ForceGatesAuditParams, ForceGatesAuditRow,
};

// Re-export spoke_rules types (V1.148 P1; production CRUD V1.166 DR-64 AR-3)
pub use spoke_rules::{
    get_spoke_rules_by_ids, insert_rule, insert_spoke_rule_for_test, list_rules_by_world,
    list_rules_by_world_limited, set_rule_status, SpokeRuleRow,
};

// Re-export runtime_lock types (V1.42 P0)
pub use runtime_lock::{
    acquire_runtime_lock, clear_stale_lock, cli_holder, is_lock_stale, release_runtime_lock,
    schedule_holder, ttl_from_env, AcquireResult, DEFAULT_RUNTIME_LOCK_TTL_SECS,
};

// Re-export compute_session types (V1.146 P2 T2)
pub use compute_session::{
    delete_compute_session, get_compute_session, insert_compute_session,
    update_compute_session_state, ComputeSessionRow,
};

// Re-export workspace_session types (V1.56 P0 DF-31)
pub use workspace_session::{
    cleanup_expired_sessions, consume_session, count_active_sessions, create_session, get_session,
    ConsumeResult, CreateSessionParams, WorkspaceSessionRow,
};

/// Runtime role for database initialization
///
/// Determines which tables to initialize:
/// - `Cli`: Initialize shared tables only
/// - `Daemon`: Initialize shared + daemon-only tables
///
/// Post-WS8: table creation is no longer role-gated at init time;
/// all tables are created by migrations. Role gates **access** instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeRole {
    /// CLI runtime - shared tables only
    Cli,
    /// Daemon runtime - shared + daemon-only tables
    Daemon,
}

/// Schema version information
///
/// Contains both version lines for observability and health checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaVersions {
    /// Local database schema version (from `workspace_meta` table)
    pub db_schema_version: u32,
    /// Contract schema version (from nexus-contracts generated constants)
    pub schema_version: u32,
}

/// Open a `SQLite` connection pool at the given path.
///
/// Creates the database file if it does not exist (`mode=rwc`),
/// then sets recommended pragmas (WAL journal, foreign keys enabled).
///
/// # Example
///
/// ```rust,no_run
/// use nexus_local_db::open_pool;
///
/// #[tokio::main]
/// async fn main() {
///     let pool = open_pool(std::path::Path::new("state.db")).await.unwrap();
/// }
/// ```
///
/// Begin a write-serializing SQLite transaction (`BEGIN IMMEDIATE`).
///
/// # Errors
///
/// Returns `LocalDbError` if the connection cannot be acquired or the
/// immediate transaction cannot start.
pub async fn begin_immediate(
    pool: &sqlx::SqlitePool,
) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>, LocalDbError> {
    let conn = pool.acquire().await?;
    sqlx::Transaction::begin(conn, Some(std::borrow::Cow::Borrowed("BEGIN IMMEDIATE")))
        .await
        .map_err(LocalDbError::from)
}

/// # Errors
///
/// Returns `LocalDbError` if the connection pool cannot be created.
pub async fn open_pool(db_path: &std::path::Path) -> Result<sqlx::SqlitePool, LocalDbError> {
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .map_err(LocalDbError::from)?;
    // SAFETY: PRAGMA statement — no table schema to validate against.
    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&pool)
        .await?;
    // SAFETY: PRAGMA statement — no table schema to validate against.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await?;
    Ok(pool)
}

/// Open a read-only pool (`mode=ro`) for verification / read surfaces.
///
/// Used by the fully-converged no-op leg and read-only listing paths
/// (V1.176 P0 QC F-002 / S-003): unlike [`open_pool`], this never runs
/// migrations or `seed_versions`, so a converged read cannot write
/// `workspace_meta` keys or churn the db. No `PRAGMA journal_mode` is set —
/// that pragma requires write access; WAL-mode databases are still readable
/// through a read-only connection. Fails honestly when the file is absent,
/// locked, or corrupt.
///
/// # Errors
///
/// Returns `LocalDbError` if the connection pool cannot be created.
pub async fn open_pool_read_only(
    db_path: &std::path::Path,
) -> Result<sqlx::SqlitePool, LocalDbError> {
    let url = format!("sqlite://{}?mode=ro", db_path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .map_err(LocalDbError::from)?;
    // SAFETY: PRAGMA statement — no table schema to validate against.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await?;
    Ok(pool)
}

/// Run all pending sqlx migrations from `./migrations/` directory.
///
/// Embeds migration files at compile time via `sqlx::migrate!()`.
/// Idempotent — already-applied migrations are skipped.
///
/// # Example
///
/// ```rust,no_run
/// use nexus_local_db::{open_pool, run_migrations};
///
/// #[tokio::main]
/// async fn main() {
///     let pool = open_pool(std::path::Path::new("state.db")).await.unwrap();
///     run_migrations(&pool).await.unwrap();
/// }
/// ```
///
/// Table-rebuild migrations run with FK enforcement suspended for their
/// duration only, and a rebuild is recorded in `_sqlx_migrations` only after
/// an in-transaction `PRAGMA foreign_key_check` passes. On any failure the
/// migration connection is closed rather than returned to the pool, so a
/// pooled connection can never leak with `foreign_keys` OFF.
///
/// # Errors
///
/// Returns `LocalDbError` if any migration fails to apply.
pub async fn run_migrations(pool: &sqlx::SqlitePool) -> Result<(), LocalDbError> {
    let migrator = sqlx::migrate!("./migrations");
    let mut conn = pool.acquire().await.map_err(LocalDbError::from)?;
    match apply_pending_migrations(&mut conn, &migrator).await {
        Ok(()) => drop(conn),
        Err(err) => {
            // Never return a failed-migration connection to the pool: its
            // PRAGMA foreign_keys state is uncertain. Best-effort restore,
            // then close on drop so the pool replaces it either way.
            // SAFETY: PRAGMA statement — no table schema to validate against.
            let _ = sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&mut *conn)
                .await;
            conn.close_on_drop();
            drop(conn);
            return Err(err);
        }
    }

    // V1.67 P2 (W-001): SQLite's `PRAGMA foreign_key_check` returns rows for
    // violations but does not raise an error on its own. Consume the result set
    // and fail the migration if any violations remain.
    // SAFETY: PRAGMA diagnostic query — no table schema to validate against.
    let violations: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(pool)
        .await?;
    if !violations.is_empty() {
        return Err(LocalDbError::ConstraintViolation {
            table: "database".to_string(),
            constraint: format!(
                "PRAGMA foreign_key_check returned {} violation(s): {violations:?}",
                violations.len()
            ),
        });
    }

    Ok(())
}

/// Apply every pending migration of `migrator` on a single connection.
///
/// v1.184 P1 (Task 1 fix round 1): per-migration FK semantics.
///
/// Ordinary migrations keep sqlx's established behavior exactly: one
/// transaction per migration for the script plus the `_sqlx_migrations`
/// success row (via [`sqlx::migrate::Migrate::apply`]), with FK enforcement
/// ON. Only migrations whose file declares an FK-off window
/// (`PRAGMA foreign_keys=OFF` — table rebuilds such as
/// `20260905000002_actor_knowledge_owners.sql`) go through
/// [`apply_fk_suspension_migration`], which scopes the suspension to that one
/// migration and gates its success row on an in-transaction
/// `PRAGMA foreign_key_check`.
///
/// The FK-off window is needed at all because sqlx 0.8.6 wraps every
/// migration in a transaction and ignores the `-- no-transaction` directive,
/// so the file's own pragma is a no-op and a DROP/recreate would cascade
/// into child tables (`kb_source_anchors`, `kb_relationships`, `mind_states`,
/// `actor_world_bindings.world_sheet_entry_id`).
async fn apply_pending_migrations(
    conn: &mut sqlx::SqliteConnection,
    migrator: &sqlx::migrate::Migrator,
) -> Result<(), LocalDbError> {
    use sqlx::migrate::Migrate as _;

    // Deterministic baseline: ordinary migrations run with FK enforcement ON.
    // SQLite defaults a fresh connection to foreign_keys=OFF, and open_pool's
    // pragma only reaches the one pooled connection that executed it.
    // SAFETY: PRAGMA statement — no table schema to validate against.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await
        .map_err(sqlx::migrate::MigrateError::Execute)?;

    // Bookkeeping identical to Migrator::run_direct (ignore_missing = false):
    // fail on a dirty (partially applied) migration, then validate applied
    // versions/checksums against the source before applying anything new.
    conn.ensure_migrations_table().await?;
    if let Some(version) = conn.dirty_version().await? {
        return Err(sqlx::migrate::MigrateError::Dirty(version).into());
    }
    let applied = conn.list_applied_migrations().await?;
    for applied_migration in &applied {
        match migrator
            .iter()
            .find(|m| m.version == applied_migration.version)
        {
            None => {
                return Err(
                    sqlx::migrate::MigrateError::VersionMissing(applied_migration.version).into(),
                );
            }
            Some(known) if known.checksum != applied_migration.checksum => {
                return Err(sqlx::migrate::MigrateError::VersionMismatch(
                    applied_migration.version,
                )
                .into());
            }
            Some(_) => {}
        }
    }
    let applied_versions: std::collections::HashSet<i64> =
        applied.iter().map(|m| m.version).collect();

    for migration in migrator.iter() {
        if migration.migration_type.is_down_migration()
            || applied_versions.contains(&migration.version)
        {
            continue;
        }
        if requires_fk_suspension(migration) {
            apply_fk_suspension_migration(conn, migration).await?;
        } else {
            conn.apply(migration).await?;
        }
    }
    Ok(())
}

/// Whether the migration file declares an FK-off window
/// (`PRAGMA foreign_keys=OFF`), i.e. a table rebuild that must run with
/// enforcement suspended. Whitespace- and case-insensitive.
fn requires_fk_suspension(migration: &sqlx::migrate::Migration) -> bool {
    let normalized: String = migration
        .sql
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect();
    normalized.contains("PRAGMAFOREIGN_KEYS=OFF")
}

/// Apply a table-rebuild migration inside an FK-off window scoped to that
/// migration only.
///
/// The pragma cannot change inside a transaction, so enforcement is
/// suspended on the connection first and restored on every outcome. If the
/// restore itself fails, the error propagates and `run_migrations` closes
/// the connection instead of returning it to the pool.
async fn apply_fk_suspension_migration(
    conn: &mut sqlx::SqliteConnection,
    migration: &sqlx::migrate::Migration,
) -> Result<(), LocalDbError> {
    // SAFETY: PRAGMA statement — no table schema to validate against.
    //
    // Mapped to `MigrateError::Execute` so a transient failure at the pragma
    // seam keeps the stock runner's co-boot retry classification
    // (`is_transient_migration_error`), matching sqlx's `Migrate::apply`.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .map_err(sqlx::migrate::MigrateError::Execute)?;

    let result = apply_fk_suspension_tx(conn, migration).await;

    // SAFETY: PRAGMA statement — no table schema to validate against.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await
        .map_err(sqlx::migrate::MigrateError::Execute)?;

    result
}

/// Transactional core of [`apply_fk_suspension_migration`]: run the script,
/// gate the success row on an in-transaction `PRAGMA foreign_key_check`
/// (any violation rolls the rebuild back instead of recording it applied),
/// then insert the sqlx-shaped `_sqlx_migrations` success row so script and
/// bookkeeping commit together.
async fn apply_fk_suspension_tx(
    conn: &mut sqlx::SqliteConnection,
    migration: &sqlx::migrate::Migration,
) -> Result<(), LocalDbError> {
    use sqlx::{Connection as _, Executor as _};

    // `begin`/integrity-query/bookkeeping/commit errors are mapped to
    // `MigrateError::Execute` — exactly how sqlx's stock `Migrate::apply`
    // classifies them (`MigrateError::Execute(#[from] Error)`) — so a
    // transient SQLITE_BUSY or `_sqlx_migrations` UNIQUE race at any of
    // these seams keeps the existing single co-boot retry instead of
    // degrading to a non-retryable `LocalDbError::Sqlx`.
    let mut tx = conn
        .begin()
        .await
        .map_err(sqlx::migrate::MigrateError::Execute)?;
    let start = std::time::Instant::now();

    let outcome: Result<(), LocalDbError> = async {
        tx.execute(&*migration.sql)
            .await
            .map_err(|err| sqlx::migrate::MigrateError::ExecuteMigration(err, migration.version))?;

        // SAFETY: PRAGMA diagnostic query — no table schema to validate against.
        let violations: Vec<(String, i64, String, i64)> =
            sqlx::query_as("PRAGMA foreign_key_check")
                .fetch_all(&mut *tx)
                .await
                .map_err(sqlx::migrate::MigrateError::Execute)?;
        if !violations.is_empty() {
            return Err(LocalDbError::ConstraintViolation {
                table: "database".to_string(),
                constraint: format!(
                    "migration {} left {} foreign-key violation(s): {violations:?}",
                    migration.version,
                    violations.len()
                ),
            });
        }

        let execution_time = i64::try_from(start.elapsed().as_nanos()).unwrap_or(i64::MAX);
        sqlx::query(
            "INSERT INTO _sqlx_migrations \
             ( version, description, success, checksum, execution_time ) \
             VALUES ( ?1, ?2, TRUE, ?3, ?4 )",
        )
        .bind(migration.version)
        .bind(&*migration.description)
        .bind(&*migration.checksum)
        .bind(execution_time)
        .execute(&mut *tx)
        .await
        .map_err(sqlx::migrate::MigrateError::Execute)?;

        Ok(())
    }
    .await;

    match outcome {
        Ok(()) => tx
            .commit()
            .await
            .map_err(sqlx::migrate::MigrateError::Execute)?,
        Err(err) => {
            tx.rollback()
                .await
                .map_err(sqlx::migrate::MigrateError::Execute)?;
            return Err(err);
        }
    }
    Ok(())
}

/// Seed version keys into `workspace_meta` table.
///
/// Sets `db_schema_version` and `schema_version` (contract version) keys.
/// Safe to call on already-seeded databases (uses INSERT OR REPLACE).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn seed_versions(pool: &sqlx::SqlitePool) -> Result<(), LocalDbError> {
    let db_ver = DB_SCHEMA_VERSION.to_string();
    sqlx::query!(
        "INSERT OR REPLACE INTO workspace_meta (key, value) VALUES ('db_schema_version', ?)",
        db_ver
    )
    .execute(pool)
    .await?;
    let schema_ver = SCHEMA_VERSION.to_string();
    sqlx::query!(
        "INSERT OR REPLACE INTO workspace_meta (key, value) VALUES ('schema_version', ?)",
        schema_ver
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Read both version lines from the database.
///
/// Returns [`SchemaVersions`] containing `db_schema_version` and `schema_version`.
#[derive(Debug, Clone, sqlx::FromRow)]
struct WorkspaceMetaRow {
    value: String,
}

/// Read both version lines from the database.
///
/// Returns [`SchemaVersions`] containing `db_schema_version` and `schema_version`.
///
/// # Errors
///
/// Returns `LocalDbError` if version keys are missing or have invalid values.
pub async fn read_versions(pool: &sqlx::SqlitePool) -> Result<SchemaVersions, LocalDbError> {
    let row = sqlx::query_as!(
        WorkspaceMetaRow,
        "SELECT value FROM workspace_meta WHERE key = 'db_schema_version'"
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| LocalDbError::MissingVersionKey {
        key: "db_schema_version".to_string(),
    })?;

    let db_schema_version =
        row.value
            .parse::<u32>()
            .map_err(|e| LocalDbError::InvalidVersionValue {
                key: "db_schema_version".to_string(),
                value: row.value.clone(), // WS8 R1: use actual malformed value
                reason: e.to_string(),
            })?;

    let row = sqlx::query_as!(
        WorkspaceMetaRow,
        "SELECT value FROM workspace_meta WHERE key = 'schema_version'"
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| LocalDbError::MissingVersionKey {
        key: "schema_version".to_string(),
    })?;

    let schema_version =
        row.value
            .parse::<u32>()
            .map_err(|e| LocalDbError::InvalidVersionValue {
                key: "schema_version".to_string(),
                value: row.value.clone(), // WS8 R1: use actual malformed value
                reason: e.to_string(),
            })?;

    Ok(SchemaVersions {
        db_schema_version,
        schema_version,
    })
}

/// Validate database state for a given runtime role.
///
/// Checks that:
/// - `workspace_meta` table exists
/// - Both version keys are present and parseable
/// - `db_schema_version` matches the current expected version
///
/// Returns `Ok(())` if all checks pass, or an error describing what's wrong.
///
/// # Errors
///
/// Returns `LocalDbError` if version validation fails.
pub async fn validate(pool: &sqlx::SqlitePool, _role: RuntimeRole) -> Result<(), LocalDbError> {
    // Check workspace_meta table exists by reading a version key
    let versions = read_versions(pool).await?;

    if versions.db_schema_version != DB_SCHEMA_VERSION {
        return Err(LocalDbError::InvalidVersionValue {
            key: "db_schema_version".to_string(),
            value: versions.db_schema_version.to_string(),
            reason: format!(
                "expected {}, got {}",
                DB_SCHEMA_VERSION, versions.db_schema_version
            ),
        });
    }

    Ok(())
}

/// Backoff between the initial `run_migrations` attempt and the single retry.
///
/// Short enough that a losing boot path recovers quickly; long enough that the
/// winning process's migration transaction (a handful of small DDL statements)
/// has committed and its `_sqlx_migrations` rows are visible when the retry
/// re-reads the applied-versions list.
const MIGRATION_RETRY_BACKOFF: std::time::Duration = std::time::Duration::from_millis(300);

/// Run migrations with a single retry after [`MIGRATION_RETRY_BACKOFF`] when
/// the first attempt fails with a transient co-boot error; surface the error
/// unchanged if it still fails.
///
/// The migration runner is a parameter so the retry/backoff control flow is
/// deterministically testable with a simulated transient failure (production
/// passes [`run_migrations`]).
async fn run_migrations_with_retry<'p, F, Fut>(
    pool: &'p sqlx::SqlitePool,
    run_once: F,
) -> Result<(), LocalDbError>
where
    F: Fn(&'p sqlx::SqlitePool) -> Fut,
    Fut: Future<Output = Result<(), LocalDbError>>,
{
    match run_once(pool).await {
        Ok(()) => Ok(()),
        Err(e) if is_transient_migration_error(&e) => {
            tracing::warn!(
                error = %e,
                backoff_ms = MIGRATION_RETRY_BACKOFF.as_millis(),
                "transient migration failure during DB init (co-boot race); retrying once"
            );
            tokio::time::sleep(MIGRATION_RETRY_BACKOFF).await;
            run_once(pool).await
        }
        Err(e) => Err(e),
    }
}

/// Returns `true` when a migration failure is transient — i.e. caused by the
/// shared-DB co-boot race (P2 QC3 F-001) rather than by the migration SQL
/// itself. Only these errors are safe to retry: when two processes apply the
/// same pending migration on a fresh database, the loser fails in one of two
/// ways:
///
/// - `SQLITE_BUSY` (extended result codes 5 / 261 / 517 / 773) once the 5s
///   default `busy_timeout` expires while the other process holds the write
///   lock — surfaces from the migration body or the transaction commit
///   ([`MigrateError::ExecuteMigration`] / [`MigrateError::Execute`]);
/// - a UNIQUE constraint violation when both processes' bookkeeping inserts
///   collide on `_sqlx_migrations.version` ([`MigrateError::Execute`]).
///
/// Everything else (a SQL error inside a migration, version/checksum drift) is
/// permanent and must be surfaced immediately.
fn is_transient_migration_error(err: &LocalDbError) -> bool {
    let LocalDbError::Migrate(migrate_err) = err else {
        return false;
    };
    let (sqlx::migrate::MigrateError::Execute(sqlx_err)
    | sqlx::migrate::MigrateError::ExecuteMigration(sqlx_err, _)) = migrate_err
    else {
        return false;
    };
    // `DatabaseError::code()` is the SQLite extended result code formatted as
    // a string (SqliteError formats its `sqlite3_extended_errcode` value).
    let sqlx::Error::Database(db_err) = sqlx_err else {
        return false;
    };
    if db_err
        .code()
        .is_some_and(|code| matches!(code.as_ref(), "5" | "261" | "517" | "773"))
    {
        return true;
    }
    // Both processes applied the same migration; the loser's bookkeeping insert
    // violates the `_sqlx_migrations.version` UNIQUE constraint.
    db_err.is_unique_violation() && db_err.message().contains("_sqlx_migrations")
}

/// Convenience function: open pool, run migrations, and seed versions.
///
/// This is the recommended entry point for CLI and daemon initialization.
/// Equivalent to calling `open_pool` + `run_migrations` + `seed_versions` in sequence.
///
/// # Errors
///
/// Returns `LocalDbError` if any step (pool creation, migration, seeding) fails.
pub async fn init_pool(db_path: &std::path::Path) -> Result<sqlx::SqlitePool, LocalDbError> {
    let pool = open_pool(db_path).await?;
    // P2 QC3 F-001: shared-DB co-boot migration race. Both the daemon and the
    // runtime boot paths call `run_migrations` on the same database; on a
    // fresh DB two processes can apply the same pending migration concurrently
    // and the loser fails (SQLITE_BUSY at the write point, or a UNIQUE
    // violation on `_sqlx_migrations`). The single retry below waits for the
    // winner's migration transaction to commit, then re-runs migrations —
    // idempotent, so already-applied migrations are skipped and boot succeeds.
    // This covers only the narrow first-boot window; steady-state write
    // contention is handled by the pool's default busy_timeout. Deliberately
    // no distributed lock — out of scope for this fix.
    run_migrations_with_retry(&pool, run_migrations).await?;
    seed_versions(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    // V1.67 P2 (R-V160P1-QC2-W002): regression test that migrations leave the
    // database with no foreign-key violations. The 202606230001 table-recreate
    // migration now includes an explicit `PRAGMA foreign_key_check`; this test
    // would fail if any migration (including that one) introduced dangling FKs.
    #[tokio::test]
    async fn migrations_leave_no_foreign_key_violations() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();

        // SAFETY: PRAGMA diagnostic query — no table schema to validate against.
        let violations: Vec<(String, i64, String, i64)> =
            sqlx::query_as("PRAGMA foreign_key_check")
                .fetch_all(&pool)
                .await
                .unwrap();

        assert!(
            violations.is_empty(),
            "PRAGMA foreign_key_check returned violations: {violations:?}"
        );
    }

    // V1.67 P2 fix-wave 1 (W-001): regression test that `run_migrations` fails
    // hard when the database contains a foreign-key violation, rather than
    // leaving `PRAGMA foreign_key_check` as a diagnostic-only result.
    #[tokio::test]
    async fn migrations_fail_on_foreign_key_violation() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();

        // Introduce a dangling FK with foreign-key enforcement temporarily off.
        // Use a single acquired connection so the PRAGMA setting is respected by
        // the insert that follows.
        let mut conn = pool.acquire().await.unwrap();
        // SAFETY: PRAGMA statement — no table schema to validate against.
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut *conn)
            .await
            .unwrap();
        // SAFETY: test-only direct insert to create a deliberate violation.
        sqlx::query(
            "INSERT INTO kb_key_blocks \
             (key_block_id, world_id, block_type, canonical_name, status, body_json) \
             VALUES (?, ?, 'character', ?, 'provisional', ?)",
        )
        .bind("kb_violator")
        .bind("nonexistent_world")
        .bind("violator")
        .bind("{}")
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&mut *conn)
            .await
            .unwrap();
        drop(conn);

        let err = run_migrations(&pool).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("PRAGMA foreign_key_check returned 1 violation"),
            "expected FK-check failure, got: {msg}"
        );
    }

    // --- v1.184 P1 Task 1 fix round 1: migration FK-suspension safety ---

    /// Pool with exactly one connection and FK enforcement installed on every
    /// connection the pool opens (including replacements after a discard), so
    /// a poisoned FK-OFF connection handed back to the pool is observable.
    async fn single_conn_fk_pool(db_path: &std::path::Path) -> sqlx::SqlitePool {
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    // SAFETY: PRAGMA statement — no table schema to validate against.
                    sqlx::query("PRAGMA foreign_keys = ON")
                        .execute(conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap()
    }

    /// Migrator containing every migration shipped before the owner-scope
    /// rebuild (`20260905000002_actor_knowledge_owners.sql`).
    fn pre_owner_migrator() -> sqlx::migrate::Migrator {
        const OWNER_VERSION: i64 = 20_260_905_000_002;
        let full = sqlx::migrate!("./migrations");
        let pre: Vec<sqlx::migrate::Migration> = full
            .migrations
            .iter()
            .filter(|m| m.version < OWNER_VERSION)
            .cloned()
            .collect();
        sqlx::migrate::Migrator {
            migrations: std::borrow::Cow::Owned(pre),
            ignore_missing: false,
            locking: true,
            no_tx: false,
        }
    }

    /// Count of successful `_sqlx_migrations` rows for the owner migration.
    async fn recorded_owner_success(pool: &sqlx::SqlitePool) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM _sqlx_migrations \
             WHERE version = 20260905000002 AND success = TRUE",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Critical (task-1-review): a failing migration must never return a
    /// pooled connection with `foreign_keys` still OFF.
    #[tokio::test]
    async fn failed_migration_never_returns_fk_off_connection_to_pool() {
        let dir = tempfile::tempdir().unwrap();
        let pool = single_conn_fk_pool(&dir.path().join("test.db")).await;

        // Pre-upgrade schema: everything before the owner rebuild.
        {
            let mut conn = pool.acquire().await.unwrap();
            apply_pending_migrations(&mut conn, &pre_owner_migrator())
                .await
                .unwrap();
        }

        // Deterministic failure injection: a VIEW named kb_key_blocks_new
        // makes the owner migration's first statement (`DROP TABLE IF EXISTS
        // kb_key_blocks_new`) fail ("use DROP VIEW to delete view ...").
        sqlx::query("CREATE VIEW kb_key_blocks_new AS SELECT 1 AS x")
            .execute(&pool)
            .await
            .unwrap();

        run_migrations(&pool)
            .await
            .expect_err("owner migration must fail on the sabotage view");

        // The pool must not hand out the poisoned connection: FK enforcement
        // is ON and a dangling write is rejected.
        let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(fk, 1, "pooled connection leaked with foreign_keys OFF");

        // SAFETY: test-only direct insert attempting a deliberate violation.
        let dangling = sqlx::query(
            "INSERT INTO kb_key_blocks \
             (key_block_id, world_id, block_type, canonical_name, status, body_json) \
             VALUES ('kb_dangling', 'nonexistent_world', 'character', 'dangling', \
                     'provisional', '{}')",
        )
        .execute(&pool)
        .await;
        let err =
            dangling.expect_err("dangling FK write must be rejected after a failed migration");
        assert!(
            err.to_string().contains("FOREIGN KEY constraint failed"),
            "expected FK violation, got: {err}"
        );
    }

    /// Important (task-1-review): the FK-off window must be scoped to the
    /// rebuild migration only — a later, unrelated pending migration keeps
    /// full FK enforcement and is never recorded when it violates FKs.
    #[tokio::test]
    async fn fk_suspension_is_scoped_to_rebuild_migrations() {
        use sqlx::migrate::{Migration, MigrationType, Migrator};
        use std::borrow::Cow;

        let migrations = vec![
            Migration::new(
                1,
                Cow::Borrowed("create parent/child"),
                MigrationType::Simple,
                Cow::Borrowed(
                    "CREATE TABLE parent (id TEXT PRIMARY KEY);\n\
                     CREATE TABLE child (id TEXT PRIMARY KEY, p_id TEXT REFERENCES parent (id));",
                ),
                false,
            ),
            Migration::new(
                2,
                Cow::Borrowed("rebuild child"),
                MigrationType::Simple,
                Cow::Borrowed(
                    "-- no-transaction\n\
                     PRAGMA foreign_keys=OFF;\n\
                     CREATE TABLE child_new (id TEXT PRIMARY KEY, p_id TEXT REFERENCES parent (id));\n\
                     INSERT INTO child_new SELECT * FROM child;\n\
                     DROP TABLE child;\n\
                     ALTER TABLE child_new RENAME TO child;\n\
                     PRAGMA foreign_keys=ON;",
                ),
                true,
            ),
            Migration::new(
                3,
                Cow::Borrowed("dangling insert"),
                MigrationType::Simple,
                Cow::Borrowed("INSERT INTO child (id, p_id) VALUES ('c1', 'missing_parent');"),
                false,
            ),
        ];
        let migrator = Migrator {
            migrations: Cow::Owned(migrations),
            ignore_missing: false,
            locking: true,
            no_tx: false,
        };

        let dir = tempfile::tempdir().unwrap();
        let pool = single_conn_fk_pool(&dir.path().join("test.db")).await;
        let mut conn = pool.acquire().await.unwrap();
        let err = apply_pending_migrations(&mut conn, &migrator)
            .await
            .expect_err("the dangling-insert migration must be rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("FOREIGN KEY constraint failed"),
            "expected FK violation, got: {msg}"
        );
        drop(conn);

        // Migrations 1 and 2 are recorded; the violating migration 3 is not.
        let recorded: Vec<i64> = sqlx::query_scalar(
            "SELECT version FROM _sqlx_migrations WHERE success = TRUE ORDER BY version",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            recorded,
            vec![1, 2],
            "the FK-violating migration must not be recorded"
        );
    }

    /// Important (task-1-review): the owner rebuild must not be recorded
    /// successful when the post-rebuild integrity check fails; after cleaning
    /// up the dangling row a retry must apply it (self-healing install).
    #[tokio::test]
    async fn owner_migration_not_recorded_when_integrity_check_fails() {
        let dir = tempfile::tempdir().unwrap();
        let pool = single_conn_fk_pool(&dir.path().join("test.db")).await;

        {
            let mut conn = pool.acquire().await.unwrap();
            apply_pending_migrations(&mut conn, &pre_owner_migrator())
                .await
                .unwrap();
        }

        // Pre-existing dangling row: world_id with no narrative_worlds parent
        // (inserted with enforcement off, mirroring the W-001 fixture above).
        {
            let mut conn = pool.acquire().await.unwrap();
            // SAFETY: PRAGMA statement — no table schema to validate against.
            sqlx::query("PRAGMA foreign_keys = OFF")
                .execute(&mut *conn)
                .await
                .unwrap();
            // SAFETY: test-only direct insert to create a deliberate violation.
            sqlx::query(
                "INSERT INTO kb_key_blocks \
                 (key_block_id, world_id, block_type, canonical_name, status, body_json) \
                 VALUES ('kb_dangling', 'nonexistent_world', 'character', 'dangling', \
                         'provisional', '{}')",
            )
            .execute(&mut *conn)
            .await
            .unwrap();
            // SAFETY: PRAGMA statement — no table schema to validate against.
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&mut *conn)
                .await
                .unwrap();
        }

        run_migrations(&pool)
            .await
            .expect_err("dangling row must fail the owner migration");
        assert_eq!(
            recorded_owner_success(&pool).await,
            0,
            "owner migration must not be recorded successful with dangling child FKs"
        );

        // Self-healing: remove the dangling row; a retry applies the rebuild.
        sqlx::query("DELETE FROM kb_key_blocks WHERE key_block_id = 'kb_dangling'")
            .execute(&pool)
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        assert_eq!(
            recorded_owner_success(&pool).await,
            1,
            "retry after cleanup must apply the owner migration"
        );
    }

    /// Important (task-1-fix-2 / task-1-review): a transient failure at the
    /// custom FK-suspension bookkeeping seam must keep the co-boot retry
    /// classification. The loser of a first-boot race on a rebuild migration
    /// collides on `_sqlx_migrations.version` at the runner's bookkeeping
    /// INSERT; sqlx's stock `Migrate::apply` surfaces that as
    /// `MigrateError::Execute` (retryable), and the custom path must too —
    /// before this fix it degraded to a non-retryable `LocalDbError::Sqlx`.
    ///
    /// Deterministic injection: the rebuild script pre-inserts its own
    /// `_sqlx_migrations` success row, so the runner's bookkeeping INSERT for
    /// the same version deterministically violates the `version` PRIMARY KEY
    /// at the custom bookkeeping seam — no real second process needed. Every
    /// attempt fails identically, so `run_migrations_with_retry` must run
    /// exactly twice (initial attempt + single retry) and surface the error.
    #[tokio::test]
    async fn fk_suspension_bookkeeping_collision_keeps_co_boot_retry() {
        use sqlx::migrate::{Migration, MigrationType, Migrator};
        use std::borrow::Cow;

        let migrations = vec![Migration::new(
            1,
            Cow::Borrowed("rebuild with bookkeeping collision"),
            MigrationType::Simple,
            Cow::Borrowed(
                "-- no-transaction\n\
                 PRAGMA foreign_keys=OFF;\n\
                 CREATE TABLE rebuild_target (id TEXT PRIMARY KEY);\n\
                 INSERT INTO _sqlx_migrations \
                     (version, description, success, checksum, execution_time) \
                 VALUES (1, 'rebuild with bookkeeping collision', TRUE, X'00', 0);\n\
                 PRAGMA foreign_keys=ON;",
            ),
            true,
        )];
        let migrator = Migrator {
            migrations: Cow::Owned(migrations),
            ignore_missing: false,
            locking: true,
            no_tx: false,
        };
        let migrator_ref = &migrator;

        let dir = tempfile::tempdir().unwrap();
        let pool = single_conn_fk_pool(&dir.path().join("test.db")).await;
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let calls_ref = &calls;

        let err = run_migrations_with_retry(&pool, |p| async move {
            calls_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut conn = p.acquire().await?;
            apply_pending_migrations(&mut conn, migrator_ref).await
        })
        .await
        .expect_err("the injected bookkeeping collision must surface after the single retry");

        // The real SQLite error at the custom bookkeeping seam keeps the
        // co-boot signature (UNIQUE on `_sqlx_migrations.version`) …
        let msg = format!("{err}");
        assert!(
            msg.contains("UNIQUE constraint failed: _sqlx_migrations.version"),
            "expected the bookkeeping UNIQUE collision, got: {msg}"
        );
        // … and is classified transient, so the existing single retry ran.
        assert!(
            is_transient_migration_error(&err),
            "custom-path bookkeeping collision must stay transient, got: {err}"
        );
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "a transient custom-path failure must take the existing single retry"
        );

        // Atomicity preserved: both attempts rolled back, nothing recorded.
        let recorded: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = TRUE")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            recorded, 0,
            "the colliding rebuild must never be recorded successful"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'rebuild_target'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0,
            "the rolled-back rebuild script must leave no schema behind"
        );
    }

    // --- P2 QC3 F-001: shared-DB co-boot migration race (retry/backoff) ---
    //
    // The race itself (two processes applying the same pending migration on a
    // fresh database) is timing-dependent and cannot be reproduced
    // deterministically in a unit test: with the 5s default busy_timeout the
    // loser usually just waits and succeeds, and the failing window only opens
    // when a migration outlives the timeout or both bookkeeping inserts
    // collide. Instead, these tests drive the retry control flow directly by
    // injecting a simulated transient failure into `run_migrations_with_retry`
    // and pin the error-classification logic with fake `DatabaseError`s.

    /// Minimal `DatabaseError` stand-in so transient-error classification can
    /// be exercised without a real `SQLite` error. `kind()` mirrors
    /// `SqliteError::kind()`: UNIQUE/PRIMARY-KEY codes map to
    /// `UniqueViolation`, everything else (incl. `SQLITE_BUSY`) to `Other`.
    struct FakeDbError {
        code: Option<&'static str>,
        message: &'static str,
    }

    impl std::fmt::Display for FakeDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "FakeDbError(code={:?}, message={})",
                self.code, self.message
            )
        }
    }

    impl std::fmt::Debug for FakeDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            std::fmt::Display::fmt(self, f)
        }
    }

    impl std::error::Error for FakeDbError {}

    impl sqlx::error::DatabaseError for FakeDbError {
        fn message(&self) -> &str {
            self.message
        }

        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            self.code.map(std::borrow::Cow::Borrowed)
        }

        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }

        fn kind(&self) -> sqlx::error::ErrorKind {
            match self.code {
                Some("19" | "2067") => sqlx::error::ErrorKind::UniqueViolation,
                _ => sqlx::error::ErrorKind::Other,
            }
        }
    }

    fn fake_db_error(code: &'static str, message: &'static str) -> LocalDbError {
        LocalDbError::Migrate(sqlx::migrate::MigrateError::Execute(sqlx::Error::Database(
            Box::new(FakeDbError {
                code: Some(code),
                message,
            }),
        )))
    }

    #[test]
    fn transient_migration_error_classification() {
        // SQLITE_BUSY family (primary + extended result codes) → transient.
        for code in ["5", "261", "517", "773"] {
            let err = fake_db_error(code, "database is locked");
            assert!(
                is_transient_migration_error(&err),
                "busy code {code} should be classified as transient"
            );
        }

        // UNIQUE violation on `_sqlx_migrations` (both processes applied the
        // same migration; the loser's bookkeeping insert collides) → transient.
        let err = fake_db_error("2067", "UNIQUE constraint failed: _sqlx_migrations.version");
        assert!(is_transient_migration_error(&err));

        // A UNIQUE violation on a real table is NOT the co-boot signature.
        let err = fake_db_error("2067", "UNIQUE constraint failed: works.work_id");
        assert!(!is_transient_migration_error(&err));

        // A generic SQL error inside a migration is permanent.
        let err = fake_db_error("1", "no such column: oops");
        assert!(!is_transient_migration_error(&err));

        // Non-database sqlx errors and non-Migrate LocalDbErrors are not
        // transient.
        assert!(!is_transient_migration_error(&LocalDbError::Sqlx(
            sqlx::Error::RowNotFound
        )));
        assert!(!is_transient_migration_error(
            &LocalDbError::ValidationError("nope".into())
        ));
    }

    #[test]
    fn transient_migration_error_detected_from_migration_body_wrapper() {
        // The migration-body path wraps the error in
        // `ExecuteMigration(error, version)` rather than `Execute`; both must
        // be recognized.
        let err = LocalDbError::Migrate(sqlx::migrate::MigrateError::ExecuteMigration(
            sqlx::Error::Database(Box::new(FakeDbError {
                code: Some("5"),
                message: "database is locked",
            })),
            1,
        ));
        assert!(is_transient_migration_error(&err));
    }

    #[tokio::test]
    async fn init_migrations_retries_once_on_transient_error() {
        // Simulated co-boot loss: the first attempt fails with a busy-like
        // error; the retry runs the real migrations, which now succeed (as
        // they would once the winning process has committed).
        let dir = tempfile::tempdir().unwrap();
        let pool = open_pool(&dir.path().join("test.db")).await.unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let calls_ref = &calls;

        let started = std::time::Instant::now();
        let result = run_migrations_with_retry(&pool, |p| async move {
            if calls_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Err(fake_db_error("5", "database is locked"))
            } else {
                run_migrations(p).await
            }
        })
        .await;

        result.expect("retry should succeed");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(
            started.elapsed() >= MIGRATION_RETRY_BACKOFF,
            "retry must back off before the second attempt"
        );
    }

    #[tokio::test]
    async fn init_migrations_surfaces_error_after_single_retry() {
        // A persistently transient error is retried exactly once, then
        // surfaced.
        let dir = tempfile::tempdir().unwrap();
        let pool = open_pool(&dir.path().join("test.db")).await.unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let calls_ref = &calls;

        let err = run_migrations_with_retry(&pool, |_p| async move {
            calls_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(fake_db_error("5", "database is locked"))
        })
        .await
        .unwrap_err();

        assert!(is_transient_migration_error(&err));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn init_migrations_does_not_retry_non_transient_error() {
        // A permanent migration failure (e.g. a SQL error inside a migration)
        // must be surfaced immediately — no retry, no backoff.
        let dir = tempfile::tempdir().unwrap();
        let pool = open_pool(&dir.path().join("test.db")).await.unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let calls_ref = &calls;

        let err = run_migrations_with_retry(&pool, |_p| async move {
            calls_ref.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(fake_db_error("1", "no such column: oops"))
        })
        .await
        .unwrap_err();

        assert!(matches!(err, LocalDbError::Migrate(_)));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
