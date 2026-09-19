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
/// The export reads through the exporting Creator's admitted selection: shared
/// rows always, owned known-private material only under the explicit
/// `include_owned_private` author intent (v1.191 P1 T10).
pub async fn pack_export(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<PackExportRequest>,
) -> Result<Json<PackExportResponse>, NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let include_owned_private = req.include_owned_private;
    core.export_world_pack(&principal, world_id, req, include_owned_private)
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
///
/// The two frozen arms (v1.191 P1 T10) are dispatched by the core authority:
/// `review_import` alone reads one import batch's quarantined atoms back, and
/// the import arm adopts foreign-governed atoms only through its `holder_map`.
/// Mixing the arms is refused as invalid input.
pub async fn pack_import(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<PackImportRequest>,
) -> Result<Json<PackImportResponse>, NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.dispatch_world_pack_import(&principal, world_id, req)
        .await
        .map(Json)
        .map_err(NexusApiError::from)
}
