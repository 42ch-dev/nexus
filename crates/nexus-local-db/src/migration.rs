//! Migration runner for local database schema evolution
//!
//! Provides sequential migration execution with version ordering.
//! Migrations are idempotent and abort on failure without corrupting version.
//!
//! ## Migration Strategy
//!
//! 1. Read current `db_schema_version`
//! 2. Execute pending migrations in order (vN → vN+1)
//! 3. Update version after each successful migration
//! 4. Abort on failure, preserve old version
//!
//! ## Idempotency
//!
//! - Migrations should use `IF NOT EXISTS` or `INSERT OR IGNORE`
//! - Safe to run multiple times
//! - Failed migrations do not advance version

use crate::error::LocalDbError;
use rusqlite::Connection;

/// Migration definition with version and upgrade function
///
/// Each migration has:
/// - `version`: Target version after applying this migration
/// - `up`: Function to execute the migration (receives connection)
pub struct Migration {
    /// Target version after applying this migration
    pub version: u32,
    /// Migration function (receives connection, returns Result)
    pub up: fn(&Connection) -> Result<(), rusqlite::Error>,
}

/// Migration: add local_identities table (V1.2 — local_only identity support)
///
/// Creates the `local_identities` table for anonymous and persistent
/// local-only creator identities (ADR-017).
fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(crate::schema::LOCAL_IDENTITIES_TABLE)?;
    Ok(())
}

/// Migration: add soul_meta table (V1.2 — SOUL lifecycle)
///
/// Creates the `soul_meta` table for per-creator SOUL.md metadata tracking.
fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(crate::schema::SOUL_META_TABLE)?;
    Ok(())
}

/// Migration: add memory_pending_review and memory_fragments tables (V1.2 — memory pipeline).
///
/// Creates the two tables for session-end review queue and memory fragments.
/// See creator-memory-soul-lifecycle-v1.md §6.2 and §7.2.
fn migrate_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(crate::schema::MEMORY_PENDING_REVIEW_TABLE)?;
    conn.execute_batch(crate::schema::MEMORY_FRAGMENTS_TABLE)?;
    Ok(())
}

/// Registry of all migrations, sorted by version
///
/// Migrations are executed in order from lowest to highest version.
/// The registry should contain all migrations from initial schema to latest.
pub fn get_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 2,
            up: migrate_v2,
        },
        Migration {
            version: 3,
            up: migrate_v3,
        },
        Migration {
            version: 4,
            up: migrate_v4,
        },
    ]
}

/// Run pending migrations on database
///
/// Executes migrations in sequential order (v1 → v2 → v3 ...):
/// 1. Read current `db_schema_version` from workspace_meta
/// 2. Filter migrations with version > current version
/// 3. Execute each migration and update version after success
/// 4. Abort on failure, preserve old version
///
/// # Idempotency
///
/// Safe to call multiple times:
/// - If database is at version N, migrations N+1, N+2, etc. will run
/// - If database is already at latest version, no migrations run
///
/// # Errors
///
/// Returns `LocalDbError` if:
/// - `workspace_meta` table does not exist
/// - `db_schema_version` key is missing
/// - Migration execution fails
/// - Version update fails
///
/// # Example
///
/// ```rust,no_run
/// use nexus_local_db::{init, run_migrations, RuntimeRole};
/// use rusqlite::Connection;
///
/// fn main() -> Result<(), nexus_local_db::LocalDbError> {
///     let conn = Connection::open("state.db")?;
///     init(&conn, RuntimeRole::Cli)?;
///     run_migrations(&conn)?;
///     Ok(())
/// }
/// ```
pub fn run_migrations(conn: &Connection) -> Result<(), LocalDbError> {
    // Read current version
    let current_version_str: String = conn.query_row(
        "SELECT value FROM workspace_meta WHERE key = 'db_schema_version'",
        [],
        |row| row.get(0),
    )?;

    let current_version: u32 =
        current_version_str
            .parse()
            .map_err(|e| LocalDbError::InvalidVersionValue {
                key: "db_schema_version".to_string(),
                value: current_version_str,
                reason: format!("failed to parse as u32: {}", e),
            })?;

    // Get migrations sorted by version
    let migrations = get_migrations();

    // Filter migrations with version > current version
    let pending_migrations: Vec<&Migration> = migrations
        .iter()
        .filter(|m| m.version > current_version)
        .collect();

    // If no pending migrations, return success
    if pending_migrations.is_empty() {
        return Ok(());
    }

    // Execute each migration in order
    for migration in pending_migrations {
        // Execute migration
        (migration.up)(conn)?;

        // Update version after successful migration
        conn.execute(
            "UPDATE workspace_meta SET value = ?1 WHERE key = 'db_schema_version'",
            [migration.version.to_string()],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{init, init_shared_tables, RuntimeRole, DB_SCHEMA_VERSION};
    use rusqlite::Connection;

    #[test]
    fn run_migrations_no_migrations_needed() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Cli).unwrap();

        // Current version is 2 (bumped), but init already creates the table,
        // so migrations from v1 to v2 are a no-op since init_shared_tables
        // already ran. However, if db_schema_version is seeded to 2,
        // no pending migrations exist.
        let result = run_migrations(&conn);
        assert!(result.is_ok());

        // Verify version unchanged
        let version: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'db_schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, DB_SCHEMA_VERSION.to_string());
    }

    #[test]
    fn run_migrations_fails_on_missing_table() {
        let conn = Connection::open_in_memory().unwrap();
        // Do NOT call init - table should not exist

        let result = run_migrations(&conn);
        assert!(result.is_err());

        match result.unwrap_err() {
            LocalDbError::Rusqlite(_) => {} // Expected - query fails on missing table
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[test]
    fn run_migrations_fails_on_missing_version_key() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();
        // Do NOT seed versions

        let result = run_migrations(&conn);
        assert!(result.is_err());

        match result.unwrap_err() {
            LocalDbError::Rusqlite(_) => {} // Expected - query fails on missing key
            other => panic!("Unexpected error type: {:?}", other),
        }
    }

    #[test]
    fn run_migrations_fails_on_invalid_version_value() {
        let conn = Connection::open_in_memory().unwrap();
        init_shared_tables(&conn).unwrap();

        // Insert invalid version value
        conn.execute(
            "INSERT INTO workspace_meta (key, value) VALUES ('db_schema_version', 'invalid')",
            [],
        )
        .unwrap();

        let result = run_migrations(&conn);
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
    fn run_migrations_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Cli).unwrap();

        // Run migrations multiple times
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();

        // Verify version unchanged
        let version: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'db_schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, DB_SCHEMA_VERSION.to_string());
    }

    #[test]
    fn get_migrations_returns_ordered_vec() {
        let migrations = get_migrations();
        assert!(migrations.len() >= 3);
        assert_eq!(migrations[0].version, 2);
        assert_eq!(migrations[1].version, 3);
        assert_eq!(migrations[2].version, 4);
    }

    #[test]
    fn migration_registry_can_be_extended() {
        // This test demonstrates that migrations can be added to registry
        let migrations = get_migrations();
        assert!(migrations.len() >= 3);

        // When migrations are added, they should be sorted by version
        // (not tested here since registry has one entry, but documented)
    }
}
