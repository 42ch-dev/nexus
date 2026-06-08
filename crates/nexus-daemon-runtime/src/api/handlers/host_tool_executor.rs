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
//!                          └─► dispatch_tool → handler
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
use nexus_local_db::works;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ─── V1.34 Tool IDs (spec §12.2) ──────────────────────────────────────────

/// Allowlist of all V1.34 tool IDs.
const TOOL_ALLOWLIST: &[&str] = &[
    // nexus.* tools (V1.34)
    "nexus.context.whoami",
    "nexus.workspace.info",
    "nexus.work.get",
    "nexus.work.patch",
    "nexus.orchestration.schedule_status",
    "nexus.context.assemble",
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
    let allowed = if tool_name == "nexus.work.patch" {
        is_nexus_write_granted(tool_name, policy)
    } else {
        is_nexus_read_granted(tool_name, policy)
    };

    if allowed {
        return Ok(());
    }

    let reason = if tool_name == "nexus.work.patch" {
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
    /// 2. Tool dispatch
    /// 3. Audit logging (gate 5) — written on **every** invocation path
    ///    (success + all denials/failures), per spec §4.3 gate 5 and §12.6.
    ///
    /// This is the single dispatch table (spec §7.1).
    pub async fn execute(
        req: &ToolExecuteRequest,
        state: &WorkspaceState,
    ) -> Result<serde_json::Value, NexusApiError> {
        tracing::info!(
            tool_name = %req.tool_name,
            caller_kind = ?req.caller_kind,
            "HostToolExecutor: executing tool"
        );

        // Gates 1–4 (no internal audit; we audit centrally below)
        let admission_result = admission_pipeline(req, state);

        let (creator_id, _workspace_slug) = match admission_result {
            Ok(pair) => pair,
            Err(e) => {
                // Audit gate 1-4 denials
                let error_code = e.error_code();
                let _ = audit_tool_execution(req, "denied", Some(error_code), state).await;
                return Err(e);
            }
        };

        // Dispatch
        let dispatch_result = dispatch_tool(req, state, &creator_id).await;

        match &dispatch_result {
            Ok(_) => {
                // Audit success
                let _ = audit_tool_execution(req, "success", None, state).await;
            }
            Err(e) => {
                // Audit handler failures
                let error_code = e.error_code();
                let _ = audit_tool_execution(req, "denied", Some(error_code), state).await;
            }
        }

        dispatch_result
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

// ─── Dispatch table (spec §7.1) ───────────────────────────────────────────

/// Dispatch to the correct handler based on `tool_name`.
///
/// This is the single dispatch table — no duplicate match tables elsewhere.
async fn dispatch_tool(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    match req.tool_name.as_str() {
        // nexus.* tools (V1.34)
        "nexus.context.whoami" => Ok(execute_context_whoami(req, state, creator_id)),
        "nexus.workspace.info" => Ok(execute_workspace_info(req, state, creator_id)),
        "nexus.work.get" => execute_work_get(req, state, creator_id).await,
        "nexus.work.patch" => execute_work_patch(req, state, creator_id).await,
        "nexus.orchestration.schedule_status" => {
            execute_schedule_status(req, state, creator_id).await
        }
        "nexus.context.assemble" => execute_context_assemble(req, state, creator_id).await,
        // fs/* baseline (V1.33)
        "fs/read_text_file" => execute_read_file(req, state),
        "fs/write_text_file" => execute_write_file(req, state),
        // Unknown — should have been caught by gate 1, but fail-closed
        other => Err(NexusApiError::BadRequest {
            code: "NOT_SUPPORTED".to_string(),
            message: format!("unsupported tool: {other}"),
        }),
    }
}

// ─── nexus.* Handlers ─────────────────────────────────────────────────────

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

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;
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
}
