//! Shared test utilities for nexus42d
//!
//! This module provides common helpers for setting up test workspaces,
//! reducing boilerplate across integration tests.

use std::path::PathBuf;

/// Create a temporary workspace directory with an initialized SQLite database.
///
/// Returns a tuple of `(temp_dir, nexus_home, db_path)` where:
/// - `temp_dir` is the `TempDir` that owns the temporary directory. **The caller
///   must keep `temp_dir` alive for the duration of the test**, otherwise the
///   temporary directory will be deleted when `TempDir` is dropped.
/// - `nexus_home` is the path to the `.nexus42` subdirectory inside the temp dir.
/// - `db_path` is the path to `state.db` inside `nexus_home`.
///
/// # Example
///
/// ```rust,ignore
/// let (tmp, nexus_home, db_path) = create_test_workspace();
/// let state = WorkspaceState::new_for_testing(nexus_home, db_path, None);
/// // `tmp` must stay in scope for the duration of the test
/// ```
pub fn create_test_workspace() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
    let nexus_home = tmp.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_home).expect("failed to create nexus_home dir");
    let db_path = nexus_home.join("state.db");

    let conn = rusqlite::Connection::open(&db_path).expect("failed to open database");
    crate::db::schema::Schema::init(&conn).expect("failed to initialize schema");
    drop(conn);

    (tmp, nexus_home, db_path)
}

/// Create a temporary workspace directory with an initialized SQLite database
/// and a pre-seeded workspace path (marking the workspace as "initialized").
///
/// Returns a tuple of `(temp_dir, nexus_home, db_path, workspace_dir)` where:
/// - `temp_dir`, `nexus_home`, `db_path` are as in [`create_test_workspace`].
/// - `workspace_dir` is the path to a created workspace directory.
pub fn create_initialized_test_workspace() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let (tmp, nexus_home, db_path) = create_test_workspace();

    let workspace_dir = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("failed to create workspace dir");

    // Seed workspace_meta so middleware recognizes the workspace as initialized
    let conn = rusqlite::Connection::open(&db_path).expect("failed to open database");
    conn.execute(
        "INSERT OR REPLACE INTO workspace_meta (key, value) VALUES ('manuscript_phase', 'brainstorm')",
        [],
    )
    .expect("failed to seed manuscript_phase");
    conn.execute(
        "INSERT OR REPLACE INTO workspace_meta (key, value) VALUES ('active_manifest_id', 'manifest-test-1')",
        [],
    )
    .expect("failed to seed active_manifest_id");
    drop(conn);

    (tmp, nexus_home, db_path, workspace_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_test_workspace_returns_valid_paths() {
        let (tmp, nexus_home, db_path) = create_test_workspace();

        assert!(nexus_home.exists(), "nexus_home should exist");
        assert!(db_path.exists(), "db_path should exist after schema init");
        assert!(
            nexus_home.starts_with(tmp.path()),
            "nexus_home should be inside temp dir"
        );
        assert_eq!(
            db_path.file_name().unwrap(),
            std::ffi::OsStr::new("state.db"),
            "db_path should end with state.db"
        );
    }

    #[test]
    fn create_initialized_test_workspace_seeds_metadata() {
        let (_tmp, _nexus_home, db_path, workspace_dir) = create_initialized_test_workspace();

        assert!(workspace_dir.exists(), "workspace_dir should exist");

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let phase: String = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'manuscript_phase'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(phase, "brainstorm");
    }
}
