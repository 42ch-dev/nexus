//! v1.184 P1 Task 2 — owner-aware `SqliteKbStore` proofs, plus the v1.191 P1
//! T6 pre-limit visibility matrix.
//!
//! Owner cutover coverage (`KnowledgeEntryRecord` / `KnowledgeOwnerRef`):
//! - all three closed owner kinds round-trip through insert/get/list;
//! - non-World owners never receive a fabricated `world_id` (typed column or
//!   `extensions.nexus` metadata);
//! - owner and the native governance pair are immutable through
//!   `update_knowledge_entry`;
//! - unknown `extensions.nexus` keys survive the read-modify-write cycle for
//!   every owner kind;
//! - legacy World-only behavior (`list_by_world`, `query`, world-scoped
//!   uniqueness) is unchanged.
//!
//! Visibility coverage (`v1191_holder_visibility`, durable §4.2): the admitted
//! read selection is a SQL eligibility predicate applied **before** the keyset
//! cursor, `LIMIT`, count and snippet observation, over two Creators, two
//! Characters, two Worlds and multiple bindings.

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{
    KnowledgeEntryBody, KnowledgeEntryRecord, KnowledgeOwnerRef, DISCLOSURE_OWNER_PRIVATE,
};
use nexus_knowledge::world_kb::query::KbQuery;
use nexus_knowledge::world_kb::store::{KbStoreError, KnowledgeReadScope};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use sqlx::SqlitePool;
use std::collections::BTreeMap;

const CREATOR: &str = "ctr_cccccccccccccccccccccccccccccccc";
const OTHER_CREATOR: &str = "ctr_dddddddddddddddddddddddddddddddd";
const WORLD_A: &str = "wld_ownerstore_a";
const WORLD_B: &str = "wld_ownerstore_b";
const CHARACTER_1: &str = "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CHARACTER_2: &str = "chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const BINDING_1: &str = "awb_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BINDING_2: &str = "awb_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

// ── Pool / seed helpers (same shapes as kb_owner_scope_migration.rs) ──────

/// Fully migrated, protocol-admitted pool. The KB tables carry writer
/// guards whose scalar functions are connection-local, so fixture writes must
/// go through the production factory rather than a raw pool.
async fn migrated_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = nexus_local_db::init_pool(&dir.path().join("test.db"))
        .await
        .unwrap();
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

/// Seed a Character row plus its registry holder (the FK on
/// `kb_key_blocks.holder_entry_id` requires the row to exist), returning the
/// stable holder id.
async fn seed_character(pool: &SqlitePool, character_id: &str) -> String {
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
    let mut tx = nexus_local_db::begin_immediate(pool).await.unwrap();
    let holder = nexus_local_db::ensure_character_holder_in_tx(&mut tx, character_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    holder
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

/// Raw owner + governance columns for assertions.
#[allow(clippy::type_complexity)]
async fn raw_owner_row(
    pool: &SqlitePool,
    key_block_id: &str,
) -> (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    sqlx::query_as(
        "SELECT owner_kind, world_id, character_id, actor_world_binding_id, holder_entry_id, \
         disclosure, extensions_nexus_json FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(key_block_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn record_with_body(owner: &KnowledgeOwnerRef, name: &str) -> KnowledgeEntryRecord {
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

    let rec = record_with_body(&KnowledgeOwnerRef::world(WORLD_A), "Aria");
    let entry_id = rec.entry_id.clone();
    let res = store.insert_knowledge_entry(rec).await.unwrap();
    assert_eq!(res.owner, KnowledgeOwnerRef::world(WORLD_A));

    let got = store.get_knowledge_entry(&entry_id).await.unwrap();
    assert_eq!(got.owner, KnowledgeOwnerRef::world(WORLD_A));
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
    let dup = record_with_body(&KnowledgeOwnerRef::world(WORLD_A), "Aria");
    let err = store.insert_knowledge_entry(dup).await.unwrap_err();
    assert_eq!(
        err,
        KbStoreError::Duplicate {
            owner: KnowledgeOwnerRef::world(WORLD_A),
            name: "Aria".to_string(),
            block_type: BlockType::Character,
        }
    );

    // The persisted owner columns + extensions metadata are the World shape,
    // and an ordinary create carries no governance (shared is the absence of
    // both columns, durable §3).
    let (kind, world_id, character_id, binding_id, holder, disclosure, extensions) =
        raw_owner_row(&pool, &entry_id).await;
    assert_eq!(kind, "world");
    assert_eq!(world_id.as_deref(), Some(WORLD_A));
    assert_eq!(character_id, None);
    assert_eq!(binding_id, None);
    assert_eq!(holder, None);
    assert_eq!(disclosure, None);
    let extensions = extensions.unwrap();
    assert!(extensions.contains("\"world_id\""));
}

/// Character owner: round-trips, is invisible to world-scoped reads, and
/// never receives a fabricated `world_id` (column or extensions metadata).
#[tokio::test]
async fn character_owner_round_trip_without_world_fabrication() {
    let (pool, _dir) = migrated_pool().await;
    seed_world(&pool, WORLD_A).await;
    seed_character(&pool, CHARACTER_1).await;
    let store = SqliteKbStore::new(pool.clone());

    let rec = record_with_body(&KnowledgeOwnerRef::character(CHARACTER_1), "Shared lore");
    let entry_id = rec.entry_id.clone();
    let res = store.insert_knowledge_entry(rec).await.unwrap();
    assert_eq!(res.owner, KnowledgeOwnerRef::character(CHARACTER_1));

    let got = store.get_knowledge_entry(&entry_id).await.unwrap();
    assert_eq!(got.owner, KnowledgeOwnerRef::character(CHARACTER_1));
    assert_eq!(got.world_id(), None);
    assert_eq!(
        got.holder_entry_id, None,
        "an ordinary create leaves governance unset"
    );

    // Invisible to legacy world-scoped reads.
    assert!(store.list_by_world(WORLD_A).await.unwrap().is_empty());
    assert_eq!(
        store
            .query(&KbQuery::new(WORLD_A))
            .await
            .unwrap()
            .total_count,
        0
    );

    // Owner-scoped listing finds it.
    let owned = store
        .list_by_owner(&KnowledgeOwnerRef::character(CHARACTER_1))
        .await
        .unwrap();
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].entry_id, entry_id);

    // No fabricated world anywhere in storage.
    let (kind, world_id, character_id, binding_id, _, _, extensions) =
        raw_owner_row(&pool, &entry_id).await;
    assert_eq!(kind, "character");
    assert_eq!(world_id, None);
    assert_eq!(character_id.as_deref(), Some(CHARACTER_1));
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
    seed_character(&pool, CHARACTER_1).await;
    seed_binding(&pool, BINDING_1, CHARACTER_1, WORLD_A).await;
    let store = SqliteKbStore::new(pool.clone());

    let rec = record_with_body(
        &KnowledgeOwnerRef::actor_world_binding(BINDING_1),
        "Private note",
    );
    let entry_id = rec.entry_id.clone();
    store.insert_knowledge_entry(rec).await.unwrap();

    let got = store.get_knowledge_entry(&entry_id).await.unwrap();
    assert_eq!(got.owner, KnowledgeOwnerRef::actor_world_binding(BINDING_1));
    assert_eq!(got.world_id(), None);

    assert!(store.list_by_world(WORLD_A).await.unwrap().is_empty());
    let owned = store
        .list_by_owner(&KnowledgeOwnerRef::actor_world_binding(BINDING_1))
        .await
        .unwrap();
    assert_eq!(owned.len(), 1);

    let (kind, world_id, character_id, binding_id, _, _, extensions) =
        raw_owner_row(&pool, &entry_id).await;
    assert_eq!(kind, "actor_world_binding");
    assert_eq!(world_id, None);
    assert_eq!(character_id, None);
    assert_eq!(binding_id.as_deref(), Some(BINDING_1));
    let extensions = extensions.unwrap();
    assert!(extensions.contains("\"actor_world_binding_id\""));
    assert!(
        !extensions.contains("\"world_id\""),
        "binding-owned row must not carry a world_id extension: {extensions}"
    );
}

/// Owner and the native governance pair are immutable through
/// `update_knowledge_entry`; a changed owner or governance column is rejected
/// and the stored row is untouched.
#[tokio::test]
async fn owner_and_governance_are_immutable_through_update() {
    let (pool, _dir) = migrated_pool().await;
    seed_world(&pool, WORLD_A).await;
    seed_world(&pool, WORLD_B).await;
    let character_holder = seed_character(&pool, CHARACTER_1).await;
    let store = SqliteKbStore::new(pool.clone());

    let rec = record_with_body(&KnowledgeOwnerRef::world(WORLD_A), "Aria");
    let entry_id = rec.entry_id.clone();
    store.insert_knowledge_entry(rec).await.unwrap();

    // Owner change (World A → Character) is rejected.
    let mut moved = store.get_knowledge_entry(&entry_id).await.unwrap();
    moved.owner = KnowledgeOwnerRef::character(CHARACTER_1);
    let err = store.update_knowledge_entry(moved).await.unwrap_err();
    assert_eq!(err, KbStoreError::ImmutableOwner(entry_id.clone()));

    // World → other World is still an owner change.
    let mut moved = store.get_knowledge_entry(&entry_id).await.unwrap();
    moved.owner = KnowledgeOwnerRef::world(WORLD_B);
    let err = store.update_knowledge_entry(moved).await.unwrap_err();
    assert_eq!(err, KbStoreError::ImmutableOwner(entry_id.clone()));

    // A governance flip through the ordinary store path is rejected: audience
    // authoring owns that transition, `KbStore::update` is not a transfer
    // mechanism (durable §3).
    let mut flipped = store.get_knowledge_entry(&entry_id).await.unwrap();
    flipped.holder_entry_id = Some(character_holder.clone());
    flipped.disclosure = Some(DISCLOSURE_OWNER_PRIVATE.to_string());
    let err = store.update_knowledge_entry(flipped).await.unwrap_err();
    assert_eq!(err, KbStoreError::ImmutableGovernance(entry_id.clone()));

    // The stored row is untouched.
    let (kind, world_id, _, _, holder, disclosure, _) = raw_owner_row(&pool, &entry_id).await;
    assert_eq!(kind, "world");
    assert_eq!(world_id.as_deref(), Some(WORLD_A));
    assert_eq!(holder, None);
    assert_eq!(disclosure, None);

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
    seed_character(&pool, CHARACTER_1).await;
    seed_binding(&pool, BINDING_1, CHARACTER_1, WORLD_A).await;
    let store = SqliteKbStore::new(pool.clone());

    for owner in [
        KnowledgeOwnerRef::world(WORLD_A),
        KnowledgeOwnerRef::character(CHARACTER_1),
        KnowledgeOwnerRef::actor_world_binding(BINDING_1),
    ] {
        let mut rec = record_with_body(&owner, &format!("extras {}", owner.kind()));
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
    seed_character(&pool, CHARACTER_1).await;
    let store = SqliteKbStore::new(pool.clone());

    store
        .insert_knowledge_entry(record_with_body(
            &KnowledgeOwnerRef::world(WORLD_A),
            "Shared name",
        ))
        .await
        .unwrap();
    // Cross-owner same-name is accepted.
    store
        .insert_knowledge_entry(record_with_body(
            &KnowledgeOwnerRef::character(CHARACTER_1),
            "Shared name",
        ))
        .await
        .unwrap();
    // Same-owner duplicate is rejected with the owner on the error.
    let err = store
        .insert_knowledge_entry(record_with_body(
            &KnowledgeOwnerRef::character(CHARACTER_1),
            "Shared name",
        ))
        .await
        .unwrap_err();
    assert_eq!(
        err,
        KbStoreError::Duplicate {
            owner: KnowledgeOwnerRef::character(CHARACTER_1),
            name: "Shared name".to_string(),
            block_type: BlockType::Character,
        }
    );
}

// ── v1.191 P1 T6 — pre-limit native visibility (durable §4.2) ──────────────
//
// One fixture: two Creators, two Characters, two Worlds and three bindings,
// with shared, creator-private, Character-private, foreign-holder-private and
// container-local rows. The ordering below is deliberate — hidden rows sit
// *before* and *between* eligible ones so that a post-`LIMIT` filter would
// return an empty or short page, drop `has_more`, or leak a count/snippet.

/// A governed row: `disclosure = owner-private` under a resolved holder.
#[allow(clippy::needless_pass_by_value)] // callers build the owner inline
fn governed_entry(
    owner: KnowledgeOwnerRef,
    name: &str,
    created_at: &str,
    holder: Option<&str>,
    disclosure: Option<&str>,
) -> KnowledgeEntryRecord {
    let mut row = match &owner {
        KnowledgeOwnerRef::World(world_id) => {
            KnowledgeEntryRecord::new(world_id, BlockType::Item, name)
        }
        KnowledgeOwnerRef::Character(character_id) => {
            KnowledgeEntryRecord::for_character(character_id, BlockType::Item, name)
        }
        KnowledgeOwnerRef::ActorWorldBinding(binding_id) => {
            KnowledgeEntryRecord::for_binding(binding_id, BlockType::Item, name)
        }
    };
    row.created_at = created_at.to_string();
    row.holder_entry_id = holder.map(str::to_string);
    row.disclosure = disclosure.map(str::to_string);
    row.body = Some(KnowledgeEntryBody {
        // Every row carries a searchable term; the hidden ones carry the
        // *same* term as a visible row so a leaking search is observable in
        // both the count and the snippet.
        summary: Some(format!("{name} beacon note")),
        ..KnowledgeEntryBody::default()
    });
    row
}

struct VisibilityFixture {
    character_1_holder: String,
    character_2_holder: String,
    creator_holder: String,
    other_creator_holder: String,
    /// `canonical_name` → `key_block_id`, for detail reads.
    ids: BTreeMap<String, String>,
}

/// Eligible for the `CHARACTER_1` `ActorView` over `WORLD_A`, in keyset order.
const CHARACTER_1_ELIGIBLE: [&str; 7] = [
    "WorldShared",
    "WorldSharedBeacon",
    "WorldChar1Private",
    "Char1Shared",
    "Char1Private",
    "Bind1Shared",
    "Bind1Private",
];

const HIDDEN_FROM_CHARACTER_1: [&str; 6] = [
    "WorldCreatorPrivate",
    "WorldForeignHolderPrivate",
    "WorldChar2Private",
    "Char1ForeignHolderPrivate",
    "Char2Private",
    "Bind2Private",
];

#[allow(clippy::too_many_lines)] // one fixture asserted across every case below
async fn seed_visibility_fixture(pool: &SqlitePool) -> VisibilityFixture {
    nexus_local_db::ensure_creator_row(pool, OTHER_CREATOR, "Other")
        .await
        .unwrap();
    seed_world(pool, WORLD_A).await;
    seed_world(pool, WORLD_B).await;
    let character_1_holder = seed_character(pool, CHARACTER_1).await;
    let character_2_holder = seed_character(pool, CHARACTER_2).await;
    seed_binding(pool, BINDING_1, CHARACTER_1, WORLD_A).await;
    seed_binding(pool, BINDING_2, CHARACTER_2, WORLD_A).await;
    let creator_holder = nexus_local_db::creator_holder_entry_id(CREATOR);
    let other_creator_holder = nexus_local_db::creator_holder_entry_id(OTHER_CREATOR);

    let store = SqliteKbStore::new(pool.clone());
    let rows = vec![
        // World A — hidden rows deliberately lead the order.
        governed_entry(
            KnowledgeOwnerRef::world(WORLD_A),
            "WorldCreatorPrivate",
            "2026-01-01T00:00:01Z",
            Some(&creator_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::world(WORLD_A),
            "WorldForeignHolderPrivate",
            "2026-01-01T00:00:02Z",
            Some(&other_creator_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::world(WORLD_A),
            "WorldShared",
            "2026-01-01T00:00:03Z",
            None,
            None,
        ),
        governed_entry(
            KnowledgeOwnerRef::world(WORLD_A),
            "WorldChar2Private",
            "2026-01-01T00:00:04Z",
            Some(&character_2_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::world(WORLD_A),
            "WorldSharedBeacon",
            "2026-01-01T00:00:05Z",
            None,
            None,
        ),
        governed_entry(
            KnowledgeOwnerRef::world(WORLD_A),
            "WorldChar1Private",
            "2026-01-01T00:00:06Z",
            Some(&character_1_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        // Character 1 container: a foreign-holder private row (storable: the
        // holder exists, the container is the Character's own) between two
        // eligible rows.
        governed_entry(
            KnowledgeOwnerRef::character(CHARACTER_1),
            "Char1ForeignHolderPrivate",
            "2026-01-01T00:00:07Z",
            Some(&character_2_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::character(CHARACTER_1),
            "Char1Shared",
            "2026-01-01T00:00:08Z",
            None,
            None,
        ),
        governed_entry(
            KnowledgeOwnerRef::character(CHARACTER_1),
            "Char1Private",
            "2026-01-01T00:00:09Z",
            Some(&character_1_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        // Character 2 and binding 2 are outside CHARACTER_1's selection.
        governed_entry(
            KnowledgeOwnerRef::character(CHARACTER_2),
            "Char2Private",
            "2026-01-01T00:00:10Z",
            Some(&character_2_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::actor_world_binding(BINDING_1),
            "Bind1Shared",
            "2026-01-01T00:00:11Z",
            None,
            None,
        ),
        governed_entry(
            KnowledgeOwnerRef::actor_world_binding(BINDING_1),
            "Bind1Private",
            "2026-01-01T00:00:12Z",
            Some(&character_1_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::actor_world_binding(BINDING_2),
            "Bind2Private",
            "2026-01-01T00:00:13Z",
            Some(&character_2_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        // Another World of the same Creator: authorized for management, never
        // for a World-scoped Character selection.
        governed_entry(
            KnowledgeOwnerRef::world(WORLD_B),
            "WorldBShared",
            "2026-01-01T00:00:14Z",
            None,
            None,
        ),
    ];

    let mut ids = BTreeMap::new();
    for row in rows {
        let name = row.canonical_name.clone();
        let inserted = store.insert_knowledge_entry(row).await.unwrap();
        ids.insert(name, inserted.entry_id);
    }

    VisibilityFixture {
        character_1_holder,
        character_2_holder,
        creator_holder,
        other_creator_holder,
        ids,
    }
}

/// `CHARACTER_1`'s `ActorView`: World A + the Character + this binding.
fn character_1_view(fixture: &VisibilityFixture) -> KnowledgeReadScope {
    KnowledgeReadScope::actor_view(
        fixture.character_1_holder.clone(),
        vec![
            KnowledgeOwnerRef::world(WORLD_A),
            KnowledgeOwnerRef::character(CHARACTER_1),
            KnowledgeOwnerRef::actor_world_binding(BINDING_1),
        ],
    )
    .unwrap()
}

/// The Creator's management review over World A: owned containers plus the
/// known-governance holder set.
fn creator_management(fixture: &VisibilityFixture) -> KnowledgeReadScope {
    KnowledgeReadScope::creator_management(
        vec![
            KnowledgeOwnerRef::world(WORLD_A),
            KnowledgeOwnerRef::character(CHARACTER_1),
            KnowledgeOwnerRef::character(CHARACTER_2),
            KnowledgeOwnerRef::actor_world_binding(BINDING_1),
            KnowledgeOwnerRef::actor_world_binding(BINDING_2),
        ],
        vec![
            fixture.creator_holder.clone(),
            fixture.character_1_holder.clone(),
            fixture.character_2_holder.clone(),
        ],
    )
}

/// Every eligible row of one selection, as sorted names.
async fn visible_names(store: &SqliteKbStore, selection: &KnowledgeReadScope) -> Vec<String> {
    let mut names = Vec::new();
    for owner in selection.containers() {
        for row in store
            .list_by_owner_complete(owner, selection)
            .await
            .unwrap()
        {
            names.push(row.canonical_name);
        }
    }
    names.sort();
    names
}

/// The keyset page walk over one selection container, one row per page: a
/// hidden row may not consume a page slot, end the walk (`has_more`) or be
/// paged into. This is the regression for filtering *after* `LIMIT`.
async fn paged_names(
    store: &SqliteKbStore,
    owner: &KnowledgeOwnerRef,
    selection: &KnowledgeReadScope,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor: Option<(String, String)> = None;
    for _ in 0..64 {
        let page = store
            .list_by_owner_keyset(owner, cursor.as_ref(), 1, selection)
            .await
            .unwrap();
        let Some(row) = page.first() else {
            break;
        };
        assert_eq!(page.len(), 1, "one-row page");
        names.push(row.canonical_name.clone());
        cursor = Some((row.created_at.clone(), row.entry_id.clone()));
    }
    names
}

/// The page boundary and container selection: a hidden row before, between and
/// after eligible rows never occupies a slot, and the walk returns exactly the
/// eligible rows in keyset order.
#[tokio::test]
async fn v1191_holder_visibility_keyset_pages_skip_hidden_before_limit() {
    let (pool, _dir) = migrated_pool().await;
    let fixture = seed_visibility_fixture(&pool).await;
    let store = SqliteKbStore::new(pool.clone());
    let selection = character_1_view(&fixture);

    let world_owner = KnowledgeOwnerRef::world(WORLD_A);
    assert_eq!(
        paged_names(&store, &world_owner, &selection).await,
        vec!["WorldShared", "WorldSharedBeacon", "WorldChar1Private"],
        "the World container pages only its eligible rows"
    );

    // The same walk over the Character and binding containers keeps the
    // foreign-holder private row and the other binding's rows out.
    assert_eq!(
        paged_names(
            &store,
            &KnowledgeOwnerRef::character(CHARACTER_1),
            &selection
        )
        .await,
        vec!["Char1Shared", "Char1Private"]
    );
    assert_eq!(
        paged_names(
            &store,
            &KnowledgeOwnerRef::actor_world_binding(BINDING_1),
            &selection
        )
        .await,
        vec!["Bind1Shared", "Bind1Private"]
    );

    // A page boundary that a post-filter would turn into a phantom empty page:
    // the first World page of a 2-row window starts after two hidden rows.
    let first_page = store
        .list_by_owner_keyset(&world_owner, None, 2, &selection)
        .await
        .unwrap();
    let names: Vec<&str> = first_page
        .iter()
        .map(|row| row.canonical_name.as_str())
        .collect();
    assert_eq!(names, vec!["WorldShared", "WorldSharedBeacon"]);
    let after = (
        first_page[1].created_at.clone(),
        first_page[1].entry_id.clone(),
    );
    let second_page = store
        .list_by_owner_keyset(&world_owner, Some(&after), 2, &selection)
        .await
        .unwrap();
    let names: Vec<&str> = second_page
        .iter()
        .map(|row| row.canonical_name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["WorldChar1Private"],
        "the page after the cursor holds the next eligible row, not a hidden one"
    );

    // Complete snapshots are the same eligible sets, and an owner outside the
    // selection yields nothing (no widening by asking for a foreign container).
    assert_eq!(
        visible_names(&store, &selection).await,
        {
            let mut expected = CHARACTER_1_ELIGIBLE.to_vec();
            expected.sort_unstable();
            expected
        },
        "hidden and absent are indistinguishable: the snapshot holds eligible rows only"
    );
    assert!(
        store
            .list_by_owner_complete(&KnowledgeOwnerRef::world(WORLD_B), &selection)
            .await
            .unwrap()
            .is_empty(),
        "a World outside the authorized containers contributes no rows"
    );
    assert!(store
        .list_by_owner_keyset(&KnowledgeOwnerRef::world(WORLD_B), None, 5, &selection)
        .await
        .unwrap()
        .is_empty());
    assert!(
        store
            .list_by_owner_complete(
                &KnowledgeOwnerRef::actor_world_binding(BINDING_2),
                &selection
            )
            .await
            .unwrap()
            .is_empty(),
        "binding-local isolation: another binding's rows never join this selection"
    );
}

/// Search: the count, the returned rows and their snippets are computed over
/// the eligible set only, while a hidden row that matches the term stays
/// invisible — and is still there for an unscoped read.
#[tokio::test]
async fn v1191_holder_visibility_search_counts_and_snippets_skip_hidden() {
    let (pool, _dir) = migrated_pool().await;
    let fixture = seed_visibility_fixture(&pool).await;
    let store = SqliteKbStore::new(pool.clone());
    let selection = character_1_view(&fixture);

    // Every row carries the term "beacon"; a hidden World row carries it too.
    let unscoped = store.query(&KbQuery::new(WORLD_A)).await.unwrap();
    assert_eq!(
        unscoped.total_count, 6,
        "the unscoped World listing sees every World-A row"
    );

    let scoped = store
        .query_with_scope(
            &KbQuery::new(WORLD_A).with_text_search("beacon"),
            &selection,
        )
        .await
        .unwrap();
    let names: Vec<&str> = scoped
        .items
        .iter()
        .map(|row| row.canonical_name.as_str())
        .collect();
    assert_eq!(
        (names, scoped.total_count),
        (
            vec!["WorldShared", "WorldSharedBeacon", "WorldChar1Private"],
            3
        ),
        "the search counts and returns eligible matches only"
    );
    for item in &scoped.items {
        let summary = item
            .body
            .as_ref()
            .and_then(|body| body.summary.as_deref())
            .unwrap_or_default();
        assert!(
            !summary.contains("WorldCreatorPrivate"),
            "no snippet may carry a hidden row: {summary}"
        );
    }

    // A term only a hidden row matches yields an empty, zero-count result even
    // though the row exists and matches for the unscoped path.
    let hidden_term = store
        .query(&KbQuery::new(WORLD_A).with_text_search("worldcreatorprivate"))
        .await
        .unwrap();
    assert_eq!(
        hidden_term.total_count, 1,
        "the hidden row exists and matches the unscoped search"
    );
    let scoped_hidden = store
        .query_with_scope(
            &KbQuery::new(WORLD_A).with_text_search("worldcreatorprivate"),
            &selection,
        )
        .await
        .unwrap();
    assert_eq!(
        (scoped_hidden.items.len(), scoped_hidden.total_count),
        (0, 0),
        "a hidden match is not scored, counted or returned"
    );

    // A World the selection never authorized yields the empty result, not a
    // filtered view of another World.
    let foreign = store
        .query_with_scope(
            &KbQuery::new(WORLD_B).with_text_search("beacon"),
            &selection,
        )
        .await
        .unwrap();
    assert_eq!((foreign.items.len(), foreign.total_count), (0, 0));

    // Management review searches the owned known-private rows too — every
    // World-A row but the foreign holder's.
    let management = store
        .query_with_scope(
            &KbQuery::new(WORLD_A).with_text_search("beacon"),
            &creator_management(&fixture),
        )
        .await
        .unwrap();
    let mut management_names: Vec<&str> = management
        .items
        .iter()
        .map(|row| row.canonical_name.as_str())
        .collect();
    management_names.sort_unstable();
    assert_eq!(
        (management_names, management.total_count),
        (
            vec![
                "WorldChar1Private",
                "WorldChar2Private",
                "WorldCreatorPrivate",
                "WorldShared",
                "WorldSharedBeacon",
            ],
            5
        ),
        "management counts the owned private rows, not the foreign holder's"
    );
}

/// The detail read is the same rule in a single-row `WHERE`: a row hidden from
/// the admitted Character is indistinguishable from a missing one.
#[tokio::test]
async fn v1191_holder_visibility_detail_hidden_is_absent() {
    let (pool, _dir) = migrated_pool().await;
    let fixture = seed_visibility_fixture(&pool).await;

    let own = nexus_local_db::get_actor_knowledge_entry(
        &pool,
        CREATOR,
        CHARACTER_1,
        &fixture.ids["Char1Private"],
    )
    .await
    .unwrap();
    assert_eq!(
        own.map(|row| row.canonical_name),
        Some("Char1Private".to_string()),
        "the admitted Character reads its own private row"
    );

    let hidden = nexus_local_db::get_actor_knowledge_entry(
        &pool,
        CREATOR,
        CHARACTER_1,
        &fixture.ids["Char1ForeignHolderPrivate"],
    )
    .await
    .unwrap();
    let absent =
        nexus_local_db::get_actor_knowledge_entry(&pool, CREATOR, CHARACTER_1, "kb_never_written")
            .await
            .unwrap();
    assert_eq!(hidden, None, "a foreign-holder private row is hidden");
    assert_eq!(absent, None, "an absent row is absent");
    assert_eq!(
        hidden, absent,
        "a hidden entry id must be indistinguishable from a missing one"
    );

    // A World-owned row is outside the Character's detail container, and a
    // shared Character row stays readable.
    assert_eq!(
        nexus_local_db::get_actor_knowledge_entry(
            &pool,
            CREATOR,
            CHARACTER_1,
            &fixture.ids["WorldChar1Private"]
        )
        .await
        .unwrap(),
        None
    );
    assert_eq!(
        nexus_local_db::get_actor_knowledge_entry(
            &pool,
            CREATOR,
            CHARACTER_1,
            &fixture.ids["Char1Shared"]
        )
        .await
        .unwrap()
        .map(|row| row.canonical_name),
        Some("Char1Shared".to_string())
    );
}

/// The boundary matrix: two Creators, two Characters, two Worlds and the
/// binding containers, stated as one visibility table per policy.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one visibility table per policy
async fn v1191_holder_visibility_policy_matrix_two_of_each() {
    let (pool, _dir) = migrated_pool().await;
    let fixture = seed_visibility_fixture(&pool).await;
    let store = SqliteKbStore::new(pool.clone());

    let character_scope = character_1_view(&fixture);
    assert_eq!(
        visible_names(&store, &character_scope).await,
        {
            let mut expected = CHARACTER_1_ELIGIBLE.to_vec();
            expected.sort_unstable();
            expected
        },
        "a Character view: own holder + authorized containers, nothing else"
    );

    // The second Character sees its own private rows only.
    let character_2_scope = KnowledgeReadScope::actor_view(
        fixture.character_2_holder.clone(),
        vec![
            KnowledgeOwnerRef::world(WORLD_A),
            KnowledgeOwnerRef::character(CHARACTER_2),
            KnowledgeOwnerRef::actor_world_binding(BINDING_2),
        ],
    )
    .unwrap();
    let character_2_names = visible_names(&store, &character_2_scope).await;
    assert!(character_2_names.contains(&"WorldChar2Private".to_string()));
    assert!(character_2_names.contains(&"Char2Private".to_string()));
    assert!(character_2_names.contains(&"Bind2Private".to_string()));
    assert!(
        !character_2_names.contains(&"WorldChar1Private".to_string())
            && !character_2_names.contains(&"Char1Private".to_string()),
        "a Character view never inherits another holder's private rows: {character_2_names:?}"
    );
    // Shared rows stay visible to both Characters (no shared row is hidden by
    // the cutover), and the Character-global row of the other Character is not
    // binding-local.
    assert!(character_2_names.contains(&"WorldShared".to_string()));
    assert!(character_2_names.contains(&"WorldSharedBeacon".to_string()));
    assert!(
        !character_2_names.contains(&"Char1Shared".to_string()),
        "another Character's global sharing stays in that Character's container"
    );

    // The Creator's own ActorView holds the Creator holder, so the
    // creator-private World row is visible and the Characters' private rows
    // are not: management review is never implied by an ActorView.
    let creator_scope = KnowledgeReadScope::actor_view(
        fixture.creator_holder.clone(),
        vec![
            KnowledgeOwnerRef::world(WORLD_A),
            KnowledgeOwnerRef::character(CHARACTER_1),
            KnowledgeOwnerRef::character(CHARACTER_2),
            KnowledgeOwnerRef::actor_world_binding(BINDING_1),
            KnowledgeOwnerRef::actor_world_binding(BINDING_2),
        ],
    )
    .unwrap();
    let creator_names = visible_names(&store, &creator_scope).await;
    assert!(creator_names.contains(&"WorldCreatorPrivate".to_string()));
    assert!(creator_names.contains(&"WorldShared".to_string()));
    assert!(creator_names.contains(&"WorldSharedBeacon".to_string()));
    for hidden in [
        "WorldChar1Private",
        "WorldChar2Private",
        "Char1Private",
        "Char2Private",
        "Bind1Private",
        "Bind2Private",
    ] {
        assert!(
            !creator_names.contains(&hidden.to_string()),
            "a Creator ActorView does not inherit the management known-governance set: {hidden}"
        );
    }
    // The foreign holder's private row in the same container stays invisible.
    assert!(!creator_names.contains(&"WorldForeignHolderPrivate".to_string()));

    // CreatorManagement: owned containers + known-governance holders, so every
    // owned private row is reviewable — and only those.
    let management = creator_management(&fixture);
    let management_names = visible_names(&store, &management).await;
    for expected in [
        "WorldShared",
        "WorldSharedBeacon",
        "WorldCreatorPrivate",
        "WorldChar1Private",
        "WorldChar2Private",
        "Char1Shared",
        "Char1Private",
        "Char1ForeignHolderPrivate",
        "Char2Private",
        "Bind1Shared",
        "Bind1Private",
        "Bind2Private",
    ] {
        assert!(
            management_names.contains(&expected.to_string()),
            "management review must reach owned known-private {expected}: {management_names:?}"
        );
    }
    assert!(
        !management_names.contains(&"WorldForeignHolderPrivate".to_string()),
        "a foreign holder's row is not reviewable through this Creator's management selection"
    );
    assert!(
        !management_names.contains(&"WorldBShared".to_string()),
        "management review is container-scoped: another World's rows stay out even though \
         they are shared"
    );

    // The holder set is what gates a private row, not the container: adding
    // the other Creator's holder admits exactly that row (and only it).
    let widened = KnowledgeReadScope::creator_management(management.containers().to_vec(), {
        let mut holders = management.authorized_holders().to_vec();
        holders.push(fixture.other_creator_holder.clone());
        holders
    });
    let widened_names = visible_names(&store, &widened).await;
    assert!(
        widened_names.contains(&"WorldForeignHolderPrivate".to_string()),
        "the admitted holder set, not the container, selects private rows"
    );
    assert_eq!(
        widened_names.len(),
        management_names.len() + 1,
        "only the newly admitted holder's row is added"
    );
}

/// The storage boundary refuses states the read rule must never see: unknown
/// disclosure vocabulary, private-without-holder and an unregistered holder.
/// The reader still excludes them defensively (`disclosure = 'owner-private'`
/// plus the admitted holder set).
#[tokio::test]
async fn v1191_holder_visibility_unknown_and_malformed_governance_refused() {
    let (pool, _dir) = migrated_pool().await;
    let fixture = seed_visibility_fixture(&pool).await;
    let store = SqliteKbStore::new(pool.clone());

    let unknown = store
        .insert_knowledge_entry(governed_entry(
            KnowledgeOwnerRef::world(WORLD_B),
            "UnknownVocabulary",
            "2026-01-01T00:01:00Z",
            Some(&fixture.character_1_holder),
            Some("owner-profile"),
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(&unknown, KbStoreError::ValidationLegacy(message)
            if message.contains("unknown disclosure vocabulary")),
        "unknown disclosure vocabulary is refused, got {unknown:?}"
    );

    let holderless = store
        .insert_knowledge_entry(governed_entry(
            KnowledgeOwnerRef::world(WORLD_B),
            "PrivateWithoutHolder",
            "2026-01-01T00:01:01Z",
            None,
            Some(DISCLOSURE_OWNER_PRIVATE),
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(&holderless, KbStoreError::ValidationLegacy(message)
            if message.contains("requires a nonempty holder_entry_id")),
        "private disclosure without a holder is refused, got {holderless:?}"
    );

    let unregistered = store
        .insert_knowledge_entry(governed_entry(
            KnowledgeOwnerRef::world(WORLD_B),
            "UnregisteredHolder",
            "2026-01-01T00:01:02Z",
            Some("hld_0000000000000000000000000000000000000000000000000000000000000000"),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ))
        .await
        .unwrap_err();
    assert!(
        !matches!(unregistered, KbStoreError::Duplicate { .. }),
        "an unregistered holder is refused by the registry FK, got {unregistered:?}"
    );

    // None of the refused rows reached storage, so the selection is unchanged.
    assert_eq!(
        visible_names(&store, &character_1_view(&fixture))
            .await
            .len(),
        CHARACTER_1_ELIGIBLE.len()
    );
    let raw: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE world_id = ?")
        .bind(WORLD_B)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw, 1, "only the fixture's World-B row exists");

    // Every hidden fixture row is still hidden from the selection, one by one.
    let hidden_names = visible_names(&store, &character_1_view(&fixture)).await;
    for hidden in HIDDEN_FROM_CHARACTER_1 {
        assert!(
            !hidden_names.contains(&hidden.to_string()),
            "{hidden} must stay hidden from the CHARACTER_1 selection"
        );
    }
}
