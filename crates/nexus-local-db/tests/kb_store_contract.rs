//! The shared `KbStore` contract, run against the SQLite backend.
//!
//! Same source as `nexus-knowledge`'s target (`#[path]`), so both backends
//! answer one contract; each case gets its own freshly migrated database and
//! store.

#![allow(clippy::unwrap_used)]

use nexus_local_db::kb_store::SqliteKbStore;

#[path = "../../nexus-knowledge/tests/support/kb_store_contract.rs"]
mod kb_store_contract;

/// A fresh migrated database with the two contract worlds seeded.
async fn fresh_store() -> (tempfile::TempDir, SqliteKbStore) {
    let dir = tempfile::tempdir().unwrap();
    let pool = nexus_local_db::open_pool(&dir.path().join("test.db"))
        .await
        .unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    for (world_id, title, slug) in [
        ("wld_1", "Test World", "test-world"),
        ("wld_2", "Second World", "second-world"),
    ] {
        // name `Test`, so the contract keeps that row shape.
        nexus_local_db::kb_store::seed::world_with(
            &pool,
            nexus_local_db::kb_store::seed::WorldSeed::new(world_id, "ctr_test", title, slug)
                .with_display_name("Test"),
        )
        .await;
    }
    (dir, SqliteKbStore::new(pool))
}

#[tokio::test]
async fn insert_and_get() {
    let (_dir, store) = fresh_store().await;
    kb_store_contract::insert_and_get(&store).await;
}

#[tokio::test]
async fn get_not_found() {
    let (_dir, store) = fresh_store().await;
    kb_store_contract::get_not_found(&store).await;
}

#[tokio::test]
async fn list_by_world() {
    let (_dir, store) = fresh_store().await;
    kb_store_contract::list_by_world(&store).await;
}

#[tokio::test]
async fn uniqueness_rejects_duplicate() {
    let (_dir, store) = fresh_store().await;
    kb_store_contract::uniqueness_rejects_duplicate(&store).await;
}

#[tokio::test]
async fn soft_delete() {
    let (_dir, store) = fresh_store().await;
    kb_store_contract::soft_delete(&store).await;
}
