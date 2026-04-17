//! Runtime handlers — health check and status

use crate::workspace::WorkspaceState;
use axum::{extract::State, Json};
use nexus_contracts::local::acp_runtime::daemon_status_v2::{
    DaemonStatusV2, DegradedInfo, HealthStatus, LifecycleState, SubsystemHealth,
    SubsystemHealthEntry,
};
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
    pub uptime_seconds: u64, // Internal endpoint uses seconds (not from schema)
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

/// GET /v1/local/daemon/status — v2 full FSM response.
///
/// Returns the full lifecycle state per daemon-lifecycle-api-v2.md §7.
/// Wire-compatible with v1: v1 clients only see `lifecycle_state` field.
/// v2 clients can check `schema_version: 2` for the full shape.
pub async fn daemon_status(State(state): State<WorkspaceState>) -> Json<DaemonStatusV2> {
    info!("Handling daemon lifecycle status request (v2)");

    // Get current lifecycle state
    let lifecycle_state = state.lifecycle_state();
    // Build the v2 response
    let uptime_seconds = state.uptime_seconds().await;
    let uptime_ms = uptime_seconds * 1000; // Convert to ms per spec §7.1
    let pid = std::process::id() as i64;

    let lifecycle_state_str = lifecycle_state.to_string();
    let lifecycle_state_enum = match lifecycle_state_str.as_str() {
        "starting" => LifecycleState::Starting,
        "running" => LifecycleState::Running,
        "degraded" => LifecycleState::Degraded,
        "stopping" => LifecycleState::Stopping,
        "failed" => LifecycleState::Failed,
        _ => LifecycleState::Failed, // fallback
    };

    // Build subsystem health (stub for now - real subsystems will populate in T6)
    let make_entry = |status: HealthStatus| SubsystemHealthEntry {
        status,
        last_check_ms: Some(0),
        active_sessions: None,
        active_workers: None,
        cache_age_ms: None,
    };
    let subsystems = SubsystemHealth {
        http: Some(make_entry(HealthStatus::Up)),
        db: Some(make_entry(HealthStatus::Up)),
        sync: Some(make_entry(HealthStatus::Up)),
        engine: Some(SubsystemHealthEntry {
            status: HealthStatus::Up,
            last_check_ms: Some(0),
            active_sessions: Some(0),
            active_workers: None,
            cache_age_ms: None,
        }),
        worker_mgr: Some(SubsystemHealthEntry {
            status: HealthStatus::Up,
            last_check_ms: Some(0),
            active_sessions: None,
            active_workers: Some(0),
            cache_age_ms: None,
        }),
        acp_registry: Some(SubsystemHealthEntry {
            status: HealthStatus::Up,
            last_check_ms: Some(0),
            active_sessions: None,
            active_workers: None,
            cache_age_ms: Some(0),
        }),
    };

    // Exit code and last error (only set in Failed state)
    let exit_code = if lifecycle_state == crate::lifecycle::LifecycleState::Failed {
        state.lifecycle_exit_code()
    } else {
        None
    };

    Json(DaemonStatusV2 {
        schema_version: 2,
        lifecycle_state: lifecycle_state_enum,
        version: env!("CARGO_PKG_VERSION").to_string(),
        implementation_scope: "full-fsm (v2)".to_string(),
        uptime_ms: Some(uptime_ms),
        started_at: None, // Could be set from lifecycle Running.entry timestamp
        pid: Some(pid),
        degraded: Some(DegradedInfo {
            subsystems: vec![],
            reasons: vec![],
        }),
        subsystems: Some(subsystems),
        exit_code: exit_code.map(|c| c as i64),
        last_error: None, // Could be set from lifecycle in Failed state
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
