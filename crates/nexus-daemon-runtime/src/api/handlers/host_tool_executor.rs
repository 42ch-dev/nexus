//! Host tool executor — the daemon's HTTP/CLI/schedule entry point.
//!
//! v1.190 P3-T3: the dispatch BODY moved to
//! [`nexus_core::execution::capabilities`], together with the 30 builtin rows,
//! the admission pipeline, the audit writer and the peer registry. What
//! remains here is the daemon's own composition: it turns its
//! [`WorkspaceState`] into the narrow owned [`ToolContext`] the core consumes.
//!
//! # Why a bridge and not a copy
//!
//! The daemon could have kept its own registry and dispatch loop, but that is
//! exactly the second spine the plan forbids: the builtin set, the peer set
//! and the user-capability set would each be resolved twice and could silently
//! diverge. The bridge is deliberately thin — every decision (which gates run,
//! which row wins, what the audit says) is the core's.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::too_many_lines)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use nexus_core::execution::capabilities::{ToolContext, ToolRuntimeFacts};
use serde::{Deserialize, Serialize};

pub use nexus_core::execution::capabilities::{
    HostToolCallerKind, ToolExecuteRequest, ToolExecuteResponse,
};

// ─── V1.34 Tool IDs (spec §12.2) ──────────────────────────────────────────

/// Allowlist of all V1.34 + V1.53 P1 + V1.54 P0 + V1.56 P1 tool IDs.
///
/// The runtime allowlist check uses the registry itself; this constant is kept
/// for test consistency and as the documented canonical list.
#[allow(dead_code)] // Used in #[cfg(test)] modules only
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
    // nexus.* tools (V1.58 P3: DF-44 reference.refresh)
    "nexus.reference.refresh",
    // nexus.* tools (V1.59 P0: DF-47 manuscript & misc parity batch)
    "nexus.manuscript.list",
    "nexus.manuscript.read_range",
    "nexus.manuscript.write",
    "nexus.manuscript.phase.get",
    "nexus.manuscript.phase.set",
    "nexus.workspace.paths",
    "nexus.research.query",
    "nexus.runtime.health",
    "nexus.trace.correlation",
    // fs/* baseline (V1.33)
    "fs/read_text_file",
    "fs/write_text_file",
];

// ─── Response/error shapes ────────────────────────────────────────────────

/// Error body for a tool execution refusal.
#[derive(Debug, Serialize)]
pub struct ToolExecuteError {
    /// The refusal's error code.
    pub code: ToolErrorCode,
    /// Optional reason detail.
    pub reason: Option<String>,
    /// The refusal message.
    pub message: String,
}

/// The error codes the tool surface reports.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolErrorCode {
    /// The tool id is not dispatchable.
    NotSupported,
    /// The request's arguments are invalid.
    InvalidInput,
    /// Admission refused the call.
    PolicyBlocked,
    /// An internal fault.
    Internal,
}

// ─── HostToolExecutor — unified caller entry points ───────────────────────

/// Internal service for executing host tools.
///
/// All caller entry points normalize into the shared request shape and then
/// dispatch through the SAME core spine.
pub struct HostToolExecutor;

impl HostToolExecutor {
    /// **Entry point 1 (CLI + HTTP)**: execute a host tool request.
    ///
    /// # Errors
    /// The core dispatch refusal, mapped to the daemon's envelope.
    pub async fn execute(
        req: &ToolExecuteRequest,
        state: &WorkspaceState,
    ) -> Result<serde_json::Value, NexusApiError> {
        Self::registry_dispatch(req, state).await
    }

    /// Core dispatch: build the owned context, then run the core spine.
    ///
    /// The admission pipeline, the audit row and the handler invocation all
    /// live in the core; this function only composes the daemon's state into
    /// the narrow context the core consumes.
    ///
    /// # Errors
    /// `NexusApiError::Uninitialized` when the workspace has no pool yet;
    /// otherwise the core dispatch refusal.
    pub async fn registry_dispatch(
        req: &ToolExecuteRequest,
        state: &WorkspaceState,
    ) -> Result<serde_json::Value, NexusApiError> {
        let context = tool_context(state).await?;
        nexus_core::execution::capabilities::execute_tool(&context, req)
            .await
            .map_err(NexusApiError::from)
    }

    /// **Entry point 3 (Schedule)**: dispatch schedule-initiated tool call.
    ///
    /// Uses `HostToolCallerKind::Schedule` for audit-trail differentiation.
    ///
    /// # Errors
    /// As [`Self::execute`].
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

/// Compose the daemon's workspace state into the core's narrow tool context.
///
/// # Errors
/// `Uninitialized` when no pool is attached — the tool surface cannot dispatch
/// without durable storage, and reporting that is honest.
pub async fn tool_context(state: &WorkspaceState) -> Result<ToolContext, NexusApiError> {
    let pool = state.pool_or_uninit()?.clone();
    // The core opens lazily on first need; a tool that requires the family
    // services (Work patch, findings, pool entries) gets the same instance the
    // HTTP family routes use.
    let core = state.core_or_uninit().await.ok();
    Ok(ToolContext::new(
        pool,
        state.nexus_home().clone(),
        state.workspace_path(),
        ToolRuntimeFacts {
            runtime_mode: *state.runtime_mode(),
            is_initialized: state.is_initialized(),
            lifecycle_state: state.lifecycle_state().to_string(),
            started_at: state.started_at().to_rfc3339(),
            uptime_seconds: state.uptime_seconds(),
        },
        core,
        state.capability_registry_holder(),
    ))
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

// ─── Include tests from the retained test file ────────────────────────────

#[cfg(test)]
#[path = "host_tool_executor_tests.rs"]
mod tests;
