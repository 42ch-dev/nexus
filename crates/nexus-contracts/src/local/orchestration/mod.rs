//! Orchestration local types.
//!
//! Capability input/output schemas and related types that are local-only
//! (not observed by `nexus-platform`).
//!
//! Design: `docs/specs/orchestration/orchestration-engine.md` §5.3.

pub mod http;
pub mod preset;
pub mod preset_gate;

use serde::{Deserialize, Serialize};

// note: sync capabilities.

/// note: input for `sync.pull` — pull remote deltas for a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPullInput {
    /// note: force a full re-sync even if no changes are detected.
    #[serde(default)]
    pub force: bool,
}

/// note: output for `sync.pull`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPullOutput {
    /// note: number of deltas pulled.
    pub deltas_pulled: u64,
    /// note: whether any conflicts were detected.
    pub conflicts: bool,
}

/// note: input for `sync.push` — push local outbox to remote.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPushInput {
    /// note: force push even if outbox is empty.
    #[serde(default)]
    pub force: bool,
}

/// note: output for `sync.push`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPushOutput {
    /// note: number of outbox entries pushed.
    pub entries_pushed: u64,
}

// note: outbox capabilities.

/// note: input for `outbox.flush` — flush pending outbox entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxFlushInput {
    /// note: maximum number of entries to flush (0 = unlimited).
    #[serde(default)]
    pub limit: u32,
}

/// note: output for `outbox.flush`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxFlushOutput {
    /// note: number of entries flushed.
    pub flushed: u64,
}

/// note: input for `outbox.compact` — compact outbox table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxCompactInput {
    /// note: retention period in days for completed entries.
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

const fn default_retention_days() -> u32 {
    30
}

/// note: output for `outbox.compact`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxCompactOutput {
    /// note: number of entries removed.
    pub removed: u64,
    /// note: number of entries retained.
    pub retained: u64,
}

// note: workspace capabilities (v1.188 P3 — recoverable content commit).

/// note: input for `workspace.open` — open a scope-relative path within the bound workspace.
///
/// Unknown fields are IGNORED, matching every other capability input (v1.195
/// P0-T2 fix). The orchestration engine injects the trusted `_creator_id` /
/// `_session_id` into every capability invocation's arguments
/// (`tasks::CapabilityTask`), so a strict parse would refuse this capability in
/// any preset graph. The declared JSON Schema (`WORKSPACE_OPEN_INPUT_SCHEMA`,
/// `additionalProperties: false`) remains the wire-shape authority for
/// schema-validating callers, and the injected identity is always the engine's
/// trusted value — never a preset-supplied one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOpenInput {
    /// note: safe workspace-relative scope path (required).
    pub path: String,
}

/// note: snapshot captured at `workspace.open` for OCC and commit validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOpenSnapshot {
    /// note: canonical workspace creative root (absolute).
    pub workspace_root: String,
    /// note: scope path opened (relative to `workspace_root`).
    pub path: String,
    /// note: whether the scope path existed at open time.
    pub existed: bool,
    /// note: relative path → lowercase SHA-256 hex for tracked regular files in scope.
    #[serde(default)]
    pub file_hashes: std::collections::HashMap<String, String>,
}

/// note: output for `workspace.open`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOpenOutput {
    pub session_id: String,
    pub snapshot: WorkspaceOpenSnapshot,
}

/// note: operation type for a workspace commit manifest entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceChangeOp {
    Create,
    Modify,
    Delete,
}

/// note: one change in a `workspace.commit` manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceChangeEntry {
    /// note: scope-relative path for this change.
    pub path: String,
    pub op: WorkspaceChangeOp,
    /// note: lowercase SHA-256 of pre-image; null/absent for create.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
    /// note: canonical base64 content for create/modify; forbidden for delete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_base64: Option<String>,
}

/// note: input for `workspace.commit`.
///
/// Unknown fields are IGNORED for the same reason as
/// [`WorkspaceOpenInput`]: the engine injects trusted `_creator_id` /
/// `_session_id` into every capability invocation, and `changes` entries stay
/// strict about their own fields. The declared JSON Schema
/// (`WORKSPACE_COMMIT_INPUT_SCHEMA`) keeps `additionalProperties: false` for
/// schema-validating callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCommitInput {
    pub session_id: String,
    pub changes: Vec<WorkspaceChangeEntry>,
}

/// note: output for `workspace.commit` after durable completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCommitOutput {
    /// note: stable durable revision (`rev_<uuid>`).
    pub revision: String,
    /// note: true only after filesystem apply and durable intent/session consume.
    pub committed: bool,
}

/// note: input for one Work artifact byte commit (`Works/<work_ref>/<rel_child>`).
///
/// Carries the classified shape the caller already holds: the port builds the
/// absolute target itself from `workspace_root` + `work_ref` + `rel_child`, so
/// classification and target construction cannot disagree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkArtifactCommitInput {
    /// note: workspace whose `Works/` tree owns the artifact (absolute).
    pub workspace_root: String,
    /// note: the Work directory name (or Work id) the caller addressed.
    pub work_ref: String,
    /// note: the artifact path below the Work root; non-empty, `..`-free.
    pub rel_child: String,
    /// note: the bytes to commit.
    pub content: String,
}

/// note: output for [`WorkArtifactCommitInput`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkArtifactCommitOutput {
    /// note: true only after the bytes landed durably under the Work lock.
    pub committed: bool,
}

// note: registry capability.

/// note: input for `registry.refresh` — refresh ACP registry cache.
///
/// V1.58 P0 fix-wave (QC2 M-1): the `force` field was removed — it was a
/// documented no-op (no cache layer exists in the daemon runtime: the
/// synthetic snapshot is always fresh and the CDN path always fetches) but
/// the capability's help text advertised it as bypassing cache freshness,
/// which was misleading. Historical callers that still send `{"force": ...}`
/// are tolerated because serde ignores unknown fields by default.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRefreshInput {}

/// note: output for `registry.refresh`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRefreshOutput {
    /// note: age of the refreshed cache in milliseconds (0 for synthetic).
    pub cache_age_ms: u64,
    /// note: number of registered capabilities in the snapshot/registry.
    ///
    /// Renamed from `agent_count` in V1.56 P1 (R-V156P1-M001).
    /// The serde aliases preserve backward compatibility for consumers
    /// using the old field name.
    #[serde(alias = "agent_count", alias = "agentCount")]
    pub capability_count: u32,
    /// note: source of the registry data: `"synthetic"`, `"cdn"`, or `"synthetic_fallback"`.
    pub source: String,
    /// note: snapshot version string (e.g. "2026-06-22.v1").
    pub snapshot_version: String,
    /// note: ISO-8601 timestamp when the output was generated.
    pub generated_at: String,
    /// note: maximum fetch timeout in milliseconds (0 if not applicable).
    pub fetch_timeout_ms: u64,
    /// note: maximum retry count (0 if not applicable).
    pub max_retries: u32,
    /// note: number of retries actually attempted (0 if synthetic or first-attempt success).
    pub retry_count: u32,
    /// note: human-readable error description when fallback occurred (empty if success).
    pub fallback_reason: String,
}

// note: creator capabilities.

/// note: input for `creator.read_memory` — read entries from creator memory store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorReadMemoryInput {
    /// note: optional keyword filter.
    pub keyword: Option<String>,
    /// note: maximum entries to return.
    #[serde(default = "default_max_entries")]
    pub limit: u32,
}

const fn default_max_entries() -> u32 {
    50
}

/// note: output for `creator.read_memory`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorReadMemoryOutput {
    /// note: number of entries returned.
    pub count: u32,
}

/// note: input for `creator.write_memory` — append/update creator memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorWriteMemoryInput {
    /// note: memory entry content.
    pub content: String,
    /// note: keywords for retrieval.
    pub keywords: Vec<String>,
}

/// note: output for `creator.write_memory`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorWriteMemoryOutput {
    /// note: ID of the written memory entry.
    pub fragment_id: String,
}

/// note: input for `creator.inject_prompt` — queue a prompt for the next ACP call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorInjectPromptInput {
    /// note: prompt text to inject.
    pub prompt: String,
    /// note: optional priority (higher = sooner).
    #[serde(default)]
    pub priority: i32,
}

/// note: output for `creator.inject_prompt`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorInjectPromptOutput {
    /// note: confirmation that the prompt was queued.
    pub queued: bool,
}

// note: creator.write_brief — validate and persist creative brief on Work.

/// note: input for `creator.write_brief` — validate and persist a creative brief.
///
/// The `brief_text` is the raw text output from the synthesizing state,
/// which should contain valid JSON matching the creative brief schema
/// (work-experience-model §4). The capability validates before writing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorWriteBriefInput {
    /// note: work entity ID to write the brief to.
    pub work_id: String,
    /// note: raw brief text (expected to be JSON matching §4 schema).
    pub brief_text: String,
}

/// note: output for `creator.write_brief`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorWriteBriefOutput {
    /// note: whether the brief was successfully validated and written.
    pub written: bool,
    /// note: updated intake status of the Work.
    pub intake_status: String,
}

// note: judge capabilities.

/// note: input for `judge.rule` — evaluate a pure rule (no LLM).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeRuleInput {
    /// note: the rule expression to evaluate.
    pub rule: String,
    /// note: the context data to evaluate against.
    pub context_data: serde_json::Value,
}

/// note: output for `judge.rule`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeRuleOutput {
    /// note: whether the rule evaluated to true (go) or false (nogo).
    pub result: bool,
    /// note: human-readable reason for the decision.
    pub reason: String,
}

// note: FL-E stages (V1.34 creator-workflow §3.1).

/// note: ordered FL-E stages — single source of truth (V1.34 creator-workflow §3.1).
///
/// All crates that need the canonical stage list or stage-to-index mapping
/// must `use` this constant and the associated functions rather than
/// maintaining local copies.
pub const FL_E_STAGES: &[&str] = &["intake", "research", "produce", "review", "persist"];

/// note: returns the index of a stage in the FL-E linear order.
///
/// Returns `None` for unknown stage strings.
#[must_use]
pub fn stage_index(stage: &str) -> Option<usize> {
    FL_E_STAGES.iter().position(|&s| s == stage)
}

/// note: checks whether a stage string is a known FL-E stage.
#[must_use]
pub fn is_valid_stage(stage: &str) -> bool {
    FL_E_STAGES.contains(&stage)
}
