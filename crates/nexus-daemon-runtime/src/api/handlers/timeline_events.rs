//! Daemon route — `GET /v1/daemon/worlds/:world_id/timeline/events`
//! (V1.147 P2 T1): thin HTTP translation over the core timeline read
//! (v1.190 P0-T3).
//!
//! The keyset-cursored `narrative_timeline_events` page read — branch /
//! status / `event_type` filters, `ev1:` cursor encoding, row mapping with
//! modules/extensions — lives in `nexus-core`
//! (`CoreService::list_timeline_events`). This handler keeps the retained
//! guard-first envelopes (404 `world {id} not found` / 403 `you do not own
//! this world`) and auth/DTO/status translation. P5-T4 owns HTTP SSE
//! framing and gap/disconnect on top of the core's bounded pull.

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, resolve_core_principal};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::daemon_api::timeline::list_timeline_events_response::ListTimelineEventsResponse;
use nexus_core::CoreTimelineEventsQuery;
use nexus_local_db::narrative_write::is_world_owned;
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
/// Ownership guard runs before any read: world must exist (404) and be owned
/// by the active creator (403). Core independently checks ownership for
/// non-HTTP callers.
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
    let pool = state.pool_or_uninit()?;
    let creator_id = require_creator(&state)?;

    // World existence (404) before any read; retained envelope message.
    // Compile-time checked query (daemon-runtime AGENTS.md mandatory rule).
    let _world = sqlx::query!(
        r#"SELECT root_fork_branch_id as "root_fork_branch_id" FROM narrative_worlds WHERE world_id = ?"#,
        world_id,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?
    .ok_or_else(|| NexusApiError::NotFound(format!("world {world_id} not found")))?;

    // Ownership guard (403) before any read; retained envelope reason. Core
    // independently checks ownership for non-HTTP callers.
    let owned = is_world_owned(pool, &creator_id, &world_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    if !owned {
        return Err(NexusApiError::Forbidden {
            resource: format!("world {world_id}"),
            reason: "you do not own this world".to_string(),
        });
    }

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
