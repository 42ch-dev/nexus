//! P0 Task 3 — Character / binding Daemon routes.
//!
//! Covers success, foreign ids, invalid WorldSheet, duplicate binding,
//! last-binding 409, cursor pagination, and stable error envelopes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::{json, Value};

const OWNER: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORLD_A: &str = "wld_worldA";
const WORLD_B: &str = "wld_worldB";

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
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().unwrap().clone();
    seed_actor_fixture(&pool).await;
    let server = TestServer::new(api::create_router(state, DaemonApiConfig::keyless()))
        .expect("test server");
    Ctx {
        _tmp: tmp,
        server,
        pool,
    }
}

async fn seed_actor_fixture(pool: &sqlx::SqlitePool) {
    for (id, name) in [(OWNER, "Owner"), (OTHER, "Other")] {
        nexus_local_db::ensure_creator_row(pool, id, name)
            .await
            .unwrap();
    }
    for (world_id, owner) in [(WORLD_A, OWNER), (WORLD_B, OWNER)] {
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(world_id)
        .bind(owner)
        .bind(world_id)
        .bind(world_id)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn seed_sheet(
    pool: &sqlx::SqlitePool,
    key_block_id: &str,
    world_id: &str,
    block_type: &str,
    status: &str,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, body_json, created_at) \
         VALUES (?, ?, ?, 'sheet', ?, '{}', datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(block_type)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

async fn create_character(server: &TestServer, name: &str, world_id: &str) -> Value {
    let resp = server
        .post("/v1/daemon/characters")
        .json(&json!({ "display_name": name, "world_id": world_id }))
        .await;
    assert_eq!(
        resp.status_code(),
        201,
        "create {name}: {} {}",
        resp.status_code(),
        resp.text()
    );
    resp.json()
}

#[tokio::test]
async fn create_character_returns_201_with_character_and_binding() {
    let ctx = ctx().await;
    let body = create_character(&ctx.server, "Ava", WORLD_A).await;
    assert!(body["character"]["character_id"]
        .as_str()
        .unwrap()
        .starts_with("chr_"));
    assert_eq!(body["character"]["owner_creator_id"], OWNER);
    assert_eq!(body["character"]["display_name"], "Ava");
    assert_eq!(body["character"]["status"], "active");
    assert_eq!(body["binding"]["world_id"], WORLD_A);
    assert_eq!(body["binding"]["status"], "active");
    assert!(body["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .starts_with("awb_"));
}

#[tokio::test]
async fn create_rejects_unowned_world_as_not_found() {
    let ctx = ctx().await;
    let resp = ctx
        .server
        .post("/v1/daemon/characters")
        .json(&json!({ "display_name": "Ghost", "world_id": "wld_missing" }))
        .await;
    assert_eq!(resp.status_code(), 404);
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn create_rejects_invalid_world_sheet_with_stable_409() {
    let ctx = ctx().await;
    seed_sheet(&ctx.pool, "kb_wrong_type", WORLD_A, "location", "confirmed").await;
    let resp = ctx
        .server
        .post("/v1/daemon/characters")
        .json(&json!({
            "display_name": "Sheeted",
            "world_id": WORLD_A,
            "world_sheet_entry_id": "kb_wrong_type"
        }))
        .await;
    assert_eq!(resp.status_code(), 409);
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_world_sheet");
    assert_ne!(body["error"]["message"], "invalid_world_sheet");
}

#[tokio::test]
async fn show_and_list_are_active_creator_scoped() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();

    let show = ctx.server.get(&format!("/v1/daemon/characters/{id}")).await;
    assert_eq!(show.status_code(), 200);
    let shown: Value = show.json();
    assert_eq!(shown["character"]["character_id"], id);

    let list = ctx.server.get("/v1/daemon/characters").await;
    assert_eq!(list.status_code(), 200);
    let listed: Value = list.json();
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    assert_eq!(listed["pagination"]["has_more"], false);

    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES ('wld_otherWorld', 'ws', ?, 'o', 'other', 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(OTHER)
    .execute(&ctx.pool)
    .await
    .unwrap();
    let foreign = nexus_local_db::create_character_with_initial_binding(
        &ctx.pool,
        nexus_local_db::CreateCharacterParams {
            owner_creator_id: OTHER,
            display_name: "Foreign",
            image_uri: None,
            persona_json: "{}",
            world_id: "wld_otherWorld",
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();

    let hidden = ctx
        .server
        .get(&format!(
            "/v1/daemon/characters/{}",
            foreign.character.character_id
        ))
        .await;
    assert_eq!(hidden.status_code(), 404);
    let body: Value = hidden.json();
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn list_paginates_with_opaque_cursor() {
    let ctx = ctx().await;
    create_character(&ctx.server, "Alpha", WORLD_A).await;
    create_character(&ctx.server, "Beta", WORLD_A).await;
    create_character(&ctx.server, "Gamma", WORLD_A).await;

    let page1 = ctx.server.get("/v1/daemon/characters?limit=1").await;
    assert_eq!(page1.status_code(), 200);
    let p1: Value = page1.json();
    assert_eq!(p1["items"].as_array().unwrap().len(), 1);
    assert_eq!(p1["pagination"]["has_more"], true);
    let cursor = p1["pagination"]["next_cursor"].as_str().unwrap();

    let page2 = ctx
        .server
        .get(&format!("/v1/daemon/characters?limit=1&cursor={cursor}"))
        .await;
    assert_eq!(page2.status_code(), 200);
    let p2: Value = page2.json();
    assert_eq!(p2["items"].as_array().unwrap().len(), 1);
    assert_ne!(
        p1["items"][0]["character_id"],
        p2["items"][0]["character_id"]
    );

    let bad = ctx
        .server
        .get("/v1/daemon/characters?cursor=not-a-cursor")
        .await;
    assert_eq!(bad.status_code(), 422);
    let err: Value = bad.json();
    assert_eq!(err["error"]["code"], "invalid_input");
}

#[tokio::test]
async fn add_binding_duplicate_and_remove_last_are_stable_conflicts() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let first_binding = created["binding"]["binding_id"].as_str().unwrap();

    let dup = ctx
        .server
        .post(&format!("/v1/daemon/characters/{chr}/bindings"))
        .json(&json!({ "world_id": WORLD_A }))
        .await;
    assert_eq!(dup.status_code(), 409);
    let dup_body: Value = dup.json();
    assert_eq!(
        dup_body["error"]["code"],
        "duplicate_active_actor_world_binding"
    );

    let second = ctx
        .server
        .post(&format!("/v1/daemon/characters/{chr}/bindings"))
        .json(&json!({ "world_id": WORLD_B }))
        .await;
    assert_eq!(second.status_code(), 201);
    let second_body: Value = second.json();
    let second_id = second_body["binding"]["binding_id"].as_str().unwrap();

    let listed = ctx
        .server
        .get(&format!("/v1/daemon/characters/{chr}/bindings"))
        .await;
    assert_eq!(listed.status_code(), 200);
    let items = listed.json::<Value>()["items"].as_array().unwrap().len();
    assert_eq!(items, 2);

    let removed = ctx
        .server
        .delete(&format!(
            "/v1/daemon/characters/{chr}/bindings/{second_id}"
        ))
        .await;
    assert_eq!(removed.status_code(), 204);

    let last = ctx
        .server
        .delete(&format!(
            "/v1/daemon/characters/{chr}/bindings/{first_binding}"
        ))
        .await;
    assert_eq!(last.status_code(), 409);
    let last_body: Value = last.json();
    assert_eq!(
        last_body["error"]["code"],
        "last_active_actor_world_binding"
    );
    assert_ne!(
        last_body["error"]["message"],
        "last_active_actor_world_binding"
    );

    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ? AND status = 'active'",
    )
    .bind(chr)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(remaining, 1);
}
