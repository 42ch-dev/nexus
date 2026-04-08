//! CLI Database Operations
//!
//! Provides database access for CLI commands.
//! Schema initialization is delegated to `nexus-local-db` module.
//!
//! **No duplicated DDL** - all shared table definitions are in `nexus-local-db`.

use nexus_local_db::{init, RuntimeRole};
use rusqlite::Connection;

/// Schema initializer for CLI-side database access.
///
/// Delegates to `nexus-local-db::init()` for shared tables.
/// Safe to call on an existing database — uses `IF NOT EXISTS`.
pub struct Schema;

impl Schema {
    /// Initialize CLI-side database schema.
    ///
    /// Creates tables used by CLI commands (creators, reference_sources,
    /// workspace_meta). Does NOT create daemon-only tables.
    /// Safe to call on an existing database — uses `IF NOT EXISTS`.
    pub fn init(conn: &Connection) -> Result<(), rusqlite::Error> {
        init(conn, RuntimeRole::Cli)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    /// Helper function to open or create workspace database for tests
    fn open_workspace_db(path: &std::path::Path) -> Result<Connection, rusqlite::Error> {
        let conn = Connection::open(path)?;
        Schema::init(&conn)?;
        Ok(conn)
    }

    #[test]
    fn schema_init_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        Schema::init(&conn).unwrap();

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
    }

    #[test]
    fn schema_init_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        Schema::init(&conn).unwrap();
        Schema::init(&conn).unwrap(); // second call should not fail
    }

    #[test]
    fn open_workspace_db_creates_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        // Remove the file to test creation
        std::fs::remove_file(path).unwrap();

        let conn = open_workspace_db(path).unwrap();

        // Verify file exists
        assert!(path.exists());

        // Verify schema initialized
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .flatten()
            .collect();

        assert!(tables.contains(&"workspace_meta".to_string()));
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
}
