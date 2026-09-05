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
