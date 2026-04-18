//! DaemonStatusV2 — local-only daemon status response.
//!
//! Response shape for GET /v1/local/daemon/status. Superset of v1 running-probe,
//! wire-compatible. Per daemon-lifecycle-api-v2.md §7.1.

use serde::{Deserialize, Serialize};

/// Response shape for GET /v1/local/daemon/status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DaemonStatusV2 {
    pub schema_version: u32,
    pub lifecycle_state: LifecycleState,
    pub version: String,
    pub implementation_scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded: Option<DegradedInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subsystems: Option<SubsystemHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Daemon lifecycle state. 'stopped' is never emitted (pseudo-state).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Starting,
    Running,
    Degraded,
    Stopping,
    Failed,
}

/// Degraded subsystems information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DegradedInfo {
    pub subsystems: Vec<DegradedSubsystem>,
    pub reasons: Vec<String>,
}

/// A degraded subsystem name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradedSubsystem {
    Http,
    Db,
    Sync,
    Engine,
    WorkerMgr,
    AcpRegistry,
}

/// Health status for each subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SubsystemHealth {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http: Option<SubsystemHealthEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db: Option<SubsystemHealthEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync: Option<SubsystemHealthEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<SubsystemHealthEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_mgr: Option<SubsystemHealthEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acp_registry: Option<SubsystemHealthEntry>,
}

/// Subsystem health entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SubsystemHealthEntry {
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_sessions: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_workers: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_age_ms: Option<u64>,
}

/// Subsystem health status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Up,
    Degraded,
    Down,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_running_state() {
        let v = DaemonStatusV2 {
            schema_version: 2,
            lifecycle_state: LifecycleState::Running,
            version: "0.1.0".to_string(),
            implementation_scope: "full-fsm (v2)".to_string(),
            uptime_ms: Some(60000),
            started_at: Some("2026-01-01T00:00:00Z".to_string()),
            pid: Some(12345),
            degraded: None,
            subsystems: None,
            exit_code: None,
            last_error: None,
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: DaemonStatusV2 = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn roundtrip_degraded_state() {
        let v = DaemonStatusV2 {
            schema_version: 2,
            lifecycle_state: LifecycleState::Degraded,
            version: "0.1.0".to_string(),
            implementation_scope: "full-fsm (v2)".to_string(),
            uptime_ms: None,
            started_at: None,
            pid: None,
            degraded: Some(DegradedInfo {
                subsystems: vec![DegradedSubsystem::Db],
                reasons: vec!["connection lost".to_string()],
            }),
            subsystems: None,
            exit_code: None,
            last_error: None,
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: DaemonStatusV2 = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }
}
