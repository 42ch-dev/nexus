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
    /// ACP status information (V1.1)
    pub acp: AcpStatusInfo,
}

/// ACP-related status information included in runtime status.
#[derive(Debug, Serialize, Default)]
pub struct AcpStatusInfo {
    /// Whether ACP tool execution is supported by the daemon
    pub tool_execution_enabled: bool,
    /// Number of active ACP sessions
    pub active_sessions: usize,
    /// Total tool executions (from audit log)
    pub total_tool_executions: u64,
}

/// GET /v1/local/runtime/status
pub async fn status(State(state): State<WorkspaceState>) -> Json<StatusResponse> {
    info!("Handling runtime status request");

    // Gather ACP-related status info
    let acp_status = gather_acp_status(&state).await;

    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.uptime_seconds().await,
        workspace_initialized: state.is_initialized().await,
        acp: acp_status,
    })
}

/// Gather ACP status information from the database.
async fn gather_acp_status(state: &WorkspaceState) -> AcpStatusInfo {
    let mut status = AcpStatusInfo {
        tool_execution_enabled: true,
        ..Default::default()
    };

    // Count active sessions
    if let Ok(conn) = state.db().await {
        if let Ok(count) = conn
            .interact(|conn| {
                let count: Result<usize, _> =
                    conn.query_row("SELECT COUNT(*) FROM acp_sessions", [], |row| row.get(0));
                count
            })
            .await
        {
            status.active_sessions = count.unwrap_or(0);
        }

        // Count total tool executions
        if let Ok(count) = conn
            .interact(|conn| {
                let count: Result<usize, _> =
                    conn.query_row("SELECT COUNT(*) FROM acp_tool_audit_log", [], |row| {
                        row.get(0)
                    });
                count
            })
            .await
        {
            status.total_tool_executions = count.unwrap_or(0) as u64;
        }
    }

    status
}
