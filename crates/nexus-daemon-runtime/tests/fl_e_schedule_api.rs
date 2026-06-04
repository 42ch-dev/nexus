//! FL-E schedule API contract tests (R-FL-E-P2-02).
//!
//! Hermetic tests that exercise the daemon schedule creation API via
//! TestServer, verifying that schedule requests are correctly enqueued
//! with the right preset, creator, and seed fields.
//!
//! Covers:
//! - Schedule creation with AddScheduleRequest DTO (snake_case fields)
//! - Work-derived context propagates via seed into core_context
//! - Cross-creator isolation for schedules
//! - List schedule verification via supervisor's own pool

#![allow(clippy::unwrap_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_contracts::local::schedule::http::AddScheduleRequest;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::{AuthMode, DaemonApiConfig};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;
use serde_json::{json, Value};
use std::sync::Arc;

struct TestCtx {
    _tmp: TestTempRoot,
    server: TestServer,
}

async fn test_ctx() -> TestCtx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;

    let mut state =
        WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;

    // Wire a schedule supervisor using a separate pool to the same DB.
    // This mirrors how boot.rs creates the schedule supervisor.
    let db_url = format!("sqlite:{}?mode=rw", db_path.display());
    let schedule_pool = Arc::new(sqlx::SqlitePool::connect(&db_url).await.unwrap());
    let supervisor = Arc::new(ScheduleSupervisor::new(schedule_pool));
    state.set_schedule_supervisor(supervisor);

    let auth_config = DaemonApiConfig {
        api_key: None,
        auth_mode: AuthMode::KeylessLocalhost,
    };
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    TestCtx { _tmp: tmp, server }
}

// ── Test 1: Schedule creation with correct AddScheduleRequest DTO ────────────

#[tokio::test]
async fn schedule_create_with_correct_dto_shape() {
    let ctx = test_ctx().await;

    let req = AddScheduleRequest {
        creator_id: "ctr_test".to_string(),
        preset_id: "research".to_string(),
        seed: Some(
            serde_json::to_string(&json!({
                "work_id": "wrk_test123",
                "fl_e_stage": "research",
                "creative_brief": "{\"genre\":\"sci-fi\"}",
                "inspiration_log": "[]"
            }))
            .unwrap(),
        ),
        label: Some("FL-E stage: research (work: wrk_test123)".to_string()),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
    };

    let resp = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req)
        .await;
    resp.assert_status(StatusCode::CREATED);

    let body: Value = resp.json();

    // Verify response uses snake_case (AddScheduleResponse field names)
    assert!(
        body.get("schedule_id").is_some(),
        "Response must contain schedule_id (snake_case): {body}"
    );
    assert_eq!(body["status"], "pending");
    assert!(
        body.get("core_context_version").is_some(),
        "Response must contain core_context_version: {body}"
    );

    // Verify via list that the schedule was persisted with correct fields
    let list_resp = ctx
        .server
        .get("/v1/local/orchestration/schedules?creator_id=ctr_test")
        .await;
    list_resp.assert_status(StatusCode::OK);

    let list_body: Value = list_resp.json();
    let schedules = list_body["schedules"].as_array().unwrap();
    assert_eq!(schedules.len(), 1, "Should find one schedule for ctr_test");
    let sched = &schedules[0];
    assert_eq!(sched["creator_id"], "ctr_test");
    assert_eq!(sched["preset_id"], "research");
    assert_eq!(sched["status"], "pending");
    assert_eq!(
        sched["label"].as_str().unwrap(),
        "FL-E stage: research (work: wrk_test123)"
    );
}

// ── Test 2: Seed data creates core_context v0 ────────────────────────────────

#[tokio::test]
async fn schedule_create_seeds_core_context_from_preset_input() {
    let ctx = test_ctx().await;

    let seed_data = json!({
        "work_id": "wrk_ctx_test",
        "fl_e_stage": "produce",
        "creative_brief": "{\"genre\":\"fantasy\"}",
        "inspiration_log": "[{\"note\":\"dragons\"}]"
    });

    let req = AddScheduleRequest {
        creator_id: "ctr_ctx".to_string(),
        preset_id: "novel-writing".to_string(),
        seed: Some(serde_json::to_string(&seed_data).unwrap()),
        label: Some("FL-E stage: produce (work: wrk_ctx_test)".to_string()),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
    };

    let resp = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req)
        .await;
    resp.assert_status(StatusCode::CREATED);

    let body: Value = resp.json();

    // Verify that core_context_version was incremented (seed creates v0)
    assert_eq!(
        body["core_context_version"], 0,
        "Seed should create core_context version 0: {body}"
    );

    // Verify schedule exists in list with correct preset
    let list_resp = ctx
        .server
        .get("/v1/local/orchestration/schedules?creator_id=ctr_ctx")
        .await;
    let list_body: Value = list_resp.json();
    let schedules = list_body["schedules"].as_array().unwrap();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0]["preset_id"], "novel-writing");
    assert_eq!(schedules[0]["creator_id"], "ctr_ctx");
}

// ── Test 3: Cross-creator isolation ──────────────────────────────────────────

#[tokio::test]
async fn schedule_list_isolation_by_creator() {
    let ctx = test_ctx().await;

    // Create two schedules with different creator IDs
    let req_a = AddScheduleRequest {
        creator_id: "ctr_alpha".to_string(),
        preset_id: "research".to_string(),
        seed: None,
        label: None,
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
    };
    let req_b = AddScheduleRequest {
        creator_id: "ctr_beta".to_string(),
        preset_id: "novel-writing".to_string(),
        seed: None,
        label: None,
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
    };

    let resp_a = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req_a)
        .await;
    resp_a.assert_status(StatusCode::CREATED);

    let resp_b = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req_b)
        .await;
    resp_b.assert_status(StatusCode::CREATED);

    // List all schedules
    let all_resp = ctx.server.get("/v1/local/orchestration/schedules").await;
    let all_body: Value = all_resp.json();
    let all_schedules = all_body["schedules"].as_array().unwrap();
    assert_eq!(all_schedules.len(), 2, "Should have 2 schedules total");

    // Filter by creator_alpha — only their schedule appears
    let alpha_resp = ctx
        .server
        .get("/v1/local/orchestration/schedules?creator_id=ctr_alpha")
        .await;
    let alpha_body: Value = alpha_resp.json();
    let alpha_schedules = alpha_body["schedules"].as_array().unwrap();
    assert_eq!(alpha_schedules.len(), 1, "Only ctr_alpha schedules");
    assert_eq!(alpha_schedules[0]["creator_id"], "ctr_alpha");
    assert_eq!(alpha_schedules[0]["preset_id"], "research");
}

// ── Test 4: Schedule without seed has no core_context ────────────────────────

#[tokio::test]
async fn schedule_create_without_seed_no_core_context() {
    let ctx = test_ctx().await;

    let req = AddScheduleRequest {
        creator_id: "ctr_noseed".to_string(),
        preset_id: "reflection-loop".to_string(),
        seed: None,
        label: None,
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
    };

    let resp = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req)
        .await;
    resp.assert_status(StatusCode::CREATED);

    let body: Value = resp.json();

    // Without seed, core_context_version should still be 0 (no seeding happened)
    assert_eq!(
        body["core_context_version"], 0,
        "Without seed, version should be 0: {body}"
    );

    // Verify schedule is in the list
    let list_resp = ctx
        .server
        .get("/v1/local/orchestration/schedules?creator_id=ctr_noseed")
        .await;
    let list_body: Value = list_resp.json();
    let schedules = list_body["schedules"].as_array().unwrap();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0]["preset_id"], "reflection-loop");
}
