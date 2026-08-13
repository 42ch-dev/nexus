//! Local database schema version constants
//!
//! DB schema version is independent from contract `schema_version`.
//! See `.mstar/archived/knowledge/local-db-refactor-legacy.md` for version line separation.

/// Current local database schema version
///
/// This version tracks `SQLite` structure migrations only.
/// Increment when adding new tables, columns, or modifying DDL.
// V1.148: 10 → 11 — spoke_rules table (RuleQueryPort production) + the V1.145–V1.147 schema additions that were also unbumped (compute_sessions direct-lane columns, kb_relationships.extensions_nexus_json, modules_json). Adopting a stricter per-structural-change bump going forward.
// V1.150: 11 → 12 — moment_directives table (Moment Directive, DF-75 P1).
// V1.150 (residual close): 12 → 13 — moment_directives rebuild dropping last_chapter_no + new moment_directive_chapter_anchors table (R-V1150P2-004/R-V1150P2-008: per-(directive, work) chapter delta TTL burn).
// V1.155: 13 → 14 — peer_hosts table (N-C3 multi-host production, P0 T1).
// V1.158: 14 → 15 — peer_hosts.last_peer_id column (R-V1155P0-001 peer_id session recording, P2 T2).
// V1.163: 15 → 16 — legacy outbox table DROP (DR-08; migration 20260812_drop_legacy_outbox.sql).
pub const DB_SCHEMA_VERSION: u32 = 16;

/// Contract schema version from generated wire types
///
/// Re-exported from nexus-contracts for convenience.
/// This tracks network contract compatibility, NOT local DB structure.
pub use nexus_contracts::generated::LATEST_SCHEMA_VERSION as SCHEMA_VERSION;
