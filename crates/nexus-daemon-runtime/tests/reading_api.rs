//! Reading depth Local API integration tests (V1.89).
//!
//! Covers the `/v1/local/reading/*` progress and annotation surface using
//! direct handler invocation.

#![allow(clippy::unwrap_used)]

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_contracts::local_api::reading::{
    ReadingAnnotationCreateRequest, ReadingAnnotationListQuery, ReadingAnnotationPatchRequest,
    ReadingProgressQuery, ReadingProgressRequest,
};
use nexus_daemon_runtime::api::errors::NexusApiError;
use nexus_daemon_runtime::api::handlers::reading;
use nexus_daemon_runtime::api::handlers::works::{create_work, CreateWorkRequest};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;

/// Build a fresh `WorkspaceState` for handler-level testing.
async fn handler_state() -> (WorkspaceState, TestTempRoot) {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let workspace_dir = tmp.path().join("creative");
    std::fs::create_dir_all(&workspace_dir).expect("create workspace dir");
    let state = WorkspaceState::new_for_testing(
        nexus_home.clone(),
        db_path.clone(),
        Some(workspace_dir.to_string_lossy().to_string()),
    )
    .await;
    test_utils::seed_test_creator_and_world(state.pool()).await;
    (state, tmp)
}

/// Build a `WorkspaceState` with no active creator for 401 tests.
async fn handler_state_no_creator() -> (WorkspaceState, TestTempRoot) {
    let tmp = tempfile::TempDir::new().unwrap();
    let nexus_home = tmp.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    let db_path = nexus_home.join("state.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    nexus_local_db::seed_versions(&pool).await.unwrap();
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    (state, test_utils::create_test_workspace().await.0)
}

/// Create a Work and return its ID.
async fn create_test_work(state: &WorkspaceState) -> String {
    let (_, resp) = create_work(
        State(state.clone()),
        Json(CreateWorkRequest {
            title: "Test Novel".into(),
            long_term_goal: "Write".into(),
            initial_idea: "Idea".into(),
            world_id: Some("wld_test_world".to_string()),
            story_ref: None,
            primary_preset_id: None,
            client_request_id: None,
            lineage_from_work_id: None,
            set_pool_active: None,
            work_profile: Some("novel".to_string()),
        }),
    )
    .await
    .expect("create work");
    resp.work_id.clone()
}

/// Extract the inner code of a `NexusApiError::BadRequest`.
fn bad_request_code(err: &NexusApiError) -> Option<&str> {
    match err {
        NexusApiError::BadRequest { code, .. } => Some(code),
        _ => None,
    }
}

// ─── Progress ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_progress_defaults_to_zero() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state).await;

    let Json(resp) = reading::get_reading_progress(
        State(state),
        Query(ReadingProgressQuery {
            work_id,
            chapter: 1,
        }),
    )
    .await
    .expect("get progress");
    assert_eq!(resp.scroll_progress, 0);
    assert!(!resp.updated_at.is_empty());
}

#[tokio::test]
async fn put_and_get_progress_round_trip() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state).await;

    let Json(put_resp) = reading::put_reading_progress(
        State(state.clone()),
        Json(ReadingProgressRequest {
            work_id: work_id.clone(),
            chapter: 1,
            scroll_progress: 7500,
        }),
    )
    .await
    .expect("put progress");
    assert_eq!(put_resp.scroll_progress, 7500);

    let Json(get_resp) = reading::get_reading_progress(
        State(state),
        Query(ReadingProgressQuery {
            work_id,
            chapter: 1,
        }),
    )
    .await
    .expect("get progress");
    assert_eq!(get_resp.scroll_progress, 7500);
    assert_eq!(get_resp.updated_at, put_resp.updated_at);
}

#[tokio::test]
async fn delete_progress_clears_row() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state).await;

    let _ = reading::put_reading_progress(
        State(state.clone()),
        Json(ReadingProgressRequest {
            work_id: work_id.clone(),
            chapter: 1,
            scroll_progress: 5000,
        }),
    )
    .await
    .expect("put progress");

    let status = reading::delete_reading_progress(
        State(state.clone()),
        Query(ReadingProgressQuery {
            work_id: work_id.clone(),
            chapter: 1,
        }),
    )
    .await
    .expect("delete progress");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let Json(get_resp) = reading::get_reading_progress(
        State(state),
        Query(ReadingProgressQuery {
            work_id,
            chapter: 1,
        }),
    )
    .await
    .expect("get progress");
    assert_eq!(get_resp.scroll_progress, 0);
}

#[tokio::test]
async fn progress_unknown_work_returns_404() {
    let (state, _tmp) = handler_state().await;
    let err = reading::get_reading_progress(
        State(state),
        Query(ReadingProgressQuery {
            work_id: "wrk_unknown".to_string(),
            chapter: 1,
        }),
    )
    .await
    .expect_err("unknown work should 404");
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn progress_requires_creator() {
    let (state, _tmp) = handler_state_no_creator().await;
    let err = reading::get_reading_progress(
        State(state),
        Query(ReadingProgressQuery {
            work_id: "wrk_any".to_string(),
            chapter: 1,
        }),
    )
    .await
    .expect_err("no creator should 401");
    assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
}

// ─── Annotations ────────────────────────────────────────────────────────────

#[tokio::test]
async fn annotation_create_list_patch_delete_round_trip() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state).await;

    let Json(created) = reading::create_annotation(
        State(state.clone()),
        Json(ReadingAnnotationCreateRequest {
            work_id: work_id.clone(),
            chapter: 1,
            start_offset: 10,
            end_offset: 20,
            selected_text: "highlighted".to_string(),
            color: "yellow".to_string(),
            note: Some("note".to_string()),
        }),
    )
    .await
    .expect("create annotation");
    assert_eq!(created.start_offset, 10);
    assert_eq!(created.end_offset, 20);
    assert_eq!(created.color, "yellow");
    assert_eq!(created.note, Some("note".to_string()));

    let Json(list) = reading::list_annotations(
        State(state.clone()),
        Query(ReadingAnnotationListQuery {
            work_id: work_id.clone(),
            chapter: 1,
        }),
    )
    .await
    .expect("list annotations");
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].annotation_id, created.annotation_id);

    let Json(updated) = reading::patch_annotation(
        State(state.clone()),
        Path(created.annotation_id.clone()),
        Json(ReadingAnnotationPatchRequest {
            color: Some("blue".to_string()),
            note: None,
        }),
    )
    .await
    .expect("patch annotation");
    assert_eq!(updated.color, "blue");
    assert_eq!(updated.note, Some("note".to_string()));

    let status = reading::delete_annotation(State(state.clone()), Path(created.annotation_id))
        .await
        .expect("delete annotation");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let Json(list2) = reading::list_annotations(
        State(state),
        Query(ReadingAnnotationListQuery {
            work_id,
            chapter: 1,
        }),
    )
    .await
    .expect("list annotations after delete");
    assert!(list2.items.is_empty());
}

#[tokio::test]
async fn patch_annotation_can_clear_note() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state).await;

    let Json(created) = reading::create_annotation(
        State(state.clone()),
        Json(ReadingAnnotationCreateRequest {
            work_id: work_id.clone(),
            chapter: 1,
            start_offset: 0,
            end_offset: 5,
            selected_text: "word".to_string(),
            color: "yellow".to_string(),
            note: Some("old note".to_string()),
        }),
    )
    .await
    .expect("create annotation");

    let Json(updated) = reading::patch_annotation(
        State(state),
        Path(created.annotation_id),
        Json(ReadingAnnotationPatchRequest {
            color: None,
            note: Some("".to_string()),
        }),
    )
    .await
    .expect("patch to clear note");
    assert_eq!(updated.note, None);
}

#[tokio::test]
async fn create_annotation_rejects_invalid_color() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state).await;

    let err = reading::create_annotation(
        State(state),
        Json(ReadingAnnotationCreateRequest {
            work_id,
            chapter: 1,
            start_offset: 0,
            end_offset: 5,
            selected_text: "word".to_string(),
            color: "red".to_string(),
            note: None,
        }),
    )
    .await
    .expect_err("invalid color should fail");
    assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(bad_request_code(&err), Some("invalid_input"));
}

#[tokio::test]
async fn create_annotation_rejects_invalid_offsets() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state).await;

    let err = reading::create_annotation(
        State(state),
        Json(ReadingAnnotationCreateRequest {
            work_id,
            chapter: 1,
            start_offset: 10,
            end_offset: 5,
            selected_text: "word".to_string(),
            color: "yellow".to_string(),
            note: None,
        }),
    )
    .await
    .expect_err("invalid offsets should fail");
    assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn annotation_not_found_returns_404() {
    let (state, _tmp) = handler_state().await;
    let err = reading::delete_annotation(State(state), Path("ann_unknown".to_string()))
        .await
        .expect_err("unknown annotation should 404");
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn annotation_cross_creator_returns_403() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state).await;

    let Json(created) = reading::create_annotation(
        State(state.clone()),
        Json(ReadingAnnotationCreateRequest {
            work_id: work_id.clone(),
            chapter: 1,
            start_offset: 0,
            end_offset: 5,
            selected_text: "word".to_string(),
            color: "yellow".to_string(),
            note: None,
        }),
    )
    .await
    .expect("create annotation");

    // Simulate a different creator by mutating the row's creator_id directly.
    // SAFETY: test-only UPDATE with a literal column value and parameterized id.
    sqlx::query(
        "UPDATE reading_annotations SET creator_id = 'other_creator' WHERE annotation_id = ?1",
    )
    .bind(&created.annotation_id)
    .execute(state.pool())
    .await
    .expect("reassign creator");

    let err = reading::patch_annotation(
        State(state.clone()),
        Path(created.annotation_id.clone()),
        Json(ReadingAnnotationPatchRequest {
            color: Some("blue".to_string()),
            note: None,
        }),
    )
    .await
    .expect_err("cross-creator patch should 403");
    assert_eq!(err.status_code(), StatusCode::FORBIDDEN);

    let err = reading::delete_annotation(State(state), Path(created.annotation_id))
        .await
        .expect_err("cross-creator delete should 403");
    assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn annotations_requires_creator() {
    let (state, _tmp) = handler_state_no_creator().await;
    let err = reading::list_annotations(
        State(state),
        Query(ReadingAnnotationListQuery {
            work_id: "wrk_any".to_string(),
            chapter: 1,
        }),
    )
    .await
    .expect_err("no creator should 401");
    assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
}
