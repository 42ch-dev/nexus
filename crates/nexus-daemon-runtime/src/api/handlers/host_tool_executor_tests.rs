//! Tests for host_tool_executor — extracted from host_tool_executor.rs (V1.57 P1)

use super::*;
use crate::test_utils::create_initialized_test_workspace;
use crate::test_utils::create_test_workspace;
use crate::workspace::WorkspaceState;

#[tokio::test]
async fn execute_rejects_unknown_tool() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let req = ToolExecuteRequest {
        tool_name: "unknown/tool".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn execute_rejects_read_without_path() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let req = ToolExecuteRequest {
        tool_name: "fs/read_text_file".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn execute_read_file_succeeds() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Write a temp file to read
    let temp = tempfile::NamedTempFile::new().expect("temp file");
    let path = temp.path().to_string_lossy().to_string();
    std::fs::write(temp.path(), "hello world").expect("write temp");

    let req = ToolExecuteRequest {
        tool_name: "fs/read_text_file".to_string(),
        parameters: serde_json::json!({ "path": path }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_ok());
    let val = result.expect("result");
    assert_eq!(val["content"], "hello world");
}

#[tokio::test]
async fn execute_write_file_succeeds() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let temp = tempfile::NamedTempFile::new().expect("temp file");
    let path = temp.path().to_string_lossy().to_string();

    let req = ToolExecuteRequest {
        tool_name: "fs/write_text_file".to_string(),
        parameters: serde_json::json!({ "path": path, "content": "written!" }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_ok());

    let content = std::fs::read_to_string(&path).expect("read back");
    assert_eq!(content, "written!");
}

#[tokio::test]
async fn whoami_returns_active_creator() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.context.whoami".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_ok());
    let val = result.expect("result");
    assert_eq!(val["creator_id"], "test_creator");
    assert_eq!(val["workspace_slug"], "default");
}

#[tokio::test]
async fn workspace_info_returns_details() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.workspace.info".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_ok());
    let val = result.expect("result");
    assert_eq!(val["creator_id"], "test_creator");
    assert_eq!(val["workspace_slug"], "default");
}

#[tokio::test]
async fn work_get_happy_path() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Create a work first
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::works::WorkRecord {
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
        schedule_ids: "[]".to_string(),
        created_at: now.clone(),
        updated_at: now,
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
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
    nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
        .await
        .expect("create work")
        .unwrap_err(); // Returns the new record in Err

    let req = ToolExecuteRequest {
        tool_name: "nexus.work.get".to_string(),
        parameters: serde_json::json!({ "work_id": work_id }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_ok());
    let val = result.expect("result");
    assert_eq!(val["work_id"], work_id);
    assert_eq!(val["title"], "Test Work");
}

#[tokio::test]
async fn work_patch_rejects_stage_field() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.work.patch".to_string(),
        parameters: serde_json::json!({
            "work_id": "wrk_test",
            "current_stage": "writing"
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    // Should be INVALID_INPUT — stable tool error code (spec §12.4)
    assert_eq!(err.error_code(), "INVALID_INPUT");
}

#[tokio::test]
async fn context_assemble_policy_blocked_when_local_only() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.context.assemble".to_string(),
        parameters: serde_json::json!({ "requires_platform": true }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    // Should be POLICY_BLOCKED
    match &result {
        Err(NexusApiError::BadRequest { code, message }) => {
            assert_eq!(code, "POLICY_BLOCKED");
            assert!(message.contains("PLATFORM_PAUSED"));
        }
        Err(e) => panic!("Expected BadRequest(POLICY_BLOCKED), got: {e:?}"),
        Ok(_) => panic!("Expected error"),
    }
}

/// Worker upcall dispatch hits the same registry as HTTP (spec §7.1).
#[tokio::test]
async fn worker_upcall_whoami_same_result_as_http() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let http_req = ToolExecuteRequest {
        tool_name: "nexus.context.whoami".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let http_result = HostToolExecutor::execute(&http_req, &state)
        .await
        .expect("HTTP execute");

    let worker_result = HostToolExecutor::dispatch_from_worker(
        "nexus.context.whoami",
        &serde_json::json!({}),
        "req-001",
        &state,
    )
    .await;

    assert!(worker_result.grant, "Worker upcall should succeed");
    assert_eq!(worker_result.request_id, "req-001");
    let output = worker_result.output.expect("worker should have output");
    assert_eq!(
        output, http_result,
        "HTTP and worker must produce same result"
    );
}

// ─── V1.53 P0 Sub-phase 1: Registry parity tests ───────────────────────

/// Parity test: old `execute()` and new `registry_dispatch()` produce
/// the same output for `nexus.context.whoami`.
#[tokio::test]
async fn registry_parity_whoami() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.context.whoami".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let old_result = HostToolExecutor::execute(&req, &state).await;
    let new_result = HostToolExecutor::registry_dispatch(&req, &state).await;
    assert_eq!(old_result.is_ok(), new_result.is_ok());
    if let (Ok(old_val), Ok(new_val)) = (&old_result, &new_result) {
        assert_eq!(old_val["creator_id"], new_val["creator_id"]);
        assert_eq!(old_val["workspace_slug"], new_val["workspace_slug"]);
    }
}

/// Parity test: old `execute()` and new `registry_dispatch()` produce
/// the same output for `nexus.workspace.info`.
#[tokio::test]
async fn registry_parity_workspace_info() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.workspace.info".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let old_result = HostToolExecutor::execute(&req, &state).await;
    let new_result = HostToolExecutor::registry_dispatch(&req, &state).await;
    assert_eq!(old_result.is_ok(), new_result.is_ok());
    if let (Ok(old_val), Ok(new_val)) = (&old_result, &new_result) {
        assert_eq!(old_val["creator_id"], new_val["creator_id"]);
        assert_eq!(old_val["workspace_slug"], new_val["workspace_slug"]);
        assert_eq!(old_val["workspace_path"], new_val["workspace_path"]);
    }
}

// ─── V1.53 P0 Sub-phase 2: Cutover verification ────────────────────────

/// Cutover test: `execute()` now routes through `registry_dispatch()`
/// internally, so both paths must produce identical output for every tool.
#[tokio::test]
async fn cutover_execute_equals_registry_dispatch() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Test with whoami (read-only, no DB setup needed)
    let req = ToolExecuteRequest {
        tool_name: "nexus.context.whoami".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let via_execute = HostToolExecutor::execute(&req, &state).await;
    let via_registry = HostToolExecutor::registry_dispatch(&req, &state).await;
    assert_eq!(via_execute.is_ok(), via_registry.is_ok());
    if let (Ok(e_val), Ok(r_val)) = (&via_execute, &via_registry) {
        assert_eq!(e_val, r_val, "execute() must route through registry");
    }
}

/// Cutover test: `execute()` rejects unknown tools through registry.
#[tokio::test]
async fn cutover_unknown_tool_rejected_by_registry() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nonexistent.nexus.capability".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    match result {
        Err(NexusApiError::BadRequest { code, .. }) => {
            assert_eq!(code, "NOT_SUPPORTED");
        }
        other => panic!("Expected BadRequest(NOT_SUPPORTED) via registry, got: {other:?}"),
    }
}

/// **R-V153P0QC1-001 enforcement**: `TOOL_ALLOWLIST` and
/// `CapabilityRegistry` rows must agree on every tool ID.
///
/// P1 will add 5 new `nexus.*` tools. This test ensures they cannot
/// be added to one list without the other — catching the drift risk
/// that qc1 identified.
#[test]
fn tool_allowlist_matches_registry_ids() {
    let reg = crate::capability_registry::host_tool_registry();
    let registry_ids: std::collections::HashSet<&str> = reg.ids().collect();
    let allowlist_ids: std::collections::HashSet<&str> = TOOL_ALLOWLIST.iter().copied().collect();

    // Every TOOL_ALLOWLIST entry must have a matching registry row
    for id in &allowlist_ids {
        assert!(
            registry_ids.contains(id),
            "TOOL_ALLOWLIST contains '{id}' but CapabilityRegistry has no row for it. \
                 Add the row to host_tool_registry() or remove the entry from TOOL_ALLOWLIST."
        );
    }

    // Every registry row must appear in TOOL_ALLOWLIST
    for id in &registry_ids {
        assert!(
            allowlist_ids.contains(id),
            "CapabilityRegistry row '{id}' is not in TOOL_ALLOWLIST. \
                 Add the id to TOOL_ALLOWLIST or remove the row from host_tool_registry()."
        );
    }
}

/// Parity test: old and new dispatch both reject unknown tools.
#[tokio::test]
async fn registry_parity_unknown_tool() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "unknown/tool".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let old_result = HostToolExecutor::execute(&req, &state).await;
    let new_result = HostToolExecutor::registry_dispatch(&req, &state).await;
    assert!(old_result.is_err());
    assert!(new_result.is_err());
    // Both should produce NOT_SUPPORTED
    match (&old_result, &new_result) {
        (Err(old_e), Err(new_e)) => {
            assert_eq!(old_e.error_code(), new_e.error_code());
        }
        _ => panic!("Both should be errors"),
    }
}

// ─── V1.53 P1: DF-46 read-heavy tool e2e tests ─────────────────────────

/// E2E test: `nexus.world.snapshot.get` returns world state for a seeded world.
#[tokio::test]
async fn world_snapshot_get_returns_world_state() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    // Seed a world
    crate::test_utils::seed_test_creator_and_world(state.pool()).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.world.snapshot.get".to_string(),
        parameters: serde_json::json!({"world_id": "wld_test_world"}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_ok(),
        "world.snapshot.get should succeed: {result:?}"
    );
    let val = result.expect("result");
    assert_eq!(val["world_id"], "wld_test_world");
    assert_eq!(val["title"], "Test World");
    drop(tmp);
}

/// Failure test: `nexus.world.snapshot.get` with missing world_id returns error.
#[tokio::test]
async fn world_snapshot_get_rejects_missing_world_id() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.world.snapshot.get".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
}

/// E2E test: `nexus.timeline.recent.get` returns events for a seeded world.
#[tokio::test]
async fn timeline_recent_get_returns_recent_events() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    // Seed world + timeline events via narrative gateway seed helpers
    let pool = state.pool().clone();
    nexus_local_db::narrative_gateway::seed::world(
        &pool,
        "wld_timeline",
        "test_creator",
        "Timeline World",
        "timeline-world",
        "private",
        "manual",
    )
    .await;
    nexus_local_db::narrative_gateway::seed::event(
        &pool,
        "evt_1",
        "wld_timeline",
        "fbk_root",
        "story_advance",
        1,
    )
    .await;
    nexus_local_db::narrative_gateway::seed::event(
        &pool,
        "evt_2",
        "wld_timeline",
        "fbk_root",
        "story_advance",
        2,
    )
    .await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.timeline.recent.get".to_string(),
        parameters: serde_json::json!({"world_id": "wld_timeline", "limit": 5}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_ok(),
        "timeline.recent.get should succeed: {result:?}"
    );
    let val = result.expect("result");
    let events = val.as_array().expect("should be an array");
    assert_eq!(events.len(), 2);
    drop(tmp);
}

/// E2E test: `nexus.kb_snapshot.read` returns key blocks for a seeded world.
#[tokio::test]
async fn kb_snapshot_read_returns_key_blocks() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    // Seed world + key blocks
    let pool = state.pool().clone();
    nexus_local_db::kb_store::seed::world(
        &pool,
        "wld_kb",
        "test_creator",
        "KB World",
        "kb-world",
        "private",
        "manual",
    )
    .await;
    nexus_local_db::kb_store::seed::key_block(
        &pool,
        "kb_1",
        "wld_kb",
        "character",
        "alice",
        "provisional",
    )
    .await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.kb_snapshot.read".to_string(),
        parameters: serde_json::json!({"world_id": "wld_kb"}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_ok(),
        "kb_snapshot.read should succeed: {result:?}"
    );
    let val = result.expect("result");
    let blocks = val.as_array().expect("should be an array");
    assert!(!blocks.is_empty(), "should return at least one key block");
    drop(tmp);
}

/// E2E test: `nexus.manuscript.chapter.get` returns chapter record.
#[tokio::test]
async fn manuscript_chapter_get_returns_chapter_record() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;

    // Create a work first, then seed chapters
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Test Novel".to_string(),
        long_term_goal: "Goal".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
        work_profile: None,
        work_ref: None,
        total_planned_chapters: Some(5),
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
    nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
        .await
        .expect("create work")
        .unwrap_err();

    // Seed chapters
    nexus_local_db::work_chapters::seed_chapters(state.pool(), &work_id, "test-novel", 5, &now)
        .await
        .expect("seed chapters");

    let req = ToolExecuteRequest {
        tool_name: "nexus.manuscript.chapter.get".to_string(),
        parameters: serde_json::json!({"work_id": work_id, "chapter": 1}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_ok(),
        "manuscript.chapter.get should succeed: {result:?}"
    );
    let val = result.expect("result");
    assert_eq!(val["work_id"], work_id);
    assert_eq!(val["chapter"], 1);
    drop(tmp);
}

/// E2E test: `nexus.observability.daemon.health` returns runtime status.
#[tokio::test]
async fn daemon_health_returns_registry_status() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.observability.daemon.health".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_ok(), "daemon.health should succeed: {result:?}");
    let val = result.expect("result");
    assert!(val["uptime_seconds"].as_u64().is_some());
    assert_eq!(val["runtime_mode"], "local_only");
    assert_eq!(val["registry_size"], 20);
    assert!(val["pool_healthy"].as_bool().unwrap_or(false));
    assert_eq!(
        val["registry_ids"].as_array().expect("registry_ids").len(),
        20
    );
}

// ─── P0 residual closure: registry dispatch regression tests (R-V153P0QC2-001) ──
// Backfill registry_dispatch parity for V1.34 tools that lacked parity tests.

#[tokio::test]
async fn registry_dispatch_returns_same_as_legacy_work_get() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Create a work
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Legacy Work".to_string(),
        long_term_goal: "Goal".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: now.clone(),
        updated_at: now,
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
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
    nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
        .await
        .expect("create work")
        .unwrap_err();

    let req = ToolExecuteRequest {
        tool_name: "nexus.work.get".to_string(),
        parameters: serde_json::json!({"work_id": work_id}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    // Both execute() and registry_dispatch() should produce the same result.
    // Since Sub-phase 2 cutover, execute() routes through registry_dispatch(),
    // so they are the same path. This test guards against regression.
    let via_execute = HostToolExecutor::execute(&req, &state).await;
    let via_registry = HostToolExecutor::registry_dispatch(&req, &state).await;
    assert_eq!(via_execute.is_ok(), via_registry.is_ok());
    if let (Ok(e_val), Ok(r_val)) = (&via_execute, &via_registry) {
        assert_eq!(e_val["work_id"], r_val["work_id"]);
        assert_eq!(e_val["title"], r_val["title"]);
    }
}

#[tokio::test]
async fn registry_dispatch_returns_same_as_legacy_work_patch() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.work.patch".to_string(),
        parameters: serde_json::json!({
            "work_id": "wrk_test",
            "current_stage": "writing"
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let via_execute = HostToolExecutor::execute(&req, &state).await;
    let via_registry = HostToolExecutor::registry_dispatch(&req, &state).await;
    assert!(via_execute.is_err());
    assert!(via_registry.is_err());
    assert_eq!(
        via_execute.as_ref().unwrap_err().error_code(),
        via_registry.as_ref().unwrap_err().error_code()
    );
}

#[tokio::test]
async fn schedule_status_happy_path() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Create a work with schedule_ids
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Scheduled Work".to_string(),
        long_term_goal: "Goal".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: r#"["sch_001","sch_002"]"#.to_string(),
        created_at: now.clone(),
        updated_at: now,
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
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
    nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
        .await
        .expect("create work")
        .unwrap_err();

    let req = ToolExecuteRequest {
        tool_name: "nexus.orchestration.schedule_status".to_string(),
        parameters: serde_json::json!({"work_id": work_id}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_ok(), "schedule_status should succeed: {result:?}");
    let val = result.expect("result");
    assert_eq!(val["work_id"], work_id);
    assert_eq!(val["count"], 2);
}

#[tokio::test]
async fn registry_dispatch_returns_same_as_legacy_context_assemble() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.context.assemble".to_string(),
        parameters: serde_json::json!({"requires_platform": true}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let via_execute = HostToolExecutor::execute(&req, &state).await;
    let via_registry = HostToolExecutor::registry_dispatch(&req, &state).await;
    assert_eq!(via_execute.is_ok(), via_registry.is_ok());
    if let (Err(e_val), Err(r_val)) = (&via_execute, &via_registry) {
        assert_eq!(e_val.error_code(), r_val.error_code());
    }
}

// ─── V1.53 P1: Cross-creator/world isolation tests (R-V153P1QC1-001) ──

/// Helper: overwrite the active creator in config.toml and return a new
/// WorkspaceState (same db) with that identity.
async fn switch_active_creator(
    nexus_home: &std::path::Path,
    db_path: &std::path::Path,
    new_creator_id: &str,
) -> WorkspaceState {
    let toml_str = format!(
            "active_creator_id = \"{new_creator_id}\"\n[active_workspace_slug_by_creator]\n\"{new_creator_id}\" = \"default\""
        );
    std::fs::write(nexus_home.join("config.toml"), toml_str).expect("write config.toml");
    WorkspaceState::new_for_testing(nexus_home.to_path_buf(), db_path.to_path_buf(), None).await
}

#[tokio::test]
async fn world_snapshot_get_cross_creator_denied() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    crate::test_utils::seed_test_creator_and_world(state.pool()).await;
    // Seed another creator
    // SAFETY: test-only data setup.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(state.pool())
    .await
    .expect("seed other creator");

    // Switch to other_creator — should be denied
    let other_state = switch_active_creator(&nexus_home, &db_path, "other_creator").await;
    let req = ToolExecuteRequest {
        tool_name: "nexus.world.snapshot.get".to_string(),
        parameters: serde_json::json!({"world_id": "wld_test_world"}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &other_state).await;
    assert!(result.is_err(), "cross-creator should be denied");
    assert_eq!(
        result.unwrap_err().error_code(),
        "FORBIDDEN",
        "should return FORBIDDEN for cross-creator access"
    );
    drop(tmp);
}

#[tokio::test]
async fn timeline_recent_get_cross_creator_denied() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    crate::test_utils::seed_test_creator_and_world(state.pool()).await;
    // Seed other creator
    // SAFETY: test-only.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(state.pool())
    .await
    .expect("seed other creator");

    let other_state = switch_active_creator(&nexus_home, &db_path, "other_creator").await;
    let req = ToolExecuteRequest {
        tool_name: "nexus.timeline.recent.get".to_string(),
        parameters: serde_json::json!({"world_id": "wld_test_world"}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &other_state).await;
    assert!(result.is_err(), "cross-creator should be denied");
    assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
    drop(tmp);
}

#[tokio::test]
async fn kb_snapshot_read_cross_creator_denied() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    crate::test_utils::seed_test_creator_and_world(state.pool()).await;
    // SAFETY: test-only.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(state.pool())
    .await
    .expect("seed other creator");

    let other_state = switch_active_creator(&nexus_home, &db_path, "other_creator").await;
    let req = ToolExecuteRequest {
        tool_name: "nexus.kb_snapshot.read".to_string(),
        parameters: serde_json::json!({"world_id": "wld_test_world"}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &other_state).await;
    assert!(result.is_err(), "cross-creator should be denied");
    assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
    drop(tmp);
}

// ─── V1.53 P1: Failure/admission test coverage (R-V153P1QC1-002) ──

#[tokio::test]
async fn timeline_recent_get_rejects_missing_world_id() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.timeline.recent.get".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
}

#[tokio::test]
async fn kb_snapshot_read_rejects_missing_world_id() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.kb_snapshot.read".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
}

#[tokio::test]
async fn manuscript_chapter_get_rejects_missing_chapter_id() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.manuscript.chapter.get".to_string(),
        parameters: serde_json::json!({"work_id": "nonexistent_work", "chapter": 1}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
}

#[tokio::test]
async fn daemon_health_rejects_without_active_creator() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    // Remove active_creator_id from config
    let toml_str = "[active_workspace_slug_by_creator]\n";
    std::fs::write(nexus_home.join("config.toml"), toml_str).expect("write config.toml");
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.observability.daemon.health".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_err(),
        "daemon.health should require active creator"
    );
    assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
    drop(tmp);
}

// ─── V1.53 P1: Timeline limit test (R-V153P1QC3-001) ──

#[tokio::test]
async fn timeline_recent_get_respects_server_limit() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    let pool = state.pool().clone();
    nexus_local_db::narrative_gateway::seed::world(
        &pool,
        "wld_limit",
        "test_creator",
        "Limit World",
        "limit-world",
        "private",
        "manual",
    )
    .await;
    // Seed 5 timeline events
    for i in 1..=5 {
        let evt_id = format!("evt_limit_{i}");
        nexus_local_db::narrative_gateway::seed::event(
            &pool,
            &evt_id,
            "wld_limit",
            "fbk_root",
            "story_advance",
            i,
        )
        .await;
    }

    // Request with limit=2 → should get only 2 events (the most recent 2)
    let req = ToolExecuteRequest {
        tool_name: "nexus.timeline.recent.get".to_string(),
        parameters: serde_json::json!({"world_id": "wld_limit", "limit": 2}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_ok(),
        "timeline with limit should succeed: {result:?}"
    );
    let val = result.expect("result");
    let events = val.as_array().expect("should be an array");
    assert_eq!(events.len(), 2, "should return exactly 2 events");
    assert_eq!(events[0]["sequence_no"], 4);
    assert_eq!(events[1]["sequence_no"], 5);
    drop(tmp);
}

#[tokio::test]
async fn timeline_recent_get_clamps_limit_to_500() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    let pool = state.pool().clone();
    nexus_local_db::narrative_gateway::seed::world(
        &pool,
        "wld_clamp",
        "test_creator",
        "Clamp World",
        "clamp-world",
        "private",
        "manual",
    )
    .await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.timeline.recent.get".to_string(),
        parameters: serde_json::json!({"world_id": "wld_clamp", "limit": 10000}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    // Should succeed but effective limit is capped at 500
    assert!(result.is_ok(), "clamped limit should succeed: {result:?}");
    let val = result.expect("result");
    let events = val.as_array().expect("should be an array");
    assert!(
        events.len() <= 500,
        "should be capped at 500 events, got {}",
        events.len()
    );
    drop(tmp);
}

// ─── V1.54 P0: DF-46 write-tool hermetic tests (T10) ──────────────────

// --- nexus.kb_snapshot.write (3 tests) ---

#[tokio::test]
async fn kb_snapshot_write_upserts_key_blocks() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    crate::test_utils::seed_test_creator_and_world(state.pool()).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.kb_snapshot.write".to_string(),
        parameters: serde_json::json!({
            "world_id": "wld_test_world",
            "blocks": [{
                "schema_version": 1,
                "key_block_id": "kb_write_1",
                "world_id": "wld_test_world",
                "block_type": "character",
                "canonical_name": "test_character",
                "status": "provisional",
                "body": {"name": "Test Char"},
                "created_at": "2026-01-01T00:00:00Z"
            }]
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_ok(),
        "kb_snapshot.write should succeed: {result:?}"
    );
    let val = result.expect("result");
    assert_eq!(val["written"], 1);
    assert_eq!(val["world_id"], "wld_test_world");
    drop(tmp);
}

#[tokio::test]
async fn kb_snapshot_write_rejects_missing_world_id() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.kb_snapshot.write".to_string(),
        parameters: serde_json::json!({"blocks": []}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
}

#[tokio::test]
async fn kb_snapshot_write_rejects_unknown_tool_variant() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.kb_snapshot.write_nonexistent".to_string(),
        parameters: serde_json::json!({"world_id": "wld_test"}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "NOT_SUPPORTED");
}

/// C-001 regression: same-creator, block with wrong world_id → rejection.
#[tokio::test]
async fn kb_snapshot_write_rejects_cross_world_block_same_creator() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    crate::test_utils::seed_test_creator_and_world(state.pool()).await;
    // Seed a second world owned by same creator
    // SAFETY: test-only data setup.
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES ('wld_other_world', 'ws', 'test_creator', 'Other World', 'other-world', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .execute(state.pool())
    .await
    .expect("seed second world");

    let req = ToolExecuteRequest {
        tool_name: "nexus.kb_snapshot.write".to_string(),
        parameters: serde_json::json!({
            "world_id": "wld_test_world",
            "blocks": [{
                "schema_version": 1,
                "key_block_id": "kb_cross_world_block",
                "world_id": "wld_other_world",  // mismatched!
                "block_type": "character",
                "canonical_name": "cross_world_char",
                "status": "provisional",
                "body": {"name": "Cross-world Char"},
                "created_at": "2026-01-01T00:00:00Z"
            }]
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_err(),
        "cross-world block should be rejected: {result:?}"
    );
    assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
    drop(tmp);
}

/// C-001 regression: cross-creator world embedded in block → rejection.
#[tokio::test]
async fn kb_snapshot_write_rejects_cross_creator_world_block() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    crate::test_utils::seed_test_creator_and_world(state.pool()).await;
    // Seed a world owned by a different creator
    // SAFETY: test-only data setup.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('other_creator', 'Other Creator', 'active', datetime('now'), '{}')",
    )
    .execute(state.pool())
    .await
    .expect("seed other creator");
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES ('wld_other_creator_world', 'ws', 'other_creator', 'Other Creator World', \
             'other-creator-world', 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .execute(state.pool())
    .await
    .expect("seed other creator world");

    let req = ToolExecuteRequest {
        tool_name: "nexus.kb_snapshot.write".to_string(),
        parameters: serde_json::json!({
            "world_id": "wld_test_world",
            "blocks": [{
                "schema_version": 1,
                "key_block_id": "kb_cross_creator_block",
                "world_id": "wld_other_creator_world",  // different creator's world
                "block_type": "character",
                "canonical_name": "cross_creator_char",
                "status": "provisional",
                "body": {"name": "Cross-creator Char"},
                "created_at": "2026-01-01T00:00:00Z"
            }]
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_err(),
        "cross-creator block should be rejected: {result:?}"
    );
    assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
    drop(tmp);
}

// --- nexus.manuscript.chapter.update (3 tests) ---

#[tokio::test]
async fn manuscript_chapter_update_writes_content() {
    let (tmp, nexus_home, db_path, workspace_dir) = create_initialized_test_workspace().await;
    let state = WorkspaceState::new_for_testing(
        nexus_home.clone(),
        db_path,
        Some(workspace_dir.to_string_lossy().to_string()),
    )
    .await;

    // Create a work and seed chapters
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Chapter Update Test".to_string(),
        long_term_goal: "Goal".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
        work_profile: None,
        work_ref: None,
        total_planned_chapters: Some(3),
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
    nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
        .await
        .expect("create work")
        .unwrap_err();
    nexus_local_db::work_chapters::seed_chapters(state.pool(), &work_id, "test-update", 3, &now)
        .await
        .expect("seed chapters");

    let req = ToolExecuteRequest {
        tool_name: "nexus.manuscript.chapter.update".to_string(),
        parameters: serde_json::json!({
            "work_id": work_id,
            "chapter": 1,
            "content": "Updated chapter content for testing."
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_ok(),
        "manuscript.chapter.update should succeed: {result:?}"
    );
    let val = result.expect("result");
    assert_eq!(val["work_id"], work_id);
    assert_eq!(val["chapter"], 1);
    // C-002 atomicity: verify DB body_path exists and the file on disk
    // contains the content we wrote (proves file written iff DB committed).
    let chapter_record = nexus_local_db::work_chapters::get_chapter(state.pool(), &work_id, 1, 1)
        .await
        .expect("get_chapter after update")
        .expect("chapter should exist after update");
    let db_body_path = chapter_record.body_path.expect("body_path should be set");
    // W-003: verify canonical path follows Works/{work_ref}/Stories/... pattern.
    assert!(
        db_body_path.starts_with("Works/"),
        "body_path should start with Works/, got: {db_body_path}"
    );
    assert!(
        db_body_path.contains("Stories/"),
        "body_path should use Stories/ layout, got: {db_body_path}"
    );
    assert!(
        db_body_path.ends_with(".md"),
        "body_path should end with .md, got: {db_body_path}"
    );
    assert!(
        !db_body_path.ends_with(".tmp"),
        "body_path should be the final file, not a .tmp: {db_body_path}"
    );
    // W-003: db_body_path is a relative canonical path; resolve to absolute.
    let on_disk_path = workspace_dir.join(&db_body_path);
    let on_disk = tokio::fs::read_to_string(&on_disk_path)
        .await
        .expect("file should exist on disk");
    assert_eq!(
        on_disk, "Updated chapter content for testing.",
        "file content should match what was written"
    );
    drop(tmp);
}

#[tokio::test]
async fn manuscript_chapter_update_rejects_missing_chapter() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.manuscript.chapter.update".to_string(),
        parameters: serde_json::json!({"work_id": "wrk_test"}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
}

#[tokio::test]
async fn manuscript_chapter_update_rejects_unknown_tool_variant() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.manuscript.chapter.update_v2".to_string(),
        parameters: serde_json::json!({"work_id": "wrk_test", "chapter": 1}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "NOT_SUPPORTED");
}

// --- nexus.world.configure (3 tests) ---

#[tokio::test]
async fn world_configure_updates_metadata() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    crate::test_utils::seed_test_creator_and_world(state.pool()).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.world.configure".to_string(),
        parameters: serde_json::json!({
            "world_id": "wld_test_world",
            "title": "Renamed World",
            "visibility": "public"
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_ok(), "world.configure should succeed: {result:?}");
    let val = result.expect("result");
    assert_eq!(val["world_id"], "wld_test_world");
    assert_eq!(val["updated"], true);
    drop(tmp);
}

#[tokio::test]
async fn world_configure_rejects_invalid_visibility() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
    crate::test_utils::seed_test_creator_and_world(state.pool()).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.world.configure".to_string(),
        parameters: serde_json::json!({
            "world_id": "wld_test_world",
            "visibility": "top_secret"
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
    drop(tmp);
}

#[tokio::test]
async fn world_configure_rejects_missing_world_id() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.world.configure".to_string(),
        parameters: serde_json::json!({"title": "No World"}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
}

// --- nexus.work.schedule.set (3 tests) ---

#[tokio::test]
async fn work_schedule_set_links_schedules() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Create a work
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Schedule Test".to_string(),
        long_term_goal: "Goal".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: now.clone(),
        updated_at: now,
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
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
    nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
        .await
        .expect("create work")
        .unwrap_err();

    let req = ToolExecuteRequest {
        tool_name: "nexus.work.schedule.set".to_string(),
        parameters: serde_json::json!({
            "work_id": work_id,
            "schedule_ids": ["sch_a", "sch_b"]
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_ok(),
        "work.schedule.set should succeed: {result:?}"
    );
    let val = result.expect("result");
    assert_eq!(val["work_id"], work_id);
    let ids = val["schedule_ids"].as_array().expect("schedule_ids array");
    assert_eq!(ids.len(), 2);
}

#[tokio::test]
async fn work_schedule_set_rejects_non_string_ids() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.work.schedule.set".to_string(),
        parameters: serde_json::json!({
            "work_id": "wrk_test",
            "schedule_ids": [1, 2, 3]
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
}

#[tokio::test]
async fn work_schedule_set_rejects_missing_schedule_ids() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.work.schedule.set".to_string(),
        parameters: serde_json::json!({"work_id": "wrk_test"}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
}

// --- nexus.finding.resolve (3 tests) ---

#[tokio::test]
async fn finding_resolve_marks_resolved() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Create a work first for FK constraint
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Findings Test".to_string(),
        long_term_goal: "Goal".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: now.clone(),
        updated_at: now,
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
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
    nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
        .await
        .expect("create work")
        .unwrap_err();

    // Seed a finding
    let finding_id = format!("fnd_{}", uuid::Uuid::new_v4());
    let now_epoch = chrono::Utc::now().timestamp();
    // SAFETY: test-only data setup.
    sqlx::query(
        "INSERT INTO findings (finding_id, work_id, chapter, severity, status, \
             title, description, target_executor, creator_id, created_at, updated_at) \
             VALUES (?, ?, 1, 'minor', 'open', \
             'Test Finding', 'A test finding', 'none', 'test_creator', ?, ?)",
    )
    .bind(&finding_id)
    .bind(&work_id)
    .bind(now_epoch)
    .bind(now_epoch)
    .execute(state.pool())
    .await
    .expect("seed finding");

    let req = ToolExecuteRequest {
        tool_name: "nexus.finding.resolve".to_string(),
        parameters: serde_json::json!({
            "finding_id": finding_id,
            "resolution": "Fixed in code"
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_ok(), "finding.resolve should succeed: {result:?}");
    let val = result.expect("result");
    assert_eq!(val["finding_id"], finding_id);
    assert_eq!(val["resolved"], true);
}

/// W-002: nonexistent finding IDs must return NOT_FOUND, not success.
#[tokio::test]
async fn finding_resolve_nonexistent_returns_not_found() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.finding.resolve".to_string(),
        parameters: serde_json::json!({"finding_id": "fnd_nonexistent_99999"}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_err(),
        "finding.resolve should reject nonexistent finding: {result:?}"
    );
    assert_eq!(result.unwrap_err().error_code(), "NOT_FOUND");
}

// --- nexus.pool.entry.manage (3 tests) ---

#[tokio::test]
async fn pool_entry_manage_adds_to_pool() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Create a work
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Pool Test Work".to_string(),
        long_term_goal: "Goal".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: now.clone(),
        updated_at: now,
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
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
    nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
        .await
        .expect("create work")
        .unwrap_err();

    let req = ToolExecuteRequest {
        tool_name: "nexus.pool.entry.manage".to_string(),
        parameters: serde_json::json!({
            "work_id": work_id,
            "action": "add"
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(
        result.is_ok(),
        "pool.entry.manage should succeed: {result:?}"
    );
    let val = result.expect("result");
    assert_eq!(val["work_id"], work_id);
    assert_eq!(val["action"], "add");
    assert_eq!(val["success"], true);
}

#[tokio::test]
async fn pool_entry_manage_rejects_invalid_action() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Create a work so the ownership check passes
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Pool Invalid Test".to_string(),
        long_term_goal: "Goal".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: now.clone(),
        updated_at: now,
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
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
    nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
        .await
        .expect("create work")
        .unwrap_err();

    let req = ToolExecuteRequest {
        tool_name: "nexus.pool.entry.manage".to_string(),
        parameters: serde_json::json!({
            "work_id": work_id,
            "action": "destroy"
        }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
}

#[tokio::test]
async fn pool_entry_manage_rejects_missing_action() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.pool.entry.manage".to_string(),
        parameters: serde_json::json!({"work_id": "wrk_test"}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
}

// ─── Cross-cutting tests (R-V153P0QC2-003/004) ──

/// Concurrent dispatch: 10 parallel invocations of `nexus.context.whoami`
/// through `registry_dispatch()` — verifies no deadlock/data race on
/// `LazyLock<CapabilityRegistry>`.
#[tokio::test]
async fn concurrent_dispatch_ten_parallel_whoami() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let state = std::sync::Arc::new(state);

    let mut handles = Vec::new();
    for i in 0..10 {
        let state = state.clone();
        handles.push(tokio::spawn(async move {
            let req = ToolExecuteRequest {
                tool_name: "nexus.context.whoami".to_string(),
                parameters: serde_json::json!({}),
                session_id: Some(format!("sess_{i}")),
                request_id: Some(format!("req_{i}")),
                caller_kind: None,
            };
            HostToolExecutor::registry_dispatch(&req, &state).await
        }));
    }

    for handle in handles {
        let result = handle.await.expect("no panic");
        assert!(
            result.is_ok(),
            "concurrent dispatch should succeed: {result:?}"
        );
        let val = result.expect("result");
        assert_eq!(val["creator_id"], "test_creator");
    }
}

/// W-003(qc3): concurrent write-tool dispatch — 10 parallel
/// `nexus.pool.entry.manage` create calls plus 10 concurrent reads
/// through `registry_dispatch()`. Verifies no deadlock/data race on
/// transaction contention for write tools.
#[tokio::test]
async fn concurrent_dispatch_ten_parallel_write_tools() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Create a work for FK constraint
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Concurrent Write Test".to_string(),
        long_term_goal: "Goal".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: now.clone(),
        updated_at: now.clone(),
        current_stage: "intake".to_string(),
        stage_status: "pending".to_string(),
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
    nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
        .await
        .expect("create work")
        .unwrap_err();
    let state = std::sync::Arc::new(state);

    let mut handles = Vec::new();
    // 5 write handles (pool.entry.manage create)
    for i in 0..5 {
        let state = state.clone();
        let wid = work_id.clone();
        handles.push(tokio::spawn(async move {
            let req = ToolExecuteRequest {
                tool_name: "nexus.pool.entry.manage".to_string(),
                parameters: serde_json::json!({
                    "work_id": wid,
                    "action": "add",
                    "pool_type": "ideas",
                    "content": format!("concurrent entry {i}"),
                }),
                session_id: Some(format!("sess_write_{i}")),
                request_id: Some(format!("req_write_{i}")),
                caller_kind: None,
            };
            HostToolExecutor::registry_dispatch(&req, &state).await
        }));
    }
    // 5 read handles (whoami — read-only, verifies LazyLock works under
    // concurrent write+read pressure).
    for i in 5..10 {
        let state = state.clone();
        handles.push(tokio::spawn(async move {
            let req = ToolExecuteRequest {
                tool_name: "nexus.context.whoami".to_string(),
                parameters: serde_json::json!({}),
                session_id: Some(format!("sess_read_{i}")),
                request_id: Some(format!("req_read_{i}")),
                caller_kind: None,
            };
            HostToolExecutor::registry_dispatch(&req, &state).await
        }));
    }

    for handle in handles {
        let result = handle.await.expect("no panic");
        assert!(
            result.is_ok(),
            "concurrent write dispatch should succeed: {result:?}"
        );
    }
}

/// Schedule caller-kind admission: `dispatch_for_schedule` produces
/// the same result as direct `execute()` for `nexus.context.whoami`.
#[tokio::test]
async fn schedule_caller_kind_same_result_as_direct_execute() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let direct_result = HostToolExecutor::execute(
        &ToolExecuteRequest {
            tool_name: "nexus.context.whoami".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        },
        &state,
    )
    .await
    .expect("direct execute");

    let schedule_result = HostToolExecutor::dispatch_for_schedule(
        "nexus.context.whoami",
        &serde_json::json!({}),
        "req-sch-001",
        &state,
    )
    .await
    .expect("schedule dispatch");

    assert_eq!(
        direct_result["creator_id"], schedule_result["creator_id"],
        "schedule dispatch should produce same creator_id"
    );
    assert_eq!(
        direct_result["workspace_slug"], schedule_result["workspace_slug"],
        "schedule dispatch should produce same workspace_slug"
    );
}

/// C-001 (qc3): Verify audit-log failure is propagated, not silently swallowed.
///
/// Drops the `acp_tool_audit_log` table before calling `registry_dispatch`
/// to simulate an audit write failure. The dispatch must return an
/// `Internal` error with code `AUDIT_LOG_FAILED` rather than silently
/// succeeding.
#[tokio::test]
async fn registry_dispatch_propagates_audit_write_failure() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;

    // Simulate audit-write failure by dropping the audit table before
    // constructing WorkspaceState. This causes the INSERT in
    // `audit_tool_execution` to fail with a SQLite error.
    {
        let audit_pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool for table drop");
        sqlx::query("DROP TABLE IF EXISTS acp_tool_audit_log")
            .execute(&audit_pool)
            .await
            .expect("drop acp_tool_audit_log");
        audit_pool.close().await;
    }

    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Use whoami — a read-only tool that passes admission and would
    // normally succeed. If the audit write fails, the dispatch must
    // propagate that failure.
    let req = ToolExecuteRequest {
        tool_name: "nexus.context.whoami".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };

    let result = HostToolExecutor::registry_dispatch(&req, &state).await;

    match result {
        Err(NexusApiError::Internal { code, .. }) => {
            assert_eq!(
                code, "AUDIT_LOG_FAILED",
                "audit write failure must propagate with code AUDIT_LOG_FAILED"
            );
        }
        other => {
            panic!("expected NexusApiError::Internal with code AUDIT_LOG_FAILED, got: {other:?}")
        }
    }
}

// ─── V1.57 P1: 3-caller integration tests ─────────────────────────────

/// CLI entry point test: `HostToolExecutor::execute()` dispatches through
/// the capability registry for a read tool.
#[tokio::test]
async fn test_host_call_dispatches_through_registry_read() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // CLI/HTTP path: execute() normalizes ToolExecuteRequest → registry dispatch
    let req = ToolExecuteRequest {
        tool_name: "nexus.context.whoami".to_string(),
        parameters: serde_json::json!({}),
        session_id: Some("test-cli-session".to_string()),
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_ok(), "CLI entry should succeed: {result:?}");
    let val = result.unwrap();
    assert_eq!(val["creator_id"], "test_creator");
}

/// Worker entry point test: `dispatch_from_worker()` dispatches through
/// the same registry as HTTP/CLI.
#[tokio::test]
async fn test_worker_agent_tool_request_dispatches_through_registry() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let worker_result = HostToolExecutor::dispatch_from_worker(
        "nexus.context.whoami",
        &serde_json::json!({}),
        "req-worker-001",
        &state,
    )
    .await;

    assert!(worker_result.grant, "Worker upcall should succeed");
    assert_eq!(worker_result.request_id, "req-worker-001");
    let output = worker_result.output.expect("worker should have output");
    assert_eq!(output["creator_id"], "test_creator");
}

/// HTTP entry point test: `execute()` → same registry dispatch as worker/CLI.
#[tokio::test]
async fn test_http_tool_execute_dispatches_through_registry() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // HTTP path: same execute() as CLI
    let req = ToolExecuteRequest {
        tool_name: "nexus.context.whoami".to_string(),
        parameters: serde_json::json!({}),
        session_id: Some("test-http-session".to_string()),
        request_id: None,
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    assert!(result.is_ok(), "HTTP entry should succeed");
    let val = result.unwrap();
    assert_eq!(val["creator_id"], "test_creator");
}

/// Dispatch equivalence: same tool_id + input → same output across all 3 paths.
#[tokio::test]
async fn test_dispatch_equivalence_all_three_paths() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Path 1: CLI/HTTP
    let req = ToolExecuteRequest {
        tool_name: "nexus.context.whoami".to_string(),
        parameters: serde_json::json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let http_result = HostToolExecutor::execute(&req, &state)
        .await
        .expect("HTTP path");

    // Path 2: Worker
    let worker_result = HostToolExecutor::dispatch_from_worker(
        "nexus.context.whoami",
        &serde_json::json!({}),
        "req-equiv",
        &state,
    )
    .await;
    assert!(worker_result.grant);
    let worker_output = worker_result.output.expect("worker output");

    // Path 3: Schedule
    let schedule_result = HostToolExecutor::dispatch_for_schedule(
        "nexus.context.whoami",
        &serde_json::json!({}),
        "req-sch-equiv",
        &state,
    )
    .await
    .expect("schedule path");

    // All three paths produce identical creator_id
    assert_eq!(http_result["creator_id"], worker_output["creator_id"]);
    assert_eq!(http_result["creator_id"], schedule_result["creator_id"]);
    assert_eq!(worker_output["creator_id"], schedule_result["creator_id"]);
}

// ─── V1.57 P3: Worker allowlist dynamic derivation ─────────────────────────

/// All 18 `nexus.*` host tool IDs that the worker must support.
///
/// V1.57 P3: Derived from `CapabilityRegistry`. Previously (V1.42 P3) the
/// worker was limited to a single ID (`nexus.orchestration.schedule_status`).
/// Now all 18 shipped `nexus.*` IDs are dispatchable via worker IPC.
const ALL_NEXUS_TOOL_IDS: &[&str] = &[
    "nexus.context.whoami",
    "nexus.workspace.info",
    "nexus.work.get",
    "nexus.work.patch",
    "nexus.orchestration.schedule_status",
    "nexus.context.assemble",
    "nexus.world.snapshot.get",
    "nexus.timeline.recent.get",
    "nexus.kb_snapshot.read",
    "nexus.manuscript.chapter.get",
    "nexus.observability.daemon.health",
    "nexus.kb_snapshot.write",
    "nexus.manuscript.chapter.update",
    "nexus.world.configure",
    "nexus.work.schedule.set",
    "nexus.finding.resolve",
    "nexus.pool.entry.manage",
    "nexus.registry.refresh",
];

/// V1.57 P3: Every `nexus.*` tool in the registry is dispatchable via
/// worker `agent_tool_request` IPC.
///
/// Verifies that the worker entry point (`dispatch_from_worker`) accepts
/// all 18 shipped `nexus.*` host tool IDs. The worker normalizes
/// `agent_tool_request` into `ToolExecuteRequest` and dispatches through
/// the same registry as CLI/HTTP (per P1's 3-caller unification).
///
/// Some tools require seeded DB state to return success (e.g. `nexus.work.get`
/// needs a work record). This test only verifies the **admission gate** —
/// that the tool ID is recognized (not `NOT_SUPPORTED`) and the active-creator
/// check passes. Handlers that fail with `INVALID_INPUT` or `NOT_FOUND` are
/// still considered "dispatchable" because the registry looked them up.
#[tokio::test]
async fn test_worker_dispatches_all_registered_nexus_tools() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    for &tool_id in ALL_NEXUS_TOOL_IDS {
        let result = HostToolExecutor::dispatch_from_worker(
            tool_id,
            &serde_json::json!({}),
            &format!("req-{tool_id}"),
            &state,
        )
        .await;

        // The tool must NOT return NOT_SUPPORTED (the allowlist check passed).
        // It may return other errors (INVALID_INPUT, NOT_FOUND, FORBIDDEN)
        // depending on required DB state — those are handler-level failures,
        // not admission failures.
        if let Some(err) = &result.error {
            assert_ne!(
                err.code, "NOT_SUPPORTED",
                "Worker rejected registered tool '{tool_id}' as NOT_SUPPORTED"
            );
        }
        // If grant=true, the dispatch succeeded. If grant=false with a
        // non-NOT_SUPPORTED error, the handler was found but input/state
        // was insufficient — that's still a successful dispatch lookup.
    }
}

/// V1.57 P3: Unknown tool IDs return NOT_SUPPORTED via worker IPC.
///
/// Confirms admission gate equivalence: an unknown ID is rejected the
/// same way on the worker path as on CLI/HTTP.
#[tokio::test]
async fn test_worker_rejects_unknown_tool() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let result = HostToolExecutor::dispatch_from_worker(
        "nexus.nonexistent.tool",
        &serde_json::json!({}),
        "req-unknown",
        &state,
    )
    .await;

    assert!(!result.grant, "Unknown tool should not be granted");
    let err = result.error.expect("Unknown tool must produce error");
    assert_eq!(err.code, "NOT_SUPPORTED");
}
