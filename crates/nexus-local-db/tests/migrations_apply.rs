#[tokio::test]
async fn all_migrations_apply_to_fresh_db() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(tmp.path()).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();

    // Assert every expected table exists.
    for table in [
        "workspace_meta",
        "creators",
        "auth_tokens",
        "acp_tool_audit_log",
        "acp_sessions",
        "local_identities",
        "soul_meta",
        "memory_pending_review",
        "memory_fragments",
        "peer_hosts",
    ] {
        // SAFETY: test-only — queries sqlite_master to verify migration table existence.
        let found: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?")
                .bind(table)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(found.0, 1, "missing table: {table}");
    }

    // Assert db_schema_version is seeded.
    // SAFETY: test-only — read-back verification of seeded version metadata.
    let v: (String,) =
        sqlx::query_as("SELECT value FROM workspace_meta WHERE key='db_schema_version'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(v.0, "4");
}

#[tokio::test]
async fn migrations_are_idempotent() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(tmp.path()).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap(); // second call is a no-op
}

/// ADR K1 §3.1–§3.2: the User `KnowledgeStore` and reference registry are
/// dropped, not sealed, once their data is archived.
#[tokio::test]
async fn retired_user_and_reference_tables_are_dropped() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::init_pool(tmp.path()).await.unwrap();

    for table in ["knowledge_entries", "reference_sources"] {
        // SAFETY: test-only — sqlite_master existence check for a retired table.
        let found: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?")
                .bind(table)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(found.0, 0, "retired table still exists: {table}");
    }

    // 32 was the schema line that still carried both tables.
    let versions = nexus_local_db::read_versions(&pool).await.unwrap();
    assert_eq!(
        versions.db_schema_version,
        nexus_local_db::DB_SCHEMA_VERSION
    );
    assert!(
        versions.db_schema_version > 32,
        "the retirement drop must advance db_schema_version past 32"
    );
}
