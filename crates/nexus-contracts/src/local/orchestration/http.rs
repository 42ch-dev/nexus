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
}

/// Response body for `GET /v1/daemon/orchestration/sessions/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionResponse {
    pub session: SessionSummary,
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
    /// One of `pause`, `resume`, `cancel`, `advance`.
    pub signal: String,
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
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

/// Response body for `GET /v1/daemon/orchestration/presets`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPresetsResponse {
    /// Loadable preset IDs.
    pub presets: Vec<String>,
}

/// Response body for `GET /v1/daemon/orchestration/presets/{id}/profile` (AR-20..23).
///
/// A manifest-derived profile for any resolvable preset (embedded, user, or
/// `_system.` qualified system preset). Every field is a pure read of the
/// already-loaded preset — no invented defaults; manifest fields the preset
/// does not carry serialize absent (AR-21).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetProfileResponse {
    /// Preset identifier from the loaded manifest (`LoadedPreset.id`).
    pub id: String,
    /// Preset schema version.
    pub version: u32,
    /// blake3 hex hash of the source YAML (identity across restarts).
    pub source_hash: String,
    /// Trigger-lane classification derived from the manifest + works-cron
    /// role membership (AR-21).
    pub lanes: PresetProfileLanes,
    /// Ordered outer state-machine states.
    pub states: Vec<PresetProfileState>,
    /// Role definitions (empty = single-agent mode).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<PresetProfileRole>,
    /// Capabilities this preset requires.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<String>,
    /// Declared signal bindings (declared, not delivered).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<PresetProfileSignal>,
}

/// Trigger-lane classification for a preset profile.
///
/// `cron` is derived per-preset from the shared works-cron role membership
/// (the brainstorm / write / review role presets). `session` is honest per
/// resolvability class: the session-start API loads embedded presets only,
/// so user presets report `session: false` (W-003/F-002). `wall_clock` /
/// `direct` are platform facts — the daemon schedule path resolves any
/// resolvable preset id, so every resolvable preset can fire on the
/// wall-clock poller or via a direct run with an explicit payload.
//
// The four bools are a 1:1 mirror of the locked trigger-lane vocabulary
// (PL-3: cron / wall-clock / session / direct) — a flat wire DTO, not a
// state machine. Refactoring into enums would change the JSON shape the
// CLI `preset show --json` must match verbatim (AR-25).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetProfileLanes {
    /// Cron (Work roles): this preset id is one of the works-cron role
    /// presets (brainstorm / write / review per `RolesSchedule`).
    pub cron: bool,
    /// Wall-clock poller: daemon schedule admission on a wall-clock tick.
    pub wall_clock: bool,
    /// Session start: `POST /v1/daemon/orchestration/sessions` / run path.
    /// Embedded and system presets only — user presets report `false`
    /// (the session-start API loads embedded presets, W-003/F-002).
    pub session: bool,
    /// Direct run: schedule start with an explicit run payload.
    pub direct: bool,
}

/// A single state in the outer state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetProfileState {
    /// Unique state identifier within this preset.
    pub id: String,
    /// Optional human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Enter action kinds (`capability` / `inner_graph` / `host_tool`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enter: Vec<PresetProfileEnterAction>,
    /// Exit condition kind (`llm_judge` / `rule` / `graph_complete` /
    /// `manual` / `timer`); absent for terminal states.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_when: Option<PresetProfileExitWhen>,
    /// Next transition form (`linear` / `goNogo` / `labeled` /
    /// `conditional` / `branches`); absent for terminal states.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<PresetProfileNext>,
    /// Whether this state is terminal (no outgoing transitions).
    pub terminal: bool,
}

/// A single enter action on a state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetProfileEnterAction {
    /// Action kind: `capability`, `inner_graph`, or `host_tool`.
    pub kind: String,
    /// Referenced name: capability name, inner graph name, or tool name.
    pub name: String,
}

/// Exit condition for a state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetProfileExitWhen {
    /// Exit condition kind: `llm_judge` / `rule` / `graph_complete` /
    /// `manual` / `timer`.
    pub kind: String,
    /// Judge prompt template path (`llm_judge`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_file: Option<String>,
    /// Judge capability name (`llm_judge`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge_capability: Option<String>,
    /// Minimum interval between re-evaluations (`llm_judge`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_interval: Option<String>,
    /// ISO-8601 duration to wait (`timer`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
}

/// Next transition form for a state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetProfileNext {
    /// Next form: `linear` / `goNogo` / `labeled` / `conditional` /
    /// `branches`.
    pub kind: String,
    /// Linear target state id (`linear`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// GO target state id (`goNogo`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub go: Option<String>,
    /// NOGO target state id (`goNogo`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nogo: Option<String>,
    /// Labeled edges (`labeled`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labeled: Vec<PresetProfileLabeledNext>,
    /// Conditional rules (`conditional` legacy form).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<PresetProfileConditionalRule>,
    /// Expression branches (`branches` form).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<PresetProfileConditionalRule>,
    /// Default target state id (`conditional` / `branches`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// A labeled next edge (`labeled` form).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetProfileLabeledNext {
    /// Label the judge returns to select this edge.
    pub label: String,
    /// Target state id.
    pub target: String,
}

/// A conditional rule (expression → target edge).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetProfileConditionalRule {
    /// Expression evaluated against context.
    pub when: String,
    /// Target state id if the expression evaluates to true.
    pub target: String,
}

/// A role definition for multi-agent presets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetProfileRole {
    /// Unique role ID within this preset.
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Path to the system prompt template (relative to the bundle root).
    pub system_prompt_file: String,
    /// Recommended skill slugs (ordered; first = primary).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommended_skills: Vec<String>,
}

/// A declared signal binding (declared, not delivered).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetProfileSignal {
    /// Declared signal name.
    pub name: String,
    /// Action kind on receive: `pause` / `force_transition`.
    pub action: String,
    /// Target state id (`force_transition`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}
