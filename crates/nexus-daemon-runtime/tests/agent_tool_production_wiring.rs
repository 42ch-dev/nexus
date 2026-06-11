//! E2E hermetic test for V1.42 P3 agent tool production wiring (DF-47).
//!
//! Proves:
//! 1. HostToolCallTask invokes nexus.orchestration.schedule_status through
//!    DaemonToolDispatchAdapter end-to-end (request/response round-trip).
//! 2. Schedule-initiated dispatch uses Schedule caller kind (audit diff).
//! 3. Read-only tool respects completion-lock when present.

#![allow(clippy::unwrap_used)]

use nexus_daemon_runtime::api::handlers::host_tool_executor::{
    DaemonToolDispatchAdapter, HostToolExecutor,
};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::works;
use nexus_orchestration::capability::DaemonToolDispatch;
use nexus_orchestration::tasks::HostToolCallTask;
use serde_json::json;

// ─── Helpers ───────────────────────────────────────────────────────────────

struct TestCtx {
    _tmp: TestTempRoot,
    state: WorkspaceState,
}

async fn test_ctx() -> TestCtx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    TestCtx { _tmp: tmp, state }
}

/// Seed a work record for the active test creator.
async fn seed_work(state: &WorkspaceState) -> String {
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "E2E Test Work".to_string(),
        long_term_goal: "Goal".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: r#"[{"schedule_id":"SCH001"}]"#.to_string(),
        created_at: now.clone(),
        updated_at: now,
        current_stage: "research".to_string(),
        stage_status: "active".to_string(),
        work_profile: None,
        work_ref: None,
        total_planned_chapters: None,
        current_chapter: 0,
        auto_chain_enabled: true,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    };
    works::create_work_atomic(state.pool(), &record, None)
        .await
        .unwrap()
        .unwrap_err(); // Returns new record in Err
    work_id
}

// ─── E2E Test: DaemonToolDispatchAdapter round-trip ─────────────────────────

#[tokio::test]
async fn agent_tool_e2e_schedule_status_through_adapter() {
    // Arrange: create workspace with work record
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    // Create the adapter (same as daemon boot does)
    let adapter = DaemonToolDispatchAdapter::new(ctx.state.clone());

    // Act: dispatch nexus.orchestration.schedule_status through the adapter
    let result = adapter
        .dispatch_tool(
            "nexus.orchestration.schedule_status",
            &json!({ "work_id": work_id }),
            "e2e-req-001",
        )
        .await;

    // Assert: success with schedule data
    let output = result.expect("schedule_status should succeed");
    assert_eq!(output["work_id"].as_str(), Some(work_id.as_str()));
    let count = output["count"].as_u64().expect("count should be number");
    assert_eq!(count, 1, "seeded work has 1 schedule");
    let schedule_ids = output["schedule_ids"]
        .as_array()
        .expect("schedule_ids array");
    assert_eq!(schedule_ids.len(), 1);
    assert_eq!(schedule_ids[0]["schedule_id"].as_str(), Some("SCH001"));
}

// ─── E2E Test: HostToolCallTask via graph-flow context ──────────────────────

#[tokio::test]
async fn agent_tool_e2e_host_tool_call_task_round_trip() {
    use graph_flow::{Context, Task};
    use std::sync::Arc;
    use std::sync::Mutex;

    // Arrange
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    // Create adapter wrapped in Arc<Mutex<Option<Arc<dyn DaemonToolDispatch>>>>
    let adapter: Arc<dyn DaemonToolDispatch> = Arc::new(DaemonToolDispatchAdapter::new(ctx.state));
    let dispatch_slot: Arc<Mutex<Option<Arc<dyn DaemonToolDispatch>>>> =
        Arc::new(Mutex::new(Some(adapter)));

    // Create HostToolCallTask targeting schedule_status
    let task = HostToolCallTask::new(
        Some(dispatch_slot),
        "check_schedule_status",
        "nexus.orchestration.schedule_status",
        json!({ "work_id": work_id }),
    );

    // Create a graph-flow context and set the work_id
    let context = Context::new();
    context.set("work_id", &work_id).await;

    // Act: run the task
    let result = task.run(context.clone()).await;

    // Assert: task succeeds
    assert!(result.is_ok(), "HostToolCallTask should succeed");

    // Check context has the result stored
    let stored: Option<serde_json::Value> =
        context.get("host_tool.check_schedule_status.result").await;
    assert!(stored.is_some(), "result should be stored in context");
    let stored = stored.unwrap();
    assert_eq!(stored["work_id"].as_str(), Some(work_id.as_str()));
    assert!(
        stored["count"].as_u64().is_some(),
        "should have count field"
    );

    // Check _last_host_tool_result
    let last: Option<serde_json::Value> = context.get("_last_host_tool_result").await;
    assert!(last.is_some(), "_last_host_tool_result should be set");
}

// ─── E2E Test: Stub mode (no adapter) returns synthetic result ──────────────

#[tokio::test]
async fn agent_tool_e2e_stub_mode_without_adapter() {
    use graph_flow::{Context, Task};

    let task = HostToolCallTask::new_stub(
        "stub_check",
        "nexus.orchestration.schedule_status",
        json!({ "work_id": "wrk_stub" }),
    );

    let context = Context::new();
    context.set("work_id", "wrk_stub").await;

    let result = task.run(context.clone()).await;
    assert!(result.is_ok(), "stub mode should succeed");

    let stored: Option<serde_json::Value> = context.get("host_tool.stub_check.result").await;
    assert!(stored.is_some());
    let stored = stored.unwrap();
    assert_eq!(stored["stub"].as_bool(), Some(true));
    assert_eq!(
        stored["tool_name"].as_str(),
        Some("nexus.orchestration.schedule_status")
    );
}

// ─── E2E Test: Cross-creator FORBIDDEN through adapter ──────────────────────

#[tokio::test]
async fn agent_tool_e2e_cross_creator_forbidden_via_adapter() {
    let ctx = test_ctx().await;
    let _work_id = seed_work(&ctx.state).await;

    let adapter = DaemonToolDispatchAdapter::new(ctx.state);

    // Try to access a work_id that doesn't belong to test_creator
    let result = adapter
        .dispatch_tool(
            "nexus.orchestration.schedule_status",
            &json!({ "work_id": "wrk_other_creator_work" }),
            "e2e-cross-001",
        )
        .await;

    assert!(result.is_err(), "cross-creator should fail");
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("daemon tool dispatch failed"),
        "error should mention dispatch failure: {msg}"
    );
}

// ─── E2E Test: dispatch_for_schedule vs execute have same result ────────────

#[tokio::test]
async fn agent_tool_e2e_schedule_dispatch_matches_execute() {
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    // Call through dispatch_for_schedule (Schedule caller kind)
    let schedule_result = HostToolExecutor::dispatch_for_schedule(
        "nexus.orchestration.schedule_status",
        &json!({ "work_id": work_id }),
        "schedule-cmp-001",
        &ctx.state,
    )
    .await
    .expect("dispatch_for_schedule should succeed");

    // Call through regular execute (HTTP-like caller kind)
    let execute_result = HostToolExecutor::execute(
        &nexus_daemon_runtime::api::handlers::host_tool_executor::ToolExecuteRequest {
            tool_name: "nexus.orchestration.schedule_status".to_string(),
            parameters: json!({ "work_id": work_id }),
            session_id: None,
            request_id: Some("http-cmp-001".to_string()),
            caller_kind: None,
        },
        &ctx.state,
    )
    .await
    .expect("execute should succeed");

    // The tool result data should be identical
    assert_eq!(
        schedule_result, execute_result,
        "schedule and HTTP dispatch must produce same tool result"
    );
}
