//! P4 Task 2 — Character ToM record/query Daemon routes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum_test::TestServer;
use nexus_contracts::BlockType;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::store::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use serde_json::{json, Value};

const OWNER: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORLD_A: &str = "wld_worldA";
const OTHER_CHR: &str = "chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct Ctx {
    _tmp: TestTempRoot,
    server: TestServer,
    pool: sqlx::SqlitePool,
}

async fn ctx() -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{OWNER}\"\n\n[active_workspace_slug_by_creator]\n\"{OWNER}\" = \"default\"\n"
        ),
    )
    .unwrap();
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    let pool = state.pool().unwrap().clone();
    seed_world(&pool).await;
    let server = TestServer::new(api::create_router(state, DaemonApiConfig::keyless()))
        .expect("test server");
    Ctx {
        _tmp: tmp,
        server,
        pool,
    }
}

async fn seed_world(pool: &sqlx::SqlitePool) {
    nexus_local_db::ensure_creator_row(pool, OWNER, "Owner")
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(WORLD_A)
    .bind(OWNER)
    .bind(WORLD_A)
    .bind(WORLD_A)
    .execute(pool)
    .await
    .unwrap();
}

async fn create_character(server: &TestServer, name: &str, world_id: &str) -> Value {
    let resp = server
        .post("/v1/daemon/characters")
        .json(&json!({ "display_name": name, "world_id": world_id }))
        .await;
    assert_eq!(resp.status_code(), 201, "{}", resp.text());
    resp.json()
}

async fn seed_carrier(pool: &sqlx::SqlitePool, character_id: &str) -> String {
    let store = SqliteKbStore::new(pool.clone());
    let mut kb = KnowledgeEntryRecord::for_character(character_id, BlockType::Character, "TomCarrier");
    kb.modules = Some(json!({ "belief": [] }));
    let id = kb.entry_id.clone();
    store.insert_knowledge_entry(kb).await.unwrap();
    id
}

fn l1_body(
    world_id: &str,
    binding_id: &str,
    carrier_id: &str,
    viewer: &str,
    expected_revision: i64,
) -> Value {
    json!({
        "world_id": world_id,
        "binding_id": binding_id,
        "carrier_entry_id": carrier_id,
        "expected_revision": expected_revision,
        "holder": viewer,
        "proposition": "I know the dock",
        "order": 1,
        "truth": "True",
        "access": "Private",
        "representation": "Explicit",
        "content_type": "Location",
        "source": "Perception",
        "context": "Neutral"
    })
}

async fn record(server: &TestServer, character_id: &str, body: Value) -> axum_test::TestResponse {
    server
        .post(&format!("/v1/daemon/characters/{character_id}/tom"))
        .json(&body)
        .await
}

async fn list_tom(
    server: &TestServer,
    character_id: &str,
    world_id: &str,
    binding_id: &str,
) -> Value {
    let path = format!(
        "/v1/daemon/characters/{character_id}/tom?world_id={world_id}&binding_id={binding_id}"
    );
    let resp = server.get(&path).await;
    assert_eq!(resp.status_code(), 200, "{}", resp.text());
    resp.json()
}

async fn mind_state_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM mind_states")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn record_l1_list_l1_before_l2_and_l2_subject_rules() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap();
    let carrier = seed_carrier(&ctx.pool, chr_a).await;

    let resp = record(
        &ctx.server,
        chr_a,
        l1_body(WORLD_A, bind_a, &carrier, chr_a, 0),
    )
    .await;
    assert_eq!(resp.status_code(), 200, "{}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["revision"], 1);

    let b = create_character(&ctx.server, "Ben", WORLD_A).await;
    let chr_b = b["character"]["character_id"].as_str().unwrap();

    let l2_ok = json!({
        "world_id": WORLD_A,
        "binding_id": bind_a,
        "carrier_entry_id": carrier,
        "expected_revision": 1,
        "holder": chr_b,
        "proposition": "Ben is cautious",
        "order": 2,
        "truth": "True",
        "access": "Private",
        "representation": "Explicit",
        "content_type": "Identity/Relation",
        "source": "Perception",
        "context": "Neutral"
    });
    let resp = record(&ctx.server, chr_a, l2_ok).await;
    assert_eq!(resp.status_code(), 200, "{}", resp.text());

    let page = list_tom(&ctx.server, chr_a, WORLD_A, bind_a).await;
    let orders: Vec<i64> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["order"].as_i64().unwrap())
        .collect();
    assert_eq!(orders, vec![1, 2]);

    let l2_self = json!({
        "world_id": WORLD_A,
        "binding_id": bind_a,
        "carrier_entry_id": carrier,
        "expected_revision": 2,
        "holder": chr_a,
        "proposition": "bad",
        "order": 2,
        "truth": "True",
        "access": "Private",
        "representation": "Explicit",
        "content_type": "Location",
        "source": "Perception",
        "context": "Neutral"
    });
    let resp = record(&ctx.server, chr_a, l2_self).await;
    assert_eq!(resp.status_code(), 422, "{}", resp.text());

    let l2_unbound = json!({
        "world_id": WORLD_A,
        "binding_id": bind_a,
        "carrier_entry_id": carrier,
        "expected_revision": 2,
        "holder": OTHER_CHR,
        "proposition": "foreign",
        "order": 2,
        "truth": "True",
        "access": "Private",
        "representation": "Explicit",
        "content_type": "Location",
        "source": "Perception",
        "context": "Neutral"
    });
    let resp = record(&ctx.server, chr_a, l2_unbound).await;
    assert_eq!(resp.status_code(), 404, "{}", resp.text());
}

#[tokio::test]
async fn foreign_carrier_alias_and_stale_revision_fail_closed() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap();
    let carrier = seed_carrier(&ctx.pool, chr_a).await;

    let world_carrier = {
        let store = SqliteKbStore::new(ctx.pool.clone());
        let mut kb = KnowledgeEntryRecord::new(WORLD_A, BlockType::Character, "WorldOwned");
        kb.modules = Some(json!({ "belief": [] }));
        let id = kb.entry_id.clone();
        store.insert_knowledge_entry(kb).await.unwrap();
        id
    };

    let before_ms = mind_state_count(&ctx.pool).await;
    let resp = record(
        &ctx.server,
        chr_a,
        l1_body(WORLD_A, bind_a, &world_carrier, chr_a, 0),
    )
    .await;
    assert_eq!(resp.status_code(), 422, "{}", resp.text());
    assert_eq!(mind_state_count(&ctx.pool).await, before_ms);

    let mut alias = l1_body(WORLD_A, bind_a, &carrier, chr_a, 0);
    alias["actor"] = json!(chr_a);
    let resp = record(&ctx.server, chr_a, alias).await;
    assert_eq!(resp.status_code(), 422, "{}", resp.text());

    let ok = record(
        &ctx.server,
        chr_a,
        l1_body(WORLD_A, bind_a, &carrier, chr_a, 0),
    )
    .await;
    assert_eq!(ok.status_code(), 200, "{}", ok.text());

    let stale = l1_body(WORLD_A, bind_a, &carrier, chr_a, 0);
    let resp = record(&ctx.server, chr_a, stale).await;
    assert_eq!(resp.status_code(), 409, "{}", resp.text());
    assert_eq!(mind_state_count(&ctx.pool).await, before_ms + 1);
}

#[tokio::test]
async fn order_outside_closed_space_and_invalid_labels_reject() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap();
    let carrier = seed_carrier(&ctx.pool, chr_a).await;
    let before_ms = mind_state_count(&ctx.pool).await;

    for bad_order in [0, 3] {
        let mut body = l1_body(WORLD_A, bind_a, &carrier, chr_a, 0);
        body["order"] = json!(bad_order);
        let resp = record(&ctx.server, chr_a, body).await;
        assert_eq!(resp.status_code(), 422, "order {bad_order}: {}", resp.text());
    }

    let mut bad_label = l1_body(WORLD_A, bind_a, &carrier, chr_a, 0);
    bad_label["truth"] = json!("Maybe");
    let resp = record(&ctx.server, chr_a, bad_label).await;
    assert_eq!(resp.status_code(), 422, "label: {}", resp.text());

    let mut alias_label = l1_body(WORLD_A, bind_a, &carrier, chr_a, 0);
    alias_label.as_object_mut().unwrap().remove("access");
    alias_label["knowledge_access"] = json!("Private");
    let resp = record(&ctx.server, chr_a, alias_label).await;
    assert_eq!(resp.status_code(), 422, "alias label: {}", resp.text());

    // Nothing mutated: no mind states, carrier revision unchanged.
    assert_eq!(mind_state_count(&ctx.pool).await, before_ms);
    let revision: i64 = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT revision FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(&carrier)
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
    .unwrap_or(0);
    assert_eq!(revision, 0);
}

#[tokio::test]
async fn record_and_list_succeed_without_agent_host() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap();
    let carrier = seed_carrier(&ctx.pool, chr_a).await;
    let resp = record(
        &ctx.server,
        chr_a,
        l1_body(WORLD_A, bind_a, &carrier, chr_a, 0),
    )
    .await;
    assert_eq!(resp.status_code(), 200, "{}", resp.text());
    let _ = list_tom(&ctx.server, chr_a, WORLD_A, bind_a).await;
}

// ── Fix round 1: adversarial admission/malformed/ordinal/bounds/error matrix ──

async fn set_character_status(pool: &sqlx::SqlitePool, character_id: &str, status: &str) {
    sqlx::query("UPDATE characters SET status = ? WHERE character_id = ?")
        .bind(status)
        .bind(character_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn set_world_status(pool: &sqlx::SqlitePool, world_id: &str, status: &str) {
    sqlx::query("UPDATE narrative_worlds SET status = ? WHERE world_id = ?")
        .bind(status)
        .bind(world_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn seed_carrier_with_modules(pool: &sqlx::SqlitePool, character_id: &str, name: &str, modules: Value) -> String {
    let store = SqliteKbStore::new(pool.clone());
    let mut kb = KnowledgeEntryRecord::for_character(character_id, BlockType::Character, name);
    kb.modules = Some(modules);
    let id = kb.entry_id.clone();
    store.insert_knowledge_entry(kb).await.unwrap();
    id
}

async fn carrier_modules_json(pool: &sqlx::SqlitePool, carrier_id: &str) -> String {
    sqlx::query_scalar("SELECT modules_json FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(carrier_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn carrier_revision(pool: &sqlx::SqlitePool, carrier_id: &str) -> i64 {
    sqlx::query_scalar::<_, Option<i64>>("SELECT revision FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(carrier_id)
        .fetch_one(pool)
        .await
        .unwrap()
        .unwrap_or(0)
}


#[tokio::test]
async fn inactive_viewer_world_and_subject_fail_closed() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap().to_string();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap().to_string();
    let carrier = seed_carrier(&ctx.pool, &chr_a).await;

    // Archived viewer: record and list reject 409 before any mutation.
    set_character_status(&ctx.pool, &chr_a, "archived").await;
    let resp = record(&ctx.server, &chr_a, l1_body(WORLD_A, &bind_a, &carrier, &chr_a, 0)).await;
    assert_eq!(resp.status_code(), 409, "archived viewer record: {}", resp.text());
    let path = format!("/v1/daemon/characters/{chr_a}/tom?world_id={WORLD_A}&binding_id={bind_a}");
    let resp = ctx.server.get(&path).await;
    assert_eq!(resp.status_code(), 409, "archived viewer list: {}", resp.text());
    set_character_status(&ctx.pool, &chr_a, "active").await;

    // Inactive world: record rejects 409.
    set_world_status(&ctx.pool, WORLD_A, "paused").await;
    let resp = record(&ctx.server, &chr_a, l1_body(WORLD_A, &bind_a, &carrier, &chr_a, 0)).await;
    assert_eq!(resp.status_code(), 409, "inactive world: {}", resp.text());
    set_world_status(&ctx.pool, WORLD_A, "active").await;

    // Archived L2 subject with an active binding still rejects.
    let b = create_character(&ctx.server, "Ben", WORLD_A).await;
    let chr_b = b["character"]["character_id"].as_str().unwrap().to_string();
    set_character_status(&ctx.pool, &chr_b, "archived").await;
    let mut l2 = l1_body(WORLD_A, &bind_a, &carrier, &chr_b, 0);
    l2["order"] = json!(2);
    let resp = record(&ctx.server, &chr_a, l2).await;
    assert_eq!(resp.status_code(), 409, "archived subject: {}", resp.text());

    assert_eq!(mind_state_count(&ctx.pool).await, 0);
    assert_eq!(carrier_revision(&ctx.pool, &carrier).await, 0);
}

#[tokio::test]
async fn malformed_modules_reject_without_rewrite_and_unknown_keys_survive() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap().to_string();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap().to_string();

    // Non-object modules: deterministic reject, no panic, bytes unchanged.
    let c1 = seed_carrier_with_modules(&ctx.pool, &chr_a, "ArrModules", json!([1, 2, 3])).await;
    let before = carrier_modules_json(&ctx.pool, &c1).await;
    let resp = record(&ctx.server, &chr_a, l1_body(WORLD_A, &bind_a, &c1, &chr_a, 0)).await;
    assert_eq!(resp.status_code(), 409, "array modules: {}", resp.text());
    assert_eq!(carrier_modules_json(&ctx.pool, &c1).await, before);

    // Non-array belief member: reject, never silently replaced with [].
    let c2 = seed_carrier_with_modules(&ctx.pool, &chr_a, "ObjBelief", json!({"belief": {"legacy": true}})).await;
    let before = carrier_modules_json(&ctx.pool, &c2).await;
    let resp = record(&ctx.server, &chr_a, l1_body(WORLD_A, &bind_a, &c2, &chr_a, 0)).await;
    assert_eq!(resp.status_code(), 409, "object belief: {}", resp.text());
    assert_eq!(carrier_modules_json(&ctx.pool, &c2).await, before);

    // Valid carrier with unknown sibling module keys: record succeeds and the
    // unknown keys round-trip verbatim through the CAS.
    let c3 = seed_carrier_with_modules(
        &ctx.pool,
        &chr_a,
        "MixedModules",
        json!({"belief": [], "mental": {"identity": {"role": "harbor_master"}}, "x_custom": {"n": 1}}),
    )
    .await;
    let resp = record(&ctx.server, &chr_a, l1_body(WORLD_A, &bind_a, &c3, &chr_a, 0)).await;
    assert_eq!(resp.status_code(), 200, "mixed modules: {}", resp.text());
    let after: Value = serde_json::from_str(&carrier_modules_json(&ctx.pool, &c3).await).unwrap();
    assert_eq!(after["mental"], json!({"identity": {"role": "harbor_master"}}));
    assert_eq!(after["x_custom"], json!({"n": 1}));
    assert_eq!(after["belief"].as_array().unwrap().len(), 1);

    assert_eq!(mind_state_count(&ctx.pool).await, 1);
}

#[tokio::test]
async fn physical_row_ordinal_survives_malformed_elements_and_cursor_pages() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap().to_string();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap().to_string();

    let valid = |text: &str| {
        json!({
            "holder": chr_a,
            "proposition": text,
            "order": 1,
            "truth": "True",
            "access": "Private",
            "representation": "Explicit",
            "content_type": "Location",
            "source": "Perception",
            "context": "Neutral"
        })
    };
    // Physical array: [valid0, garbage, valid2].
    let carrier = seed_carrier_with_modules(
        &ctx.pool,
        &chr_a,
        "OrdinalCarrier",
        json!({"belief": [valid("first"), 42, valid("third")]}),
    )
    .await;

    let path = format!(
        "/v1/daemon/characters/{chr_a}/tom?world_id={WORLD_A}&binding_id={bind_a}&limit=1"
    );
    let page1: Value = {
        let resp = ctx.server.get(&path).await;
        assert_eq!(resp.status_code(), 200, "{}", resp.text());
        resp.json()
    };
    assert_eq!(page1["items"].as_array().unwrap().len(), 1);
    assert_eq!(page1["items"][0]["row_ordinal"], 0, "first row keeps physical ordinal 0");
    assert_eq!(page1["pagination"]["has_more"], true);
    let cursor = page1["pagination"]["next_cursor"].as_str().unwrap().to_string();
    let cursor = cursor.replace('\u{1f}', "%1F");

    let page2: Value = {
        let resp = ctx.server.get(&format!("{path}&cursor={cursor}")).await;
        assert_eq!(resp.status_code(), 200, "{}", resp.text());
        resp.json()
    };
    let items = page2["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "exactly one remaining valid row: {page2}");
    assert_eq!(items[0]["row_ordinal"], 2, "row after malformed element keeps physical ordinal 2");
    assert_eq!(items[0]["carrier_entry_id"], carrier);
    assert_eq!(items[0]["proposition"], "third");
    assert_eq!(page2["pagination"]["has_more"], false);
}

#[tokio::test]
async fn corpus_and_row_caps_fail_closed_before_materialization() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap().to_string();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap().to_string();
    let path = format!("/v1/daemon/characters/{chr_a}/tom?world_id={WORLD_A}&binding_id={bind_a}");

    // Carrier corpus above the documented per-scope cap rejects deterministically.
    let store = SqliteKbStore::new(ctx.pool.clone());
    for i in 0..=200 {
        let mut kb = KnowledgeEntryRecord::for_character(&chr_a, BlockType::Character, &format!("Bulk{i}"));
        kb.modules = Some(json!({"belief": []}));
        store.insert_knowledge_entry(kb).await.unwrap();
    }
    let resp = ctx.server.get(&path).await;
    assert_eq!(resp.status_code(), 409, "carrier cap: {}", resp.text());

    // Belief rows above the per-carrier cap reject deterministically.
    let ctx2 = crate::ctx().await;
    let a2 = create_character(&ctx2.server, "Ava", WORLD_A).await;
    let chr_a2 = a2["character"]["character_id"].as_str().unwrap().to_string();
    let bind_a2 = a2["binding"]["binding_id"].as_str().unwrap().to_string();
    let rows: Vec<Value> = (0..=200)
        .map(|i| json!({"holder": chr_a2, "proposition": format!("p{i}"), "order": 1}))
        .collect();
    let _carrier = seed_carrier_with_modules(&ctx2.pool, &chr_a2, "BigBelief", json!({"belief": rows})).await;
    let resp = ctx2
        .server
        .get(&format!("/v1/daemon/characters/{chr_a2}/tom?world_id={WORLD_A}&binding_id={bind_a2}"))
        .await;
    assert_eq!(resp.status_code(), 409, "row cap: {}", resp.text());
}

#[tokio::test]
async fn storage_failure_is_internal_not_not_found() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap().to_string();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap().to_string();
    let carrier = seed_carrier(&ctx.pool, &chr_a).await;

    sqlx::query("DROP TABLE kb_key_blocks")
        .execute(&ctx.pool)
        .await
        .unwrap();
    let resp = record(&ctx.server, &chr_a, l1_body(WORLD_A, &bind_a, &carrier, &chr_a, 0)).await;
    assert_eq!(resp.status_code(), 500, "storage failure must not be a 404: {}", resp.text());
}

#[tokio::test]
async fn extreme_expected_revision_rejects_without_panic_or_mutation() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap().to_string();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap().to_string();
    let carrier = seed_carrier(&ctx.pool, &chr_a).await;

    for extreme in [i64::MAX as u64, u64::MAX] {
        let mut body = l1_body(WORLD_A, &bind_a, &carrier, &chr_a, 0);
        body["expected_revision"] = json!(extreme);
        let resp = record(&ctx.server, &chr_a, body).await;
        assert_eq!(resp.status_code(), 422, "revision {extreme}: {}", resp.text());
    }
    // i64::MAX - 1 is representable; CAS misses normally as a 409, never a panic.
    let mut body = l1_body(WORLD_A, &bind_a, &carrier, &chr_a, 0);
    body["expected_revision"] = json!(i64::MAX - 1);
    let resp = record(&ctx.server, &chr_a, body).await;
    assert_eq!(resp.status_code(), 409, "near-max revision: {}", resp.text());

    assert_eq!(mind_state_count(&ctx.pool).await, 0);
    assert_eq!(carrier_revision(&ctx.pool, &carrier).await, 0);
}

// ── Fix round 2: invalid JSON / bounded derivative history / DB-side bounds ──

async fn set_carrier_modules_text(pool: &sqlx::SqlitePool, carrier_id: &str, raw: &str) {
    sqlx::query("UPDATE kb_key_blocks SET modules_json = ? WHERE key_block_id = ?")
        .bind(raw)
        .bind(carrier_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn insert_derivative_mind_state(
    pool: &sqlx::SqlitePool,
    mind_state_id: &str,
    carrier_id: &str,
    occurred_at: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO mind_states \
         (mind_state_id, schema_version, holder_entry_id, canonical_name, occurred_at, \
          sort_key, snapshot_json, deltas_json, source_anchor_json, created_at, updated_at, \
          extensions_json) \
         VALUES (?, 1, ?, 'derivative', ?, '0001', '{}', '[]', NULL, ?, ?, '{\"nexus\":{}}')",
    )
    .bind(mind_state_id)
    .bind(carrier_id)
    .bind(occurred_at)
    .bind(created_at)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn invalid_json_modules_fails_closed_and_never_overwritten() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap().to_string();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap().to_string();
    let carrier = seed_carrier(&ctx.pool, &chr_a).await;
    set_carrier_modules_text(&ctx.pool, &carrier, "{\"belief\": [").await; // invalid JSON text

    let resp = record(&ctx.server, &chr_a, l1_body(WORLD_A, &bind_a, &carrier, &chr_a, 0)).await;
    assert_eq!(resp.status_code(), 409, "invalid json record: {}", resp.text());
    assert_eq!(
        resp.json::<Value>()["error"]["code"], "carrier_modules_invalid_json",
        "must be distinguishable from shape-malformed / absent"
    );
    // Bytes are preserved: never coerced to absent or overwritten.
    assert_eq!(carrier_modules_json(&ctx.pool, &carrier).await, "{\"belief\": [");
    assert_eq!(mind_state_count(&ctx.pool).await, 0);
    assert_eq!(carrier_revision(&ctx.pool, &carrier).await, 0);

    let path = format!("/v1/daemon/characters/{chr_a}/tom?world_id={WORLD_A}&binding_id={bind_a}");
    let resp = ctx.server.get(&path).await;
    assert_eq!(resp.status_code(), 409, "invalid json list: {}", resp.text());
    assert_eq!(
        resp.json::<Value>()["error"]["code"], "carrier_modules_invalid_json",
        "list must also fail closed on invalid persisted JSON"
    );
}

#[tokio::test]
async fn derivative_history_uses_one_grouped_row_per_carrier() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap().to_string();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap().to_string();
    let carrier = seed_carrier(&ctx.pool, &chr_a).await;

    let resp = record(&ctx.server, &chr_a, l1_body(WORLD_A, &bind_a, &carrier, &chr_a, 0)).await;
    assert_eq!(resp.status_code(), 200, "{}", resp.text());

    // Add out-of-band derivatives with far-future created_at (the anti-join
    // orders by created_at / mind_state_id) plus a run of older history; only
    // the single latest row's occurred_at must surface — never a concatenated
    // or unbounded list.
    insert_derivative_mind_state(&ctx.pool, "ms_bg_old", &carrier, "2099-01-01T00:00:00Z", "2099-01-01T00:00:00Z").await;
    insert_derivative_mind_state(&ctx.pool, "ms_bg_old2", &carrier, "2099-01-02T00:00:00Z", "2099-01-02T00:00:00Z").await;
    insert_derivative_mind_state(&ctx.pool, "ms_bg_latest", &carrier, "2099-01-03T00:00:00Z", "2099-01-03T00:00:00Z").await;

    let page = list_tom(&ctx.server, &chr_a, WORLD_A, &bind_a).await;
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["carrier_recorded_at"], "2099-01-03T00:00:00Z");
}

#[tokio::test]
async fn oversize_belief_array_rejects_via_db_probe_without_panic() {
    // The DB-side probe rejects at 201 rows before any record materialization;
    // a valid but huge corpus still yields a deterministic 409 (no panic).
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr_a = a["character"]["character_id"].as_str().unwrap().to_string();
    let bind_a = a["binding"]["binding_id"].as_str().unwrap().to_string();
    let rows: Vec<Value> = (0..=200)
        .map(|i| json!({"holder": chr_a, "proposition": format!("p{i}"), "order": 1}))
        .collect();
    let carrier = seed_carrier_with_modules(&ctx.pool, &chr_a, "Huge", json!({"belief": rows})).await;
    let path = format!("/v1/daemon/characters/{chr_a}/tom?world_id={WORLD_A}&binding_id={bind_a}");
    let resp = ctx.server.get(&path).await;
    assert_eq!(resp.status_code(), 409, "oversize list: {}", resp.text());
    assert_eq!(
        resp.json::<Value>()["error"]["code"], "view_incomplete",
        "must be a bounded-work rejection, not a read of all rows"
    );
    // Record on the same oversize carrier rejects before appending a 201st row.
    let resp = record(&ctx.server, &chr_a, l1_body(WORLD_A, &bind_a, &carrier, &chr_a, 0)).await;
    assert_eq!(resp.status_code(), 409, "oversize record: {}", resp.text());
    assert_eq!(mind_state_count(&ctx.pool).await, 0);
}
