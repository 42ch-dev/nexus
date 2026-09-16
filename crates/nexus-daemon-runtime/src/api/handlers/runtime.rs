//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]
//! Runtime handlers — health check and status

use crate::workspace::WorkspaceState;
use axum::{extract::State, Json};
use nexus_contracts::local::acp_runtime::daemon_status_v2::{
    DaemonStatusV2, DegradedInfo, HealthStatus, LifecycleState, SubsystemHealth,
    SubsystemHealthEntry,
};
use nexus_contracts::{
    CertFingerprintResponse, CoreServiceStopRequest, RuntimeApi, RuntimeApiStatus,
};
use serde::Serialize;
use std::sync::OnceLock;
use tracing::info;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// GET /v1/daemon/runtime/health
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
    /// Current runtime mode (`local_only` / `local_first` / `cloud_enhanced`).
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

/// GET /v1/daemon/daemon/status — v2 full FSM response.
///
/// Returns the full lifecycle state per daemon-lifecycle-api.md §7.
/// Wire-compatible with v1: v1 clients only see `lifecycle_state` field.
/// v2 clients can check `schema_version: 2` for the full shape.
pub async fn daemon_status(State(state): State<WorkspaceState>) -> Json<DaemonStatusV2> {
    info!("Handling daemon lifecycle status request (v2)");

    // Get current lifecycle state
    let lifecycle_state = state.lifecycle_state();
    // Build the v2 response
    let uptime_seconds = state.uptime_seconds();
    let uptime_ms = uptime_seconds * 1000; // Convert to ms per spec §7.1
    let pid = i64::from(std::process::id());

    let lifecycle_state_str = lifecycle_state.to_string();
    let lifecycle_state_enum = match lifecycle_state_str.as_str() {
        "starting" => LifecycleState::Starting,
        "running" => LifecycleState::Running,
        "degraded" => LifecycleState::Degraded,
        "stopping" => LifecycleState::Stopping,
        _ => LifecycleState::Failed, // "failed" or unknown fallback
    };

    // Build subsystem health from actual workspace state.
    // When no creator DB is open (pre-attach boot), DB is reported as Down.
    let db_status = match state.pool() {
        Some(pool) => match sqlx::query_scalar!("SELECT 1 as \"count!\"")
            .fetch_one(pool)
            .await
        {
            Ok(_) => HealthStatus::Up,
            Err(_) => HealthStatus::Down,
        },
        None => HealthStatus::Down,
    };

    let make_entry = |status: HealthStatus| SubsystemHealthEntry {
        status,
        last_check_ms: Some(0),
        active_sessions: None,
        active_workers: None,
        cache_age_ms: None,
    };

    let subsystems = SubsystemHealth {
        http: Some(make_entry(HealthStatus::Up)),
        db: Some(make_entry(db_status)),
        sync: Some(make_entry(HealthStatus::Down)),
        engine: Some(SubsystemHealthEntry {
            status: if state.engine().is_some() {
                HealthStatus::Up
            } else {
                HealthStatus::Down
            },
            last_check_ms: Some(0),
            active_sessions: None,
            active_workers: None,
            cache_age_ms: None,
        }),
        acp_registry: Some(SubsystemHealthEntry {
            status: if state.capability_registry().is_some() {
                HealthStatus::Up
            } else {
                HealthStatus::Down
            },
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
        started_at: Some(state.started_at().to_rfc3339()),
        pid: Some(pid),
        degraded: Some(DegradedInfo {
            subsystems: vec![],
            reasons: vec![],
        }),
        subsystems: Some(subsystems),
        exit_code: exit_code.map(i64::from),
        last_error: None, // Could be set from lifecycle in Failed state
    })
}

/// GET /v1/daemon/runtime/status
pub async fn status(State(state): State<WorkspaceState>) -> Json<StatusResponse> {
    info!("Handling runtime status request");

    // Gather ACP-related status info
    let acp_status = gather_acp_status(&state).await;

    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.uptime_seconds(),
        workspace_initialized: state.is_initialized(),
        acp: acp_status,
        runtime_mode: state.runtime_mode_as_str().to_string(),
    })
}

/// GET /v1/daemon/runtime/cert-fingerprint
///
/// Returns the SHA-256 fingerprint of the daemon's TLS certificate for TOFU
/// pinning. No authentication required. Loopback-only daemons return an empty
/// fingerprint and no `created_at`.
pub async fn cert_fingerprint(
    State(state): State<WorkspaceState>,
) -> Json<CertFingerprintResponse> {
    info!("Handling cert-fingerprint request");
    state.tls_fingerprint().map_or_else(
        || {
            Json(CertFingerprintResponse {
                fingerprint: String::new(),
                algorithm: "sha256".parse().expect("valid algorithm constant"),
                created_at: None,
            })
        },
        Json,
    )
}

/// Gather ACP status information from the database.
///
/// When no creator DB is open (pre-attach boot), returns the default
/// `AcpStatusInfo` with zeroed counts.
async fn gather_acp_status(state: &WorkspaceState) -> AcpStatusInfo {
    let mut status = AcpStatusInfo {
        tool_execution_enabled: true,
        ..Default::default()
    };

    let Some(pool) = state.pool() else {
        return status;
    };

    // Count active sessions
    if let Ok(row) = sqlx::query_scalar!("SELECT COUNT(*) as \"count!\" FROM acp_sessions")
        .fetch_one(pool)
        .await
    {
        // SAFETY: SQLite COUNT(*) result fits in usize; unwrap_or handles theoretical overflow
        status.active_sessions = usize::try_from(row).unwrap_or(0);
    }

    // Count total tool executions
    if let Ok(row) = sqlx::query_scalar!("SELECT COUNT(*) as \"count!\" FROM acp_tool_audit_log")
        .fetch_one(pool)
        .await
    {
        // SAFETY: SQLite COUNT(*) is non-negative; cast_unsigned preserves value
        status.total_tool_executions = row.cast_unsigned();
    }

    status
}

/// This daemon process's launch identity (architecture §7, frozen protocol).
///
/// The instance id is minted once per boot; the pid is diagnostic only and
/// never a stop authorization. `POST /v1/daemon/runtime/stop` refuses any
/// request whose `expected_instance_id`/`expected_engine_epoch` pair does not
/// match this identity — a stale discovery record can never stop its
/// replacement, and there is no PID-only stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRuntimeIdentity {
    pub instance_id: String,
    pub engine_epoch: u64,
}

static RUNTIME_IDENTITY: OnceLock<ServiceRuntimeIdentity> = OnceLock::new();

/// Install the boot identity. Idempotent: the first caller wins and later
/// boots in the same process cannot silently rotate identity underneath live
/// clients. Returns `false` when an identity was already installed.
pub fn init_runtime_identity(identity: ServiceRuntimeIdentity) -> bool {
    RUNTIME_IDENTITY.set(identity).is_ok()
}

fn runtime_identity() -> Option<&'static ServiceRuntimeIdentity> {
    RUNTIME_IDENTITY.get()
}

/// POST /v1/daemon/runtime/stop
///
/// Instance-bound stop: `CoreServiceStopRequest { expected_instance_id,
/// expected_engine_epoch }`. A mismatch returns 409 `instance_conflict` and
/// performs no stop; a match requests the graceful shutdown and reports
/// `stopping` while the drain runs (poll runtime health for liveness).
pub async fn stop(
    State(state): State<WorkspaceState>,
    Json(request): Json<CoreServiceStopRequest>,
) -> Result<Json<RuntimeApi>, crate::api::errors::NexusApiError> {
    info!("Handling runtime stop request");
    let Some(identity) = runtime_identity() else {
        // No installable identity means the handler cannot prove ownership;
        // refusing is the only safe direction.
        return Err(crate::api::errors::NexusApiError::ConflictCoded {
            code: "instance_conflict".into(),
            message: "runtime identity is unavailable; refusing to stop".into(),
        });
    };
    let matches = request.expected_instance_id.as_str() == identity.instance_id
        && request.expected_engine_epoch == Some(identity.engine_epoch);
    if !matches {
        return Err(crate::api::errors::NexusApiError::ConflictCoded {
            code: "instance_conflict".into(),
            message: "stop request does not match this service instance".into(),
        });
    }
    state.request_shutdown();
    Ok(Json(RuntimeApi {
        status: RuntimeApiStatus::Stopping,
    }))
}
