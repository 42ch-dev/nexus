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
    /// Current runtime mode (local_only / local_first / cloud_enhanced).
    pub runtime_mode: String,
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

#[derive(Serialize)]
pub struct DaemonStatusResponse {
    /// Normalized lifecycle name aligned with cli-spec-v1 §10.1 (`snake_case`).
    /// While the Local API is accepting connections, this is `running`.
    pub lifecycle_state: &'static str,
    pub version: String,
    /// Human-readable scope note for consumers (full six-state FSM is not yet implemented).
    pub implementation_scope: &'static str,
}

/// GET /v1/local/daemon/status — minimal lifecycle probe (TD-9 partial delivery).
///
/// Exposes a stable JSON shape for automation. **Not** a full §10.1 state machine yet; current
/// authoritative design for the 6-state HSM lives in
/// `.agents/plans/knowledge/daemon-lifecycle-api-v2.md` (the v1 gap analysis is archived at
/// `.agents/plans/archived/knowledge/daemon-lifecycle-api-v1.md`).
pub async fn daemon_status(State(_state): State<WorkspaceState>) -> Json<DaemonStatusResponse> {
    info!("Handling daemon lifecycle status request");
    Json(DaemonStatusResponse {
        lifecycle_state: "running",
        version: env!("CARGO_PKG_VERSION").to_string(),
        implementation_scope:
            "listening — full Stopped/Starting/Degraded/Stopping/Failed FSM deferred",
    })
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
        runtime_mode: state.runtime_mode_as_str().to_string(),
    })
}

/// Gather ACP status information from the database.
async fn gather_acp_status(state: &WorkspaceState) -> AcpStatusInfo {
    let mut status = AcpStatusInfo {
        tool_execution_enabled: true,
        ..Default::default()
    };

    let pool = state.pool();

    // Count active sessions
    if let Ok(count) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM acp_sessions")
        .fetch_one(pool)
        .await
    {
        status.active_sessions = count.0 as usize;
    }

    // Count total tool executions
    if let Ok(count) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM acp_tool_audit_log")
        .fetch_one(pool)
        .await
    {
        status.total_tool_executions = count.0 as u64;
    }

    status
}
