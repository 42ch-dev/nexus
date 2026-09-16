//! Moment Directive route — `POST /v1/daemon/moment-directive` (V1.151 P0,
//!
//! DF-76). Thin translation over the core Moment Directive family
//! (v1.190 P2-T3): scope ownership (retained 403), CLI-parity validation,
//! the set/show/clear semantics with Work-wins / World-override precedence
//! and the `replace` conflict all live in [`nexus_core`]; the handler keeps
//! only wire parsing and envelope translation.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::resolve_core_principal;
use crate::workspace::WorkspaceState;
use axum::{extract::State, Json};
use nexus_contracts::generated::daemon_api::inspector::moment_directive_request::MomentDirectiveRequest;
use nexus_contracts::generated::daemon_api::inspector::moment_directive_response::MomentDirectiveResponse;

/// Set / show / clear the active Moment Directive for an owned scope.
#[allow(clippy::missing_errors_doc)]
pub async fn moment_directive(
    State(state): State<WorkspaceState>,
    Json(req): Json<MomentDirectiveRequest>,
) -> Result<Json<MomentDirectiveResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core.moment_directive(&principal, req).await?;
    Ok(Json(response))
}
