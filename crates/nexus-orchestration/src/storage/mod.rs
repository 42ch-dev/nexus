//! SQLite-backed [`graph_flow::SessionStorage`] implementation.
//!
//! Reuses the shared `Arc<SqlitePool>` from `nexus-local-db` (post-WS8).
//! The `orchestration_sessions` table is created by migration
//! `crates/nexus-local-db/migrations/<N>_orchestration_sessions.sql`.
//!
//! Design: `.agents/knowledge/specs/orchestration-engine.md` §4.3.

pub mod sqlite;

pub use sqlite::SqliteSessionStorage;
