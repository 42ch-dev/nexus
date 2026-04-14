//! Local database schema definitions (DDL)
//!
//! Single source of truth for all local SQLite tables.
//!
//! ## Table Classification
//!
//! **Shared Tables** (both CLI and daemon depend):
//! - `workspace_meta`: Key-value store for workspace-level metadata
//! - `creators`: Creator entity cache
//! - `reference_sources`: Reference material scan index and status
//!
//! **Daemon-only Tables**:
//! - `outbox`: Sync command queue for platform communication
//! - `auth_tokens`: OAuth token local storage
//! - `device_code_sessions`: Device authorization sessions
//! - `acp_tool_audit_log`: ACP tool invocation audit trail
//! - `acp_sessions`: ACP session persistence

/// SQLite pragmas recommended for workspace database
pub const PRAGMAS: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
"#;

/// Workspace metadata — key-value store for workspace-level settings.
///
/// Stores both version lines:
/// - `db_schema_version`: Local SQLite structure version
/// - `schema_version`: Contract schema version (from nexus-contracts)
pub const WORKSPACE_META_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS workspace_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now'))
);
"#;

/// Creator cache — stores registered Creator entities.
pub const CREATORS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS creators (
    creator_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    cached_at TEXT NOT NULL,
    data TEXT NOT NULL
);
"#;

/// Reference source registry — tracks scanned research references.
///
/// **V1.1 (CLI-R8)**: Added `content` column for extracted text.
/// This column MUST be present to fix drift between CLI and daemon schemas.
pub const REFERENCE_SOURCES_TABLE: &str = r#"
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

// ============================================================================
// Daemon-only Tables
// ============================================================================

/// Outbox queue — pending commands for platform sync.
///
/// Used by daemon to queue sync commands for the platform.
/// Commands are processed and removed after successful delivery.
pub const OUTBOX_TABLE: &str = r#"
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
///
/// Persists access and refresh tokens for authenticated users.
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
///
/// Used during device flow authentication to track pending sessions.
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
///
/// Provides audit trail for tool invocations by ACP agents.
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

/// ACP sessions — tracks active ACP agent sessions for persistence across CLI invocations.
///
/// Maintains session state for long-running ACP agent interactions.
pub const ACP_SESSIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS acp_sessions (
    session_id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_active TEXT NOT NULL,
    workspace_hint TEXT NOT NULL DEFAULT '',
    metadata TEXT NOT NULL DEFAULT '{}'
);
"#;

/// Local identities — stores local-only creator identities (anonymous and persistent).
///
/// Used in `local_only` mode (ADR-017) for identities that do not require platform registration.
/// Anonymous identities are ephemeral; persistent identities survive restarts.
/// All identities use `ctr_` prefix IDs matching the CreatorId pattern.
pub const LOCAL_IDENTITIES_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS local_identities (
    creator_id TEXT PRIMARY KEY,
    identity_type TEXT NOT NULL,
    display_name TEXT,
    created_at TEXT NOT NULL,
    platform_linked INTEGER NOT NULL DEFAULT 0,
    platform_creator_id TEXT
);
"#;

/// SOUL metadata — lightweight per-creator SOUL.md tracking.
///
/// Stores path, hashes, and timestamps for fast lookups without file I/O.
pub const SOUL_META_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS soul_meta (
    creator_id TEXT NOT NULL PRIMARY KEY,
    file_path TEXT NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,
    personality_hash TEXT,
    experience_hash TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#;

/// Initialize shared tables (used by both CLI and daemon).
///
/// Creates the three shared tables with `IF NOT EXISTS` for idempotency.
pub fn init_shared_tables(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(PRAGMAS)?;
    conn.execute_batch(WORKSPACE_META_TABLE)?;
    conn.execute_batch(CREATORS_TABLE)?;
    conn.execute_batch(REFERENCE_SOURCES_TABLE)?;
    conn.execute_batch(LOCAL_IDENTITIES_TABLE)?;
    Ok(())
}

/// Initialize daemon-only tables.
///
/// Creates the five daemon-specific tables with `IF NOT EXISTS` for idempotency.
/// Called by `init()` when role is `Daemon`.
pub fn init_daemon_tables(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(OUTBOX_TABLE)?;
    conn.execute_batch(AUTH_TOKENS_TABLE)?;
    conn.execute_batch(DEVICE_CODE_SESSIONS_TABLE)?;
    conn.execute_batch(ACP_TOOL_AUDIT_LOG_TABLE)?;
    conn.execute_batch(ACP_SESSIONS_TABLE)?;
    Ok(())
}

/// Seed version keys in workspace_meta.
///
/// Uses `INSERT OR IGNORE` for idempotency.
/// Replaces deprecated `wire_schema_version` with `schema_version`.
pub fn seed_versions(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    use crate::version::{DB_SCHEMA_VERSION, SCHEMA_VERSION};

    // Seed db_schema_version (local SQLite structure version)
    conn.execute(
        "INSERT OR IGNORE INTO workspace_meta (key, value) VALUES ('db_schema_version', ?1)",
        rusqlite::params![DB_SCHEMA_VERSION.to_string()],
    )?;

    // Seed schema_version (contract schema version)
    conn.execute(
        "INSERT OR IGNORE INTO workspace_meta (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![SCHEMA_VERSION.to_string()],
    )?;

    Ok(())
}

/// Migrate deprecated `wire_schema_version` key to `schema_version`.
///
/// This migration handles existing databases that still use the old key name.
/// Safe to run on new databases (no-op if key doesn't exist).
pub fn migrate_wire_to_schema_version(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    // Check if wire_schema_version exists
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM workspace_meta WHERE key = 'wire_schema_version')",
        [],
        |row| row.get(0),
    )?;

    if exists {
        // Rename wire_schema_version to schema_version
        // Use INSERT OR REPLACE to handle case where schema_version already exists
        conn.execute(
            "INSERT OR REPLACE INTO workspace_meta (key, value)
             SELECT 'schema_version', value FROM workspace_meta WHERE key = 'wire_schema_version'",
            [],
        )?;

        // Remove the old key
        conn.execute(
            "DELETE FROM workspace_meta WHERE key = 'wire_schema_version'",
            [],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DB_SCHEMA_VERSION, SCHEMA_VERSION};
    use rusqlite::Connection;

    #[test]
    fn init_shared_tables_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

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
        assert!(tables.contains(&"local_identities".to_string()));
    }

    #[test]
    fn init_shared_tables_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();
        init_shared_tables(&conn).unwrap(); // second call should not fail
    }

    #[test]
    fn reference_sources_has_content_column() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

        // Verify content column exists by attempting to use it
        conn.execute(
            "INSERT INTO reference_sources
             (reference_source_id, workspace_id, source_type, uri, title, content, scan_status, created_at)
             VALUES ('ref_test', 'local', 'pdf', 'test.pdf', 'Test', 'Extracted text', 'pending', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let content: Option<String> = conn
            .query_row(
                "SELECT content FROM reference_sources WHERE reference_source_id = 'ref_test'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(content, Some("Extracted text".to_string()));
    }

    #[test]
    fn local_identities_table_has_required_columns() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

        conn.execute(
            "INSERT INTO local_identities
             (creator_id, identity_type, display_name, created_at, platform_linked)
             VALUES ('ctr_localTest123', 'persistent', 'Test User', '2026-01-01T00:00:00Z', 0)",
            [],
        )
        .unwrap();

        let (identity_type, platform_linked): (String, i32) = conn
            .query_row(
                "SELECT identity_type, platform_linked FROM local_identities WHERE creator_id = 'ctr_localTest123'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(identity_type, "persistent");
        assert_eq!(platform_linked, 0);
    }

    #[test]
    fn seed_versions_inserts_keys() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();
        seed_versions(&conn).unwrap();

        let db_version: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'db_schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let schema_version: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(db_version, DB_SCHEMA_VERSION.to_string());
        assert_eq!(schema_version, SCHEMA_VERSION.to_string());
    }

    #[test]
    fn seed_versions_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();
        seed_versions(&conn).unwrap();
        seed_versions(&conn).unwrap(); // second call should not change values

        let db_version: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'db_schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        // Value should still be the first insertion
        assert_eq!(db_version, DB_SCHEMA_VERSION.to_string());
    }

    #[test]
    fn migrate_wire_to_schema_version_renames_key() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

        // Simulate old database with wire_schema_version
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('wire_schema_version', '42')",
            [],
        )
        .unwrap();

        migrate_wire_to_schema_version(&conn).unwrap();

        // Verify schema_version exists with correct value
        let schema_version: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(schema_version, "42");

        // Verify wire_schema_version is removed
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workspace_meta WHERE key = 'wire_schema_version')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(!exists);
    }

    #[test]
    fn migrate_wire_to_schema_version_no_op_on_new_db() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();
        seed_versions(&conn).unwrap();

        // Migrate on a new database (no wire_schema_version key)
        migrate_wire_to_schema_version(&conn).unwrap();

        // Verify schema_version is unchanged
        let schema_version: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(schema_version, SCHEMA_VERSION.to_string());
    }

    // ============================================================================
    // Daemon-only Tables Tests
    // ============================================================================

    #[test]
    fn init_daemon_tables_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init_daemon_tables(&conn).unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .flatten()
            .collect();

        assert!(tables.contains(&"outbox".to_string()));
        assert!(tables.contains(&"auth_tokens".to_string()));
        assert!(tables.contains(&"device_code_sessions".to_string()));
        assert!(tables.contains(&"acp_tool_audit_log".to_string()));
        assert!(tables.contains(&"acp_sessions".to_string()));
    }

    #[test]
    fn init_daemon_tables_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        init_daemon_tables(&conn).unwrap();
        init_daemon_tables(&conn).unwrap(); // second call should not fail
    }

    #[test]
    fn outbox_table_has_required_columns() {
        let conn = Connection::open_in_memory().unwrap();
        init_daemon_tables(&conn).unwrap();

        // Insert a row to verify column structure
        conn.execute(
            "INSERT INTO outbox (command_type, payload, status, created_at)
             VALUES ('sync', '{\"test\": true}', 'pending', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let (status, error): (String, Option<String>) = conn
            .query_row(
                "SELECT status, error FROM outbox WHERE command_type = 'sync'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(status, "pending");
        assert_eq!(error, None);
    }

    #[test]
    fn auth_tokens_table_has_required_columns() {
        let conn = Connection::open_in_memory().unwrap();
        init_daemon_tables(&conn).unwrap();

        conn.execute(
            "INSERT INTO auth_tokens (user_id, access_token, refresh_token, expires_at, created_at)
             VALUES ('user1', 'access123', 'refresh456', '2026-12-31T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let access_token: String = conn
            .query_row(
                "SELECT access_token FROM auth_tokens WHERE user_id = 'user1'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(access_token, "access123");
    }

    #[test]
    fn device_code_sessions_table_has_required_columns() {
        let conn = Connection::open_in_memory().unwrap();
        init_daemon_tables(&conn).unwrap();

        conn.execute(
            "INSERT INTO device_code_sessions (device_code, user_code, verification_uri, expires_at, status)
             VALUES ('device123', 'USER-CODE', 'https://example.com/verify', '2026-12-31T00:00:00Z', 'pending')",
            [],
        )
        .unwrap();

        let user_code: String = conn
            .query_row(
                "SELECT user_code FROM device_code_sessions WHERE device_code = 'device123'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(user_code, "USER-CODE");
    }

    #[test]
    fn acp_tool_audit_log_table_has_defaults() {
        let conn = Connection::open_in_memory().unwrap();
        init_daemon_tables(&conn).unwrap();

        // Insert with minimal required fields (created_at has default)
        conn.execute(
            "INSERT INTO acp_tool_audit_log (tool_name, path, outcome, agent_id, session_id)
             VALUES ('read_file', '/path/to/file', 'success', 'agent1', 'session1')",
            [],
        )
        .unwrap();

        // Verify created_at was set by default
        let created_at: String = conn
            .query_row(
                "SELECT created_at FROM acp_tool_audit_log WHERE tool_name = 'read_file'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        // Should be a datetime string, not empty
        assert!(!created_at.is_empty());
    }

    #[test]
    fn acp_sessions_table_has_defaults() {
        let conn = Connection::open_in_memory().unwrap();
        init_daemon_tables(&conn).unwrap();

        conn.execute(
            "INSERT INTO acp_sessions (session_id, agent_id, created_at, last_active)
             VALUES ('session1', 'agent1', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        // Verify defaults were applied
        let (workspace_hint, metadata): (String, String) = conn
            .query_row(
                "SELECT workspace_hint, metadata FROM acp_sessions WHERE session_id = 'session1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(workspace_hint, "");
        assert_eq!(metadata, "{}");
    }
}
