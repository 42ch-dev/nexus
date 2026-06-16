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

pub mod findings;
pub mod force_gates_audit;
pub mod identity;
pub mod inspiration_items;
pub mod kb_extract_job;
pub mod kb_store;
pub mod knowledge_store;
pub mod memory_fragment;
pub mod narrative_gateway;
pub mod narrative_write;
pub mod novel_pool_entries;
pub mod pending_review;
pub mod prompt_injection;
pub mod reference_source;
pub mod runtime_lock;
pub mod soul_meta;
pub mod work_chapters;
pub mod works;
pub mod world_stories;

mod error;
mod seed_shared;
mod version;

// Re-export version constants
pub use version::{DB_SCHEMA_VERSION, SCHEMA_VERSION};

// Re-export error types
pub use error::LocalDbError;

// Re-export sqlx pool type for consumers
pub use sqlx::SqlitePool;

// Re-export identity types
pub use identity::{
    create_local_identity, delete_local_identity, get_local_identity, link_to_platform,
    list_local_identities, unlink_from_platform, LocalIdentityRow,
};

// Re-export soul_meta types
pub use soul_meta::{
    delete as delete_soul_meta, get as get_soul_meta, upsert as upsert_soul_meta, SoulMeta,
};

// Re-export pending_review types
pub use pending_review::{
    count_pending_reviews, create_pending_review, delete_pending_review, get_pending_review,
    list_pending_reviews, PendingReviewRecord,
};

// Re-export memory_fragment types
pub use memory_fragment::{
    count_fragments, create_fragment, delete_fragment, get_all_keywords, list_fragments,
    list_fragments_by_session, list_fragments_filtered, MemoryFragmentRecord,
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

// Re-export reference_source types
pub use reference_source::{
    get_by_id as get_reference_by_id, list as list_references, register as register_reference,
    ReferenceSourceRow, RegisterParams, SourceMutability,
};

// Re-export kb_extract_job types
pub use kb_extract_job::{
    claim_job as claim_extract_job, enqueue as enqueue_extract_job,
    enqueue_with_artifact as enqueue_extract_job_with_artifact, get as get_extract_job,
    list_by_creator as list_extract_jobs, mark_done as mark_extract_job_done,
    mark_failed as mark_extract_job_failed, mark_running as mark_extract_job_running,
    next_queued as next_queued_extract_job, KbExtractJob,
};

// Re-export prompt_injection types
pub use prompt_injection::{
    claim_prompt_injections, enqueue_prompt_injection, mark_prompt_injections_consumed,
    NewPromptInjection, PromptInjectionRow,
};

// Re-export findings types
pub use findings::{
    count_open_findings_by_severity, create_finding, create_finding_from_review,
    create_finding_from_review_tx, delete_finding, get_finding, is_valid_status,
    is_valid_transition, list_findings, prune_resolved_findings_older_than, update_finding,
    Finding, FindingListFilters, FindingPatch, ReviewVerdictFinding, SeverityCount,
    ACTIONABLE_FINDING_STATUSES, RETENTION_DEFAULT_DAYS, VALID_STATUSES,
};

// Re-export works types
pub use works::{
    advance_work_stage_atomic, append_inspiration, count_works, create_work,
    find_work_by_client_request_id, get_work, has_active_fl_e_schedule, list_works, patch_work,
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
    is_work_completed, next_chapter, next_chapter_volume_aware, reconcile_from_filesystem,
    seed_chapters, seed_chapters_multi_volume, seed_chapters_multi_volume_tx, update_paths,
    update_status, InsertChapterParams, ReconcileDiff, ReconcileOp, ReconcileReport,
    WorkChapterRecord,
};

// Re-export force_gates_audit types
pub use force_gates_audit::{
    insert_force_gates_audit, list_force_gates_audit, prune_force_gates_audit_before,
    ForceGatesAuditParams, ForceGatesAuditRow,
};

// Re-export runtime_lock types (V1.42 P0)
pub use runtime_lock::{
    acquire_runtime_lock, clear_stale_lock, cli_holder, is_lock_stale, release_runtime_lock,
    schedule_holder, ttl_from_env, AcquireResult, DEFAULT_RUNTIME_LOCK_TTL_SECS,
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
/// # Errors
///
/// Returns `LocalDbError` if any migration fails to apply.
pub async fn run_migrations(pool: &sqlx::SqlitePool) -> Result<(), LocalDbError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(LocalDbError::from)?;
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
    run_migrations(&pool).await?;
    seed_versions(&pool).await?;
    Ok(pool)
}
