//! Agent Tool API hermetic tests (V1.34 P4 — spec §10, §12.7).
//!
//! Tests covering the admission pipeline, tool dispatch, and permission
//! enforcement for the unified HostToolExecutor registry.
//!
//! Test vectors from spec §10:
//! - TV-1: Happy path nexus.work.get (active creator, own work)
//! - TV-2: Cross-creator nexus.work.get → FORBIDDEN
//! - TV-3: nexus.context.assemble platform-required → POLICY_BLOCKED
//!
//! Additional tests (spec §12.7):
//! - nexus.context.whoami returns active creator id
//! - nexus.workspace.info returns workspace slug
//! - nexus.work.patch happy path (append inspiration_log)
//! - nexus.work.patch rejects forbidden field (current_stage)
//! - nexus.orchestration.schedule_status happy path

#![allow(clippy::unwrap_used)]

use nexus_daemon_runtime::api::handlers::host_tool_executor::{
    HostToolCallerKind, HostToolExecutor, ToolExecuteRequest,
};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::works;
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

/// Create a test work record for the active test_creator.
async fn seed_work(state: &WorkspaceState) -> String {
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Test Work".to_string(),
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
    };
    works::create_work_atomic(state.pool(), &record, None)
        .await
        .unwrap()
        .unwrap_err(); // Returns new record in Err
    work_id
}

/// Create a test ctx with a different active creator.
async fn test_ctx_other_creator() -> TestCtx {
    let tmp = tempfile::TempDir::new().unwrap();
    let user_home = tmp.path();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();

    let other_creator = "ctr_other_creator";
    let toml_str = format!(
        "active_creator_id = \"{other_creator}\"\n[active_workspace_slug_by_creator]\n\"{other_creator}\" = \"default\""
    );
    std::fs::write(nexus_home.join("config.toml"), toml_str).unwrap();

    let op_dir = nexus_home_layout::operational_workspace_dir(user_home, other_creator, "default");
    std::fs::create_dir_all(&op_dir).unwrap();
    let meta = serde_json::json!({
        "schema_version": 1,
        "creator_id": other_creator,
        "workspace_slug": "default",
        "local_root": user_home.join("creative"),
        "created_at": "2020-01-01T00:00:00Z"
    });
    std::fs::write(
        op_dir.join("meta.json"),
        serde_json::to_string(&meta).unwrap(),
    )
    .unwrap();

    let db_path = nexus_home_layout::workspace_state_db_path(user_home, other_creator, "default");

    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    nexus_local_db::seed_versions(&pool).await.unwrap();

    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    std::mem::forget(tmp);
    TestCtx {
        _tmp: test_utils::create_test_workspace().await.0,
        state,
    }
}

fn make_request(tool_name: &str, params: serde_json::Value) -> ToolExecuteRequest {
    ToolExecuteRequest {
        tool_name: tool_name.to_string(),
        parameters: params,
        session_id: None,
        request_id: None,
        caller_kind: None,
    }
}

// ─── TV-1: nexus.context.whoami ────────────────────────────────────────────

#[tokio::test]
async fn whoami_returns_active_creator_id() {
    let ctx = test_ctx().await;
    let req = make_request("nexus.context.whoami", json!({}));
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["creator_id"], "test_creator");
    assert_eq!(val["workspace_slug"], "default");
}

// ─── nexus.workspace.info ──────────────────────────────────────────────────

#[tokio::test]
async fn workspace_info_returns_workspace_slug() {
    let ctx = test_ctx().await;
    let req = make_request("nexus.workspace.info", json!({}));
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["creator_id"], "test_creator");
    assert_eq!(val["workspace_slug"], "default");
    assert_eq!(val["runtime_mode"], "local_only");
}

// ─── TV-1: nexus.work.get happy path ──────────────────────────────────────

#[tokio::test]
async fn work_get_happy_path_returns_work_stage_fields() {
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    let req = make_request("nexus.work.get", json!({ "work_id": work_id }));
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["work_id"], work_id);
    // WorkApiDto does not include creator_id (privacy filter)
    assert_eq!(val["title"], "Test Work");
    assert_eq!(val["current_stage"], "research");
    assert_eq!(val["stage_status"], "active");
}

// ─── TV-2: cross-creator nexus.work.get → FORBIDDEN ───────────────────────

#[tokio::test]
async fn work_get_cross_creator_returns_forbidden() {
    // Create a work with test_creator
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    // Try to read it with other_creator
    let other_ctx = test_ctx_other_creator().await;
    let req = make_request("nexus.work.get", json!({ "work_id": work_id }));
    let result = HostToolExecutor::execute(&req, &other_ctx.state).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_code(), "FORBIDDEN");
}

// ─── nexus.work.patch happy path (append inspiration) ─────────────────────

#[tokio::test]
async fn work_patch_append_inspiration_happy_path() {
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    let req = make_request(
        "nexus.work.patch",
        json!({
            "work_id": work_id,
            "inspiration_log": [
                { "text": "Agent had an idea", "source": "acp_agent" }
            ]
        }),
    );
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["work_id"], work_id);

    // Verify inspiration was appended
    let inspiration = val["inspiration_log"].as_array().unwrap();
    assert!(!inspiration.is_empty());
    let last = inspiration.last().unwrap();
    assert_eq!(last["note"], "Agent had an idea");
}

// ─── nexus.work.patch rejects forbidden field (current_stage) ─────────────

#[tokio::test]
async fn work_patch_rejects_current_stage_field() {
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    let req = make_request(
        "nexus.work.patch",
        json!({
            "work_id": work_id,
            "current_stage": "writing"
        }),
    );
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_code(), "BAD_REQUEST");
}

// ─── TV-3: nexus.context.assemble POLICY_BLOCKED ──────────────────────────

#[tokio::test]
async fn context_assemble_policy_blocked_when_platform_required() {
    let ctx = test_ctx().await;

    let req = make_request(
        "nexus.context.assemble",
        json!({ "requires_platform": true }),
    );
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_err());
    match result {
        Err(nexus_daemon_runtime::api::errors::NexusApiError::BadRequest { code, message }) => {
            assert_eq!(code, "POLICY_BLOCKED");
            assert!(
                message.contains("PLATFORM_PAUSED"),
                "Message should contain PLATFORM_PAUSED: {message}"
            );
        }
        Err(e) => panic!("Expected BadRequest(POLICY_BLOCKED), got: {e:?}"),
        Ok(_) => panic!("Expected error"),
    }
}

// ─── nexus.orchestration.schedule_status happy path ────────────────────────

#[tokio::test]
async fn schedule_status_happy_path() {
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    let req = make_request(
        "nexus.orchestration.schedule_status",
        json!({ "work_id": work_id }),
    );
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_ok());
    let val = result.unwrap();
    assert_eq!(val["work_id"], work_id);
    assert_eq!(val["count"], 1);
}
