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

///
/// # Errors
///
/// Returns [`NexusApiError`] when the creator/workspace guard rejects the
/// request, the core authority denies it (ownership, admission or validation),
/// or the bounded store read/write fails.
/// Export an owned World's lore; the response remains the handbook pack.
///
/// The export reads through the exporting Creator's admitted selection. The
/// explicit author intent to include owned known-private material
/// (`include_owned_private`) is a frozen schema input for the custodian
/// checkpoint; the generated request type does not carry it yet, so this arm
/// serves the shared-only admitted export.
pub async fn pack_export(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<PackExportRequest>,
) -> Result<Json<PackExportResponse>, NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.export_world_pack(&principal, world_id, req, false)
        .await
        .map(Json)
        .map_err(NexusApiError::from)
}

///
/// # Errors
///
/// Returns [`NexusApiError`] when the creator/workspace guard rejects the
/// request, the core authority denies it (ownership, admission or validation),
/// or the bounded store read/write fails.
/// Import a pack; pack-level source anchors remain an accepted no-op.
pub async fn pack_import(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<PackImportRequest>,
) -> Result<Json<PackImportResponse>, NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.import_world_pack(&principal, world_id, req, Vec::new())
        .await
        .map(Json)
        .map_err(NexusApiError::from)
}
