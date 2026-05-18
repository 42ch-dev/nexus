//! Complex HTTP handlers with orchestration logic exceed line limits.
#![allow(clippy::too_many_lines)]
//! Local API — HTTP JSON endpoints for CLI communication
//!
//! # Route protection model (V1.20+)
//!
//! **Unguarded routes** (no auth even in keyed-all mode):
//! - `GET /v1/local/runtime/health` — minimal liveness check
//! - `GET /v1/local/runtime/status` — runtime diagnostic status
//! - `GET /v1/local/daemon/status` — daemon lifecycle snapshot
//!
//! All other routes are behind `require_api_key` middleware.
//! See [`auth_middleware::DaemonApiConfig`] for dual-mode startup.

pub mod auth_middleware;
pub mod errors;
pub mod handlers;
pub mod middleware;

use crate::api::auth_middleware::DaemonApiConfig;
use crate::workspace::WorkspaceState;
use axum::{
    middleware as axum_mw,
    routing::{get, post},
    Router,
};
use tower_http::cors::CorsLayer;

/// Create the Local API router
///
/// **Unguarded routes** (no auth, always reachable):
/// - runtime health, status, daemon lifecycle snapshot
///
/// **Protected routes** (behind `require_api_key` middleware):
/// - All other routes (workspace, creators, sync, ACP, memory,
///   orchestration, agent-host, monitoring, world, explore).
pub fn create_router(state: WorkspaceState, auth_config: DaemonApiConfig) -> Router {
    // --- Unguarded: runtime liveness / status (always accessible) ---
    let runtime_routes = Router::new()
        .route("/v1/local/runtime/health", get(handlers::runtime::health))
        .route("/v1/local/runtime/status", get(handlers::runtime::status))
        .route(
            "/v1/local/daemon/status",
            get(handlers::runtime::daemon_status),
        );

    // --- Protected: everything else behind require_api_key ---
    let protected_routes = Router::new()
        // Monitoring
        .route(
            "/v1/local/monitoring/pool",
            get(handlers::monitoring::pool_status),
        )
        // Workspace
        .route("/v1/local/workspace", get(handlers::workspace::info))
        .route(
            "/v1/local/workspace/init",
            post(handlers::workspace::init_workspace),
        )
        // Creators & references
        .route("/v1/local/creators", get(handlers::creators::list))
        .route("/v1/local/references", get(handlers::references::list))
        // Sync
        .route("/v1/local/sync/status", get(handlers::sync::status))
        .route("/v1/local/sync/push", post(handlers::sync::push))
        .route("/v1/local/sync/pull", post(handlers::sync::pull))
        .route("/v1/local/sync/resolve", post(handlers::sync::resolve))
        .route("/v1/local/sync/replay", get(handlers::sync::replay))
        // World (Batch 2 will remove these)
        .route("/v1/local/world/fork", post(handlers::world::fork))
        .route("/v1/local/world/snapshot", post(handlers::world::snapshot))
        // Explore (Batch 2 will remove these)
        .route("/v1/local/explore/browse", post(handlers::explore::browse))
        .route("/v1/local/explore/search", post(handlers::explore::search))
        // ACP tool execution
        .route(
            "/v1/local/acp/tool/execute",
            post(handlers::acp::tool_execute),
        )
        // ACP session management
        .route(
            "/v1/local/acp/sessions",
            get(handlers::sessions::list_sessions),
        )
        .route(
            "/v1/local/acp/sessions/{id}",
            axum::routing::delete(handlers::sessions::delete_session),
        )
        // Memory pending review
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
        )
        // Orchestration engine-session routes
        .route(
            "/v1/local/orchestration/sessions",
            get(handlers::orchestration::sessions::list_sessions)
                .post(handlers::orchestration::sessions::create_session),
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
        )
        .route(
            "/v1/local/orchestration/presets/{id}:reload",
            axum::routing::post(handlers::orchestration::presets::reload_preset),
        )
        // Schedule management routes (WS7)
        .route(
            "/v1/local/orchestration/schedules",
            axum::routing::post(handlers::orchestration::schedules::add_schedule)
                .get(handlers::orchestration::schedules::list_schedules),
        )
        .route(
            "/v1/local/orchestration/schedules/{schedule_id}",
            axum::routing::get(handlers::orchestration::schedules::inspect_schedule)
                .delete(handlers::orchestration::schedules::delete_schedule),
        )
        .route(
            "/v1/local/orchestration/schedules/{schedule_id}/core-context",
            axum::routing::patch(handlers::orchestration::schedules::edit_core_context)
                .get(handlers::orchestration::schedules::get_core_context),
        )
        .route(
            "/v1/local/orchestration/schedules/{schedule_id}/core-context-history",
            axum::routing::get(handlers::orchestration::schedules::get_core_context_history),
        )
        .route(
            "/v1/local/orchestration/schedules/{schedule_id}/signal",
            axum::routing::post(handlers::orchestration::schedules::signal_schedule),
        )
        // Agent Host routes
        .route(
            "/v1/local/agent-host/health",
            get(handlers::agent_host::health),
        )
        .route(
            "/v1/local/agent-host/providers",
            get(handlers::agent_host::list_providers),
        )
        .route(
            "/v1/local/agent-host/sessions",
            post(handlers::agent_host::create_session).get(handlers::agent_host::list_sessions),
        )
        .route(
            "/v1/local/agent-host/sessions/{id}",
            axum::routing::delete(handlers::agent_host::shutdown_session),
        )
        // Apply require_api_key middleware to all protected routes
        .route_layer(axum_mw::from_fn_with_state(
            auth_config,
            auth_middleware::require_api_key,
        ));

    Router::new()
        .merge(runtime_routes)
        .merge(protected_routes)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
