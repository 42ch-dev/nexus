//! The `KbStore` contract both backends owe, asserted once.
//!
//! Included with `#[path]` by the `kb_store_contract` integration targets in
//! `nexus-knowledge` (`InMemoryKbStore`) and `nexus-local-db`
//! (`SqliteKbStore`). Each target hands in its own freshly constructed store,
//! so nothing here names a backend or shares one store across a case.

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::{KbStore, KbStoreError, KnowledgeEntryRecord, KnowledgeOwnerRef};

fn make_block(world_id: &str, block_type: BlockType, name: &str) -> KnowledgeEntryRecord {
    KnowledgeEntryRecord::new(world_id, block_type, name)
}

/// Insert returns the stored identity and `get` reads the record back.
pub async fn insert_and_get<S: KbStore>(store: &S) {
    let kb = make_block("wld_1", BlockType::Character, "Hero");

    let result = store.insert_knowledge_entry(kb.clone()).await.unwrap();
    assert_eq!(result.entry_id, kb.entry_id);
    assert_eq!(result.owner, KnowledgeOwnerRef::world("wld_1"));

    let fetched = store.get_knowledge_entry(&kb.entry_id).await.unwrap();
    assert_eq!(fetched.canonical_name, "Hero");
    assert_eq!(fetched.world_id(), Some("wld_1"));
}

/// An unknown id is `NotFound`, never a backend-specific storage error.
pub async fn get_not_found<S: KbStore>(store: &S) {
    let err = store.get_knowledge_entry("kb_absent").await.unwrap_err();
    assert!(matches!(&err, KbStoreError::NotFound(id) if id == "kb_absent"));
}

/// `list_by_world` returns only that world's active entries.
pub async fn list_by_world<S: KbStore>(store: &S) {
    for kb in [
        make_block("wld_1", BlockType::Character, "Hero"),
        make_block("wld_1", BlockType::Scene, "Forest"),
        make_block("wld_2", BlockType::Character, "Villain"),
    ] {
        store.insert_knowledge_entry(kb).await.unwrap();
    }

    let w1 = store.list_by_world("wld_1").await.unwrap();
    assert_eq!(w1.len(), 2);

    let w2 = store.list_by_world("wld_2").await.unwrap();
    assert_eq!(w2.len(), 1);
    assert_eq!(w2[0].canonical_name, "Villain");
}

/// A second active entry with the same `(owner, canonical_name, block_type)`
/// is refused with the colliding owner and name, not a raw storage error.
pub async fn uniqueness_rejects_duplicate<S: KbStore>(store: &S) {
    store
        .insert_knowledge_entry(make_block("wld_1", BlockType::Character, "Hero"))
        .await
        .unwrap();

    let err = store
        .insert_knowledge_entry(make_block("wld_1", BlockType::Character, "Hero"))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, KbStoreError::Duplicate { owner, name, .. }
            if owner == &KnowledgeOwnerRef::world("wld_1") && name == "Hero")
    );
}

/// Delete is soft: the row stays readable as `deleted` and leaves `list_by_world`.
pub async fn soft_delete<S: KbStore>(store: &S) {
    let kb = make_block("wld_1", BlockType::Character, "Hero");
    let id = kb.entry_id.clone();
    store.insert_knowledge_entry(kb).await.unwrap();

    store.delete_knowledge_entry(&id).await.unwrap();

    let fetched = store.get_knowledge_entry(&id).await.unwrap();
    assert_eq!(fetched.status, "deleted");
    assert_eq!(
        store.list_by_world("wld_1").await.unwrap(),
        Vec::<KnowledgeEntryRecord>::new()
    );
}
