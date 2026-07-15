//! Reading depth Daemon API handlers (V1.89).
//!
//! Endpoints under `/v1/daemon/reading/*` expose persisted scroll progress and
//! character-offset annotations/highlights per (creator, work, chapter).

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::{read_active_creator_id, read_active_workspace_slug};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::daemon_api::reading::{
    ReadingAnnotation, ReadingAnnotationCreateRequest, ReadingAnnotationListQuery,
    ReadingAnnotationListResponse, ReadingAnnotationPatchRequest, ReadingProgressQuery,
    ReadingProgressRequest, ReadingProgressResponse,
};
use nexus_local_db::reading::{self, AnnotationRow};
use nexus_local_db::works;
use uuid::Uuid;

// ─── Helpers ───────────────────────────────────────────────────────────────

const ANNOTATION_ID_PREFIX: &str = "ann_";
const VALID_COLORS: [&str; 4] = ["yellow", "blue", "green", "pink"];

/// Format a `DateTime` as an ISO 8601 string.
fn format_timestamp(dt: &chrono::DateTime<chrono::Utc>) -> String {
    dt.to_rfc3339()
}

/// Convert a `u64` offset/progress value to `i64`, returning a 400 if it
/// does not fit (defense-in-depth; schema bounds keep values well within range).
fn u64_to_i64(value: u64, field: &str) -> Result<i64, NexusApiError> {
    i64::try_from(value).map_err(|_| NexusApiError::BadRequest {
        code: "invalid_input".to_string(),
        message: format!("{field} exceeds maximum supported value"),
    })
}

/// Validate a highlight color against the V1.89 enum.
fn validate_color(color: &str) -> Result<(), NexusApiError> {
    if VALID_COLORS.contains(&color) {
        Ok(())
    } else {
        Err(NexusApiError::BadRequest {
            code: "invalid_input".to_string(),
            message: format!(
                "color must be one of {}, got '{color}'",
                VALID_COLORS.join(", ")
            ),
        })
    }
}

/// Ensure the active creator exists and an active workspace is selected.
fn require_active_scope(state: &WorkspaceState) -> Result<String, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::Uninitialized)?;
    Ok(creator_id)
}

/// Verify that `work_id` belongs to the active creator.
async fn verify_work_ownership(
    state: &WorkspaceState,
    creator_id: &str,
    work_id: &str,
) -> Result<(), NexusApiError> {
    works::get_work(state.pool_or_uninit()?, creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;
    Ok(())
}

/// Map a database annotation row to the contract DTO.
fn to_annotation_dto(row: &AnnotationRow) -> ReadingAnnotation {
    ReadingAnnotation {
        annotation_id: row.annotation_id.clone(),
        work_id: row.work_id.clone(),
        chapter: row.chapter,
        start_offset: u64::try_from(row.start_offset).unwrap_or_default(),
        end_offset: u64::try_from(row.end_offset).unwrap_or_default(),
        selected_text: row.selected_text.clone(),
        color: row.color.clone(),
        note: row.note.clone(),
        created_at: format_timestamp(&row.created_at),
        updated_at: format_timestamp(&row.updated_at),
    }
}

/// Fetch an annotation and enforce active-creator ownership.
async fn load_annotation_for_creator(
    state: &WorkspaceState,
    creator_id: &str,
    annotation_id: &str,
) -> Result<AnnotationRow, NexusApiError> {
    let row = reading::get_annotation(state.pool_or_uninit()?, annotation_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("annotation {annotation_id}")))?;

    if row.creator_id != creator_id {
        return Err(NexusApiError::Forbidden {
            resource: format!("annotation {annotation_id}"),
            reason: "annotation belongs to a different creator".to_string(),
        });
    }

    Ok(row)
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// `GET /v1/daemon/reading/progress` — get persisted scroll progress.
pub async fn get_reading_progress(
    State(state): State<WorkspaceState>,
    Query(query): Query<ReadingProgressQuery>,
) -> Result<Json<ReadingProgressResponse>, NexusApiError> {
    let creator_id = require_active_scope(&state)?;
    verify_work_ownership(&state, &creator_id, &query.work_id).await?;

    let chapter = query.chapter;
    let (scroll_progress, updated_at) = reading::get_reading_progress(
        state.pool_or_uninit()?,
        &creator_id,
        &query.work_id,
        chapter,
    )
    .await?;

    let response = ReadingProgressResponse {
        work_id: query.work_id,
        chapter,
        scroll_progress: u64::try_from(scroll_progress).unwrap_or_default(),
        updated_at: updated_at.map_or_else(
            || format_timestamp(&chrono::Utc::now()),
            |ts| format_timestamp(&ts),
        ),
    };

    Ok(Json(response))
}

/// `PUT /v1/daemon/reading/progress` — upsert scroll progress.
pub async fn put_reading_progress(
    State(state): State<WorkspaceState>,
    Json(body): Json<ReadingProgressRequest>,
) -> Result<Json<ReadingProgressResponse>, NexusApiError> {
    let creator_id = require_active_scope(&state)?;
    verify_work_ownership(&state, &creator_id, &body.work_id).await?;

    let chapter = body.chapter;
    let scroll_progress = u64_to_i64(body.scroll_progress, "scroll_progress")?;
    let updated_at = reading::upsert_reading_progress(
        state.pool_or_uninit()?,
        &creator_id,
        &body.work_id,
        chapter,
        scroll_progress,
    )
    .await?;

    let response = ReadingProgressResponse {
        work_id: body.work_id,
        chapter,
        scroll_progress: body.scroll_progress,
        updated_at: format_timestamp(&updated_at),
    };

    Ok(Json(response))
}

/// `DELETE /v1/daemon/reading/progress` — delete persisted scroll progress.
pub async fn delete_reading_progress(
    State(state): State<WorkspaceState>,
    Query(query): Query<ReadingProgressQuery>,
) -> Result<axum::http::StatusCode, NexusApiError> {
    let creator_id = require_active_scope(&state)?;
    verify_work_ownership(&state, &creator_id, &query.work_id).await?;

    reading::delete_reading_progress(
        state.pool_or_uninit()?,
        &creator_id,
        &query.work_id,
        query.chapter,
    )
    .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `GET /v1/daemon/reading/annotations` — list annotations for a chapter.
pub async fn list_annotations(
    State(state): State<WorkspaceState>,
    Query(query): Query<ReadingAnnotationListQuery>,
) -> Result<Json<ReadingAnnotationListResponse>, NexusApiError> {
    let creator_id = require_active_scope(&state)?;
    verify_work_ownership(&state, &creator_id, &query.work_id).await?;

    let rows = reading::list_annotations(
        state.pool_or_uninit()?,
        &creator_id,
        &query.work_id,
        query.chapter,
    )
    .await?;

    let items = rows.iter().map(to_annotation_dto).collect();
    let response = ReadingAnnotationListResponse { items };

    Ok(Json(response))
}

/// `POST /v1/daemon/reading/annotations` — create an annotation.
pub async fn create_annotation(
    State(state): State<WorkspaceState>,
    Json(body): Json<ReadingAnnotationCreateRequest>,
) -> Result<Json<ReadingAnnotation>, NexusApiError> {
    let creator_id = require_active_scope(&state)?;
    verify_work_ownership(&state, &creator_id, &body.work_id).await?;

    validate_color(&body.color)?;

    let start_offset = u64_to_i64(body.start_offset, "start_offset")?;
    let end_offset = u64_to_i64(body.end_offset, "end_offset")?;
    let annotation_id = format!("{ANNOTATION_ID_PREFIX}{}", Uuid::new_v4().simple());

    let row = reading::create_annotation(
        state.pool_or_uninit()?,
        &creator_id,
        &body.work_id,
        body.chapter,
        &annotation_id,
        start_offset,
        end_offset,
        &body.selected_text,
        &body.color,
        body.note.as_deref(),
    )
    .await?;

    Ok(Json(to_annotation_dto(&row)))
}

/// `PATCH /v1/daemon/reading/annotations/{annotation_id}` — edit an annotation.
pub async fn patch_annotation(
    State(state): State<WorkspaceState>,
    Path(annotation_id): Path<String>,
    Json(body): Json<ReadingAnnotationPatchRequest>,
) -> Result<Json<ReadingAnnotation>, NexusApiError> {
    let creator_id = require_active_scope(&state)?;
    let _ = load_annotation_for_creator(&state, &creator_id, &annotation_id).await?;

    if let Some(ref color) = body.color {
        validate_color(color)?;
    }

    // Translate empty-string note to None (clear note); missing field stays None
    // on the request, which we interpret as "do not change" via `note: None` below.
    let note_change = body.note.as_ref().map(|n| {
        let trimmed = n.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let row = reading::update_annotation(
        state.pool_or_uninit()?,
        &annotation_id,
        body.color.as_deref(),
        note_change,
    )
    .await?
    .ok_or_else(|| NexusApiError::NotFound(format!("annotation {annotation_id}")))?;

    Ok(Json(to_annotation_dto(&row)))
}

/// `DELETE /v1/daemon/reading/annotations/{annotation_id}` — delete an annotation.
pub async fn delete_annotation(
    State(state): State<WorkspaceState>,
    Path(annotation_id): Path<String>,
) -> Result<axum::http::StatusCode, NexusApiError> {
    let creator_id = require_active_scope(&state)?;
    let _ = load_annotation_for_creator(&state, &creator_id, &annotation_id).await?;

    reading::delete_annotation(state.pool_or_uninit()?, &annotation_id).await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
