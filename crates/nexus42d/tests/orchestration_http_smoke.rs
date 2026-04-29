//! HTTP smoke tests for `/v1/local/orchestration/*` endpoints.
//!
//! Uses an ephemeral engine (in-memory storage, no DB needed).

use axum::body::Body;
use axum::http::Request;
use axum::Router;
use nexus_orchestration::{CapabilityRegistry, GraphFlowEngine, OrchestrationEngine};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

/// Build an Axum app with an ephemeral in-memory orchestration engine.
async fn axum_app_with_ephemeral_engine() -> Router {
    let (tmp, nexus_home, db_path) = nexus42d::test_utils::create_test_workspace().await;

    let mut state =
        nexus42d::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Wire an ephemeral engine with in-memory storage.
    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let registry = Arc::new(CapabilityRegistry::with_builtins());
    let engine: Arc<dyn OrchestrationEngine> =
        Arc::new(GraphFlowEngine::new_with_storage(storage, registry.clone()));
    state.set_engine(engine);

    // Wire a capability registry.
    state.set_capability_registry(registry);

    // Keep tmp alive — leak is acceptable in tests.
    std::mem::forget(tmp);

    nexus42d::api::create_router(state)
}

#[tokio::test]
async fn get_sessions_returns_empty_array_initially() {
    let app = axum_app_with_ephemeral_engine().await;
    let req = Request::builder()
        .method("GET")
        .uri("/v1/local/orchestration/sessions")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v["sessions"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_capabilities_returns_list() {
    let app = axum_app_with_ephemeral_engine().await;
    let req = Request::builder()
        .method("GET")
        .uri("/v1/local/orchestration/capabilities")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    let caps = v["capabilities"].as_array().unwrap();
    assert!(!caps.is_empty(), "should have at least one capability");
    assert!(
        caps.iter().map(|c| c["name"].as_str().expect("capability name should be string")).any(|x| x == "sync.pull"),
        "expected sync.pull in capabilities"
    );
}

#[tokio::test]
async fn get_presets_returns_system_maintenance() {
    let (tmp, nexus_home, db_path) = nexus42d::test_utils::create_test_workspace().await;

    // Create _system/maintenance/ directory with embedded preset YAML.
    let system_dir = nexus_home
        .join("presets")
        .join("_system")
        .join("maintenance");
    std::fs::create_dir_all(&system_dir).unwrap();
    std::fs::write(
        system_dir.join("preset.yaml"),
        nexus_orchestration::system_preset_dir::EMBEDDED_MAINTENANCE_YAML,
    )
    .unwrap();

    let mut state =
        nexus42d::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Wire an ephemeral engine with in-memory storage.
    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let registry = Arc::new(CapabilityRegistry::with_builtins());
    let engine: Arc<dyn OrchestrationEngine> =
        Arc::new(GraphFlowEngine::new_with_storage(storage, registry.clone()));
    state.set_engine(engine);

    // Wire a capability registry.
    state.set_capability_registry(registry);

    // Keep tmp alive — leak is acceptable in tests.
    std::mem::forget(tmp);

    let app = nexus42d::api::create_router(state);
    let req = Request::builder()
        .method("GET")
        .uri("/v1/local/orchestration/presets")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    let presets = v["presets"].as_array().unwrap();
    assert_eq!(presets.len(), 2);
    assert!(presets
        .iter()
        .any(|p| p.as_str().unwrap() == "_system.maintenance"));
    assert!(presets
        .iter()
        .any(|p| p.as_str().unwrap() == "novel-writing"));
}

#[tokio::test]
async fn post_signal_on_nonexistent_session_returns_404() {
    let app = axum_app_with_ephemeral_engine().await;
    let body = serde_json::json!({"signal": "pause"});
    let req = Request::builder()
        .method("POST")
        .uri("/v1/local/orchestration/sessions/nonexistent-id/signal")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 404);
}
