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

mod error;
mod version;

// Re-export version constants
pub use version::{DB_SCHEMA_VERSION, SCHEMA_VERSION};

// Re-export error types
pub use error::LocalDbError;

// Re-export sqlx pool type for consumers
pub use sqlx::SqlitePool;

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
    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await?;
    Ok(pool)
}

/// Run all pending sqlx migrations from `./migrations/` directory.
///
/// Uses `sqlx::migrate!()` to locate and apply timestamped `.sql` files.
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
