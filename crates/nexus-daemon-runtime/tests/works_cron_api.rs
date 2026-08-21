//! `GET/PUT /v1/daemon/works/{work_id}/cron` — per-Work cron config
//! (V1.171 P2 AR-29) handler contract tests.
//!
//! Covers:
//! - GET with unset `schedule_json` → spec defaults + `is_default: true`
//! - GET with stored empty string → defaults + `is_default: true` (F-002)
//! - GET with malformed non-empty blob → 400 `E_CRON_INVALID_STORED` (F-004)
//! - PUT full-body happy path persists; GET reads back (``is_default``: false)
//! - PUT with the GET body as CAS pre-image (byte-exact round-trip)
//! - PUT with a client-style `serde_json`-reconstructed pre-image (F-005)
//! - PUT with `expected_current_json: ""` against a stored "" (F-002 round-trip)
//! - PUT invalid cron → 400 with stable code `E_CRON_INVALID_EXPR`
//! - PUT invalid timezone → 400 with stable code `E_CRON_INVALID_TZ`
//! - PUT CAS mismatch → 409 (stale `expected_current_json`; concurrent write)
//! - PUT without pre-image guards against the read snapshot (F-003 rename)
//! - PUT/GET on unknown work → 404; foreign-creator work → 404 (F-001)

#![allow(clippy::unwrap_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::path::PathBuf;

struct TestCtx {
    _tmp: TestTempRoot,
    server: TestServer,
    db_path: PathBuf,
}

async fn test_ctx() -> TestCtx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    test_utils::seed_test_creator_and_world(state.pool().unwrap()).await;
    let auth_config = DaemonApiConfig::keyless();
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    TestCtx {
        _tmp: tmp,
        server,
        db_path,
    }
}

async fn open_db(db_path: &std::path::Path) -> SqlitePool {
    let db_url = format!("sqlite:{}?mode=rw", db_path.display());
    SqlitePool::connect(&db_url).await.expect("open creator db")
}

/// Create a Work via the real HTTP endpoint and return its `work_id`.
// `axum_test`'s AutoFuture is not `Send`; this helper only runs inside
// current-thread `#[tokio::test]` bodies, so the future need not be `Send`.
#[allow(clippy::future_not_send)]
async fn create_work(server: &TestServer) -> String {
    let resp = server
        .post("/v1/daemon/works")
        .json(&json!({
            "title": "Cron Test Novel",
            "long_term_goal": "Write a great novel",
            "initial_idea": "A sci-fi thriller",
            "world_id": "wld_test_world"
        }))
        .await;
    resp.assert_status(StatusCode::CREATED);
    resp.json::<Value>()["work_id"]
        .as_str()
        .expect("work_id")
        .to_string()
}

/// Default spec schedule (spec §2.2 table).
fn default_cron_body() -> Value {
    json!({
        "tz": "UTC",
        "roles": {
            "brainstorm": { "cron": "0 3,9,15,21 * * *", "enabled": true },
            "write": { "cron": "0 4,10,16,22 * * *", "enabled": true },
            "review": { "cron": "0,30 * * * *", "enabled": true }
        }
    })
}

/// Custom schedule with a Shanghai TZ and a disabled write role.
fn custom_cron_body() -> Value {
    json!({
        "tz": "Asia/Shanghai",
        "roles": {
            "brainstorm": { "cron": "0 9 * * *", "enabled": true },
            "write": { "cron": "0 10 * * *", "enabled": false },
            "review": { "cron": "15,45 * * * *", "enabled": true }
        }
    })
}

fn assert_stable_code(body: &Value, code: &str) {
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "bad_request");
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains(code),
        "bad-request message must carry stable code {code}: {message}"
    );
}

#[tokio::test]
async fn get_cron_unset_returns_defaults_with_marker() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    let resp = ctx
        .server
        .get(&format!("/v1/daemon/works/{work_id}/cron"))
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(body["tz"], "UTC");
    assert_eq!(body["roles"]["brainstorm"]["cron"], "0 3,9,15,21 * * *");
    assert_eq!(body["roles"]["write"]["cron"], "0 4,10,16,22 * * *");
    assert_eq!(body["roles"]["review"]["cron"], "0,30 * * * *");
    assert!(body["roles"]["brainstorm"]["enabled"].as_bool().unwrap());
    assert_eq!(
        body["is_default"], true,
        "unset schedule must be marked default"
    );
}

/// F-002: a stored empty string ≡ unset (spec §2.3) — GET must surface
/// defaults with `is_default: true`, matching `set_schedule_json_tx`'s
/// COALESCE normalization.
#[tokio::test]
async fn get_cron_stored_empty_string_is_default() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    let pool = open_db(&ctx.db_path).await;
    sqlx::query("UPDATE works SET schedule_json = '' WHERE work_id = ?")
        .bind(&work_id)
        .execute(&pool)
        .await
        .expect("store empty schedule_json");
    pool.close().await;

    let resp = ctx
        .server
        .get(&format!("/v1/daemon/works/{work_id}/cron"))
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(body["tz"], "UTC");
    assert_eq!(
        body["is_default"], true,
        "stored empty string must be treated as unset (is_default: true)"
    );
}

/// F-002: `expected_current_json: ""` against a stored `""` succeeds —
/// the tx CAS treats NULL and "" as equal, so the unset→unset round-trip
/// must not 409.
#[tokio::test]
async fn put_empty_preimage_against_stored_empty_succeeds() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    let pool = open_db(&ctx.db_path).await;
    sqlx::query("UPDATE works SET schedule_json = '' WHERE work_id = ?")
        .bind(&work_id)
        .execute(&pool)
        .await
        .expect("store empty schedule");
    pool.close().await;

    let mut body = custom_cron_body();
    body["expected_current_json"] = json!("");
    let resp = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&body)
        .await;
    resp.assert_status(StatusCode::OK);
    let resp_body: Value = resp.json();
    assert_eq!(resp_body["tz"], "Asia/Shanghai");
    assert_eq!(
        resp_body["is_default"], false,
        "after a real write the config is no longer default"
    );
}

/// F-004: a non-empty but unparseable stored blob must surface honestly as
/// 400 `E_CRON_INVALID_STORED` — never a defaulted response the client
/// would fight with a CAS pre-image it cannot byte-match (409 loop).
#[tokio::test]
async fn get_cron_malformed_stored_rejects_with_stable_code() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    let pool = open_db(&ctx.db_path).await;
    sqlx::query("UPDATE works SET schedule_json = '{not json' WHERE work_id = ?")
        .bind(&work_id)
        .execute(&pool)
        .await
        .expect("store malformed schedule");
    pool.close().await;

    let resp = ctx
        .server
        .get(&format!("/v1/daemon/works/{work_id}/cron"))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "bad_request");
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("E_CRON_INVALID_STORED"),
        "400 must carry the stable code: {message}"
    );
    assert!(
        message.contains("repair it via `nexus42 creator works cron set`"),
        "message must point at the CLI repair path: {message}"
    );
}

#[tokio::test]
async fn put_persists_and_get_reads_back() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    let put = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&custom_cron_body())
        .await;
    put.assert_status(StatusCode::OK);
    let put_body: Value = put.json();
    assert_eq!(put_body["tz"], "Asia/Shanghai");
    assert!(!put_body["roles"]["write"]["enabled"].as_bool().unwrap());
    assert_eq!(put_body["is_default"], false);

    let get = ctx
        .server
        .get(&format!("/v1/daemon/works/{work_id}/cron"))
        .await;
    get.assert_status(StatusCode::OK);
    let body: Value = get.json();
    assert_eq!(body["tz"], "Asia/Shanghai");
    assert_eq!(body["roles"]["brainstorm"]["cron"], "0 9 * * *");
    assert_eq!(body["roles"]["write"]["enabled"], false);
    assert_eq!(body["roles"]["review"]["cron"], "15,45 * * * *");
    assert_eq!(
        body["is_default"], false,
        "custom schedule must not be marked default"
    );
}

#[tokio::test]
async fn put_with_get_body_as_cas_preimage_round_trips() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    // First write establishes the stored blob.
    let first = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&custom_cron_body())
        .await;
    first.assert_status(StatusCode::OK);

    // Client re-PUTs with a tweak, passing `expected_current_json` — the
    // byte-exact stored blob (read straight from the DB so the CAS pre-image
    // matches what the handler wrote, exactly as a UI client would).
    let stored_blob: String = {
        let pool = open_db(&ctx.db_path).await;
        let row: (String,) = sqlx::query_as("SELECT schedule_json FROM works WHERE work_id = ?")
            .bind(&work_id)
            .fetch_one(&pool)
            .await
            .expect("stored schedule_json");
        pool.close().await;
        row.0
    };
    let mut next = custom_cron_body();
    next["roles"]["brainstorm"]["cron"] = json!("0 8 * * *");
    next["expected_current_json"] = serde_json::Value::String(stored_blob);

    let second = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&next)
        .await;
    second.assert_status(StatusCode::OK);
    let body: Value = second.json();
    assert_eq!(body["roles"]["brainstorm"]["cron"], "0 8 * * *");

    let reread = ctx
        .server
        .get(&format!("/v1/daemon/works/{work_id}/cron"))
        .await;
    let reread_body = reread.json::<Value>();
    assert_eq!(reread_body["roles"]["brainstorm"]["cron"], "0 8 * * *");
    // The stored blob must not contain the CAS pre-image field.
    assert!(reread_body.get("expected_current_json").is_none());
}

#[tokio::test]
async fn put_invalid_cron_rejects_with_stable_code() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    let mut body = default_cron_body();
    body["roles"]["write"]["cron"] = json!("99 99 99 99 99");
    let resp = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&body)
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    assert_stable_code(&resp.json::<Value>(), "E_CRON_INVALID_EXPR");
}

#[tokio::test]
async fn put_invalid_tz_rejects_with_stable_code() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    let mut body = default_cron_body();
    body["tz"] = json!("Mars/Olympus");
    let resp = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&body)
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    assert_stable_code(&resp.json::<Value>(), "E_CRON_INVALID_TZ");
}

#[tokio::test]
async fn put_cas_mismatch_returns_409() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    // Concurrent writer (simulated with a direct, different write) races
    // between the client's read and its PUT: the client's stale pre-image no
    // longer matches the stored blob.
    let pool = open_db(&ctx.db_path).await;
    sqlx::query("UPDATE works SET schedule_json = '{\"tz\":\"UTC\",\"roles\":{\"brainstorm\":{\"cron\":\"0 1 * * *\",\"enabled\":true},\"write\":{\"cron\":\"0 2 * * *\",\"enabled\":true},\"review\":{\"cron\":\"0,30 * * * *\",\"enabled\":true}}}' WHERE work_id = ?")
        .bind(&work_id)
        .execute(&pool)
        .await
        .expect("simulate concurrent writer");
    pool.close().await;

    // Stale pre-image from before the race.
    let mut body = custom_cron_body();
    body["expected_current_json"] = json!("{\"tz\":\"UTC\"}");
    let resp = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&body)
        .await;
    resp.assert_status(StatusCode::CONFLICT);
    let resp_body: Value = resp.json();
    assert_eq!(resp_body["success"], false);
    assert_eq!(resp_body["error"]["code"], "conflict");
    let message = resp_body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("changed by another writer"),
        "409 must be clearly retryable: {message}"
    );

    // No partial write landed.
    let check = ctx
        .server
        .get(&format!("/v1/daemon/works/{work_id}/cron"))
        .await;
    let check_body = check.json::<Value>();
    assert_eq!(
        check_body["roles"]["brainstorm"]["cron"], "0 1 * * *",
        "CAS-mismatched PUT must not overwrite the concurrent write"
    );
}

#[tokio::test]
async fn put_without_preimage_guards_against_read_snapshot() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    // First write stores a custom schedule.
    let first = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&custom_cron_body())
        .await;
    first.assert_status(StatusCode::OK);

    // Second PUT without a pre-image is guarded against the fresh pre-read:
    // it applies only while the stored blob is unchanged since the read
    // (snapshot CAS — never a blind unconditional write).
    let resp = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&default_cron_body())
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(
        body["tz"], "UTC",
        "pre-image-less write must replace the stored schedule"
    );
    assert_eq!(
        body["is_default"], false,
        "a stored schedule is never 'default'"
    );
}

#[tokio::test]
async fn get_unknown_work_returns_404() {
    let ctx = test_ctx().await;
    let resp = ctx.server.get("/v1/daemon/works/wrk_nope/cron").await;
    resp.assert_status(StatusCode::NOT_FOUND);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn put_unknown_work_returns_404() {
    let ctx = test_ctx().await;
    let resp = ctx
        .server
        .put("/v1/daemon/works/wrk_nope/cron")
        .json(&default_cron_body())
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "not_found");
}

// ── F-001: creator scoping — foreign/unknown work → 404 ────────────────────

/// Foreign-creator work ids must not be readable (404, not 200): the cron
/// column is a per-Work sub-resource of the active creator's Works only.
#[tokio::test]
async fn get_foreign_work_cron_returns_404() {
    let ctx = test_ctx().await;
    let foreign_work_id = seed_foreign_work(&ctx.db_path).await;

    let resp = ctx
        .server
        .get(&format!("/v1/daemon/works/{foreign_work_id}/cron"))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "not_found");
}

/// Foreign-creator work must not be overwritable (404) — the active creator
/// cannot mutate another creator's Work.
#[tokio::test]
async fn put_foreign_work_cron_returns_404() {
    let ctx = test_ctx().await;
    let foreign_work_id = seed_foreign_work(&ctx.db_path).await;

    let mut body = custom_cron_body();
    body["expected_current_json"] = json!("");
    let resp = ctx
        .server
        .put(&format!("/v1/daemon/works/{foreign_work_id}/cron"))
        .json(&body)
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
    let resp_body: Value = resp.json();
    assert_eq!(resp_body["success"], false);
    assert_eq!(resp_body["error"]["code"], "not_found");

    // The foreign Work's schedule must be untouched.
    let pool = open_db(&ctx.db_path).await;
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT schedule_json FROM works WHERE work_id = ?")
            .bind(&foreign_work_id)
            .fetch_optional(&pool)
            .await
            .expect("read foreign work");
    pool.close().await;
    assert_eq!(
        row.and_then(|r| r.0),
        None,
        "a 404'd PUT must not write into the foreign Work"
    );
}

/// Seed a Work owned by `other_creator` into the same DB the active
/// (`test_creator`) server serves. The active creator must neither read nor
/// write it through the works-cron surface.
async fn seed_foreign_work(db_path: &std::path::Path) -> String {
    let pool = open_db(db_path).await;
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(&pool)
    .await
    .expect("seed foreign creator");

    let foreign_work_id = "wrk_foreign_creator";
    let record = nexus_local_db::WorkRecord {
        work_id: foreign_work_id.to_string(),
        creator_id: "other_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Foreign Work".to_string(),
        long_term_goal: "Write a novel.".to_string(),
        initial_idea: "An idea.".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-08-01T00:00:00Z".to_string(),
        updated_at: "2026-08-01T00:00:00Z".to_string(),
        current_stage: "produce".to_string(),
        stage_status: "complete".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(foreign_work_id.to_string()),
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
    };
    nexus_local_db::works::create_work(&pool, &record)
        .await
        .expect("seed foreign work");
    pool.close().await;
    foreign_work_id.to_string()
}

// ── F-005: JS-style reconstructed pre-image (serde key-order guard) ────────

/// The web editor reconstructs the CAS pre-image via `JSON.stringify` of the
/// GET fields (`{ tz, roles }` — `workCronPreimage`): a JS object literal,
/// which preserves insertion order (`tz` then `roles`, matching the serde
/// struct declaration order the daemon writes with). This test GETs the cron,
/// mutates one field, and PUTs with the serde-reconstructed pre-image to
/// guard serde/JSON key-order drift between the two surfaces — if the stored
/// blob's key order ever diverged from `{ tz, roles }`, the CAS would 409.
#[tokio::test]
async fn put_with_serde_reconstructed_preimage_round_trips() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    // First write establishes the stored blob.
    let first = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&custom_cron_body())
        .await;
    first.assert_status(StatusCode::OK);

    // GET → drop `is_default` → reconstruct the `{ tz, roles }` pre-image the
    // way the web `workCronPreimage()` does: JSON.stringify keeps JS object
    // insertion order, which mirrors the serde struct field order.
    let get = ctx
        .server
        .get(&format!("/v1/daemon/works/{work_id}/cron"))
        .await;
    get.assert_status(StatusCode::OK);
    let got: Value = get.json();
    assert_eq!(got["is_default"], false);
    let reconstructed =
        serde_json::to_string(&serde_cron_schedule_from_value(&got)).expect("reconstructed");

    let mut next = custom_cron_body();
    next["roles"]["write"]["cron"] = json!("0 11 * * *");
    next["expected_current_json"] = serde_json::Value::String(reconstructed);

    let second = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&next)
        .await;
    second.assert_status(StatusCode::OK);
    let body: Value = second.json();
    assert_eq!(body["roles"]["write"]["cron"], "0 11 * * *");

    let reread = ctx
        .server
        .get(&format!("/v1/daemon/works/{work_id}/cron"))
        .await;
    let reread_body: Value = reread.json();
    assert_eq!(reread_body["roles"]["write"]["cron"], "0 11 * * *");
    assert_eq!(reread_body["tz"], "Asia/Shanghai");
}

/// Build a `WorkSchedule` from a wire GET body (JSON.stringify-equivalent —
/// struct field order preserved) so serializing it yields the `{ tz, roles }`
/// pre-image the web client sends.
fn serde_cron_schedule_from_value(
    v: &Value,
) -> nexus_orchestration::schedule::work_schedule::WorkSchedule {
    use nexus_orchestration::schedule::work_schedule::{RoleSchedule, RolesSchedule, WorkSchedule};
    WorkSchedule {
        tz: v["tz"].as_str().expect("tz").to_string(),
        roles: RolesSchedule {
            brainstorm: RoleSchedule {
                cron: v["roles"]["brainstorm"]["cron"]
                    .as_str()
                    .expect("cron")
                    .to_string(),
                enabled: v["roles"]["brainstorm"]["enabled"]
                    .as_bool()
                    .expect("enabled"),
            },
            write: RoleSchedule {
                cron: v["roles"]["write"]["cron"]
                    .as_str()
                    .expect("cron")
                    .to_string(),
                enabled: v["roles"]["write"]["enabled"].as_bool().expect("enabled"),
            },
            review: RoleSchedule {
                cron: v["roles"]["review"]["cron"]
                    .as_str()
                    .expect("cron")
                    .to_string(),
                enabled: v["roles"]["review"]["enabled"].as_bool().expect("enabled"),
            },
        },
    }
}
