//! P0 Task 3 — Character / binding Daemon routes.
//!
//! Covers success, foreign ids, invalid `WorldSheet`, duplicate binding,
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
    nexus_home: std::path::PathBuf,
    state: WorkspaceState,
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
    seed_actor_fixture(&pool).await;
    let server = TestServer::new(api::create_router(
        state.clone(),
        DaemonApiConfig::keyless(),
    ));
    Ctx {
        _tmp: tmp,
        server,
        pool,
        nexus_home,
        state,
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

// axum-test `AutoFuture` is not `Send` by design; this helper is only awaited on the single-threaded `#[tokio::test]` runtime.
#[allow(clippy::future_not_send)]
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
        .delete(&format!("/v1/daemon/characters/{chr}/bindings/{second_id}"))
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

fn switch_active_creator(ctx: &Ctx, creator_id: &str) {
    std::fs::write(
        ctx.nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{creator_id}\"\n\n[active_workspace_slug_by_creator]\n\"{creator_id}\" = \"default\"\n"
        ),
    )
    .unwrap();
}

fn assert_canonical_invalid_input(resp: &axum_test::TestResponse) {
    assert_eq!(resp.status_code(), 422, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["success"], false, "body={body}");
    assert_eq!(body["error"]["code"], "invalid_input", "body={body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty()),
        "body={body}"
    );
}

async fn count_bindings(pool: &sqlx::SqlitePool, character_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ?")
        .bind(character_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_foreign_character(
    pool: &sqlx::SqlitePool,
) -> nexus_local_db::character::CharacterRecord {
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES ('wld_otherWorld', 'ws', ?, 'o', 'other', 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(OTHER)
    .execute(pool)
    .await
    .unwrap();
    let foreign = nexus_local_db::create_character_with_initial_binding(
        pool,
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
    foreign.character
}

#[tokio::test]
async fn foreign_binding_routes_are_404_and_do_not_mutate() {
    let ctx = ctx().await;
    let foreign = seed_foreign_character(&ctx.pool).await;
    let chr = foreign.character_id.clone();
    let before = count_bindings(&ctx.pool, &chr).await;
    assert_eq!(before, 1);

    let add = ctx
        .server
        .post(&format!("/v1/daemon/characters/{chr}/bindings"))
        .json(&json!({ "world_id": WORLD_A }))
        .await;
    assert_eq!(add.status_code(), 404, "body={}", add.text());
    let add_body: Value = add.json();
    assert_eq!(add_body["success"], false);
    assert_eq!(add_body["error"]["code"], "not_found");

    let listed = ctx
        .server
        .get(&format!("/v1/daemon/characters/{chr}/bindings"))
        .await;
    assert_eq!(listed.status_code(), 404, "body={}", listed.text());
    let listed_body: Value = listed.json();
    assert_eq!(listed_body["error"]["code"], "not_found");

    let binding_id: String =
        sqlx::query_scalar("SELECT binding_id FROM actor_world_bindings WHERE character_id = ?")
            .bind(&chr)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    let show = ctx
        .server
        .get(&format!(
            "/v1/daemon/characters/{chr}/bindings/{binding_id}"
        ))
        .await;
    assert_eq!(show.status_code(), 404, "body={}", show.text());
    assert_eq!(show.json::<Value>()["error"]["code"], "not_found");

    let patch = ctx
        .server
        .patch(&format!(
            "/v1/daemon/characters/{chr}/bindings/{binding_id}"
        ))
        .json(&json!({ "expected_revision": 0, "world_sheet_entry_id": null }))
        .await;
    assert_eq!(patch.status_code(), 404, "body={}", patch.text());
    assert_eq!(patch.json::<Value>()["error"]["code"], "not_found");

    let removed = ctx
        .server
        .delete(&format!(
            "/v1/daemon/characters/{chr}/bindings/{binding_id}"
        ))
        .await;
    assert_eq!(removed.status_code(), 404, "body={}", removed.text());
    let removed_body: Value = removed.json();
    assert_eq!(removed_body["error"]["code"], "not_found");

    assert_eq!(count_bindings(&ctx.pool, &chr).await, before);
}

#[tokio::test]
async fn switching_active_creator_hides_owned_characters() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();

    switch_active_creator(&ctx, OTHER);

    let hidden = ctx.server.get(&format!("/v1/daemon/characters/{id}")).await;
    assert_eq!(hidden.status_code(), 404, "body={}", hidden.text());
    let hidden_body: Value = hidden.json();
    assert_eq!(hidden_body["error"]["code"], "not_found");

    let list = ctx.server.get("/v1/daemon/characters").await;
    assert_eq!(list.status_code(), 200, "body={}", list.text());
    let listed: Value = list.json();
    assert_eq!(listed["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_rejects_malformed_json_unknown_properties_and_invalid_ids_with_canonical_envelope()
{
    let ctx = ctx().await;

    let malformed = ctx
        .server
        .post("/v1/daemon/characters")
        .content_type("application/json")
        .text("{oops")
        .await;
    assert_canonical_invalid_input(&malformed);

    let unknown = ctx
        .server
        .post("/v1/daemon/characters")
        .json(&json!({
            "display_name": "Ada",
            "world_id": WORLD_A,
            "owner_creator_id": OWNER
        }))
        .await;
    assert_canonical_invalid_input(&unknown);

    let invalid_id = ctx
        .server
        .post("/v1/daemon/characters")
        .json(&json!({
            "display_name": "Ada",
            "world_id": "not-a-world"
        }))
        .await;
    assert_canonical_invalid_input(&invalid_id);
}

#[tokio::test]
async fn duplicate_display_name_is_stable_409() {
    let ctx = ctx().await;
    create_character(&ctx.server, "Ava", WORLD_A).await;
    let resp = ctx
        .server
        .post("/v1/daemon/characters")
        .json(&json!({ "display_name": "Ava", "world_id": WORLD_B }))
        .await;
    assert_eq!(resp.status_code(), 409, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "duplicate_character_display_name");
    assert_ne!(body["error"]["message"], "duplicate_character_display_name");
}

#[tokio::test]
async fn untrimmed_display_name_is_rejected() {
    let ctx = ctx().await;
    let resp = ctx
        .server
        .post("/v1/daemon/characters")
        .json(&json!({ "display_name": " Ava ", "world_id": WORLD_A }))
        .await;
    assert_eq!(resp.status_code(), 422, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input");
}

#[tokio::test]
async fn create_list_show_accept_real_local_creator_id() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let owner = "ctr_localabcdef123456";
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{owner}\"\n\n[active_workspace_slug_by_creator]\n\"{owner}\" = \"default\"\n"
        ),
    )
    .unwrap();
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    let pool = state.pool().unwrap().clone();
    nexus_local_db::ensure_creator_row(&pool, owner, "Local")
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(WORLD_A)
    .bind(owner)
    .bind(WORLD_A)
    .bind("world-a")
    .execute(&pool)
    .await
    .unwrap();
    let server = TestServer::new(api::create_router(state, DaemonApiConfig::keyless()));
    let created = server
        .post("/v1/daemon/characters")
        .json(&json!({ "display_name": "LocalAva", "world_id": WORLD_A }))
        .await;
    assert_eq!(created.status_code(), 201, "body={}", created.text());
    let body: Value = created.json();
    assert_eq!(body["character"]["owner_creator_id"], owner);
    let id = body["character"]["character_id"].as_str().unwrap();
    let show = server.get(&format!("/v1/daemon/characters/{id}")).await;
    assert_eq!(show.status_code(), 200, "body={}", show.text());
    let listed = server.get("/v1/daemon/characters").await;
    assert_eq!(listed.status_code(), 200, "body={}", listed.text());
    let listed_body: Value = listed.json();
    assert_eq!(listed_body["items"].as_array().unwrap().len(), 1);
    drop(tmp);
}

#[tokio::test]
async fn list_paginates_large_fixture_with_sql_bounds() {
    let ctx = ctx().await;
    let mut ids = Vec::new();
    for i in 0..25 {
        let body = create_character(&ctx.server, &format!("Char{i:02}"), WORLD_A).await;
        ids.push(
            body["character"]["character_id"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let url = cursor.as_ref().map_or_else(
            || "/v1/daemon/characters?limit=10".to_string(),
            |c| format!("/v1/daemon/characters?limit=10&cursor={c}"),
        );
        let page = ctx.server.get(&url).await;
        assert_eq!(page.status_code(), 200, "body={}", page.text());
        let p: Value = page.json();
        let items = p["items"].as_array().unwrap();
        assert!(items.len() <= 10);
        for item in items {
            seen.push(item["character_id"].as_str().unwrap().to_string());
        }
        if p["pagination"]["has_more"].as_bool().unwrap() {
            cursor = Some(p["pagination"]["next_cursor"].as_str().unwrap().to_string());
        } else {
            break;
        }
    }
    assert_eq!(seen.len(), 25);
    seen.sort();
    ids.sort();
    assert_eq!(seen, ids);
}

#[allow(clippy::future_not_send)]
async fn patch_character(server: &TestServer, id: &str, body: Value) -> axum_test::TestResponse {
    server
        .patch(&format!("/v1/daemon/characters/{id}"))
        .json(&body)
        .await
}

#[allow(clippy::future_not_send)]
async fn archive_character(
    server: &TestServer,
    id: &str,
    expected_revision: i64,
) -> axum_test::TestResponse {
    server
        .post(&format!("/v1/daemon/characters/{id}/archive"))
        .json(&json!({ "expected_revision": expected_revision }))
        .await
}

#[allow(clippy::future_not_send)]
async fn restore_character(
    server: &TestServer,
    id: &str,
    expected_revision: i64,
) -> axum_test::TestResponse {
    server
        .post(&format!("/v1/daemon/characters/{id}/restore"))
        .json(&json!({ "expected_revision": expected_revision }))
        .await
}

#[tokio::test]
async fn patch_character_cas_updates_revision_and_selected_fields() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    let resp = patch_character(
        &ctx.server,
        id,
        json!({
            "expected_revision": 0,
            "display_name": "Ada",
            "image_uri": "https://example.test/ava.png",
            "persona": { "tone": "dry" }
        }),
    )
    .await;
    assert_eq!(resp.status_code(), 200, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["character"]["display_name"], "Ada");
    assert_eq!(body["character"]["revision"], 1);
    assert_eq!(
        body["character"]["image_uri"],
        "https://example.test/ava.png"
    );
    assert_eq!(body["character"]["persona"]["tone"], "dry");
}

#[tokio::test]
async fn patch_stale_revision_is_character_revision_conflict() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    let ok = patch_character(
        &ctx.server,
        id,
        json!({ "expected_revision": 0, "display_name": "Ada" }),
    )
    .await;
    assert_eq!(ok.status_code(), 200);
    let stale = patch_character(
        &ctx.server,
        id,
        json!({ "expected_revision": 0, "display_name": "Bea" }),
    )
    .await;
    assert_eq!(stale.status_code(), 409, "body={}", stale.text());
    let body: Value = stale.json();
    assert_eq!(body["error"]["code"], "character_revision_conflict");
}

#[tokio::test]
async fn patch_omit_vs_clear_members() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    patch_character(
        &ctx.server,
        id,
        json!({
            "expected_revision": 0,
            "image_uri": "https://example.test/keep.png",
            "persona": { "role": "scout" }
        }),
    )
    .await;
    let omit = patch_character(
        &ctx.server,
        id,
        json!({ "expected_revision": 1, "display_name": "Ava2" }),
    )
    .await;
    assert_eq!(omit.status_code(), 200, "body={}", omit.text());
    let kept: Value = omit.json();
    assert_eq!(
        kept["character"]["image_uri"],
        "https://example.test/keep.png"
    );
    assert_eq!(kept["character"]["persona"]["role"], "scout");

    let cleared = patch_character(
        &ctx.server,
        id,
        json!({ "expected_revision": 2, "image_uri": null, "persona": null }),
    )
    .await;
    assert_eq!(cleared.status_code(), 200, "body={}", cleared.text());
    let body: Value = cleared.json();
    assert!(body["character"]["image_uri"].is_null());
    assert_eq!(body["character"]["persona"], json!({}));
}

#[tokio::test]
async fn patch_no_op_leaves_revision_unchanged() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    let before = patch_character(
        &ctx.server,
        id,
        json!({ "expected_revision": 0, "display_name": "Ava" }),
    )
    .await;
    assert_eq!(before.status_code(), 200);
    let body: Value = before.json();
    assert_eq!(body["character"]["revision"], 0);
}

#[tokio::test]
async fn archive_restore_round_trip_and_list_includes_archived() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();

    let archived = archive_character(&ctx.server, id, 0).await;
    assert_eq!(archived.status_code(), 200, "body={}", archived.text());
    let arch_body: Value = archived.json();
    assert_eq!(arch_body["character"]["status"], "archived");
    assert_eq!(arch_body["character"]["revision"], 1);

    let list = ctx.server.get("/v1/daemon/characters").await;
    let listed: Value = list.json();
    assert!(listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["character_id"] == id && c["status"] == "archived"));

    let write_denied = patch_character(
        &ctx.server,
        id,
        json!({ "expected_revision": 1, "display_name": "Nope" }),
    )
    .await;
    assert_eq!(
        write_denied.status_code(),
        409,
        "body={}",
        write_denied.text()
    );
    let denied_body: Value = write_denied.json();
    assert_eq!(denied_body["error"]["code"], "character_inactive");

    let restored = restore_character(&ctx.server, id, 1).await;
    assert_eq!(restored.status_code(), 200, "body={}", restored.text());
    let restored_body: Value = restored.json();
    assert_eq!(restored_body["character"]["status"], "active");
    assert_eq!(restored_body["character"]["character_id"], id);
    assert_eq!(restored_body["character"]["revision"], 2);
}

#[tokio::test]
async fn archive_same_state_cas_no_op_keeps_revision() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    let first = archive_character(&ctx.server, id, 0).await;
    assert_eq!(first.status_code(), 200);
    let again = archive_character(&ctx.server, id, 1).await;
    assert_eq!(again.status_code(), 200, "body={}", again.text());
    let body: Value = again.json();
    assert_eq!(body["character"]["status"], "archived");
    assert_eq!(body["character"]["revision"], 1);
}

#[tokio::test]
async fn restore_requires_active_owned_world_binding() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    archive_character(&ctx.server, id, 0).await;
    sqlx::query("UPDATE narrative_worlds SET status = 'paused' WHERE owner_creator_id = ?")
        .bind(OWNER)
        .execute(&ctx.pool)
        .await
        .unwrap();
    let resp = restore_character(&ctx.server, id, 1).await;
    assert_eq!(resp.status_code(), 409, "body={}", resp.text());
    let fail_body: Value = resp.json();
    assert_eq!(
        fail_body["error"]["code"],
        "character_restore_requires_active_binding"
    );
}

#[tokio::test]
async fn restore_name_collision_is_duplicate_character_display_name() {
    let ctx = ctx().await;
    let first = create_character(&ctx.server, "Ava", WORLD_A).await;
    let first_id = first["character"]["character_id"].as_str().unwrap();
    archive_character(&ctx.server, first_id, 0).await;
    create_character(&ctx.server, "Ava", WORLD_B).await;
    let resp = restore_character(&ctx.server, first_id, 1).await;
    assert_eq!(resp.status_code(), 409, "body={}", resp.text());
    let dup_body: Value = resp.json();
    assert_eq!(
        dup_body["error"]["code"],
        "duplicate_character_display_name"
    );
}

#[tokio::test]
async fn archive_refuses_character_busy_while_activity_outstanding() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    let guard = ctx
        .state
        .actor_sessions()
        .admit_character_activity(&ctx.pool, OWNER, id)
        .await
        .expect("admit activity");
    let busy = archive_character(&ctx.server, id, 0).await;
    assert_eq!(busy.status_code(), 409, "body={}", busy.text());
    let busy_body: Value = busy.json();
    assert_eq!(busy_body["error"]["code"], "character_busy");
    drop(guard);
    let ok = archive_character(&ctx.server, id, 0).await;
    assert_eq!(ok.status_code(), 200, "body={}", ok.text());
}

#[tokio::test]
async fn add_binding_holds_activity_fence_blocking_archive() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    let guard = ctx
        .state
        .actor_sessions()
        .admit_character_activity(&ctx.pool, OWNER, id)
        .await
        .expect("admit activity as add_binding does before local-db");
    let busy = archive_character(&ctx.server, id, 0).await;
    assert_eq!(busy.status_code(), 409, "body={}", busy.text());
    assert_eq!(busy.json::<Value>()["error"]["code"], "character_busy");
    drop(guard);
    let ok = ctx
        .server
        .post(&format!("/v1/daemon/characters/{id}/bindings"))
        .json(&json!({ "world_id": WORLD_B }))
        .await;
    assert_eq!(ok.status_code(), 201, "body={}", ok.text());
}

#[tokio::test]
async fn remove_binding_holds_activity_fence_blocking_archive() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    let binding_id = ctx
        .server
        .post(&format!("/v1/daemon/characters/{id}/bindings"))
        .json(&json!({ "world_id": WORLD_B }))
        .await
        .json::<Value>()["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();
    let guard = ctx
        .state
        .actor_sessions()
        .admit_character_activity(&ctx.pool, OWNER, id)
        .await
        .expect("admit activity as remove_binding does before local-db");
    let busy = archive_character(&ctx.server, id, 0).await;
    assert_eq!(busy.status_code(), 409, "body={}", busy.text());
    assert_eq!(busy.json::<Value>()["error"]["code"], "character_busy");
    drop(guard);
    let ok = ctx
        .server
        .delete(&format!("/v1/daemon/characters/{id}/bindings/{binding_id}"))
        .await;
    assert_eq!(ok.status_code(), 204, "body={}", ok.text());
}

#[tokio::test]
async fn patch_binding_holds_activity_fence_blocking_archive() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let binding_id = created["binding"]["binding_id"].as_str().unwrap();
    let guard = ctx
        .state
        .actor_sessions()
        .admit_character_activity(&ctx.pool, OWNER, chr)
        .await
        .expect("admit activity as patch_binding does before local-db");
    let busy = archive_character(&ctx.server, chr, 0).await;
    assert_eq!(busy.status_code(), 409, "body={}", busy.text());
    assert_eq!(busy.json::<Value>()["error"]["code"], "character_busy");
    drop(guard);
    let ok = patch_binding(
        &ctx.server,
        chr,
        binding_id,
        json!({ "expected_revision": 0, "world_sheet_entry_id": null }),
    )
    .await;
    assert_eq!(ok.status_code(), 200, "body={}", ok.text());
}

#[tokio::test]
async fn patch_rename_collision_is_duplicate_character_display_name() {
    let ctx = ctx().await;
    create_character(&ctx.server, "Ava", WORLD_A).await;
    let second = create_character(&ctx.server, "Bea", WORLD_B).await;
    let second_id = second["character"]["character_id"].as_str().unwrap();
    let resp = patch_character(
        &ctx.server,
        second_id,
        json!({ "expected_revision": 0, "display_name": "Ava" }),
    )
    .await;
    assert_eq!(resp.status_code(), 409, "body={}", resp.text());
    assert_eq!(
        resp.json::<Value>()["error"]["code"],
        "duplicate_character_display_name"
    );
    let row: (String, i64) =
        sqlx::query_as("SELECT display_name, revision FROM characters WHERE character_id = ?")
            .bind(second_id)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(row.0, "Bea");
    assert_eq!(row.1, 0);
}

#[allow(clippy::future_not_send)]
async fn patch_binding(
    server: &TestServer,
    chr: &str,
    binding_id: &str,
    body: Value,
) -> axum_test::TestResponse {
    server
        .patch(&format!(
            "/v1/daemon/characters/{chr}/bindings/{binding_id}"
        ))
        .json(&body)
        .await
}

#[tokio::test]
async fn binding_detail_link_relink_clear_and_cas_errors() {
    let ctx = ctx().await;
    sqlx::query(
        "INSERT INTO kb_key_blocks          (key_block_id, world_id, block_type, canonical_name, status, body_json, created_at)          VALUES ('kb_sheet_a', ?, 'character', 'sheet_a', 'confirmed', '{}', datetime('now'))",
    )
    .bind(WORLD_A)
    .execute(&ctx.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO kb_key_blocks          (key_block_id, world_id, block_type, canonical_name, status, body_json, created_at)          VALUES ('kb_sheet_b', ?, 'character', 'sheet_b', 'confirmed', '{}', datetime('now'))",
    )
    .bind(WORLD_A)
    .execute(&ctx.pool)
    .await
    .unwrap();
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let binding_id = created["binding"]["binding_id"].as_str().unwrap();

    let show = ctx
        .server
        .get(&format!(
            "/v1/daemon/characters/{chr}/bindings/{binding_id}"
        ))
        .await;
    assert_eq!(show.status_code(), 200, "body={}", show.text());
    assert_eq!(show.json::<Value>()["binding"]["revision"], 0);

    let linked = patch_binding(
        &ctx.server,
        chr,
        binding_id,
        json!({ "expected_revision": 0, "world_sheet_entry_id": "kb_sheet_a" }),
    )
    .await;
    assert_eq!(linked.status_code(), 200, "body={}", linked.text());
    let linked_body: Value = linked.json();
    assert_eq!(linked_body["binding"]["revision"], 1);
    assert_eq!(linked_body["binding"]["world_sheet_entry_id"], "kb_sheet_a");

    let relinked = patch_binding(
        &ctx.server,
        chr,
        binding_id,
        json!({ "expected_revision": 1, "world_sheet_entry_id": "kb_sheet_b" }),
    )
    .await;
    assert_eq!(relinked.status_code(), 200);
    assert_eq!(relinked.json::<Value>()["binding"]["revision"], 2);

    let cleared = patch_binding(
        &ctx.server,
        chr,
        binding_id,
        json!({ "expected_revision": 2, "world_sheet_entry_id": null }),
    )
    .await;
    assert_eq!(cleared.status_code(), 200);
    let cleared_body: Value = cleared.json();
    assert_eq!(cleared_body["binding"]["revision"], 3);
    assert!(cleared_body["binding"]["world_sheet_entry_id"].is_null());

    let noop = patch_binding(
        &ctx.server,
        chr,
        binding_id,
        json!({ "expected_revision": 3, "world_sheet_entry_id": null }),
    )
    .await;
    assert_eq!(noop.status_code(), 200, "body={}", noop.text());
    assert_eq!(noop.json::<Value>()["binding"]["revision"], 3);

    let stale = patch_binding(
        &ctx.server,
        chr,
        binding_id,
        json!({ "expected_revision": 2, "world_sheet_entry_id": null }),
    )
    .await;
    assert_eq!(stale.status_code(), 409, "body={}", stale.text());
    assert_eq!(
        stale.json::<Value>()["error"]["code"],
        "binding_revision_conflict"
    );

    let empty = patch_binding(
        &ctx.server,
        chr,
        binding_id,
        json!({ "expected_revision": 3 }),
    )
    .await;
    assert_eq!(empty.status_code(), 422);
    assert_eq!(empty.json::<Value>()["error"]["code"], "invalid_input");
}

#[tokio::test]
async fn binding_detail_retained_read_survives_archive() {
    let ctx = ctx().await;
    seed_sheet(&ctx.pool, "kb_retained", WORLD_A, "character", "confirmed").await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let binding_id = created["binding"]["binding_id"].as_str().unwrap();
    patch_binding(
        &ctx.server,
        chr,
        binding_id,
        json!({ "expected_revision": 0, "world_sheet_entry_id": "kb_retained" }),
    )
    .await;

    archive_character(&ctx.server, chr, 0).await;

    let show = ctx
        .server
        .get(&format!(
            "/v1/daemon/characters/{chr}/bindings/{binding_id}"
        ))
        .await;
    assert_eq!(show.status_code(), 200, "body={}", show.text());
    assert_eq!(
        show.json::<Value>()["binding"]["world_sheet_entry_id"],
        "kb_retained"
    );

    let denied = patch_binding(
        &ctx.server,
        chr,
        binding_id,
        json!({ "expected_revision": 1, "world_sheet_entry_id": null }),
    )
    .await;
    assert_eq!(denied.status_code(), 409, "body={}", denied.text());
    assert_eq!(
        denied.json::<Value>()["error"]["code"],
        "character_inactive"
    );
}

#[tokio::test]
async fn patch_binding_rejects_invalid_world_sheet() {
    let ctx = ctx().await;
    seed_sheet(&ctx.pool, "kb_wrong_type", WORLD_A, "location", "confirmed").await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let binding_id = created["binding"]["binding_id"].as_str().unwrap();

    let resp = patch_binding(
        &ctx.server,
        chr,
        binding_id,
        json!({ "expected_revision": 0, "world_sheet_entry_id": "kb_wrong_type" }),
    )
    .await;
    assert_eq!(resp.status_code(), 409, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_world_sheet");
    assert_ne!(body["error"]["message"], "invalid_world_sheet");
}

#[tokio::test]
async fn add_binding_rejects_paused_world_with_404_zero_mutation() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    sqlx::query("UPDATE narrative_worlds SET status = 'paused' WHERE world_id = ?")
        .bind(WORLD_B)
        .execute(&ctx.pool)
        .await
        .unwrap();
    let before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ?")
            .bind(chr)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    let resp = ctx
        .server
        .post(&format!("/v1/daemon/characters/{chr}/bindings"))
        .json(&json!({ "world_id": WORLD_B }))
        .await;
    assert_eq!(resp.status_code(), 404, "body={}", resp.text());
    let after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ?")
            .bind(chr)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(after, before);
}

#[tokio::test]
async fn create_character_rejects_paused_world_with_404_zero_mutation() {
    let ctx = ctx().await;
    sqlx::query("UPDATE narrative_worlds SET status = 'paused' WHERE world_id = ?")
        .bind(WORLD_A)
        .execute(&ctx.pool)
        .await
        .unwrap();
    let chars_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM characters")
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    let resp = ctx
        .server
        .post("/v1/daemon/characters")
        .json(&json!({ "display_name": "Paused", "world_id": WORLD_A }))
        .await;
    assert_eq!(resp.status_code(), 404, "body={}", resp.text());
    let chars_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM characters")
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(chars_after, chars_before);
}

#[tokio::test]
async fn patch_binding_rejects_overlength_world_sheet_bytes_at_api() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let binding_id = created["binding"]["binding_id"].as_str().unwrap();
    // 128 Unicode scalars but 131 bytes — passes wire maxLength, fails storage bytes check.
    let overlength = format!("kb_{}{}", "a".repeat(124), "\u{1f3ad}");

    let resp = patch_binding(
        &ctx.server,
        chr,
        binding_id,
        json!({ "expected_revision": 0, "world_sheet_entry_id": overlength }),
    )
    .await;
    assert_eq!(resp.status_code(), 409, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_world_sheet");
    assert_ne!(body["error"]["message"], "invalid_world_sheet");
}

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use nexus_agent_host::{HostFacade, HostSession, HostSessionId, SessionState};

struct CountingHost {
    shutdowns: AtomicUsize,
    execs: AtomicUsize,
    sessions: std::sync::Mutex<std::collections::HashMap<HostSessionId, HostSession>>,
}

impl CountingHost {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            shutdowns: AtomicUsize::new(0),
            execs: AtomicUsize::new(0),
            sessions: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }
}

#[async_trait]
impl HostFacade for CountingHost {
    async fn start(
        &self,
        _config: nexus_agent_host::capability::model::HostStartConfig,
    ) -> nexus_agent_host::HostResult<()> {
        Ok(())
    }

    async fn create_session(
        &self,
        request: nexus_agent_host::capability::CreateSessionRequest,
    ) -> nexus_agent_host::HostResult<HostSession> {
        let session = HostSession {
            id: HostSessionId::new(),
            provider_id: request.provider_id,
            state: SessionState::Ready,
            created_at: chrono::Utc::now(),
            active_op_id: None,
            negotiated_capabilities:
                nexus_agent_host::capability::model::CapabilityDescriptor::native_cli_limited(),
            owner: request.owner,
            process_identity: None,
        };
        self.sessions
            .lock()
            .expect("sessions")
            .insert(session.id.clone(), session.clone());
        Ok(session)
    }

    async fn exec(
        &self,
        _session_id: HostSessionId,
        _op: nexus_agent_host::capability::model::HostOperation,
    ) -> nexus_agent_host::HostResult<nexus_agent_host::capability::model::HostEventStream> {
        self.execs.fetch_add(1, Ordering::SeqCst);
        Ok(Box::pin(futures_util::stream::empty()))
    }

    async fn cancel(
        &self,
        _op_id: nexus_agent_host::HostOperationId,
    ) -> nexus_agent_host::HostResult<()> {
        Ok(())
    }

    async fn health(
        &self,
    ) -> nexus_agent_host::HostResult<nexus_agent_host::capability::model::HostHealth> {
        Ok(nexus_agent_host::capability::model::HostHealth {
            running: true,
            active_sessions: self.sessions.lock().expect("sessions").len(),
            active_operations: 0,
        })
    }

    async fn shutdown(&self) -> nexus_agent_host::HostResult<()> {
        Ok(())
    }

    async fn shutdown_session(
        &self,
        session_id: HostSessionId,
    ) -> nexus_agent_host::HostResult<()> {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
        self.sessions.lock().expect("sessions").remove(&session_id);
        Ok(())
    }

    async fn list_sessions(&self) -> nexus_agent_host::HostResult<Vec<HostSession>> {
        Ok(self
            .sessions
            .lock()
            .expect("sessions")
            .values()
            .cloned()
            .collect())
    }

    async fn provider_catalog(
        &self,
    ) -> nexus_agent_host::HostResult<nexus_agent_host::ProviderCatalog> {
        Ok(nexus_agent_host::ProviderCatalog::new())
    }

    fn subscribe_events(
        &self,
        _session_id: HostSessionId,
    ) -> tokio::sync::broadcast::Receiver<nexus_agent_host::capability::model::HostEvent> {
        tokio::sync::broadcast::channel(1).1
    }
}

async fn ctx_with_host() -> (Ctx, Arc<CountingHost>) {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{OWNER}\"\n\n[active_workspace_slug_by_creator]\n\"{OWNER}\" = \"default\"\n"
        ),
    )
    .unwrap();
    let mut state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    let host = CountingHost::new();
    state.set_agent_host(host.clone());
    let pool = state.pool().unwrap().clone();
    seed_actor_fixture(&pool).await;
    let server = TestServer::new(api::create_router(
        state.clone(),
        DaemonApiConfig::keyless(),
    ));
    let ctx = Ctx {
        _tmp: tmp,
        server,
        pool,
        nexus_home,
        state,
    };
    (ctx, host)
}

#[allow(clippy::future_not_send)]
async fn create_character_session(
    server: &TestServer,
    character_id: &str,
    binding_id: &str,
) -> String {
    let resp = server
        .post("/v1/daemon/agent-host/sessions")
        .json(&json!({
            "provider_id": "mock-provider",
            "actor_ref": {"actor_kind": "character", "character_id": character_id},
            "viewpoint": {"world_id": WORLD_A, "binding_id": binding_id}
        }))
        .await;
    assert_eq!(resp.status_code(), 200, "create session: {}", resp.text());
    let body: Value = resp.json();
    body["session_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn patch_empty_body_is_invalid_input() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    let resp = patch_character(&ctx.server, id, json!({ "expected_revision": 0 })).await;
    assert_canonical_invalid_input(&resp);
}

#[tokio::test]
async fn restore_same_state_cas_no_op_keeps_session_executable_without_shutdown() {
    let (ctx, host) = ctx_with_host().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    let binding_id = created["binding"]["binding_id"].as_str().unwrap();
    let session_id = create_character_session(&ctx.server, id, binding_id).await;

    let before_shutdowns = host.shutdowns.load(Ordering::SeqCst);
    let no_op = restore_character(&ctx.server, id, 0).await;
    assert_eq!(no_op.status_code(), 200, "body={}", no_op.text());
    assert_eq!(
        host.shutdowns.load(Ordering::SeqCst),
        before_shutdowns,
        "same-state restore no-op must not retire sessions"
    );

    let exec = ctx
        .server
        .post(&format!(
            "/v1/daemon/agent-host/sessions/{session_id}/operations"
        ))
        .json(&json!({"kind": "prompt", "content": "ping"}))
        .await;
    assert_eq!(
        exec.status_code(),
        200,
        "execute after no-op: {}",
        exec.text()
    );
    assert_eq!(host.execs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn pre_archive_session_execute_is_stale_after_material_archive() {
    let (ctx, host) = ctx_with_host().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let id = created["character"]["character_id"].as_str().unwrap();
    let binding_id = created["binding"]["binding_id"].as_str().unwrap();
    let session_id = create_character_session(&ctx.server, id, binding_id).await;

    let archived = archive_character(&ctx.server, id, 0).await;
    assert_eq!(archived.status_code(), 200, "body={}", archived.text());
    assert_eq!(host.shutdowns.load(Ordering::SeqCst), 1);

    let exec = ctx
        .server
        .post(&format!(
            "/v1/daemon/agent-host/sessions/{session_id}/operations"
        ))
        .json(&json!({"kind": "prompt", "content": "nope"}))
        .await;
    assert_eq!(exec.status_code(), 409, "body={}", exec.text());
    let body: Value = exec.json();
    assert_eq!(body["error"]["code"], "actor_session_stale");
    assert_eq!(host.execs.load(Ordering::SeqCst), 0);
}
