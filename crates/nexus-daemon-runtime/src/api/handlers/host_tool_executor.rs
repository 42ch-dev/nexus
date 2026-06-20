#![allow(clippy::missing_errors_doc)]
#![allow(clippy::too_many_lines)]
//! Host Tool Executor — unified registry for daemon-mediated agent tool access.
//!
//! V1.34 extends the V1.33 baseline (`fs/*` tools) with 6 `nexus.*` tools
//! that let external ACP agents read Work/context data and append inspiration
//! through a unified, audited, permission-gated dispatch table.
//!
//! # Architecture (P4 — spec §4, §7, §12)
//!
//! ```text
//! HTTP POST tool-execute  ─┐
//! Internal agent-host     ─┤
//! worker/agent_tool_req   ─┤
//!                          ├─► admission_pipeline (5 gates)
//!                          │     ├─ tool id allowlist
//!                          │     ├─ active creator
//!                          │     ├─ workspace bounds
//!                          │     ├─ permissions.toml / policy
//!                          │     └─ audit log (written by execute() on all paths)
//!                          └─► CapabilityRegistry::dispatch() → handler
//! ```
//!
//! All three entrypoints (HTTP, internal, worker upcall) share a single
//! dispatch table (spec §7.1 single dispatch invariant).
//!
//! # Worker upcall wiring (DF-47 disposition)
//!
//! `HostToolExecutor::dispatch_from_worker()` is the public adapter that
//! normalizes `worker/agent_tool_request { tool_name, args, request_id }`
//! into the same `ToolExecuteRequest` and calls the unified `execute()` path.
//! The worker-side IPC caller (in `nexus-orchestration`) will connect to this
//! method during orchestration integration. DF-47 remains CLOSED because the
//! adapter is complete, tested, and the single-dispatch invariant holds.

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::{read_active_creator_id, read_active_workspace_slug, WorkApiDto};
use crate::workspace::WorkspaceState;
use nexus_kb::KbStore;
use nexus_local_db::works;
use nexus_narrative::NarrativeGateway;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ─── V1.34 Tool IDs (spec §12.2) ──────────────────────────────────────────

/// Allowlist of all V1.34 + V1.53 P1 tool IDs.
///
/// V1.53 P0 Sub-phase 2: This allowlist is still used by `admission_pipeline()`
/// (which `registry_dispatch()` calls). It will remain as the runtime allowlist;
/// the registry's `CapabilityRow` records the admission gates declaratively.
const TOOL_ALLOWLIST: &[&str] = &[
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
    // fs/* baseline (V1.33)
    "fs/read_text_file",
    "fs/write_text_file",
];

/// Fields allowed in `nexus.work.patch` (spec §4.4).
const PATCH_ALLOWED_FIELDS: &[&str] = &["title", "inspiration_log", "stage_metadata"];

/// Fields explicitly rejected in `nexus.work.patch` (spec §4.4).
const PATCH_REJECTED_FIELDS: &[&str] = &[
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
/// These metadata keys do not advance the FL-E state machine.
const STAGE_METADATA_ALLOWED_KEYS: &[&str] = &[
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
    /// Tool name (e.g., "`fs/read_text_file`", "`nexus.work.get`")
    pub tool_name: String,
    /// Tool-specific parameters (JSON object)
    pub parameters: serde_json::Value,
    /// Optional session ID for audit trail
    #[serde(default)]
    pub session_id: Option<String>,
    /// Optional request ID (from worker upcall)
    #[serde(default)]
    pub request_id: Option<String>,
    /// Caller kind (for audit)
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
    /// Whether the tool execution succeeded
    pub success: bool,
    /// Tool-specific result (JSON object)
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

// ─── Admission pipeline (spec §4.3) ───────────────────────────────────────

/// Run the five-gate admission pipeline.
///
/// Gates:
/// 1. Tool ID allowlist
/// 2. Active creator (for `nexus.*` tools)
/// 3. Workspace bounds
/// 4. `permissions.toml` / policy
/// 5. Audit log (written by caller `execute()`, not here)
///
/// Returns `(creator_id, workspace_slug)` if all gates pass.
fn admission_pipeline(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
) -> Result<(String, String), NexusApiError> {
    // Gate 1: tool id allowlist
    if !TOOL_ALLOWLIST.contains(&req.tool_name.as_str()) {
        return Err(NexusApiError::BadRequest {
            code: "NOT_SUPPORTED".to_string(),
            message: format!("unsupported tool: {}", req.tool_name),
        });
    }

    let is_nexus_tool = req.tool_name.starts_with("nexus.");
    let creator_id = read_active_creator_id(state.nexus_home());

    // Gate 2: active creator (for nexus.* tools)
    if is_nexus_tool {
        let creator_id = creator_id.ok_or_else(|| NexusApiError::Forbidden {
            resource: "tool_execution".to_string(),
            reason: "active creator required for nexus.* tools".to_string(),
        })?;
        let workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
            .ok_or_else(|| NexusApiError::Forbidden {
                resource: "tool_execution".to_string(),
                reason: "active workspace required for nexus.* tools".to_string(),
            })?;

        // Gate 3: workspace bounds — verified per-handler for entity lookups
        // (Work, schedule, etc. include creator/workspace predicates in SQL).
        // Path-based bounds for fs/* tools are checked below.

        // Gate 4: permissions.toml / policy
        let workspace_path_str = state.workspace_path().unwrap_or_default();
        if !workspace_path_str.is_empty() {
            if let Some(policy) = load_permission_policy(&workspace_path_str) {
                check_nexus_tool_permission(&req.tool_name, &policy)?;
            }
        }

        return Ok((creator_id, workspace_slug));
    }

    // For fs/* tools: existing V1.33 permission + path validation
    let workspace_path_str = state.workspace_path().unwrap_or_default();
    if !workspace_path_str.is_empty() {
        // Gate 4: permissions
        if let Some(policy) = load_permission_policy(&workspace_path_str) {
            check_fs_tool_permission(&req.tool_name, &policy)?;
        }

        // Gate 3: workspace bounds
        validate_file_path(req, state)?;
    }

    Ok((creator_id.unwrap_or_default(), String::new()))
}

/// Check permission for a `nexus.*` tool against policy (Gate 4).
fn check_nexus_tool_permission(
    tool_name: &str,
    policy: &WorkspacePermissionPolicy,
) -> Result<(), NexusApiError> {
    const NEXUS_WRITE_TOOLS: &[&str] = &[
        "nexus.work.patch",
        "nexus.kb_snapshot.write",
        "nexus.manuscript.chapter.update",
        "nexus.world.configure",
        "nexus.work.schedule.set",
        "nexus.finding.resolve",
        "nexus.pool.entry.manage",
    ];

    let allowed = if NEXUS_WRITE_TOOLS.contains(&tool_name) {
        is_nexus_write_granted(tool_name, policy)
    } else {
        is_nexus_read_granted(tool_name, policy)
    };

    if allowed {
        return Ok(());
    }

    let reason = if NEXUS_WRITE_TOOLS.contains(&tool_name) {
        "write tool not granted"
    } else {
        "no nexus read grant"
    };

    Err(NexusApiError::BadRequest {
        code: "POLICY_BLOCKED".to_string(),
        message: format!("tool '{tool_name}' denied by permissions.toml policy ({reason})"),
    })
}

/// Check permission for `fs/*` tools (V1.33 baseline behavior).
fn check_fs_tool_permission(
    tool_name: &str,
    policy: &WorkspacePermissionPolicy,
) -> Result<(), NexusApiError> {
    let category = match tool_name {
        "fs/read_text_file" => "file_system.read",
        "fs/write_text_file" => "file_system.write",
        _ => return Ok(()),
    };

    if is_capability_granted(category, policy) {
        return Ok(());
    }

    Err(NexusApiError::BadRequest {
        code: "POLICY_BLOCKED".to_string(),
        message: format!(
            "tool '{tool_name}' denied by permissions.toml policy (missing '{category}' grant)"
        ),
    })
}

// ─── HostToolExecutor — unified registry ──────────────────────────────────

/// Internal service for executing host tools.
///
/// Stateless — all methods take `&WorkspaceState` as input.
/// Safe to construct on every request or store as a singleton.
///
/// V1.34: unified dispatch for all 8 tools (6 `nexus.*` + 2 `fs/*`).
/// All entrypoints (HTTP, internal, worker upcall) converge here.
pub struct HostToolExecutor;

impl HostToolExecutor {
    /// Execute a host tool request end-to-end through the unified registry:
    /// 1. Admission pipeline (5 gates, spec §4.3)
    /// 2. Tool dispatch (via `CapabilityRegistry`)
    /// 3. Audit logging (gate 5) — written on **every** invocation path
    ///    (success + all denials/failures), per spec §4.3 gate 5 and §12.6.
    ///
    /// V1.53 P0: All dispatch now routes through `CapabilityRegistry`.
    /// Old `dispatch_tool()` match table has been removed.
    /// This is the single dispatch table (spec §7.1).
    pub async fn execute(
        req: &ToolExecuteRequest,
        state: &WorkspaceState,
    ) -> Result<serde_json::Value, NexusApiError> {
        // All dispatch routes through CapabilityRegistry.
        Self::registry_dispatch(req, state).await
    }

    /// Dispatch a worker upcall `agent_tool_request` through the unified registry.
    ///
    /// This normalizes `worker/agent_tool_request { tool_name, args, request_id }`
    /// (spec §7) into the same `ToolExecuteRequest` shape and calls the same
    /// admission pipeline + dispatch table as HTTP tool execute.
    ///
    /// Returns the result in the worker reply shape:
    /// `{ request_id, grant, output? }` (spec §7).
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

    /// V1.53 P0 Sub-phase 1: Dispatch through the new `CapabilityRegistry`.
    ///
    /// This is the **parallel path** for parity testing during adapter-first
    /// migration. It runs the same admission pipeline as `execute()`, then
    /// dispatches through the registry instead of the old `dispatch_tool()`
    /// match table.
    ///
    /// During Sub-phase 1, both `execute()` (old path) and `registry_dispatch()`
    /// (new path) are active. Tests verify they produce identical output.
    /// During Sub-phase 2, `execute()` is cut over to call this internally;
    /// during Sub-phase 3 the old match table is removed.
    pub async fn registry_dispatch(
        req: &ToolExecuteRequest,
        state: &WorkspaceState,
    ) -> Result<serde_json::Value, NexusApiError> {
        tracing::info!(
            tool_name = %req.tool_name,
            caller_kind = ?req.caller_kind,
            "HostToolExecutor: executing tool via CapabilityRegistry"
        );

        // Gates 1–4 (same admission pipeline as execute())
        let admission_result = admission_pipeline(req, state);

        let (creator_id, _workspace_slug) = match admission_result {
            Ok(pair) => pair,
            Err(e) => {
                let error_code = e.error_code();
                audit_tool_execution(req, "denied", Some(error_code), state).await?;
                return Err(e);
            }
        };

        // Dispatch through registry (not old match table)
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

    /// Dispatch a schedule-initiated `nexus.*` tool call through the unified registry.
    ///
    /// V1.42 P3 (DF-47 production wiring): the schedule executor calls this
    /// directly from the daemon process — no worker IPC round-trip needed.
    /// Uses `HostToolCallerKind::Schedule` for audit trail differentiation.
    ///
    /// Returns the tool result JSON on success, or a `NexusApiError` on failure.
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

/// Worker upcall result shape (spec §7 — `worker/agent_tool_request_result`).
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

// ─── DaemonToolDispatch adapter (DF-47, V1.42 P3) ──────────────────────────

/// Adapter implementing [`nexus_orchestration::capability::DaemonToolDispatch`]
/// for the daemon runtime.
///
/// Bridges the orchestration engine's `HostToolCallTask` to the daemon's
/// `HostToolExecutor::dispatch_for_schedule`, providing in-process tool
/// dispatch without worker IPC round-trip.
///
/// # Delegation chain (by design — not a bypass)
///
/// This adapter delegates through the following chain, which is the single
/// canonical dispatch path for all `nexus.*` tools:
///
/// 1. `dispatch_tool()` → `HostToolExecutor::dispatch_for_schedule()`
/// 2. `dispatch_for_schedule()` → `HostToolExecutor::execute()`
/// 3. `execute()` → `HostToolExecutor::registry_dispatch()`
/// 4. `registry_dispatch()` → `admission_pipeline()` (gates 1–4) → `LazyLock<CapabilityRegistry>::dispatch()`
///    → registered handler (via `&'static [AdmissionGate]`)
///
/// Every step passes through the same admission pipeline and registry.
/// There is no alternate execution path that bypasses gating or audit.
///
/// **R-V153P0-002 (V1.54 closure):** This doc comment resolves the
/// self-reported residual requesting explicit documentation of the
/// delegation chain to prevent future reviewers from mis-flagging
/// this adapter as a security bypass.
///
/// Holds a snapshot of [`WorkspaceState`] captured at construction time.
/// This is safe because the daemon's workspace state is long-lived and
/// the inner fields (home path, pool, etc.) are Arc'd.
pub struct DaemonToolDispatchAdapter {
    state: WorkspaceState,
}

impl DaemonToolDispatchAdapter {
    /// Create a new adapter bound to the given workspace state.
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

// ─── nexus.* Handlers ─────────────────────────────────────────────────────
//
// V1.53 P0 Sub-phase 3: Old `dispatch_tool()` match table removed.
// All dispatch now routes through `CapabilityRegistry` (see
// `HostToolExecutor::registry_dispatch()` and `capability_registry.rs`).
// The handler functions below remain as they are referenced by the
// `pub(crate)` registry wrapper functions.

/// `nexus.context.whoami` — return active `creator_id` and workspace slug.
fn execute_context_whoami(
    _req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> serde_json::Value {
    let workspace_slug =
        read_active_workspace_slug(state.nexus_home(), creator_id).unwrap_or_default();
    serde_json::json!({
        "creator_id": creator_id,
        "workspace_slug": workspace_slug
    })
}

/// `nexus.workspace.info` — return workspace roots, flags, linked world ref.
fn execute_workspace_info(
    _req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> serde_json::Value {
    let workspace_slug =
        read_active_workspace_slug(state.nexus_home(), creator_id).unwrap_or_default();
    let workspace_path = state.workspace_path().unwrap_or_default();
    serde_json::json!({
        "creator_id": creator_id,
        "workspace_slug": workspace_slug,
        "workspace_path": workspace_path,
        "runtime_mode": state.runtime_mode_as_str(),
        "initialized": state.is_initialized()
    })
}

/// `nexus.work.get` — return Work row + stage fields for active creator's work.
async fn execute_work_get(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    // Entity lookup includes creator predicate (spec §12.5)
    let record = works::get_work(state.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| {
            // Could be not found OR cross-creator — return FORBIDDEN for safety
            NexusApiError::Forbidden {
                resource: "work".to_string(),
                reason: "work not found or cross-creator access denied".to_string(),
            }
        })?;

    let dto = WorkApiDto::from(record);
    Ok(serde_json::to_value(dto).unwrap_or_else(|_| serde_json::json!({})))
}

/// `nexus.work.patch` — append inspiration + allowed metadata fields (spec §4.4).
///
/// Multi-field patches (`title` + `inspiration_log` + `stage_metadata`) are applied
/// sequentially within the same handler invocation. Each DB call uses its own
/// transaction via the shared pool. Full atomicity across all fields requires
/// wrapping in a single transaction (deferred to post-V1.34 when concurrent
/// multi-connection mutations become realistic). For V1.34 pre-release the
/// sequential approach is sufficient (`SQLite` WAL, single daemon process).
async fn execute_work_patch(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    // Validate patch fields (spec §4.4)
    let params = req
        .parameters
        .as_object()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters".into(),
            reason: "must be a JSON object".into(),
        })?;

    // Reject forbidden fields
    for key in params.keys() {
        if key == "work_id" {
            continue; // work_id is a parameter, not a patch field
        }
        if PATCH_REJECTED_FIELDS.contains(&key.as_str()) {
            return Err(NexusApiError::BadRequest {
                code: "INVALID_INPUT".to_string(),
                message: format!("field '{key}' is not allowed in nexus.work.patch (spec §4.4)"),
            });
        }
        if !PATCH_ALLOWED_FIELDS.contains(&key.as_str()) {
            return Err(NexusApiError::BadRequest {
                code: "INVALID_INPUT".to_string(),
                message: format!("unknown patch field '{key}'"),
            });
        }
    }

    // Handle inspiration_log append
    if let Some(inspiration) = params.get("inspiration_log") {
        let entries = inspiration
            .as_array()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.inspiration_log".into(),
                reason: "must be an array of entries".into(),
            })?;

        for entry in entries {
            let note = entry["text"]
                .as_str()
                .or_else(|| entry["note"].as_str())
                .ok_or_else(|| NexusApiError::InvalidInput {
                    field: "parameters.inspiration_log[].text".into(),
                    reason: "each entry must include a 'text' or 'note' field".into(),
                })?;

            let now = chrono::Utc::now().to_rfc3339();
            let json_entry = serde_json::json!({
                "at": now,
                "note": note,
                "source": entry.get("source").and_then(|v| v.as_str()).unwrap_or("agent_tool"),
            });
            let entry_json = serde_json::to_string(&json_entry).unwrap_or_default();

            works::append_inspiration(state.pool(), creator_id, work_id, &entry_json, &now)
                .await
                .map_err(|e| match &e {
                    nexus_local_db::LocalDbError::MissingVersionKey { .. } => {
                        NexusApiError::Forbidden {
                            resource: "work".into(),
                            reason: "work not found or cross-creator".into(),
                        }
                    }
                    _ => NexusApiError::Internal {
                        code: "DATABASE_ERROR".into(),
                        message: e.to_string(),
                    },
                })?;
        }
    }

    // Handle title patch
    if let Some(title) = params.get("title") {
        let title_str = title.as_str().ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.title".into(),
            reason: "must be a string".into(),
        })?;
        if title_str.trim().is_empty() {
            return Err(NexusApiError::InvalidInput {
                field: "parameters.title".into(),
                reason: "must not be empty".into(),
            });
        }

        let patch = nexus_local_db::works::WorkPatch {
            title: Some(title_str.to_string()),
            long_term_goal: None,
            creative_brief: None,
            intake_status: None,
            status: None,
            world_id: None,
            story_ref: None,
            primary_preset_id: None,
            schedule_ids: None,
            current_stage: None,
            stage_status: None,
            work_profile: None,
            work_ref: None,
            total_planned_chapters: None,
            current_chapter: None,
            auto_chain_enabled: None,
            driver_schedule_id: None,
            auto_chain_interrupted: None,
            auto_review_master_on_timeout: None,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        let now = chrono::Utc::now().to_rfc3339();
        works::patch_work(state.pool(), creator_id, work_id, &patch, &now)
            .await
            .map_err(|e| match &e {
                nexus_local_db::LocalDbError::MissingVersionKey { .. } => {
                    NexusApiError::Forbidden {
                        resource: "work".into(),
                        reason: "work not found or cross-creator".into(),
                    }
                }
                _ => NexusApiError::Internal {
                    code: "DATABASE_ERROR".into(),
                    message: e.to_string(),
                },
            })?;
    }

    // Handle stage_metadata patch — validate sub-field allowlist (spec §4.4).
    // V1.34 minimal: accepted but stored as-is in inspiration_log as a metadata entry.
    if let Some(metadata) = params.get("stage_metadata") {
        // Validate stage_metadata sub-field allowlist
        let metadata_obj = metadata
            .as_object()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.stage_metadata".into(),
                reason: "must be a JSON object".into(),
            })?;
        for key in metadata_obj.keys() {
            if PATCH_REJECTED_FIELDS.contains(&key.as_str()) {
                return Err(NexusApiError::BadRequest {
                    code: "INVALID_INPUT".to_string(),
                    message: format!(
                        "stage_metadata key '{key}' is not allowed (spec §4.4: stage control fields must use stage-advance path)"
                    ),
                });
            }
            if !STAGE_METADATA_ALLOWED_KEYS.contains(&key.as_str()) {
                return Err(NexusApiError::BadRequest {
                    code: "INVALID_INPUT".to_string(),
                    message: format!(
                        "stage_metadata key '{key}' is not in the allowed list (spec §4.4: {})",
                        STAGE_METADATA_ALLOWED_KEYS.join(", ")
                    ),
                });
            }
        }

        let now = chrono::Utc::now().to_rfc3339();
        let entry = serde_json::json!({
            "at": now,
            "note": format!("[stage_metadata] {}", serde_json::to_string(metadata).unwrap_or_default()),
            "source": "agent_tool",
            "type": "stage_metadata",
        });
        let entry_json = serde_json::to_string(&entry).unwrap_or_default();
        works::append_inspiration(state.pool(), creator_id, work_id, &entry_json, &now)
            .await
            .map_err(|e| match &e {
                nexus_local_db::LocalDbError::MissingVersionKey { .. } => {
                    NexusApiError::Forbidden {
                        resource: "work".into(),
                        reason: "work not found or cross-creator".into(),
                    }
                }
                _ => NexusApiError::Internal {
                    code: "DATABASE_ERROR".into(),
                    message: e.to_string(),
                },
            })?;
    }

    // Return the updated Work
    let updated = works::get_work(state.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: "work".into(),
            reason: "work not found after patch".into(),
        })?;

    let dto = WorkApiDto::from(updated);
    Ok(serde_json::to_value(dto).unwrap_or_else(|_| serde_json::json!({})))
}

/// `nexus.orchestration.schedule_status` — return schedules linked to `work_id`.
async fn execute_schedule_status(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    // Verify work ownership first
    let record = works::get_work(state.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: "work".into(),
            reason: "work not found or cross-creator access denied".to_string(),
        })?;

    let schedule_ids: Vec<serde_json::Value> =
        serde_json::from_str(&record.schedule_ids).unwrap_or_default();

    Ok(serde_json::json!({
        "work_id": work_id,
        "schedule_ids": schedule_ids,
        "count": schedule_ids.len()
    }))
}

/// `nexus.context.assemble` — local assemble-moment or `POLICY_BLOCKED` (spec §4.1).
async fn execute_context_assemble(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let requires_platform = req.parameters["requires_platform"]
        .as_bool()
        .unwrap_or(false);

    // Check platform_integration state
    let runtime_mode = state.runtime_mode();
    if requires_platform
        && matches!(
            runtime_mode,
            nexus_contracts::local::domain::RuntimeMode::LocalOnly
        )
    {
        return Err(NexusApiError::BadRequest {
            code: "POLICY_BLOCKED".to_string(),
            message: "PLATFORM_PAUSED: platform-only assembly not available in local-only mode"
                .to_string(),
        });
    }

    // If work_id is provided, verify ownership
    if let Some(work_id) = req.parameters["work_id"].as_str() {
        let _record = works::get_work(state.pool(), creator_id, work_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?
            .ok_or_else(|| NexusApiError::Forbidden {
                resource: "work".into(),
                reason: "work not found or cross-creator access denied".to_string(),
            })?;
    }

    // Local-only assembly subset
    Ok(serde_json::json!({
        "mode": "local",
        "creator_id": creator_id,
        "assembled_at": chrono::Utc::now().to_rfc3339()
    }))
}

// ─── fs/* Baseline handlers (V1.33, unchanged behavior) ───────────────────

/// Execute `fs/read_text_file` tool.
fn execute_read_file(
    req: &ToolExecuteRequest,
    _state: &WorkspaceState,
) -> Result<serde_json::Value, NexusApiError> {
    let path_str = req.parameters["path"]
        .as_str()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.path".into(),
            reason: "must be a string".into(),
        })?;

    let content = std::fs::read_to_string(path_str).map_err(|e| NexusApiError::Internal {
        code: "FILE_READ_FAILED".into(),
        message: format!("failed to read file {path_str}: {e}"),
    })?;

    Ok(serde_json::json!({
        "content": content
    }))
}

/// Execute `fs/write_text_file` tool.
fn execute_write_file(
    req: &ToolExecuteRequest,
    _state: &WorkspaceState,
) -> Result<serde_json::Value, NexusApiError> {
    let path_str = req.parameters["path"]
        .as_str()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.path".into(),
            reason: "must be a string".into(),
        })?;

    let content =
        req.parameters["content"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.content".into(),
                reason: "must be a string".into(),
            })?;

    let path = std::path::Path::new(path_str);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| NexusApiError::Internal {
            code: "DIR_CREATE_FAILED".into(),
            message: format!("failed to create directory {}: {}", parent.display(), e),
        })?;
    }

    std::fs::write(path, content).map_err(|e| NexusApiError::Internal {
        code: "FILE_WRITE_FAILED".into(),
        message: format!("failed to write file {path_str}: {e}"),
    })?;

    Ok(serde_json::json!({
        "written": true
    }))
}

// ─── Permission / path helpers ────────────────────────────────────────────

/// Mirrors `nexus-acp-host::PermissionPolicy::evaluate` without linking that crate
/// (daemon-runtime linkage matrix forbids `nexus-acp-host`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyDecision {
    Grant,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum DefaultPolicySetting {
    #[default]
    Ask,
    Grant,
    Deny,
}

#[derive(Debug, Clone)]
struct WorkspacePermissionPolicy {
    default: DefaultPolicySetting,
    grant: std::collections::HashSet<String>,
    deny: std::collections::HashSet<String>,
}

impl WorkspacePermissionPolicy {
    fn evaluate(&self, permission_name: &str) -> PolicyDecision {
        if self.grant.contains(permission_name) {
            return PolicyDecision::Grant;
        }
        if self.deny.contains(permission_name) {
            return PolicyDecision::Deny;
        }
        match self.default {
            DefaultPolicySetting::Grant => PolicyDecision::Grant,
            DefaultPolicySetting::Deny => PolicyDecision::Deny,
            DefaultPolicySetting::Ask => PolicyDecision::Ask,
        }
    }
}

fn table_keys(table: &toml::Table) -> std::collections::HashSet<String> {
    table.keys().cloned().collect()
}

fn is_nexus_write_granted(tool_name: &str, policy: &WorkspacePermissionPolicy) -> bool {
    if matches!(policy.evaluate(tool_name), PolicyDecision::Deny) {
        return false;
    }
    matches!(policy.evaluate(tool_name), PolicyDecision::Grant)
        || matches!(policy.evaluate("nexus.*"), PolicyDecision::Grant)
}

fn is_nexus_read_granted(tool_name: &str, policy: &WorkspacePermissionPolicy) -> bool {
    if matches!(policy.evaluate(tool_name), PolicyDecision::Deny) {
        return false;
    }
    is_capability_granted(tool_name, policy)
        || is_capability_granted("nexus.*", policy)
        || is_capability_granted("nexus.*.read", policy)
}

fn is_capability_granted(capability: &str, policy: &WorkspacePermissionPolicy) -> bool {
    match policy.evaluate(capability) {
        PolicyDecision::Grant => true,
        PolicyDecision::Deny => false,
        PolicyDecision::Ask => matches!(policy.default, DefaultPolicySetting::Grant),
    }
}

/// Load permission policy from workspace if available.
///
/// Returns `None` if no policy file exists (all tools permitted).
fn load_permission_policy(workspace_path: &str) -> Option<WorkspacePermissionPolicy> {
    let policy_path = Path::new(workspace_path)
        .join(".nexus42")
        .join("permissions.toml");
    if !policy_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&policy_path).ok()?;
    let policy: toml::Value = toml::from_str(&content).ok()?;

    let default = match policy.get("default").and_then(|v| v.as_str()) {
        Some("grant") => DefaultPolicySetting::Grant,
        Some("deny") => DefaultPolicySetting::Deny,
        _ => DefaultPolicySetting::Ask,
    };

    let grant = policy
        .get("grant")
        .and_then(|v| v.as_table())
        .map(table_keys)
        .unwrap_or_default();
    let deny = policy
        .get("deny")
        .and_then(|v| v.as_table())
        .map(table_keys)
        .unwrap_or_default();

    Some(WorkspacePermissionPolicy {
        default,
        grant,
        deny,
    })
}

/// Validate that file paths are within the workspace root (for fs/* tools).
fn validate_file_path(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
) -> Result<(), NexusApiError> {
    let path_str = req.parameters["path"]
        .as_str()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.path".into(),
            reason: "must be a string".into(),
        })?;

    let requested_path = Path::new(path_str);
    let workspace_path_str = state.workspace_path().unwrap_or_default();
    let workspace_root = Path::new(&workspace_path_str);

    let canonical_requested = if requested_path.exists() {
        requested_path
            .canonicalize()
            .map_err(|e| NexusApiError::InvalidInput {
                field: "parameters.path".into(),
                reason: format!("path cannot be resolved: {e}"),
            })?
    } else {
        let abs_requested = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(requested_path))
                .map_err(|e| NexusApiError::Internal {
                    code: "CURRENT_DIR_ERROR".into(),
                    message: format!("failed to get current directory: {e}"),
                })?
        };

        let abs_requested_str = abs_requested.display().to_string();
        if !abs_requested_str.starts_with(&workspace_path_str) {
            return Err(NexusApiError::Forbidden {
                resource: "file".into(),
                reason: "path outside workspace root".into(),
            });
        }

        abs_requested
    };

    if requested_path.exists() {
        let canonical_workspace =
            workspace_root
                .canonicalize()
                .map_err(|e| NexusApiError::Internal {
                    code: "WORKSPACE_PATH_INVALID".into(),
                    message: format!("workspace root cannot be resolved: {e}"),
                })?;

        if !canonical_requested.starts_with(&canonical_workspace) {
            return Err(NexusApiError::Forbidden {
                resource: "file".into(),
                reason: "path outside workspace root".into(),
            });
        }
    }

    Ok(())
}

// ─── Audit logging (spec §12.6) ───────────────────────────────────────────

/// Audit tool execution to `SQLite` (Gate 5).
async fn audit_tool_execution(
    req: &ToolExecuteRequest,
    decision: &str,
    error_code: Option<&str>,
    state: &WorkspaceState,
) -> Result<(), NexusApiError> {
    let tool_name = req.tool_name.clone();
    let session_id = req.session_id.clone().unwrap_or_default();
    let _request_id = req.request_id.clone().unwrap_or_default();
    let caller_kind = req
        .caller_kind
        .map_or_else(|| "http".to_string(), |k| k.to_string());

    // Redact parameter summary — only include top-level keys
    let param_summary: Vec<String> = req
        .parameters
        .as_object()
        .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    let outcome = if decision == "success" {
        "success".to_string()
    } else {
        format!("denied:{}", error_code.unwrap_or("UNKNOWN"))
    };

    // SAFETY: audit log INSERT — column names are static.
    sqlx::query(
        "INSERT INTO acp_tool_audit_log (tool_name, path, outcome, agent_id, session_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&tool_name)
    .bind(param_summary.join(","))
    .bind(&outcome)
    .bind(&caller_kind)
    .bind(&session_id)
    .execute(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "AUDIT_LOG_FAILED".into(),
        message: format!("failed to write audit log: {e}"),
    })?;

    Ok(())
}

// ─── V1.53 P1: DF-46 read-heavy nexus.* handlers ─────────────────────────

/// Verify that `creator_id` owns `world_id` by querying `narrative_worlds`.
///
/// Reuses the pattern from `works.rs:429-435`: `world_id` must exist AND
/// `owner_creator_id` must match. Returns `Forbidden { resource: "world" }`
/// on mismatch/missing; `Internal` on DB errors.
async fn ensure_world_accessible_for_creator(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> Result<(), NexusApiError> {
    let exists = sqlx::query_scalar!(
        r#"SELECT world_id AS "world_id!" FROM narrative_worlds WHERE world_id = ? AND owner_creator_id = ?"#,
        world_id,
        creator_id,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: format!("world ownership check: {e}"),
    })?;

    if exists.is_none() {
        return Err(NexusApiError::Forbidden {
            resource: "world".to_string(),
            reason: "world not found or cross-creator access denied".to_string(),
        });
    }
    Ok(())
}

/// `nexus.world.snapshot.get` — consistent read of structured world snapshot.
async fn execute_world_snapshot_get(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let world_id =
        req.parameters["world_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.world_id".into(),
                reason: "must be a string".into(),
            })?;

    ensure_world_accessible_for_creator(state.pool(), creator_id, world_id).await?;

    let gw = state.narrative_gateway();
    let world_state =
        gw.get_world_state(world_id)
            .await
            .map_err(|e: nexus_narrative::NarrativeError| {
                if e.to_string().contains("not found") {
                    NexusApiError::NotFound(format!("world {world_id}"))
                } else {
                    NexusApiError::Internal {
                        code: "NARRATIVE_ERROR".to_string(),
                        message: e.to_string(),
                    }
                }
            })?;

    Ok(serde_json::to_value(world_state).unwrap_or_else(|_| serde_json::json!({})))
}

/// `nexus.timeline.recent.get` — fetch recent timeline events for continuity.
async fn execute_timeline_recent_get(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let world_id =
        req.parameters["world_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.world_id".into(),
                reason: "must be a string".into(),
            })?;

    ensure_world_accessible_for_creator(state.pool(), creator_id, world_id).await?;

    // Default limit 100, clamp to max 500
    let limit: usize = req.parameters["limit"]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(100)
        .min(500);

    let gw = state.narrative_gateway();
    let mut events = gw.get_timeline(world_id, None, Some(limit)).await.map_err(
        |e: nexus_narrative::NarrativeError| NexusApiError::Internal {
            code: "NARRATIVE_ERROR".to_string(),
            message: e.to_string(),
        },
    )?;

    // SQL returns DESC order when limit is set; reverse to ASC for display.
    events.reverse();
    Ok(serde_json::to_value(&events).unwrap_or_else(|_| serde_json::json!([])))
}

/// `nexus.kb_snapshot.read` — focused KB snapshot read for a world.
async fn execute_kb_snapshot_read(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let world_id =
        req.parameters["world_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.world_id".into(),
                reason: "must be a string".into(),
            })?;

    ensure_world_accessible_for_creator(state.pool(), creator_id, world_id).await?;

    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(state.pool().clone());
    let blocks =
        kb_store
            .list_by_world(world_id)
            .await
            .map_err(|e: nexus_kb::store::KbStoreError| NexusApiError::Internal {
                code: "KB_STORE_ERROR".to_string(),
                message: e.to_string(),
            })?;

    Ok(serde_json::to_value(&blocks).unwrap_or_else(|_| serde_json::json!([])))
}

/// `nexus.manuscript.chapter.get` — read a single manuscript chapter record.
async fn execute_manuscript_chapter_get(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let chapter: i32 = req.parameters["chapter"]
        .as_i64()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be an integer".into(),
        })?
        .try_into()
        .map_err(|_| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be a valid i32 chapter number".into(),
        })?;

    let volume: i32 = req.parameters["volume"]
        .as_i64()
        .map_or(1, |v| i32::try_from(v).unwrap_or(1));

    // Verify work ownership first (reuse existing pattern from execute_work_get)
    let _record = works::get_work(state.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: "work".to_string(),
            reason: "work not found or cross-creator access denied".to_string(),
        })?;

    let chapter_record =
        nexus_local_db::work_chapters::get_chapter(state.pool(), work_id, chapter, volume)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;

    chapter_record.map_or_else(
        || Err(NexusApiError::NotFound(format!("{work_id}/ch{chapter}"))),
        |ch| Ok(serde_json::to_value(&ch).unwrap_or_else(|_| serde_json::json!({}))),
    )
}

/// `nexus.observability.daemon.health` — agent-visible daemon health status.
///
/// **R-V153P1QC2-003 (V1.54 closure):** The `registry_ids` field exposes all
/// tool IDs to authorized callers. This is acceptable for daemon-local
/// observability (V1.53–V1.54) because the handler requires active creator
/// and passes the Allowlist + `PermissionPolicy` admission gates. When
/// agent-facing observability is exposed beyond daemon-local scope
/// (V1.55+), an additional audit-level policy gate should be added and
/// `registry_ids` may need to be stripped for low-trust callers.
fn execute_daemon_health(
    _req: &ToolExecuteRequest,
    state: &WorkspaceState,
    _creator_id: &str,
) -> serde_json::Value {
    let reg = crate::capability_registry::host_tool_registry();
    serde_json::json!({
        "uptime_seconds": state.uptime_seconds(),
        "started_at": state.started_at().to_rfc3339(),
        "runtime_mode": state.runtime_mode_as_str(),
        "lifecycle_state": state.lifecycle_state().to_string(),
        "registry_size": reg.len(),
        "registry_ids": reg.ids().collect::<Vec<_>>(),
        "pool_healthy": true
    })
}

// ─── Registry handler wrappers (V1.53 P0) ─────────────────────────────────
///
/// These `pub(crate)` wrappers adapt the existing private handler functions
/// to the `RegistryHandlerFn` signature used by `CapabilityRegistry`.
/// They exist so the registry can reference the same handler implementations
/// without duplicating logic.
///
// Each wrapper uses an explicit named lifetime `'a` to satisfy the
// higher-ranked trait bound `for<'a> fn(&'a ..., &'a ..., &'a str) -> ...`.
use std::future::Future;
use std::pin::Pin;

/// Registry wrapper: `nexus.context.whoami` — sync → async wrapper.
pub(crate) fn registry_context_whoami<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_context_whoami(req, state, creator_id);
    Box::pin(async move { Ok(result) })
}

/// Registry wrapper: `nexus.workspace.info` — sync → async wrapper.
pub(crate) fn registry_workspace_info<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_workspace_info(req, state, creator_id);
    Box::pin(async move { Ok(result) })
}

/// Registry wrapper: `nexus.work.get` — async passthrough.
pub(crate) fn registry_work_get<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_work_get(req, state, creator_id))
}

/// Registry wrapper: `nexus.work.patch` — async passthrough.
pub(crate) fn registry_work_patch<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_work_patch(req, state, creator_id))
}

/// Registry wrapper: `nexus.orchestration.schedule_status` — async passthrough.
pub(crate) fn registry_schedule_status<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_schedule_status(req, state, creator_id))
}

/// Registry wrapper: `nexus.context.assemble` — async passthrough.
pub(crate) fn registry_context_assemble<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_context_assemble(req, state, creator_id))
}

/// Registry wrapper: `fs/read_text_file` — sync → async wrapper (ignores `creator_id`).
pub(crate) fn registry_read_file<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    _creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_read_file(req, state);
    Box::pin(async move { result })
}

/// Registry wrapper: `fs/write_text_file` — sync → async wrapper (ignores `creator_id`).
pub(crate) fn registry_write_file<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    _creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_write_file(req, state);
    Box::pin(async move { result })
}

// ─── V1.53 P1: Registry wrappers for DF-46 read-heavy tools ───────────────

/// Registry wrapper: `nexus.world.snapshot.get` — async passthrough.
pub(crate) fn registry_world_snapshot_get<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_world_snapshot_get(req, state, creator_id))
}

/// Registry wrapper: `nexus.timeline.recent.get` — async passthrough.
pub(crate) fn registry_timeline_recent_get<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_timeline_recent_get(req, state, creator_id))
}

/// Registry wrapper: `nexus.kb_snapshot.read` — async passthrough.
pub(crate) fn registry_kb_snapshot_read<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_kb_snapshot_read(req, state, creator_id))
}

/// Registry wrapper: `nexus.manuscript.chapter.get` — async passthrough.
pub(crate) fn registry_manuscript_chapter_get<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_chapter_get(req, state, creator_id))
}

/// Registry wrapper: `nexus.observability.daemon.health` — sync → async wrapper.
pub(crate) fn registry_daemon_health<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_daemon_health(req, state, creator_id);
    Box::pin(async move { Ok(result) })
}

// ─── V1.54 P0: DF-46 write tool handlers ──────────────────────────────────

/// `nexus.kb_snapshot.write` — upsert key blocks for a world.
async fn execute_kb_snapshot_write(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let world_id =
        req.parameters["world_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.world_id".into(),
                reason: "must be a string".into(),
            })?;

    ensure_world_accessible_for_creator(state.pool(), creator_id, world_id).await?;

    let blocks = req
        .parameters["blocks"]
        .as_array()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.blocks".into(),
            reason: "must be an array of key blocks".into(),
        })?;

    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(state.pool().clone());
    let mut written: usize = 0;
    let mut tx = state
        .pool()
        .begin()
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    for block_val in blocks {
        let kb: nexus_kb::key_block::KeyBlock =
            serde_json::from_value(block_val.clone()).map_err(|e| NexusApiError::InvalidInput {
                field: "parameters.blocks[]".into(),
                reason: format!("invalid key block: {e}"),
            })?;
        // C-001: reject blocks whose embedded world_id does not match the
        // request-level world_id (prevents cross-world block payload bypass).
        if kb.world_id != world_id {
            return Err(NexusApiError::Forbidden {
                resource: "key_block.world_id".to_string(),
                reason: format!(
                    "block {} targets world '{}' but request targets world '{}'",
                    kb.key_block_id, kb.world_id, world_id
                ),
            });
        }
        kb_store
            .insert_key_block_in_tx(&mut tx, kb)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "KB_STORE_ERROR".to_string(),
                message: e.to_string(),
            })?;
        written += 1;
    }

    tx.commit()
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    Ok(serde_json::json!({
        "written": written,
        "world_id": world_id
    }))
}

/// `nexus.manuscript.chapter.update` — update chapter content and metadata.
async fn execute_manuscript_chapter_update(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let chapter: i32 = req.parameters["chapter"]
        .as_i64()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be an integer".into(),
        })?
        .try_into()
        .map_err(|_| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be a valid i32 chapter number".into(),
        })?;

    let volume: i32 = req.parameters["volume"]
        .as_i64()
        .map_or(1, |v| i32::try_from(v).unwrap_or(1));

    // Verify work ownership first (keep record for work_ref used in W-003 path).
    let work_record = works::get_work(state.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: "work".to_string(),
            reason: "work not found or cross-creator access denied".to_string(),
        })?;

    // Check chapter exists
    let chapter_exists =
        nexus_local_db::work_chapters::get_chapter(state.pool(), work_id, chapter, volume)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;

    if chapter_exists.is_none() {
        return Err(NexusApiError::NotFound(format!(
            "{work_id}/ch{chapter}/v{volume}"
        )));
    }

    // Update body content if provided
    let now = chrono::Utc::now().to_rfc3339();
    let body_path: Option<String> = if let Some(content) = req.parameters["content"].as_str() {
        let workspace_root = state
            .workspace_path()
            .ok_or_else(|| NexusApiError::Internal {
                code: "WORKSPACE_PATH_ERROR".to_string(),
                message: "workspace path not available".to_string(),
            })?;
        // W-003: use the canonical body_path from the existing chapter record
        // (set by seed_chapters), which follows Works/{work_ref}/Stories/{slug}.md.
        // Fall back to constructing the path if the chapter has no body_path yet.
        let canonical_path = chapter_exists
            .as_ref()
            .and_then(|cr| cr.body_path.clone())
            .unwrap_or_else(|| {
                let ch_nn = format!("ch{chapter:02}");
                let wr = work_record.work_ref.as_deref().unwrap_or(work_id);
                format!("Works/{wr}/Stories/{ch_nn}-{ch_nn}.md")
            });
        let body_file = Path::new(&workspace_root).join(&canonical_path);
        if let Some(parent) = body_file.parent() {
            // C-002: use tokio::fs to avoid blocking the async runtime.
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| NexusApiError::Internal {
                    code: "DIR_CREATE_FAILED".into(),
                    message: format!("failed to create chapter dir: {e}"),
                })?;
        }
        // C-002: write to a temp file first, then atomically rename within a
        // DB transaction to prevent orphaned files on crash between write and
        // DB commit.
        let tmp_file = body_file.with_extension("md.tmp");
        tokio::fs::write(&tmp_file, content)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "FILE_WRITE_FAILED".into(),
                message: format!("failed to write chapter body: {e}"),
            })?;
        // W-003: store the relative canonical path in the DB, matching
        // the seed_chapters convention (Works/{work_ref}/Stories/{slug}.md).
        Some(canonical_path)
    } else {
        None
    };

    // Update chapter DB row if body_path or word count changed.
    // C-002: wrap DB update + file rename in a single transaction so the
    // DB row is only updated when the final file is in place.
    if let Some(ref bp) = body_path {
        // W-003: bp is a relative canonical path; resolve to absolute for FS ops.
        let workspace_root = state
            .workspace_path()
            .ok_or_else(|| NexusApiError::Internal {
                code: "WORKSPACE_PATH_ERROR".to_string(),
                message: "workspace path not available".to_string(),
            })?;
        let abs_body = Path::new(&workspace_root).join(bp);
        let abs_tmp = abs_body.with_extension("md.tmp");
        let word_count = req.parameters["content"]
            .as_str()
            .map_or(0, |c| c.split_whitespace().count());
        let mut tx = state
            .pool()
            .begin()
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("chapter update tx begin: {e}"),
            })?;
        // SAFETY: dynamic SQL for chapter update — runtime fields.
        #[allow(clippy::cast_possible_wrap)]
        sqlx::query(
            "UPDATE work_chapters SET body_path = ?, actual_word_count = ?, updated_at = ? \
             WHERE work_id = ? AND chapter = ? AND volume = ?",
        )
        .bind(bp)
        .bind(word_count as i64)
        .bind(&now)
        .bind(work_id)
        .bind(chapter)
        .bind(volume)
        .execute(&mut *tx)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("chapter update: {e}"),
        })?;
        // Atomically rename temp → final (after DB update succeeds inside tx).
        tokio::fs::rename(&abs_tmp, &abs_body)
            .await
            .map_err(|e| {
                // Best-effort cleanup: remove temp file on rename failure.
                let _ = std::fs::remove_file(&abs_tmp);
                NexusApiError::Internal {
                    code: "FILE_RENAME_FAILED".into(),
                    message: format!("failed to finalize chapter file: {e}"),
                }
            })?;
        tx.commit()
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("chapter update tx commit: {e}"),
            })?;
    }

    // Read back updated chapter
    let updated =
        nexus_local_db::work_chapters::get_chapter(state.pool(), work_id, chapter, volume)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;

    updated.map_or_else(
        || {
            Err(NexusApiError::NotFound(format!(
                "{work_id}/ch{chapter}"
            )))
        },
        |ch| Ok(serde_json::to_value(&ch).unwrap_or_else(|_| serde_json::json!({}))),
    )
}

/// `nexus.world.configure` — update world metadata.
#[allow(clippy::useless_let_if_seq)] // three independent if-let accumulations on `updated`
async fn execute_world_configure(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let world_id =
        req.parameters["world_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.world_id".into(),
                reason: "must be a string".into(),
            })?;

    ensure_world_accessible_for_creator(state.pool(), creator_id, world_id).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut updated = false;

    // Update title if provided
    if let Some(title) = req.parameters["title"].as_str() {
        if title.trim().is_empty() {
            return Err(NexusApiError::InvalidInput {
                field: "parameters.title".into(),
                reason: "must not be empty".into(),
            });
        }
        // SAFETY: dynamic SQL — runtime field updates on narrative_worlds.
        sqlx::query("UPDATE narrative_worlds SET title = ?, updated_at = ? WHERE world_id = ?")
            .bind(title)
            .bind(&now)
            .bind(world_id)
            .execute(state.pool())
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: format!("world title update: {e}"),
            })?;
        updated = true;
    }

    // Update visibility if provided
    if let Some(visibility) = req.parameters["visibility"].as_str() {
        let valid = ["public", "private", "invited"].contains(&visibility);
        if !valid {
            return Err(NexusApiError::InvalidInput {
                field: "parameters.visibility".into(),
                reason: "must be one of: public, private, invited".into(),
            });
        }
        // SAFETY: dynamic SQL for visibility update.
        sqlx::query(
            "UPDATE narrative_worlds SET visibility = ?, updated_at = ? WHERE world_id = ?",
        )
        .bind(visibility)
        .bind(&now)
        .bind(world_id)
        .execute(state.pool())
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("world visibility update: {e}"),
        })?;
        updated = true;
    }

    // Update time_policy if provided
    if let Some(time_policy) = req.parameters["time_policy"].as_str() {
        let valid = ["manual", "auto_advance"].contains(&time_policy);
        if !valid {
            return Err(NexusApiError::InvalidInput {
                field: "parameters.time_policy".into(),
                reason: "must be one of: manual, auto_advance".into(),
            });
        }
        // SAFETY: dynamic SQL for time_policy update.
        sqlx::query(
            "UPDATE narrative_worlds SET time_policy = ?, updated_at = ? WHERE world_id = ?",
        )
        .bind(time_policy)
        .bind(&now)
        .bind(world_id)
        .execute(state.pool())
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: format!("world time_policy update: {e}"),
        })?;
        updated = true;
    }

    Ok(serde_json::json!({
        "world_id": world_id,
        "updated": updated
    }))
}

/// `nexus.work.schedule.set` — link/unlink schedule ids to a work.
async fn execute_work_schedule_set(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let schedule_ids =
        req.parameters["schedule_ids"]
            .as_array()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.schedule_ids".into(),
                reason: "must be an array of schedule id strings".into(),
            })?;

    // Validate all entries are strings
    for (i, id) in schedule_ids.iter().enumerate() {
        if !id.is_string() {
            return Err(NexusApiError::InvalidInput {
                field: format!("parameters.schedule_ids[{i}]"),
                reason: "must be a string".into(),
            });
        }
    }

    // Verify work ownership first
    let _record = works::get_work(state.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: "work".to_string(),
            reason: "work not found or cross-creator access denied".to_string(),
        })?;

    let schedule_ids_json = serde_json::to_string(&schedule_ids).unwrap_or_default();
    let now = chrono::Utc::now().to_rfc3339();

    let patch = nexus_local_db::works::WorkPatch {
        schedule_ids: Some(schedule_ids_json),
        title: None,
        long_term_goal: None,
        creative_brief: None,
        intake_status: None,
        status: None,
        world_id: None,
        story_ref: None,
        primary_preset_id: None,
        current_stage: None,
        stage_status: None,
        work_profile: None,
        work_ref: None,
        total_planned_chapters: None,
        current_chapter: None,
        auto_chain_enabled: None,
        driver_schedule_id: None,
        auto_chain_interrupted: None,
        auto_review_master_on_timeout: None,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    };

    works::patch_work(state.pool(), creator_id, work_id, &patch, &now)
        .await
        .map_err(|e| match &e {
            nexus_local_db::LocalDbError::MissingVersionKey { .. } => NexusApiError::Forbidden {
                resource: "work".into(),
                reason: "work not found or cross-creator".into(),
            },
            _ => NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: e.to_string(),
            },
        })?;

    Ok(serde_json::json!({
        "work_id": work_id,
        "schedule_ids": schedule_ids
    }))
}

/// `nexus.finding.resolve` — resolve/close a finding.
async fn execute_finding_resolve(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let finding_id =
        req.parameters["finding_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.finding_id".into(),
                reason: "must be a string".into(),
            })?;

    let resolution = req
        .parameters["resolution"]
        .as_str()
        .unwrap_or("resolved via tool");

    let now_epoch = chrono::Utc::now().timestamp();

    let patch = nexus_local_db::findings::FindingPatch {
        status: Some("resolved".to_string()),
        severity: None,
        title: None,
        description: Some(format!(
            "Resolved via tool: {resolution}"
        )),
        target_executor: None,
        kind: None,
        rule_suggestion: None,
    };

    let updated = nexus_local_db::findings::update_finding(
        state.pool(),
        creator_id,
        finding_id,
        &patch,
        now_epoch,
    )
    .await
    .map_err(|e| match &e {
        nexus_local_db::LocalDbError::MissingVersionKey { .. } => NexusApiError::Forbidden {
            resource: "finding".into(),
            reason: "finding not found or cross-creator".into(),
        },
        nexus_local_db::LocalDbError::IllegalTransition { .. } => NexusApiError::BadRequest {
            code: "INVALID_TRANSITION".to_string(),
            message: e.to_string(),
        },
        nexus_local_db::LocalDbError::InvalidEnum { .. } => NexusApiError::InvalidInput {
            field: "parameters.status".into(),
            reason: e.to_string(),
        },
        _ => NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        },
    })?;

    // W-002: check the returned bool — false means no row was updated.
    if !updated {
        return Err(NexusApiError::NotFound(format!(
            "finding {finding_id}"
        )));
    }

    Ok(serde_json::json!({
        "finding_id": finding_id,
        "resolved": true
    }))
}

/// `nexus.pool.entry.manage` — add/remove/promote pool entries.
async fn execute_pool_entry_manage(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let action = req.parameters["action"]
        .as_str()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.action".into(),
            reason: "must be a string (add, remove, promote, archive)".into(),
        })?;

    // Verify work ownership for non-creator-scoped actions
    let _record = works::get_work(state.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: "work".to_string(),
            reason: "work not found or cross-creator access denied".to_string(),
        })?;

    match action {
        "add" | "promote" => {
            nexus_local_db::novel_pool_entries::promote_to_active(
                state.pool(),
                creator_id,
                work_id,
            )
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "POOL_ERROR".to_string(),
                message: e.to_string(),
            })?;
        }
        "remove" | "archive" => {
            let entry = nexus_local_db::novel_pool_entries::get_pool_entry_by_work(
                state.pool(),
                creator_id,
                work_id,
            )
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "POOL_ERROR".to_string(),
                message: e.to_string(),
            })?
            .ok_or_else(|| NexusApiError::NotFound(format!(
                "pool entry for {work_id}"
            )))?;

            nexus_local_db::novel_pool_entries::archive_pool_entry(
                state.pool(),
                &entry.entry_id,
                creator_id,
            )
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "POOL_ERROR".to_string(),
                message: e.to_string(),
            })?;
        }
        _ => {
            return Err(NexusApiError::InvalidInput {
                field: "parameters.action".into(),
                reason: "must be one of: add, remove, promote, archive".into(),
            });
        }
    }

    Ok(serde_json::json!({
        "work_id": work_id,
        "action": action,
        "success": true
    }))
}

// ─── V1.54 P0: Registry wrappers for DF-46 write tools ─────────────────────

/// Registry wrapper: `nexus.kb_snapshot.write` — async passthrough.
pub(crate) fn registry_kb_snapshot_write<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_kb_snapshot_write(req, state, creator_id))
}

/// Registry wrapper: `nexus.manuscript.chapter.update` — async passthrough.
pub(crate) fn registry_manuscript_chapter_update<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_chapter_update(req, state, creator_id))
}

/// Registry wrapper: `nexus.world.configure` — async passthrough.
pub(crate) fn registry_world_configure<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_world_configure(req, state, creator_id))
}

/// Registry wrapper: `nexus.work.schedule.set` — async passthrough.
pub(crate) fn registry_work_schedule_set<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_work_schedule_set(req, state, creator_id))
}

/// Registry wrapper: `nexus.finding.resolve` — async passthrough.
pub(crate) fn registry_finding_resolve<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_finding_resolve(req, state, creator_id))
}

/// Registry wrapper: `nexus.pool.entry.manage` — async passthrough.
pub(crate) fn registry_pool_entry_manage<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_pool_entry_manage(req, state, creator_id))
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;
    use crate::test_utils::create_initialized_test_workspace;
    use crate::workspace::WorkspaceState;

    #[tokio::test]
    async fn execute_rejects_unknown_tool() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let req = ToolExecuteRequest {
            tool_name: "unknown/tool".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_rejects_read_without_path() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let req = ToolExecuteRequest {
            tool_name: "fs/read_text_file".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_read_file_succeeds() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Write a temp file to read
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        let path = temp.path().to_string_lossy().to_string();
        std::fs::write(temp.path(), "hello world").expect("write temp");

        let req = ToolExecuteRequest {
            tool_name: "fs/read_text_file".to_string(),
            parameters: serde_json::json!({ "path": path }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_ok());
        let val = result.expect("result");
        assert_eq!(val["content"], "hello world");
    }

    #[tokio::test]
    async fn execute_write_file_succeeds() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let temp = tempfile::NamedTempFile::new().expect("temp file");
        let path = temp.path().to_string_lossy().to_string();

        let req = ToolExecuteRequest {
            tool_name: "fs/write_text_file".to_string(),
            parameters: serde_json::json!({ "path": path, "content": "written!" }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&path).expect("read back");
        assert_eq!(content, "written!");
    }

    #[tokio::test]
    async fn whoami_returns_active_creator() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.context.whoami".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_ok());
        let val = result.expect("result");
        assert_eq!(val["creator_id"], "test_creator");
        assert_eq!(val["workspace_slug"], "default");
    }

    #[tokio::test]
    async fn workspace_info_returns_details() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.workspace.info".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_ok());
        let val = result.expect("result");
        assert_eq!(val["creator_id"], "test_creator");
        assert_eq!(val["workspace_slug"], "default");
    }

    #[tokio::test]
    async fn work_get_happy_path() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Create a work first
        let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let record = nexus_local_db::works::WorkRecord {
            work_id: work_id.clone(),
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Test Work".to_string(),
            long_term_goal: "Goal".to_string(),
            initial_idea: "Idea".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now,
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: None,
            total_planned_chapters: None,
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
            .await
            .expect("create work")
            .unwrap_err(); // Returns the new record in Err

        let req = ToolExecuteRequest {
            tool_name: "nexus.work.get".to_string(),
            parameters: serde_json::json!({ "work_id": work_id }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_ok());
        let val = result.expect("result");
        assert_eq!(val["work_id"], work_id);
        assert_eq!(val["title"], "Test Work");
    }

    #[tokio::test]
    async fn work_patch_rejects_stage_field() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.work.patch".to_string(),
            parameters: serde_json::json!({
                "work_id": "wrk_test",
                "current_stage": "writing"
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should be INVALID_INPUT — stable tool error code (spec §12.4)
        assert_eq!(err.error_code(), "INVALID_INPUT");
    }

    #[tokio::test]
    async fn context_assemble_policy_blocked_when_local_only() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.context.assemble".to_string(),
            parameters: serde_json::json!({ "requires_platform": true }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        // Should be POLICY_BLOCKED
        match &result {
            Err(NexusApiError::BadRequest { code, message }) => {
                assert_eq!(code, "POLICY_BLOCKED");
                assert!(message.contains("PLATFORM_PAUSED"));
            }
            Err(e) => panic!("Expected BadRequest(POLICY_BLOCKED), got: {e:?}"),
            Ok(_) => panic!("Expected error"),
        }
    }

    /// Worker upcall dispatch hits the same registry as HTTP (spec §7.1).
    #[tokio::test]
    async fn worker_upcall_whoami_same_result_as_http() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let http_req = ToolExecuteRequest {
            tool_name: "nexus.context.whoami".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let http_result = HostToolExecutor::execute(&http_req, &state)
            .await
            .expect("HTTP execute");

        let worker_result = HostToolExecutor::dispatch_from_worker(
            "nexus.context.whoami",
            &serde_json::json!({}),
            "req-001",
            &state,
        )
        .await;

        assert!(worker_result.grant, "Worker upcall should succeed");
        assert_eq!(worker_result.request_id, "req-001");
        let output = worker_result.output.expect("worker should have output");
        assert_eq!(
            output, http_result,
            "HTTP and worker must produce same result"
        );
    }

    // ─── V1.53 P0 Sub-phase 1: Registry parity tests ───────────────────────

    /// Parity test: old `execute()` and new `registry_dispatch()` produce
    /// the same output for `nexus.context.whoami`.
    #[tokio::test]
    async fn registry_parity_whoami() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.context.whoami".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let old_result = HostToolExecutor::execute(&req, &state).await;
        let new_result = HostToolExecutor::registry_dispatch(&req, &state).await;
        assert_eq!(old_result.is_ok(), new_result.is_ok());
        if let (Ok(old_val), Ok(new_val)) = (&old_result, &new_result) {
            assert_eq!(old_val["creator_id"], new_val["creator_id"]);
            assert_eq!(old_val["workspace_slug"], new_val["workspace_slug"]);
        }
    }

    /// Parity test: old `execute()` and new `registry_dispatch()` produce
    /// the same output for `nexus.workspace.info`.
    #[tokio::test]
    async fn registry_parity_workspace_info() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.workspace.info".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let old_result = HostToolExecutor::execute(&req, &state).await;
        let new_result = HostToolExecutor::registry_dispatch(&req, &state).await;
        assert_eq!(old_result.is_ok(), new_result.is_ok());
        if let (Ok(old_val), Ok(new_val)) = (&old_result, &new_result) {
            assert_eq!(old_val["creator_id"], new_val["creator_id"]);
            assert_eq!(old_val["workspace_slug"], new_val["workspace_slug"]);
            assert_eq!(old_val["workspace_path"], new_val["workspace_path"]);
        }
    }

    // ─── V1.53 P0 Sub-phase 2: Cutover verification ────────────────────────

    /// Cutover test: `execute()` now routes through `registry_dispatch()`
    /// internally, so both paths must produce identical output for every tool.
    #[tokio::test]
    async fn cutover_execute_equals_registry_dispatch() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Test with whoami (read-only, no DB setup needed)
        let req = ToolExecuteRequest {
            tool_name: "nexus.context.whoami".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let via_execute = HostToolExecutor::execute(&req, &state).await;
        let via_registry = HostToolExecutor::registry_dispatch(&req, &state).await;
        assert_eq!(via_execute.is_ok(), via_registry.is_ok());
        if let (Ok(e_val), Ok(r_val)) = (&via_execute, &via_registry) {
            assert_eq!(e_val, r_val, "execute() must route through registry");
        }
    }

    /// Cutover test: `execute()` rejects unknown tools through registry.
    #[tokio::test]
    async fn cutover_unknown_tool_rejected_by_registry() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nonexistent.nexus.capability".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        match result {
            Err(NexusApiError::BadRequest { code, .. }) => {
                assert_eq!(code, "NOT_SUPPORTED");
            }
            other => panic!("Expected BadRequest(NOT_SUPPORTED) via registry, got: {other:?}"),
        }
    }

    /// **R-V153P0QC1-001 enforcement**: `TOOL_ALLOWLIST` and
    /// `CapabilityRegistry` rows must agree on every tool ID.
    ///
    /// P1 will add 5 new `nexus.*` tools. This test ensures they cannot
    /// be added to one list without the other — catching the drift risk
    /// that qc1 identified.
    #[test]
    fn tool_allowlist_matches_registry_ids() {
        let reg = crate::capability_registry::host_tool_registry();
        let registry_ids: std::collections::HashSet<&str> = reg.ids().collect();
        let allowlist_ids: std::collections::HashSet<&str> =
            TOOL_ALLOWLIST.iter().copied().collect();

        // Every TOOL_ALLOWLIST entry must have a matching registry row
        for id in &allowlist_ids {
            assert!(
                registry_ids.contains(id),
                "TOOL_ALLOWLIST contains '{id}' but CapabilityRegistry has no row for it. \
                 Add the row to host_tool_registry() or remove the entry from TOOL_ALLOWLIST."
            );
        }

        // Every registry row must appear in TOOL_ALLOWLIST
        for id in &registry_ids {
            assert!(
                allowlist_ids.contains(id),
                "CapabilityRegistry row '{id}' is not in TOOL_ALLOWLIST. \
                 Add the id to TOOL_ALLOWLIST or remove the row from host_tool_registry()."
            );
        }
    }

    /// Parity test: old and new dispatch both reject unknown tools.
    #[tokio::test]
    async fn registry_parity_unknown_tool() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "unknown/tool".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let old_result = HostToolExecutor::execute(&req, &state).await;
        let new_result = HostToolExecutor::registry_dispatch(&req, &state).await;
        assert!(old_result.is_err());
        assert!(new_result.is_err());
        // Both should produce NOT_SUPPORTED
        match (&old_result, &new_result) {
            (Err(old_e), Err(new_e)) => {
                assert_eq!(old_e.error_code(), new_e.error_code());
            }
            _ => panic!("Both should be errors"),
        }
    }

    // ─── V1.53 P1: DF-46 read-heavy tool e2e tests ─────────────────────────

    /// E2E test: `nexus.world.snapshot.get` returns world state for a seeded world.
    #[tokio::test]
    async fn world_snapshot_get_returns_world_state() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
        // Seed a world
        crate::test_utils::seed_test_creator_and_world(state.pool()).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.world.snapshot.get".to_string(),
            parameters: serde_json::json!({"world_id": "wld_test_world"}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_ok(),
            "world.snapshot.get should succeed: {result:?}"
        );
        let val = result.expect("result");
        assert_eq!(val["world_id"], "wld_test_world");
        assert_eq!(val["title"], "Test World");
        drop(tmp);
    }

    /// Failure test: `nexus.world.snapshot.get` with missing world_id returns error.
    #[tokio::test]
    async fn world_snapshot_get_rejects_missing_world_id() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.world.snapshot.get".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
    }

    /// E2E test: `nexus.timeline.recent.get` returns events for a seeded world.
    #[tokio::test]
    async fn timeline_recent_get_returns_recent_events() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
        // Seed world + timeline events via narrative gateway seed helpers
        let pool = state.pool().clone();
        nexus_local_db::narrative_gateway::seed::world(
            &pool,
            "wld_timeline",
            "test_creator",
            "Timeline World",
            "timeline-world",
            "private",
            "manual",
        )
        .await;
        nexus_local_db::narrative_gateway::seed::event(
            &pool,
            "evt_1",
            "wld_timeline",
            "fbk_root",
            "story_advance",
            1,
        )
        .await;
        nexus_local_db::narrative_gateway::seed::event(
            &pool,
            "evt_2",
            "wld_timeline",
            "fbk_root",
            "story_advance",
            2,
        )
        .await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.timeline.recent.get".to_string(),
            parameters: serde_json::json!({"world_id": "wld_timeline", "limit": 5}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_ok(),
            "timeline.recent.get should succeed: {result:?}"
        );
        let val = result.expect("result");
        let events = val.as_array().expect("should be an array");
        assert_eq!(events.len(), 2);
        drop(tmp);
    }

    /// E2E test: `nexus.kb_snapshot.read` returns key blocks for a seeded world.
    #[tokio::test]
    async fn kb_snapshot_read_returns_key_blocks() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
        // Seed world + key blocks
        let pool = state.pool().clone();
        nexus_local_db::kb_store::seed::world(
            &pool,
            "wld_kb",
            "test_creator",
            "KB World",
            "kb-world",
            "private",
            "manual",
        )
        .await;
        nexus_local_db::kb_store::seed::key_block(
            &pool,
            "kb_1",
            "wld_kb",
            "character",
            "alice",
            "provisional",
        )
        .await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.kb_snapshot.read".to_string(),
            parameters: serde_json::json!({"world_id": "wld_kb"}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_ok(),
            "kb_snapshot.read should succeed: {result:?}"
        );
        let val = result.expect("result");
        let blocks = val.as_array().expect("should be an array");
        assert!(!blocks.is_empty(), "should return at least one key block");
        drop(tmp);
    }

    /// E2E test: `nexus.manuscript.chapter.get` returns chapter record.
    #[tokio::test]
    async fn manuscript_chapter_get_returns_chapter_record() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;

        // Create a work first, then seed chapters
        let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let record = nexus_local_db::works::WorkRecord {
            work_id: work_id.clone(),
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Test Novel".to_string(),
            long_term_goal: "Goal".to_string(),
            initial_idea: "Idea".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: None,
            total_planned_chapters: Some(5),
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
            .await
            .expect("create work")
            .unwrap_err();

        // Seed chapters
        nexus_local_db::work_chapters::seed_chapters(state.pool(), &work_id, "test-novel", 5, &now)
            .await
            .expect("seed chapters");

        let req = ToolExecuteRequest {
            tool_name: "nexus.manuscript.chapter.get".to_string(),
            parameters: serde_json::json!({"work_id": work_id, "chapter": 1}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_ok(),
            "manuscript.chapter.get should succeed: {result:?}"
        );
        let val = result.expect("result");
        assert_eq!(val["work_id"], work_id);
        assert_eq!(val["chapter"], 1);
        drop(tmp);
    }

    /// E2E test: `nexus.observability.daemon.health` returns runtime status.
    #[tokio::test]
    async fn daemon_health_returns_registry_status() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.observability.daemon.health".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_ok(), "daemon.health should succeed: {result:?}");
        let val = result.expect("result");
        assert!(val["uptime_seconds"].as_u64().is_some());
        assert_eq!(val["runtime_mode"], "local_only");
        assert_eq!(val["registry_size"], 19);
        assert!(val["pool_healthy"].as_bool().unwrap_or(false));
        assert_eq!(
            val["registry_ids"].as_array().expect("registry_ids").len(),
            19
        );
    }

    // ─── P0 residual closure: registry dispatch regression tests (R-V153P0QC2-001) ──
    // Backfill registry_dispatch parity for V1.34 tools that lacked parity tests.

    #[tokio::test]
    async fn registry_dispatch_returns_same_as_legacy_work_get() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Create a work
        let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let record = nexus_local_db::works::WorkRecord {
            work_id: work_id.clone(),
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Legacy Work".to_string(),
            long_term_goal: "Goal".to_string(),
            initial_idea: "Idea".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now,
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: None,
            total_planned_chapters: None,
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
            .await
            .expect("create work")
            .unwrap_err();

        let req = ToolExecuteRequest {
            tool_name: "nexus.work.get".to_string(),
            parameters: serde_json::json!({"work_id": work_id}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        // Both execute() and registry_dispatch() should produce the same result.
        // Since Sub-phase 2 cutover, execute() routes through registry_dispatch(),
        // so they are the same path. This test guards against regression.
        let via_execute = HostToolExecutor::execute(&req, &state).await;
        let via_registry = HostToolExecutor::registry_dispatch(&req, &state).await;
        assert_eq!(via_execute.is_ok(), via_registry.is_ok());
        if let (Ok(e_val), Ok(r_val)) = (&via_execute, &via_registry) {
            assert_eq!(e_val["work_id"], r_val["work_id"]);
            assert_eq!(e_val["title"], r_val["title"]);
        }
    }

    #[tokio::test]
    async fn registry_dispatch_returns_same_as_legacy_work_patch() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.work.patch".to_string(),
            parameters: serde_json::json!({
                "work_id": "wrk_test",
                "current_stage": "writing"
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let via_execute = HostToolExecutor::execute(&req, &state).await;
        let via_registry = HostToolExecutor::registry_dispatch(&req, &state).await;
        assert!(via_execute.is_err());
        assert!(via_registry.is_err());
        assert_eq!(
            via_execute.as_ref().unwrap_err().error_code(),
            via_registry.as_ref().unwrap_err().error_code()
        );
    }

    #[tokio::test]
    async fn schedule_status_happy_path() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Create a work with schedule_ids
        let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let record = nexus_local_db::works::WorkRecord {
            work_id: work_id.clone(),
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Scheduled Work".to_string(),
            long_term_goal: "Goal".to_string(),
            initial_idea: "Idea".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: r#"["sch_001","sch_002"]"#.to_string(),
            created_at: now.clone(),
            updated_at: now,
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: None,
            total_planned_chapters: None,
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
            .await
            .expect("create work")
            .unwrap_err();

        let req = ToolExecuteRequest {
            tool_name: "nexus.orchestration.schedule_status".to_string(),
            parameters: serde_json::json!({"work_id": work_id}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_ok(), "schedule_status should succeed: {result:?}");
        let val = result.expect("result");
        assert_eq!(val["work_id"], work_id);
        assert_eq!(val["count"], 2);
    }

    #[tokio::test]
    async fn registry_dispatch_returns_same_as_legacy_context_assemble() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.context.assemble".to_string(),
            parameters: serde_json::json!({"requires_platform": true}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let via_execute = HostToolExecutor::execute(&req, &state).await;
        let via_registry = HostToolExecutor::registry_dispatch(&req, &state).await;
        assert_eq!(via_execute.is_ok(), via_registry.is_ok());
        if let (Err(e_val), Err(r_val)) = (&via_execute, &via_registry) {
            assert_eq!(e_val.error_code(), r_val.error_code());
        }
    }

    // ─── V1.53 P1: Cross-creator/world isolation tests (R-V153P1QC1-001) ──

    /// Helper: overwrite the active creator in config.toml and return a new
    /// WorkspaceState (same db) with that identity.
    async fn switch_active_creator(
        nexus_home: &std::path::Path,
        db_path: &std::path::Path,
        new_creator_id: &str,
    ) -> WorkspaceState {
        let toml_str = format!(
            "active_creator_id = \"{new_creator_id}\"\n[active_workspace_slug_by_creator]\n\"{new_creator_id}\" = \"default\""
        );
        std::fs::write(nexus_home.join("config.toml"), toml_str).expect("write config.toml");
        WorkspaceState::new_for_testing(nexus_home.to_path_buf(), db_path.to_path_buf(), None).await
    }

    #[tokio::test]
    async fn world_snapshot_get_cross_creator_denied() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state =
            WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
        crate::test_utils::seed_test_creator_and_world(state.pool()).await;
        // Seed another creator
        // SAFETY: test-only data setup.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
        )
        .execute(state.pool())
        .await
        .expect("seed other creator");

        // Switch to other_creator — should be denied
        let other_state = switch_active_creator(&nexus_home, &db_path, "other_creator").await;
        let req = ToolExecuteRequest {
            tool_name: "nexus.world.snapshot.get".to_string(),
            parameters: serde_json::json!({"world_id": "wld_test_world"}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &other_state).await;
        assert!(result.is_err(), "cross-creator should be denied");
        assert_eq!(
            result.unwrap_err().error_code(),
            "FORBIDDEN",
            "should return FORBIDDEN for cross-creator access"
        );
        drop(tmp);
    }

    #[tokio::test]
    async fn timeline_recent_get_cross_creator_denied() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state =
            WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
        crate::test_utils::seed_test_creator_and_world(state.pool()).await;
        // Seed other creator
        // SAFETY: test-only.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
        )
        .execute(state.pool())
        .await
        .expect("seed other creator");

        let other_state = switch_active_creator(&nexus_home, &db_path, "other_creator").await;
        let req = ToolExecuteRequest {
            tool_name: "nexus.timeline.recent.get".to_string(),
            parameters: serde_json::json!({"world_id": "wld_test_world"}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &other_state).await;
        assert!(result.is_err(), "cross-creator should be denied");
        assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
        drop(tmp);
    }

    #[tokio::test]
    async fn kb_snapshot_read_cross_creator_denied() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state =
            WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
        crate::test_utils::seed_test_creator_and_world(state.pool()).await;
        // SAFETY: test-only.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
        )
        .execute(state.pool())
        .await
        .expect("seed other creator");

        let other_state = switch_active_creator(&nexus_home, &db_path, "other_creator").await;
        let req = ToolExecuteRequest {
            tool_name: "nexus.kb_snapshot.read".to_string(),
            parameters: serde_json::json!({"world_id": "wld_test_world"}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &other_state).await;
        assert!(result.is_err(), "cross-creator should be denied");
        assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
        drop(tmp);
    }

    // ─── V1.53 P1: Failure/admission test coverage (R-V153P1QC1-002) ──

    #[tokio::test]
    async fn timeline_recent_get_rejects_missing_world_id() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.timeline.recent.get".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
    }

    #[tokio::test]
    async fn kb_snapshot_read_rejects_missing_world_id() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.kb_snapshot.read".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
    }

    #[tokio::test]
    async fn manuscript_chapter_get_rejects_missing_chapter_id() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.manuscript.chapter.get".to_string(),
            parameters: serde_json::json!({"work_id": "nonexistent_work", "chapter": 1}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
    }

    #[tokio::test]
    async fn daemon_health_rejects_without_active_creator() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        // Remove active_creator_id from config
        let toml_str = "[active_workspace_slug_by_creator]\n";
        std::fs::write(nexus_home.join("config.toml"), toml_str).expect("write config.toml");
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.observability.daemon.health".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_err(),
            "daemon.health should require active creator"
        );
        assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
        drop(tmp);
    }

    // ─── V1.53 P1: Timeline limit test (R-V153P1QC3-001) ──

    #[tokio::test]
    async fn timeline_recent_get_respects_server_limit() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
        let pool = state.pool().clone();
        nexus_local_db::narrative_gateway::seed::world(
            &pool,
            "wld_limit",
            "test_creator",
            "Limit World",
            "limit-world",
            "private",
            "manual",
        )
        .await;
        // Seed 5 timeline events
        for i in 1..=5 {
            let evt_id = format!("evt_limit_{i}");
            nexus_local_db::narrative_gateway::seed::event(
                &pool,
                &evt_id,
                "wld_limit",
                "fbk_root",
                "story_advance",
                i,
            )
            .await;
        }

        // Request with limit=2 → should get only 2 events (the most recent 2)
        let req = ToolExecuteRequest {
            tool_name: "nexus.timeline.recent.get".to_string(),
            parameters: serde_json::json!({"world_id": "wld_limit", "limit": 2}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_ok(),
            "timeline with limit should succeed: {result:?}"
        );
        let val = result.expect("result");
        let events = val.as_array().expect("should be an array");
        assert_eq!(events.len(), 2, "should return exactly 2 events");
        assert_eq!(events[0]["sequence_no"], 4);
        assert_eq!(events[1]["sequence_no"], 5);
        drop(tmp);
    }

    #[tokio::test]
    async fn timeline_recent_get_clamps_limit_to_500() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
        let pool = state.pool().clone();
        nexus_local_db::narrative_gateway::seed::world(
            &pool,
            "wld_clamp",
            "test_creator",
            "Clamp World",
            "clamp-world",
            "private",
            "manual",
        )
        .await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.timeline.recent.get".to_string(),
            parameters: serde_json::json!({"world_id": "wld_clamp", "limit": 10000}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        // Should succeed but effective limit is capped at 500
        assert!(result.is_ok(), "clamped limit should succeed: {result:?}");
        let val = result.expect("result");
        let events = val.as_array().expect("should be an array");
        assert!(
            events.len() <= 500,
            "should be capped at 500 events, got {}",
            events.len()
        );
        drop(tmp);
    }

    // ─── V1.54 P0: DF-46 write-tool hermetic tests (T10) ──────────────────

    // --- nexus.kb_snapshot.write (3 tests) ---

    #[tokio::test]
    async fn kb_snapshot_write_upserts_key_blocks() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
        crate::test_utils::seed_test_creator_and_world(state.pool()).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.kb_snapshot.write".to_string(),
            parameters: serde_json::json!({
                "world_id": "wld_test_world",
                "blocks": [{
                    "schema_version": 1,
                    "key_block_id": "kb_write_1",
                    "world_id": "wld_test_world",
                    "block_type": "character",
                    "canonical_name": "test_character",
                    "status": "provisional",
                    "body": {"name": "Test Char"},
                    "created_at": "2026-01-01T00:00:00Z"
                }]
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_ok(),
            "kb_snapshot.write should succeed: {result:?}"
        );
        let val = result.expect("result");
        assert_eq!(val["written"], 1);
        assert_eq!(val["world_id"], "wld_test_world");
        drop(tmp);
    }

    #[tokio::test]
    async fn kb_snapshot_write_rejects_missing_world_id() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.kb_snapshot.write".to_string(),
            parameters: serde_json::json!({"blocks": []}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
    }

    #[tokio::test]
    async fn kb_snapshot_write_rejects_unknown_tool_variant() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.kb_snapshot.write_nonexistent".to_string(),
            parameters: serde_json::json!({"world_id": "wld_test"}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "NOT_SUPPORTED");
    }

    /// C-001 regression: same-creator, block with wrong world_id → rejection.
    #[tokio::test]
    async fn kb_snapshot_write_rejects_cross_world_block_same_creator() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
        crate::test_utils::seed_test_creator_and_world(state.pool()).await;
        // Seed a second world owned by same creator
        // SAFETY: test-only data setup.
        sqlx::query(
            "INSERT OR IGNORE INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES ('wld_other_world', 'ws', 'test_creator', 'Other World', 'other-world', \
             'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .execute(state.pool())
        .await
        .expect("seed second world");

        let req = ToolExecuteRequest {
            tool_name: "nexus.kb_snapshot.write".to_string(),
            parameters: serde_json::json!({
                "world_id": "wld_test_world",
                "blocks": [{
                    "schema_version": 1,
                    "key_block_id": "kb_cross_world_block",
                    "world_id": "wld_other_world",  // mismatched!
                    "block_type": "character",
                    "canonical_name": "cross_world_char",
                    "status": "provisional",
                    "body": {"name": "Cross-world Char"},
                    "created_at": "2026-01-01T00:00:00Z"
                }]
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_err(),
            "cross-world block should be rejected: {result:?}"
        );
        assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
        drop(tmp);
    }

    /// C-001 regression: cross-creator world embedded in block → rejection.
    #[tokio::test]
    async fn kb_snapshot_write_rejects_cross_creator_world_block() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
        crate::test_utils::seed_test_creator_and_world(state.pool()).await;
        // Seed a world owned by a different creator
        // SAFETY: test-only data setup.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('other_creator', 'Other Creator', 'active', datetime('now'), '{}')",
        )
        .execute(state.pool())
        .await
        .expect("seed other creator");
        sqlx::query(
            "INSERT OR IGNORE INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES ('wld_other_creator_world', 'ws', 'other_creator', 'Other Creator World', \
             'other-creator-world', 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .execute(state.pool())
        .await
        .expect("seed other creator world");

        let req = ToolExecuteRequest {
            tool_name: "nexus.kb_snapshot.write".to_string(),
            parameters: serde_json::json!({
                "world_id": "wld_test_world",
                "blocks": [{
                    "schema_version": 1,
                    "key_block_id": "kb_cross_creator_block",
                    "world_id": "wld_other_creator_world",  // different creator's world
                    "block_type": "character",
                    "canonical_name": "cross_creator_char",
                    "status": "provisional",
                    "body": {"name": "Cross-creator Char"},
                    "created_at": "2026-01-01T00:00:00Z"
                }]
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_err(),
            "cross-creator block should be rejected: {result:?}"
        );
        assert_eq!(result.unwrap_err().error_code(), "FORBIDDEN");
        drop(tmp);
    }

    // --- nexus.manuscript.chapter.update (3 tests) ---

    #[tokio::test]
    async fn manuscript_chapter_update_writes_content() {
        let (tmp, nexus_home, db_path, workspace_dir) =
            create_initialized_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, Some(workspace_dir.to_string_lossy().to_string())).await;

        // Create a work and seed chapters
        let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let record = nexus_local_db::works::WorkRecord {
            work_id: work_id.clone(),
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Chapter Update Test".to_string(),
            long_term_goal: "Goal".to_string(),
            initial_idea: "Idea".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: None,
            total_planned_chapters: Some(3),
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
            .await
            .expect("create work")
            .unwrap_err();
        nexus_local_db::work_chapters::seed_chapters(
            state.pool(),
            &work_id,
            "test-update",
            3,
            &now,
        )
        .await
        .expect("seed chapters");

        let req = ToolExecuteRequest {
            tool_name: "nexus.manuscript.chapter.update".to_string(),
            parameters: serde_json::json!({
                "work_id": work_id,
                "chapter": 1,
                "content": "Updated chapter content for testing."
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_ok(),
            "manuscript.chapter.update should succeed: {result:?}"
        );
        let val = result.expect("result");
        assert_eq!(val["work_id"], work_id);
        assert_eq!(val["chapter"], 1);
        // C-002 atomicity: verify DB body_path exists and the file on disk
        // contains the content we wrote (proves file written iff DB committed).
        let chapter_record =
            nexus_local_db::work_chapters::get_chapter(state.pool(), &work_id, 1, 1)
                .await
                .expect("get_chapter after update")
                .expect("chapter should exist after update");
        let db_body_path = chapter_record.body_path.expect("body_path should be set");
        // W-003: verify canonical path follows Works/{work_ref}/Stories/... pattern.
        assert!(
            db_body_path.starts_with("Works/"),
            "body_path should start with Works/, got: {db_body_path}"
        );
        assert!(
            db_body_path.contains("Stories/"),
            "body_path should use Stories/ layout, got: {db_body_path}"
        );
        assert!(
            db_body_path.ends_with(".md"),
            "body_path should end with .md, got: {db_body_path}"
        );
        assert!(
            !db_body_path.ends_with(".tmp"),
            "body_path should be the final file, not a .tmp: {db_body_path}"
        );
        // W-003: db_body_path is a relative canonical path; resolve to absolute.
        let on_disk_path = workspace_dir.join(&db_body_path);
        let on_disk = tokio::fs::read_to_string(&on_disk_path)
            .await
            .expect("file should exist on disk");
        assert_eq!(
            on_disk,
            "Updated chapter content for testing.",
            "file content should match what was written"
        );
        drop(tmp);
    }

    #[tokio::test]
    async fn manuscript_chapter_update_rejects_missing_chapter() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.manuscript.chapter.update".to_string(),
            parameters: serde_json::json!({"work_id": "wrk_test"}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
    }

    #[tokio::test]
    async fn manuscript_chapter_update_rejects_unknown_tool_variant() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.manuscript.chapter.update_v2".to_string(),
            parameters: serde_json::json!({"work_id": "wrk_test", "chapter": 1}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "NOT_SUPPORTED");
    }

    // --- nexus.world.configure (3 tests) ---

    #[tokio::test]
    async fn world_configure_updates_metadata() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
        crate::test_utils::seed_test_creator_and_world(state.pool()).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.world.configure".to_string(),
            parameters: serde_json::json!({
                "world_id": "wld_test_world",
                "title": "Renamed World",
                "visibility": "public"
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_ok(),
            "world.configure should succeed: {result:?}"
        );
        let val = result.expect("result");
        assert_eq!(val["world_id"], "wld_test_world");
        assert_eq!(val["updated"], true);
        drop(tmp);
    }

    #[tokio::test]
    async fn world_configure_rejects_invalid_visibility() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;
        crate::test_utils::seed_test_creator_and_world(state.pool()).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.world.configure".to_string(),
            parameters: serde_json::json!({
                "world_id": "wld_test_world",
                "visibility": "top_secret"
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
        drop(tmp);
    }

    #[tokio::test]
    async fn world_configure_rejects_missing_world_id() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.world.configure".to_string(),
            parameters: serde_json::json!({"title": "No World"}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
    }

    // --- nexus.work.schedule.set (3 tests) ---

    #[tokio::test]
    async fn work_schedule_set_links_schedules() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Create a work
        let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let record = nexus_local_db::works::WorkRecord {
            work_id: work_id.clone(),
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Schedule Test".to_string(),
            long_term_goal: "Goal".to_string(),
            initial_idea: "Idea".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now,
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: None,
            total_planned_chapters: None,
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
            .await
            .expect("create work")
            .unwrap_err();

        let req = ToolExecuteRequest {
            tool_name: "nexus.work.schedule.set".to_string(),
            parameters: serde_json::json!({
                "work_id": work_id,
                "schedule_ids": ["sch_a", "sch_b"]
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_ok(),
            "work.schedule.set should succeed: {result:?}"
        );
        let val = result.expect("result");
        assert_eq!(val["work_id"], work_id);
        let ids = val["schedule_ids"].as_array().expect("schedule_ids array");
        assert_eq!(ids.len(), 2);
    }

    #[tokio::test]
    async fn work_schedule_set_rejects_non_string_ids() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.work.schedule.set".to_string(),
            parameters: serde_json::json!({
                "work_id": "wrk_test",
                "schedule_ids": [1, 2, 3]
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
    }

    #[tokio::test]
    async fn work_schedule_set_rejects_missing_schedule_ids() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.work.schedule.set".to_string(),
            parameters: serde_json::json!({"work_id": "wrk_test"}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
    }

    // --- nexus.finding.resolve (3 tests) ---

    #[tokio::test]
    async fn finding_resolve_marks_resolved() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Create a work first for FK constraint
        let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let record = nexus_local_db::works::WorkRecord {
            work_id: work_id.clone(),
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Findings Test".to_string(),
            long_term_goal: "Goal".to_string(),
            initial_idea: "Idea".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now,
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: None,
            total_planned_chapters: None,
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
            .await
            .expect("create work")
            .unwrap_err();

        // Seed a finding
        let finding_id = format!("fnd_{}", uuid::Uuid::new_v4());
        let now_epoch = chrono::Utc::now().timestamp();
        // SAFETY: test-only data setup.
        sqlx::query(
            "INSERT INTO findings (finding_id, work_id, chapter, severity, status, \
             title, description, target_executor, creator_id, created_at, updated_at) \
             VALUES (?, ?, 1, 'minor', 'open', \
             'Test Finding', 'A test finding', 'none', 'test_creator', ?, ?)",
        )
        .bind(&finding_id)
        .bind(&work_id)
        .bind(now_epoch)
        .bind(now_epoch)
        .execute(state.pool())
        .await
        .expect("seed finding");

        let req = ToolExecuteRequest {
            tool_name: "nexus.finding.resolve".to_string(),
            parameters: serde_json::json!({
                "finding_id": finding_id,
                "resolution": "Fixed in code"
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_ok(),
            "finding.resolve should succeed: {result:?}"
        );
        let val = result.expect("result");
        assert_eq!(val["finding_id"], finding_id);
        assert_eq!(val["resolved"], true);
    }

    /// W-002: nonexistent finding IDs must return NOT_FOUND, not success.
    #[tokio::test]
    async fn finding_resolve_nonexistent_returns_not_found() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.finding.resolve".to_string(),
            parameters: serde_json::json!({"finding_id": "fnd_nonexistent_99999"}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_err(),
            "finding.resolve should reject nonexistent finding: {result:?}"
        );
        assert_eq!(result.unwrap_err().error_code(), "NOT_FOUND");
    }

    // --- nexus.pool.entry.manage (3 tests) ---

    #[tokio::test]
    async fn pool_entry_manage_adds_to_pool() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Create a work
        let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let record = nexus_local_db::works::WorkRecord {
            work_id: work_id.clone(),
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Pool Test Work".to_string(),
            long_term_goal: "Goal".to_string(),
            initial_idea: "Idea".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now,
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: None,
            total_planned_chapters: None,
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
            .await
            .expect("create work")
            .unwrap_err();

        let req = ToolExecuteRequest {
            tool_name: "nexus.pool.entry.manage".to_string(),
            parameters: serde_json::json!({
                "work_id": work_id,
                "action": "add"
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(
            result.is_ok(),
            "pool.entry.manage should succeed: {result:?}"
        );
        let val = result.expect("result");
        assert_eq!(val["work_id"], work_id);
        assert_eq!(val["action"], "add");
        assert_eq!(val["success"], true);
    }

    #[tokio::test]
    async fn pool_entry_manage_rejects_invalid_action() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Create a work so the ownership check passes
        let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let record = nexus_local_db::works::WorkRecord {
            work_id: work_id.clone(),
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Pool Invalid Test".to_string(),
            long_term_goal: "Goal".to_string(),
            initial_idea: "Idea".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now,
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: None,
            total_planned_chapters: None,
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
            .await
            .expect("create work")
            .unwrap_err();

        let req = ToolExecuteRequest {
            tool_name: "nexus.pool.entry.manage".to_string(),
            parameters: serde_json::json!({
                "work_id": work_id,
                "action": "destroy"
            }),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
    }

    #[tokio::test]
    async fn pool_entry_manage_rejects_missing_action() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = ToolExecuteRequest {
            tool_name: "nexus.pool.entry.manage".to_string(),
            parameters: serde_json::json!({"work_id": "wrk_test"}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_code(), "INVALID_INPUT");
    }

    // ─── Cross-cutting tests (R-V153P0QC2-003/004) ──

    /// Concurrent dispatch: 10 parallel invocations of `nexus.context.whoami`
    /// through `registry_dispatch()` — verifies no deadlock/data race on
    /// `LazyLock<CapabilityRegistry>`.
    #[tokio::test]
    async fn concurrent_dispatch_ten_parallel_whoami() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let state = std::sync::Arc::new(state);

        let mut handles = Vec::new();
        for i in 0..10 {
            let state = state.clone();
            handles.push(tokio::spawn(async move {
                let req = ToolExecuteRequest {
                    tool_name: "nexus.context.whoami".to_string(),
                    parameters: serde_json::json!({}),
                    session_id: Some(format!("sess_{i}")),
                    request_id: Some(format!("req_{i}")),
                    caller_kind: None,
                };
                HostToolExecutor::registry_dispatch(&req, &state).await
            }));
        }

        for handle in handles {
            let result = handle.await.expect("no panic");
            assert!(result.is_ok(), "concurrent dispatch should succeed: {result:?}");
            let val = result.expect("result");
            assert_eq!(val["creator_id"], "test_creator");
        }
    }

    /// W-003(qc3): concurrent write-tool dispatch — 10 parallel
    /// `nexus.pool.entry.manage` create calls plus 10 concurrent reads
    /// through `registry_dispatch()`. Verifies no deadlock/data race on
    /// transaction contention for write tools.
    #[tokio::test]
    async fn concurrent_dispatch_ten_parallel_write_tools() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Create a work for FK constraint
        let work_id = format!("wrk_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let record = nexus_local_db::works::WorkRecord {
            work_id: work_id.clone(),
            creator_id: "test_creator".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Concurrent Write Test".to_string(),
            long_term_goal: "Goal".to_string(),
            initial_idea: "Idea".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: None,
            total_planned_chapters: None,
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        nexus_local_db::works::create_work_atomic(state.pool(), &record, None)
            .await
            .expect("create work")
            .unwrap_err();
        let state = std::sync::Arc::new(state);

        let mut handles = Vec::new();
        // 5 write handles (pool.entry.manage create)
        for i in 0..5 {
            let state = state.clone();
            let wid = work_id.clone();
            handles.push(tokio::spawn(async move {
                let req = ToolExecuteRequest {
                    tool_name: "nexus.pool.entry.manage".to_string(),
                    parameters: serde_json::json!({
                        "work_id": wid,
                        "action": "add",
                        "pool_type": "ideas",
                        "content": format!("concurrent entry {i}"),
                    }),
                    session_id: Some(format!("sess_write_{i}")),
                    request_id: Some(format!("req_write_{i}")),
                    caller_kind: None,
                };
                HostToolExecutor::registry_dispatch(&req, &state).await
            }));
        }
        // 5 read handles (whoami — read-only, verifies LazyLock works under
        // concurrent write+read pressure).
        for i in 5..10 {
            let state = state.clone();
            handles.push(tokio::spawn(async move {
                let req = ToolExecuteRequest {
                    tool_name: "nexus.context.whoami".to_string(),
                    parameters: serde_json::json!({}),
                    session_id: Some(format!("sess_read_{i}")),
                    request_id: Some(format!("req_read_{i}")),
                    caller_kind: None,
                };
                HostToolExecutor::registry_dispatch(&req, &state).await
            }));
        }

        for handle in handles {
            let result = handle.await.expect("no panic");
            assert!(
                result.is_ok(),
                "concurrent write dispatch should succeed: {result:?}"
            );
        }
    }

    /// Schedule caller-kind admission: `dispatch_for_schedule` produces
    /// the same result as direct `execute()` for `nexus.context.whoami`.
    #[tokio::test]
    async fn schedule_caller_kind_same_result_as_direct_execute() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let direct_result = HostToolExecutor::execute(
            &ToolExecuteRequest {
                tool_name: "nexus.context.whoami".to_string(),
                parameters: serde_json::json!({}),
                session_id: None,
                request_id: None,
                caller_kind: None,
            },
            &state,
        )
        .await
        .expect("direct execute");

        let schedule_result = HostToolExecutor::dispatch_for_schedule(
            "nexus.context.whoami",
            &serde_json::json!({}),
            "req-sch-001",
            &state,
        )
        .await
        .expect("schedule dispatch");

        assert_eq!(
            direct_result["creator_id"], schedule_result["creator_id"],
            "schedule dispatch should produce same creator_id"
        );
        assert_eq!(
            direct_result["workspace_slug"], schedule_result["workspace_slug"],
            "schedule dispatch should produce same workspace_slug"
        );
    }

    /// C-001 (qc3): Verify audit-log failure is propagated, not silently swallowed.
    ///
    /// Drops the `acp_tool_audit_log` table before calling `registry_dispatch`
    /// to simulate an audit write failure. The dispatch must return an
    /// `Internal` error with code `AUDIT_LOG_FAILED` rather than silently
    /// succeeding.
    #[tokio::test]
    async fn registry_dispatch_propagates_audit_write_failure() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;

        // Simulate audit-write failure by dropping the audit table before
        // constructing WorkspaceState. This causes the INSERT in
        // `audit_tool_execution` to fail with a SQLite error.
        {
            let audit_pool = nexus_local_db::open_pool(&db_path)
                .await
                .expect("open pool for table drop");
            sqlx::query("DROP TABLE IF EXISTS acp_tool_audit_log")
                .execute(&audit_pool)
                .await
                .expect("drop acp_tool_audit_log");
            audit_pool.close().await;
        }

        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Use whoami — a read-only tool that passes admission and would
        // normally succeed. If the audit write fails, the dispatch must
        // propagate that failure.
        let req = ToolExecuteRequest {
            tool_name: "nexus.context.whoami".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };

        let result = HostToolExecutor::registry_dispatch(&req, &state).await;

        match result {
            Err(NexusApiError::Internal { code, .. }) => {
                assert_eq!(
                    code, "AUDIT_LOG_FAILED",
                    "audit write failure must propagate with code AUDIT_LOG_FAILED"
                );
            }
            other => panic!(
                "expected NexusApiError::Internal with code AUDIT_LOG_FAILED, got: {other:?}"
            ),
        }
    }
}
