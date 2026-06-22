//! Cross-caller E2E test harness — V1.57 P3
//!
//! Verifies dispatch equivalence across all 3 caller entry points
//! (CLI/HTTP, Worker, Schedule) for all 18 shipped `nexus.*` host tool IDs.
//!
//! # Coverage
//!
//! - 18 `nexus.*` IDs × 3 caller paths = 54 invocation cases.
//! - Output equivalence: same `(ID, input)` → same result (success/failure) across all 3 paths.
//! - Admission gate equivalence: a request rejected on one path is rejected on all 3 paths.
//! - Unknown tool rejection: `NOT_SUPPORTED` consistent across all 3 paths.
//!
//! # Reconciliation
//!
//! The plan stub estimated 35 IDs but the actual `capability::Registry` has
//! 18 `nexus.*` host tools. Updated from 105 to 54 cases.

#![allow(clippy::unwrap_used)]

use nexus_daemon_runtime::api::errors::NexusApiError;
use nexus_daemon_runtime::api::handlers::host_tool_executor::{
    HostToolExecutor, ToolExecuteRequest,
};
use nexus_daemon_runtime::capability_registry::host_tool_registry;
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::json;

// ─── 18 shipped nexus.* host tool IDs (V1.57 P0 roster) ───────────────────

const NEXUS_TOOL_IDS: &[&str] = &[
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
    "nexus.reference.refresh",
];

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

/// Seed a minimal Work record for tests that need a work_id.
async fn seed_work(state: &WorkspaceState) -> String {
    let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let record = nexus_local_db::works::WorkRecord {
        work_id: work_id.clone(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "E2E Test Work".to_string(),
        long_term_goal: "Test Goal".to_string(),
        initial_idea: "Test Idea".to_string(),
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
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
    nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
        .await
        .unwrap()
        .unwrap_err(); // Returns Err with new record per existing pattern
    work_id
}

fn err_code_from_execute(r: &Result<serde_json::Value, NexusApiError>) -> Option<String> {
    r.as_ref().err().map(|e| e.error_code().to_string())
}

fn err_code_from_worker(
    r: &nexus_daemon_runtime::api::handlers::host_tool_executor::WorkerToolResult,
) -> Option<String> {
    r.error.as_ref().map(|e| e.code.clone())
}

fn err_code_from_schedule(r: &Result<serde_json::Value, NexusApiError>) -> Option<String> {
    r.as_ref().err().map(|e| e.error_code().to_string())
}

/// Assert two JSON values are structurally equivalent, allowing timestamp drift.
///
/// Timestamp fields (e.g. `assembled_at` in context.assemble) are generated
/// per-invocation and may differ between calls. Non-timestamp fields must match.
fn assert_outputs_equivalent(
    tool_id: &str,
    http: &serde_json::Value,
    worker: &serde_json::Value,
    schedule: &serde_json::Value,
) {
    // Check all 3 have the same top-level keys
    let http_keys: std::collections::BTreeSet<_> = http.as_object().unwrap().keys().collect();
    let worker_keys: std::collections::BTreeSet<_> = worker.as_object().unwrap().keys().collect();
    let schedule_keys: std::collections::BTreeSet<_> =
        schedule.as_object().unwrap().keys().collect();
    assert_eq!(
        http_keys, worker_keys,
        "{tool_id}: HTTP ⇔ Worker key mismatch"
    );
    assert_eq!(
        http_keys, schedule_keys,
        "{tool_id}: HTTP ⇔ Schedule key mismatch"
    );

    // Compare non-timestamp values
    const TIMESTAMP_KEYS: &[&str] = &["assembled_at", "created_at", "updated_at", "generatedAt"];
    for key in http_keys {
        if TIMESTAMP_KEYS.contains(&key.as_str()) {
            continue; // Allow timestamp drift
        }
        assert_eq!(
            http[key], worker[key],
            "{tool_id}: HTTP ⇔ Worker value mismatch for key '{key}'"
        );
        assert_eq!(
            http[key], schedule[key],
            "{tool_id}: HTTP ⇔ Schedule value mismatch for key '{key}'"
        );
    }
}

// ─── T2: Unknown tool rejection — all 3 paths ────────────────────────────

#[tokio::test]
async fn unknown_tool_rejected_on_all_3_paths() {
    let ctx = test_ctx().await;

    let req = ToolExecuteRequest {
        tool_name: "nexus.nonexistent.tool".to_string(),
        parameters: json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let http_result = HostToolExecutor::execute(&req, &ctx.state).await;
    let worker_result = HostToolExecutor::dispatch_from_worker(
        "nexus.nonexistent.tool",
        &json!({}),
        "req-unknown",
        &ctx.state,
    )
    .await;
    let schedule_result = HostToolExecutor::dispatch_for_schedule(
        "nexus.nonexistent.tool",
        &json!({}),
        "req-sch-unknown",
        &ctx.state,
    )
    .await;

    assert_eq!(
        err_code_from_execute(&http_result).as_deref(),
        Some("NOT_SUPPORTED")
    );
    assert_eq!(
        err_code_from_worker(&worker_result).as_deref(),
        Some("NOT_SUPPORTED")
    );
    assert_eq!(
        err_code_from_schedule(&schedule_result).as_deref(),
        Some("NOT_SUPPORTED")
    );
}

// ─── T2: Entire registry admission equivalence check ─────────────────────

/// For every registered `nexus.*` host tool ID, verify the admission-gate
/// result is equivalent across all 3 caller paths.
///
/// This verifies that the tool IS registered (not NOT_SUPPORTED) through
/// each path. It does NOT require handler success — many handlers need
/// seeded DB state. Instead it verifies that the error code (success or
/// handler-level failure like INVALID_INPUT) is consistent.
#[tokio::test]
async fn all_18_ids_admission_equivalent_across_3_paths() {
    let ctx = test_ctx().await;

    for &tool_id in NEXUS_TOOL_IDS {
        let params = json!({});

        let http = HostToolExecutor::execute(
            &ToolExecuteRequest {
                tool_name: tool_id.to_string(),
                parameters: params.clone(),
                session_id: None,
                request_id: None,
                caller_kind: None,
            },
            &ctx.state,
        )
        .await;

        let worker = HostToolExecutor::dispatch_from_worker(
            tool_id,
            &params,
            &format!("req-{tool_id}-worker"),
            &ctx.state,
        )
        .await;

        let schedule = HostToolExecutor::dispatch_for_schedule(
            tool_id,
            &params,
            &format!("req-{tool_id}-sch"),
            &ctx.state,
        )
        .await;

        let http_err = err_code_from_execute(&http);
        let worker_err = err_code_from_worker(&worker);
        let sched_err = err_code_from_schedule(&schedule);

        // NOT_SUPPORTED must NOT appear for any registered tool
        assert_ne!(
            http_err.as_deref(),
            Some("NOT_SUPPORTED"),
            "{tool_id}: HTTP path must not return NOT_SUPPORTED for registered tool"
        );
        assert_ne!(
            worker_err.as_deref(),
            Some("NOT_SUPPORTED"),
            "{tool_id}: Worker path must not return NOT_SUPPORTED for registered tool"
        );
        assert_ne!(
            sched_err.as_deref(),
            Some("NOT_SUPPORTED"),
            "{tool_id}: Schedule path must not return NOT_SUPPORTED for registered tool"
        );

        // All 3 paths must produce the same error code (or all success)
        if http_err != worker_err || http_err != sched_err {
            panic!(
                "{tool_id}: dispatch mismatch across 3 paths — \
                 HTTP={http_err:?}, Worker={worker_err:?}, Schedule={sched_err:?}"
            );
        }

        // If all succeeded, output must be structurally equivalent.
        // Timestamp fields (e.g. context.assemble's `assembled_at`) may differ
        // between calls — check non-timestamp fields only.
        if http_err.is_none() {
            let http_v = http.as_ref().unwrap();
            let worker_v = worker.output.as_ref().unwrap();
            let schedule_v = schedule.as_ref().unwrap();
            assert_outputs_equivalent(tool_id, http_v, worker_v, schedule_v);
        }
    }
}

// ─── T2: Context-based tools (no DB seed needed) ─────────────────────────

#[tokio::test]
async fn whoami_equivalent_all_3_paths() {
    let ctx = test_ctx().await;
    let http_v = HostToolExecutor::execute(
        &ToolExecuteRequest {
            tool_name: "nexus.context.whoami".to_string(),
            parameters: json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        },
        &ctx.state,
    )
    .await
    .expect("HTTP whoami");
    let worker_r = HostToolExecutor::dispatch_from_worker(
        "nexus.context.whoami",
        &json!({}),
        "req-whoami-worker",
        &ctx.state,
    )
    .await;
    assert!(worker_r.grant);
    let worker_v = worker_r.output.unwrap();
    let schedule_v = HostToolExecutor::dispatch_for_schedule(
        "nexus.context.whoami",
        &json!({}),
        "req-whoami-sch",
        &ctx.state,
    )
    .await
    .expect("Schedule whoami");

    assert_eq!(http_v["creator_id"], worker_v["creator_id"]);
    assert_eq!(http_v["creator_id"], schedule_v["creator_id"]);
}

#[tokio::test]
async fn workspace_info_equivalent_all_3_paths() {
    let ctx = test_ctx().await;
    let http_v = HostToolExecutor::execute(
        &ToolExecuteRequest {
            tool_name: "nexus.workspace.info".to_string(),
            parameters: json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        },
        &ctx.state,
    )
    .await
    .expect("HTTP workspace.info");
    let worker_r = HostToolExecutor::dispatch_from_worker(
        "nexus.workspace.info",
        &json!({}),
        "req-wsi-worker",
        &ctx.state,
    )
    .await;
    assert!(worker_r.grant);
    let worker_v = worker_r.output.unwrap();
    let schedule_v = HostToolExecutor::dispatch_for_schedule(
        "nexus.workspace.info",
        &json!({}),
        "req-wsi-sch",
        &ctx.state,
    )
    .await
    .expect("Schedule workspace.info");

    assert_eq!(http_v["creator_id"], worker_v["creator_id"]);
    assert_eq!(http_v["creator_id"], schedule_v["creator_id"]);
    assert_eq!(http_v["workspace_slug"], worker_v["workspace_slug"]);
    assert_eq!(http_v["workspace_slug"], schedule_v["workspace_slug"]);
}

// ─── T2: Seeded-ID tests ─────────────────────────────────────────────────

#[tokio::test]
async fn work_get_equivalent_all_3_paths() {
    let ctx = test_ctx().await;
    let work_id = seed_work(&ctx.state).await;
    let params = json!({"work_id": work_id});

    let http_v = HostToolExecutor::execute(
        &ToolExecuteRequest {
            tool_name: "nexus.work.get".to_string(),
            parameters: params.clone(),
            session_id: None,
            request_id: None,
            caller_kind: None,
        },
        &ctx.state,
    )
    .await
    .expect("HTTP work.get");
    let worker_r = HostToolExecutor::dispatch_from_worker(
        "nexus.work.get",
        &params,
        "req-wg-worker",
        &ctx.state,
    )
    .await;
    assert!(worker_r.grant);
    let worker_v = worker_r.output.unwrap();
    let schedule_v = HostToolExecutor::dispatch_for_schedule(
        "nexus.work.get",
        &params,
        "req-wg-sch",
        &ctx.state,
    )
    .await
    .expect("Schedule work.get");

    assert_eq!(http_v["work_id"], worker_v["work_id"]);
    assert_eq!(http_v["work_id"], schedule_v["work_id"]);
    assert_eq!(http_v["work_id"], work_id);
}

#[tokio::test]
async fn daemon_health_equivalent_all_3_paths() {
    let ctx = test_ctx().await;
    let http_v = HostToolExecutor::execute(
        &ToolExecuteRequest {
            tool_name: "nexus.observability.daemon.health".to_string(),
            parameters: json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        },
        &ctx.state,
    )
    .await
    .expect("HTTP daemon.health");
    let worker_r = HostToolExecutor::dispatch_from_worker(
        "nexus.observability.daemon.health",
        &json!({}),
        "req-dh-worker",
        &ctx.state,
    )
    .await;
    assert!(worker_r.grant);
    let worker_v = worker_r.output.unwrap();
    let schedule_v = HostToolExecutor::dispatch_for_schedule(
        "nexus.observability.daemon.health",
        &json!({}),
        "req-dh-sch",
        &ctx.state,
    )
    .await
    .expect("Schedule daemon.health");

    assert_eq!(http_v["status"], worker_v["status"]);
    assert_eq!(http_v["status"], schedule_v["status"]);
}

#[tokio::test]
async fn registry_refresh_equivalent_all_3_paths() {
    let ctx = test_ctx().await;
    let http_v = HostToolExecutor::execute(
        &ToolExecuteRequest {
            tool_name: "nexus.registry.refresh".to_string(),
            parameters: json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        },
        &ctx.state,
    )
    .await
    .expect("HTTP registry.refresh");
    let worker_r = HostToolExecutor::dispatch_from_worker(
        "nexus.registry.refresh",
        &json!({}),
        "req-rr-worker",
        &ctx.state,
    )
    .await;
    assert!(worker_r.grant);
    let worker_v = worker_r.output.unwrap();
    let schedule_v = HostToolExecutor::dispatch_for_schedule(
        "nexus.registry.refresh",
        &json!({}),
        "req-rr-sch",
        &ctx.state,
    )
    .await
    .expect("Schedule registry.refresh");

    assert_eq!(http_v["source"], worker_v["source"]);
    assert_eq!(http_v["source"], schedule_v["source"]);
}

// ─── T2: NOT_SUPPORTED for unregistered IDs — all 3 paths ────────────────

#[tokio::test]
async fn not_supported_equivalence_all_3_paths() {
    let ctx = test_ctx().await;
    for unknown_id in [
        "nexus.nonexistent.tool",
        "unknown/namespace",
        "nexus.publish.chapter",
    ] {
        let req = ToolExecuteRequest {
            tool_name: unknown_id.to_string(),
            parameters: json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let http = HostToolExecutor::execute(&req, &ctx.state).await;
        let worker =
            HostToolExecutor::dispatch_from_worker(unknown_id, &json!({}), "req-ns", &ctx.state)
                .await;
        let schedule = HostToolExecutor::dispatch_for_schedule(
            unknown_id,
            &json!({}),
            "req-ns-sch",
            &ctx.state,
        )
        .await;

        assert_eq!(
            err_code_from_execute(&http).as_deref(),
            Some("NOT_SUPPORTED"),
            "{unknown_id}: HTTP"
        );
        assert_eq!(
            err_code_from_worker(&worker).as_deref(),
            Some("NOT_SUPPORTED"),
            "{unknown_id}: Worker"
        );
        assert_eq!(
            err_code_from_schedule(&schedule).as_deref(),
            Some("NOT_SUPPORTED"),
            "{unknown_id}: Schedule"
        );
    }
}

// ─── T2: Registry integrity check ────────────────────────────────────────

#[test]
fn all_19_nexus_tool_ids_registered_in_capability_registry() {
    let reg = host_tool_registry();
    for &tool_id in NEXUS_TOOL_IDS {
        assert!(
            reg.lookup(tool_id).is_some(),
            "Tool '{tool_id}' must be registered in CapabilityRegistry"
        );
    }
    assert_eq!(
        reg.len(),
        21,
        "Registry must have 21 entries (19 nexus.* + 2 fs/*)"
    );
}

// ─── T3: Profile-set IDs are NOT action capabilities ─────────────────────

/// V1.57 P3: Verify that `nexus.profile.{minimal,writer,publisher}` are
/// **not** registered as action capabilities in `host_tool_registry()`.
///
/// Per `acp-capability-set.md` §3.3 and §4 roster, these three IDs are
/// `scaffold-equivalent` — §3.3 metadata for capability grouping, not
/// runtime action IDs. They must NOT have handler bindings.
#[test]
fn test_profile_sets_are_not_action_capabilities() {
    let reg = host_tool_registry();

    for profile_id in [
        "nexus.profile.minimal",
        "nexus.profile.writer",
        "nexus.profile.publisher",
    ] {
        assert!(
            reg.lookup(profile_id).is_none(),
            "Profile-set ID '{profile_id}' must NOT be registered in host_tool_registry() — it is §3.3 metadata, not an action capability"
        );
    }

    // Also verify these IDs are NOT in the NEXUS_TOOL_IDS action list
    for profile_id in [
        "nexus.profile.minimal",
        "nexus.profile.writer",
        "nexus.profile.publisher",
    ] {
        assert!(
            !NEXUS_TOOL_IDS.contains(&profile_id),
            "Profile-set ID '{profile_id}' must NOT appear in NEXUS_TOOL_IDS"
        );
    }
}
