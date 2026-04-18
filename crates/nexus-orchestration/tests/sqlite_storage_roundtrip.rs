use graph_flow::{Session, SessionStorage};
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use std::sync::Arc;

#[tokio::test]
async fn session_roundtrip() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(db.path())
        .await
        .expect("open pool");
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("run migrations");
    let storage: Arc<dyn SessionStorage> = Arc::new(SqliteSessionStorage::new(Arc::new(pool)));

    let session = Session::new_from_task("sess-001".into(), "dummy-task");
    storage.save(session).await.unwrap();
    let loaded = storage
        .get("sess-001")
        .await
        .unwrap()
        .expect("session present");
    assert_eq!(loaded.id, "sess-001");
    storage.delete("sess-001").await.unwrap();
    assert!(storage.get("sess-001").await.unwrap().is_none());
}

#[tokio::test]
async fn restart_resume_smoke() {
    let db = tempfile::NamedTempFile::new().unwrap();
    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (first)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (first)");
        let storage = SqliteSessionStorage::new(std::sync::Arc::new(pool));
        let session = Session::new_from_task("sess-restart".into(), "dummy-task");
        storage.save(session).await.unwrap();
    } // pool drops — simulates daemon shutdown
    {
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool (second)");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations (second) — idempotent");
        let storage = SqliteSessionStorage::new(std::sync::Arc::new(pool));
        assert!(storage.get("sess-restart").await.unwrap().is_some());
    }
}
