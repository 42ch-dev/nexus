//! API response models for daemon communication

use nexus_contracts::local::domain::RuntimeMode;
use serde::Deserialize;

/// Runtime status response from daemon's `/v1/local/runtime/status` endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeStatus {
    /// Daemon version string
    pub version: String,
    /// Daemon uptime in seconds
    pub uptime_seconds: u64,
    /// Whether the workspace has been initialized
    pub workspace_initialized: bool,
    /// ACP status information (V1.1)
    pub acp: AcpStatusInfo,
    /// Current runtime mode (local_only / local_first / cloud_enhanced)
    #[serde(default)]
    #[allow(dead_code)]
    pub runtime_mode: Option<RuntimeMode>,
}

/// ACP-related status information included in runtime status (V1.1).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AcpStatusInfo {
    /// Whether ACP tool execution is supported by the daemon
    #[serde(default)]
    pub tool_execution_enabled: bool,
    /// Number of active ACP sessions
    #[serde(default)]
    pub active_sessions: usize,
    /// Total tool executions from audit log
    #[serde(default)]
    pub total_tool_executions: u64,
}

/// Response from the daemon's memory review endpoint.
///
/// Returned after the daemon processes the pending review queue,
/// summarizing how many memories were promoted, fragmented, or dropped.
#[derive(Debug, Clone, Deserialize)]
pub struct ReviewResponse {
    /// Number of memories promoted to long-term storage
    pub promoted: usize,
    /// Number of memories broken into fragments
    pub fragmented: usize,
    /// Number of memories dropped (below quality threshold)
    pub dropped: usize,
}

/// A single memory fragment row returned by the daemon's fragments endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct FragmentRow {
    /// Unique fragment identifier
    pub fragment_id: String,
    /// Short human-readable summary of the fragment content
    pub summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_status_deserialize_complete() {
        let json = serde_json::json!({
            "version": "0.1.0",
            "uptime_seconds": 3600,
            "workspace_initialized": true,
            "acp": {
                "tool_execution_enabled": true,
                "active_sessions": 5,
                "total_tool_executions": 42
            },
            "runtime_mode": "local_only"
        });

        let status: RuntimeStatus = serde_json::from_value(json).expect("Failed to deserialize");

        assert_eq!(status.version, "0.1.0");
        assert_eq!(status.uptime_seconds, 3600);
        assert!(status.workspace_initialized);
        assert!(status.acp.tool_execution_enabled);
        assert_eq!(status.acp.active_sessions, 5);
        assert_eq!(status.acp.total_tool_executions, 42);
        assert_eq!(status.runtime_mode, Some(RuntimeMode::LocalOnly));
    }

    #[test]
    fn test_runtime_status_deserialize_minimal() {
        let json = serde_json::json!({
            "version": "0.1.0",
            "uptime_seconds": 0,
            "workspace_initialized": false,
            "acp": {}
        });

        let status: RuntimeStatus = serde_json::from_value(json).expect("Failed to deserialize");

        assert_eq!(status.version, "0.1.0");
        assert_eq!(status.uptime_seconds, 0);
        assert!(!status.workspace_initialized);
        // Default values for ACP status
        assert!(!status.acp.tool_execution_enabled);
        assert_eq!(status.acp.active_sessions, 0);
        assert_eq!(status.acp.total_tool_executions, 0);
        // runtime_mode defaults to None when missing
        assert!(status.runtime_mode.is_none());
    }

    #[test]
    fn test_acp_status_info_defaults() {
        let acp = AcpStatusInfo::default();
        assert!(!acp.tool_execution_enabled);
        assert_eq!(acp.active_sessions, 0);
        assert_eq!(acp.total_tool_executions, 0);
    }

    #[test]
    fn test_review_response_deserialize() {
        let json = serde_json::json!({
            "promoted": 3,
            "fragmented": 1,
            "dropped": 0
        });
        let resp: ReviewResponse = serde_json::from_value(json).expect("Failed to deserialize");
        assert_eq!(resp.promoted, 3);
        assert_eq!(resp.fragmented, 1);
        assert_eq!(resp.dropped, 0);
    }

    #[test]
    fn test_fragment_row_deserialize() {
        let json = serde_json::json!({
            "fragment_id": "frag-001",
            "summary": "Key insight about world-building"
        });
        let row: FragmentRow = serde_json::from_value(json).expect("Failed to deserialize");
        assert_eq!(row.fragment_id, "frag-001");
        assert_eq!(row.summary, "Key insight about world-building");
    }

    #[test]
    fn test_fragment_row_list_deserialize() {
        let json = serde_json::json!([
            { "fragment_id": "frag-001", "summary": "First" },
            { "fragment_id": "frag-002", "summary": "Second" }
        ]);
        let rows: Vec<FragmentRow> = serde_json::from_value(json).expect("Failed to deserialize");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].fragment_id, "frag-001");
        assert_eq!(rows[1].fragment_id, "frag-002");
    }
}
