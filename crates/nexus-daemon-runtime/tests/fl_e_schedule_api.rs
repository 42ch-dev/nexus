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
//!
//! PR #50 review (cursor automation, medium): preset gate authorization
//! bypass regression. Tests for gated presets (`research`, etc.) seed
//! a Work row first via `seed_work()` so the gate-eval path can load
//! the Work snapshot. Tests for un-gated presets (e.g., non-novel) need
//! no Work seed.

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
use nexus_local_db::works::{self, WorkRecord};
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

/// Seed a minimal Work row so gated presets can load a Work snapshot during
/// gate evaluation. Returns the pool for further seeding if needed.
async fn seed_work(db_path: &PathBuf, work_id: &str, creator_id: &str) {
    let db_url = format!("sqlite:{}?mode=rw", db_path.display());
    let pool = sqlx::SqlitePool::connect(&db_url).await.unwrap();
    let record = WorkRecord {
        work_id: work_id.to_string(),
        creator_id: creator_id.to_string(),
        workspace_slug: "default".to_string(),
        status: "draft".to_string(),
        title: "Test Novel".to_string(),
        long_term_goal: "Write a great novel".to_string(),
        initial_idea: "A sci-fi thriller".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(), // satisfies research preset's intake_status==complete gate
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-04T10:00:00Z".to_string(),
        updated_at: "2026-06-04T10:00:00Z".to_string(),
        driver_schedule_id: None,
        auto_chain_enabled: false,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
        work_profile: Some("novel".to_string()),
        work_ref: Some(work_id.to_string()),
        current_stage: "research".to_string(),
        stage_status: "active".to_string(),
        total_planned_chapters: Some(10),
        current_chapter: 0,
    };
    works::create_work(&pool, &record)
        .await
        .expect("seed_work: create_work failed");
}

// ── Test 1: Schedule creation with correct AddScheduleRequest DTO ────────────

#[tokio::test]
async fn schedule_create_with_correct_dto_shape() {
    let ctx = test_ctx().await;
    seed_work(&ctx.db_path, "wrk_test123", "ctr_test").await;

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
    let schedules = list_body["items"].as_array().unwrap();
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
    seed_work(&ctx.db_path, "wrk_ctx_test", "ctr_ctx").await;

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
    let schedules = list_body["items"].as_array().unwrap();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0]["preset_id"], "research");
    assert_eq!(schedules[0]["creator_id"], "ctr_ctx");
}

// ── Test 3: Cross-creator isolation ──────────────────────────────────────────

#[tokio::test]
async fn schedule_list_isolation_by_creator() {
    let ctx = test_ctx().await;
    seed_work(&ctx.db_path, "wrk_alpha_test", "ctr_alpha").await;
    seed_work(&ctx.db_path, "wrk_beta_test", "ctr_beta").await;

    // Create two schedules with different creator IDs
    let req_a = AddScheduleRequest {
        creator_id: "ctr_alpha".to_string(),
        preset_id: "research".to_string(),
        seed: Some(serde_json::to_string(&json!({"work_id": "wrk_alpha_test"})).unwrap()),
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
        seed: Some(serde_json::to_string(&json!({"work_id": "wrk_beta_test"})).unwrap()),
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
    let all_schedules = all_body["items"].as_array().unwrap();
    assert_eq!(all_schedules.len(), 2, "Should have 2 schedules total");

    // Filter by creator_alpha — only their schedule appears
    let alpha_resp = ctx
        .server
        .get("/v1/local/orchestration/schedules?creator_id=ctr_alpha")
        .await;
    let alpha_body: Value = alpha_resp.json();
    let alpha_schedules = alpha_body["items"].as_array().unwrap();
    assert_eq!(alpha_schedules.len(), 1, "Only ctr_alpha schedules");
    assert_eq!(alpha_schedules[0]["creator_id"], "ctr_alpha");
    assert_eq!(alpha_schedules[0]["preset_id"], "research");
}

// ── Test 4: Schedule without seed has no core_context ────────────────────────

#[tokio::test]
async fn schedule_create_without_seed_no_core_context() {
    let ctx = test_ctx().await;

    // V1.47: switched from `reflection-loop` (now `novel-chapter-review`,
    // which declares novel-only gates and would 422 without a work_id) to
    // `memory-augmented` — a non-gated preset — so this test keeps its
    // original intent (seed propagation, not gate evaluation).
    let req = AddScheduleRequest {
        creator_id: "ctr_noseed".to_string(),
        preset_id: "memory-augmented".to_string(),
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
    let schedules = list_body["items"].as_array().unwrap();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0]["preset_id"], "memory-augmented");
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
    seed_work(&ctx.db_path, "wrk_real_test", "ctr_real").await;

    // Create a schedule with an empty creator_id (the pre-fix bug scenario).
    // PR #50 review: gated presets now require work_id AND gate-eval enforces
    // creator scoping. Empty creator_id + no work_id fails closed with 422
    // (correct: a request with no identity and no Work context cannot be
    // authorized). The pre-fix bug is therefore amplified: requests can no
    // longer bypass gate checks regardless of creator_id.
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
    // Empty creator_id + no work_id → 422 preset_gates_failed (gate-eval requires
    // work_id, which is None here). This is the desired fail-closed behavior.
    resp_empty.assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    // Create a schedule with a proper creator_id
    let req_real = AddScheduleRequest {
        creator_id: "ctr_real".to_string(),
        preset_id: "research".to_string(),
        seed: Some(serde_json::to_string(&json!({"work_id": "wrk_real_test"})).unwrap()),
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
    let schedules = list_body["items"].as_array().unwrap();
    assert_eq!(schedules.len(), 1, "Only ctr_real schedule should appear");
    assert_eq!(schedules[0]["creator_id"], "ctr_real");

    // Listing all schedules shows only the real one (PR #50 review:
    // empty creator_id request was rejected with 422 preset_gates_failed —
    // the schedule was never created). Pre-fix this assertion expected 2.
    let all_resp = ctx.server.get("/v1/local/orchestration/schedules").await;
    let all_body: Value = all_resp.json();
    let all_schedules = all_body["items"].as_array().unwrap();
    assert_eq!(
        all_schedules.len(),
        1,
        "Only the real schedule exists; the empty-creator_id request was rejected"
    );
    assert_eq!(all_schedules[0]["creator_id"], "ctr_real");
}

// ── Test 5b: PR #50 review — gated preset without work_id is rejected ────────
//
// Cursor automation flagged that the C-1 fix in P0.5 (making gate evaluation
// conditional on work_id presence) created a security regression: any client
// could POST a gated preset (e.g. `research`, `novel-writing`) without
// providing work_id and the schedule would be enqueued without gate checks.
// The fix is to always evaluate gates when the preset declares them and
// fail closed (422 preset_gates_failed) if work_id is missing.

#[tokio::test]
async fn gated_preset_without_work_id_is_rejected() {
    let ctx = test_ctx().await;
    // Note: intentionally NOT calling seed_work() — work_id must be omitted
    // from the request to exercise the security path.

    let req = AddScheduleRequest {
        creator_id: "ctr_security".to_string(),
        preset_id: "research".to_string(), // gated preset
        seed: None,                        // no work_id via seed
        label: Some("security regression test".to_string()),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: None, // no work_id via input
        force_gates: false,
        reason: None,
    };
    let resp = ctx
        .server
        .post("/v1/local/orchestration/schedules")
        .json(&req)
        .await;
    resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

// ── F-F1 sort tests ─────────────────────────────────────────────────────────

async fn create_schedule_with_label(server: &TestServer, creator_id: &str, label: &str) {
    let req = AddScheduleRequest {
        creator_id: creator_id.to_string(),
        preset_id: "memory-augmented".to_string(),
        seed: None,
        label: Some(label.to_string()),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: None,
        force_gates: false,
        reason: None,
    };
    let resp = server
        .post("/v1/local/orchestration/schedules")
        .json(&req)
        .await;
    resp.assert_status(StatusCode::CREATED);
}

#[tokio::test]
async fn schedule_list_sort_by_label_ascending() {
    let ctx = test_ctx().await;
    create_schedule_with_label(&ctx.server, "ctr_sort", "Beta").await;
    create_schedule_with_label(&ctx.server, "ctr_sort", "Alpha").await;
    create_schedule_with_label(&ctx.server, "ctr_sort", "Charlie").await;

    let resp = ctx
        .server
        .get("/v1/local/orchestration/schedules?creator_id=ctr_sort&sort=label")
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    let items = body["items"].as_array().unwrap();
    let labels: Vec<String> = items
        .iter()
        .map(|i| i["label"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(labels, vec!["Alpha", "Beta", "Charlie"]);
}

#[tokio::test]
async fn schedule_list_sort_descending_and_pagination() {
    let ctx = test_ctx().await;
    create_schedule_with_label(&ctx.server, "ctr_sort2", "Alpha").await;
    create_schedule_with_label(&ctx.server, "ctr_sort2", "Beta").await;
    create_schedule_with_label(&ctx.server, "ctr_sort2", "Charlie").await;

    let resp = ctx
        .server
        .get("/v1/local/orchestration/schedules?creator_id=ctr_sort2&sort=-label&limit=2")
        .await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["label"], "Charlie");
    assert_eq!(items[1]["label"], "Beta");
    assert_eq!(body["pagination"]["has_more"], true);

    let next_cursor = body["pagination"]["next_cursor"].as_str().unwrap();
    let resp2 = ctx
        .server
        .get(&format!(
            "/v1/local/orchestration/schedules?creator_id=ctr_sort2&sort=-label&limit=2&cursor={next_cursor}"
        ))
        .await;
    resp2.assert_status(StatusCode::OK);
    let body2: Value = resp2.json();
    let items2 = body2["items"].as_array().unwrap();
    assert_eq!(items2.len(), 1);
    assert_eq!(items2[0]["label"], "Alpha");
    assert_eq!(body2["pagination"]["has_more"], false);
}

#[tokio::test]
async fn schedule_list_invalid_sort_key_returns_schedule_sort_invalid() {
    let ctx = test_ctx().await;
    create_schedule_with_label(&ctx.server, "ctr_sort3", "Alpha").await;

    let resp = ctx
        .server
        .get("/v1/local/orchestration/schedules?creator_id=ctr_sort3&sort=unknown_key")
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    let body: Value = resp.json();
    assert_eq!(body["success"], false);
    assert_eq!(body["error"]["code"], "schedule_sort_invalid");
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

    let error = &body["error"];
    assert_eq!(error["code"], "preset_gates_failed", "body: {body}");
    assert_eq!(error["details"]["preset_id"], "novel-writing");
    assert_eq!(error["details"]["work_id"], "wrk_gate_test");

    let failed_gates = error["details"]["failed_gates"]
        .as_array()
        .expect("failed_gates array");
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
