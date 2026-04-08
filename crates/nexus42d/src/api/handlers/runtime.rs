//! Runtime handlers — health check and status

use crate::workspace::WorkspaceState;
use axum::{extract::State, Json};
use serde::Serialize;
use tracing::info;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// GET /v1/local/runtime/health
pub async fn health(State(_state): State<WorkspaceState>) -> Json<HealthResponse> {
    info!("Handling health check request");
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
    info!("Handling runtime status request");
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.uptime_seconds().await,
        workspace_initialized: state.is_initialized().await,
    })
}
