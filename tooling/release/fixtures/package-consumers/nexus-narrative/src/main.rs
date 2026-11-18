//! Consumes the packed `nexus-narrative` and `nexus-knowledge` archives
//! together: writes and reads a World KB entry through the real `KbStore`
//! surface, then inserts and queries a `World` through the real
//! `NarrativeGateway`, verifying identity and state field by field.
//!
//! No local database and no `CoreService` are involved: `InMemoryKbStore` is
//! the package's own public store, and the gateway is the package's own
//! public read model.

use nexus_contracts::{BlockType, TimePolicy, Visibility};
use nexus_knowledge::world_kb::{InMemoryKbStore, KbQuery, KbStore, KnowledgeEntryRecord};
use nexus_narrative::world::World;
use nexus_narrative::{InMemoryNarrativeGateway, NarrativeError, NarrativeGateway};

const WORLD_ID: &str = "wld_consumer";
const OWNER_ID: &str = "ctr_consumer";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // ── nexus-knowledge: the actual KbStore surface ────────────────────
    let store = InMemoryKbStore::new();
    let entry = KnowledgeEntryRecord::new(WORLD_ID, BlockType::Character, "Ash of the Vale");
    let entry_id = entry.entry_id.clone();

    let inserted = store
        .insert_knowledge_entry(entry)
        .await
        .expect("the store accepts a world-owned entry");
    assert_eq!(inserted.entry_id, entry_id);
    assert_eq!(inserted.owner.world_id(), Some(WORLD_ID));

    let fetched = store
        .get_knowledge_entry(&entry_id)
        .await
        .expect("the inserted entry is addressable by id");
    assert_eq!(fetched.entry_id, entry_id);
    assert_eq!(fetched.canonical_name, "Ash of the Vale");
    assert_eq!(fetched.world_id(), Some(WORLD_ID));
    assert_eq!(fetched.status, "provisional");

    let page = store
        .query(&KbQuery::new(WORLD_ID))
        .await
        .expect("the world query succeeds");
    assert_eq!(page.total_count, 1);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].entry_id, entry_id);
    assert_eq!(page.items[0].block_type, BlockType::Character);

    // ── nexus-narrative: the gateway over that KbStore ─────────────────
    let mut world = World::new(
        WORLD_ID,
        OWNER_ID,
        "The Ashen Vale",
        "ashen-vale",
        Visibility::Private,
        TimePolicy::Manual,
    );
    world
        .advance_time_pointer("evt_1")
        .expect("an active world advances its pointer");
    assert_eq!(world.status, "active");
    let created_at = world.created_at.clone();

    let gateway = InMemoryNarrativeGateway::new(store);
    gateway.insert_world(world);

    let state = gateway
        .get_world_state(WORLD_ID)
        .await
        .expect("the inserted world is readable");
    assert_eq!(state.world_id, WORLD_ID);
    assert_eq!(state.title, "The Ashen Vale");
    assert_eq!(state.slug, "ashen-vale");
    assert_eq!(state.status, "active");
    assert!(!state.is_fork);
    assert_eq!(state.fork_branch_id, None);
    assert_eq!(state.parent_world_id, None);
    assert_eq!(state.canon_revision, Some(0));
    assert_eq!(state.current_time_pointer.as_deref(), Some("evt_1"));
    assert_eq!(state.current_timeline_head_id, None);
    assert_eq!(state.created_at, created_at);

    let worlds = gateway.list_worlds().await.expect("worlds are listed");
    assert_eq!(worlds.len(), 1);
    assert_eq!(worlds[0].world_id, WORLD_ID);
    assert_eq!(worlds[0].title, "The Ashen Vale");

    // A missing world is a named refusal, not an empty projection, and the
    // public variant exposes the missing identity — so assert the payload
    // instead of accepting any `NotFound`.
    match gateway.get_world_state("wld_absent").await {
        Err(NarrativeError::NotFound { resource }) => {
            assert_eq!(resource, "world wld_absent");
        }
        Err(other) => panic!("a missing world must refuse with NotFound, got {other:?}"),
        Ok(state) => panic!("a missing world must not project, got {state:?}"),
    }

    println!("nexus-narrative read back world {WORLD_ID} with entry {entry_id}");
}
