//! `GET/PUT /v1/daemon/works/{work_id}/cron` — per-Work cron config
//! (V1.171 P2 AR-29) handler contract tests.
//!
//! Covers:
//! - GET with unset `schedule_json` → spec defaults + `is_default: true`
//! - PUT full-body happy path persists; GET reads back (``is_default``: false)
//! - PUT with the GET body as CAS pre-image (byte-exact round-trip)
//! - PUT invalid cron → 400 with stable code `E_CRON_INVALID_EXPR`
//! - PUT invalid timezone → 400 with stable code `E_CRON_INVALID_TZ`
//! - PUT CAS mismatch → 409 (stale `expected_current_json`; concurrent write)
//! - PUT on unknown work → 404; GET on unknown work → 404

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
async fn put_without_preimage_is_unconditional_write() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.server).await;

    // First write stores a custom schedule.
    let first = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&custom_cron_body())
        .await;
    first.assert_status(StatusCode::OK);

    // Second PUT without a pre-image is an unconditional (blind) write —
    // `expected_current_json` is an optional CAS guard, not a requirement.
    let resp = ctx
        .server
        .put(&format!("/v1/daemon/works/{work_id}/cron"))
        .json(&default_cron_body())
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(
        body["tz"], "UTC",
        "blind write must replace the stored schedule"
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
