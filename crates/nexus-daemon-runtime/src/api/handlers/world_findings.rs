//! World findings HTTP translation. Core owns the bounded read, the
//! projection and the world-ownership guard (typed denial → retained
//! envelopes).

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, resolve_core_principal};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::daemon_api::WorldFindingsListResponse;

pub async fn list_world_findings(State(state): State<WorkspaceState>, Path(world_id): Path<String>) -> Result<Json<WorldFindingsListResponse>, NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.list_world_findings(&principal, world_id).await.map(Json).map_err(NexusApiError::from)
}
