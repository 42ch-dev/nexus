//! API response models for daemon communication

use nexus_contracts::local::domain::RuntimeMode;
use serde::{Deserialize, Serialize};

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
    /// Current runtime mode (`local_only` / `local_first` / `cloud_enhanced`)
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

/// Response from `GET /v1/local/memory/fragments`.
#[derive(Debug, Clone, Deserialize)]
pub struct ListFragmentsResponse {
    pub fragments: Vec<FragmentRow>,
}

// ─── Pending review models ──────────────────────────────────────────────────

/// Response from `GET /v1/local/memory/pending-review`.
#[derive(Debug, Clone, Deserialize)]
pub struct ListPendingReviewsResponse {
    pub items: Vec<PendingReviewRow>,
    pub pagination: PaginationInfo,
}

/// A single pending review row from the daemon API.
#[derive(Debug, Clone, Deserialize)]
pub struct PendingReviewRow {
    pub pending_id: String,
    pub session_id: String,
    pub creator_id: String,
    pub world_id: Option<String>,
    pub task_kind: String,
    pub raw_digest: String,
    pub created_at: String,
}

/// Response from `DELETE /v1/local/memory/pending-review/{id}`.
#[derive(Debug, Clone, Deserialize)]
pub struct DeletePendingReviewResponse {
    pub success: bool,
    pub pending_id: String,
}

// ─── Workspace management models (V1.20 Batch 4) ─────────────────────────

/// Response from `GET /v1/local/workspaces`.
#[derive(Debug, Clone, Deserialize)]
pub struct ListWorkspacesResponse {
    pub items: Vec<WorkspaceSummary>,
}

/// A single workspace summary in the list response.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceSummary {
    pub creator_id: String,
    pub workspace_slug: String,
    pub creative_root: String,
    pub display_name: Option<String>,
}

/// Request body for `POST /v1/local/workspaces`.
#[derive(Debug, Clone, Serialize)]
pub struct CreateWorkspaceRequest {
    pub creator_id: String,
    pub workspace_slug: String,
    pub creative_root: Option<std::path::PathBuf>,
    pub display_name: Option<String>,
}

/// Response from `POST /v1/local/workspaces`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkspaceResponse {
    pub creator_id: String,
    pub workspace_slug: String,
    pub creative_root: String,
    pub operational_dir: String,
    pub state_db_path: String,
}

/// Response from `GET /v1/local/workspaces/active`.
#[derive(Debug, Clone, Deserialize)]
pub struct ActiveWorkspaceResponse {
    pub creator_id: String,
    pub workspace_slug: String,
    pub creative_root: Option<String>,
    pub operational_dir: String,
}

/// Request body for `PUT /v1/local/workspaces/active`.
#[derive(Debug, Clone, Serialize)]
pub struct SetActiveWorkspaceRequest {
    pub creator_id: Option<String>,
    pub workspace_slug: String,
}

/// Response from `PUT /v1/local/workspaces/active`.
#[derive(Debug, Clone, Deserialize)]
pub struct SetActiveWorkspaceResponse {
    pub creator_id: String,
    pub workspace_slug: String,
}

// ─── Creator management models (V1.20 Batch 5) ────────────────────────────

/// Response from `GET /v1/local/creators/active`.
#[derive(Debug, Clone, Deserialize)]
pub struct ActiveCreatorResponse {
    pub creator_id: String,
    pub handle: Option<String>,
    pub display_name: Option<String>,
}

/// Request body for `PUT /v1/local/creators/active`.
#[derive(Debug, Clone, Serialize)]
pub struct SetActiveCreatorRequest {
    pub creator_id: String,
}

/// Response from `PUT /v1/local/creators/active`.
#[derive(Debug, Clone, Deserialize)]
pub struct SetActiveCreatorResponse {
    pub creator_id: String,
}

/// Response from `POST /v1/local/creators/{id}:logout`.
#[derive(Debug, Clone, Deserialize)]
pub struct LogoutCreatorResponse {
    pub creator_id: String,
    pub cleared: bool,
}

// ─── Preset management models (V1.20 Batch 5) ─────────────────────────────

/// A single preset summary.
#[derive(Debug, Clone, Deserialize)]
pub struct PresetSummary {
    pub id: String,
    pub source: String,
}

/// Response from `GET /v1/local/presets`.
#[derive(Debug, Clone, Deserialize)]
pub struct ListPresetsGroupedResponse {
    pub embedded: Vec<PresetSummary>,
    pub system: Vec<PresetSummary>,
    pub user: Vec<PresetSummary>,
}

/// Request body for `POST /v1/local/presets`.
#[derive(Debug, Clone, Serialize)]
pub struct ScaffoldPresetRequest {
    pub name: String,
}

/// Response from `POST /v1/local/presets`.
#[derive(Debug, Clone, Deserialize)]
pub struct ScaffoldPresetResponse {
    pub id: String,
    pub path: String,
}

/// Request body for `POST /v1/local/presets:validate`.
#[derive(Debug, Clone, Serialize)]
pub struct ValidatePresetRequest {
    pub path: String,
}

/// Response from `POST /v1/local/presets:validate`.
#[derive(Debug, Clone, Deserialize)]
pub struct ValidatePresetResponse {
    pub valid: bool,
    pub id: Option<String>,
    pub version: Option<u32>,
    pub state_count: Option<usize>,
    pub errors: Vec<String>,
}

/// Response from `POST /v1/local/presets/{id}:reload`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReloadPresetResponse {
    pub id: String,
    pub reloaded: bool,
}

// ─── KB models (V1.20 Batch 5) ────────────────────────────────────────────

/// A single KB entry summary.
#[derive(Debug, Clone, Deserialize)]
pub struct KbEntrySummary {
    pub entry_id: String,
    pub title: String,
    pub created_at: String,
}

/// Response from `GET /v1/local/kb/entries`.
#[derive(Debug, Clone, Deserialize)]
pub struct ListKbEntriesResponse {
    pub items: Vec<KbEntrySummary>,
    pub pagination: PaginationInfo,
}

/// Pagination info in list responses.
#[derive(Debug, Clone, Deserialize)]
pub struct PaginationInfo {
    pub limit: usize,
    pub next_cursor: Option<String>,
}

/// Request body for `POST /v1/local/kb/entries`.
#[derive(Debug, Clone, Serialize)]
pub struct AddKbEntryRequest {
    pub creator_id: String,
    pub workspace_slug: Option<String>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub file_path: Option<String>,
}

/// Response from `POST /v1/local/kb/entries`.
#[derive(Debug, Clone, Deserialize)]
pub struct AddKbEntryResponse {
    pub entry_id: String,
    pub title: String,
}

/// Response from `GET /v1/local/kb/entries/{id}`.
#[derive(Debug, Clone, Deserialize)]
pub struct GetKbEntryResponse {
    pub entry_id: String,
    pub title: String,
    pub created_at: String,
    pub content: String,
}

/// Response from `DELETE /v1/local/kb/entries/{id}`.
#[derive(Debug, Clone, Deserialize)]
pub struct DeleteKbEntryResponse {
    pub entry_id: String,
    pub deleted: bool,
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
