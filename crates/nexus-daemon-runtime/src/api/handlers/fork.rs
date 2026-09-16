//! Local timeline fork HTTP translation. Core owns fork-point validation,
//!
//! branch allocation, the canon lineage marker and the world-ownership
//! guard (whose typed denial renders the retained 403/404 envelopes here).

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, resolve_core_principal};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::daemon_api::{CreateForkRequest, CreateForkResponse};
use serde_json::json;

///
/// # Errors
///
/// Returns [`NexusApiError`] when the creator/workspace guard rejects the
/// request, the core authority denies it (ownership, admission or validation),
/// or the bounded store read/write fails.
pub async fn create_fork(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<CreateForkRequest>,
) -> Result<Json<CreateForkResponse>, NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .create_fork(&principal, world_id, req)
        .await
        .map_err(|e| match e {
            nexus_core::CoreError::InvalidInput { .. } => NexusApiError::InputValidationFailed {
                details: json!({ "fork_point": "fork point not found on parent branch" }),
            },
            other => NexusApiError::from(other),
        })?;
    Ok(Json(response))
}
