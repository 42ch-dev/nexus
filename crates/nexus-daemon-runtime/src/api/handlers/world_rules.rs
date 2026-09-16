//! World rule HTTP translation; validation, store orchestration and the
//! world-ownership guard live in core (typed denial → retained envelopes).

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, resolve_core_principal};
use crate::workspace::WorkspaceState;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_contracts::daemon_api::worlds::{
    WorldRuleCreateRequest, WorldRuleResponse, WorldRuleUpdateRequest, WorldRulesListResponse,
};

pub async fn list_world_rules(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
) -> Result<Json<WorldRulesListResponse>, NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.list_world_rules(&principal, world_id)
        .await
        .map(Json)
        .map_err(NexusApiError::from)
}

pub async fn create_world_rule(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
    Json(req): Json<WorldRuleCreateRequest>,
) -> Result<(StatusCode, Json<WorldRuleResponse>), NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .create_world_rule(&principal, world_id, req)
        .await
        .map_err(NexusApiError::from)?;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn update_world_rule(
    State(state): State<WorkspaceState>,
    Path((world_id, rule_id)): Path<(String, String)>,
    Json(req): Json<WorldRuleUpdateRequest>,
) -> Result<Json<WorldRuleResponse>, NexusApiError> {
    require_creator(&state)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.update_world_rule(&principal, world_id, rule_id, req)
        .await
        .map(Json)
        .map_err(NexusApiError::from)
}
