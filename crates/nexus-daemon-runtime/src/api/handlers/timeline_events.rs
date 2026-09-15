//! Daemon route — `GET /v1/daemon/worlds/:world_id/timeline/events`
//! (V1.147 P2 T1): thin HTTP translation over the core timeline read
//! (v1.190 P0-T3).
//!
//! The keyset-cursored `narrative_timeline_events` page read — branch /
//! status / `event_type` filters, `ev1:` cursor encoding, row mapping with
//! modules/extensions, and the world-ownership guard — lives in
//! `nexus-core` (`CoreService::list_timeline_events`). The core guard's
//! typed denial renders this route's retained envelopes verbatim (404
//! `world {id} not found` / 403 `you do not own this world`); the handler
//! keeps auth/DTO/status translation only. P5-T4 owns HTTP SSE framing
//! and gap/disconnect on top of the core's bounded pull.

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, resolve_core_principal};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::daemon_api::timeline::list_timeline_events_response::ListTimelineEventsResponse;
use nexus_core::CoreTimelineEventsQuery;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TimelineEventsParams {
    pub branch_id: Option<String>,
    pub status: Option<String>,
    pub event_type: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

/// `GET /v1/daemon/worlds/:world_id/timeline/events`
///
/// Ownership/existence guard runs inside the core read before any page
/// query; its typed denial renders the retained envelopes (404/403).
///
/// # Errors
///
/// - `401 AuthRequired` if no active creator/workspace is configured.
/// - `404 NotFound` for an unknown world.
/// - `403 Forbidden` for a world owned by another creator.
/// - `400 invalid_input` for a malformed cursor or status filter.
/// - `500 Internal` on database error.
#[allow(clippy::missing_errors_doc)]
pub async fn get_timeline_events(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Query(params): Query<TimelineEventsParams>,
) -> Result<Json<ListTimelineEventsResponse>, NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .list_timeline_events(
            &principal,
            world_id,
            CoreTimelineEventsQuery {
                branch_id: params.branch_id,
                status: params.status,
                event_type: params.event_type,
                limit: params.limit,
                cursor: params.cursor,
            },
        )
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(response))
}
