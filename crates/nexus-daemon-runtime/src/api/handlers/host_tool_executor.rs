#![allow(clippy::missing_errors_doc)]
#![allow(clippy::too_many_lines)]
//! Host Tool Executor — 3-caller entry points for daemon-mediated agent tool access.
//!
//! V1.57 P1: Refactored from god-file (4298→≤800 lines).
//! Three caller entry points (CLI, worker, HTTP) all dispatch through the
//! same `CapabilityRegistry::dispatch` path. Handlers live in
//! [`host_tool_handlers`]; admission/permission/audit live there too.
//!
//! # Architecture (post-V1.57 P1)
//!
//! ```text
//! CLI host-call          ─┐
//! Worker agent_tool_req  ─┤  normalize → admission → CapabilityRegistry::dispatch
//! HTTP ToolExecuteRequest ─┘
//! ```

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use serde::{Deserialize, Serialize};

// Import admission + audit from the handlers module
use super::host_tool_handlers::{admission_pipeline, audit_tool_execution};

// ─── V1.34 Tool IDs (spec §12.2) ──────────────────────────────────────────

/// Allowlist of all V1.34 + V1.53 P1 tool IDs.
pub(crate) const TOOL_ALLOWLIST: &[&str] = &[
    // nexus.* tools (V1.34)
    "nexus.context.whoami",
    "nexus.workspace.info",
    "nexus.work.get",
    "nexus.work.patch",
    "nexus.orchestration.schedule_status",
    "nexus.context.assemble",
    // nexus.* tools (V1.53 P1: DF-46 read-heavy slice)
    "nexus.world.snapshot.get",
    "nexus.timeline.recent.get",
    "nexus.kb_snapshot.read",
    "nexus.manuscript.chapter.get",
    "nexus.observability.daemon.health",
    // nexus.* tools (V1.54 P0: DF-46 write tools)
    "nexus.kb_snapshot.write",
    "nexus.manuscript.chapter.update",
    "nexus.world.configure",
    "nexus.work.schedule.set",
    "nexus.finding.resolve",
    "nexus.pool.entry.manage",
    // nexus.* tools (V1.56 P1: DF-29 registry.refresh)
    "nexus.registry.refresh",
    // fs/* baseline (V1.33)
    "fs/read_text_file",
    "fs/write_text_file",
];

/// Fields allowed in `nexus.work.patch` (spec §4.4).
pub(crate) const PATCH_ALLOWED_FIELDS: &[&str] = &["title", "inspiration_log", "stage_metadata"];

/// Fields explicitly rejected in `nexus.work.patch` (spec §4.4).
pub(crate) const PATCH_REJECTED_FIELDS: &[&str] = &[
    "current_stage",
    "stage",
    "stage_status",
    "stage_started_at",
    "stage_completed_at",
    "creator_id",
    "workspace_id",
    "work_id",
    "run_intents",
];

/// Sub-fields allowed inside `stage_metadata` (spec §4.4).
pub(crate) const STAGE_METADATA_ALLOWED_KEYS: &[&str] = &[
    "agent_notes",
    "research_summary_ref",
    "draft_outline_ref",
    "review_summary_ref",
    "last_agent_tool_request_id",
];

// ─── Request / Response types ─────────────────────────────────────────────

/// Request for executing a host tool through the daemon.
///
/// Shared by HTTP, internal agent-host route, and worker upcall (spec §5).
#[derive(Debug, Clone, Deserialize)]
pub struct ToolExecuteRequest {
    pub tool_name: String,
    pub parameters: serde_json::Value,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub caller_kind: Option<HostToolCallerKind>,
}

/// Who is calling the tool registry (spec §12.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostToolCallerKind {
    AcpAgent,
    Schedule,
}

impl std::fmt::Display for HostToolCallerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AcpAgent => write!(f, "acp_agent"),
            Self::Schedule => write!(f, "schedule"),
        }
    }
}

/// Response from tool execution.
#[derive(Debug, Serialize)]
pub struct ToolExecuteResponse {
    pub success: bool,
    pub result: serde_json::Value,
}

/// Tool error code (spec §12.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolErrorCode {
    Forbidden,
    PolicyBlocked,
    NotSupported,
    InvalidInput,
}

/// Structured tool error for audit and wire response.
#[derive(Debug, Serialize)]
pub struct ToolExecuteError {
    pub code: ToolErrorCode,
    pub reason: Option<String>,
    pub message: String,
}

// ─── HostToolExecutor — unified 3-caller entry points ─────────────────────

/// Internal service for executing host tools via 3 caller entry points.
///
/// V1.57 P1: Refactored from god-file. Three caller entry points exist:
/// 1. CLI `host-call` — normalizes `host-call <tool_id> --args <json>`
/// 2. Worker `agent_tool_request` — normalizes worker IPC messages
/// 3. HTTP `ToolExecuteRequest` — normalizes daemon HTTP POST payload
///
/// All three dispatch through the same `CapabilityRegistry::dispatch` path.
pub struct HostToolExecutor;

impl HostToolExecutor {
    /// **Entry point 1 (CLI + HTTP)**: Execute a host tool request.
    ///
    /// Normalizes the `ToolExecuteRequest` wire format → admission pipeline
    /// → `CapabilityRegistry::dispatch` → audit log.
    pub async fn execute(
        req: &ToolExecuteRequest,
        state: &WorkspaceState,
    ) -> Result<serde_json::Value, NexusApiError> {
        Self::registry_dispatch(req, state).await
    }

    /// **Entry point 2 (Worker upcall)**: Dispatch `agent_tool_request` through registry.
    ///
    /// Normalizes `worker/agent_tool_request { tool_name, args, request_id }`
    /// into `ToolExecuteRequest` → same admission + dispatch + audit path.
    pub async fn dispatch_from_worker(
        tool_name: &str,
        args: &serde_json::Value,
        request_id: &str,
        state: &WorkspaceState,
    ) -> WorkerToolResult {
        let req = ToolExecuteRequest {
            tool_name: tool_name.to_string(),
            parameters: args.clone(),
            session_id: None,
            request_id: Some(request_id.to_string()),
            caller_kind: Some(HostToolCallerKind::AcpAgent),
        };

        match Self::execute(&req, state).await {
            Ok(result) => WorkerToolResult {
                request_id: request_id.to_string(),
                grant: true,
                output: Some(result),
                error: None,
            },
            Err(e) => {
                let code = e.error_code().to_string();
                let message = e.to_string();
                WorkerToolResult {
                    request_id: request_id.to_string(),
                    grant: false,
                    output: None,
                    error: Some(WorkerToolError { code, message }),
                }
            }
        }
    }

    /// Core dispatch: admission pipeline → `CapabilityRegistry::dispatch` → audit.
    pub async fn registry_dispatch(
        req: &ToolExecuteRequest,
        state: &WorkspaceState,
    ) -> Result<serde_json::Value, NexusApiError> {
        tracing::info!(
            tool_name = %req.tool_name,
            caller_kind = ?req.caller_kind,
            "HostToolExecutor: executing tool via CapabilityRegistry"
        );

        // Gates 1–4
        let admission_result = admission_pipeline(req, state);

        let (creator_id, _workspace_slug) = match admission_result {
            Ok(pair) => pair,
            Err(e) => {
                let error_code = e.error_code();
                audit_tool_execution(req, "denied", Some(error_code), state).await?;
                return Err(e);
            }
        };

        let reg = crate::capability_registry::host_tool_registry();
        let dispatch_result = reg.dispatch(req, state, &creator_id).await;

        match &dispatch_result {
            Ok(_) => {
                audit_tool_execution(req, "success", None, state).await?;
            }
            Err(e) => {
                let error_code = e.error_code();
                audit_tool_execution(req, "denied", Some(error_code), state).await?;
            }
        }

        dispatch_result
    }

    /// **Entry point 3 (Schedule)**: Dispatch schedule-initiated tool call.
    ///
    /// Uses `HostToolCallerKind::Schedule` for audit trail differentiation.
    /// Returns the tool result JSON or `NexusApiError`.
    pub async fn dispatch_for_schedule(
        tool_name: &str,
        args: &serde_json::Value,
        request_id: &str,
        state: &WorkspaceState,
    ) -> Result<serde_json::Value, NexusApiError> {
        let req = ToolExecuteRequest {
            tool_name: tool_name.to_string(),
            parameters: args.clone(),
            session_id: None,
            request_id: Some(request_id.to_string()),
            caller_kind: Some(HostToolCallerKind::Schedule),
        };

        Self::execute(&req, state).await
    }
}

/// Worker upcall result shape.
#[derive(Debug, Serialize)]
pub struct WorkerToolResult {
    pub request_id: String,
    pub grant: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<WorkerToolError>,
}

/// Error payload in worker upcall result.
#[derive(Debug, Serialize)]
pub struct WorkerToolError {
    pub code: String,
    pub message: String,
}

// ─── DaemonToolDispatch adapter (DF-47) ───────────────────────────────────

/// Adapter implementing [`nexus_orchestration::capability::DaemonToolDispatch`].
pub struct DaemonToolDispatchAdapter {
    state: WorkspaceState,
}

impl DaemonToolDispatchAdapter {
    #[must_use]
    pub const fn new(state: WorkspaceState) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl nexus_orchestration::capability::DaemonToolDispatch for DaemonToolDispatchAdapter {
    async fn dispatch_tool(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        request_id: &str,
    ) -> Result<serde_json::Value, nexus_orchestration::capability::CapabilityError> {
        HostToolExecutor::dispatch_for_schedule(tool_name, args, request_id, &self.state)
            .await
            .map_err(|e| match &e {
                NexusApiError::Forbidden { .. } => {
                    nexus_orchestration::capability::CapabilityError::Forbidden(format!(
                        "daemon tool dispatch failed for {tool_name}: {e}"
                    ))
                }
                _ => nexus_orchestration::capability::CapabilityError::Internal(format!(
                    "daemon tool dispatch failed for {tool_name}: {e}"
                )),
            })
    }
}

// ─── Re-exports from host_tool_handlers (for capability_registry.rs) ──────
//
// `capability_registry.rs::build_registry()` imports via
// `use crate::api::handlers::host_tool_executor as hte`
// and calls `hte::registry_*`. These re-exports preserve backward compat.

pub(crate) use super::host_tool_handlers::registry_context_assemble;
pub(crate) use super::host_tool_handlers::registry_context_whoami;
pub(crate) use super::host_tool_handlers::registry_daemon_health;
pub(crate) use super::host_tool_handlers::registry_finding_resolve;
pub(crate) use super::host_tool_handlers::registry_kb_snapshot_read;
pub(crate) use super::host_tool_handlers::registry_kb_snapshot_write;
pub(crate) use super::host_tool_handlers::registry_manuscript_chapter_get;
pub(crate) use super::host_tool_handlers::registry_manuscript_chapter_update;
pub(crate) use super::host_tool_handlers::registry_pool_entry_manage;
pub(crate) use super::host_tool_handlers::registry_read_file;
pub(crate) use super::host_tool_handlers::registry_registry_refresh;
pub(crate) use super::host_tool_handlers::registry_schedule_status;
pub(crate) use super::host_tool_handlers::registry_timeline_recent_get;
pub(crate) use super::host_tool_handlers::registry_work_get;
pub(crate) use super::host_tool_handlers::registry_work_patch;
pub(crate) use super::host_tool_handlers::registry_work_schedule_set;
pub(crate) use super::host_tool_handlers::registry_workspace_info;
pub(crate) use super::host_tool_handlers::registry_world_configure;
pub(crate) use super::host_tool_handlers::registry_world_snapshot_get;
pub(crate) use super::host_tool_handlers::registry_write_file;

// ─── Include tests from extracted test file ───────────────────────────────

#[cfg(test)]
#[path = "host_tool_executor_tests.rs"]
mod tests;
