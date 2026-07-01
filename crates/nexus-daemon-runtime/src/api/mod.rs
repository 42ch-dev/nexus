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
pub mod pagination;
pub mod path_guard;
pub mod runtime_lock;
pub mod sort;

use crate::api::auth_middleware::DaemonApiConfig;
#[cfg(not(debug_assertions))]
use crate::static_assets;
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
        .route(
            "/v1/local/presets/{id}",
            get(handlers::preset_management::get_preset)
                .patch(handlers::preset_management::update_preset)
                .delete(handlers::preset_management::delete_preset),
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
        // DF-31 skeleton: workspace session open/commit
        .route(
            "/v1/local/workspace/open",
            post(handlers::workspace::open_workspace),
        )
        .route(
            "/v1/local/workspace/commit",
            post(handlers::workspace::commit_workspace),
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

/// Narrative read surface routes — SQLite-backed, read-only (V1.26+).
///
/// Minimal read-only daemon routes backed by `NarrativeGateway` with
/// `SQLite` persistent stores (V1.26 local persistence). Distinct from
/// the work-scope `/v1/local/kb/*` file-index routes.
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

/// Strategy canvas write-boundary routes (V1.71 Track A).
///
/// These three `POST` endpoints mutate a local strategy preset bundle
/// (`preset.yaml` + optional prompt template files) and enforce revision
/// consistency via the YAML `revision:` field.
fn strategy_routes() -> Router<WorkspaceState> {
    Router::new()
        .route(
            "/v1/local/strategies/{strategy_id}/states/{state_id}/patch",
            post(handlers::strategy::patch_state),
        )
        .route(
            "/v1/local/strategies/{strategy_id}/transitions/patch",
            post(handlers::strategy::patch_transition),
        )
        .route(
            "/v1/local/strategies/{strategy_id}/states/{state_id}/prompt/patch",
            post(handlers::strategy::patch_prompt_template),
        )
}

/// Memory routes (sync routes removed in V1.21 — cloud-sync is CLI-only).
///
/// V1.33 P4: Added `POST /v1/local/memory/review` and
/// `GET /v1/local/memory/fragments` to close the review/fragment loop.
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
        // Memory review pipeline (V1.33 P4)
        .route("/v1/local/memory/review", post(handlers::memory::review))
        // Memory fragments (V1.33 P4)
        .route(
            "/v1/local/memory/fragments",
            get(handlers::memory::fragments),
        )
        // Creator-SOUL narrative (V1.81)
        .route(
            "/v1/local/memory/soul/reflect",
            post(handlers::memory::reflect_soul),
        )
}

/// Canvas Outline+Timeline write-boundary routes (V1.72 Track A).
///
/// Kept separate from `works_routes` so the chapter `/{n}/patch` route can be
/// registered before `/{n}` and the overall works registry stays under the
/// clippy line threshold.
fn canvas_outline_routes() -> Router<WorkspaceState> {
    Router::new()
        .route(
            "/v1/local/works/{work_id}/outline",
            get(handlers::outline::get_work_outline),
        )
        .route(
            "/v1/local/works/{work_id}/outline/patch",
            post(handlers::outline::patch_outline_structure),
        )
        .route(
            "/v1/local/works/{work_id}/chapters/{n}/patch",
            post(handlers::outline::patch_outline_chapter),
        )
        .route(
            "/v1/local/works/{work_id}/timeline/patch",
            post(handlers::outline::patch_timeline_event),
        )
}

/// Canvas World KB write-boundary routes (V1.73 Track A).
///
/// World KB routes under `/v1/local/worlds/{world_id}/kb/*`. Per-row OCC on
/// `kb_key_blocks.revision` (entity edits), `kb_extract_jobs.version`
/// (promotion), and `kb_relationships.revision` (relationship edits).
fn world_kb_routes() -> Router<WorkspaceState> {
    Router::new()
        .route(
            "/v1/local/worlds/{world_id}/kb/patch-entity",
            post(handlers::world_kb::patch_entity),
        )
        .route(
            "/v1/local/worlds/{world_id}/kb/patch-relationship",
            post(handlers::world_kb::patch_relationship),
        )
        .route(
            "/v1/local/worlds/{world_id}/kb/promote-candidate",
            post(handlers::world_kb::promote_candidate),
        )
        .route(
            "/v1/local/worlds/{world_id}/kb/graph",
            get(handlers::world_kb::get_graph),
        )
        .route(
            "/v1/local/worlds/{world_id}/kb/candidates",
            get(handlers::world_kb::get_candidates),
        )
}

/// Works routes — Work CRUD + inspiration + reconcile-chapters (V1.33 §7.2, V1.36 §8).
///
/// Also includes findings sub-routes (V1.39 P1) merged into the same router
/// to avoid axum 0.7 path-param conflict across `.merge()` boundaries.
fn works_routes() -> Router<WorkspaceState> {
    Router::new()
        .route(
            "/v1/local/works",
            post(handlers::works::create_work).get(handlers::works::list_works),
        )
        // P0 pool route (DF-60 §5.3)
        .route(
            "/v1/local/works/pool",
            post(handlers::works::set_pool_active).get(handlers::works::list_pool),
        )
        // P1 selection pool routes (DF-61)
        .route(
            "/v1/local/works/pool/promote",
            post(handlers::works::promote_pool_entry),
        )
        .route(
            "/v1/local/works/pool/archive",
            post(handlers::works::archive_pool_entry_handler),
        )
        // P1 inspiration pool routes (DF-61)
        .route(
            "/v1/local/works/pool/inspiration",
            post(handlers::works::add_inspiration).get(handlers::works::list_inspiration),
        )
        .route(
            "/v1/local/works/pool/inspiration/promote",
            post(handlers::works::promote_inspiration_handler),
        )
        .route(
            "/v1/local/works/pool/inspiration/archive",
            post(handlers::works::archive_inspiration_handler),
        )
        .route(
            "/v1/local/works/{work_id}",
            get(handlers::works::get_work).patch(handlers::works::patch_work),
        )
        // ── Canvas Outline+Timeline routes (V1.72) ─────────────────────────
        .merge(canvas_outline_routes())
        .route(
            "/v1/local/works/{work_id}/inspiration",
            post(handlers::works::append_inspiration),
        )
        .route(
            "/v1/local/works/{work_id}/completion-lock/release",
            post(handlers::works::release_completion_lock_handler),
        )
        .route(
            "/v1/local/works/{work_id}/reconcile-chapters",
            post(handlers::works::reconcile_chapters),
        )
        // ── Chapter content sub-routes (V1.65 P0) ────────────────────────
        // Nest chapter routes under /v1/local/works/{work_id}/chapters so the
        // work_id prefix is shared and future Works sub-resources cannot
        // accidentally interleave with chapter paths (qc1 S-5).
        .nest(
            "/v1/local/works/{work_id}/chapters",
            Router::new()
                .route("/", get(handlers::chapters::list_chapters))
                .route(
                    "/{n}",
                    get(handlers::chapters::get_chapter).patch(handlers::chapters::patch_chapter),
                )
                .route("/{n}/outline", get(handlers::chapters::get_chapter_outline))
                .route("/{n}/body", get(handlers::chapters::get_chapter_body)),
        )
        // ── Findings sub-routes (V1.39 P1) ───────────────────────────
        .route(
            "/v1/local/works/{work_id}/findings",
            post(handlers::findings::create_finding_handler)
                .get(handlers::findings::list_findings_handler),
        )
        .route(
            "/v1/local/works/{work_id}/findings/from-review",
            post(handlers::findings::create_from_review_handler),
        )
        .route(
            "/v1/local/works/{work_id}/findings/{finding_id}",
            get(handlers::findings::get_finding_handler)
                .patch(handlers::findings::update_finding_handler)
                .delete(handlers::findings::delete_finding_handler),
        )
        // ── Stale findings banner endpoint (V1.39 P4 T3) ─────────────
        .route(
            "/v1/local/findings/stale",
            get(handlers::findings::list_stale_findings_handler),
        )
        // ── Retention prune endpoint (V1.49 P3, quality-loop §9.4) ───
        .route(
            "/v1/local/findings/prune",
            post(handlers::findings::prune_findings_handler),
        )
        // ── Creator-scoped finding lookup (V1.48 P2 — accept path) ────
        .route(
            "/v1/local/findings/{finding_id}",
            get(handlers::findings::get_finding_creator_scoped_handler),
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
        .merge(works_routes())
        .merge(narrative_routes())
        .merge(strategy_routes())
        .merge(world_kb_routes())
        // Legacy creators list & references
        .route("/v1/local/creators", get(handlers::creators::list))
        .route("/v1/local/references", get(handlers::references::list))
        .route(
            "/v1/local/references/:reference_id",
            get(handlers::references::get),
        )
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

    // ── Top-level router ────────────────────────────────────────────
    // Explicit /v1/local/* + agent-host routes take priority over the SPA
    // fallback. The SPA shell carries no data; all data access is through
    // the protected /v1/local/* routes.
    let router = Router::new().merge(runtime_routes).merge(protected_routes);

    // SPA fallback is **release-only**: serves the embedded Web UI at
    // unmatched non-API paths. Excluded in debug/test builds so unmatched
    // paths return the framework 404 (avoids masking API routing bugs in
    // tests; dev uses the Vite dev-server proxy, never the daemon's `/`).
    #[cfg(not(debug_assertions))]
    let router = router.fallback(static_assets::serve_embedded_app);

    router
        .layer(CorsLayer::permissive())
        // Request ID middleware: runs on ALL requests (before auth),
        // so error responses from auth middleware also include request_id.
        .route_layer(axum_mw::from_fn(middleware::attach_request_id))
        .with_state(state)
}
