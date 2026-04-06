//! Runtime handlers — health check and status

use axum::{extract::State, Json};
use serde::Serialize;
use crate::workspace::WorkspaceState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// GET /v1/local/runtime/health
pub async fn health(State(_state): State<WorkspaceState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub uptime_seconds: u64,
    pub workspace_initialized: bool,
}

/// GET /v1/local/runtime/status
pub async fn status(State(state): State<WorkspaceState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.uptime_seconds().await,
        workspace_initialized: state.is_initialized().await,
    })
}
