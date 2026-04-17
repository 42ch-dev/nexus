#[tokio::test]
async fn open_pool_creates_file_and_sets_pragmas() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(tmp.path()).await.unwrap();
    let jm: (String,) = sqlx::query_as("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(jm.0.to_lowercase(), "wal");
}
