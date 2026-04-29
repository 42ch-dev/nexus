//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Workspace handlers

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

#[derive(Serialize)]
pub struct WorkspaceInfo {
    pub initialized: bool,
    pub workspace_path: Option<String>,
    pub database_path: String,
}

/// GET /v1/local/workspace
pub async fn info(State(state): State<WorkspaceState>) -> Json<WorkspaceInfo> {
    info!("Handling workspace info request");
    Json(WorkspaceInfo {
        initialized: state.is_initialized(),
        workspace_path: state.workspace_path(),
        database_path: state.database_path(),
    })
}

#[derive(Deserialize)]
pub struct InitWorkspaceRequest {
    pub path: String,
}

#[derive(Serialize)]
pub struct InitWorkspaceResponse {
    pub success: bool,
    pub message: String,
}

/// POST /v1/local/workspace/init
pub async fn init_workspace(
    State(state): State<WorkspaceState>,
    Json(req): Json<InitWorkspaceRequest>,
) -> Result<Json<InitWorkspaceResponse>, NexusApiError> {
    info!("Handling workspace init request");
    debug!(path = %req.path, "Initializing workspace");

    // Validate input
    if req.path.trim().is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "path".into(),
            reason: "must not be empty".into(),
        });
    }

    state
        .init_workspace(&req.path)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "WORKSPACE_INIT_FAILED".into(),
            message: e.to_string(),
        })?;

    info!("Workspace init completed");
    Ok(Json(InitWorkspaceResponse {
        success: true,
        message: format!("Workspace initialized at {}", req.path),
    }))
}
