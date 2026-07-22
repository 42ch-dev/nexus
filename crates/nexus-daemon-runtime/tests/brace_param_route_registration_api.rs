//! Brace-form → colon-form route registration regression tests
//! (V1.133 P0 — R-HOTFIX-404-PARAM-SYNTAX).
//!
//! Proves that each route group converted in the V1.133 brace-param sweep is
//! actually registered in the router (returns a handler-level response, not
//! the framework 404 with empty body that the pre-fix brace-form produced).
//!
//! Each test issues a request with a dummy ID to a representative route from
//! one of the 8 converted groups that previously had no TestServer-level
//! coverage (the agent-host `cancel` edge case is already covered by the
//! handler-level tests in `agent_host.rs`). The goal is route-matching proof,
//! not handler success: any non-framework-404 response (handler 404 with JSON
//! body, 400, 409 tier-2 guard, 500) proves the route is registered.
//!
//! Why this is sufficient: framework 404 (axum default fallback) returns a
//! 404 status with an **empty body** and no `Content-Type` header. Handler
//! responses always include a JSON body (`NexusApiError` envelope on error,
//! or a success body). Asserting the response is not a 404-with-empty-body
//! therefore distinguishes "route not registered" from "handler ran".

#![allow(clippy::unwrap_used)]

use axum::http::StatusCode;
use axum_test::{TestResponse, TestServer};
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use serial_test::serial;

struct RouteCtx {
    // Keep the temp dir alive for the whole test (Drop deletes it).
    _tmp: test_utils::TestTempRoot,
    server: TestServer,
}

/// Minimal TestServer with the full router — no engine seeded.
/// Route-matching proof does not require handler success, so a bare workspace
/// state is enough: tier-2 guard or handler-level errors both prove the route
/// is registered.
async fn route_ctx() -> RouteCtx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let auth_config = DaemonApiConfig::keyless();
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    RouteCtx { _tmp: tmp, server }
}

/// Assert the response is NOT a framework 404 (empty body — the symptom of the
/// brace-form route bug). Handler-level responses — including handler 404 with
/// a JSON `NexusApiError` body, 400 invalid-input, 409 tier-2 guard, 500 — all
/// prove the route matched.
fn assert_not_framework_404(resp: &TestResponse, route: &str) {
    let status = resp.status_code();
    let body = resp.text();
    let is_framework_404 = status == StatusCode::NOT_FOUND && body.is_empty();
    assert!(
        !is_framework_404,
        "framework 404 — route not registered: {route}\n\
         status={status}, body='{body}'"
    );
}

// ── Group 1: orchestration schedules ──────────────────────────────────────

#[tokio::test]
#[serial]
async fn schedule_by_id_hits_handler_not_framework_404() {
    let ctx = route_ctx().await;
    let route = "/v1/daemon/orchestration/schedules/dummy-id";
    let resp = ctx.server.get(route).await;
    assert_not_framework_404(&resp, route);
}

// ── Group 2: KB entries ───────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn kb_entry_by_id_hits_handler_not_framework_404() {
    let ctx = route_ctx().await;
    let route = "/v1/daemon/kb/entries/dummy-id";
    let resp = ctx.server.get(route).await;
    assert_not_framework_404(&resp, route);
}

// ── Group 3: narrative worlds ─────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn narrative_world_by_id_hits_handler_not_framework_404() {
    let ctx = route_ctx().await;
    let route = "/v1/daemon/narrative/worlds/dummy-id";
    let resp = ctx.server.get(route).await;
    assert_not_framework_404(&resp, route);
}

// ── Group 4: reading annotations ──────────────────────────────────────────
// `:annotation_id` route only chains PATCH/DELETE; use DELETE to exercise the
// route without a request body.

#[tokio::test]
#[serial]
async fn reading_annotation_by_id_hits_handler_not_framework_404() {
    let ctx = route_ctx().await;
    let route = "/v1/daemon/reading/annotations/dummy-id";
    let resp = ctx.server.delete(route).await;
    assert_not_framework_404(&resp, route);
}

// ── Group 5: memory pending-review ────────────────────────────────────────
// `:id` route is DELETE-only.

#[tokio::test]
#[serial]
async fn memory_pending_review_by_id_hits_handler_not_framework_404() {
    let ctx = route_ctx().await;
    let route = "/v1/daemon/memory/pending-review/dummy-id";
    let resp = ctx.server.delete(route).await;
    assert_not_framework_404(&resp, route);
}

// ── Group 6: works ────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn work_by_id_hits_handler_not_framework_404() {
    let ctx = route_ctx().await;
    let route = "/v1/daemon/works/dummy-id";
    let resp = ctx.server.get(route).await;
    assert_not_framework_404(&resp, route);
}

// ── Group 7: worlds (hard-delete surface) ─────────────────────────────────
// `/v1/daemon/worlds/:world_id` is DELETE-only (distinct from the GET-only
// `/v1/daemon/narrative/worlds/:world_id` in Group 3).

#[tokio::test]
#[serial]
async fn world_by_id_hits_handler_not_framework_404() {
    let ctx = route_ctx().await;
    let route = "/v1/daemon/worlds/dummy-id";
    let resp = ctx.server.delete(route).await;
    assert_not_framework_404(&resp, route);
}

// ── Group 8: findings (creator-scoped lookup) ─────────────────────────────

#[tokio::test]
#[serial]
async fn finding_by_id_hits_handler_not_framework_404() {
    let ctx = route_ctx().await;
    let route = "/v1/daemon/findings/dummy-id";
    let resp = ctx.server.get(route).await;
    assert_not_framework_404(&resp, route);
}
