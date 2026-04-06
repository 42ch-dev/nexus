//! Local API — HTTP JSON endpoints for CLI communication
//!
//! Endpoints:
//! - GET  /v1/local/runtime/health   — Health check
//! - GET  /v1/local/runtime/status   — Runtime status
//! - GET  /v1/local/workspace        — Workspace info
//! - POST /v1/local/workspace/init   — Initialize workspace
//! - POST /v1/local/context/assemble — Context assembly (placeholder)
//! - GET  /v1/local/auth/status      — Auth status
//! - GET  /v1/local/creators         — List creators
//! - GET  /v1/local/manuscript       — Manuscript status
//! - GET  /v1/local/references       — List reference sources

pub mod handlers;

use crate::workspace::WorkspaceState;
use axum::{
    routing::{get, post},
    Router,
};
use tower_http::cors::CorsLayer;

/// Create the Local API router
pub fn create_router(state: WorkspaceState) -> Router {
    let runtime_routes = Router::new()
        .route("/v1/local/runtime/health", get(handlers::runtime::health))
        .route("/v1/local/runtime/status", get(handlers::runtime::status));

    let workspace_routes = Router::new()
        .route("/v1/local/workspace", get(handlers::workspace::info))
        .route(
            "/v1/local/workspace/init",
            post(handlers::workspace::init_workspace),
        );

    let auth_routes = Router::new().route("/v1/local/auth/status", get(handlers::auth::status));

    let creator_routes = Router::new().route("/v1/local/creators", get(handlers::creators::list));

    let manuscript_routes =
        Router::new().route("/v1/local/manuscript", get(handlers::manuscript::status));

    let reference_routes =
        Router::new().route("/v1/local/references", get(handlers::references::list));

    let context_routes = Router::new().route(
        "/v1/local/context/assemble",
        post(handlers::context::assemble),
    );

    Router::new()
        .merge(runtime_routes)
        .merge(workspace_routes)
        .merge(auth_routes)
        .merge(creator_routes)
        .merge(manuscript_routes)
        .merge(reference_routes)
        .merge(context_routes)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
