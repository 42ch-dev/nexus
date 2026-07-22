//! Preset by-id route regression tests (orch-load-404 / R-HOTFIX-404-PARAM-SYNTAX).
//!
//! Verifies `create_router` path matching for Tier-1
//! `GET/POST /v1/daemon/presets/:id` — not handler-only unit tests.

#![allow(clippy::unwrap_used)]

use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::Value;
use serial_test::serial;

async fn test_server() -> (test_utils::TestTempRoot, TestServer) {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let auth_config = DaemonApiConfig::keyless();
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    (tmp, server)
}

#[tokio::test]
#[serial]
async fn list_presets_hits_handler_not_framework_404() {
    let (_tmp, server) = test_server().await;
    let resp = server.get("/v1/daemon/presets").await;
    assert_ne!(
        resp.status_code(),
        404,
        "framework 404 — route not registered; body={}",
        resp.text()
    );
    assert!(
        resp.status_code().is_success(),
        "GET presets list: {} body={}",
        resp.status_code(),
        resp.text()
    );
    let body: Value = resp.json();
    assert!(body.get("embedded").is_some());
}

#[tokio::test]
#[serial]
async fn get_preset_by_id_hits_handler_not_framework_404() {
    let (_tmp, server) = test_server().await;
    let resp = server.get("/v1/daemon/presets/novel-writing").await;
    assert_ne!(
        resp.status_code(),
        404,
        "framework 404 — route not registered; body={}",
        resp.text()
    );
    assert!(
        resp.status_code().is_success(),
        "GET preset by id: {} body={}",
        resp.status_code(),
        resp.text()
    );
    let body: Value = resp.json();
    assert_eq!(body["id"], "novel-writing");
    assert_eq!(body["source"], "embedded");
}

#[tokio::test]
#[serial]
async fn get_preset_unknown_returns_handler_json_404_not_empty_body() {
    let (_tmp, server) = test_server().await;
    let resp = server
        .get("/v1/daemon/presets/definitely-missing-preset-id")
        .await;
    assert_eq!(resp.status_code(), 404);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
#[serial]
async fn reload_preset_by_id_hits_handler_not_framework_404() {
    let (_tmp, server) = test_server().await;
    let resp = server.post("/v1/daemon/presets/novel-writing:reload").await;
    assert_ne!(
        resp.status_code(),
        404,
        "framework 404 — route not registered; body={}",
        resp.text()
    );
    assert!(
        resp.status_code().is_success(),
        "POST preset reload: {} body={}",
        resp.status_code(),
        resp.text()
    );
    let body: Value = resp.json();
    assert_eq!(body["id"], "novel-writing");
    assert_eq!(body["reloaded"], true);
}

#[tokio::test]
#[serial]
async fn scaffold_then_get_user_preset_round_trip() {
    let (_tmp, server) = test_server().await;
    let scaffold = server
        .post("/v1/daemon/presets")
        .json(&serde_json::json!({ "name": "route-test-preset" }))
        .await;
    assert!(
        scaffold.status_code().is_success(),
        "scaffold preset: {} body={}",
        scaffold.status_code(),
        scaffold.text()
    );

    let resp = server.get("/v1/daemon/presets/route-test-preset").await;
    assert_ne!(
        resp.status_code(),
        404,
        "framework 404 after scaffold; body={}",
        resp.text()
    );
    assert!(
        resp.status_code().is_success(),
        "GET user preset: {} body={}",
        resp.status_code(),
        resp.text()
    );
    let body: Value = resp.json();
    assert_eq!(body["id"], "route-test-preset");
    assert_eq!(body["source"], "user");
}
