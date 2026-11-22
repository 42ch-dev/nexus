//! HTTP request/response types for the `/v1/daemon/orchestration/*` endpoints.
//!
//! Hand-coded local types — NOT codegen'd, NOT in `schemas/`.
//! Design: `docs/specs/architecture/schemas-external-consumer-boundary.md`.
//!
//! The daemon exposes these as local-only HTTP; `nexus-platform` never
//! observes them over any wire channel.

use crate::generated::daemon_api::kb::pagination_info::PaginationInfo;
use serde::{Deserialize, Serialize};

// note: sessions.

/// note: query parameters for `GET /v1/daemon/orchestration/sessions`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ListSessionsQuery {
    /// note: filter by creator ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    /// note: maximum number of items to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// note: opaque pagination cursor returned by the previous response's
    /// `pagination.next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// note: comma-separated sort terms (e.g. `-status`, `preset_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

/// note: response body for `GET /v1/daemon/orchestration/sessions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSessionsResponse {
    /// note: active engine sessions.
    pub items: Vec<SessionSummary>,
    /// note: cursor-based pagination envelope.
    pub pagination: PaginationInfo,
}

/// note: a single session summary item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// note: opaque session identifier.
    pub session_id: String,
    /// note: creator that owns the session.
    pub creator_id: String,
    /// note: preset the session is running.
    pub preset_id: String,
    /// note: current status.
    pub status: String,
    /// note: task the session is currently executing (if any).
    pub current_task_id: Option<String>,
    /// note: actionable failure reason for terminal/uncertain outcomes (e.g. an
    /// unconfirmed cancel cleanup that left the run `interrupted`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    /// note: shared durable execution projection (A2/A7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionProjection>,
}

/// note: response body for `GET /v1/daemon/orchestration/sessions/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionResponse {
    pub session: SessionSummary,
}

/// note: shared durable execution projection (`A2/A7`) — the single operator-facing
/// classification used by session and schedule inspection.
///
/// Field names are `snake_case` (the promoted schema SSOT) even inside the
/// camelCase session DTO; `recovery_class`/`allowed_actions` are the
/// actionable contract, prose is never parsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionProjection {
    /// note: durable execution version; `None` when no run exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_version: Option<u32>,
    /// note: durable state revision; `None` when no run exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_revision: Option<u64>,
    /// note: shared A7 recovery classification.
    pub recovery_class: String,
    /// note: current durable A4 human wait, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<ExecutionWait>,
    /// note: stable machine reason code for uncertain/terminal outcomes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    /// note: legal operator actions in the current durable state.
    pub allowed_actions: Vec<String>,
}

/// note: the current durable A4 human wait token.
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

/// note: role → provider binding (`A1`) for the camelCase session-create wire
/// contract (`agentBindings`).
///
/// The schedule path uses the `snake_case`
/// [`crate::local::schedule::http::AgentBindingDto`]; session creation
/// preserves its existing camelCase convention (`presetId`, `creatorId`,
/// `agentBindings` → `providerId`, `model`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAgentBindingDto {
    /// note: provider id.
    pub provider_id: String,
    /// note: optional model id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// note: request body for `POST /v1/daemon/orchestration/sessions` (schedule start).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    /// note: preset ID to run (e.g. `"novel-writing"`).
    pub preset_id: String,
    /// note: creator ID that owns this session.
    pub creator_id: String,
    /// note: optional seed text for `preset.input.*`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    /// note: role → provider binding map (A1, v1.186 P2 T1). Frozen at admission
    /// into the run descriptor; graph node `agent` selects the role key.
    /// Unknown role/provider references are refused before enqueue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_bindings: Option<std::collections::HashMap<String, SessionAgentBindingDto>>,
}

/// note: response body for `POST /v1/daemon/orchestration/sessions` (schedule start).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionResponse {
    /// note: the created session ID.
    pub session_id: String,
}

/// note: request body for `POST /v1/daemon/orchestration/presets/{id}:reload`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReloadPresetRequest {}

/// note: response body for `POST /v1/daemon/orchestration/presets/{id}:reload`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReloadPresetResponse {
    /// note: preset ID that was reloaded.
    pub preset_id: String,
    /// note: new source hash after reload.
    pub source_hash: String,
}

/// note: request body for `POST /v1/daemon/orchestration/sessions/{id}/signal`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalSessionRequest {
    /// note: one of `pause`, `resume`, `cancel`, `advance`, `continue`.
    pub signal: String,
    /// note: exact durable wait token for `continue` (A4). Required for
    /// `signal: "continue"`; ignored/absent for other signals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_id: Option<String>,
}

// note: capabilities.

/// note: query parameters for `GET /v1/daemon/orchestration/capabilities`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ListCapabilitiesQuery {
    /// note: maximum number of items to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// note: opaque pagination cursor returned by the previous response's
    /// `pagination.next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// note: comma-separated sort terms (e.g. `-name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

/// note: response body for `GET /v1/daemon/orchestration/capabilities`.
#[derive(Debug, Clone, Serialize)]
pub struct ListCapabilitiesResponse {
    /// note: registered capabilities with their schemas.
    pub items: Vec<CapabilityInfo>,
    /// note: cursor-based pagination envelope.
    pub pagination: PaginationInfo,
}

/// note: a single capability description.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityInfo {
    /// note: dot-separated capability name, e.g. `"sync.pull"`.
    pub name: String,
    /// note: JSON Schema (draft 2020-12) for valid inputs.
    pub input_schema: String,
    /// note: JSON Schema (draft 2020-12) for the output shape.
    pub output_schema: String,
    /// note: provenance (AR-40): `"builtin"` (ships with the engine) or `"user"`
    /// (locally-installed developer capability). Mirrors the wire enum; the
    /// orchestration `CapabilityOrigin` enum NEVER crosses the crate boundary
    /// (dependency direction, AR-40) — the handler maps it to this string.
    pub origin: String,
}

// why: the canonical preset wire DTOs (`OrchestrationPresetListResponse` and the
// `PresetProfile*` family) are generated from
// `schemas/core/orchestration-presets-api.schema.json` and live in
// `crate::generated::core::orchestration_presets_api`. The handwritten copies
// were retired by the P5 schema lane, not copied.
