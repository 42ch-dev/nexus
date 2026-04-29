//! Integration test: engine wired at startup produces _system.maintenance session.
//!
//! This test verifies that when the daemon wires up the engine with `system_preset`,
//! the _system.maintenance session appears in the sessions list.
//!
//! We don't boot the full daemon (which would bind ports) but instead replicate
//! the startup wiring logic in a test harness.

use axum::body::Body;
use axum::http::Request;
use nexus_orchestration::{CapabilityRegistry, GraphFlowEngine, OrchestrationEngine};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn engine_started_with_system_preset_session_appears() {
    // Replicate the daemon startup wiring.
    let (tmp, nexus_home, db_path) = nexus42d::test_utils::create_test_workspace().await;
    let mut state =
        nexus42d::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Create engine with SQLite storage (same as main.rs).
    let db_pool: sqlx::SqlitePool = state.pool().clone();
    let storage = Arc::new(
        nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(std::sync::Arc::new(
            db_pool,
        )),
    );
    let registry = Arc::new(CapabilityRegistry::with_builtins());
    let concrete_engine = GraphFlowEngine::new_with_storage(storage, registry.clone());

    // Start _system.maintenance session (same as main.rs).
    let sys_graph = nexus_orchestration::system_preset::build(registry.clone());
    concrete_engine
        .start_session("_system.maintenance", sys_graph)
        .await
        .expect("start _system.maintenance");

    let engine: Arc<dyn OrchestrationEngine> = Arc::new(concrete_engine);
    state.set_engine(engine);
    state.set_capability_registry(registry);

    // Keep tmp alive.
    std::mem::forget(tmp);

    // Hit the sessions endpoint via Axum.
    let app = nexus42d::api::create_router(state);
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
    let sessions = v["sessions"].as_array().unwrap();

    // Verify _system.maintenance session is present.
    let found = sessions
        .iter()
        .any(|s| s["presetId"] == "_system.maintenance");
    assert!(
        found,
        "expected _system.maintenance in sessions: {sessions:?}"
    );
}
