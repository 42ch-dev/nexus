//! Inspector handlers — Daemon HTTP surface for the enriched MCA assembly
//! inspector packet (V1.151 P0, DF-76). Thin translation over the core
//! inspector assembly (v1.190 P2-T3): world ownership (retained 403), the
//! work→world binding check (400, QC2-S-001), CLI-parity `MomentRequest`
//! wiring, read-only directive composition and the bounded packet builder
//! all live in [`nexus_core`]; the handler keeps only wire parsing and
//! envelope translation.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::resolve_core_principal;
use crate::workspace::WorkspaceState;
use axum::{extract::State, Json};
use nexus_contracts::generated::daemon_api::inspector::{
    moment_inspect_request::MomentInspectRequest, moment_inspect_response::MomentInspectResponse,
};

/// Assemble one moment over an owned World and return the enriched
/// inspector packet.
#[allow(clippy::missing_errors_doc)]
pub async fn inspect_moment(
    State(state): State<WorkspaceState>,
    Json(req): Json<MomentInspectRequest>,
) -> Result<Json<MomentInspectResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core.inspect_moment(&principal, req).await?;
    Ok(Json(response))
}
