//! Sessions route regression tests (orch-load-404 / R-HOTFIX-404-PARAM-SYNTAX).
//!
//! Verifies `create_router` path matching for Tier-2
//! `GET /v1/daemon/orchestration/sessions` and
//! `GET /v1/daemon/orchestration/sessions/:session_id` — not handler-only unit tests.

#![allow(clippy::unwrap_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_contracts::local::orchestration::http::CreateSessionRequest;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::{GraphFlowEngine, OrchestrationEngine};
use serde_json::Value;
use serial_test::serial;
use std::sync::Arc;

struct EngineCtx {
    _tmp: test_utils::TestTempRoot,
    server: TestServer,
}

async fn test_server_with_engine() -> EngineCtx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let registry = Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
    let engine = Arc::new(GraphFlowEngine::new_with_storage(
        storage,
        nexus_orchestration::CapabilityRegistryHolder::with_registry(registry.clone()),
    ));
    state.set_engine(engine as Arc<dyn OrchestrationEngine>);
    state.set_capability_registry(
        nexus_orchestration::CapabilityRegistryHolder::with_registry(registry),
    );

    let auth_config = DaemonApiConfig::keyless();
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    EngineCtx { _tmp: tmp, server }
}

// axum_test's AutoFuture is not Send; this helper is awaited directly by #[tokio::test], never spawned
#[allow(clippy::future_not_send)]
async fn create_session(server: &TestServer, creator_id: &str, preset_id: &str) -> String {
    let req = CreateSessionRequest {
        creator_id: creator_id.to_string(),
        preset_id: preset_id.to_string(),
        seed: None,
    };
    let resp = server
        .post("/v1/daemon/orchestration/sessions")
        .json(&req)
        .await;
    resp.assert_status(StatusCode::CREATED);
    let body: Value = resp.json();
    body["sessionId"].as_str().unwrap().to_string()
}

#[tokio::test]
#[serial]
async fn list_sessions_hits_handler_not_framework_404() {
    let ctx = test_server_with_engine().await;
    let resp = ctx
        .server
        .get("/v1/daemon/orchestration/sessions?creator_id=test_creator")
        .await;
    assert_ne!(
        resp.status_code(),
        404,
        "framework 404 — route not registered; body={}",
        resp.text()
    );
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    assert!(body.get("items").is_some());
    assert!(body.get("pagination").is_some());
}

#[tokio::test]
#[serial]
async fn get_session_by_id_hits_handler_not_framework_404() {
    let ctx = test_server_with_engine().await;
    let session_id = create_session(&ctx.server, "test_creator", "novel-writing").await;

    let resp = ctx
        .server
        .get(&format!("/v1/daemon/orchestration/sessions/{session_id}"))
        .await;
    assert_ne!(
        resp.status_code(),
        404,
        "framework 404 — route not registered; body={}",
        resp.text()
    );
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(body["session"]["sessionId"], session_id);
    assert_eq!(body["session"]["presetId"], "novel-writing");
}

#[tokio::test]
#[serial]
async fn get_session_unknown_returns_handler_json_404_not_empty_body() {
    let ctx = test_server_with_engine().await;
    let resp = ctx
        .server
        .get("/v1/daemon/orchestration/sessions/definitely-missing-session-id")
        .await;
    assert_eq!(resp.status_code(), 404);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
#[serial]
async fn sessions_without_engine_returns_503_not_404() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let auth_config = DaemonApiConfig::keyless();
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");

    let resp = server
        .get("/v1/daemon/orchestration/sessions?creator_id=test_creator")
        .await;
    assert_ne!(
        resp.status_code(),
        404,
        "engine-absent must not surface as framework 404; body={}",
        resp.text()
    );
    resp.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "service_unavailable");

    std::mem::forget(tmp);
}

#[tokio::test]
#[serial]
async fn sessions_without_active_creator_returns_409_not_404() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    std::fs::write(nexus_home.join("config.toml"), "").expect("clear active creator config");

    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let registry = Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
    let engine = Arc::new(GraphFlowEngine::new_with_storage(
        storage,
        nexus_orchestration::CapabilityRegistryHolder::with_registry(registry.clone()),
    ));
    state.set_engine(engine as Arc<dyn OrchestrationEngine>);
    state.set_capability_registry(
        nexus_orchestration::CapabilityRegistryHolder::with_registry(registry),
    );

    let auth_config = DaemonApiConfig::keyless();
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");

    let resp = server
        .get("/v1/daemon/orchestration/sessions?creator_id=test_creator")
        .await;
    assert_ne!(
        resp.status_code(),
        404,
        "tier-2 guard must not surface as framework 404; body={}",
        resp.text()
    );
    resp.assert_status(StatusCode::CONFLICT);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "uninitialized");

    std::mem::forget(tmp);
}
