//! Test support module for integration tests.
//!
//! Provides helpers for building Axum app instances wired to ephemeral
//! orchestration engines (in-memory storage, no DB needed for smoke tests).
//!
//! Gated behind `#[cfg(test)]` because it uses dev-dependencies.

use crate::api;
use crate::workspace::WorkspaceState;
use axum::Router;
use nexus_orchestration::{GraphFlowEngine, OrchestrationEngine};
use std::sync::Arc;

/// Build an Axum app with an ephemeral in-memory orchestration engine.
///
/// Suitable for HTTP smoke tests that don't need a real DB or daemon
/// lifecycle. The engine starts with no active sessions.
pub async fn axum_app_with_ephemeral_engine() -> Router {
    let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;

    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Wire an ephemeral engine with in-memory storage.
    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let registry = Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
    let engine = Arc::new(GraphFlowEngine::new_with_storage(storage, registry.clone()));
    state.set_engine(engine as Arc<dyn OrchestrationEngine>);

    // Wire a capability registry.
    state.set_capability_registry(registry);

    // Keep tmp alive for the duration of the test.
    // The caller receives the Router; tmp is dropped when this function returns,
    // but the DB pool inside WorkspaceState keeps the file alive.
    // We need to leak tmp to avoid early drop — acceptable for tests.
    std::mem::forget(tmp);

    // Use keyless-localhost mode for tests (loopback connections accepted without key).
    let auth_config = api::auth_middleware::DaemonApiConfig::keyless();
    api::create_router(state, auth_config)
}
