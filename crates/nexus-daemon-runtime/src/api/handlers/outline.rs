//! Thin canvas outline+timeline HTTP adapters over the guarded core authoring
//! service.
//!
//! Endpoints under `/v1/daemon/works/{work_id}/outline/*` and
//! `/v1/daemon/works/{work_id}/timeline/*` translate the wire envelope to
//! `nexus_core` outline operations and back; `outline_revision` CAS, per-Work
//! locks, validation rules and the frontmatter file format live in the core.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::{
    OutlinePatchChapterRequest, OutlinePatchResponse, OutlinePatchStructureRequest,
    TimelinePatchEventRequest, WorkOutline,
};

/// `GET /v1/daemon/works/{work_id}/outline` — canonical work outline + timeline.
pub async fn get_work_outline(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
) -> Result<Json<WorkOutline>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let outline = core.work_outline(&principal, work_id).await.map_err(super::chapters::content_error)?;
    Ok(Json(outline))
}

/// `POST /v1/daemon/works/{work_id}/outline/patch` — structured outline patch.
pub async fn patch_outline_structure(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<OutlinePatchStructureRequest>,
) -> Result<Json<OutlinePatchResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core
        .patch_outline_structure(&principal, work_id, req)
        .await
        .map_err(super::chapters::content_error)?;
    Ok(Json(response))
}

/// `POST /v1/daemon/works/{work_id}/chapters/{chapter_id}/patch` — outline chapter patch.
pub async fn patch_outline_chapter(
    State(state): State<WorkspaceState>,
    Path((work_id, n)): Path<(String, String)>,
    Json(req): Json<OutlinePatchChapterRequest>,
) -> Result<Json<OutlinePatchResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core
        .patch_outline_chapter(&principal, work_id, n, req)
        .await
        .map_err(super::chapters::content_error)?;
    Ok(Json(response))
}

/// `POST /v1/daemon/works/{work_id}/timeline/patch` — structured timeline patch.
pub async fn patch_timeline_event(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<TimelinePatchEventRequest>,
) -> Result<Json<OutlinePatchResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core
        .patch_timeline_event(&principal, work_id, req)
        .await
        .map_err(super::chapters::content_error)?;
    Ok(Json(response))
}
