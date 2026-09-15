//! HTTP request/response types for the `/v1/daemon/orchestration/*` endpoints.
//!
//! Hand-coded local types — NOT codegen'd, NOT in `schemas/`.
//! Design: `.mstar/archived/knowledge/schemas-boundary.md` §3.
//!
//! The daemon exposes these as local-only HTTP; `nexus-platform` never
//! observes them over any wire channel.

use crate::generated::daemon_api::kb::pagination_info::PaginationInfo;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/// Query parameters for `GET /v1/daemon/orchestration/sessions`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ListSessionsQuery {
    /// Filter by creator ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    /// Maximum number of items to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Opaque pagination cursor returned by the previous response's
    /// `pagination.next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Comma-separated sort terms (e.g. `-status`, `preset_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

/// Response body for `GET /v1/daemon/orchestration/sessions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSessionsResponse {
    /// Active engine sessions.
    pub items: Vec<SessionSummary>,
    /// Cursor-based pagination envelope.
    pub pagination: PaginationInfo,
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
    /// Actionable failure reason for terminal/uncertain outcomes (e.g. an
    /// unconfirmed cancel cleanup that left the run `interrupted`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    /// Shared durable execution projection (A2/A7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionProjection>,
}

/// Response body for `GET /v1/daemon/orchestration/sessions/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionResponse {
    pub session: SessionSummary,
}

/// Shared durable execution projection (`A2/A7`) — the single operator-facing
/// classification used by session and schedule inspection.
///
/// Field names are `snake_case` (the promoted schema SSOT) even inside the
/// camelCase session DTO; `recovery_class`/`allowed_actions` are the
/// actionable contract, prose is never parsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionProjection {
    /// Durable execution version; `None` when no run exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_version: Option<u32>,
    /// Durable state revision; `None` when no run exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_revision: Option<u64>,
    /// Shared A7 recovery classification.
    pub recovery_class: String,
    /// Current durable A4 human wait, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<ExecutionWait>,
    /// Stable machine reason code for uncertain/terminal outcomes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    /// Legal operator actions in the current durable state.
    pub allowed_actions: Vec<String>,
}

/// The current durable A4 human wait token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionWait {
    pub wait_id: String,
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_task_id: Option<String>,
    pub kind: String,
}

/// Role → provider binding (`A1`) for the camelCase session-create wire
/// contract (`agentBindings`).
///
/// The schedule path uses the `snake_case`
/// [`crate::local::schedule::http::AgentBindingDto`]; session creation
/// preserves its existing camelCase convention (`presetId`, `creatorId`,
/// `agentBindings` → `providerId`, `model`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAgentBindingDto {
    /// Provider id.
    pub provider_id: String,
    /// Optional model id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Request body for `POST /v1/daemon/orchestration/sessions` (schedule start).
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
    /// Role → provider binding map (A1, v1.186 P2 T1). Frozen at admission
    /// into the run descriptor; graph node `agent` selects the role key.
    /// Unknown role/provider references are refused before enqueue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_bindings: Option<std::collections::HashMap<String, SessionAgentBindingDto>>,
}

/// Response body for `POST /v1/daemon/orchestration/sessions` (schedule start).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionResponse {
    /// The created session ID.
    pub session_id: String,
}

/// Request body for `POST /v1/daemon/orchestration/presets/{id}:reload`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReloadPresetRequest {}

/// Response body for `POST /v1/daemon/orchestration/presets/{id}:reload`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReloadPresetResponse {
    /// Preset ID that was reloaded.
    pub preset_id: String,
    /// New source hash after reload.
    pub source_hash: String,
}

/// Request body for `POST /v1/daemon/orchestration/sessions/{id}/signal`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalSessionRequest {
    /// One of `pause`, `resume`, `cancel`, `advance`, `continue`.
    pub signal: String,
    /// Exact durable wait token for `continue` (A4). Required for
    /// `signal: "continue"`; ignored/absent for other signals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Capabilities
// ---------------------------------------------------------------------------

/// Query parameters for `GET /v1/daemon/orchestration/capabilities`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ListCapabilitiesQuery {
    /// Maximum number of items to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Opaque pagination cursor returned by the previous response's
    /// `pagination.next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Comma-separated sort terms (e.g. `-name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

/// Response body for `GET /v1/daemon/orchestration/capabilities`.
#[derive(Debug, Clone, Serialize)]
pub struct ListCapabilitiesResponse {
    /// Registered capabilities with their schemas.
    pub items: Vec<CapabilityInfo>,
    /// Cursor-based pagination envelope.
    pub pagination: PaginationInfo,
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
    /// Provenance (AR-40): `"builtin"` (ships with the engine) or `"user"`
    /// (locally-installed developer capability). Mirrors the wire enum; the
    /// orchestration `CapabilityOrigin` enum NEVER crosses the crate boundary
    /// (dependency direction, AR-40) — the handler maps it to this string.
    pub origin: String,
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

// The canonical preset wire DTOs (`OrchestrationPresetListResponse` and the
// `PresetProfile*` family) are generated from
// `schemas/core/orchestration-presets-api.schema.json` and live in
// `crate::generated::core::orchestration_presets_api`. The handwritten copies
// were retired by the P5 schema lane, not copied.
