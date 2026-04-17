//! Local API — HTTP JSON endpoints for CLI communication
//!
//! Endpoints:
//! - GET  /v1/local/runtime/health   — Health check
//! - GET  /v1/local/runtime/status   — Runtime status
//! - GET  /v1/local/daemon/status   — Daemon lifecycle snapshot (minimal; see knowledge doc)
//! - GET  /v1/local/monitoring/pool  — Database pool status (QC-W3)
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
//! - POST /v1/local/publish/story — Publish manuscript story (platform proxy)
//! - POST /v1/local/publish/history — Publish history (platform proxy)
//! - GET  /v1/local/sync/status      — Sync status
//! - POST /v1/local/acp/tool/execute — ACP tool execution (daemon-mediated)
//! - GET  /v1/local/acp/sessions     — List ACP sessions
//! - DELETE /v1/local/acp/sessions/{id} — Delete an ACP session
//! - POST /v1/local/memory/pending-review — Create pending review entry (session-end capture)
//! - GET  /v1/local/memory/pending-review — List pending reviews for creator
//! - GET  /v1/local/memory/pending-review/count — Count pending reviews
//! - DELETE /v1/local/memory/pending-review/{id} — Delete pending review

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
/// - runtime health & status, daemon lifecycle snapshot (`/v1/local/daemon/status`)
/// - workspace info & init
/// - auth status, device authorization, token exchange, logout
///
/// **Auth-guarded routes** (require valid Bearer token):
/// - creators, manuscript, references, context/assemble
pub fn create_router(state: WorkspaceState) -> Router {
    let runtime_routes = Router::new()
        .route("/v1/local/runtime/health", get(handlers::runtime::health))
        .route("/v1/local/runtime/status", get(handlers::runtime::status))
        .route(
            "/v1/local/daemon/status",
            get(handlers::runtime::daemon_status),
        );

    let monitoring_routes = Router::new().route(
        "/v1/local/monitoring/pool",
        get(handlers::monitoring::pool_status),
    );

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

    // Sync routes (unguarded — status and replay can checked without auth)
    // Push and resolve are also available via daemon (auth is for platform communication)
    let sync_routes = Router::new()
        .route("/v1/local/sync/status", get(handlers::sync::status))
        .route("/v1/local/sync/push", post(handlers::sync::push))
        .route("/v1/local/sync/pull", post(handlers::sync::pull))
        .route("/v1/local/sync/resolve", post(handlers::sync::resolve))
        .route("/v1/local/sync/replay", get(handlers::sync::replay));

    let world_routes = Router::new()
        .route("/v1/local/world/fork", post(handlers::world::fork))
        .route("/v1/local/world/snapshot", post(handlers::world::snapshot));

    let explore_routes = Router::new()
        .route("/v1/local/explore/browse", post(handlers::explore::browse))
        .route("/v1/local/explore/search", post(handlers::explore::search));

    let publish_routes = Router::new()
        .route("/v1/local/publish/story", post(handlers::publish::story))
        .route(
            "/v1/local/publish/history",
            post(handlers::publish::history),
        );

    // ACP tool execution routes (unguarded — workspace validation in handler)
    let acp_routes = Router::new().route(
        "/v1/local/acp/tool/execute",
        post(handlers::acp::tool_execute),
    );

    // ACP session management routes (unguarded — session data in SQLite)
    let session_routes = Router::new()
        .route(
            "/v1/local/acp/sessions",
            get(handlers::sessions::list_sessions),
        )
        .route(
            "/v1/local/acp/sessions/{id}",
            axum::routing::delete(handlers::sessions::delete_session),
        );

    // Memory pending review routes (unguarded — for session-end capture)
    let memory_routes = Router::new()
        .route(
            "/v1/local/memory/pending-review",
            post(handlers::memory::create_pending_review),
        )
        .route(
            "/v1/local/memory/pending-review",
            get(handlers::memory::list_pending_reviews),
        )
        .route(
            "/v1/local/memory/pending-review/count",
            get(handlers::memory::count_pending_reviews),
        )
        .route(
            "/v1/local/memory/pending-review/{id}",
            axum::routing::delete(handlers::memory::delete_pending_review),
        );

    // Orchestration engine-session routes (unguarded — local-only API)
    let orchestration_routes = Router::new()
        .route(
            "/v1/local/orchestration/sessions",
            get(handlers::orchestration::sessions::list_sessions),
        )
        .route(
            "/v1/local/orchestration/sessions/{session_id}",
            get(handlers::orchestration::sessions::get_session),
        )
        .route(
            "/v1/local/orchestration/sessions/{session_id}/signal",
            post(handlers::orchestration::sessions::signal_session),
        )
        .route(
            "/v1/local/orchestration/capabilities",
            get(handlers::orchestration::capabilities::list_capabilities),
        )
        .route(
            "/v1/local/orchestration/presets",
            get(handlers::orchestration::presets::list_presets),
        );

    Router::new()
        .merge(runtime_routes)
        .merge(monitoring_routes)
        .merge(workspace_routes)
        .merge(auth_routes)
        .merge(creator_routes)
        .merge(manuscript_routes)
        .merge(reference_routes)
        .merge(context_routes)
        .merge(sync_routes)
        .merge(world_routes)
        .merge(explore_routes)
        .merge(publish_routes)
        .merge(acp_routes)
        .merge(session_routes)
        .merge(memory_routes)
        .merge(orchestration_routes)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
