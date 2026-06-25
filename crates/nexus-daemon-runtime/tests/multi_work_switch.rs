//! Multi-work lifecycle hermetic tests — daemon-runtime layer (DF-60, V1.41 P0 T8).
//!
//! Covers:
//! - TC4: completion-lock blocks mutation (409 Conflict)
//! - TC5: runtime-lock rejects concurrent mutate (423 Locked)
//! - TC6: completion ceremony end-to-end (via handler invocation)

#![allow(clippy::unwrap_used)]

use axum::extract::{Path, State};
use nexus_daemon_runtime::api::errors::NexusApiError;
use nexus_daemon_runtime::api::handlers::works::{CreateWorkRequest, PatchWorkRequest};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::works;

// ─── Helpers ───────────────────────────────────────────────────────────────

async fn handler_state() -> (WorkspaceState, test_utils::TestTempRoot) {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    test_utils::seed_test_creator_and_world(state.pool()).await;
    (state, tmp)
}

fn minimal_patch() -> PatchWorkRequest {
    PatchWorkRequest {
        title: Some("Attempted change".into()),
        long_term_goal: None,
        creative_brief: None,
        intake_status: None,
        status: None,
        world_id: None,
        story_ref: None,
        primary_preset_id: None,
        current_stage: None,
        stage_status: None,
        force: None,
        auto_review_master_on_timeout: None,
        auto_chain_interrupted: None,
        work_profile: None,
    }
}

/// Create a work via the API, then apply DB patches for lock fields.
/// The INSERT hard-codes the 5 new T4 columns as NULL, so we must
/// patch them afterwards.
async fn create_and_patch_work(
    state: &WorkspaceState,
    completion_locked: Option<&str>,
    runtime_lock_holder: Option<&str>,
) -> String {
    let create_req = CreateWorkRequest {
        title: "Test Novel".into(),
        long_term_goal: "Write a novel".into(),
        initial_idea: "A story".into(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        primary_preset_id: None,
        client_request_id: None,
        lineage_from_work_id: None,
        set_pool_active: None,
        work_profile: None,
    };
    let (_, create_resp) = nexus_daemon_runtime::api::handlers::works::create_work(
        State(state.clone()),
        axum::Json(create_req),
    )
    .await
    .unwrap();
    let work_id = create_resp.work_id.clone();

    // Apply lock fields via DB patch
    let mut patch = works::WorkPatch::default();
    if let Some(ts) = completion_locked {
        patch.status = Some("completed".to_string());
        patch.novel_completion_status = Some(Some("finalize_complete".to_string()));
        patch.completion_locked_at = Some(Some(ts.to_string()));
    }
    if let Some(holder) = runtime_lock_holder {
        patch.runtime_lock_holder = Some(Some(holder.to_string()));
        // V1.42 P0: use current timestamp so lock is NOT stale (within 2h TTL).
        // Stale locks are force-cleared by RuntimeLockGuard::acquire.
        patch.runtime_lock_acquired_at = Some(Some(chrono::Utc::now().to_rfc3339()));
    }
    if completion_locked.is_some() || runtime_lock_holder.is_some() {
        works::patch_work(
            state.pool(),
            "test_creator",
            &work_id,
            &patch,
            "2026-06-10T12:00:00Z",
        )
        .await
        .unwrap();
    }

    work_id
}

// ─── TC4: Completion-lock blocks mutation (409 Conflict) ────────────────

#[tokio::test]
async fn test_completion_lock_blocks_mutation() {
    let (state, _tmp) = handler_state().await;

    let work_id = create_and_patch_work(&state, Some("2026-06-10T12:00:00Z"), None).await;

    let result = nexus_daemon_runtime::api::handlers::works::patch_work(
        State(state),
        Path(work_id.clone()),
        axum::Json(minimal_patch()),
    )
    .await;

    let err = result.expect_err("completion-locked work should reject PATCH");
    match err {
        NexusApiError::Conflict(msg) => {
            assert!(
                msg.contains("completion-locked"),
                "expected completion-locked message, got: {msg}"
            );
        }
        other => panic!("expected NexusApiError::Conflict, got: {other:?}"),
    }
}

// ─── TC5: Runtime-lock rejects concurrent mutate (423 Locked) ──────────

#[tokio::test]
async fn test_runtime_lock_rejects_concurrent_mutate() {
    let (state, _tmp) = handler_state().await;

    let work_id = create_and_patch_work(&state, None, Some("sched_worker_1")).await;

    let result = nexus_daemon_runtime::api::handlers::works::patch_work(
        State(state),
        Path(work_id.clone()),
        axum::Json(minimal_patch()),
    )
    .await;

    let err = result.expect_err("runtime-locked work should reject PATCH");
    match err {
        NexusApiError::Locked { reason, .. } => {
            assert!(
                reason.contains("locked"),
                "expected locked message, got: {reason}"
            );
        }
        other => panic!("expected NexusApiError::Locked, got: {other:?}"),
    }
}

// ─── TC6: Completion ceremony end-to-end ────────────────────────────────

#[tokio::test]
async fn test_completion_ceremony_blocks_subsequent_patch() {
    let (state, _tmp) = handler_state().await;

    // Create a normal work
    let work_id = create_and_patch_work(&state, None, None).await;

    // First PATCH should succeed (no locks)
    let result = nexus_daemon_runtime::api::handlers::works::patch_work(
        State(state.clone()),
        Path(work_id.clone()),
        axum::Json(PatchWorkRequest {
            title: Some("Safe change".into()),
            ..minimal_patch()
        }),
    )
    .await;
    assert!(
        result.is_ok(),
        "first PATCH on unlocked work should succeed"
    );

    // Now apply completion-lock via DB (simulating mark_work_completed)
    let completion_patch = works::WorkPatch {
        status: Some("completed".to_string()),
        novel_completion_status: Some(Some("finalize_complete".to_string())),
        completion_locked_at: Some(Some("2026-06-10T14:00:00Z".to_string())),
        ..Default::default()
    };
    works::patch_work(
        state.pool(),
        "test_creator",
        &work_id,
        &completion_patch,
        "2026-06-10T14:00:00Z",
    )
    .await
    .unwrap();

    // Second PATCH should be blocked by completion-lock
    let result2 = nexus_daemon_runtime::api::handlers::works::patch_work(
        State(state),
        Path(work_id.clone()),
        axum::Json(minimal_patch()),
    )
    .await;

    let err = result2.expect_err("second PATCH after completion-lock should fail");
    match err {
        NexusApiError::Conflict(msg) => {
            assert!(msg.contains("completion-locked"), "got: {msg}");
        }
        other => panic!("expected Conflict, got: {other:?}"),
    }
}
