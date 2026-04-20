//! HTTP request/response types for the `/v1/local/orchestration/*` endpoints.
//!
//! Hand-coded local types — NOT codegen'd, NOT in `schemas/`.
//! Design: `.agents/plans/knowledge/schemas-boundary-v1.md` §3.
//!
//! The daemon exposes these as local-only HTTP; `nexus-platform` never
//! observes them over any wire channel.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/// Query parameters for `GET /v1/local/orchestration/sessions`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsQuery {
    /// Filter by creator ID.
    pub creator_id: Option<String>,
}

/// Response body for `GET /v1/local/orchestration/sessions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSessionsResponse {
    /// Active engine sessions.
    pub sessions: Vec<SessionSummary>,
}

/// A single session summary item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// Opaque session identifier.
    pub session_id: String,
    /// Creator that owns the session.
    pub creator_id: String,
    /// Preset the session is running.
    pub preset_id: String,
    /// Current status.
    pub status: String,
    /// Task the session is currently executing (if any).
    pub current_task_id: Option<String>,
}

/// Response body for `GET /v1/local/orchestration/sessions/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionResponse {
    pub session: SessionSummary,
}

/// Request body for `POST /v1/local/orchestration/sessions` (schedule start).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    /// Preset ID to run (e.g. `"novel-writing"`).
    pub preset_id: String,
    /// Creator ID that owns this session.
    pub creator_id: String,
    /// Optional seed text for `preset.input.*`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
}

/// Response body for `POST /v1/local/orchestration/sessions` (schedule start).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionResponse {
    /// The created session ID.
    pub session_id: String,
}

/// Request body for `POST /v1/local/orchestration/presets/{id}:reload`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReloadPresetRequest {}

/// Response body for `POST /v1/local/orchestration/presets/{id}:reload`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReloadPresetResponse {
    /// Preset ID that was reloaded.
    pub preset_id: String,
    /// New source hash after reload.
    pub source_hash: String,
}

/// Request body for `POST /v1/local/orchestration/sessions/{id}/signal`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalSessionRequest {
    /// One of `pause`, `resume`, `cancel`, `advance`.
    pub signal: String,
}

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

/// Response body for `GET /v1/local/orchestration/capabilities`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCapabilitiesResponse {
    /// Registered capabilities with their schemas.
    pub capabilities: Vec<CapabilityInfo>,
}

/// A single capability description.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityInfo {
    /// Dot-separated capability name, e.g. `"sync.pull"`.
    pub name: String,
    /// JSON Schema (draft 2020-12) for valid inputs.
    pub input_schema: String,
    /// JSON Schema (draft 2020-12) for the output shape.
    pub output_schema: String,
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

/// Response body for `GET /v1/local/orchestration/presets`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPresetsResponse {
    /// Loadable preset IDs.
    pub presets: Vec<String>,
}
