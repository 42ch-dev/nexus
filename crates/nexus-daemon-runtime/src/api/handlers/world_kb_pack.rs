//! World Pack HTTP translation. Core owns parsing, conflict policy, export
//! assembly, guarded persistence and the world-ownership guard (typed
//! denial → retained envelopes).

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, resolve_core_principal};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::daemon_api::kb::{
    PackExportRequest, PackExportResponse, PackImportRequest, PackImportResponse,
};

/// Export an owned World's lore; the response remains the handbook pack.
pub async fn pack_export(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<PackExportRequest>,
) -> Result<Json<PackExportResponse>, NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.export_world_pack(&principal, world_id, req)
        .await
        .map(Json)
        .map_err(NexusApiError::from)
}

/// Import a pack; pack-level source anchors remain an accepted no-op.
pub async fn pack_import(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<PackImportRequest>,
) -> Result<Json<PackImportResponse>, NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.import_world_pack(&principal, world_id, req)
        .await
        .map(Json)
        .map_err(NexusApiError::from)
}
