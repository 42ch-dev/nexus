//! Local database schema version constants
//!
//! DB schema version is independent from contract `schema_version`.
//! See `.mstar/archived/knowledge/local-db-refactor-legacy.md` for version line separation.

/// Current local database schema version
///
/// This version tracks `SQLite` structure migrations only.
/// Increment when adding new tables, columns, or modifying DDL.
// V1.148: 10 → 11 — spoke_rules table (RuleQueryPort production) + the V1.145–V1.147 schema additions that were also unbumped (compute_sessions direct-lane columns, kb_relationships.extensions_nexus_json, modules_json). Adopting a stricter per-structural-change bump going forward.
pub const DB_SCHEMA_VERSION: u32 = 11;

/// Contract schema version from generated wire types
///
/// Re-exported from nexus-contracts for convenience.
/// This tracks network contract compatibility, NOT local DB structure.
pub use nexus_contracts::generated::LATEST_SCHEMA_VERSION as SCHEMA_VERSION;
