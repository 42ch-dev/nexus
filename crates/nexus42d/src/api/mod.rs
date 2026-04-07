//! Local API — HTTP JSON endpoints for CLI communication
//!
//! Endpoints:
//! - GET  /v1/local/runtime/health   — Health check
//! - GET  /v1/local/runtime/status   — Runtime status
//! - GET  /v1/local/workspace        — Workspace info
//! - POST /v1/local/workspace/init   — Initialize workspace
//! - GET  /v1/local/auth/status      — Auth status
//! - POST /v1/local/auth/device      — Start device authorization
//! - POST /v1/local/auth/token       — Exchange device code for tokens
//! - POST /v1/local/auth/logout      — Clear tokens (logout)
//! - GET  /v1/local/creators         — List creators (auth required)
//! - GET  /v1/local/manuscript       — Manuscript status (auth required)
//! - GET  /v1/local/references       — List reference sources (auth required)
//! - POST /v1/local/context/assemble — Context assembly (auth required)
//! - GET  /v1/local/sync/status      — Sync status

pub mod auth_middleware;
pub mod errors;
pub mod handlers;
pub mod middleware;

use crate::workspace::WorkspaceState;
use axum::{
    middleware as axum_mw,
    routing::{get, post},
    Router,
};
use tower_http::cors::CorsLayer;

/// Create the Local API router
///
/// **Unguarded routes** (no auth, no workspace init):
/// - runtime health & status
/// - workspace info & init
/// - auth status, device authorization, token exchange, logout
///
/// **Auth-guarded routes** (require valid Bearer token):
/// - creators, manuscript, references, context/assemble
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

    let auth_routes = Router::new()
        .route("/v1/local/auth/status", get(handlers::auth::status))
        .route(
            "/v1/local/auth/device",
            post(handlers::auth::device_authorization),
        )
        .route("/v1/local/auth/token", post(handlers::auth::exchange_token))
        .route("/v1/local/auth/logout", post(handlers::auth::logout));

    // Auth-guarded routes (require valid Bearer token)
    let creator_routes = Router::new()
        .route("/v1/local/creators", get(handlers::creators::list))
        .route_layer(axum_mw::from_fn_with_state(
            state.clone(),
            auth_middleware::require_auth,
        ));

    let manuscript_routes = Router::new()
        .route("/v1/local/manuscript", get(handlers::manuscript::status))
        .route_layer(axum_mw::from_fn_with_state(
            state.clone(),
            auth_middleware::require_auth,
        ));

    let reference_routes = Router::new()
        .route("/v1/local/references", get(handlers::references::list))
        .route_layer(axum_mw::from_fn_with_state(
            state.clone(),
            auth_middleware::require_auth,
        ));

    let context_routes = Router::new()
        .route(
            "/v1/local/context/assemble",
            post(handlers::context::assemble),
        )
        .route_layer(axum_mw::from_fn_with_state(
            state.clone(),
            auth_middleware::require_auth,
        ));

    // Sync routes (unguarded — can check status without auth)
    let sync_routes = Router::new().route("/v1/local/sync/status", get(handlers::sync::status));

    Router::new()
        .merge(runtime_routes)
        .merge(workspace_routes)
        .merge(auth_routes)
        .merge(creator_routes)
        .merge(manuscript_routes)
        .merge(reference_routes)
        .merge(context_routes)
        .merge(sync_routes)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
