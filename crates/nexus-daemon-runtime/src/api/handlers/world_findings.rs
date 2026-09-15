//! World findings HTTP translation. Core owns the bounded read and projection.

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, require_world_owner, resolve_core_principal};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::daemon_api::WorldFindingsListResponse;

pub async fn list_world_findings(State(state): State<WorkspaceState>, Path(world_id): Path<String>) -> Result<Json<WorldFindingsListResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    require_world_owner(state.pool_or_uninit()?, &world_id, &creator_id).await?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.list_world_findings(&principal, world_id).await.map(Json).map_err(NexusApiError::from)
}
