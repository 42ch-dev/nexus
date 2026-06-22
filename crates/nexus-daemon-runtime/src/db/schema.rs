//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Daemon Database Schema
//!
//! Delegates all schema initialization to `nexus-local-db` module.
//! No DDL definitions remain in this file — all tables are centrally managed.
//!
//! **All tables** (shared + daemon-only):
//! - Initialized by `nexus_local_db::init_pool()` which runs migrations
//! - Single source of truth in `crates/nexus-local-db/migrations/`

/// Schema initializer for daemon runtime.
///
/// Delegates to `nexus-local-db::init_pool()` for all table creation.
/// Safe to call multiple times — migrations are idempotent.
pub struct Schema;

impl Schema {
    /// Initialize the daemon database schema (async).
    ///
    /// Calls `nexus_local_db::init_pool()` which opens a pool,
    /// runs migrations, and seeds version keys.
    pub async fn init(
        db_path: &std::path::Path,
    ) -> Result<sqlx::SqlitePool, nexus_local_db::LocalDbError> {
        let pool = nexus_local_db::init_pool(db_path).await?;
        // R-V159P1-002: legacy `outbox` table deprecation notice belongs on
        // the PRODUCTION init path (not the `#[cfg(test)]` DDL-assertion
        // block where it previously lived). The table is still created by
        // the initial migration but has zero active Rust consumers; phased
        // removal is planned for V1.61+ (outbox-consolidation.md §6).
        tracing::warn!(
            "legacy outbox table deprecated — zero active consumers; phased removal planned post-V1.59. \
             See .mstar/knowledge/specs/outbox-consolidation.md §6."
        );
        Ok(pool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_local_db::SCHEMA_VERSION;

    #[tokio::test]
    async fn schema_init_creates_all_tables() {
        let tmp = tempfile::TempDir::new().expect("TempDir creation should succeed");
        let db_path = tmp.path().join("test.db");
        let pool = Schema::init(&db_path)
            .await
            .expect("Schema::init should succeed");

        // SAFETY: test-only DDL verification — queries sqlite_master metadata table.
        let tables: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .fetch_all(&pool)
                .await
                .expect("SELECT should succeed");

        let table_names: Vec<&str> = tables.iter().map(|t| t.0.as_str()).collect();

        // Shared tables
        assert!(
            table_names.contains(&"workspace_meta"),
            "missing workspace_meta"
        );
        assert!(table_names.contains(&"creators"), "missing creators");
        assert!(
            table_names.contains(&"reference_sources"),
            "missing reference_sources"
        );

        // Daemon-only tables
        // DEPRECATED (V1.59 P1 T3): legacy `outbox` table has zero active consumers
        // and is planned for phased removal (see outbox-consolidation.md §6).
        // Table is still created by initial migration but no Rust code reads/writes it.
        // R-V159P1-002: the deprecation `tracing::warn!` lives on the production
        // `Schema::init` path now; this block keeps only the DDL-presence assertion.
        assert!(table_names.contains(&"outbox"), "missing legacy outbox");
        assert!(table_names.contains(&"auth_tokens"), "missing auth_tokens");
        assert!(
            table_names.contains(&"acp_tool_audit_log"),
            "missing acp_tool_audit_log"
        );
        assert!(
            table_names.contains(&"acp_sessions"),
            "missing acp_sessions"
        );
    }

    #[tokio::test]
    async fn schema_init_is_idempotent() {
        let tmp = tempfile::TempDir::new().expect("TempDir creation should succeed");
        let db_path = tmp.path().join("test.db");
        Schema::init(&db_path)
            .await
            .expect("Schema::init should succeed");
        Schema::init(&db_path)
            .await
            .expect("Schema::init should be idempotent"); // second call should not fail
    }

    #[tokio::test]
    async fn schema_versions_seeded_correctly() {
        let tmp = tempfile::TempDir::new().expect("TempDir creation should succeed");
        let db_path = tmp.path().join("test.db");
        let pool = Schema::init(&db_path)
            .await
            .expect("Schema::init should succeed");

        // SAFETY: test-only DDL verification — reads seeded version from workspace_meta.
        let db_version: (String,) =
            sqlx::query_as("SELECT value FROM workspace_meta WHERE key = 'db_schema_version'")
                .fetch_one(&pool)
                .await
                .expect("SELECT should succeed");
        assert_eq!(db_version.0, nexus_local_db::DB_SCHEMA_VERSION.to_string());

        // SAFETY: test-only DDL verification — reads seeded version from workspace_meta.
        let schema_version: (String,) =
            sqlx::query_as("SELECT value FROM workspace_meta WHERE key = 'schema_version'")
                .fetch_one(&pool)
                .await
                .expect("SELECT should succeed");
        assert_eq!(schema_version.0, SCHEMA_VERSION.to_string());
    }

    #[tokio::test]
    async fn reference_sources_has_content_column() {
        let tmp = tempfile::TempDir::new().expect("TempDir creation should succeed");
        let db_path = tmp.path().join("test.db");
        let pool = Schema::init(&db_path)
            .await
            .expect("Schema::init should succeed");

        // SAFETY: test-only DDL verification — inserts and reads back reference_sources row.
        sqlx::query(
            "INSERT INTO reference_sources
             (reference_source_id, workspace_id, source_type, uri, title, content, scan_status, created_at)
             VALUES ('ref_test', 'local', 'pdf', 'test.pdf', 'Test', 'Extracted text', 'pending', '2026-01-01T00:00:00Z')"
        )
        .execute(&pool)
        .await
        .expect("INSERT should succeed");

        let content: (Option<String>,) = sqlx::query_as(
            "SELECT content FROM reference_sources WHERE reference_source_id = 'ref_test'",
        )
        .fetch_one(&pool)
        .await
        .expect("SELECT should succeed");

        assert_eq!(content.0, Some("Extracted text".to_string()));
    }

    #[tokio::test]
    async fn creators_table_has_default_status() {
        let tmp = tempfile::TempDir::new().expect("TempDir creation should succeed");
        let db_path = tmp.path().join("test.db");
        let pool = Schema::init(&db_path)
            .await
            .expect("Schema::init should succeed");

        // SAFETY: test-only DDL verification — inserts and reads back creators row.
        sqlx::query(
            "INSERT INTO creators (creator_id, display_name, cached_at, data)
             VALUES ('ctr_test', 'Test', '2026-01-01T00:00:00Z', '{}')",
        )
        .execute(&pool)
        .await
        .expect("INSERT should succeed");

        let status: (String,) =
            sqlx::query_as("SELECT status FROM creators WHERE creator_id = 'ctr_test'")
                .fetch_one(&pool)
                .await
                .expect("SELECT should succeed");

        assert_eq!(status.0, "active");
    }

    #[tokio::test]
    async fn reference_sources_table_has_tags_and_content_hash() {
        let tmp = tempfile::TempDir::new().expect("TempDir creation should succeed");
        let db_path = tmp.path().join("test.db");
        let pool = Schema::init(&db_path)
            .await
            .expect("Schema::init should succeed");

        // SAFETY: test-only DDL verification — inserts and reads back reference_sources row
        // with nullable columns (tags, content_hash).
        sqlx::query(
            "INSERT INTO reference_sources
             (reference_source_id, workspace_id, source_type, uri, title, tags, content_hash, scan_status, created_at)
             VALUES ('ref_test', 'local', 'pdf', 'test.pdf', 'Test', 'tag1,tag2', 'abc123', 'pending', '2026-01-01T00:00:00Z')"
        )
        .execute(&pool)
        .await
        .expect("INSERT should succeed");

        let row: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT tags, content_hash FROM reference_sources WHERE reference_source_id = 'ref_test'"
        )
        .fetch_one(&pool)
        .await
        .expect("SELECT should succeed");

        assert_eq!(row.0, Some("tag1,tag2".to_string()));
        assert_eq!(row.1, Some("abc123".to_string()));
    }

    #[tokio::test]
    async fn pragmas_are_set() {
        let tmp = tempfile::TempDir::new().expect("TempDir creation should succeed");
        let db_path = tmp.path().join("test.db");
        let pool = Schema::init(&db_path)
            .await
            .expect("Schema::init should succeed");

        // SAFETY: PRAGMA statement — not supported by compile-time checked macros.
        let jm: (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .expect("PRAGMA should succeed");
        assert_eq!(jm.0.to_lowercase(), "wal");

        // SAFETY: PRAGMA statement — not supported by compile-time checked macros.
        let fk: (i32,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .expect("PRAGMA should succeed");
        assert_eq!(fk.0, 1);
    }
}
