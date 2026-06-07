//! Orchestration local types.
//!
//! Capability input/output schemas and related types that are local-only
//! (not observed by `nexus-platform`).
//!
//! Design: `.mstar/knowledge/specs/orchestration-engine.md` §5.3.

pub mod http;
pub mod preset;
pub mod preset_gate;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Sync capabilities
// ---------------------------------------------------------------------------

/// Input for `sync.pull` — pull remote deltas for a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPullInput {
    /// Force a full re-sync even if no changes are detected.
    #[serde(default)]
    pub force: bool,
}

/// Output for `sync.pull`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPullOutput {
    /// Number of deltas pulled.
    pub deltas_pulled: u64,
    /// Whether any conflicts were detected.
    pub conflicts: bool,
}

/// Input for `sync.push` — push local outbox to remote.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPushInput {
    /// Force push even if outbox is empty.
    #[serde(default)]
    pub force: bool,
}

/// Output for `sync.push`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPushOutput {
    /// Number of outbox entries pushed.
    pub entries_pushed: u64,
}

// ---------------------------------------------------------------------------
// Outbox capabilities
// ---------------------------------------------------------------------------

/// Input for `outbox.flush` — flush pending outbox entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxFlushInput {
    /// Maximum number of entries to flush (0 = unlimited).
    #[serde(default)]
    pub limit: u32,
}

/// Output for `outbox.flush`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxFlushOutput {
    /// Number of entries flushed.
    pub flushed: u64,
}

/// Input for `outbox.compact` — compact outbox table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxCompactInput {
    /// Retention period in days for completed entries.
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

const fn default_retention_days() -> u32 {
    30
}

/// Output for `outbox.compact`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxCompactOutput {
    /// Number of entries removed.
    pub removed: u64,
    /// Number of entries retained.
    pub retained: u64,
}

// ---------------------------------------------------------------------------
// Workspace capabilities
// ---------------------------------------------------------------------------

/// Input for `workspace.open` — ensure workspace directory is present and valid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOpenInput {
    /// Workspace path (if None, uses default).
    pub path: Option<String>,
}

/// Output for `workspace.open`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOpenOutput {
    /// Resolved workspace path.
    pub workspace_path: String,
    /// Whether the workspace was created (vs already existed).
    pub created: bool,
}

/// Input for `workspace.commit` — commit manuscript diff into working copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCommitInput {
    /// Commit message.
    pub message: String,
}

/// Output for `workspace.commit`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCommitOutput {
    /// Commit hash or identifier.
    pub revision: String,
}

// ---------------------------------------------------------------------------
// Registry capability
// ---------------------------------------------------------------------------

/// Input for `registry.refresh` — refresh ACP registry cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRefreshInput {
    /// Force refresh even if cache is fresh.
    #[serde(default)]
    pub force: bool,
}

/// Output for `registry.refresh`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRefreshOutput {
    /// Age of the refreshed cache in milliseconds.
    pub cache_age_ms: u64,
    /// Number of agents in the registry.
    pub agent_count: u32,
}

// ---------------------------------------------------------------------------
// Creator capabilities
// ---------------------------------------------------------------------------

/// Input for `creator.read_memory` — read entries from creator memory store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorReadMemoryInput {
    /// Optional keyword filter.
    pub keyword: Option<String>,
    /// Maximum entries to return.
    #[serde(default = "default_max_entries")]
    pub limit: u32,
}

const fn default_max_entries() -> u32 {
    50
}

/// Output for `creator.read_memory`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorReadMemoryOutput {
    /// Number of entries returned.
    pub count: u32,
}

/// Input for `creator.write_memory` — append/update creator memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorWriteMemoryInput {
    /// Memory entry content.
    pub content: String,
    /// Keywords for retrieval.
    pub keywords: Vec<String>,
}

/// Output for `creator.write_memory`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorWriteMemoryOutput {
    /// ID of the written memory entry.
    pub fragment_id: String,
}

/// Input for `creator.inject_prompt` — queue a prompt for the next ACP call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorInjectPromptInput {
    /// Prompt text to inject.
    pub prompt: String,
    /// Optional priority (higher = sooner).
    #[serde(default)]
    pub priority: i32,
}

/// Output for `creator.inject_prompt`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorInjectPromptOutput {
    /// Confirmation that the prompt was queued.
    pub queued: bool,
}

// ---------------------------------------------------------------------------
// creator.write_brief — validate and persist creative brief on Work
// ---------------------------------------------------------------------------

/// Input for `creator.write_brief` — validate and persist a creative brief.
///
/// The `brief_text` is the raw text output from the synthesizing state,
/// which should contain valid JSON matching the creative brief schema
/// (work-experience-model §4). The capability validates before writing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorWriteBriefInput {
    /// Work entity ID to write the brief to.
    pub work_id: String,
    /// Raw brief text (expected to be JSON matching §4 schema).
    pub brief_text: String,
}

/// Output for `creator.write_brief`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatorWriteBriefOutput {
    /// Whether the brief was successfully validated and written.
    pub written: bool,
    /// Updated intake status of the Work.
    pub intake_status: String,
}

// ---------------------------------------------------------------------------
// Judge capabilities
// ---------------------------------------------------------------------------

/// Input for `judge.rule` — evaluate a pure rule (no LLM).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeRuleInput {
    /// The rule expression to evaluate.
    pub rule: String,
    /// The context data to evaluate against.
    pub context_data: serde_json::Value,
}

/// Output for `judge.rule`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeRuleOutput {
    /// Whether the rule evaluated to true (go) or false (nogo).
    pub result: bool,
    /// Human-readable reason for the decision.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// FL-E stages (V1.34 creator-workflow §3.1)
// ---------------------------------------------------------------------------

/// Ordered FL-E stages — single source of truth (V1.34 creator-workflow §3.1).
///
/// All crates that need the canonical stage list or stage-to-index mapping
/// must `use` this constant and the associated functions rather than
/// maintaining local copies.
pub const FL_E_STAGES: &[&str] = &["intake", "research", "produce", "review", "persist"];

/// Returns the index of a stage in the FL-E linear order.
///
/// Returns `None` for unknown stage strings.
#[must_use]
pub fn stage_index(stage: &str) -> Option<usize> {
    FL_E_STAGES.iter().position(|&s| s == stage)
}

/// Checks whether a stage string is a known FL-E stage.
#[must_use]
pub fn is_valid_stage(stage: &str) -> bool {
    FL_E_STAGES.contains(&stage)
}
