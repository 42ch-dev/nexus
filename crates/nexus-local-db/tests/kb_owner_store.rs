//! v1.184 P1 Task 2 — owner-aware `SqliteKbStore` proofs.
//!
//! Covers the `KnowledgeEntryRecord` / `KnowledgeOwnerRef` cutover at the
//! storage boundary:
//! - all three closed owner kinds round-trip through insert/get/list;
//! - non-World owners never receive a fabricated `world_id` (typed column or
//!   `extensions.nexus` metadata);
//! - `creator_only` is World-owned only (DB CHECK) and round-trips;
//! - owner and `creator_only` are immutable through `update_knowledge_entry`;
//! - unknown `extensions.nexus` keys survive the read-modify-write cycle for
//!   every owner kind;
//! - legacy World-only behavior (`list_by_world`, `query`, world-scoped
//!   uniqueness) is unchanged.

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{
    KnowledgeEntryBody, KnowledgeEntryRecord, KnowledgeOwnerRef,
};
use nexus_knowledge::world_kb::query::KbQuery;
use nexus_knowledge::world_kb::store::KbStoreError;
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use sqlx::SqlitePool;

const CREATOR: &str = "ctr_cccccccccccccccccccccccccccccccc";
const WORLD_A: &str = "wld_ownerstore_a";
const WORLD_B: &str = "wld_ownerstore_b";
const CHARACTER: &str = "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BINDING: &str = "awb_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

// ── Pool / seed helpers (same shapes as kb_owner_scope_migration.rs) ──────

async fn migrated_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("test.db").display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    // SAFETY: PRAGMA statement — no table schema to validate against.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    nexus_local_db::ensure_creator_row(&pool, CREATOR, "Owner")
        .await
        .unwrap();
    (pool, dir)
}

async fn seed_world(pool: &SqlitePool, world_id: &str) {
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', '2026-08-01T00:00:00Z')",
    )
    .bind(world_id)
    .bind(CREATOR)
    .bind(world_id)
    .bind(world_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_character(pool: &SqlitePool, character_id: &str) {
    sqlx::query(
        "INSERT INTO characters \
         (character_id, owner_creator_id, display_name, status, image_uri, persona_json, \
          created_at, updated_at) \
         VALUES (?, ?, ?, 'active', NULL, '{}', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
    )
    .bind(character_id)
    .bind(CREATOR)
    .bind(character_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_binding(pool: &SqlitePool, binding_id: &str, character_id: &str, world_id: &str) {
    sqlx::query(
        "INSERT INTO actor_world_bindings \
         (binding_id, character_id, world_id, status, world_sheet_entry_id, \
          created_at, updated_at) \
         VALUES (?, ?, ?, 'active', NULL, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
    )
    .bind(binding_id)
    .bind(character_id)
    .bind(world_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Raw owner columns + extensions metadata for assertions.
async fn raw_owner_row(
    pool: &SqlitePool,
    key_block_id: &str,
) -> (String, Option<String>, Option<String>, Option<String>, i64, Option<String>) {
    sqlx::query_as(
        "SELECT owner_kind, world_id, character_id, actor_world_binding_id, creator_only, \
         extensions_nexus_json FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(key_block_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn record_with_body(owner: KnowledgeOwnerRef, name: &str) -> KnowledgeEntryRecord {
    let mut rec = match &owner {
        KnowledgeOwnerRef::World(id) => KnowledgeEntryRecord::new(id, BlockType::Character, name),
        KnowledgeOwnerRef::Character(id) => {
            KnowledgeEntryRecord::for_character(id, BlockType::Character, name)
        }
        KnowledgeOwnerRef::ActorWorldBinding(id) => {
            KnowledgeEntryRecord::for_binding(id, BlockType::Character, name)
        }
    };
    rec.body = Some(KnowledgeEntryBody {
        summary: Some(format!("{name} summary")),
        ..KnowledgeEntryBody::default()
    });
    rec
}

// ── Tests ───────────────────────────────────────────────────────────────

/// World golden: the legacy World-owned behavior is unchanged end to end.
#[tokio::test]
async fn world_owner_round_trip_preserves_legacy_behavior() {
    let (pool, _dir) = migrated_pool().await;
    seed_world(&pool, WORLD_A).await;
    let store = SqliteKbStore::new(pool.clone());

    let rec = record_with_body(KnowledgeOwnerRef::world(WORLD_A), "Aria");
    let entry_id = rec.entry_id.clone();
    let res = store.insert_knowledge_entry(rec).await.unwrap();
    assert_eq!(res.owner, KnowledgeOwnerRef::world(WORLD_A));

    let got = store.get_knowledge_entry(&entry_id).await.unwrap();
    assert_eq!(got.owner, KnowledgeOwnerRef::world(WORLD_A));
    assert!(!got.creator_only);
    assert_eq!(got.world_id(), Some(WORLD_A));
    assert_eq!(got.canonical_name, "Aria");
    assert_eq!(
        got.body.as_ref().and_then(|b| b.summary.as_deref()),
        Some("Aria summary")
    );

    // Legacy world-scoped reads see the row.
    let listed = store.list_by_world(WORLD_A).await.unwrap();
    assert_eq!(listed.len(), 1);
    let queried = store.query(&KbQuery::new(WORLD_A)).await.unwrap();
    assert_eq!(queried.total_count, 1);

    // World-scoped active uniqueness still rejects a same-name duplicate.
    let dup = record_with_body(KnowledgeOwnerRef::world(WORLD_A), "Aria");
    let err = store.insert_knowledge_entry(dup).await.unwrap_err();
    assert_eq!(
        err,
        KbStoreError::Duplicate {
            owner: KnowledgeOwnerRef::world(WORLD_A),
            name: "Aria".to_string(),
            block_type: BlockType::Character,
        }
    );

    // The persisted owner columns + extensions metadata are the World shape.
    let (kind, world_id, character_id, binding_id, creator_only, extensions) =
        raw_owner_row(&pool, &entry_id).await;
    assert_eq!(kind, "world");
    assert_eq!(world_id.as_deref(), Some(WORLD_A));
    assert_eq!(character_id, None);
    assert_eq!(binding_id, None);
    assert_eq!(creator_only, 0);
    let extensions = extensions.unwrap();
    assert!(extensions.contains("\"world_id\""));
}

/// Character owner: round-trips, is invisible to world-scoped reads, and
/// never receives a fabricated `world_id` (column or extensions metadata).
#[tokio::test]
async fn character_owner_round_trip_without_world_fabrication() {
    let (pool, _dir) = migrated_pool().await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER).await;
    let store = SqliteKbStore::new(pool.clone());

    let rec = record_with_body(KnowledgeOwnerRef::character(CHARACTER), "Shared lore");
    let entry_id = rec.entry_id.clone();
    let res = store.insert_knowledge_entry(rec).await.unwrap();
    assert_eq!(res.owner, KnowledgeOwnerRef::character(CHARACTER));

    let got = store.get_knowledge_entry(&entry_id).await.unwrap();
    assert_eq!(got.owner, KnowledgeOwnerRef::character(CHARACTER));
    assert_eq!(got.world_id(), None);
    assert!(!got.creator_only);

    // Invisible to legacy world-scoped reads.
    assert!(store.list_by_world(WORLD_A).await.unwrap().is_empty());
    assert_eq!(store.query(&KbQuery::new(WORLD_A)).await.unwrap().total_count, 0);

    // Owner-scoped listing finds it.
    let owned = store
        .list_by_owner(&KnowledgeOwnerRef::character(CHARACTER))
        .await
        .unwrap();
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].entry_id, entry_id);

    // No fabricated world anywhere in storage.
    let (kind, world_id, character_id, binding_id, _, extensions) =
        raw_owner_row(&pool, &entry_id).await;
    assert_eq!(kind, "character");
    assert_eq!(world_id, None);
    assert_eq!(character_id.as_deref(), Some(CHARACTER));
    assert_eq!(binding_id, None);
    let extensions = extensions.unwrap();
    assert!(extensions.contains("\"character_id\""));
    assert!(
        !extensions.contains("\"world_id\""),
        "character-owned row must not carry a world_id extension: {extensions}"
    );
}

/// Binding owner: same isolation and no-fabrication contract as Character.
#[tokio::test]
async fn binding_owner_round_trip_without_world_fabrication() {
    let (pool, _dir) = migrated_pool().await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER).await;
    seed_binding(&pool, BINDING, CHARACTER, WORLD_A).await;
    let store = SqliteKbStore::new(pool.clone());

    let rec = record_with_body(KnowledgeOwnerRef::actor_world_binding(BINDING), "Private note");
    let entry_id = rec.entry_id.clone();
    store.insert_knowledge_entry(rec).await.unwrap();

    let got = store.get_knowledge_entry(&entry_id).await.unwrap();
    assert_eq!(got.owner, KnowledgeOwnerRef::actor_world_binding(BINDING));
    assert_eq!(got.world_id(), None);

    assert!(store.list_by_world(WORLD_A).await.unwrap().is_empty());
    let owned = store
        .list_by_owner(&KnowledgeOwnerRef::actor_world_binding(BINDING))
        .await
        .unwrap();
    assert_eq!(owned.len(), 1);

    let (kind, world_id, character_id, binding_id, _, extensions) =
        raw_owner_row(&pool, &entry_id).await;
    assert_eq!(kind, "actor_world_binding");
    assert_eq!(world_id, None);
    assert_eq!(character_id, None);
    assert_eq!(binding_id.as_deref(), Some(BINDING));
    let extensions = extensions.unwrap();
    assert!(extensions.contains("\"actor_world_binding_id\""));
    assert!(
        !extensions.contains("\"world_id\""),
        "binding-owned row must not carry a world_id extension: {extensions}"
    );
}

/// `creator_only` persists on World-owned rows and is rejected for
/// non-World owners by the DB CHECK (fail closed at the store boundary).
#[tokio::test]
async fn creator_only_is_world_owned_only() {
    let (pool, _dir) = migrated_pool().await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER).await;
    let store = SqliteKbStore::new(pool.clone());

    let mut rec = record_with_body(KnowledgeOwnerRef::world(WORLD_A), "Creator lore");
    rec.creator_only = true;
    let entry_id = rec.entry_id.clone();
    store.insert_knowledge_entry(rec).await.unwrap();

    let got = store.get_knowledge_entry(&entry_id).await.unwrap();
    assert!(got.creator_only);
    assert_eq!(got.owner, KnowledgeOwnerRef::world(WORLD_A));

    // World-scoped reads still surface the row (filtering creator-only rows
    // out of Character views is the Task 3 view service's job).
    assert_eq!(store.list_by_world(WORLD_A).await.unwrap().len(), 1);

    // Character-owned creator_only violates the CHECK.
    let mut bad = record_with_body(KnowledgeOwnerRef::character(CHARACTER), "Bad");
    bad.creator_only = true;
    assert!(store.insert_knowledge_entry(bad).await.is_err());
}

/// Owner and `creator_only` are immutable through `update_knowledge_entry`;
/// a changed owner or flag is rejected and the stored row is untouched.
#[tokio::test]
async fn owner_and_creator_only_are_immutable_through_update() {
    let (pool, _dir) = migrated_pool().await;
    seed_world(&pool, WORLD_A).await;
    seed_world(&pool, WORLD_B).await;
    seed_character(&pool, CHARACTER).await;
    let store = SqliteKbStore::new(pool.clone());

    let rec = record_with_body(KnowledgeOwnerRef::world(WORLD_A), "Aria");
    let entry_id = rec.entry_id.clone();
    store.insert_knowledge_entry(rec).await.unwrap();

    // Owner change (World A → Character) is rejected.
    let mut moved = store.get_knowledge_entry(&entry_id).await.unwrap();
    moved.owner = KnowledgeOwnerRef::character(CHARACTER);
    let err = store.update_knowledge_entry(moved).await.unwrap_err();
    assert_eq!(err, KbStoreError::ImmutableOwner(entry_id.clone()));

    // World → other World is still an owner change.
    let mut moved = store.get_knowledge_entry(&entry_id).await.unwrap();
    moved.owner = KnowledgeOwnerRef::world(WORLD_B);
    let err = store.update_knowledge_entry(moved).await.unwrap_err();
    assert_eq!(err, KbStoreError::ImmutableOwner(entry_id.clone()));

    // creator_only flip is rejected.
    let mut flipped = store.get_knowledge_entry(&entry_id).await.unwrap();
    flipped.creator_only = true;
    let err = store.update_knowledge_entry(flipped).await.unwrap_err();
    assert_eq!(err, KbStoreError::ImmutableOwner(entry_id.clone()));

    // The stored row is untouched.
    let (kind, world_id, _, _, creator_only, _) = raw_owner_row(&pool, &entry_id).await;
    assert_eq!(kind, "world");
    assert_eq!(world_id.as_deref(), Some(WORLD_A));
    assert_eq!(creator_only, 0);

    // A same-owner body update still works.
    let mut same = store.get_knowledge_entry(&entry_id).await.unwrap();
    same.body = Some(KnowledgeEntryBody {
        summary: Some("revised".to_string()),
        ..KnowledgeEntryBody::default()
    });
    store.update_knowledge_entry(same).await.unwrap();
    let got = store.get_knowledge_entry(&entry_id).await.unwrap();
    assert_eq!(
        got.body.as_ref().and_then(|b| b.summary.as_deref()),
        Some("revised")
    );
}

/// Unknown `extensions.nexus` keys survive the read-modify-write cycle for
/// every owner kind; typed owner keys never leak into the extras bag.
#[tokio::test]
async fn unknown_nexus_extension_keys_round_trip_all_owners() {
    let (pool, _dir) = migrated_pool().await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER).await;
    seed_binding(&pool, BINDING, CHARACTER, WORLD_A).await;
    let store = SqliteKbStore::new(pool.clone());

    for owner in [
        KnowledgeOwnerRef::world(WORLD_A),
        KnowledgeOwnerRef::character(CHARACTER),
        KnowledgeOwnerRef::actor_world_binding(BINDING),
    ] {
        let mut rec = record_with_body(owner.clone(), &format!("extras {}", owner.kind()));
        rec.extensions_nexus_extras = Some(serde_json::json!({"custom_flag": "keep-me"}));
        let entry_id = rec.entry_id.clone();
        store.insert_knowledge_entry(rec).await.unwrap();

        let got = store.get_knowledge_entry(&entry_id).await.unwrap();
        assert_eq!(
            got.extensions_nexus_extras,
            Some(serde_json::json!({"custom_flag": "keep-me"})),
            "extras lost for owner {owner:?}"
        );
        assert_eq!(got.owner, owner);

        // And the extras survive an update cycle too.
        store.update_knowledge_entry(got).await.unwrap();
        let got = store.get_knowledge_entry(&entry_id).await.unwrap();
        assert_eq!(
            got.extensions_nexus_extras,
            Some(serde_json::json!({"custom_flag": "keep-me"})),
            "extras lost on update for owner {owner:?}"
        );
    }
}

/// Owner-scoped active uniqueness: the same `(block_type, canonical_name)`
/// may be active under different owners but not twice under one owner.
#[tokio::test]
async fn owner_scoped_active_uniqueness_via_store() {
    let (pool, _dir) = migrated_pool().await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER).await;
    let store = SqliteKbStore::new(pool.clone());

    store
        .insert_knowledge_entry(record_with_body(
            KnowledgeOwnerRef::world(WORLD_A),
            "Shared name",
        ))
        .await
        .unwrap();
    // Cross-owner same-name is accepted.
    store
        .insert_knowledge_entry(record_with_body(
            KnowledgeOwnerRef::character(CHARACTER),
            "Shared name",
        ))
        .await
        .unwrap();
    // Same-owner duplicate is rejected with the owner on the error.
    let err = store
        .insert_knowledge_entry(record_with_body(
            KnowledgeOwnerRef::character(CHARACTER),
            "Shared name",
        ))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        KbStoreError::Duplicate {
            owner: KnowledgeOwnerRef::character(CHARACTER),
            name: "Shared name".to_string(),
            block_type: BlockType::Character,
        }
    );
}

// v1.184 P1 fix parity: the SQLite store must reject `creator_only` on a
// Character- or binding-owned record with a structured validation error
// *before* the insert (the schema CHECK remains defense in depth), matching
// the in-memory / conversion boundaries.
#[tokio::test]
async fn insert_rejects_creator_only_on_non_world_owner() {
    let (pool, _dir) = migrated_pool().await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER).await;
    let store = SqliteKbStore::new(pool);

    for owner in [
        KnowledgeOwnerRef::character(CHARACTER),
        KnowledgeOwnerRef::actor_world_binding(BINDING),
    ] {
        let mut rec = record_with_body(owner.clone(), "Flagged");
        rec.creator_only = true;
        let err = store.insert_knowledge_entry(rec).await.unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ValidationLegacy(msg) if msg.contains("creator_only")),
            "creator_only on {owner:?} must be rejected with a validation error, got {err:?}"
        );
    }
}

// World-owned creator_only remains accepted (DB CHECK allows it).
#[tokio::test]
async fn insert_accepts_creator_only_on_world_owner() {
    let (pool, _dir) = migrated_pool().await;
    seed_world(&pool, WORLD_A).await;
    let store = SqliteKbStore::new(pool);

    let mut rec = record_with_body(KnowledgeOwnerRef::world(WORLD_A), "Flagged");
    rec.creator_only = true;
    let result = store.insert_knowledge_entry(rec).await.unwrap();
    assert_eq!(result.owner, KnowledgeOwnerRef::world(WORLD_A));

    // The typed creator_only column round-trips to true.
    let got = store
        .get_knowledge_entry(&result.entry_id)
        .await
        .unwrap();
    assert!(got.creator_only);
}
