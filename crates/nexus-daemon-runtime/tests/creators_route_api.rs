//! Creator by-id route regression tests (Setup Continue hotfix).
//!
//! Verifies `create_router` path matching for Tier-1 GET/PATCH/POST
//! `/v1/daemon/creators/:creator_id` — not handler-only unit tests.
//!
//! Uses `HOME` override so `patch_creator` / `logout_creator` disk helpers
//! (`dirs::home_dir`) do not touch the developer's real `~/.nexus42/`.

#![allow(clippy::unwrap_used)]

use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::Value;
use serial_test::serial;

struct HomeOverride {
    original: Option<String>,
}

impl HomeOverride {
    fn set(home: &std::path::Path) -> Self {
        let original = std::env::var("HOME").ok();
        std::env::set_var("HOME", home);
        Self { original }
    }
}

impl Drop for HomeOverride {
    fn drop(&mut self) {
        match &self.original {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }
}

struct TestCtx {
    _tmp: test_utils::TestTempRoot,
    _home: HomeOverride,
    server: TestServer,
}

async fn test_ctx() -> TestCtx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let user_home = tmp.path().to_path_buf();
    let home = HomeOverride::set(&user_home);
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    test_utils::seed_test_creator_and_world(state.pool().unwrap()).await;
    let auth_config = DaemonApiConfig::keyless();
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    TestCtx {
        _tmp: tmp,
        _home: home,
        server,
    }
}

#[tokio::test]
#[serial]
async fn get_creator_by_id_hits_handler_not_framework_404() {
    let ctx = test_ctx().await;
    let resp = ctx.server.get("/v1/daemon/creators/test_creator").await;
    assert_ne!(
        resp.status_code(),
        404,
        "framework 404 — route not registered; body={}",
        resp.text()
    );
    assert!(
        resp.status_code().is_success(),
        "GET creator by id: {} body={}",
        resp.status_code(),
        resp.text()
    );
    let body: Value = resp.json();
    assert_eq!(body["creator_id"], "test_creator");
}

#[tokio::test]
#[serial]
async fn patch_creator_display_name_happy_path() {
    let ctx = test_ctx().await;
    let resp = ctx
        .server
        .patch("/v1/daemon/creators/test_creator")
        .json(&serde_json::json!({ "display_name": "Setup Display" }))
        .await;
    assert_ne!(
        resp.status_code(),
        404,
        "framework 404 — route not registered; body={}",
        resp.text()
    );
    assert!(
        resp.status_code().is_success(),
        "PATCH creator: {} body={}",
        resp.status_code(),
        resp.text()
    );
    let body: Value = resp.json();
    assert_eq!(body["creator_id"], "test_creator");
    assert_eq!(body["display_name"], "Setup Display");
}

#[tokio::test]
#[serial]
async fn patch_creator_rejects_empty_display_name() {
    let ctx = test_ctx().await;
    let resp = ctx
        .server
        .patch("/v1/daemon/creators/test_creator")
        .json(&serde_json::json!({ "display_name": "" }))
        .await;
    assert_eq!(resp.status_code(), 400);
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input");
}

#[tokio::test]
#[serial]
async fn post_creator_logout_verb_hits_handler_not_405() {
    let ctx = test_ctx().await;
    let resp = ctx
        .server
        .post("/v1/daemon/creators/test_creator:logout")
        .await;
    assert_ne!(
        resp.status_code(),
        405,
        "logout must not be captured as GET/PATCH-only :creator_id; body={}",
        resp.text()
    );
    assert_ne!(
        resp.status_code(),
        404,
        "logout must not be framework 404; body={}",
        resp.text()
    );
    assert!(
        resp.status_code().is_success(),
        "POST logout: {} body={}",
        resp.status_code(),
        resp.text()
    );
    let body: Value = resp.json();
    assert_eq!(body["creator_id"], "test_creator");
}

#[tokio::test]
#[serial]
async fn get_creator_rejects_colon_verb_segment() {
    let ctx = test_ctx().await;
    let resp = ctx
        .server
        .get("/v1/daemon/creators/test_creator:logout")
        .await;
    assert_eq!(resp.status_code(), 400);
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input");
}

// ── POST /v1/daemon/creators — create creator (V1.129 P0 T2) ───────────────

/// The happy path: POST creates a row, returns 201 + `CreatorDetail` body shape
/// (architect lock #1), and the new creator is visible via `GET /v1/daemon/creators`.
#[tokio::test]
#[serial]
async fn post_creators_creates_row_and_returns_201_with_creator_detail() {
    let ctx = test_ctx().await;
    let resp = ctx
        .server
        .post("/v1/daemon/creators")
        .json(&serde_json::json!({ "display_name": "Maya Patel" }))
        .await;

    assert_ne!(
        resp.status_code(),
        404,
        "framework 404 — POST route not registered; body={}",
        resp.text()
    );
    assert_ne!(
        resp.status_code(),
        405,
        "405 Method Not Allowed — POST not chained on the creators route; body={}",
        resp.text()
    );
    assert_eq!(
        resp.status_code(),
        201,
        "expected 201 Created, got {} body={}",
        resp.status_code(),
        resp.text()
    );

    let body: Value = resp.json();
    assert_eq!(body["handle"], Value::Null);
    assert_eq!(body["display_name"], "Maya Patel");
    assert_eq!(body["has_api_key"], false);
    assert_eq!(body["has_cached_token"], false);
    assert_eq!(body["is_active"], false);
    let creator_id = body["creator_id"]
        .as_str()
        .expect("creator_id should be a string")
        .to_string();
    assert!(
        creator_id.starts_with("ctr_local"),
        "creator_id should follow the ctr_local… pattern, got {creator_id}"
    );

    // Persistence: the new creator is visible in the list endpoint (footer source of truth).
    let list_resp = ctx.server.get("/v1/daemon/creators").await;
    assert!(
        list_resp.status_code().is_success(),
        "list after create should succeed: {}",
        list_resp.text()
    );
    let list_body: Value = list_resp.json();
    let found = list_body["items"]
        .as_array()
        .expect("items should be an array")
        .iter()
        .any(|item| item["creator_id"] == creator_id.as_str());
    assert!(
        found,
        "created creator {creator_id} should appear in GET /v1/daemon/creators"
    );
}

/// Empty / whitespace-only `display_name` returns 400 `invalid_input` per the
/// canonical `NexusApiError` envelope (spec § Interfaces, Error contract).
#[tokio::test]
#[serial]
async fn post_creators_rejects_empty_display_name() {
    let ctx = test_ctx().await;
    let resp = ctx
        .server
        .post("/v1/daemon/creators")
        .json(&serde_json::json!({ "display_name": "   " }))
        .await;
    assert_eq!(resp.status_code(), 400);
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input");
    assert_eq!(body["error"]["details"]["field"], "display_name");
}

/// Display-name length cap mirrors `patch_creator` (256 chars by char count).
#[tokio::test]
#[serial]
async fn post_creators_rejects_oversize_display_name() {
    let ctx = test_ctx().await;
    let too_long = "🎨".repeat(257);
    let resp = ctx
        .server
        .post("/v1/daemon/creators")
        .json(&serde_json::json!({ "display_name": too_long }))
        .await;
    assert_eq!(resp.status_code(), 400);
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input");
    assert_eq!(body["error"]["details"]["field"], "display_name");
}
