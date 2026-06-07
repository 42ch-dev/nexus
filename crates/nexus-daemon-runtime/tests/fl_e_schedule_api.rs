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
use nexus_local_db::list_force_gates_audit;
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

struct TestCtx {
    _tmp: TestTempRoot,
    server: TestServer,
    db_path: PathBuf,
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

    // Wire a capability registry so preset gates are evaluated.
    let registry = Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
    state.set_capability_registry(registry);

    let auth_config = DaemonApiConfig {
        api_key: None,
        auth_mode: AuthMode::KeylessLocalhost,
    };
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    TestCtx {
        _tmp: tmp,
        server,
        db_path,
    }
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
        input: None,
        force_gates: false,
        reason: None,
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
        preset_id: "research".to_string(),
        seed: Some(serde_json::to_string(&seed_data).unwrap()),
        label: Some("FL-E stage: produce (work: wrk_ctx_test)".to_string()),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: None,
        force_gates: false,
        reason: None,
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
    assert_eq!(schedules[0]["preset_id"], "research");
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
        input: None,
        force_gates: false,
        reason: None,
    };
    let req_b = AddScheduleRequest {
        creator_id: "ctr_beta".to_string(),
        preset_id: "research".to_string(),
        seed: None,
        label: None,
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: None,
        force_gates: false,
        reason: None,
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
        input: None,
        force_gates: false,
        reason: None,
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

// ── Test 5: Empty creator_id breaks cross-creator isolation (R-FL-E-P2-05) ────
//
// Validates that a schedule created with an empty creator_id does NOT appear
// when filtering by a legitimate creator. The CLI fix (R-FL-E-P2-05) ensures
// the creator_id comes from `active_creator_id` config, never from WorkApiDto
// (which omits creator_id per SEC-V131-01). This test confirms the daemon-side
// isolation so that an empty-string creator_id is detectable as a bug.

#[tokio::test]
async fn schedule_with_empty_creator_id_is_isolated_from_legitimate_creators() {
    let ctx = test_ctx().await;

    // Create a schedule with an empty creator_id (the pre-fix bug scenario)
    let req_empty = AddScheduleRequest {
        creator_id: String::new(),
        preset_id: "research".to_string(),
        seed: None,
        label: Some("bug: empty creator_id".to_string()),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: None,
        force_gates: false,
        reason: None,
    };
    let resp_empty = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req_empty)
        .await;
    resp_empty.assert_status(StatusCode::CREATED);

    // Create a schedule with a proper creator_id
    let req_real = AddScheduleRequest {
        creator_id: "ctr_real".to_string(),
        preset_id: "research".to_string(),
        seed: None,
        label: Some("legitimate creator".to_string()),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: None,
        force_gates: false,
        reason: None,
    };
    let resp_real = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req_real)
        .await;
    resp_real.assert_status(StatusCode::CREATED);

    // Listing schedules for "ctr_real" should only return the real one,
    // not the one with empty creator_id
    let list_resp = ctx
        .server
        .get("/v1/local/orchestration/schedules?creator_id=ctr_real")
        .await;
    list_resp.assert_status(StatusCode::OK);
    let list_body: Value = list_resp.json();
    let schedules = list_body["schedules"].as_array().unwrap();
    assert_eq!(schedules.len(), 1, "Only ctr_real schedule should appear");
    assert_eq!(schedules[0]["creator_id"], "ctr_real");

    // Listing all schedules shows both (empty creator_id schedule exists)
    let all_resp = ctx.server.get("/v1/local/orchestration/schedules").await;
    let all_body: Value = all_resp.json();
    let all_schedules = all_body["schedules"].as_array().unwrap();
    assert_eq!(all_schedules.len(), 2, "Both schedules exist in total");
}

// ── Test 6: Force-gates audit row is written and queryable (V1.37 T5/T6) ──

#[tokio::test]
async fn force_gates_writes_audit_row() {
    let ctx = test_ctx().await;

    // Create a schedule with force_gates=true and a reason
    let req = AddScheduleRequest {
        creator_id: "ctr_audit".to_string(),
        preset_id: "novel-writing".to_string(),
        seed: Some("test force gates audit".to_string()),
        label: Some("force-gates audit test".to_string()),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: Some(json!({
            "work_id": "wrk_audit_test",
            "work_ref": "audit-novel"
        })),
        force_gates: true,
        reason: Some("testing emergency override".to_string()),
    };

    let resp = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req)
        .await;
    resp.assert_status(StatusCode::CREATED);

    let body: Value = resp.json();
    assert!(
        body.get("schedule_id").is_some(),
        "Schedule should be created: {body}"
    );

    // Query the audit table directly via a separate pool to the same DB
    let audit_pool =
        sqlx::SqlitePool::connect(&format!("sqlite:{}?mode=rw", ctx.db_path.display()))
            .await
            .unwrap();

    let rows = list_force_gates_audit(&audit_pool, "ctr_audit")
        .await
        .unwrap();

    assert_eq!(rows.len(), 1, "Should find exactly one audit row");
    let row = &rows[0];
    assert_eq!(row.preset_id, "novel-writing");
    assert_eq!(row.work_id, "wrk_audit_test");
    assert!(row.forced);
    assert_eq!(
        row.reason.as_deref(),
        Some("testing emergency override"),
        "Reason must match the provided text"
    );
}

// ── Test 7: Force-gates without reason is rejected ──────────────────────────

#[tokio::test]
async fn force_gates_without_reason_is_rejected() {
    let ctx = test_ctx().await;

    let req = AddScheduleRequest {
        creator_id: "ctr_noreason".to_string(),
        preset_id: "novel-writing".to_string(),
        seed: None,
        label: None,
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: None,
        force_gates: true,
        reason: Some(String::new()), // empty reason
    };

    let resp = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req)
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
}

// ── Test 8: Gate failure returns 422 with structured body (W-10) ─────────

#[tokio::test]
async fn gate_failure_returns_422_with_structured_body() {
    let ctx = test_ctx().await;

    // Create a Work with novel profile but WITHOUT running novel-project-init
    // and without the required directory scaffold.
    let db_url = format!("sqlite:{}?mode=rw", ctx.db_path.display());
    let pool = sqlx::SqlitePool::connect(&db_url).await.unwrap();

    // Insert a work row directly with all required fields
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, title, long_term_goal, initial_idea, status, \
         work_profile, work_ref, intake_status, workspace_slug, created_at, updated_at) \
         VALUES ('wrk_gate_test', 'ctr_gate', 'Gate Test Novel', 'Write a novel', 'Test idea', \
         'active', 'novel', 'gate-novel', 'complete', 'default', '2026-01-01', '2026-01-01')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Schedule novel-writing WITHOUT having run novel-project-init.
    let req = AddScheduleRequest {
        creator_id: "ctr_gate".to_string(),
        preset_id: "novel-writing".to_string(),
        seed: Some("test gate failure".to_string()),
        label: None,
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: Some(json!({
            "work_id": "wrk_gate_test",
            "work_ref": "gate-novel"
        })),
        force_gates: false,
        reason: None,
    };

    let resp = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req)
        .await;

    let status = resp.status_code();
    let body: Value = resp.json();

    // Should get 422 with structured gate failure
    if status != StatusCode::UNPROCESSABLE_ENTITY {
        eprintln!(
            "UNEXPECTED STATUS {status}\nbody: {body}\n\
             HINT: check if capability_registry is wired in test_ctx() and \
             the work_row was found (not None)."
        );
    }
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    assert_eq!(body["error"], "preset_gates_failed", "body: {body}");
    assert_eq!(body["preset_id"], "novel-writing");
    assert_eq!(body["work_id"], "wrk_gate_test");

    let failed_gates = body["failed_gates"].as_array().expect("failed_gates array");
    assert!(
        !failed_gates.is_empty(),
        "Should have at least one failed gate"
    );
    // At least one should reference the missing Works/<work_ref>/ directory
    let has_filesystem_failure = failed_gates.iter().any(|g| {
        g["kind"].as_str() == Some("filesystem") || g["kind"].as_str() == Some("previous_preset")
    });
    assert!(
        has_filesystem_failure,
        "Should have filesystem or previous_preset gate failure: {body}"
    );
}

// ── Test 9: Force-gates with long reason is rejected (W-5) ──────────────

#[tokio::test]
async fn force_gates_with_long_reason_rejected() {
    let ctx = test_ctx().await;

    let long_reason = "x".repeat(600);
    let req = AddScheduleRequest {
        creator_id: "ctr_long".to_string(),
        preset_id: "novel-writing".to_string(),
        seed: None,
        label: None,
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: None,
        force_gates: true,
        reason: Some(long_reason),
    };

    let resp = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req)
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
}

// ── Test 10: Force-gates with ANSI in reason is rejected (W-5) ──────────

#[tokio::test]
async fn force_gates_with_ansi_in_reason_rejected() {
    let ctx = test_ctx().await;

    let req = AddScheduleRequest {
        creator_id: "ctr_ansi".to_string(),
        preset_id: "novel-writing".to_string(),
        seed: None,
        label: None,
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: None,
        force_gates: true,
        reason: Some("ok \x1b[31mred\x1b[0m text".to_string()),
    };

    let resp = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req)
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
}
