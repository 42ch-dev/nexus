//! World rule HTTP translation; validation and store orchestration live in core.

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::{require_creator, require_world_owner, resolve_core_principal};
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use axum::http::StatusCode;
use nexus_contracts::daemon_api::worlds::{WorldRuleCreateRequest, WorldRuleUpdateRequest, WorldRuleResponse, WorldRulesListResponse};

pub async fn list_world_rules(State(state): State<WorkspaceState>, Path(world_id): Path<String>) -> Result<Json<WorldRulesListResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    require_world_owner(state.pool_or_uninit()?, &world_id, &creator_id).await?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.list_world_rules(&principal, world_id).await.map(Json).map_err(NexusApiError::from)
}

pub async fn create_world_rule(State(state): State<WorkspaceState>, Path(world_id): Path<String>, Json(req): Json<WorldRuleCreateRequest>) -> Result<(StatusCode, Json<WorldRuleResponse>), NexusApiError> {
    let creator_id = require_creator(&state)?;
    require_world_owner(state.pool_or_uninit()?, &world_id, &creator_id).await?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core.create_world_rule(&principal, world_id, req).await.map_err(NexusApiError::from)?;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn update_world_rule(State(state): State<WorkspaceState>, Path((world_id, rule_id)): Path<(String, String)>, Json(req): Json<WorldRuleUpdateRequest>) -> Result<Json<WorldRuleResponse>, NexusApiError> {
    let creator_id = require_creator(&state)?;
    require_world_owner(state.pool_or_uninit()?, &world_id, &creator_id).await?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.update_world_rule(&principal, world_id, rule_id, req).await.map(Json).map_err(NexusApiError::from)
}
