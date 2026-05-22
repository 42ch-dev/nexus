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
    routing::{delete, get, post},
    Router,
};
use tower_http::cors::CorsLayer;

/// Agent Host routes (V1.20 Batch 3).
fn agent_host_routes() -> Router<WorkspaceState> {
    Router::new()
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
            "/v1/local/agent-host/sessions/{session_id}",
            get(handlers::agent_host::get_session).delete(handlers::agent_host::shutdown_session),
        )
        .route(
            "/v1/local/agent-host/sessions/{session_id}/operations",
            post(handlers::agent_host::execute_operation),
        )
        .route(
            "/v1/local/agent-host/operations/{operation_id}:cancel",
            post(handlers::agent_host::cancel_operation),
        )
        .route(
            "/v1/local/agent-host/sessions/{session_id}/events",
            get(handlers::agent_host::session_events),
        )
}

/// Orchestration engine and schedule routes.
///
/// The CLI exercises these session-control routes through `daemon schedule`;
/// there is intentionally no separate direct daemon session-control CLI group.
fn orchestration_routes() -> Router<WorkspaceState> {
    Router::new()
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
            post(handlers::orchestration::presets::reload_preset),
        )
        // Schedule management routes (WS7)
        .route(
            "/v1/local/orchestration/schedules",
            post(handlers::orchestration::schedules::add_schedule)
                .get(handlers::orchestration::schedules::list_schedules),
        )
        .route(
            "/v1/local/orchestration/schedules/{schedule_id}",
            get(handlers::orchestration::schedules::inspect_schedule)
                .delete(handlers::orchestration::schedules::delete_schedule),
        )
        .route(
            "/v1/local/orchestration/schedules/{schedule_id}/core-context",
            axum::routing::patch(handlers::orchestration::schedules::edit_core_context)
                .get(handlers::orchestration::schedules::get_core_context),
        )
        .route(
            "/v1/local/orchestration/schedules/{schedule_id}/core-context-history",
            get(handlers::orchestration::schedules::get_core_context_history),
        )
        .route(
            "/v1/local/orchestration/schedules/{schedule_id}/signal",
            post(handlers::orchestration::schedules::signal_schedule),
        )
}

/// Creator management routes — local-only (registration proxy removed in V1.21).
///
/// Registration now lives in the CLI via `nexus-cloud-sync`.
/// The daemon only provides local creator status, active selection, and logout.
fn creator_routes() -> Router<WorkspaceState> {
    Router::new()
        .route(
            "/v1/local/creators/{creator_id}",
            get(handlers::creators::get_creator),
        )
        .route(
            "/v1/local/creators/{creator_id}:logout",
            post(handlers::creators::logout_creator),
        )
        .route(
            "/v1/local/creators/active",
            get(handlers::creators::get_active_creator).put(handlers::creators::set_active_creator),
        )
}

/// Preset management routes (V1.20 Batch 5, T34–T37).
fn preset_routes() -> Router<WorkspaceState> {
    Router::new()
        .route(
            "/v1/local/presets",
            get(handlers::preset_management::list_presets)
                .post(handlers::preset_management::scaffold_preset),
        )
        .route(
            "/v1/local/presets:validate",
            post(handlers::preset_management::validate_preset),
        )
        .route(
            "/v1/local/presets/{id}:reload",
            post(handlers::preset_management::reload_preset),
        )
}

/// Work-scope KB routes — local work file index only (V1.20 Batch 5, T39; KCA-003 C2).
///
/// These routes serve the CLI local work KB index under
/// `~/.nexus42/creators/<id>/workspaces/<slug>/kb/`. They do **not**
/// provide World KB (`nexus-kb`) or User knowledge (`nexus-knowledge`) access.
/// Full KB route redesign is deferred to a future iteration.
fn kb_routes() -> Router<WorkspaceState> {
    Router::new()
        .route(
            "/v1/local/kb/entries",
            get(handlers::kb::list_entries).post(handlers::kb::add_entry),
        )
        .route(
            "/v1/local/kb/entries/{entry_id}",
            get(handlers::kb::get_entry).delete(handlers::kb::delete_entry),
        )
}

/// Workspace management routes (V1.20 Batch 4, T21–T24) + legacy single-workspace.
fn workspace_routes() -> Router<WorkspaceState> {
    Router::new()
        .route("/v1/local/workspace", get(handlers::workspace::info))
        .route(
            "/v1/local/workspace/init",
            post(handlers::workspace::init_workspace),
        )
        .route(
            "/v1/local/workspaces",
            get(handlers::workspaces::list_workspaces).post(handlers::workspaces::create_workspace),
        )
        .route(
            "/v1/local/workspaces/active",
            get(handlers::workspaces::get_active_workspace)
                .put(handlers::workspaces::set_active_workspace),
        )
}

/// Narrative read surface routes — in-memory, read-only (V1.25 Theme C, C1.1).
///
/// Minimal read-only daemon routes backed by `NarrativeGateway` with
/// in-memory stores. Distinct from the work-scope `/v1/local/kb/*`
/// file-index routes. No persistence across daemon restarts.
fn narrative_routes() -> Router<WorkspaceState> {
    Router::new()
        .route(
            "/v1/local/narrative/worlds",
            get(handlers::narrative::list_worlds),
        )
        .route(
            "/v1/local/narrative/worlds/{world_id}",
            get(handlers::narrative::get_world),
        )
}

/// Memory routes (sync routes removed in V1.21 — cloud-sync is CLI-only).
fn memory_routes() -> Router<WorkspaceState> {
    Router::new()
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
            delete(handlers::memory::delete_pending_review),
        )
}
/// Create the Local API router
///
/// **Unguarded routes** (no auth, always reachable):
/// - runtime health, status, daemon lifecycle snapshot
///
/// **Protected routes** (behind `require_api_key` middleware):
/// - All other routes (workspace, creators, memory,
///   orchestration, agent-host, monitoring).
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
        // Workspace + Creator + Preset + KB + Memory (Batch 4–5 route groups)
        .merge(workspace_routes())
        .merge(creator_routes())
        .merge(preset_routes())
        .merge(kb_routes())
        .merge(memory_routes())
        .merge(narrative_routes())
        // Legacy creators list & references
        .route("/v1/local/creators", get(handlers::creators::list))
        .route("/v1/local/references", get(handlers::references::list))
        // ACP tool execution — internal route only (not public ACP routes)
        .route(
            "/v1/local/agent-host/internal/tool-executions",
            post(handlers::acp::tool_execute),
        )
        // Orchestration routes
        .merge(orchestration_routes())
        // ── Agent Host routes (V1.20 Batch 3) ─────────────────────────
        .merge(agent_host_routes())
        // Apply require_api_key middleware to all protected routes
        .route_layer(axum_mw::from_fn_with_state(
            auth_config,
            auth_middleware::require_api_key,
        ));

    Router::new()
        .merge(runtime_routes)
        .merge(protected_routes)
        .layer(CorsLayer::permissive())
        // Request ID middleware: runs on ALL requests (before auth),
        // so error responses from auth middleware also include request_id.
        .route_layer(axum_mw::from_fn(middleware::attach_request_id))
        .with_state(state)
}
