//! CLI Database Operations
//!
//! Provides database access for CLI commands.
//! Schema initialization is delegated to `nexus-local-db` module.
//!
//! **No duplicated DDL** - all shared table definitions are in `nexus-local-db`.

use std::path::Path;

use nexus_local_db::{open_pool as local_db_open_pool, run_migrations, SqlitePool};

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
    pub async fn init(db_path: &Path) -> Result<SqlitePool, nexus_local_db::LocalDbError> {
        let pool = local_db_open_pool(db_path).await?;
        run_migrations(&pool).await?;
        nexus_local_db::seed_versions(&pool).await?;
        Ok(pool)
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
        assert!(tables.contains(&"reference_sources".to_string()));
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
        assert!(tables_raw.into_iter().flatten().any(|x| x == "workspace_meta"));
    }

    #[tokio::test]
    async fn reference_sources_has_content_column() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = Schema::init(&db_path).await.unwrap();

        // Verify content column exists (drift fix validation)
        let ref_id = "ref_test";
        let ws_id = "local";
        let src_type = "pdf";
        let uri = "test.pdf";
        let title = "Test";
        let content = "Extracted text";
        let scan_status = "pending";
        let created_at = "2026-01-01T00:00:00Z";
        sqlx::query!(
            "INSERT INTO reference_sources
             (reference_source_id, workspace_id, source_type, uri, title, content, scan_status, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            ref_id, ws_id, src_type, uri, title, content, scan_status, created_at
        )
        .execute(&pool)
        .await
        .unwrap();

        let result: Option<String> = sqlx::query_scalar!(
            "SELECT content FROM reference_sources WHERE reference_source_id = ?",
            ref_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(result, Some("Extracted text".to_string()));
    }
}
