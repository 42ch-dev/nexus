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

mod error;
mod identity;
mod memory_fragment;
mod migration;
mod pending_review;
mod schema;
mod soul_meta;
mod version;

// Re-export version constants
pub use version::{DB_SCHEMA_VERSION, SCHEMA_VERSION};

// Re-export error types
pub use error::LocalDbError;

// Re-export migration components
pub use migration::{get_migrations, run_migrations, Migration};

// Re-export schema components for direct access
pub use schema::{
    init_daemon_tables, init_shared_tables, migrate_wire_to_schema_version, seed_versions,
    ACP_SESSIONS_TABLE, ACP_TOOL_AUDIT_LOG_TABLE, AUTH_TOKENS_TABLE, CREATORS_TABLE,
    DEVICE_CODE_SESSIONS_TABLE, LOCAL_IDENTITIES_TABLE, MEMORY_FRAGMENTS_TABLE,
    MEMORY_PENDING_REVIEW_TABLE, OUTBOX_TABLE, PRAGMAS, REFERENCE_SOURCES_TABLE, SOUL_META_TABLE,
    WORKSPACE_META_TABLE,
};

// Re-export identity CRUD components
pub use identity::{
    create_local_identity, delete_local_identity, get_local_identity, link_to_platform,
    list_local_identities, unlink_from_platform, LocalIdentityRow,
};

// Re-export soul_meta CRUD components
pub use soul_meta::{
    delete as delete_soul_meta, get as get_soul_meta, upsert as upsert_soul_meta, SoulMeta,
};

// Re-export pending_review CRUD components
pub use pending_review::{
    count_pending_reviews, create_pending_review, delete_pending_review, get_pending_review,
    list_pending_reviews, PendingReviewRecord,
};

// Re-export memory_fragment CRUD components
pub use memory_fragment::{
    create_fragment, delete_fragment, get_all_keywords, list_fragments, list_fragments_by_session,
    MemoryFragmentRecord,
};

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

/// Initialize local database schema based on runtime role.
///
/// This function:
/// 1. Sets SQLite pragmas (journal_mode=WAL, foreign_keys=ON)
/// 2. Creates shared tables (workspace_meta, creators, reference_sources)
/// 3. Creates daemon-only tables if role == Daemon
/// 4. Seeds version keys (db_schema_version, schema_version)
/// 5. Migrates deprecated wire_schema_version key to schema_version
///
/// All operations use `IF NOT EXISTS` or `INSERT OR IGNORE` for idempotency.
/// Safe to call multiple times on an existing database.
///
/// # Example
///
/// ```rust,no_run
/// use nexus_local_db::{init, RuntimeRole};
/// use rusqlite::Connection;
///
/// fn main() -> Result<(), rusqlite::Error> {
///     let conn = Connection::open("state.db")?;
///     init(&conn, RuntimeRole::Cli)?;
///     Ok(())
/// }
/// ```
pub fn init(conn: &rusqlite::Connection, role: RuntimeRole) -> Result<(), rusqlite::Error> {
    // Initialize shared tables (both CLI and daemon)
    init_shared_tables(conn)?;

    // Initialize daemon-only tables if role is Daemon
    if role == RuntimeRole::Daemon {
        init_daemon_tables(conn)?;
    }

    // Seed version keys
    seed_versions(conn)?;

    // Migrate deprecated wire_schema_version to schema_version
    migrate_wire_to_schema_version(conn)?;

    Ok(())
}

/// Read schema version information from workspace_meta table.
///
/// Returns a `SchemaVersions` struct containing both version lines:
/// - `db_schema_version`: Local SQLite structure version (read from DB)
/// - `schema_version`: Contract schema version (from nexus-contracts constant)
///
/// # Errors
///
/// Returns `LocalDbError` if:
/// - `workspace_meta` table does not exist
/// - `db_schema_version` key is missing
/// - `schema_version` key is missing
/// - Version values are not valid u32 integers
///
/// # Example
///
/// ```rust,no_run
/// use nexus_local_db::{init, read_versions, RuntimeRole};
/// use rusqlite::Connection;
///
/// fn main() -> Result<(), nexus_local_db::LocalDbError> {
///     let conn = Connection::open("state.db")?;
///     init(&conn, RuntimeRole::Cli)?;
///     let versions = read_versions(&conn)?;
///     println!("DB schema version: {}", versions.db_schema_version);
///     println!("Contract schema version: {}", versions.schema_version);
///     Ok(())
/// }
/// ```
pub fn read_versions(conn: &rusqlite::Connection) -> Result<SchemaVersions, LocalDbError> {
    // Read db_schema_version from workspace_meta
    let db_schema_version_str: String = conn.query_row(
        "SELECT value FROM workspace_meta WHERE key = 'db_schema_version'",
        [],
        |row| row.get(0),
    )?;

    let db_schema_version: u32 =
        db_schema_version_str
            .parse()
            .map_err(|e| LocalDbError::InvalidVersionValue {
                key: "db_schema_version".to_string(),
                value: db_schema_version_str,
                reason: format!("failed to parse as u32: {}", e),
            })?;

    // schema_version comes from nexus-contracts constant, not DB
    // We verify the DB key exists but use the constant value
    let schema_version_str: String = conn.query_row(
        "SELECT value FROM workspace_meta WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    )?;

    // Validate that the stored value is a valid u32 (for health check purposes)
    let _: u32 = schema_version_str
        .parse()
        .map_err(|e| LocalDbError::InvalidVersionValue {
            key: "schema_version".to_string(),
            value: schema_version_str,
            reason: format!("failed to parse as u32: {}", e),
        })?;

    // Return schema_version from contracts constant (per design requirement)
    Ok(SchemaVersions {
        db_schema_version,
        schema_version: SCHEMA_VERSION,
    })
}

/// Validate database schema health and version integrity.
///
/// Performs the following checks:
/// 1. Verify `workspace_meta` table exists
/// 2. Verify `db_schema_version` key exists
/// 3. Verify `schema_version` key exists
/// 4. Verify version values are valid u32 integers
///
/// # Errors
///
/// Returns `LocalDbError` with descriptive message if any check fails.
/// Messages include actionable guidance for remediation.
///
/// # Example
///
/// ```rust,no_run
/// use nexus_local_db::{init, validate, RuntimeRole};
/// use rusqlite::Connection;
///
/// fn main() -> Result<(), nexus_local_db::LocalDbError> {
///     let conn = Connection::open("state.db")?;
///     init(&conn, RuntimeRole::Cli)?;
///     validate(&conn)?; // Returns Ok(()) if healthy
///     Ok(())
/// }
/// ```
pub fn validate(conn: &rusqlite::Connection) -> Result<(), LocalDbError> {
    // Check workspace_meta table exists
    let table_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workspace_meta')",
        [],
        |row| row.get(0),
    )?;

    if !table_exists {
        return Err(LocalDbError::MissingWorkspaceMetaTable);
    }

    // Check db_schema_version key exists
    let db_version_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM workspace_meta WHERE key = 'db_schema_version')",
        [],
        |row| row.get(0),
    )?;

    if !db_version_exists {
        return Err(LocalDbError::MissingVersionKey {
            key: "db_schema_version".to_string(),
        });
    }

    // Check schema_version key exists
    let schema_version_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM workspace_meta WHERE key = 'schema_version')",
        [],
        |row| row.get(0),
    )?;

    if !schema_version_exists {
        return Err(LocalDbError::MissingVersionKey {
            key: "schema_version".to_string(),
        });
    }

    // Validate db_schema_version is a valid u32
    let db_version_str: String = conn.query_row(
        "SELECT value FROM workspace_meta WHERE key = 'db_schema_version'",
        [],
        |row| row.get(0),
    )?;

    db_version_str
        .parse::<u32>()
        .map_err(|e| LocalDbError::InvalidVersionValue {
            key: "db_schema_version".to_string(),
            value: db_version_str,
            reason: format!("failed to parse as u32: {}", e),
        })?;

    // Validate schema_version is a valid u32
    let schema_version_str: String = conn.query_row(
        "SELECT value FROM workspace_meta WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    )?;

    schema_version_str
        .parse::<u32>()
        .map_err(|e| LocalDbError::InvalidVersionValue {
            key: "schema_version".to_string(),
            value: schema_version_str,
            reason: format!("failed to parse as u32: {}", e),
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn init_cli_creates_shared_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Cli).unwrap();

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
    fn init_daemon_creates_shared_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Daemon).unwrap();

        // Verify shared tables exist
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
    fn init_daemon_creates_daemon_only_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Daemon).unwrap();

        // Verify daemon-only tables exist
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
    fn init_cli_does_not_create_daemon_only_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Cli).unwrap();

        // Verify daemon-only tables do NOT exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .flatten()
            .collect();

        assert!(!tables.contains(&"outbox".to_string()));
        assert!(!tables.contains(&"auth_tokens".to_string()));
        assert!(!tables.contains(&"device_code_sessions".to_string()));
        assert!(!tables.contains(&"acp_tool_audit_log".to_string()));
        assert!(!tables.contains(&"acp_sessions".to_string()));
    }

    #[test]
    fn init_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Cli).unwrap();
        init(&conn, RuntimeRole::Cli).unwrap(); // second call should not fail

        // Verify versions are still correct
        let db_version: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'db_schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(db_version, DB_SCHEMA_VERSION.to_string());
    }

    #[test]
    fn init_seeds_schema_version_not_wire_schema_version() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Cli).unwrap();

        // Verify schema_version exists
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
    fn init_migrates_existing_wire_schema_version() {
        let conn = Connection::open_in_memory().unwrap();

        // Manually create tables and old key
        conn.execute_batch(WORKSPACE_META_TABLE).unwrap();
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('wire_schema_version', '42')",
            [],
        )
        .unwrap();

        // Call init - should migrate wire_schema_version to schema_version
        init(&conn, RuntimeRole::Cli).unwrap();

        // Verify schema_version exists
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
    fn pragmas_are_set() {
        use tempfile::TempDir;

        // WAL mode requires a persistent database file, not in-memory
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let conn = Connection::open(&db_path).unwrap();
        init(&conn, RuntimeRole::Cli).unwrap();

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

    // ============================================================================
    // read_versions() Tests (Phase D - Task 7)
    // ============================================================================

    #[test]
    fn read_versions_returns_correct_struct() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Cli).unwrap();

        let versions = read_versions(&conn).unwrap();

        assert_eq!(versions.db_schema_version, DB_SCHEMA_VERSION);
        assert_eq!(versions.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn read_versions_schema_version_from_constant() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Cli).unwrap();

        let versions = read_versions(&conn).unwrap();

        // schema_version MUST come from contracts constant, not from DB value
        // Even if DB has a different value, we return the constant
        assert_eq!(versions.schema_version, SCHEMA_VERSION);
        assert_eq!(
            versions.schema_version,
            nexus_contracts::generated::LATEST_SCHEMA_VERSION
        );
    }

    #[test]
    fn read_versions_fails_on_missing_table() {
        let conn = Connection::open_in_memory().unwrap();
        // Do NOT call init - table should not exist

        let result = read_versions(&conn);
        assert!(result.is_err());

        match result.unwrap_err() {
            LocalDbError::Rusqlite(_) => {} // Expected - query fails on missing table
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[test]
    fn read_versions_fails_on_missing_db_schema_version_key() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();
        // Do NOT seed versions

        let result = read_versions(&conn);
        assert!(result.is_err());

        match result.unwrap_err() {
            LocalDbError::Rusqlite(_) => {} // Expected - query fails on missing key
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[test]
    fn read_versions_fails_on_invalid_version_value() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

        // Insert invalid db_schema_version value (not a u32)
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('db_schema_version', 'not_a_number')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('schema_version', '1')",
            [],
        )
        .unwrap();

        let result = read_versions(&conn);
        assert!(result.is_err());

        match result.unwrap_err() {
            LocalDbError::InvalidVersionValue { key, value, reason } => {
                assert_eq!(key, "db_schema_version");
                assert_eq!(value, "not_a_number");
                assert!(reason.contains("failed to parse as u32"));
            }
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[test]
    fn read_versions_fails_on_invalid_schema_version_value() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

        // Insert valid db_schema_version but invalid schema_version
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('db_schema_version', '1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('schema_version', 'invalid')",
            [],
        )
        .unwrap();

        let result = read_versions(&conn);
        assert!(result.is_err());

        match result.unwrap_err() {
            LocalDbError::InvalidVersionValue { key, value, reason } => {
                assert_eq!(key, "schema_version");
                assert_eq!(value, "invalid");
                assert!(reason.contains("failed to parse as u32"));
            }
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    // ============================================================================
    // validate() Tests (Phase D - Task 8)
    // ============================================================================

    #[test]
    fn validate_returns_ok_for_healthy_database() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Cli).unwrap();

        let result = validate(&conn);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_fails_on_missing_workspace_meta_table() {
        let conn = Connection::open_in_memory().unwrap();
        // Do NOT call init - no tables exist

        let result = validate(&conn);
        assert!(result.is_err());

        match result.unwrap_err() {
            LocalDbError::MissingWorkspaceMetaTable => {} // Expected
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[test]
    fn validate_fails_on_missing_db_schema_version_key() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

        // Insert only schema_version, not db_schema_version
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('schema_version', '1')",
            [],
        )
        .unwrap();

        let result = validate(&conn);
        assert!(result.is_err());

        match result.unwrap_err() {
            LocalDbError::MissingVersionKey { key } => {
                assert_eq!(key, "db_schema_version");
            }
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[test]
    fn validate_fails_on_missing_schema_version_key() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

        // Insert only db_schema_version, not schema_version
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('db_schema_version', '1')",
            [],
        )
        .unwrap();

        let result = validate(&conn);
        assert!(result.is_err());

        match result.unwrap_err() {
            LocalDbError::MissingVersionKey { key } => {
                assert_eq!(key, "schema_version");
            }
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[test]
    fn validate_fails_on_invalid_db_schema_version_value() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

        // Insert invalid values
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('db_schema_version', 'invalid')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('schema_version', '1')",
            [],
        )
        .unwrap();

        let result = validate(&conn);
        assert!(result.is_err());

        match result.unwrap_err() {
            LocalDbError::InvalidVersionValue { key, value, reason } => {
                assert_eq!(key, "db_schema_version");
                assert_eq!(value, "invalid");
                assert!(reason.contains("failed to parse as u32"));
            }
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[test]
    fn validate_fails_on_invalid_schema_version_value() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

        // Insert invalid values
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('db_schema_version', '1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('schema_version', '-5')",
            [],
        )
        .unwrap();

        let result = validate(&conn);
        assert!(result.is_err());

        match result.unwrap_err() {
            LocalDbError::InvalidVersionValue { key, value, reason } => {
                assert_eq!(key, "schema_version");
                assert_eq!(value, "-5");
                assert!(reason.contains("failed to parse as u32"));
            }
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[test]
    fn validate_error_messages_are_descriptive() {
        let conn = Connection::open_in_memory().unwrap();
        // Do NOT call init - no tables exist

        let err = validate(&conn).unwrap_err();
        let msg = err.to_string();

        // Error message should include actionable guidance
        assert!(msg.contains("workspace_meta table does not exist"));
        assert!(msg.contains("call init() first"));
    }

    #[test]
    fn validate_missing_key_error_has_guidance() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

        let err = validate(&conn).unwrap_err();
        let msg = err.to_string();

        // Error message should include guidance for remediation
        assert!(msg.contains("required key"));
        assert!(msg.contains("call init() to seed version keys"));
    }
}
