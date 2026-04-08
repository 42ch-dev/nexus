//! Daemon Database Schema
//!
//! Delegates all schema initialization to `nexus-local-db` module.
//! No DDL definitions remain in this file — all tables are centrally managed.
//!
//! **All tables** (shared + daemon-only):
//! - Initialized by `nexus-local-db::init(RuntimeRole::Daemon)`
//! - Single source of truth in `nexus-local-db/src/schema.rs`

use nexus_local_db::{init, RuntimeRole};
use rusqlite::Connection;

/// Schema initializer for daemon runtime.
///
/// Delegates to `nexus-local-db::init()` for all table creation.
/// Safe to call multiple times — uses `IF NOT EXISTS`.
pub struct Schema;

impl Schema {
    /// Initialize the daemon database schema.
    ///
    /// Calls `nexus-local-db::init(RuntimeRole::Daemon)` which creates
    /// all tables (shared + daemon-only) and seeds version keys.
    pub fn init(conn: &Connection) -> Result<(), rusqlite::Error> {
        // Delegate to nexus-local-db (shared + daemon-only tables)
        init(conn, RuntimeRole::Daemon)?;
        Ok(())
    }
}

// All DDL moved to nexus-local-db/src/schema.rs
// Daemon-only table constants can be imported from nexus_local_db if needed:
// - nexus_local_db::OUTBOX_TABLE
// - nexus_local_db::AUTH_TOKENS_TABLE
// - nexus_local_db::DEVICE_CODE_SESSIONS_TABLE
// - nexus_local_db::ACP_TOOL_AUDIT_LOG_TABLE
// - nexus_local_db::ACP_SESSIONS_TABLE

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_local_db::SCHEMA_VERSION;
    use rusqlite::Connection;

    #[test]
    fn schema_init_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
        Schema::init(&conn).unwrap();

        // Verify all tables exist (shared + daemon-only)
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .flatten()
            .collect();

        // Shared tables
        assert!(tables.contains(&"workspace_meta".to_string()));
        assert!(tables.contains(&"creators".to_string()));
        assert!(tables.contains(&"reference_sources".to_string()));

        // Daemon-only tables
        assert!(tables.contains(&"outbox".to_string()));
        assert!(tables.contains(&"auth_tokens".to_string()));
        assert!(tables.contains(&"device_code_sessions".to_string()));
        assert!(tables.contains(&"acp_tool_audit_log".to_string()));
        assert!(tables.contains(&"acp_sessions".to_string()));
    }

    #[test]
    fn schema_init_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        Schema::init(&conn).unwrap();
        Schema::init(&conn).unwrap(); // second call should not fail
    }

    #[test]
    fn schema_versions_seeded_correctly() {
        let conn = Connection::open_in_memory().unwrap();
        Schema::init(&conn).unwrap();

        // Verify db_schema_version (local SQLite structure)
        let db_version: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'db_schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(db_version, nexus_local_db::DB_SCHEMA_VERSION.to_string());

        // Verify schema_version (contract schema version)
        let schema_version: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(schema_version, SCHEMA_VERSION.to_string());

        // Verify wire_schema_version does NOT exist
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
    fn reference_sources_has_content_column() {
        let conn = Connection::open_in_memory().unwrap();
        Schema::init(&conn).unwrap();

        // Verify content column exists (drift fix validation)
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

    #[test]
    fn pragmas_are_set() {
        use tempfile::TempDir;

        // WAL mode requires a persistent database file, not in-memory
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let conn = Connection::open(&db_path).unwrap();
        Schema::init(&conn).unwrap();

        // Verify journal_mode is WAL
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();

        assert_eq!(journal_mode, "wal");

        // Verify foreign_keys is ON (returns integer 0 or 1)
        let foreign_keys: i32 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();

        assert_eq!(foreign_keys, 1);
    }
}
