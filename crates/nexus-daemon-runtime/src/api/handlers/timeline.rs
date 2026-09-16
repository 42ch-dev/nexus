//! Daemon route — `GET /v1/daemon/timeline/overview` (thin translation).
//!
//! v1.190 P0-T3: the overview projection (aggregated era/event counts,
//! keyset cursor on `world_id`) lives in `nexus-core`
//! (`CoreService::timeline_overview`); this handler keeps auth/DTO/status
//! translation only.

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::resolve_core_principal;
use crate::workspace::WorkspaceState;
use axum::extract::{Query, State};
use axum::Json;
use nexus_contracts::TimelineOverviewResponse;
use nexus_core::CoreTimelineOverviewQuery;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TimelineOverviewParams {
    pub cursor: Option<String>,
}

/// `GET /v1/daemon/timeline/overview`
///
/// # Errors
///
/// - `401 AuthRequired` if no active creator/workspace is configured.
/// - `400 invalid_input` for a malformed cursor.
/// - `500 Internal` on database error.
pub async fn get_timeline_overview(
    State(state): State<WorkspaceState>,
    Query(params): Query<TimelineOverviewParams>,
) -> Result<Json<TimelineOverviewResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .timeline_overview(
            &principal,
            CoreTimelineOverviewQuery {
                cursor: params.cursor,
            },
        )
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(response))
}
