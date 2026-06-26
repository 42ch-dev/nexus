//! F-F1 sort-contract integration tests for in-memory list endpoints.
//!
//! Covers sessions and capabilities, which keep in-memory sorting by design
//! (bounded sets) per V1.67 P0 fix-wave.

#![allow(clippy::unwrap_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_contracts::local::orchestration::http::CreateSessionRequest;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::{AuthMode, DaemonApiConfig};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::{GraphFlowEngine, OrchestrationEngine};
use serde_json::Value;
use std::sync::Arc;

struct TestCtx {
    server: TestServer,
}

async fn sessions_ctx() -> TestCtx {
    let (tmp, nexus_home, db_path) =
        nexus_daemon_runtime::test_utils::create_test_workspace().await;

    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let registry = Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
    let engine = Arc::new(GraphFlowEngine::new_with_storage(storage, registry.clone()));
    state.set_engine(engine as Arc<dyn OrchestrationEngine>);
    state.set_capability_registry(registry);

    std::mem::forget(tmp);

    let auth_config = DaemonApiConfig {
        api_key: None,
        auth_mode: AuthMode::KeylessLocalhost,
    };
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    TestCtx { server }
}

async fn create_session(server: &TestServer, creator_id: &str, preset_id: &str) -> String {
    let req = CreateSessionRequest {
        creator_id: creator_id.to_string(),
        preset_id: preset_id.to_string(),
        seed: None,
    };
    let resp = server
        .post("/v1/local/orchestration/sessions")
        .json(&req)
        .await;
    resp.assert_status(StatusCode::CREATED);
    let body: Value = resp.json();
    body["sessionId"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn sessions_list_sort_by_preset_id_ascending() {
    let ctx = sessions_ctx().await;
    let sid_a = create_session(&ctx.server, "ctr_session", "memory-augmented").await;
    let sid_b = create_session(&ctx.server, "ctr_session", "novel-writing").await;

    let resp = ctx
        .server
        .get("/v1/local/orchestration/sessions?creator_id=ctr_session&sort=preset_id")
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    let items = body["items"].as_array().unwrap();
    let ids: Vec<String> = items
        .iter()
        .map(|i| i["sessionId"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, vec![sid_a, sid_b]);
}

#[tokio::test]
async fn sessions_list_invalid_sort_key_returns_session_sort_invalid() {
    let ctx = sessions_ctx().await;
    let _ = create_session(&ctx.server, "ctr_session", "memory-augmented").await;

    let resp = ctx
        .server
        .get("/v1/local/orchestration/sessions?creator_id=ctr_session&sort=unknown_key")
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "session_sort_invalid");
}

#[tokio::test]
async fn capabilities_list_sort_by_name() {
    let ctx = sessions_ctx().await;

    let resp = ctx
        .server
        .get("/v1/local/orchestration/capabilities?sort=name")
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    let items = body["items"].as_array().unwrap();
    assert!(
        !items.is_empty(),
        "capabilities registry should not be empty"
    );

    let names: Vec<String> = items
        .iter()
        .map(|i| i["name"].as_str().unwrap().to_string())
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[tokio::test]
async fn capabilities_list_invalid_sort_key_returns_capability_sort_invalid() {
    let ctx = sessions_ctx().await;

    let resp = ctx
        .server
        .get("/v1/local/orchestration/capabilities?sort=unknown_key")
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "capability_sort_invalid");
}
