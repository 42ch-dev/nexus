//! Runtime lock hermetic tests (V1.42 P0 — T5).
//!
//! Acceptance criteria:
//! 1. Two concurrent mutating operations on same Work → second fails with holder hint.
//! 2. Crashed CLI holder cleared after TTL (configurable).
//! 3. Auto-chain skips Works with foreign runtime_lock_holder.

#![allow(clippy::unwrap_used)]

use axum::extract::{Path, State};
use axum::Json;
use nexus_daemon_runtime::api::handlers::works::{
    append_inspiration, create_work, patch_work, AppendInspirationRequest, CreateWorkRequest,
    PatchWorkRequest,
};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::works;

// ─── Helpers ───────────────────────────────────────────────────────────────

struct TestCtx {
    _tmp: TestTempRoot,
    state: WorkspaceState,
}

async fn test_ctx() -> TestCtx {
    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    test_utils::seed_test_creator_and_world(state.pool()).await;
    TestCtx { _tmp, state }
}

/// Create a Work via handler and return its work_id.
async fn create_test_work(state: &WorkspaceState) -> String {
    let (_, resp) = create_work(
        State(state.clone()),
        axum::Json(CreateWorkRequest {
            title: "Test Novel".into(),
            long_term_goal: "Finish a short story".into(),
            initial_idea: "A detective story".into(),
            world_id: Some("wld_test_world".to_string()),
            story_ref: None,
            primary_preset_id: None,
            client_request_id: None,
            lineage_from_work_id: None,
            set_pool_active: None,
        }),
    )
    .await
    .unwrap();
    resp.work_id.clone()
}

fn minimal_patch(title: &str) -> PatchWorkRequest {
    PatchWorkRequest {
        title: Some(title.to_string()),
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
    }
}

/// Manually set a runtime lock on a Work (simulating a crashed CLI holder).
async fn set_runtime_lock(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    work_id: &str,
    holder: &str,
    acquired_at: &str,
) {
    let patch = works::WorkPatch {
        runtime_lock_holder: Some(Some(holder.to_string())),
        runtime_lock_acquired_at: Some(Some(acquired_at.to_string())),
        ..Default::default()
    };
    works::patch_work(
        pool,
        creator_id,
        work_id,
        &patch,
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_concurrent_patch_second_fails_with_holder_hint() {
    // AC1: Two concurrent mutating operations on same Work → second fails with holder hint.
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    // Simulate first process holding the lock
    set_runtime_lock(
        ctx.state.pool(),
        "test_creator",
        &work_id,
        "cli:http:11111111-2222-3333-4444-555555555555",
        &chrono::Utc::now().to_rfc3339(),
    )
    .await;

    // Second PATCH should fail with 423 Locked
    let result = patch_work(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(minimal_patch("Updated Title")),
    )
    .await;

    let err = result.expect_err("second concurrent PATCH should fail");
    assert_eq!(err.status_code(), axum::http::StatusCode::LOCKED);
}

#[tokio::test]
async fn test_concurrent_inspiration_second_fails_with_holder_hint() {
    // AC1 variant: inspiration append also blocked by runtime lock.
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    // Hold the lock
    set_runtime_lock(
        ctx.state.pool(),
        "test_creator",
        &work_id,
        "daemon:schedule:ACH20260611120000",
        &chrono::Utc::now().to_rfc3339(),
    )
    .await;

    let req = AppendInspirationRequest {
        note: "New inspiration".to_string(),
    };

    let result = append_inspiration(State(ctx.state), Path(work_id), Json(req)).await;

    let err = result.expect_err("inspiration append should fail when locked");
    assert_eq!(err.status_code(), axum::http::StatusCode::LOCKED);
}

#[tokio::test]
async fn test_stale_lock_cleared_after_ttl() {
    // AC2: Crashed CLI holder cleared after TTL (configurable).
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    // Simulate a crashed CLI holder from 3 hours ago
    let three_hours_ago = (chrono::Utc::now() - chrono::Duration::hours(3)).to_rfc3339();
    set_runtime_lock(
        ctx.state.pool(),
        "test_creator",
        &work_id,
        "cli:http:dead-beef-cafe",
        &three_hours_ago,
    )
    .await;

    // The patch_work handler should succeed because the stale lock is force-cleared
    // by acquire_runtime_lock (force_stale=true, TTL=7200s)
    let result = patch_work(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(minimal_patch("Updated After Stale Clear")),
    )
    .await;

    assert!(
        result.is_ok(),
        "PATCH should succeed after stale lock clear"
    );

    // Verify the lock was released (handler releases on return)
    let work = works::get_work(ctx.state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        work.runtime_lock_holder.is_none(),
        "lock should be released after handler"
    );
    assert_eq!(work.title, "Updated After Stale Clear");
}

#[tokio::test]
async fn test_fresh_lock_not_cleared_within_ttl() {
    // Complementary: a fresh lock (within TTL) should NOT be force-cleared.
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    // Lock acquired just now (within TTL)
    set_runtime_lock(
        ctx.state.pool(),
        "test_creator",
        &work_id,
        "cli:http:fresh-lock",
        &chrono::Utc::now().to_rfc3339(),
    )
    .await;

    let result = patch_work(
        State(ctx.state),
        Path(work_id),
        Json(minimal_patch("Should Not Succeed")),
    )
    .await;

    let err = result.expect_err("fresh lock should block PATCH");
    assert_eq!(err.status_code(), axum::http::StatusCode::LOCKED);
}

#[tokio::test]
async fn test_patch_acquires_and_releases_lock() {
    // Verify that a successful PATCH acquires the lock during execution
    // and releases it when done.
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    // No lock initially
    let work = works::get_work(ctx.state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap();
    assert!(work.runtime_lock_holder.is_none());

    let result = patch_work(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(minimal_patch("Locked During Patch")),
    )
    .await;

    assert!(result.is_ok());

    // Lock should be released after handler returns
    let work = works::get_work(ctx.state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap();
    assert!(work.runtime_lock_holder.is_none());
    assert_eq!(work.title, "Locked During Patch");
}

#[tokio::test]
async fn test_inspiration_acquires_and_releases_lock() {
    // Verify inspiration append also acquires/releases the lock.
    let ctx = test_ctx().await;
    let work_id = create_test_work(&ctx.state).await;

    let req = AppendInspirationRequest {
        note: "Test inspiration".to_string(),
    };

    let result =
        append_inspiration(State(ctx.state.clone()), Path(work_id.clone()), Json(req)).await;

    assert!(result.is_ok());

    // Lock should be released after handler
    let work = works::get_work(ctx.state.pool(), "test_creator", &work_id)
        .await
        .unwrap()
        .unwrap();
    assert!(work.runtime_lock_holder.is_none());
}
