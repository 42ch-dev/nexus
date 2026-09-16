//! Thin reading-depth HTTP adapters over the guarded core reading service.
//!
//! Endpoints under `/v1/daemon/reading/*` translate the wire envelope to
//! `nexus_core` reading operations and back; scope/isolation, color/offset
//! validation, the tri-state note patch and storage semantics live in the
//! core (P1-T3).

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::daemon_api::reading::{
    ReadingAnnotation, ReadingAnnotationCreateRequest, ReadingAnnotationListQuery,
    ReadingAnnotationListResponse, ReadingAnnotationPatchRequest, ReadingProgressQuery,
    ReadingProgressRequest, ReadingProgressResponse,
};

/// Map a core reading fault onto the legacy HTTP classification.
///
/// `NotFound` resources round-trip verbatim; the `invalid_input` 422 code is
/// carried as `InvalidInput.field`; the foreign-creator annotation access
/// rides the `annotation_owner:` prefix carrier and re-emits the legacy
/// `Forbidden { resource, reason }` pair; the core `local_db_err` lowercase
/// `database_error: …` carrier is re-classified as the legacy
/// `DATABASE_ERROR`; every other internal category keeps the shared
/// `CORE_ERROR` shape.
fn reading_error(error: nexus_core::CoreError) -> NexusApiError {
    match error {
        nexus_core::CoreError::InvalidInput { field, reason } => NexusApiError::BadRequest {
            code: field,
            message: reason,
        },
        nexus_core::CoreError::NotFound { resource } => NexusApiError::NotFound(resource),
        nexus_core::CoreError::Forbidden { resource } => {
            let foreign = resource
                .strip_prefix("annotation_owner:")
                .map(str::to_owned);
            NexusApiError::Forbidden {
                resource: foreign.clone().unwrap_or(resource),
                reason: foreign.map_or_else(
                    || "forbidden".to_string(),
                    |_| "annotation belongs to a different creator".to_string(),
                ),
            }
        }
        nexus_core::CoreError::Internal { category } => match category.split_once(": ") {
            Some(("database_error", message)) => NexusApiError::Internal {
                code: "DATABASE_ERROR".to_owned(),
                message: message.to_owned(),
            },
            _ => nexus_core::CoreError::Internal { category }.into(),
        },
        other => other.into(),
    }
}

/// `GET /v1/daemon/reading/progress` — get persisted scroll progress.
pub async fn get_reading_progress(
    State(state): State<WorkspaceState>,
    Query(query): Query<ReadingProgressQuery>,
) -> Result<Json<ReadingProgressResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core
        .get_reading_progress(&principal, query)
        .await
        .map_err(reading_error)?;
    Ok(Json(response))
}

/// `PUT /v1/daemon/reading/progress` — upsert scroll progress.
pub async fn put_reading_progress(
    State(state): State<WorkspaceState>,
    Json(body): Json<ReadingProgressRequest>,
) -> Result<Json<ReadingProgressResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core
        .put_reading_progress(&principal, body.work_id.clone(), body)
        .await
        .map_err(reading_error)?;
    Ok(Json(response))
}

/// `DELETE /v1/daemon/reading/progress` — delete persisted scroll progress.
pub async fn delete_reading_progress(
    State(state): State<WorkspaceState>,
    Query(query): Query<ReadingProgressQuery>,
) -> Result<axum::http::StatusCode, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    core.delete_reading_progress(&principal, query)
        .await
        .map_err(reading_error)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `GET /v1/daemon/reading/annotations` — list annotations for a chapter.
pub async fn list_annotations(
    State(state): State<WorkspaceState>,
    Query(query): Query<ReadingAnnotationListQuery>,
) -> Result<Json<ReadingAnnotationListResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core
        .list_annotations(&principal, query)
        .await
        .map_err(reading_error)?;
    Ok(Json(response))
}

/// `POST /v1/daemon/reading/annotations` — create an annotation.
pub async fn create_annotation(
    State(state): State<WorkspaceState>,
    Json(body): Json<ReadingAnnotationCreateRequest>,
) -> Result<Json<ReadingAnnotation>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let annotation = core
        .create_annotation(&principal, body)
        .await
        .map_err(reading_error)?;
    Ok(Json(annotation))
}

/// `PATCH /v1/daemon/reading/annotations/{annotation_id}` — edit an annotation.
pub async fn patch_annotation(
    State(state): State<WorkspaceState>,
    Path(annotation_id): Path<String>,
    Json(body): Json<ReadingAnnotationPatchRequest>,
) -> Result<Json<ReadingAnnotation>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let annotation = core
        .patch_annotation(&principal, annotation_id, body)
        .await
        .map_err(reading_error)?;
    Ok(Json(annotation))
}

/// `DELETE /v1/daemon/reading/annotations/{annotation_id}` — delete an annotation.
pub async fn delete_annotation(
    State(state): State<WorkspaceState>,
    Path(annotation_id): Path<String>,
) -> Result<axum::http::StatusCode, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    core.delete_annotation(&principal, annotation_id)
        .await
        .map_err(reading_error)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
