//! Nexus Local Database Module
//!
//! Single ownership of local SQLite (`state.db`) capabilities.
//! Provides unified API for CLI and daemon to initialize, migrate, and query local DB.
//!
//! ## Version Lines (Decoupled)
//!
//! - `db_schema_version`: Local SQLite structure version (managed by migrations)
//! - `schema_version`: Contract schema version (from nexus-contracts, network compatibility)
//!
//! See `.agents/plans/knowledge/local-db-refactor-v1.md` for design baseline.

pub mod identity;
pub mod memory_fragment;
pub mod pending_review;
pub mod soul_meta;

mod error;
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
    create_fragment, delete_fragment, get_all_keywords, list_fragments, list_fragments_by_session,
    MemoryFragmentRecord,
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
    /// Local database schema version (from workspace_meta table)
    pub db_schema_version: u32,
    /// Contract schema version (from nexus-contracts generated constants)
    pub schema_version: u32,
}

/// Open a SQLite connection pool at the given path.
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
pub async fn init_pool(db_path: &std::path::Path) -> Result<sqlx::SqlitePool, LocalDbError> {
    let pool = open_pool(db_path).await?;
    run_migrations(&pool).await?;
    seed_versions(&pool).await?;
    Ok(pool)
}
