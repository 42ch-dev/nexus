//! API response models for daemon communication

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
            }
        });

        let status: RuntimeStatus = serde_json::from_value(json).expect("Failed to deserialize");

        assert_eq!(status.version, "0.1.0");
        assert_eq!(status.uptime_seconds, 3600);
        assert!(status.workspace_initialized);
        assert!(status.acp.tool_execution_enabled);
        assert_eq!(status.acp.active_sessions, 5);
        assert_eq!(status.acp.total_tool_executions, 42);
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
    }

    #[test]
    fn test_acp_status_info_defaults() {
        let acp = AcpStatusInfo::default();
        assert!(!acp.tool_execution_enabled);
        assert_eq!(acp.active_sessions, 0);
        assert_eq!(acp.total_tool_executions, 0);
    }
}
