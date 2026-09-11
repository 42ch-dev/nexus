//! P1 Task 3 — Actor `KnowledgeView` Daemon routes.
//!
//! Auth denial, stored-owner admission, union composition, cursor pagination,
//! and binding-removal `KnowledgeEntry` dependency 409.

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
const MISSING_CHR: &str = "chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct Ctx {
    _tmp: TestTempRoot,
    server: TestServer,
    pool: sqlx::SqlitePool,
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

// axum-test `AutoFuture` is not `Send` by design; this helper is only awaited on the single-threaded `#[tokio::test]` runtime.
#[allow(clippy::future_not_send)]
async fn add_entry(server: &TestServer, body: Value) -> Value {
    let resp = server
        .post("/v1/daemon/actor-knowledge/entries")
        .json(&body)
        .await;
    assert_eq!(
        resp.status_code(),
        201,
        "add_entry {}: {}",
        resp.status_code(),
        resp.text()
    );
    resp.json()
}

fn names(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["canonical_name"].as_str().unwrap().to_string())
        .collect()
}

async fn count_bindings(pool: &sqlx::SqlitePool, character_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ? AND status = 'active'",
    )
    .bind(character_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn actor_knowledge_view_requires_api_key() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{OWNER}\"\n\n[active_workspace_slug_by_creator]\n\"{OWNER}\" = \"default\"\n"
        ),
    )
    .unwrap();
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let server = TestServer::new(api::create_router(
        state,
        DaemonApiConfig::keyed("test-secret"),
    ));
    let resp = server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "creator", "creator_id": OWNER },
            "world_id": WORLD_A
        }))
        .await;
    assert_eq!(resp.status_code(), 401, "body={}", resp.text());
    drop(tmp);
}

#[tokio::test]
async fn invalid_and_unbound_owners_are_404() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let binding = created["binding"]["binding_id"].as_str().unwrap();

    let missing_world = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "creator", "creator_id": OWNER },
            "world_id": "wld_missing"
        }))
        .await;
    assert_eq!(missing_world.status_code(), 404, "{}", missing_world.text());

    let missing_chr = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "character", "character_id": MISSING_CHR },
            "world_id": WORLD_A,
            "binding_id": binding
        }))
        .await;
    assert_eq!(missing_chr.status_code(), 404, "{}", missing_chr.text());

    let add_foreign = ctx
        .server
        .post("/v1/daemon/actor-knowledge/entries")
        .json(&json!({
            "owner_kind": "character",
            "character_id": MISSING_CHR,
            "block_type": "item",
            "canonical_name": "Nope"
        }))
        .await;
    assert_eq!(add_foreign.status_code(), 404, "{}", add_foreign.text());

    let _ = chr;
}

#[tokio::test]
async fn character_view_without_binding_id_is_422_not_creator_page() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    add_entry(
        &ctx.server,
        json!({
            "owner_kind": "world",
            "world_id": WORLD_A,
            "block_type": "item",
            "canonical_name": "WorldSecret",
            "creator_only": true
        }),
    )
    .await;

    let resp = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "character", "character_id": chr },
            "world_id": WORLD_A
        }))
        .await;
    assert_eq!(resp.status_code(), 422, "{}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input");
    assert!(body.get("items").is_none());
}

#[tokio::test]
async fn creator_only_hidden_from_character_visible_to_creator() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let binding = created["binding"]["binding_id"].as_str().unwrap();

    add_entry(
        &ctx.server,
        json!({
            "owner_kind": "world",
            "world_id": WORLD_A,
            "block_type": "item",
            "canonical_name": "PublicWorld",
            "creator_only": false
        }),
    )
    .await;
    add_entry(
        &ctx.server,
        json!({
            "owner_kind": "world",
            "world_id": WORLD_A,
            "block_type": "item",
            "canonical_name": "SecretWorld",
            "creator_only": true
        }),
    )
    .await;
    add_entry(
        &ctx.server,
        json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "CharNote"
        }),
    )
    .await;

    let character_page = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "character", "character_id": chr },
            "world_id": WORLD_A,
            "binding_id": binding
        }))
        .await;
    assert_eq!(
        character_page.status_code(),
        200,
        "{}",
        character_page.text()
    );
    let character_body: Value = character_page.json();
    let character_names = names(&character_body);
    assert!(character_names.contains(&"PublicWorld".into()));
    assert!(character_names.contains(&"CharNote".into()));
    assert!(!character_names.contains(&"SecretWorld".into()));

    let creator_page = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "creator", "creator_id": OWNER },
            "world_id": WORLD_A
        }))
        .await;
    assert_eq!(creator_page.status_code(), 200, "{}", creator_page.text());
    let creator_names = names(&creator_page.json());
    assert!(creator_names.contains(&"PublicWorld".into()));
    assert!(creator_names.contains(&"SecretWorld".into()));
    assert!(creator_names.contains(&"CharNote".into()));
}

#[tokio::test]
async fn binding_local_entries_are_isolated() {
    let ctx = ctx().await;
    let a = create_character(&ctx.server, "Ava", WORLD_A).await;
    let b = create_character(&ctx.server, "Ben", WORLD_A).await;
    let a_chr = a["character"]["character_id"].as_str().unwrap();
    let a_bind = a["binding"]["binding_id"].as_str().unwrap();
    let b_chr = b["character"]["character_id"].as_str().unwrap();
    let b_bind = b["binding"]["binding_id"].as_str().unwrap();

    add_entry(
        &ctx.server,
        json!({
            "owner_kind": "actor_world_binding",
            "character_id": a_chr,
            "binding_id": a_bind,
            "world_id": WORLD_A,
            "block_type": "item",
            "canonical_name": "AvaLocal"
        }),
    )
    .await;

    let a_page = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "character", "character_id": a_chr },
            "world_id": WORLD_A,
            "binding_id": a_bind
        }))
        .await;
    assert_eq!(a_page.status_code(), 200, "{}", a_page.text());
    assert!(names(&a_page.json()).contains(&"AvaLocal".into()));

    let b_page = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "character", "character_id": b_chr },
            "world_id": WORLD_A,
            "binding_id": b_bind
        }))
        .await;
    assert_eq!(b_page.status_code(), 200, "{}", b_page.text());
    assert!(!names(&b_page.json()).contains(&"AvaLocal".into()));
}

#[tokio::test]
async fn view_orders_and_paginates_with_opaque_k2_cursor() {
    let ctx = ctx().await;
    for name in ["Alpha", "Bravo", "Charlie"] {
        add_entry(
            &ctx.server,
            json!({
                "owner_kind": "world",
                "world_id": WORLD_A,
                "block_type": "item",
                "canonical_name": name
            }),
        )
        .await;
    }

    let page1 = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "creator", "creator_id": OWNER },
            "world_id": WORLD_A,
            "limit": 2
        }))
        .await;
    assert_eq!(page1.status_code(), 200, "{}", page1.text());
    let p1: Value = page1.json();
    assert_eq!(p1["items"].as_array().unwrap().len(), 2);
    assert_eq!(p1["pagination"]["has_more"], true);
    let cursor = p1["pagination"]["next_cursor"].as_str().unwrap();
    assert!(cursor.starts_with("k2:"));
    assert!(cursor.contains('\u{1f}'));

    let page2 = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "creator", "creator_id": OWNER },
            "world_id": WORLD_A,
            "limit": 2,
            "cursor": cursor
        }))
        .await;
    assert_eq!(page2.status_code(), 200, "{}", page2.text());
    let p2: Value = page2.json();
    assert_eq!(p2["items"].as_array().unwrap().len(), 1);
    assert_ne!(p1["items"][0]["entry_id"], p2["items"][0]["entry_id"]);
    assert_ne!(p1["items"][1]["entry_id"], p2["items"][0]["entry_id"]);

    for cursor in [
        "v1:12".to_string(),
        "k2:2026-01-01T00:00:00Z\u{1f}kb_a\u{1f}extra".to_string(),
    ] {
        let bad = ctx
            .server
            .post("/v1/daemon/actor-knowledge/view")
            .json(&json!({
                "actor_ref": { "actor_kind": "creator", "creator_id": OWNER },
                "world_id": WORLD_A,
                "cursor": cursor
            }))
            .await;
        assert_eq!(
            bad.status_code(),
            422,
            "cursor={cursor} body={}",
            bad.text()
        );
        let body: Value = bad.json();
        assert_eq!(body["error"]["code"], "invalid_input");
    }
}

#[tokio::test]
async fn non_last_binding_with_owned_knowledge_is_stable_409() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();
    let bind_a = created["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    let second = ctx
        .server
        .post(&format!("/v1/daemon/characters/{chr}/bindings"))
        .json(&json!({ "world_id": WORLD_B }))
        .await;
    assert_eq!(second.status_code(), 201, "{}", second.text());

    add_entry(
        &ctx.server,
        json!({
            "owner_kind": "actor_world_binding",
            "character_id": chr,
            "binding_id": bind_a,
            "world_id": WORLD_A,
            "block_type": "item",
            "canonical_name": "LocalA"
        }),
    )
    .await;

    let before = count_bindings(&ctx.pool, &chr).await;
    assert_eq!(before, 2);
    let ke_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE actor_world_binding_id = ?")
            .bind(&bind_a)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(ke_before, 1);

    let remove = ctx
        .server
        .delete(&format!("/v1/daemon/characters/{chr}/bindings/{bind_a}"))
        .await;
    assert_eq!(remove.status_code(), 409, "{}", remove.text());
    let body: Value = remove.json();
    assert_eq!(body["error"]["code"], "binding_has_owned_knowledge");
    assert_eq!(count_bindings(&ctx.pool, &chr).await, before);
    let ke_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE actor_world_binding_id = ?")
            .bind(&bind_a)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(ke_after, ke_before);
}

#[tokio::test]
async fn last_binding_409_wins_even_with_owned_knowledge() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let bind = created["binding"]["binding_id"].as_str().unwrap();
    add_entry(
        &ctx.server,
        json!({
            "owner_kind": "actor_world_binding",
            "character_id": chr,
            "binding_id": bind,
            "world_id": WORLD_A,
            "block_type": "item",
            "canonical_name": "OnlyBinding"
        }),
    )
    .await;
    let remove = ctx
        .server
        .delete(&format!("/v1/daemon/characters/{chr}/bindings/{bind}"))
        .await;
    assert_eq!(remove.status_code(), 409, "{}", remove.text());
    let body: Value = remove.json();
    assert_eq!(body["error"]["code"], "last_active_actor_world_binding");
    assert_eq!(count_bindings(&ctx.pool, chr).await, 1);
}

#[tokio::test]
async fn character_knowledge_list_is_character_owned_only() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    add_entry(
        &ctx.server,
        json!({
            "owner_kind": "world",
            "world_id": WORLD_A,
            "block_type": "item",
            "canonical_name": "WorldRow"
        }),
    )
    .await;
    add_entry(
        &ctx.server,
        json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "CharRow"
        }),
    )
    .await;
    let listed = ctx
        .server
        .get(&format!("/v1/daemon/characters/{chr}/knowledge"))
        .await;
    assert_eq!(listed.status_code(), 200, "{}", listed.text());
    let names = names(&listed.json());
    assert_eq!(names, vec!["CharRow".to_string()]);
}

#[tokio::test]
async fn character_view_and_binding_add_require_owned_target_world() {
    const FOREIGN_WORLD: &str = "wld_foreign";
    let ctx = ctx().await;
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(FOREIGN_WORLD)
    .bind(OTHER)
    .bind(FOREIGN_WORLD)
    .bind(FOREIGN_WORLD)
    .execute(&ctx.pool)
    .await
    .unwrap();

    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();
    let foreign_binding = "awb_cccccccccccccccccccccccccccccccc";
    sqlx::query(
        "INSERT INTO actor_world_bindings \
         (binding_id, character_id, world_id, status, created_at, updated_at) \
         VALUES (?, ?, ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind(foreign_binding)
    .bind(&chr)
    .bind(FOREIGN_WORLD)
    .execute(&ctx.pool)
    .await
    .unwrap();

    let view = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "character", "character_id": chr },
            "world_id": FOREIGN_WORLD,
            "binding_id": foreign_binding
        }))
        .await;
    assert_eq!(view.status_code(), 404, "{}", view.text());
    let view_body: Value = view.json();
    assert!(view_body.get("items").is_none());

    let add = ctx
        .server
        .post("/v1/daemon/actor-knowledge/entries")
        .json(&json!({
            "owner_kind": "actor_world_binding",
            "character_id": chr,
            "binding_id": foreign_binding,
            "world_id": FOREIGN_WORLD,
            "block_type": "item",
            "canonical_name": "ShouldNotInsert"
        }))
        .await;
    assert_eq!(add.status_code(), 404, "{}", add.text());
    let ke: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE actor_world_binding_id = ?")
            .bind(foreign_binding)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(ke, 0);
}

async fn insert_legacy_default_world_row(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    entry_id: &str,
    name: &str,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, owner_kind, world_id, block_type, canonical_name, status) \
         VALUES (?, 'world', ?, 'item', ?, 'confirmed')",
    )
    .bind(entry_id)
    .bind(world_id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_owned_row(
    pool: &sqlx::SqlitePool,
    entry_id: &str,
    owner_kind: &str,
    owner_column: &str,
    owner_id: &str,
    name: &str,
    created_at: &str,
) {
    let sql = format!(
        "INSERT INTO kb_key_blocks \
         (key_block_id, owner_kind, {owner_column}, block_type, canonical_name, status, created_at) \
         VALUES (?, ?, ?, 'item', ?, 'confirmed', ?)"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(entry_id)
        .bind(owner_kind)
        .bind(owner_id)
        .bind(name)
        .bind(created_at)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn view_projects_legacy_sqlite_datetime_without_rewriting_bytes() {
    let ctx = ctx().await;
    insert_legacy_default_world_row(
        &ctx.pool,
        WORLD_A,
        "kb_legacydefault000000000000000001",
        "LegacyDefault",
    )
    .await;
    let stored: String =
        sqlx::query_scalar("SELECT created_at FROM kb_key_blocks WHERE key_block_id = ?")
            .bind("kb_legacydefault000000000000000001")
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert!(
        stored.contains(' ') && !stored.contains('T'),
        "legacy bytes must stay SQLite datetime: {stored}"
    );

    let page = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "creator", "creator_id": OWNER },
            "world_id": WORLD_A
        }))
        .await;
    assert_eq!(page.status_code(), 200, "{}", page.text());
    let body: Value = page.json();
    let item = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["canonical_name"] == "LegacyDefault")
        .expect("legacy row");
    let wire = item["created_at"].as_str().expect("rfc3339 string");
    assert!(
        wire.contains('T'),
        "wire created_at must be canonical RFC3339: {wire}"
    );
    let stored_after: String =
        sqlx::query_scalar("SELECT created_at FROM kb_key_blocks WHERE key_block_id = ?")
            .bind("kb_legacydefault000000000000000001")
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(stored_after, stored);
}

#[tokio::test]
async fn view_paginates_large_multi_owner_union_without_skip_or_duplicate() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let bind = created["binding"]["binding_id"].as_str().unwrap();

    let mut expected = Vec::new();
    for i in 0..40 {
        let id = format!("kb_worldpad{i:032}");
        let name = format!("WorldPad{i:03}");
        insert_owned_row(
            &ctx.pool,
            &id,
            "world",
            "world_id",
            WORLD_A,
            &name,
            "2026-01-01 00:00:00",
        )
        .await;
        expected.push(id);
    }
    for i in 0..40 {
        let id = format!("kb_charpad{i:033}");
        let name = format!("CharPad{i:03}");
        insert_owned_row(
            &ctx.pool,
            &id,
            "character",
            "character_id",
            chr,
            &name,
            &format!("2026-01-01T00:00:{i:02}Z"),
        )
        .await;
        expected.push(id);
    }
    for i in 0..40 {
        let id = format!("kb_bindpad{i:033}");
        let name = format!("BindPad{i:03}");
        insert_owned_row(
            &ctx.pool,
            &id,
            "actor_world_binding",
            "actor_world_binding_id",
            bind,
            &name,
            &format!("2026-01-02T00:00:{i:02}Z"),
        )
        .await;
        expected.push(id);
    }

    let mut seen = Vec::new();
    let mut cursor = None;
    for _ in 0..40 {
        let mut body = json!({
            "actor_ref": { "actor_kind": "creator", "creator_id": OWNER },
            "world_id": WORLD_A,
            "limit": 10
        });
        if let Some(c) = &cursor {
            body["cursor"] = json!(c);
        }
        let page = ctx
            .server
            .post("/v1/daemon/actor-knowledge/view")
            .json(&body)
            .await;
        assert_eq!(page.status_code(), 200, "{}", page.text());
        let payload: Value = page.json();
        for item in payload["items"].as_array().unwrap() {
            seen.push(item["entry_id"].as_str().unwrap().to_string());
        }
        if payload["pagination"]["has_more"] == false {
            assert!(payload["pagination"]["next_cursor"].is_null());
            break;
        }
        cursor = Some(
            payload["pagination"]["next_cursor"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }

    expected.sort();
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(seen.len(), unique.len(), "pagination must not duplicate");
    unique.sort();
    expected.sort();
    assert_eq!(unique, expected, "pagination must not skip union members");
}

#[tokio::test]
async fn view_paginates_same_millisecond_reverse_ids_without_skip_or_duplicate() {
    let ctx = ctx().await;
    insert_owned_row(
        &ctx.pool,
        "kb_m",
        "world",
        "world_id",
        WORLD_A,
        "MsLateId",
        "2026-01-01T10:00:00.123200Z",
    )
    .await;
    insert_owned_row(
        &ctx.pool,
        "kb_a",
        "world",
        "world_id",
        WORLD_A,
        "MsEarlyId",
        "2026-01-01T10:00:00.123300Z",
    )
    .await;

    let mut seen = Vec::new();
    let mut cursor = None;
    for _ in 0..8 {
        let mut body = json!({
            "actor_ref": { "actor_kind": "creator", "creator_id": OWNER },
            "world_id": WORLD_A,
            "limit": 1
        });
        if let Some(c) = &cursor {
            body["cursor"] = json!(c);
        }
        let page = ctx
            .server
            .post("/v1/daemon/actor-knowledge/view")
            .json(&body)
            .await;
        assert_eq!(page.status_code(), 200, "{}", page.text());
        let payload: Value = page.json();
        for item in payload["items"].as_array().unwrap() {
            seen.push(item["entry_id"].as_str().unwrap().to_string());
        }
        if payload["pagination"]["has_more"] == false {
            assert!(payload["pagination"]["next_cursor"].is_null());
            break;
        }
        cursor = Some(
            payload["pagination"]["next_cursor"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    assert_eq!(seen, vec!["kb_a".to_string(), "kb_m".to_string()]);
}

/// PR #240 finding 1: public actor-knowledge operations must reject
/// owned-but-inactive Worlds and Characters (Host admission parity), not just
/// foreign/missing ones.
// Fail-closed matrix across view+add for inactive/foreign actors; one contract.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn inactive_world_and_character_fail_closed_on_view_and_add() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();
    let binding = created["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    add_entry(
        &ctx.server,
        json!({
            "owner_kind": "world",
            "world_id": WORLD_A,
            "block_type": "item",
            "canonical_name": "Keystone",
            "creator_only": false
        }),
    )
    .await;

    // Active scope still works as a control.
    let ok = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "character", "character_id": chr },
            "world_id": WORLD_A,
            "binding_id": binding
        }))
        .await;
    assert_eq!(ok.status_code(), 200, "control: {}", ok.text());

    // Archive the Character: the KnowledgeView is a retained read and still
    // 200 (owned Character + stored binding tuple); the character-owned add
    // fails closed with character_inactive.
    sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
        .bind(&chr)
        .execute(&ctx.pool)
        .await
        .unwrap();
    let view_archived_chr = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "character", "character_id": chr },
            "world_id": WORLD_A,
            "binding_id": binding
        }))
        .await;
    assert_eq!(
        view_archived_chr.status_code(),
        200,
        "archived Character view is retained: {}",
        view_archived_chr.text()
    );
    let body: Value = view_archived_chr.json();
    assert!(body.get("items").is_some());

    let add_archived_chr = ctx
        .server
        .post("/v1/daemon/actor-knowledge/entries")
        .json(&json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "Nope"
        }))
        .await;
    assert_eq!(
        add_archived_chr.status_code(),
        409,
        "{}",
        add_archived_chr.text()
    );
    let body: Value = add_archived_chr.json();
    assert_eq!(body["error"]["code"], "character_inactive");

    // Archive the World: the Creator view is a retained read (an owned
    // archived World keeps its retained history visible); the World-owned add
    // still fails closed with world_inactive.
    sqlx::query("UPDATE narrative_worlds SET status = 'archived' WHERE world_id = ?")
        .bind(WORLD_A)
        .execute(&ctx.pool)
        .await
        .unwrap();
    let view_archived_world = ctx
        .server
        .post("/v1/daemon/actor-knowledge/view")
        .json(&json!({
            "actor_ref": { "actor_kind": "creator", "creator_id": OWNER },
            "world_id": WORLD_A
        }))
        .await;
    assert_eq!(
        view_archived_world.status_code(),
        200,
        "archived World Creator view is retained: {}",
        view_archived_world.text()
    );
    let body: Value = view_archived_world.json();
    assert!(body.get("items").is_some());

    let add_archived_world = ctx
        .server
        .post("/v1/daemon/actor-knowledge/entries")
        .json(&json!({
            "owner_kind": "world",
            "world_id": WORLD_A,
            "block_type": "item",
            "canonical_name": "Nope"
        }))
        .await;
    assert_eq!(
        add_archived_world.status_code(),
        409,
        "{}",
        add_archived_world.text()
    );
    let body: Value = add_archived_world.json();
    assert_eq!(body["error"]["code"], "world_inactive");
}
/// Fix round 1 (L2): character-owned KB add admits and holds the per-Character
/// activity fence so a concurrent lifecycle transition refuses `character_busy`
/// (durable §11.3.1/§11.3.2).
#[tokio::test]
async fn kb_character_add_holds_activity_fence() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "FenceKb", WORLD_A).await;
    let chr = created["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();

    let guard = ctx
        .state
        .actor_sessions()
        .admit_character_activity(&ctx.pool, OWNER, &chr)
        .await
        .expect("admit activity as add_entry does before insert");
    let busy = ctx
        .state
        .actor_sessions()
        .try_character_transition(&ctx.pool, OWNER, &chr)
        .await
        .expect_err("archive blocked while KB-add activity fence held");
    assert_eq!(busy.error_code(), "character_busy");
    assert_eq!(busy.status_code(), axum::http::StatusCode::CONFLICT);

    drop(guard);
    add_entry(
        &ctx.server,
        json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "HeldFence"
        }),
    )
    .await;
}

// --- P2 Task 3: knowledge detail / summary CAS / delete routes ---

// axum-test `AutoFuture` is not `Send` by design; helpers only on single-threaded runtime.
#[allow(clippy::future_not_send)]
async fn detail(
    server: &TestServer,
    character_id: &str,
    entry_id: &str,
) -> axum_test::TestResponse {
    server
        .get(&format!(
            "/v1/daemon/characters/{character_id}/knowledge/{entry_id}"
        ))
        .await
}

#[allow(clippy::future_not_send)]
async fn patch_detail(
    server: &TestServer,
    character_id: &str,
    entry_id: &str,
    body: Value,
) -> axum_test::TestResponse {
    server
        .patch(&format!(
            "/v1/daemon/characters/{character_id}/knowledge/{entry_id}"
        ))
        .json(&body)
        .await
}

#[allow(clippy::future_not_send)]
async fn delete_detail_query(
    server: &TestServer,
    character_id: &str,
    entry_id: &str,
    query: &str,
) -> axum_test::TestResponse {
    let path = if query.is_empty() {
        format!("/v1/daemon/characters/{character_id}/knowledge/{entry_id}")
    } else {
        format!("/v1/daemon/characters/{character_id}/knowledge/{entry_id}?{query}")
    };
    server.delete(&path).await
}

#[allow(clippy::future_not_send)]
async fn delete_detail(
    server: &TestServer,
    character_id: &str,
    entry_id: &str,
    expected_revision: u64,
) -> axum_test::TestResponse {
    server
        .delete(&format!(
            "/v1/daemon/characters/{character_id}/knowledge/{entry_id}?expected_revision={expected_revision}"
        ))
        .await
}

#[tokio::test]
async fn knowledge_detail_summary_lifecycle_and_preserves_body_keys() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();

    let added = add_entry(
        &ctx.server,
        json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "Fact",
            "summary": "initial summary"
        }),
    )
    .await;
    let entry_id = added["item"]["entry_id"].as_str().unwrap();
    assert_eq!(added["item"]["revision"], 0);

    sqlx::query("UPDATE kb_key_blocks SET body_json = ? WHERE key_block_id = ?")
        .bind(r#"{"summary":"initial summary","custom_flag":true}"#)
        .bind(entry_id)
        .execute(&ctx.pool)
        .await
        .unwrap();

    let got = detail(&ctx.server, chr, entry_id).await;
    assert_eq!(got.status_code(), 200, "{}", got.text());
    let body: Value = got.json();
    assert_eq!(body["summary"], "initial summary");
    assert_eq!(body["item"]["canonical_name"], "Fact");
    assert_eq!(body["item"]["revision"], 0);

    let patched = patch_detail(
        &ctx.server,
        chr,
        entry_id,
        json!({ "expected_revision": 0, "summary": "edited summary" }),
    )
    .await;
    assert_eq!(patched.status_code(), 200, "{}", patched.text());
    let body: Value = patched.json();
    assert_eq!(body["summary"], "edited summary");
    assert_eq!(body["item"]["revision"], 1);

    let row: (String,) =
        sqlx::query_as("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(entry_id)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    let stored: Value = serde_json::from_str(&row.0).unwrap();
    assert_eq!(stored["custom_flag"], true);

    let rename_only = patch_detail(
        &ctx.server,
        chr,
        entry_id,
        json!({ "expected_revision": 1, "canonical_name": "FactRenamed" }),
    )
    .await;
    assert_eq!(rename_only.status_code(), 200, "{}", rename_only.text());
    let body: Value = rename_only.json();
    assert_eq!(body["item"]["canonical_name"], "FactRenamed");
    assert_eq!(body["summary"], "edited summary");
    assert_eq!(body["item"]["revision"], 2);

    let cleared = patch_detail(
        &ctx.server,
        chr,
        entry_id,
        json!({ "expected_revision": 2, "summary": null }),
    )
    .await;
    assert_eq!(cleared.status_code(), 200, "{}", cleared.text());
    let body: Value = cleared.json();
    assert!(body["summary"].is_null());
    assert_eq!(body["item"]["revision"], 3);

    let deleted = delete_detail(&ctx.server, chr, entry_id, 3).await;
    assert_eq!(deleted.status_code(), 204, "{}", deleted.text());
    let missing = detail(&ctx.server, chr, entry_id).await;
    assert_eq!(missing.status_code(), 404, "{}", missing.text());
}

#[tokio::test]
async fn knowledge_patch_omitted_summary_preserves_value() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let added = add_entry(
        &ctx.server,
        json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "KeepSummary",
            "summary": "stay"
        }),
    )
    .await;
    let entry_id = added["item"]["entry_id"].as_str().unwrap();

    let patched = patch_detail(
        &ctx.server,
        chr,
        entry_id,
        json!({ "expected_revision": 0, "canonical_name": "Renamed" }),
    )
    .await;
    assert_eq!(patched.status_code(), 200, "{}", patched.text());
    let body: Value = patched.json();
    assert_eq!(body["summary"], "stay");
}

#[tokio::test]
async fn world_create_with_summary_is_invalid_input() {
    let ctx = ctx().await;
    let resp = ctx
        .server
        .post("/v1/daemon/actor-knowledge/entries")
        .json(&json!({
            "owner_kind": "world",
            "world_id": WORLD_A,
            "block_type": "item",
            "canonical_name": "Nope",
            "summary": "forbidden"
        }))
        .await;
    assert_eq!(resp.status_code(), 422, "{}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input");
}

#[tokio::test]
async fn knowledge_detail_wrong_scope_is_404_without_revision_leak() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let added = add_entry(
        &ctx.server,
        json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "Secret",
            "summary": "hidden"
        }),
    )
    .await;
    let entry_id = added["item"]["entry_id"].as_str().unwrap();

    let foreign_chr = detail(&ctx.server, MISSING_CHR, entry_id).await;
    assert_eq!(foreign_chr.status_code(), 404, "{}", foreign_chr.text());
    assert!(!foreign_chr.text().contains("revision"));

    let foreign_entry = detail(&ctx.server, chr, "kb_missingentry").await;
    assert_eq!(foreign_entry.status_code(), 404, "{}", foreign_entry.text());
    assert!(!foreign_entry.text().contains("revision"));
}

#[tokio::test]
async fn knowledge_stale_revision_and_in_use_delete_errors() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let added = add_entry(
        &ctx.server,
        json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "Anchored",
            "summary": "text"
        }),
    )
    .await;
    let entry_id = added["item"]["entry_id"].as_str().unwrap();

    let stale = patch_detail(
        &ctx.server,
        chr,
        entry_id,
        json!({ "expected_revision": 9, "summary": "nope" }),
    )
    .await;
    assert_eq!(stale.status_code(), 409, "{}", stale.text());
    let body: Value = stale.json();
    assert_eq!(body["error"]["code"], "knowledge_revision_conflict");

    sqlx::query(
        "INSERT INTO kb_source_anchors (key_block_id, anchor_ordinal, source_anchor_json) VALUES (?, 0, '{}')",
    )
    .bind(entry_id)
    .execute(&ctx.pool)
    .await
    .unwrap();

    let in_use = delete_detail(&ctx.server, chr, entry_id, 0).await;
    assert_eq!(in_use.status_code(), 409, "{}", in_use.text());
    let body: Value = in_use.json();
    assert_eq!(body["error"]["code"], "knowledge_entry_in_use");
}

#[tokio::test]
async fn knowledge_empty_patch_is_invalid_input() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let added = add_entry(
        &ctx.server,
        json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "Noop"
        }),
    )
    .await;
    let entry_id = added["item"]["entry_id"].as_str().unwrap();

    let resp = patch_detail(
        &ctx.server,
        chr,
        entry_id,
        json!({ "expected_revision": 0 }),
    )
    .await;
    assert_eq!(resp.status_code(), 422, "{}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input");
}

#[tokio::test]
async fn knowledge_archived_character_detail_read_write_split() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let added = add_entry(
        &ctx.server,
        json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "ArchiveMe",
            "summary": "retained"
        }),
    )
    .await;
    let entry_id = added["item"]["entry_id"].as_str().unwrap();

    sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
        .bind(chr)
        .execute(&ctx.pool)
        .await
        .unwrap();

    let read = detail(&ctx.server, chr, entry_id).await;
    assert_eq!(read.status_code(), 200, "{}", read.text());
    let body: Value = read.json();
    assert_eq!(body["summary"], "retained");

    let write = patch_detail(
        &ctx.server,
        chr,
        entry_id,
        json!({ "expected_revision": 0, "summary": "nope" }),
    )
    .await;
    assert_eq!(write.status_code(), 409, "{}", write.text());
    let body: Value = write.json();
    assert_eq!(body["error"]["code"], "character_inactive");
}

#[tokio::test]
async fn knowledge_delete_malformed_expected_revision_is_invalid_input() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let added = add_entry(
        &ctx.server,
        json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "DeleteQuery"
        }),
    )
    .await;
    let entry_id = added["item"]["entry_id"].as_str().unwrap();

    for (query, label) in [
        ("", "missing"),
        ("expected_revision=abc", "non-integer"),
        ("expected_revision=9223372036854775807", "overflow"),
    ] {
        let resp = delete_detail_query(&ctx.server, chr, entry_id, query).await;
        assert_eq!(resp.status_code(), 422, "{label}: {}", resp.text());
        let body: Value = resp.json();
        assert_eq!(body["error"]["code"], "invalid_input", "{label}");
    }
}

#[tokio::test]
async fn character_create_oversize_multibyte_summary_is_invalid_input() {
    let ctx = ctx().await;
    let created = create_character(&ctx.server, "Ava", WORLD_A).await;
    let chr = created["character"]["character_id"].as_str().unwrap();
    let at_limit = "字".repeat(21845) + "a";
    assert_eq!(at_limit.len(), 65536);
    let over = format!("{at_limit}b");
    assert!(over.len() > 65536);
    let resp = ctx
        .server
        .post("/v1/daemon/actor-knowledge/entries")
        .json(&json!({
            "owner_kind": "character",
            "character_id": chr,
            "block_type": "item",
            "canonical_name": "OversizeSummary",
            "summary": over
        }))
        .await;
    assert_eq!(resp.status_code(), 422, "{}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input");
}
