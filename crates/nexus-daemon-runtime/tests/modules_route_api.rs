//! Compute modules route regression tests (orch-load-404 / R-HOTFIX-404-PARAM-SYNTAX).
//!
//! Verifies `create_router` path matching for Tier-1
//! `GET /v1/daemon/compute/modules` and
//! `GET /v1/daemon/compute/modules/:module_id` — not handler-only unit tests.

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
async fn list_modules_hits_handler_not_framework_404() {
    let (_tmp, server) = test_server().await;
    let resp = server.get("/v1/daemon/compute/modules").await;
    assert_ne!(
        resp.status_code(),
        404,
        "framework 404 — route not registered; body={}",
        resp.text()
    );
    assert!(
        resp.status_code().is_success(),
        "GET modules list: {} body={}",
        resp.status_code(),
        resp.text()
    );
    let body: Value = resp.json();
    assert!(body.get("items").is_some());
    assert!(body.get("has_more").is_some());
    let items = body["items"].as_array().expect("items array");
    assert!(
        items.iter().any(|m| m["module_id"] == "basic-combat"),
        "basic-combat should appear in list: {items:?}"
    );
}

#[tokio::test]
#[serial]
async fn get_module_by_id_hits_handler_not_framework_404() {
    let (_tmp, server) = test_server().await;
    let resp = server.get("/v1/daemon/compute/modules/basic-combat").await;
    assert_ne!(
        resp.status_code(),
        404,
        "framework 404 — route not registered; body={}",
        resp.text()
    );
    assert!(
        resp.status_code().is_success(),
        "GET module by id: {} body={}",
        resp.status_code(),
        resp.text()
    );
    let body: Value = resp.json();
    assert_eq!(body["module_id"], "basic-combat");
    assert_eq!(body["name"], "Basic Combat");
    assert_eq!(body["nexus_abi_version"], 1);
}

#[tokio::test]
#[serial]
async fn get_module_unknown_returns_handler_json_404_not_empty_body() {
    let (_tmp, server) = test_server().await;
    let resp = server
        .get("/v1/daemon/compute/modules/definitely-missing-module-id")
        .await;
    assert_eq!(resp.status_code(), 404);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "not_found");
}
