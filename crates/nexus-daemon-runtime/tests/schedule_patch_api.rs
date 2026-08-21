//! `PATCH /v1/daemon/orchestration/schedules/{schedule_id}` — edit
//! label/metadata (V1.171 P2 AR-29) handler contract tests.
//!
//! Covers:
//! - label PATCH round-trip (200 + updated `ScheduleSummary`)
//! - label clear via empty string (label → null)
//! - no-op PATCH (`{}`) preserves the label and bumps `updated_at`
//! - unknown schedule id → 404 with the canonical envelope
//! - over-long label → 400 with a stable code

#![allow(clippy::unwrap_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::path::PathBuf;

struct TestCtx {
    _tmp: TestTempRoot,
    server: TestServer,
    db_path: PathBuf,
}

async fn test_ctx() -> TestCtx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    test_utils::seed_test_creator_and_world(state.pool().unwrap()).await;
    let auth_config = DaemonApiConfig::keyless();
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    TestCtx {
        _tmp: tmp,
        server,
        db_path,
    }
}

/// Open the creator DB read-write (same `?mode=rw` pattern as
/// `fl_e_schedule_api.rs`).
async fn open_db(db_path: &std::path::Path) -> SqlitePool {
    let db_url = format!("sqlite:{}?mode=rw", db_path.display());
    SqlitePool::connect(&db_url).await.expect("open creator db")
}

/// Seed a `creator_schedules` row directly (the daemon has no
/// schedule-create endpoint in this slice; the CLI creates schedules).
async fn seed_schedule(pool: &SqlitePool, schedule_id: &str, label: Option<&str>) {
    // SAFETY: test-only data setup.
    sqlx::query(
        "INSERT INTO creator_schedules \
         (schedule_id, creator_id, preset_id, preset_version, status, \
          concurrency_kind, current_core_context_version, label, \
          created_at, updated_at, work_id) \
         VALUES (?, ?, 'novel-writing', 1, 'pending', 'serial', 0, ?, 0, 0, NULL)",
    )
    .bind(schedule_id)
    .bind("test_creator")
    .bind(label)
    .execute(pool)
    .await
    .expect("seed schedule");
}

/// The `ScheduleSummary` wire shape the PATCH returns.
fn assert_summary_shape(body: &Value, expected_id: &str) {
    assert_eq!(body["schedule_id"], expected_id);
    assert_eq!(body["creator_id"], "test_creator");
    assert_eq!(body["preset_id"], "novel-writing");
    assert_eq!(body["status"], "pending");
    assert!(body["updated_at"].is_string());
}

#[tokio::test]
async fn patch_schedule_label_round_trip() {
    let ctx = test_ctx().await;
    let pool = open_db(&ctx.db_path).await;
    seed_schedule(&pool, "SCH_patch_r1", Some("before")).await;
    pool.close().await;

    let resp = ctx
        .server
        .patch("/v1/daemon/orchestration/schedules/SCH_patch_r1")
        .json(&json!({ "label": "after" }))
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    assert_summary_shape(&body, "SCH_patch_r1");
    assert_eq!(body["label"], "after");
}

#[tokio::test]
async fn patch_schedule_label_clear_with_empty_string() {
    let ctx = test_ctx().await;
    let pool = open_db(&ctx.db_path).await;
    seed_schedule(&pool, "SCH_patch_clr", Some("remove me")).await;
    pool.close().await;

    let resp = ctx
        .server
        .patch("/v1/daemon/orchestration/schedules/SCH_patch_clr")
        .json(&json!({ "label": "" }))
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(body["schedule_id"], "SCH_patch_clr");
    assert!(body["label"].is_null(), "empty label must clear to null");
}

#[tokio::test]
async fn patch_schedule_empty_body_preserves_label_and_bumps_updated_at() {
    let ctx = test_ctx().await;
    let pool = open_db(&ctx.db_path).await;
    seed_schedule(&pool, "SCH_patch_noop", Some("keep me")).await;
    pool.close().await;

    let resp = ctx
        .server
        .patch("/v1/daemon/orchestration/schedules/SCH_patch_noop")
        .json(&json!({}))
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(body["schedule_id"], "SCH_patch_noop");
    assert_eq!(body["label"], "keep me");
    // updated_at is an epoch integer in the DB, serialized as a string in
    // ScheduleSummary; a fresh PATCH must bump it past the seeded 0.
    let updated_at: i64 = body["updated_at"]
        .as_str()
        .expect("updated_at epoch string")
        .parse()
        .expect("updated_at parses as i64");
    assert!(
        updated_at > 0,
        "updated_at must be refreshed, got {updated_at}"
    );
}

#[tokio::test]
async fn patch_schedule_unknown_id_returns_404() {
    let ctx = test_ctx().await;
    let resp = ctx
        .server
        .patch("/v1/daemon/orchestration/schedules/SCH_does_not_exist")
        .json(&json!({ "label": "x" }))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "not_found");
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("SCH_does_not_exist"),
        "404 must name the schedule id: {message}"
    );
}

#[tokio::test]
async fn patch_schedule_overlong_label_returns_400() {
    let ctx = test_ctx().await;
    let pool = open_db(&ctx.db_path).await;
    seed_schedule(&pool, "SCH_patch_long", None).await;
    pool.close().await;

    let resp = ctx
        .server
        .patch("/v1/daemon/orchestration/schedules/SCH_patch_long")
        .json(&json!({ "label": "x".repeat(513) }))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn patch_schedule_preserves_status_and_core_context_version() {
    let ctx = test_ctx().await;
    let pool = open_db(&ctx.db_path).await;
    seed_schedule(&pool, "SCH_patch_pres", Some("lbl")).await;
    // Give the row a non-default status + version to prove PATCH only
    // touches label/updated_at.
    sqlx::query(
        "UPDATE creator_schedules SET status = 'paused', current_core_context_version = 7 \
         WHERE schedule_id = 'SCH_patch_pres'",
    )
    .execute(&pool)
    .await
    .expect("preset status");
    pool.close().await;

    let resp = ctx
        .server
        .patch("/v1/daemon/orchestration/schedules/SCH_patch_pres")
        .json(&json!({ "label": "new label" }))
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(body["label"], "new label");
    assert_eq!(body["status"], "paused", "status must be untouched");
    assert_eq!(
        body["current_core_context_version"], 7,
        "core-context version must be untouched"
    );
}
