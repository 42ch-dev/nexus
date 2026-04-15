//! Local database schema version constants
//!
//! DB schema version is independent from contract schema_version.
//! See `.agents/plans/knowledge/local-db-refactor-v1.md` for version line separation.

/// Current local database schema version
///
/// This version tracks SQLite structure migrations only.
/// Increment when adding new tables, columns, or modifying DDL.
pub const DB_SCHEMA_VERSION: u32 = 4;

/// Contract schema version from generated wire types
///
/// Re-exported from nexus-contracts for convenience.
/// This tracks network contract compatibility, NOT local DB structure.
pub use nexus_contracts::generated::LATEST_SCHEMA_VERSION as SCHEMA_VERSION;
