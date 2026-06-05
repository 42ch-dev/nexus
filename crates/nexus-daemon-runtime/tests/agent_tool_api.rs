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
//!
//! Fix wave 2 coverage:
//! - Error code surface (POLICY_BLOCKED, NOT_SUPPORTED, FORBIDDEN, INVALID_INPUT)
//! - Audit log on every invocation path
//! - stage_metadata sub-field allowlist
//! - Worker upcall equivalence (HTTP and worker hit same dispatch)

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
    assert_eq!(err.error_code(), "INVALID_INPUT");
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

// ─── Fix wave 2: Error code surface tests ──────────────────────────────────

/// Helper: count audit log rows for a given tool_name + outcome prefix.
async fn count_audit_rows(pool: &sqlx::SqlitePool, tool_name: &str, outcome_prefix: &str) -> i64 {
    // SAFETY: dynamic SQL for test helper; compile-time macro not applicable.
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM acp_tool_audit_log WHERE tool_name = ? AND outcome LIKE ?",
    )
    .bind(tool_name)
    .bind(format!("{outcome_prefix}%"))
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

/// Helper: get latest audit row outcome for a given tool_name.
async fn latest_audit_outcome(pool: &sqlx::SqlitePool, tool_name: &str) -> String {
    // SAFETY: dynamic SQL for test helper; compile-time macro not applicable.
    let row: (String,) = sqlx::query_as(
        "SELECT outcome FROM acp_tool_audit_log WHERE tool_name = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(tool_name)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

#[tokio::test]
async fn error_code_not_supported_for_unknown_tool() {
    let ctx = test_ctx().await;
    let req = make_request("nexus.unknown.tool", json!({}));
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "NOT_SUPPORTED");
}

#[tokio::test]
async fn error_code_forbidden_for_missing_work() {
    let ctx = test_ctx().await;
    let req = make_request("nexus.work.get", json!({ "work_id": "wrk_nonexistent" }));
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
}

#[tokio::test]
async fn error_code_invalid_input_for_missing_params() {
    let ctx = test_ctx().await;
    let req = make_request("nexus.work.get", json!({}));
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
}

#[tokio::test]
async fn error_code_policy_blocked_surface_in_assemble() {
    let ctx = test_ctx().await;
    let req = make_request(
        "nexus.context.assemble",
        json!({ "requires_platform": true }),
    );
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    // Wire contract: error_code() returns "POLICY_BLOCKED" (spec §12.4)
    assert_eq!(err.error_code(), "POLICY_BLOCKED");
}

#[tokio::test]
async fn worker_upcall_surfaces_forbidden_error_code() {
    let ctx = test_ctx().await;
    let other_ctx = test_ctx_other_creator().await;
    let work_id = seed_work(&ctx.state).await;

    let result = HostToolExecutor::dispatch_from_worker(
        "nexus.work.get",
        &json!({ "work_id": work_id }),
        "req-002",
        &other_ctx.state,
    )
    .await;

    assert!(!result.grant);
    let err = result.error.expect("worker error should be present");
    assert_eq!(
        err.code, "FORBIDDEN",
        "Worker error code must surface FORBIDDEN"
    );
}

#[tokio::test]
async fn worker_upcall_surfaces_policy_blocked_error_code() {
    let ctx = test_ctx().await;
    let result = HostToolExecutor::dispatch_from_worker(
        "nexus.context.assemble",
        &json!({ "requires_platform": true }),
        "req-003",
        &ctx.state,
    )
    .await;

    assert!(!result.grant);
    let err = result.error.expect("worker error should be present");
    assert_eq!(
        err.code, "POLICY_BLOCKED",
        "Worker error code must surface POLICY_BLOCKED"
    );
}

#[tokio::test]
async fn worker_upcall_surfaces_not_supported_error_code() {
    let ctx = test_ctx().await;
    let result = HostToolExecutor::dispatch_from_worker(
        "nexus.unknown.tool",
        &json!({}),
        "req-004",
        &ctx.state,
    )
    .await;

    assert!(!result.grant);
    let err = result.error.expect("worker error should be present");
    assert_eq!(
        err.code, "NOT_SUPPORTED",
        "Worker error code must surface NOT_SUPPORTED"
    );
}

// ─── Fix wave 2: Audit log on every invocation path ─────────────────────────

#[tokio::test]
async fn audit_log_written_on_success() {
    let ctx = test_ctx().await;
    let req = make_request("nexus.context.whoami", json!({}));
    let _ = HostToolExecutor::execute(&req, &ctx.state).await;

    let count = count_audit_rows(ctx.state.pool(), "nexus.context.whoami", "success").await;
    assert_eq!(count, 1, "Audit log must record success");
}

#[tokio::test]
async fn audit_log_written_on_unknown_tool_denial() {
    let ctx = test_ctx().await;
    let req = make_request("nexus.unknown.tool", json!({}));
    let _ = HostToolExecutor::execute(&req, &ctx.state).await;

    let outcome = latest_audit_outcome(ctx.state.pool(), "nexus.unknown.tool").await;
    assert!(
        outcome.starts_with("denied:"),
        "Audit log must record denial, got: {outcome}"
    );
    assert!(
        outcome.contains("NOT_SUPPORTED"),
        "Audit log must contain NOT_SUPPORTED, got: {outcome}"
    );
}

#[tokio::test]
async fn audit_log_written_on_cross_creator_denial() {
    let ctx = test_ctx().await;
    let other_ctx = test_ctx_other_creator().await;
    let work_id = seed_work(&ctx.state).await;

    let req = make_request("nexus.work.get", json!({ "work_id": work_id }));
    let _ = HostToolExecutor::execute(&req, &other_ctx.state).await;

    let count = count_audit_rows(other_ctx.state.pool(), "nexus.work.get", "denied").await;
    assert_eq!(count, 1, "Audit log must record cross-creator denial");
}

#[tokio::test]
async fn audit_log_written_on_policy_blocked() {
    let ctx = test_ctx().await;
    let req = make_request(
        "nexus.context.assemble",
        json!({ "requires_platform": true }),
    );
    let _ = HostToolExecutor::execute(&req, &ctx.state).await;

    let outcome = latest_audit_outcome(ctx.state.pool(), "nexus.context.assemble").await;
    assert!(
        outcome.contains("denied:"),
        "Audit must record denial, got: {outcome}"
    );
}

#[tokio::test]
async fn audit_log_written_on_invalid_input() {
    let ctx = test_ctx().await;
    let req = make_request("nexus.work.get", json!({}));
    let _ = HostToolExecutor::execute(&req, &ctx.state).await;

    let count = count_audit_rows(ctx.state.pool(), "nexus.work.get", "denied").await;
    assert_eq!(count, 1, "Audit log must record invalid input denial");
}

// ─── Fix wave 2: stage_metadata sub-field allowlist ─────────────────────────

#[tokio::test]
async fn stage_metadata_accepts_allowed_keys() {
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    let req = make_request(
        "nexus.work.patch",
        json!({
            "work_id": work_id,
            "stage_metadata": {
                "agent_notes": "some notes",
                "research_summary_ref": "ref://123"
            }
        }),
    );
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_ok(), "Allowed stage_metadata keys should succeed");
}

#[tokio::test]
async fn stage_metadata_rejects_disallowed_sub_key() {
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    let req = make_request(
        "nexus.work.patch",
        json!({
            "work_id": work_id,
            "stage_metadata": {
                "current_stage": "writing"
            }
        }),
    );
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_code(), "INVALID_INPUT");
}

#[tokio::test]
async fn stage_metadata_rejects_unknown_sub_key() {
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    let req = make_request(
        "nexus.work.patch",
        json!({
            "work_id": work_id,
            "stage_metadata": {
                "malicious_field": "evil"
            }
        }),
    );
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_code(), "INVALID_INPUT");
}

#[tokio::test]
async fn stage_metadata_rejects_non_object() {
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    let req = make_request(
        "nexus.work.patch",
        json!({
            "work_id": work_id,
            "stage_metadata": "not-an-object"
        }),
    );
    let result = HostToolExecutor::execute(&req, &ctx.state).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.error_code(), "INVALID_INPUT");
}

// ─── Fix wave 2: Worker upcall path equivalence ─────────────────────────────

#[tokio::test]
async fn worker_upcall_whoami_equivalent_to_http() {
    let ctx = test_ctx().await;

    let http_result =
        HostToolExecutor::execute(&make_request("nexus.context.whoami", json!({})), &ctx.state)
            .await
            .expect("HTTP execute");

    let worker_result = HostToolExecutor::dispatch_from_worker(
        "nexus.context.whoami",
        &json!({}),
        "req-eq-001",
        &ctx.state,
    )
    .await;

    assert!(worker_result.grant);
    assert_eq!(worker_result.request_id, "req-eq-001");
    let output = worker_result.output.expect("worker should have output");
    assert_eq!(
        output, http_result,
        "HTTP and worker must produce same result"
    );
}

#[tokio::test]
async fn worker_upcall_schedule_status_equivalent_to_http() {
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;

    let http_result = HostToolExecutor::execute(
        &make_request(
            "nexus.orchestration.schedule_status",
            json!({ "work_id": work_id }),
        ),
        &ctx.state,
    )
    .await
    .expect("HTTP execute");

    let worker_result = HostToolExecutor::dispatch_from_worker(
        "nexus.orchestration.schedule_status",
        &json!({ "work_id": work_id }),
        "req-eq-002",
        &ctx.state,
    )
    .await;

    assert!(worker_result.grant);
    let output = worker_result.output.expect("worker output");
    assert_eq!(
        output, http_result,
        "HTTP and worker must produce same result"
    );
}
