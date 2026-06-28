//! Canvas Outline+Timeline API integration tests (V1.72 P0).
//!
//! These tests invoke handlers directly rather than going through axum-test
//! HTTP routing, because axum-test has a known limitation with hyphenated
//! UUIDs in path segments (see `tests/works_api.rs`). They still exercise the
//! full DB + filesystem stack behind each handler.

#![allow(clippy::unwrap_used)]

use axum::extract::{Path, State};
use axum::Json;
use nexus_daemon_runtime::api::handlers::{
    outline,
    works::{CreateWorkRequest, PatchWorkRequest},
};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::json;

struct TestCtx {
    _tmp: test_utils::TestTempRoot,
    state: WorkspaceState,
}

async fn test_ctx() -> TestCtx {
    let (tmp, nexus_home, db_path, workspace_dir) =
        test_utils::create_initialized_test_workspace().await;
    let state = WorkspaceState::new_for_testing(
        nexus_home,
        db_path,
        Some(workspace_dir.to_string_lossy().to_string()),
    )
    .await;
    test_utils::seed_test_creator_and_world(state.pool()).await;
    TestCtx { _tmp: tmp, state }
}

async fn create_work(state: &WorkspaceState) -> String {
    let req = CreateWorkRequest {
        title: "Outline Test Novel".to_string(),
        long_term_goal: "Test the outline canvas".to_string(),
        initial_idea: "A test story".to_string(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        primary_preset_id: None,
        lineage_from_work_id: None,
        client_request_id: None,
        set_pool_active: None,
        work_profile: None,
    };
    let (_status, Json(resp)) =
        nexus_daemon_runtime::api::handlers::works::create_work(State(state.clone()), Json(req))
            .await
            .unwrap();
    resp.work_id
}

async fn set_story_ref(state: &WorkspaceState, work_id: &str, story_ref: &str) {
    let req = PatchWorkRequest {
        title: None,
        long_term_goal: None,
        creative_brief: None,
        intake_status: None,
        status: None,
        world_id: None,
        story_ref: Some(Some(story_ref.to_string())),
        primary_preset_id: None,
        current_stage: None,
        stage_status: None,
        force: None,
        auto_review_master_on_timeout: None,
        auto_chain_interrupted: None,
        work_profile: None,
    };
    let _ = nexus_daemon_runtime::api::handlers::works::patch_work(
        State(state.clone()),
        Path(work_id.to_string()),
        Json(req),
    )
    .await
    .unwrap();
}

async fn seed_chapter(pool: &sqlx::SqlitePool, work_id: &str, chapter: i32) {
    let now = chrono::Utc::now().to_rfc3339();
    let params = nexus_local_db::work_chapters::InsertChapterParams {
        work_id,
        chapter,
        volume: Some(1),
        slug: Some(&format!("ch{chapter:02}")),
        planned_word_count: 4000,
        outline_path: None,
        body_path: None,
        now: &now,
    };
    nexus_local_db::work_chapters::insert_chapter(pool, &params)
        .await
        .expect("seed chapter");
}

#[tokio::test]
async fn outline_read_returns_default_frontmatter() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.state).await;
    set_story_ref(&ctx.state, &work_id, "outline-test-novel").await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    let Json(body) = outline::get_work_outline(State(ctx.state.clone()), Path(work_id.clone()))
        .await
        .unwrap();
    let body = serde_json::to_value(body).unwrap();
    assert_eq!(body["outline_revision"], 0);
    assert_eq!(body["volumes"].as_array().unwrap().len(), 1);
    assert_eq!(body["volumes"][0]["volume_id"], 1);
    assert_eq!(body["volumes"][0]["chapter_ids"], json!([1]));
}

#[tokio::test]
async fn outline_structure_patch_moves_chapter_and_bumps_revision() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.state).await;
    set_story_ref(&ctx.state, &work_id, "outline-test-novel").await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    let req = json!({
        "work_id": work_id,
        "base_revision": 0,
        "operation": "move_chapter",
        "chapter_id": 1,
        "volume_id": 2
    });
    let Json(resp) = outline::patch_outline_structure(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(serde_json::from_value(req).unwrap()),
    )
    .await
    .unwrap();
    let resp = serde_json::to_value(resp).unwrap();
    assert_eq!(resp["new_revision"], 1);

    let Json(body) = outline::get_work_outline(State(ctx.state.clone()), Path(work_id.clone()))
        .await
        .unwrap();
    let body = serde_json::to_value(body).unwrap();
    assert_eq!(body["outline_revision"], 1);
    let volumes = body["volumes"].as_array().unwrap();
    assert_eq!(volumes.len(), 1);
    assert_eq!(volumes[0]["volume_id"], 2);
    assert_eq!(volumes[0]["chapter_ids"], json!([1]));
}

#[tokio::test]
async fn outline_chapter_patch_updates_title_and_status() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.state).await;
    set_story_ref(&ctx.state, &work_id, "outline-test-novel").await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    let req = json!({
        "work_id": work_id,
        "chapter_id": 1,
        "base_revision": 0,
        "set": {
            "title": "Opening Scene",
            "status": "outlined"
        }
    });
    let Json(resp) = outline::patch_outline_chapter(
        State(ctx.state.clone()),
        Path((work_id.clone(), "1".to_string())),
        Json(serde_json::from_value(req).unwrap()),
    )
    .await
    .unwrap();
    let resp = serde_json::to_value(resp).unwrap();
    assert_eq!(resp["new_revision"], 1);

    let Json(body) = outline::get_work_outline(State(ctx.state.clone()), Path(work_id.clone()))
        .await
        .unwrap();
    let body = serde_json::to_value(body).unwrap();
    assert_eq!(body["chapter_titles"]["1"], "Opening Scene");
}

#[tokio::test]
async fn outline_timeline_patch_adds_event_and_links_chapter() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.state).await;
    set_story_ref(&ctx.state, &work_id, "outline-test-novel").await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    let req = json!({
        "work_id": work_id,
        "base_revision": 0,
        "operation": "add_event",
        "title": "The Inciting Incident",
        "realizes_chapter_id": 1
    });
    let Json(resp) = outline::patch_timeline_event(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(serde_json::from_value(req).unwrap()),
    )
    .await
    .unwrap();
    let resp = serde_json::to_value(resp).unwrap();
    assert_eq!(resp["new_revision"], 1);

    let Json(body) = outline::get_work_outline(State(ctx.state.clone()), Path(work_id.clone()))
        .await
        .unwrap();
    let body = serde_json::to_value(body).unwrap();
    let events = body["timeline_events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["title"], "The Inciting Incident");
    assert_eq!(events[0]["realizes_chapter_id"], 1);
}

#[tokio::test]
async fn outline_patch_rejects_stale_revision_with_conflict() {
    let ctx = test_ctx().await;
    let work_id = create_work(&ctx.state).await;
    set_story_ref(&ctx.state, &work_id, "outline-test-novel").await;
    seed_chapter(ctx.state.pool(), &work_id, 1).await;

    let req = json!({
        "work_id": work_id,
        "base_revision": 0,
        "operation": "move_chapter",
        "chapter_id": 1,
        "volume_id": 2
    });
    let _ = outline::patch_outline_structure(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(serde_json::from_value(req.clone()).unwrap()),
    )
    .await
    .unwrap();

    let err = outline::patch_outline_structure(
        State(ctx.state.clone()),
        Path(work_id.clone()),
        Json(serde_json::from_value(req).unwrap()),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        nexus_daemon_runtime::api::errors::NexusApiError::OutlineConflict { .. }
    ));
}
