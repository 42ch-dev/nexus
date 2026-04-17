//! Daemon Status v2
//!
//! Response shape for GET /v1/local/daemon/status. Superset of v1 running-probe, wire-compatible. Per daemon-lifecycle-api-v2.md §7.1.
//!
//! @schema_version 2
//! @source daemon-status-v2.schema.json

use serde::{Deserialize, Serialize};

/// Response shape for GET /v1/local/daemon/status. Superset of v1 running-probe, wire-compatible. Per daemon-lifecycle-api-v2.md §7.1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DaemonStatusV2 {
    pub schema_version: u32,
    pub lifecycle_state: String,
    pub version: String,
    pub implementation_scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subsystems: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}
/// SubsystemHealthEntry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SubsystemHealthEntry {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_sessions: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_workers: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_age_ms: Option<u64>,
}
