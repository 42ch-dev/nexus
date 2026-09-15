//! Local timeline fork HTTP translation. Core owns fork-point validation,
//! branch allocation and the canon lineage marker through narrative_write.

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, require_world_owner, resolve_core_principal};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::daemon_api::{CreateForkRequest, CreateForkResponse};
use serde_json::json;

pub async fn create_fork(State(state): State<WorkspaceState>, Path(world_id): Path<String>, Json(req): Json<CreateForkRequest>) -> Result<Json<CreateForkResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id = require_creator(&state)?;
    // Preserve the guard-first 403 resource/reason envelope, before any
    // fork-point read. Core independently checks ownership for non-HTTP callers.
    require_world_owner(pool, &world_id, &creator_id).await?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core.create_fork(&principal, world_id, req).await.map_err(|e| match e {
        nexus_core::CoreError::InvalidInput { .. } => NexusApiError::InputValidationFailed {
            details: json!({ "fork_point": "fork point not found on parent branch" }),
        },
        other => NexusApiError::from(other),
    })?;
    Ok(Json(response))
}
