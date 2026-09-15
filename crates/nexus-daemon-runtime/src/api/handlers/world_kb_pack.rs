//! World Pack HTTP translation. Core owns parsing, conflict policy, export
//! assembly and guarded persistence. Shared HTTP guards retain their envelopes.

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, require_world_owner, resolve_core_principal};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::daemon_api::kb::{PackExportRequest, PackExportResponse, PackImportRequest, PackImportResponse};

/// Export an owned World's lore; the response remains the handbook pack.
pub async fn pack_export(State(state): State<WorkspaceState>, Path(world_id): Path<String>, Json(req): Json<PackExportRequest>) -> Result<Json<PackExportResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id = require_creator(&state)?;
    require_world_owner(pool, &world_id, &creator_id).await?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.export_world_pack(&principal, world_id, req).await.map(Json).map_err(NexusApiError::from)
}

/// Import a pack; pack-level source anchors remain an accepted no-op.
pub async fn pack_import(State(state): State<WorkspaceState>, Path(world_id): Path<String>, Json(req): Json<PackImportRequest>) -> Result<Json<PackImportResponse>, NexusApiError> {
    let pool = state.pool_or_uninit()?;
    let creator_id = require_creator(&state)?;
    require_world_owner(pool, &world_id, &creator_id).await?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.import_world_pack(&principal, world_id, req).await.map(Json).map_err(NexusApiError::from)
}
