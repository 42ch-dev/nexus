//! Unit tests for `ComputeInputBuilder` (Task 3, V1.147 P0).
//!
//! Coverage:
//! - Filter by `required_key_block_types`
//! - Referenced `_id` entries loaded
//! - Cross-world reference rejected
//! - Empty computable set → `NoComputableEntries`
//! - `narrative_state` shape + default branch

#![allow(clippy::unwrap_used, clippy::expect_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{WorldKbBody, WorldKbEntry};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::{open_pool, run_migrations};
use nexus_orchestration::compute_input_builder::{ComputeBuildError, ComputeInputBuilder};
use nexus_wasm_host::ModuleManifest;
use serde_json::{json, Map, Value};

// ── Helpers ────────────────────────────────────────────────────────────

async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("compute_input_builder_test.db");
    let pool = open_pool(&db_path).await.unwrap();
    run_migrations(&pool).await.unwrap();
    (pool, dir)
}

async fn seed_creator(pool: &sqlx::SqlitePool, creator_id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES (?, ?, 'active', datetime('now'), '{}')",
    )
    .bind(creator_id)
    .bind(creator_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_world(pool: &sqlx::SqlitePool, world_id: &str) {
    // SAFETY: test-only — uses runtime query to avoid duplicating sqlx cache.
    seed_creator(pool, "ctr_test").await;
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
         VALUES (?, 'wrk_test', 'ctr_test', 'Test World', 'test-world', 'active', 'private', 'single', '{}')",
    )
    .bind(world_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_world_with_head(pool: &sqlx::SqlitePool, world_id: &str, head_event_id: &str) {
    // SAFETY: test-only — uses runtime query to avoid duplicating sqlx cache.
    seed_creator(pool, "ctr_test").await;
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, current_timeline_head_id, metadata_json) \
         VALUES (?, 'wrk_test', 'ctr_test', 'Test World', 'test-world', 'active', 'private', 'single', ?, '{}')",
    )
    .bind(world_id)
    .bind(head_event_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a computable `WorldKbEntry` with the given block_type and attributes.
async fn seed_kb_entry(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    block_type: BlockType,
    name: &str,
    computable: bool,
) -> WorldKbEntry {
    let mut body = WorldKbBody {
        summary: Some(format!("{name} entry")),
        computable: Some(computable),
        ..Default::default()
    };
    if computable {
        body.state = Some(json!({"hp": 100, "atk": 10}));
    }
    let kb = WorldKbEntry {
        world_id: world_id.to_string(),
        block_type,
        canonical_name: name.to_string(),
        body: Some(body),
        ..WorldKbEntry::new(world_id, block_type, name)
    };
    let kb_store = SqliteKbStore::new(pool.clone());
    kb_store.insert_knowledge_entry(kb.clone()).await.unwrap();
    kb
}

fn basic_manifest(required_types: Vec<&str>) -> ModuleManifest {
    ModuleManifest {
        module_id: "test-module".to_string(),
        name: "Test Module".to_string(),
        version: "0.1.0".to_string(),
        nexus_abi_version: 1,
        required_key_block_types: required_types.into_iter().map(String::from).collect(),
        compute_export: "compute".to_string(),
        init_export: "init".to_string(),
        description: None,
        author: None,
        host_functions: vec![],
        schemas: None,
        battle_report_kind: None,
        max_fuel: None,
        max_memory_mib: None,
        max_wall_time_ms: None,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

/// Step 1: Empty computable set → `NoComputableEntries`.
#[tokio::test]
async fn empty_computable_set_returns_no_computable_entries() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_abc123").await;
    // No KB entries seeded → query returns empty set.

    let manifest = basic_manifest(vec!["character"]);
    let builder = ComputeInputBuilder::new(pool, "wld_abc123", manifest, Map::new());

    let result = builder.build().await;
    match result {
        Err(ComputeBuildError::NoComputableEntries) => {} // expected
        other => panic!("expected NoComputableEntries, got {other:?}"),
    }
}

/// Step 2: Filter by `required_key_block_types`.
#[tokio::test]
async fn filters_by_required_key_block_types() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;

    // Seed a character (computable) and an item (computable).
    let char_entry = seed_kb_entry(&pool, "wld_test", BlockType::Character, "hero", true).await;
    seed_kb_entry(&pool, "wld_test", BlockType::Item, "potion", true).await;

    // Manifest only wants characters.
    let manifest = basic_manifest(vec!["character"]);
    let builder = ComputeInputBuilder::new(pool, "wld_test", manifest, Map::new());

    let input = builder.build().await.expect("build should succeed");

    // Only the character entry should be in key_blocks.
    assert_eq!(input.key_blocks.len(), 1, "only character should be passed");
    let entry_id = input.key_blocks[0]
        .get("entry_id")
        .and_then(Value::as_str)
        .expect("entry_id present");
    assert_eq!(entry_id, char_entry.entry_id);
}

/// Step 3: Non-computable entries are excluded.
#[tokio::test]
async fn non_computable_entries_excluded() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;

    // Seed a character: one computable, one not.
    let computable = seed_kb_entry(&pool, "wld_test", BlockType::Character, "hero", true).await;
    seed_kb_entry(&pool, "wld_test", BlockType::Character, "npc", false).await;

    let manifest = basic_manifest(vec!["character"]);
    let builder = ComputeInputBuilder::new(pool, "wld_test", manifest, Map::new());

    let input = builder.build().await.expect("build should succeed");
    assert_eq!(
        input.key_blocks.len(),
        1,
        "only computable entries should be included"
    );
    let entry_id = input.key_blocks[0]
        .get("entry_id")
        .and_then(Value::as_str)
        .expect("entry_id present");
    assert_eq!(entry_id, computable.entry_id);
}

/// Step 4: Referenced `_id` entries are loaded into key_blocks.
///
/// The attacker is computable and already in the manifest filter set;
/// the defender is non-computable so it exercises the `_id` load path
/// (not the initial query dedup branch).
#[tokio::test]
async fn referenced_id_entries_loaded() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;

    // Attacker: computable, in manifest filter → comes from initial query.
    let attacker = seed_kb_entry(&pool, "wld_test", BlockType::Character, "swordsman", true).await;
    // Defender: non-computable → must be loaded via the `_id` reference path.
    let defender = seed_kb_entry(&pool, "wld_test", BlockType::Character, "archer", false).await;

    let mut params = Map::new();
    params.insert(
        "attacker_id".to_string(),
        Value::String(attacker.entry_id.clone()),
    );
    params.insert(
        "defender_id".to_string(),
        Value::String(defender.entry_id.clone()),
    );
    // Also pass a non-_id key — should be ignored for entry loading.
    params.insert("difficulty".to_string(), Value::String("hard".to_string()));

    // Manifest only wants characters; only the attacker will be in the
    // initial computable query — the defender is loaded via `_id`.
    let manifest = basic_manifest(vec!["character"]);
    let builder = ComputeInputBuilder::new(pool, "wld_test", manifest, params);

    let input = builder.build().await.expect("build should succeed");

    // Both characters should be present (no duplicates).
    let entry_ids: Vec<&str> = input
        .key_blocks
        .iter()
        .filter_map(|kb| kb.get("entry_id").and_then(Value::as_str))
        .collect();
    assert_eq!(
        entry_ids.len(),
        2,
        "attacker (query) + defender (loaded via _id)"
    );
    assert!(entry_ids.contains(&attacker.entry_id.as_str()));
    assert!(entry_ids.contains(&defender.entry_id.as_str()));
}

/// Step 5: Cross-world reference is rejected.
#[tokio::test]
async fn cross_world_reference_rejected() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;
    seed_world(&pool, "wld_other").await;

    // Seed a character in the primary world.
    seed_kb_entry(&pool, "wld_test", BlockType::Character, "hero", true).await;
    // Seed a character in a DIFFERENT world.
    let other_entry =
        seed_kb_entry(&pool, "wld_other", BlockType::Character, "stranger", true).await;

    let mut params = Map::new();
    params.insert(
        "ally_id".to_string(),
        Value::String(other_entry.entry_id.clone()),
    );

    let manifest = basic_manifest(vec!["character"]);
    let builder = ComputeInputBuilder::new(pool, "wld_test", manifest, params);

    let result = builder.build().await;
    match result {
        Err(ComputeBuildError::ReferencedEntryNotInWorld(msg)) => {
            assert!(
                msg.contains("belongs to world"),
                "error message should mention world mismatch, got: {msg}"
            );
            assert!(
                msg.contains(&other_entry.entry_id),
                "error should name the offending entry id"
            );
            assert!(
                msg.contains("wld_other"),
                "error should mention the referenced world"
            );
        }
        other => panic!("expected ReferencedEntryNotInWorld, got {other:?}"),
    }
}

/// Step 5b: Referenced `*_id` entry that does not exist → `ReferencedEntryNotFound`.
#[tokio::test]
async fn referenced_entry_not_found_error() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;

    // Seed one computable entry (so the initial query is non-empty).
    seed_kb_entry(&pool, "wld_test", BlockType::Character, "hero", true).await;

    // Reference a non-existent entry ID.
    let mut params = Map::new();
    params.insert(
        "ally_id".to_string(),
        Value::String("ent_nonexistent_999".to_string()),
    );

    let manifest = basic_manifest(vec!["character"]);
    let builder = ComputeInputBuilder::new(pool, "wld_test", manifest, params);

    let result = builder.build().await;
    match result {
        Err(ComputeBuildError::ReferencedEntryNotFound(msg)) => {
            assert!(
                msg.contains("ent_nonexistent_999"),
                "error should name the missing entry id, got: {msg}"
            );
        }
        other => panic!("expected ReferencedEntryNotFound, got {other:?}"),
    }
}

/// Step 6: `narrative_state` shape includes `timeline_position: "0"`.
#[tokio::test]
async fn narrative_state_shape() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;
    seed_kb_entry(&pool, "wld_test", BlockType::Character, "hero", true).await;

    let manifest = basic_manifest(vec!["character"]);
    let builder = ComputeInputBuilder::new(pool, "wld_test", manifest, Map::new());

    let input = builder.build().await.expect("build should succeed");

    let ns = input
        .narrative_state
        .expect("narrative_state should be Some");
    assert_eq!(
        ns.timeline_position.as_deref(),
        Some("0"),
        "timeline_position defaults to 0"
    );
}

/// Step 7: `world_ref` contains `world_id`, `branch_id`, and `timeline_head_event_id`.
#[tokio::test]
async fn world_ref_contains_expected_fields() {
    let (pool, _dir) = fresh_pool().await;

    // Use an event ID format matching the canonical generate_event_id
    // pattern (evt_ + hex) to satisfy the newtype regex guard.
    let head_id = "evt_0";
    seed_world_with_head(&pool, "wld_test", head_id).await;

    seed_kb_entry(&pool, "wld_test", BlockType::Character, "hero", true).await;

    let manifest = basic_manifest(vec!["character"]);
    let builder = ComputeInputBuilder::new(pool, "wld_test", manifest, Map::new());

    let input = builder.build().await.expect("build should succeed");

    let world_ref = &input.world_ref;
    let wid = world_ref.world_id.as_ref().expect("world_id present");
    assert_eq!(wid.as_str(), "wld_test");

    // Default branch is "fbk_root" when no fork branch exists.
    let bid = world_ref.branch_id.as_deref().expect("branch_id present");
    assert_eq!(bid, "fbk_root");

    // Timeline head should be present when seeded.
    let head = world_ref
        .timeline_head_event_id
        .as_ref()
        .expect("timeline_head_event_id present when seeded");
    assert_eq!(head.as_str(), head_id);
}

/// Step 8: `invocation` is passed through verbatim.
#[tokio::test]
async fn invocation_params_passed_through() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;
    seed_kb_entry(&pool, "wld_test", BlockType::Character, "hero", true).await;

    let mut params = Map::new();
    params.insert("difficulty".to_string(), json!("hard"));
    params.insert("seed".to_string(), json!(42));

    let manifest = basic_manifest(vec!["character"]);
    let builder = ComputeInputBuilder::new(pool, "wld_test", manifest, params);

    let input = builder.build().await.expect("build should succeed");

    assert_eq!(
        input.invocation.get("difficulty").and_then(Value::as_str),
        Some("hard")
    );
    assert_eq!(
        input.invocation.get("seed").and_then(Value::as_i64),
        Some(42)
    );
}

/// Step 9: `schema_version` is set to 1.
#[tokio::test]
async fn schema_version_is_one() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;
    seed_kb_entry(&pool, "wld_test", BlockType::Character, "hero", true).await;

    let manifest = basic_manifest(vec!["character"]);
    let builder = ComputeInputBuilder::new(pool, "wld_test", manifest, Map::new());

    let input = builder.build().await.expect("build should succeed");
    assert_eq!(u64::from(input.schema_version), 1);
}

/// Step 10: Empty `required_key_block_types` passes ALL computable entries.
#[tokio::test]
async fn empty_required_types_passes_all_computable() {
    let (pool, _dir) = fresh_pool().await;
    seed_world(&pool, "wld_test").await;

    seed_kb_entry(&pool, "wld_test", BlockType::Character, "hero", true).await;
    seed_kb_entry(&pool, "wld_test", BlockType::Item, "sword", true).await;

    let manifest = basic_manifest(vec![]); // no type filter
    let builder = ComputeInputBuilder::new(pool, "wld_test", manifest, Map::new());

    let input = builder.build().await.expect("build should succeed");
    assert_eq!(input.key_blocks.len(), 2, "all computable entries passed");
}
