//! Daemon route tests — `POST /v1/daemon/worlds/:world_id/forks`
//! (V1.162 P1 T2, plan `2026-08-12-v1.162-p1-fork-backend-foundation`).
//!
//! The route is a thin delegate to the `nexus.fork.create` capability; these
//! tests pin the observable HTTP contract: authz order (ownership FIRST →
//! 403 before any fork-point read), fork-point validation → 422, happy path →
//! 200 with the locked response DTO, and the T1 closure — the canon
//! `fork_created` marker with `fork_lineage` is readable through the EXISTING
//! timeline-events route (no new read route this iteration).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::{json, Value};

struct Ctx {
    _tmp: TestTempRoot,
    server: TestServer,
    pool: sqlx::SqlitePool,
}

/// Standard server: seeded creator + owned world under keyless auth.
async fn ctx() -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().expect("pool").clone();
    test_utils::seed_test_creator_and_world(&pool).await;
    let app = api::create_router(state, DaemonApiConfig::keyless());
    let server = TestServer::new(app).expect("test server");
    Ctx {
        _tmp: tmp,
        server,
        pool,
    }
}

/// Seed a World owned by `test_creator` with one parent-branch event to use
/// as the fork point. Returns `(world_id, parent_branch_id, fork_point)`.
async fn seed_world_with_fork_point(pool: &sqlx::SqlitePool) -> (String, String, String) {
    let w = nexus_local_db::narrative_write::create_world(
        pool,
        "test_creator",
        "Fork Test",
        "fork-test",
        "private",
        "manual",
    )
    .await
    .expect("create world");
    let evt = nexus_local_db::narrative_write::append_event(
        pool,
        &w.world_id,
        &w.root_fork_branch_id,
        "story_advance",
        Some("Parent event"),
        None,
    )
    .await
    .expect("append parent event");
    (w.world_id, w.root_fork_branch_id, evt.event_id)
}

/// Seed a World owned by a *different* creator (ownership-gate tests).
async fn seed_foreign_world(pool: &sqlx::SqlitePool) -> String {
    // SAFETY: test-only seed against the known creators/narrative_worlds schema.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
    // SAFETY: test-only seed against the known narrative_worlds schema.
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
           VALUES ('wld_fork_foreign', 'ws', 'other_creator', 'Foreign World', 'foreign-world', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .execute(pool)
    .await
    .unwrap();
    "wld_fork_foreign".to_string()
}

/// Assert the canonical daemon API error envelope
/// (`{"success": false, "error": {"code", "message", ...}}`).
fn assert_error_envelope(resp: &axum_test::TestResponse, status: StatusCode, code: &str) {
    assert_eq!(resp.status_code(), status, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["success"], false, "body={body}");
    assert_eq!(body["error"]["code"], code, "body={body}");
    assert!(
        body["error"]["message"].is_string(),
        "error.message must be a string: {body}"
    );
}

/// POST the locked create-fork request body for a world.
#[allow(clippy::future_not_send)]
async fn post_fork(
    server: &TestServer,
    world_id: &str,
    parent_branch_id: &str,
    forked_from_event_id: &str,
    label: Option<&str>,
) -> axum_test::TestResponse {
    let mut body = json!({
        "parent_branch_id": parent_branch_id,
        "forked_from_event_id": forked_from_event_id,
    });
    if let Some(label) = label {
        body["label"] = json!(label);
    }
    server
        .post(&format!("/v1/daemon/worlds/{world_id}/forks"))
        .json(&body)
        .await
}

// ─── POST /v1/daemon/worlds/:world_id/forks ───────────────────────────────

/// Happy path: owner creates a fork from a valid fork-point → 200 with
/// `branch_id` (starts `fbk_`) + parent + fork-point + `created_at`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_happy_path() {
    let ctx = ctx().await;
    let (world_id, parent_branch, fork_point) = seed_world_with_fork_point(&ctx.pool).await;

    let resp = post_fork(
        &ctx.server,
        &world_id,
        &parent_branch,
        &fork_point,
        Some("alt-ending"),
    )
    .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    let branch_id = body["branch_id"]
        .as_str()
        .expect("branch_id must be a string");
    assert!(
        branch_id.starts_with("fbk_"),
        "branch_id must start with 'fbk_': {branch_id}"
    );
    assert_eq!(body["parent_branch_id"], parent_branch);
    assert_eq!(body["forked_from_event_id"], fork_point);
    assert!(
        body["created_at"].as_str().map_or(0, str::len) > 0,
        "created_at must be a non-empty string: {body}"
    );
}

/// Foreign world → 403, BEFORE any fork-point read (the request references
/// fork-point ids that exist only in the owner's world — the guard must not
/// leak whether they resolve).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_foreign_world_forbidden_403() {
    let ctx = ctx().await;
    let foreign_world = seed_foreign_world(&ctx.pool).await;

    let resp = post_fork(
        &ctx.server,
        &foreign_world,
        "fbk_any_parent",
        "evt_any_fork_point",
        None,
    )
    .await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");
}

/// Bad fork-point (event not on the stated parent branch / non-existent) →
/// 422 with `invalid_input`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_bad_fork_point_422() {
    let ctx = ctx().await;
    let (world_id, parent_branch, _fork_point) = seed_world_with_fork_point(&ctx.pool).await;

    let resp = post_fork(
        &ctx.server,
        &world_id,
        &parent_branch,
        "evt_does_not_exist",
        None,
    )
    .await;
    assert_error_envelope(&resp, StatusCode::UNPROCESSABLE_ENTITY, "invalid_input");
}

/// Round-trip closure with T1: create a fork, then read the EXISTING
/// timeline-events route with `branch_id=<new>&event_type=fork_created&
/// status=canon` — exactly one canon marker whose `extensions.fork_lineage`
/// carries the parent + fork-point + label.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fork_create_result_lineage_readable() {
    let ctx = ctx().await;
    let (world_id, parent_branch, fork_point) = seed_world_with_fork_point(&ctx.pool).await;

    let resp = post_fork(
        &ctx.server,
        &world_id,
        &parent_branch,
        &fork_point,
        Some("alt-ending"),
    )
    .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let new_branch = resp.json::<Value>()["branch_id"]
        .as_str()
        .expect("branch_id")
        .to_string();

    let read = ctx
        .server
        .get(&format!(
            "/v1/daemon/worlds/{world_id}/timeline/events?branch_id={new_branch}&event_type=fork_created&status=canon"
        ))
        .await;
    assert_eq!(read.status_code(), StatusCode::OK, "body={}", read.text());
    let body: Value = read.json();
    let items = body["items"].as_array().expect("items array");
    assert_eq!(
        items.len(),
        1,
        "exactly one canon fork_created marker: {body}"
    );
    let marker = &items[0];
    assert_eq!(marker["event_type"], "fork_created");
    assert_eq!(marker["branch_id"], new_branch);
    assert_eq!(marker["status"], "canon");
    let lineage = &marker["extensions"]["fork_lineage"];
    assert_eq!(lineage["parent_branch_id"], parent_branch);
    assert_eq!(lineage["forked_from_event_id"], fork_point);
    assert_eq!(lineage["label"], "alt-ending");
}
