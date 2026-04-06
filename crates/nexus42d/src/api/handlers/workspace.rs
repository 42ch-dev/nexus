//! Workspace handlers

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use crate::workspace::WorkspaceState;

#[derive(Serialize)]
pub struct WorkspaceInfo {
    pub initialized: bool,
    pub workspace_path: Option<String>,
    pub database_path: String,
}

/// GET /v1/local/workspace
pub async fn info(State(state): State<WorkspaceState>) -> Json<WorkspaceInfo> {
    Json(WorkspaceInfo {
        initialized: state.is_initialized().await,
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
) -> Json<InitWorkspaceResponse> {
    match state.init_workspace(&req.path).await {
        Ok(()) => Json(InitWorkspaceResponse {
            success: true,
            message: format!("Workspace initialized at {}", req.path),
        }),
        Err(e) => Json(InitWorkspaceResponse {
            success: false,
            message: format!("Failed to initialize: {}", e),
        }),
    }
}
