//! The shared `KbStore` contract, run against the in-memory backend.

#![allow(clippy::unwrap_used)]

use nexus_knowledge::world_kb::InMemoryKbStore;

#[path = "support/kb_store_contract.rs"]
mod kb_store_contract;

#[tokio::test]
async fn insert_and_get() {
    kb_store_contract::insert_and_get(&InMemoryKbStore::new()).await;
}

#[tokio::test]
async fn get_not_found() {
    kb_store_contract::get_not_found(&InMemoryKbStore::new()).await;
}

#[tokio::test]
async fn list_by_world() {
    kb_store_contract::list_by_world(&InMemoryKbStore::new()).await;
}

#[tokio::test]
async fn uniqueness_rejects_duplicate() {
    kb_store_contract::uniqueness_rejects_duplicate(&InMemoryKbStore::new()).await;
}

#[tokio::test]
async fn soft_delete() {
    kb_store_contract::soft_delete(&InMemoryKbStore::new()).await;
}
