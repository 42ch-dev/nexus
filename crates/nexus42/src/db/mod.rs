//! CLI Database Schema
//!
//! Canonical schema is maintained in `nexus42d/src/db/schema.rs` (daemon owns
//! the database). This module provides the same definitions for CLI-side
//! operations that touch `state.db` directly.
//!
//! **Keep in sync with the daemon schema.** Any column addition/removal in the
//! daemon must be mirrored here.

use nexus_contracts::generated::LATEST_SCHEMA_VERSION;
use rusqlite::Connection;

/// Database schema version for local SQLite migrations.
/// Must match `nexus42d::db::schema::DB_SCHEMA_VERSION`.
pub const DB_SCHEMA_VERSION: u32 = 1;

/// Wire contract schema version for network payload compatibility.
/// Sourced from generated contracts to avoid manual drift.
pub const WIRE_SCHEMA_VERSION: u32 = LATEST_SCHEMA_VERSION;

/// Schema initializer for CLI-side database access.
///
/// Only creates the tables the CLI needs. Safe to call multiple times.
pub struct Schema;

impl Schema {
    /// Initialize the CLI-side database schema.
    ///
    /// Creates tables used by CLI commands (creators, reference_sources,
    /// workspace_meta). Does NOT create daemon-only tables (outbox).
    /// Safe to call on an existing database — uses `IF NOT EXISTS`.
    pub fn init(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(WORKSPACE_META_TABLE)?;
        conn.execute_batch(CREATORS_TABLE)?;
        conn.execute_batch(REFERENCE_SOURCES_TABLE)?;

        // V1.1 migration: Add content column if it doesn't exist
        let _ = conn.execute("ALTER TABLE reference_sources ADD COLUMN content TEXT", []);

        // Seed schema version rows (idempotent).
        conn.execute(
            "INSERT OR IGNORE INTO workspace_meta (key, value) VALUES ('db_schema_version', ?1)",
            rusqlite::params![DB_SCHEMA_VERSION.to_string()],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO workspace_meta (key, value) VALUES ('wire_schema_version', ?1)",
            rusqlite::params![WIRE_SCHEMA_VERSION.to_string()],
        )?;

        Ok(())
    }
}

/// Workspace metadata — key-value store for workspace-level settings.
/// Keep in sync with daemon schema.
const WORKSPACE_META_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS workspace_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now'))
);
"#;

/// Creator cache — stores registered Creator entities.
/// Keep in sync with daemon schema.
const CREATORS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS creators (
    creator_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    cached_at TEXT NOT NULL,
    data TEXT NOT NULL
);
"#;

/// Reference source registry — tracks scanned research references.
/// Keep in sync with daemon schema.
///
/// V1.1 (CLI-R8): Added `content` column for extracted text.
const REFERENCE_SOURCES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS reference_sources (
    reference_source_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL DEFAULT 'local',
    source_type TEXT NOT NULL,
    uri TEXT NOT NULL,
    title TEXT NOT NULL,
    tags TEXT,
    content_hash TEXT,
    content TEXT,
    scan_status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT
);
"#;
