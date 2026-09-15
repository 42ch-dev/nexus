//! Strategy canvas transport adapters. Authoring and OCC live in nexus-core.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::{
    StrategyPatchPromptTemplateRequest, StrategyPatchResponse, StrategyPatchStateRequest,
    StrategyPatchTransitionRequest,
};

/// Patch a state, retaining the Strategy response envelope.
pub async fn patch_state(
    State(state): State<WorkspaceState>,
    Path((strategy_id, state_id)): Path<(String, String)>,
    Json(request): Json<StrategyPatchStateRequest>,
) -> Result<Json<StrategyPatchResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core.patch_strategy_state(&principal, strategy_id, state_id, request)
        .await?;
    Ok(Json(super::wire_cast(response)))
}

/// Patch a transition, retaining the Strategy response envelope.
pub async fn patch_transition(
    State(state): State<WorkspaceState>,
    Path(strategy_id): Path<String>,
    Json(request): Json<StrategyPatchTransitionRequest>,
) -> Result<Json<StrategyPatchResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core.patch_strategy_transition(&principal, strategy_id, request)
        .await?;
    Ok(Json(super::wire_cast(response)))
}

/// Patch prompt bytes, retaining the Strategy response envelope.
pub async fn patch_prompt_template(
    State(state): State<WorkspaceState>,
    Path((strategy_id, state_id)): Path<(String, String)>,
    Json(request): Json<StrategyPatchPromptTemplateRequest>,
) -> Result<Json<StrategyPatchResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core.patch_strategy_prompt_template(&principal, strategy_id, state_id, request)
        .await?;
    Ok(Json(super::wire_cast(response)))
}
