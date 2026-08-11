//! V1.151 P0 — `POST /v1/daemon/moment-directive` integration tests
//! (DF-76 T3).
//!
//! Proves the directive set/show/clear Daemon HTTP surface end-to-end over a
//! real `axum` router + `SQLite`:
//!
//! - `set` (Work + World scope) → 200 with the inserted directive row
//!   (incl. body, `status: "active"`, `ttl_remaining`).
//! - `set` without `replace` while a directive is already active in the
//!   scope → 409 `conflict` (unique partial index, no silent overwrite).
//! - `set` with `"replace": true` → 200; the old row is retained with
//!   `replaced_by` set to the new id (soft-delete chain).
//! - `show` → 200 with the row incl. body; no active directive → 200 `{}`.
//! - `clear` → 200 `{}`; the row is soft-deleted (`status: "expired"`,
//!   retained for DF-76 inspection); subsequent `show` → `{}`.
//! - Ownership guard: a scope owned by a *different* creator → 403
//!   `forbidden` (Work scope via `works::get_work`, World scope via
//!   `narrative_write::is_world_owned`).
//! - Validation mirroring CLI `handle_set`: empty body → 400 `invalid_input`.
//! - Unauthenticated request (keyed-all auth, no `X-API-Key`) → 401
//!   `auth_required`.
//!
//! Auth is keyless for the happy/ownership/validation tests
//! (`DaemonApiConfig::keyless`); the unauthenticated case uses a keyed-all
//! config (`DaemonApiConfig::keyed`). The tier-2 `require_active_creator`
//! gate reads the active creator from the seeded `config.toml`
//! (`test_creator`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::{create_work, moment_directive::get_by_id, WorkRecord};
use serde_json::{json, Value};

/// World owned by `test_creator` (seeded by `seed_test_creator_and_world`).
const OWNED_WORLD: &str = "wld_test_world";
/// World owned by `other_creator` (ownership-gate tests).
const FOREIGN_WORLD: &str = "wld_foreign";
/// Work owned by `test_creator` (seeded here), bound to `OWNED_WORLD`.
const OWNED_WORK: &str = "wrk_owned";
/// Work owned by `other_creator` (ownership-gate tests).
const FOREIGN_WORK: &str = "wrk_foreign";

struct Ctx {
    _tmp: TestTempRoot,
    server: TestServer,
    pool: sqlx::SqlitePool,
}

/// Standard server: seeded creator + owned world/work (+ foreign world/work
/// for the ownership gate) under keyless auth.
async fn ctx() -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().expect("pool").clone();
    test_utils::seed_test_creator_and_world(&pool).await;
    seed_foreign_world(&pool).await;
    seed_works(&pool).await;
    let app = api::create_router(state, DaemonApiConfig::keyless());
    let server = TestServer::new(app).expect("test server");
    Ctx {
        _tmp: tmp,
        server,
        pool,
    }
}

/// Server under keyed-all auth: a request without the `X-API-Key` header
/// must be rejected 401 by `require_api_key` before any handler runs.
async fn ctx_keyed() -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().expect("pool").clone();
    test_utils::seed_test_creator_and_world(&pool).await;
    seed_works(&pool).await;
    let app = api::create_router(state, DaemonApiConfig::keyed("test-key"));
    let server = TestServer::new(app).expect("test server");
    Ctx {
        _tmp: tmp,
        server,
        pool,
    }
}

/// Seed a second World owned by a *different* creator (ownership-gate tests).
async fn seed_foreign_world(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only seed against the known creators/narrative_worlds schema.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
    // SAFETY: test-only seed against the known narrative_worlds schema.
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
           VALUES (?, 'ws', 'other_creator', 'Foreign World', 'foreign-world', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(FOREIGN_WORLD)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed one Work per creator: `OWNED_WORK` (`test_creator`, bound to
/// `OWNED_WORLD`) and `FOREIGN_WORK` (`other_creator`, bound to
/// `FOREIGN_WORLD`).
async fn seed_works(pool: &sqlx::SqlitePool) {
    create_work(
        pool,
        &work_record(OWNED_WORK, "test_creator", Some(OWNED_WORLD)),
    )
    .await
    .unwrap();
    create_work(
        pool,
        &work_record(FOREIGN_WORK, "other_creator", Some(FOREIGN_WORLD)),
    )
    .await
    .unwrap();
}

/// Build a `WorkRecord` with sane defaults for the test DB.
fn work_record(work_id: &str, creator_id: &str, world_id: Option<&str>) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: creator_id.to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: format!("Work {work_id}"),
        long_term_goal: "Write a novel.".to_string(),
        initial_idea: "An idea.".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: world_id.map(str::to_string),
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-08-01T00:00:00Z".to_string(),
        updated_at: "2026-08-01T00:00:00Z".to_string(),
        current_stage: "produce".to_string(),
        stage_status: "complete".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(work_id.to_string()),
        total_planned_chapters: Some(10),
        current_chapter: 1,
        auto_chain_enabled: true,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    }
}

/// A minimal `set` body for the owned Work scope.
fn set_body() -> Value {
    json!({
        "action": "set",
        "scope": { "kind": "work", "id": OWNED_WORK },
        "body": "Keep the prose terse.",
        "insert_depth": "mid",
        "ttl_kind": "generations",
        "ttl_remaining": 3,
    })
}

/// Assert the canonical daemon API error envelope
/// (`{"success": false, "error": {"code", "message", ...}}`).
fn assert_error_envelope(resp: &axum_test::TestResponse, status: StatusCode, code: &str) {
    assert_eq!(resp.status_code(), status, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["success"], false, "body={body}");
    assert_eq!(body["error"]["code"], code, "body={body}");
    assert!(
        body["error"]["message"].is_string(),
        "error.message must be a string: {body}"
    );
}

// ─── POST /v1/daemon/moment-directive ───────────────────────────────────

/// Work-scope round trip: `set` → row returned (incl. body); `show` → the
/// row incl. body; `clear` → `{}` and the row is soft-deleted (retained);
/// subsequent `show` → `{}`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_show_clear_work_scope_round_trip() {
    let ctx = ctx().await;

    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&set_body())
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let set: Value = resp.json();
    assert_eq!(set["status"], "active", "body={set}");
    assert_eq!(set["scope_kind"], "work", "body={set}");
    assert_eq!(set["scope_id"], OWNED_WORK, "body={set}");
    assert_eq!(set["body"], "Keep the prose terse.", "body={set}");
    assert_eq!(set["insert_depth"], "mid", "body={set}");
    assert_eq!(set["ttl_kind"], "generations", "body={set}");
    assert_eq!(set["ttl_remaining"], 3, "body={set}");
    // DR-63 (V1.158 P2): the typed response keeps the exact 15-field row
    // shape the pre-schema `Json<Value>` emitted — including the nullable
    // fields present-as-`null` (never omitted) and no extra keys.
    assert_eq!(
        set.as_object().map(serde_json::Map::len),
        Some(15),
        "set must return exactly the 15 directive-row fields, got: {set}"
    );
    assert_eq!(set["last_focused_event_id"], Value::Null, "body={set}");
    assert_eq!(set["expires_at"], Value::Null, "body={set}");
    assert_eq!(set["replaced_by"], Value::Null, "body={set}");
    assert!(set["creator_id"].is_string(), "body={set}");
    assert!(set["created_at"].is_i64(), "body={set}");
    assert!(set["updated_at"].is_i64(), "body={set}");
    assert!(set["clear_on_scene_change"].is_boolean(), "body={set}");
    let directive_id = set["directive_id"]
        .as_str()
        .expect("directive_id")
        .to_string();

    // show → the row incl. body (author surface, DF-76).
    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "show",
            "scope": { "kind": "work", "id": OWNED_WORK },
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let shown: Value = resp.json();
    assert_eq!(shown["directive_id"], directive_id, "body={shown}");
    assert_eq!(shown["status"], "active", "body={shown}");
    assert_eq!(shown["body"], "Keep the prose terse.", "body={shown}");

    // clear → {} and the row is soft-deleted (status='expired', retained).
    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "clear",
            "scope": { "kind": "work", "id": OWNED_WORK },
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    assert_eq!(resp.json::<Value>(), json!({}), "clear must return {{}}");

    let row = get_by_id(&ctx.pool, &directive_id)
        .await
        .expect("row retained after clear")
        .expect("row still present (soft-delete)");
    assert_eq!(
        row.status, "expired",
        "clear soft-deletes (DF-76 retention)"
    );
    assert!(row.expires_at.is_some(), "expires_at set on soft-delete");

    // show after clear → {} (no active directive).
    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "show",
            "scope": { "kind": "work", "id": OWNED_WORK },
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    assert_eq!(
        resp.json::<Value>(),
        json!({}),
        "show after clear must return {{}}"
    );
}

/// World-scope round trip: `set` on the owned World → row returned;
/// `show` → row incl. body.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_show_world_scope_round_trip() {
    let ctx = ctx().await;

    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "set",
            "scope": { "kind": "world", "id": OWNED_WORLD },
            "body": "World override.",
            "insert_depth": "head",
            "ttl_kind": "chapters",
            "ttl_remaining": 5,
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let set: Value = resp.json();
    assert_eq!(set["scope_kind"], "world", "body={set}");
    assert_eq!(set["scope_id"], OWNED_WORLD, "body={set}");
    assert_eq!(set["body"], "World override.", "body={set}");
    assert_eq!(set["insert_depth"], "head", "body={set}");
    assert_eq!(set["ttl_kind"], "chapters", "body={set}");
    assert_eq!(set["ttl_remaining"], 5, "body={set}");

    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "show",
            "scope": { "kind": "world", "id": OWNED_WORLD },
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let shown: Value = resp.json();
    assert_eq!(shown["body"], "World override.", "body={shown}");
    assert_eq!(shown["ttl_kind"], "chapters", "body={shown}");
}

/// `set` without `replace` while a directive is already active in the scope
/// → 409 `conflict` (unique partial index — no silent overwrite).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_without_replace_when_active_conflicts_409() {
    let ctx = ctx().await;

    let first = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&set_body())
        .await;
    assert_eq!(first.status_code(), StatusCode::OK, "body={}", first.text());
    let first_id = first.json::<Value>()["directive_id"]
        .as_str()
        .expect("directive_id")
        .to_string();

    let second = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&set_body())
        .await;
    assert_error_envelope(&second, StatusCode::CONFLICT, "conflict");
    assert!(
        second.text().contains("replace"),
        "conflict message must point at the replace option: {}",
        second.text()
    );

    // The original row is untouched (still active, no replaced_by).
    let row = get_by_id(&ctx.pool, &first_id)
        .await
        .expect("read row")
        .expect("first row still active");
    assert_eq!(row.status, "active", "no silent overwrite");
    assert!(
        row.replaced_by.is_none(),
        "no replaced_by without --replace"
    );
}

/// `set` with `"replace": true` supersedes the active directive: 200, and
/// the old row is soft-deleted with `replaced_by` set to the new id.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_with_replace_supersedes_active() {
    let ctx = ctx().await;

    let first = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&set_body())
        .await;
    assert_eq!(first.status_code(), StatusCode::OK, "body={}", first.text());
    let first_id = first.json::<Value>()["directive_id"]
        .as_str()
        .expect("directive_id")
        .to_string();

    let mut replace = set_body();
    replace["body"] = json!("Second directive.");
    replace["replace"] = json!(true);
    let second = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&replace)
        .await;
    assert_eq!(
        second.status_code(),
        StatusCode::OK,
        "body={}",
        second.text()
    );
    let second_id = second.json::<Value>()["directive_id"]
        .as_str()
        .expect("directive_id")
        .to_string();
    assert_ne!(second_id, first_id);

    let old = get_by_id(&ctx.pool, &first_id)
        .await
        .expect("read old row")
        .expect("old row retained");
    assert_eq!(old.status, "expired", "replaced row soft-deleted");
    assert_eq!(
        old.replaced_by.as_deref(),
        Some(second_id.as_str()),
        "replaced_by chains to the new directive"
    );

    // show now returns the new directive.
    let shown = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "show",
            "scope": { "kind": "work", "id": OWNED_WORK },
        }))
        .await;
    assert_eq!(shown.status_code(), StatusCode::OK, "body={}", shown.text());
    assert_eq!(
        shown.json::<Value>()["directive_id"],
        second_id,
        "show reflects the replacement"
    );
}

/// `show` with no active directive → 200 `{}`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn show_without_active_directive_returns_empty_object() {
    let ctx = ctx().await;

    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "show",
            "scope": { "kind": "work", "id": OWNED_WORK },
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    assert_eq!(resp.json::<Value>(), json!({}), "no directive → {{}}");
}

/// W-2 (QC3): `show` for a Work scope resolves the **effective** directive —
/// the Work's own wins; with none, the bound World's override is inherited
/// (mirrors the CLI `resolve_effective_for_show`). Both halves are proven:
/// work-wins over a present World override, and inheritance once the Work's
/// own directive is cleared.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn show_work_scope_resolves_effective_directive() {
    let ctx = ctx().await;

    // Seed a World-scoped override on the owned World.
    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "set",
            "scope": { "kind": "world", "id": OWNED_WORLD },
            "body": "World override.",
            "insert_depth": "head",
            "ttl_kind": "generations",
            "ttl_remaining": 7,
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());

    // Seed the Work's own directive — it must win over the World override.
    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&set_body())
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let set: Value = resp.json();
    let work_directive_id = set["directive_id"]
        .as_str()
        .expect("directive_id")
        .to_string();

    // show on the Work scope → the Work's own directive (work-wins).
    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "show",
            "scope": { "kind": "work", "id": OWNED_WORK },
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let shown: Value = resp.json();
    assert_eq!(
        shown["directive_id"], work_directive_id,
        "work directive must win: {shown}"
    );
    assert_eq!(shown["scope_kind"], "work", "source scope: {shown}");

    // Clear the Work's own directive → show now inherits the World override.
    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "clear",
            "scope": { "kind": "work", "id": OWNED_WORK },
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());

    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "show",
            "scope": { "kind": "work", "id": OWNED_WORK },
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let shown: Value = resp.json();
    assert_eq!(
        shown["body"], "World override.",
        "inherited override: {shown}"
    );
    assert_eq!(
        shown["scope_kind"], "world",
        "inherited source scope: {shown}"
    );
    assert_eq!(
        shown["scope_id"], OWNED_WORLD,
        "inherited source scope id: {shown}"
    );
}

/// Validation mirrors CLI `handle_set`: empty body → 400 `invalid_input`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_empty_body_rejects_400() {
    let ctx = ctx().await;

    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "set",
            "scope": { "kind": "work", "id": OWNED_WORK },
            "body": "   ",
            "insert_depth": "mid",
            "ttl_kind": "generations",
            "ttl_remaining": 3,
        }))
        .await;
    assert_error_envelope(&resp, StatusCode::BAD_REQUEST, "invalid_input");
    assert!(
        resp.text().contains("non-empty"),
        "message must name the non-empty rule: {}",
        resp.text()
    );
}

/// Ownership gate: a scope owned by another creator must reject with 403
/// before any directive behavior runs (Work scope + World scope).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreign_scope_rejects_403() {
    let ctx = ctx().await;

    // Foreign Work scope.
    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "set",
            "scope": { "kind": "work", "id": FOREIGN_WORK },
            "body": "Nope.",
            "insert_depth": "mid",
            "ttl_kind": "generations",
            "ttl_remaining": 3,
        }))
        .await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");

    // Foreign World scope.
    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&json!({
            "action": "set",
            "scope": { "kind": "world", "id": FOREIGN_WORLD },
            "body": "Nope.",
            "insert_depth": "mid",
            "ttl_kind": "generations",
            "ttl_remaining": 3,
        }))
        .await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");
}

/// Unauthenticated request: under keyed-all auth, a request without the
/// `X-API-Key` header is rejected 401 `auth_required` by `require_api_key`
/// before any handler runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unauthenticated_rejects_401() {
    let ctx = ctx_keyed().await;

    let resp = ctx
        .server
        .post("/v1/daemon/moment-directive")
        .json(&set_body())
        .await;
    assert_error_envelope(&resp, StatusCode::UNAUTHORIZED, "auth_required");
}
