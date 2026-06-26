//! Chapter content Local API integration tests (R-V165-QC1-W2).
//!
//! Covers the V1.65 chapter-content surface under
//! `/v1/local/works/{work_id}/chapters/*` using direct handler invocation.
//!
//! HTTP routing is intentionally avoided for path-parameterized endpoints
//! because axum-test mishandles hyphenated UUIDs in path segments; the
//! Works API tests (`tests/works_api.rs`) document the same limitation.

#![allow(clippy::unwrap_used)]

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_contracts::{
    ChapterContentQuery, ChapterStatus, ListChaptersQuery, PatchChapterRequest,
    PutChapterOutlineRequest,
};
use nexus_daemon_runtime::api::errors::NexusApiError;
use nexus_daemon_runtime::api::handlers::chapters;
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

/// Extract the inner code of a `NexusApiError::BadRequest`.
fn bad_request_code(err: &NexusApiError) -> Option<&str> {
    match err {
        NexusApiError::BadRequest { code, .. } => Some(code),
        _ => None,
    }
}

/// Create a Work, assign a `work_ref`, and seed a handful of chapters.
async fn create_work_with_chapters(state: &WorkspaceState, count: i32) -> (String, String) {
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
    let work_id = resp.work_id.clone();

    let patch = nexus_local_db::works::WorkPatch {
        work_ref: Some(Some("test-novel".to_string())),
        ..Default::default()
    };
    let now = chrono::Utc::now().to_rfc3339();
    nexus_local_db::works::patch_work(state.pool(), "test_creator", &work_id, &patch, &now)
        .await
        .expect("patch work_ref");

    let now = chrono::Utc::now().to_rfc3339();
    nexus_local_db::work_chapters::seed_chapters(state.pool(), &work_id, "test-novel", count, &now)
        .await
        .expect("seed chapters");

    (work_id, "test-novel".to_string())
}

// ─── T1.1: LIST ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_chapters_cursor_pagination_items_and_next_cursor() {
    let (state, _tmp) = handler_state().await;
    let (work_id, _work_ref) = create_work_with_chapters(&state, 5).await;

    let Json(first) = chapters::list_chapters(
        State(state.clone()),
        Path(work_id.clone()),
        Query(ListChaptersQuery {
            status: None,
            limit: Some(2),
            cursor: None,
        }),
    )
    .await
    .expect("first page");
    assert_eq!(first.items.len(), 2);
    assert!(first.pagination.has_more);
    let cursor = first
        .pagination
        .next_cursor
        .expect("first page should have next_cursor");
    assert!(cursor.starts_with("v2:"));

    let Json(second) = chapters::list_chapters(
        State(state.clone()),
        Path(work_id.clone()),
        Query(ListChaptersQuery {
            status: None,
            limit: Some(2),
            cursor: Some(cursor.clone()),
        }),
    )
    .await
    .expect("second page");
    assert_eq!(second.items.len(), 2);
    assert!(second.pagination.has_more);
    let cursor2 = second
        .pagination
        .next_cursor
        .expect("second page should have next_cursor");
    assert!(cursor2.starts_with("v2:"));
    assert_ne!(cursor, cursor2, "cursor should advance");

    let Json(third) = chapters::list_chapters(
        State(state.clone()),
        Path(work_id),
        Query(ListChaptersQuery {
            status: None,
            limit: Some(2),
            cursor: Some(cursor2),
        }),
    )
    .await
    .expect("third page");
    assert_eq!(third.items.len(), 1);
    assert!(!third.pagination.has_more);
    assert!(third.pagination.next_cursor.is_none());
}

#[tokio::test]
async fn list_chapters_returns_404_for_unknown_work() {
    let (state, _tmp) = handler_state().await;
    let err = chapters::list_chapters(
        State(state),
        Path("wrk_unknown".to_string()),
        Query(ListChaptersQuery {
            status: None,
            limit: None,
            cursor: None,
        }),
    )
    .await
    .expect_err("unknown work should 404");
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_chapters_returns_401_without_creator() {
    let (state, _tmp) = handler_state_no_creator().await;
    let err = chapters::list_chapters(
        State(state),
        Path("wrk_any".to_string()),
        Query(ListChaptersQuery {
            status: None,
            limit: None,
            cursor: None,
        }),
    )
    .await
    .expect_err("no creator should 401");
    assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
}

// ─── T1.2: OUTLINE PUT — atomic file write + DB metadata ────────────────────

#[tokio::test]
async fn put_outline_creates_file_and_updates_metadata() {
    let (state, _tmp) = handler_state().await;
    let (work_id, work_ref) = create_work_with_chapters(&state, 3).await;
    let root = state.workspace_path().expect("workspace path");

    let content = "# Chapter 1\n\nOutline text.";
    let resp = chapters::put_chapter_outline(
        State(state.clone()),
        Path((work_id.clone(), "1".to_string())),
        Query(ChapterContentQuery { volume: None }),
        Json(PutChapterOutlineRequest {
            content: content.to_string(),
        }),
    )
    .await
    .expect("put outline");
    assert_eq!(resp.content, content);
    assert_eq!(
        resp.outline_path,
        format!("Works/{work_ref}/Outlines/chapters/ch01-outline.md")
    );
    assert!(!resp.updated_at.is_empty());

    let file_path = std::path::PathBuf::from(&root).join(format!(
        "Works/{work_ref}/Outlines/chapters/ch01-outline.md"
    ));
    assert!(file_path.exists(), "outline file should be created");

    let record = nexus_local_db::work_chapters::get_chapter(state.pool(), &work_id, 1, 1)
        .await
        .expect("db query")
        .expect("chapter row");
    assert_eq!(
        record.outline_path,
        Some(format!(
            "Works/{work_ref}/Outlines/chapters/ch01-outline.md"
        ))
    );
    assert!(!record.updated_at.is_empty());
}

#[tokio::test]
async fn put_outline_returns_404_for_unknown_chapter() {
    let (state, _tmp) = handler_state().await;
    let (work_id, _work_ref) = create_work_with_chapters(&state, 1).await;
    let err = chapters::put_chapter_outline(
        State(state),
        Path((work_id, "99".to_string())),
        Query(ChapterContentQuery { volume: None }),
        Json(PutChapterOutlineRequest {
            content: "text".to_string(),
        }),
    )
    .await
    .expect_err("unknown chapter should 404");
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
}

// ─── T1.3: STRUCTURE PATCH ──────────────────────────────────────────────────

#[tokio::test]
async fn patch_chapter_status_not_started_to_outlined_succeeds() {
    let (state, _tmp) = handler_state().await;
    let (work_id, _work_ref) = create_work_with_chapters(&state, 3).await;

    let resp = chapters::patch_chapter(
        State(state),
        Path((work_id, "1".to_string())),
        Query(ChapterContentQuery { volume: None }),
        Json(PatchChapterRequest {
            status: Some(ChapterStatus::Outlined),
            ..Default::default()
        }),
    )
    .await
    .expect("patch to outlined");
    assert_eq!(resp.status, ChapterStatus::Outlined);
}

#[tokio::test]
async fn patch_chapter_reverse_transition_is_rejected() {
    let (state, _tmp) = handler_state().await;
    let (work_id, _work_ref) = create_work_with_chapters(&state, 3).await;

    let _ = chapters::patch_chapter(
        State(state.clone()),
        Path((work_id.clone(), "1".to_string())),
        Query(ChapterContentQuery { volume: None }),
        Json(PatchChapterRequest {
            status: Some(ChapterStatus::Outlined),
            ..Default::default()
        }),
    )
    .await
    .expect("patch to outlined");

    let err = chapters::patch_chapter(
        State(state),
        Path((work_id, "1".to_string())),
        Query(ChapterContentQuery { volume: None }),
        Json(PatchChapterRequest {
            status: Some(ChapterStatus::NotStarted),
            ..Default::default()
        }),
    )
    .await
    .expect_err("reverse transition should fail");
    assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(
        bad_request_code(&err),
        Some("chapter_status_transition_invalid")
    );
}

#[tokio::test]
async fn patch_published_chapter_structure_is_hard_blocked() {
    let (state, _tmp) = handler_state().await;
    let (work_id, _work_ref) = create_work_with_chapters(&state, 3).await;

    nexus_local_db::work_chapters::update_status(
        state.pool(),
        &work_id,
        1,
        1,
        "published",
        None,
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .expect("set published");

    let err = chapters::patch_chapter(
        State(state),
        Path((work_id, "1".to_string())),
        Query(ChapterContentQuery { volume: None }),
        Json(PatchChapterRequest {
            slug: Some("new-slug".to_string()),
            ..Default::default()
        }),
    )
    .await
    .expect_err("published edit should fail");
    assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(
        bad_request_code(&err),
        Some("chapter_structure_edit_blocked")
    );
}

#[tokio::test]
async fn patch_finalized_chapter_structure_requires_confirmation() {
    let (state, _tmp) = handler_state().await;
    let (work_id, _work_ref) = create_work_with_chapters(&state, 3).await;

    nexus_local_db::work_chapters::update_status(
        state.pool(),
        &work_id,
        1,
        1,
        "finalized",
        None,
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .expect("set finalized");

    let err = chapters::patch_chapter(
        State(state.clone()),
        Path((work_id.clone(), "1".to_string())),
        Query(ChapterContentQuery { volume: None }),
        Json(PatchChapterRequest {
            slug: Some("new-slug".to_string()),
            ..Default::default()
        }),
    )
    .await
    .expect_err("finalized edit without confirmation should fail");
    assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
    assert_eq!(
        bad_request_code(&err),
        Some("chapter_structure_confirmation_required")
    );

    let resp = chapters::patch_chapter(
        State(state),
        Path((work_id, "1".to_string())),
        Query(ChapterContentQuery { volume: None }),
        Json(PatchChapterRequest {
            slug: Some("new-slug".to_string()),
            confirm_structural_edit: Some(true),
            ..Default::default()
        }),
    )
    .await
    .expect("finalized edit with confirmation");
    assert_eq!(resp.slug, Some("new-slug".to_string()));
}

// ─── T1.4: BODY GET — read-only markdown + frontmatter + W-002 guard ────────

#[tokio::test]
async fn get_body_returns_read_only_markdown_with_frontmatter() {
    let (state, _tmp) = handler_state().await;
    let (work_id, work_ref) = create_work_with_chapters(&state, 3).await;
    let root = state.workspace_path().expect("workspace path");

    let body_path =
        std::path::PathBuf::from(&root).join(format!("Works/{work_ref}/Stories/ch01-ch01.md"));
    std::fs::create_dir_all(body_path.parent().unwrap()).unwrap();
    let content = "---\ntitle: Chapter One\n---\n\nBody content here.";
    std::fs::write(&body_path, content).unwrap();

    let resp = chapters::get_chapter_body(
        State(state),
        Path((work_id.clone(), "1".to_string())),
        Query(ChapterContentQuery { volume: None }),
    )
    .await
    .expect("get body");
    assert_eq!(resp.content, content);
    assert_eq!(
        resp.body_path,
        format!("Works/{work_ref}/Stories/ch01-ch01.md")
    );
    assert!(resp.read_only);
}

#[tokio::test]
async fn get_body_rejects_escaped_body_path() {
    let (state, _tmp) = handler_state().await;
    let (work_id, _work_ref) = create_work_with_chapters(&state, 3).await;

    sqlx::query(
        "UPDATE work_chapters SET body_path = ? WHERE work_id = ? AND chapter = ? AND volume = ?",
    )
    .bind("../escape.md")
    .bind(&work_id)
    .bind(1)
    .bind(1)
    .execute(state.pool())
    .await
    .expect("update body_path");

    let err = chapters::get_chapter_body(
        State(state),
        Path((work_id, "1".to_string())),
        Query(ChapterContentQuery { volume: None }),
    )
    .await
    .expect_err("escaped path should fail");
    assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
    let code = bad_request_code(&err).expect("should be BadRequest");
    assert!(
        code == "chapter_body_path_forbidden" || code == "chapter_path_unresolvable",
        "unexpected code: {code}"
    );
}

#[tokio::test]
async fn get_body_returns_404_for_unknown_chapter() {
    let (state, _tmp) = handler_state().await;
    let (work_id, _work_ref) = create_work_with_chapters(&state, 1).await;
    let err = chapters::get_chapter_body(
        State(state),
        Path((work_id, "99".to_string())),
        Query(ChapterContentQuery { volume: None }),
    )
    .await
    .expect_err("unknown chapter should 404");
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
}
