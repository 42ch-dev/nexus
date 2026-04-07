//! Canonical Database Schema
//!
//! This module is the **single source of truth** for all SQLite table
//! definitions used by the Nexus daemon. Both the daemon and CLI share
//! the same `state.db` file; CLI-side definitions live in
//! `crates/nexus42/src/db/schema.rs` and must be kept in sync.

use rusqlite::Connection;

/// Current schema version, stored in `workspace_meta` as `schema_version`.
pub const SCHEMA_VERSION: &str = "1";

/// Schema initializer. All table definitions are centralized here.
pub struct Schema;

impl Schema {
    /// Initialize the database schema.
    ///
    /// Safe to call multiple times — every statement uses `IF NOT EXISTS`.
    /// Also sets recommended SQLite pragmas.
    pub fn init(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(PRAGMAS)?;
        conn.execute_batch(WORKSPACE_META_TABLE)?;
        conn.execute_batch(CREATORS_TABLE)?;
        conn.execute_batch(REFERENCE_SOURCES_TABLE)?;
        conn.execute_batch(OUTBOX_TABLE)?;
        conn.execute_batch(AUTH_TOKENS_TABLE)?;
        conn.execute_batch(DEVICE_CODE_SESSIONS_TABLE)?;
        conn.execute_batch(ACP_TOOL_AUDIT_LOG_TABLE)?;

        // Seed schema version row (idempotent)
        conn.execute(
            "INSERT OR IGNORE INTO workspace_meta (key, value) VALUES ('schema_version', ?1)",
            rusqlite::params![SCHEMA_VERSION],
        )?;

        Ok(())
    }
}

/// SQLite pragmas recommended for the workspace database.
const PRAGMAS: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
"#;

/// Workspace metadata — key-value store for workspace-level settings.
const WORKSPACE_META_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS workspace_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now'))
);
"#;

/// Creator cache — stores registered Creator entities.
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
const REFERENCE_SOURCES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS reference_sources (
    reference_source_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL DEFAULT 'local',
    source_type TEXT NOT NULL,
    uri TEXT NOT NULL,
    title TEXT NOT NULL,
    tags TEXT,
    content_hash TEXT,
    scan_status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT
);
"#;

/// Outbox queue — pending commands for platform sync.
const OUTBOX_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    command_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    sent_at TEXT,
    error TEXT
);
"#;

/// Auth tokens — stores OAuth tokens for user authentication.
pub const AUTH_TOKENS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS auth_tokens (
    user_id TEXT PRIMARY KEY,
    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);
"#;

/// Device code sessions — tracks OAuth device authorization grants.
pub const DEVICE_CODE_SESSIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS device_code_sessions (
    device_code TEXT PRIMARY KEY,
    user_code TEXT NOT NULL,
    verification_uri TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
);
"#;

/// ACP tool audit log — records all agent tool executions through daemon.
pub const ACP_TOOL_AUDIT_LOG_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS acp_tool_audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_name TEXT NOT NULL,
    path TEXT NOT NULL,
    outcome TEXT NOT NULL,
    agent_id TEXT,
    session_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_init_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        Schema::init(&conn).unwrap();

        // Verify each table exists
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .flatten()
            .collect();

        assert!(tables.contains(&"workspace_meta".to_string()));
        assert!(tables.contains(&"creators".to_string()));
        assert!(tables.contains(&"reference_sources".to_string()));
        assert!(tables.contains(&"outbox".to_string()));
        assert!(tables.contains(&"auth_tokens".to_string()));
    }

    #[test]
    fn schema_init_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        Schema::init(&conn).unwrap();
        Schema::init(&conn).unwrap(); // second call should not fail
    }

    #[test]
    fn schema_version_seeded() {
        let conn = Connection::open_in_memory().unwrap();
        Schema::init(&conn).unwrap();

        let version: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn creators_table_has_default_status() {
        let conn = Connection::open_in_memory().unwrap();
        Schema::init(&conn).unwrap();

        // Insert a row without specifying status — should default to 'active'
        conn.execute(
            "INSERT INTO creators (creator_id, display_name, cached_at, data)
             VALUES ('ctr_test', 'Test', '2026-01-01T00:00:00Z', '{}')",
            [],
        )
        .unwrap();

        let status: String = conn
            .query_row(
                "SELECT status FROM creators WHERE creator_id = 'ctr_test'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(status, "active");
    }

    #[test]
    fn reference_sources_table_has_tags_and_content_hash() {
        let conn = Connection::open_in_memory().unwrap();
        Schema::init(&conn).unwrap();

        // Insert with tags and content_hash columns
        conn.execute(
            "INSERT INTO reference_sources
             (reference_source_id, workspace_id, source_type, uri, title, tags, content_hash, scan_status, created_at)
             VALUES ('ref_test', 'local', 'pdf', 'test.pdf', 'Test', 'tag1,tag2', 'abc123', 'pending', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let (tags, hash): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT tags, content_hash FROM reference_sources WHERE reference_source_id = 'ref_test'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(tags, Some("tag1,tag2".to_string()));
        assert_eq!(hash, Some("abc123".to_string()));
    }
}
