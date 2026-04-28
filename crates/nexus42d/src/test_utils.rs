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
/// let (tmp, nexus_home, db_path) = create_test_workspace().await;
/// let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
/// // `tmp` must stay in scope for the duration of the test
/// ```
pub async fn create_test_workspace() -> (TestTempRoot, PathBuf, PathBuf) {
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

    // Write as TOML so daemon reads config.toml natively
    let toml_str = format!(
        "active_creator_id = \"{}\"\n[active_workspace_slug_by_creator]\n\"{}\" = \"{}\"",
        TEST_CREATOR_ID, TEST_CREATOR_ID, TEST_WORKSPACE_SLUG
    );
    std::fs::write(nexus_home.join("config.toml"), toml_str).expect("config.toml");

    let db_path =
        nexus_home_layout::workspace_state_db_path(user_home, TEST_CREATOR_ID, TEST_WORKSPACE_SLUG);

    // Initialize schema via nexus_local_db
    let pool = nexus_local_db::open_pool(&db_path)
        .await
        .expect("failed to open database");
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("failed to run migrations");
    nexus_local_db::seed_versions(&pool)
        .await
        .expect("failed to seed versions");

    (tmp, nexus_home, db_path)
}

/// Create a temporary workspace directory with an initialized SQLite database
/// and a pre-seeded workspace path (marking the workspace as "initialized").
///
/// Returns a tuple of `(temp_dir, nexus_home, db_path, workspace_dir)` where:
/// - `temp_dir`, `nexus_home`, `db_path` are as in [`create_test_workspace`].
/// - `workspace_dir` is the path to a created workspace directory.
pub async fn create_initialized_test_workspace() -> (TestTempRoot, PathBuf, PathBuf, PathBuf) {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;

    let workspace_dir = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("failed to create workspace dir");

    // Seed workspace_meta so middleware recognizes the workspace as initialized
    let pool = nexus_local_db::open_pool(&db_path)
        .await
        .expect("failed to open database");
    // SAFETY: test-only — DML helper that seeds workspace_meta for test setup.
    sqlx::query(
        "INSERT OR REPLACE INTO workspace_meta (key, value) VALUES ('manuscript_phase', 'brainstorm')",
    )
    .execute(&pool)
    .await
    .expect("failed to seed manuscript_phase");
    // SAFETY: test-only — DML helper that seeds workspace_meta for test setup.
    sqlx::query(
        "INSERT OR REPLACE INTO workspace_meta (key, value) VALUES ('active_manifest_id', 'manifest-test-1')",
    )
    .execute(&pool)
    .await
    .expect("failed to seed active_manifest_id");

    (tmp, nexus_home, db_path, workspace_dir)
}

/// Seed a valid (non-expired) auth token for testing.
///
/// Inserts a row into `auth_tokens` with the given `user_id`, `access_token`,
/// and `refresh_token`. The token will expire 1 hour from now.
#[cfg(test)]
pub async fn seed_valid_token(
    state: &crate::workspace::WorkspaceState,
    user_id: &str,
    access_token: &str,
    refresh_token: &str,
) {
    use chrono::Utc;

    let expires_at = (Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    let created_at = Utc::now().to_rfc3339();

    // SAFETY: test-only data setup — inserts mock auth_tokens for middleware tests.
    // Uses dynamic bind values (expires_at computed at runtime).
    sqlx::query(
        "INSERT OR REPLACE INTO auth_tokens (user_id, access_token, refresh_token, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(access_token)
    .bind(refresh_token)
    .bind(&expires_at)
    .bind(&created_at)
    .execute(state.pool())
    .await
    .unwrap();
}

/// Seed an expired auth token for testing.
///
/// Inserts a row into `auth_tokens` that expired 1 hour ago.
#[cfg(test)]
pub async fn seed_expired_token(
    state: &crate::workspace::WorkspaceState,
    user_id: &str,
    access_token: &str,
    refresh_token: &str,
) {
    use chrono::Utc;

    let expires_at = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    let created_at = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();

    // SAFETY: test-only data setup — inserts mock auth_tokens for middleware tests.
    // Uses dynamic bind values (expires_at computed at runtime).
    sqlx::query(
        "INSERT OR REPLACE INTO auth_tokens (user_id, access_token, refresh_token, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(access_token)
    .bind(refresh_token)
    .bind(&expires_at)
    .bind(&created_at)
    .execute(state.pool())
    .await
    .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_test_workspace_returns_valid_paths() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;

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

    #[tokio::test]
    async fn create_initialized_test_workspace_seeds_metadata() {
        let (_tmp, _nexus_home, db_path, workspace_dir) = create_initialized_test_workspace().await;

        assert!(workspace_dir.exists(), "workspace_dir should exist");

        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        // SAFETY: test-only — read-back verification of seeded test data.
        let phase: (String,) =
            sqlx::query_as("SELECT value FROM workspace_meta WHERE key = 'manuscript_phase'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(phase.0, "brainstorm");
    }
}
