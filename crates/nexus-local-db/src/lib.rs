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

mod schema;
mod version;

// Re-export version constants
pub use version::{DB_SCHEMA_VERSION, SCHEMA_VERSION};

/// Runtime role for database initialization
///
/// Determines which tables to initialize:
/// - `Cli`: Initialize shared tables only
/// - `Daemon`: Initialize shared tables + daemon-only tables
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

// Placeholder APIs - will be implemented in Phase B-E
// pub fn init(conn: &rusqlite::Connection, role: RuntimeRole) -> Result<(), Error>;
// pub fn migrate(conn: &rusqlite::Connection) -> Result<(), Error>;
// pub fn read_versions(conn: &rusqlite::Connection) -> Result<SchemaVersions, Error>;
// pub fn validate(conn: &rusqlite::Connection) -> Result<(), Error>;
