//! Database Operations (shared by both cohorts)
//!
//! Provides the workspace `state.db` initializer for CLI commands **and** the
//! headless Connect runtime (`commands::connect::open_workspace_pool`).
//! Schema initialization is delegated to `nexus-local-db` module.
//!
//! **No duplicated DDL** - all shared table definitions are in `nexus-local-db`.

use std::path::Path;

use nexus_local_db::SqlitePool;

/// Schema initializer for CLI-side database access.
///
/// Delegates to `nexus-local-db` for shared tables (migrations).
/// Safe to call on an existing database — migrations are idempotent.
pub struct Schema;

impl Schema {
    /// Initialize CLI-side database schema.
    ///
    /// Opens a pool via nexus-local-db, runs migrations, and seeds version keys.
    /// Safe to call on an existing database — migrations are idempotent.
    ///
    /// # Errors
    ///
    /// Returns a `LocalDbError` if pool creation, migrations, or seeding fails.
    pub async fn init(db_path: &Path) -> Result<SqlitePool, nexus_local_db::LocalDbError> {
        // Ensure parent directory exists before opening SQLite connection.
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                nexus_local_db::LocalDbError::IoWithPath {
                    path: parent.display().to_string(),
                    source: e,
                }
            })?;
        }
        // P2 QC3 F-001: delegate to `init_pool` so the runtime/CLI boot path
        // shares the co-boot migration retry/backoff (open pool + migrations
        // with transient-retry + seed versions, in one call).
        nexus_local_db::init_pool(db_path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn schema_init_creates_tables() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = Schema::init(&db_path).await.unwrap();

        let tables_raw: Vec<Option<String>> =
            sqlx::query_scalar!("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .fetch_all(&pool)
                .await
                .unwrap();
        let tables: Vec<String> = tables_raw.into_iter().flatten().collect();

        assert!(tables.contains(&"workspace_meta".to_string()));
        assert!(tables.contains(&"creators".to_string()));
    }

    #[tokio::test]
    async fn schema_init_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        Schema::init(&db_path).await.unwrap();
        Schema::init(&db_path).await.unwrap(); // second call should not fail
    }

    #[tokio::test]
    async fn open_workspace_db_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.db");

        let pool = Schema::init(&path).await.unwrap();

        // Verify file exists
        assert!(path.exists());

        // Verify schema initialized
        let tables_raw: Vec<Option<String>> =
            sqlx::query_scalar!("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(tables_raw
            .into_iter()
            .flatten()
            .any(|x| x == "workspace_meta"));
    }
}
