use nexus_local_db::{
    init_pool, open_pool, read_versions, run_migrations, seed_versions, validate, RuntimeRole,
};

#[tokio::test]
async fn seed_and_read_versions_roundtrip() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = open_pool(tmp.path()).await.unwrap();
    run_migrations(&pool).await.unwrap();
    seed_versions(&pool).await.unwrap();
    let v = read_versions(&pool).await.unwrap();
    assert_eq!(v.db_schema_version, nexus_local_db::DB_SCHEMA_VERSION);
    assert_eq!(v.schema_version, nexus_local_db::SCHEMA_VERSION);
}

#[tokio::test]
async fn validate_passes_after_full_init() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = open_pool(tmp.path()).await.unwrap();
    run_migrations(&pool).await.unwrap();
    seed_versions(&pool).await.unwrap();
    validate(&pool, RuntimeRole::Daemon).await.unwrap();
}

#[tokio::test]
async fn init_pool_convenience() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = init_pool(tmp.path()).await.unwrap();

    // Verify all tables exist
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE '_sqlx_%' ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let table_names: Vec<&str> = tables.iter().map(|t| t.0.as_str()).collect();
    assert!(table_names.contains(&"workspace_meta"));
    assert!(table_names.contains(&"local_identities"));
    assert!(table_names.contains(&"soul_meta"));
    assert!(table_names.contains(&"memory_fragments"));

    // Verify versions are seeded
    let v = read_versions(&pool).await.unwrap();
    assert_eq!(v.db_schema_version, nexus_local_db::DB_SCHEMA_VERSION);
}
