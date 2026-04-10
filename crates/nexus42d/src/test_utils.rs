//! Shared test utilities for nexus42d
//!
//! This module provides common helpers for setting up test workspaces,
//! reducing boilerplate across integration tests.

use std::ops::Deref;
use std::path::PathBuf;

/// Wrapper around [`tempfile::TempDir`] so tests get a `must_use` reminder to keep the root alive.
#[must_use = "Temporary directory is deleted when dropped; keep TestTempRoot in scope for the whole test."]
pub struct TestTempRoot(tempfile::TempDir);

impl Deref for TestTempRoot {
    type Target = tempfile::TempDir;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

const TEST_CREATOR_ID: &str = "test_creator";
const TEST_WORKSPACE_SLUG: &str = "default";

/// Create a temporary workspace directory with an initialized SQLite database (ADR-014 layout).
///
/// Returns a tuple of `(temp_dir, nexus_home, db_path)` where:
/// - `temp_dir` is the [`TestTempRoot`] that owns the temporary directory. **The caller
///   must keep `temp_dir` alive for the duration of the test**, otherwise the
///   temporary directory will be deleted when it is dropped.
/// - `nexus_home` is the path to the `.nexus42` subdirectory inside the temp dir
///   (this is **not** the same as `$HOME`; tests should set `HOME` to `temp_dir.path()`).
/// - `db_path` is the path to `state.db` under `creators/<id>/workspaces/<slug>/`.
///
/// # Example
///
/// ```rust,ignore
/// let (tmp, nexus_home, db_path) = create_test_workspace();
/// let state = WorkspaceState::new_for_testing(nexus_home, db_path, None);
/// // `tmp` must stay in scope for the duration of the test
/// ```
pub fn create_test_workspace() -> (TestTempRoot, PathBuf, PathBuf) {
    let tmp = TestTempRoot(tempfile::TempDir::new().expect("failed to create temp dir"));
    let user_home = tmp.path();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).expect("failed to create nexus_home dir");

    let op_dir = nexus_home_layout::operational_workspace_dir(
        user_home,
        TEST_CREATOR_ID,
        TEST_WORKSPACE_SLUG,
    );
    std::fs::create_dir_all(&op_dir).expect("operational dir");
    let meta = serde_json::json!({
        "schema_version": 1,
        "creator_id": TEST_CREATOR_ID,
        "workspace_slug": TEST_WORKSPACE_SLUG,
        "local_root": user_home.join("creative"),
        "created_at": "2020-01-01T00:00:00Z"
    });
    std::fs::write(
        op_dir.join("meta.json"),
        serde_json::to_string(&meta).expect("meta json"),
    )
    .expect("meta.json");

    let cfg = serde_json::json!({
        "active_creator_id": TEST_CREATOR_ID,
        "active_workspace_slug_by_creator": { TEST_CREATOR_ID: TEST_WORKSPACE_SLUG }
    });
    std::fs::write(
        nexus_home.join("config.json"),
        serde_json::to_string(&cfg).expect("config json"),
    )
    .expect("config.json");

    let db_path =
        nexus_home_layout::workspace_state_db_path(user_home, TEST_CREATOR_ID, TEST_WORKSPACE_SLUG);

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
pub fn create_initialized_test_workspace() -> (TestTempRoot, PathBuf, PathBuf, PathBuf) {
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
