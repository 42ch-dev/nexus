//! Host tool handlers — extracted from `host_tool_executor.rs` (V1.57 P1).
//!
//! Contains admission pipeline, permission checks, audit logging,
//! tool handler implementations, and registry wrapper functions.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::too_many_lines)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::{read_active_creator_id, read_active_workspace_slug, WorkApiDto};
use crate::workspace::WorkspaceState;
use nexus_kb::KbStore;
use nexus_local_db::works;
use nexus_narrative::NarrativeGateway;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

// Re-import from parent module
use super::host_tool_executor::{
    ToolExecuteRequest, PATCH_ALLOWED_FIELDS, PATCH_REJECTED_FIELDS, STAGE_METADATA_ALLOWED_KEYS,
    TOOL_ALLOWLIST,
};

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
pub(crate) fn admission_pipeline(
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
pub(crate) async fn audit_tool_execution(
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
//
// These `pub(crate)` wrappers adapt the existing private handler functions
// to the `RegistryHandlerFn` signature used by `CapabilityRegistry`.
// They exist so the registry can reference the same handler implementations
// without duplicating logic.
//
// Each wrapper uses an explicit named lifetime `'a` to satisfy the
// higher-ranked trait bound `for<'a> fn(&'a ..., &'a ..., &'a str) -> ...`.

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

// ─── V1.56 P1: nexus.registry.refresh ──────────────────────────────────────

/// `nexus.registry.refresh` — return the registry snapshot (synthetic or CDN).
async fn execute_registry_refresh(
    _req: &ToolExecuteRequest,
    _state: &WorkspaceState,
    _creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    use nexus_orchestration::capability::Capability;
    let cap = nexus_orchestration::capability::builtins::RegistryRefresh::new();
    let input = serde_json::json!({"force": false});
    cap.run(input).await.map_err(|e| NexusApiError::Internal {
        code: "REGISTRY_REFRESH_FAILED".to_string(),
        message: format!("registry.refresh failed: {e}"),
    })
}

pub(crate) fn registry_registry_refresh<'a>(
    req: &'a ToolExecuteRequest,
    state: &'a WorkspaceState,

    // ─── V1.56 P1 + V1.54 P0: registry.refresh + write tools ─────────────────
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_registry_refresh(req, state, creator_id))
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

    let blocks =
        req.parameters["blocks"]
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

    tx.commit().await.map_err(|e| NexusApiError::Internal {
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
        tokio::fs::rename(&abs_tmp, &abs_body).await.map_err(|e| {
            // Best-effort cleanup: remove temp file on rename failure.
            let _ = std::fs::remove_file(&abs_tmp);
            NexusApiError::Internal {
                code: "FILE_RENAME_FAILED".into(),
                message: format!("failed to finalize chapter file: {e}"),
            }
        })?;
        tx.commit().await.map_err(|e| NexusApiError::Internal {
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
        || Err(NexusApiError::NotFound(format!("{work_id}/ch{chapter}"))),
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

    let resolution = req.parameters["resolution"]
        .as_str()
        .unwrap_or("resolved via tool");

    let now_epoch = chrono::Utc::now().timestamp();

    let patch = nexus_local_db::findings::FindingPatch {
        status: Some("resolved".to_string()),
        severity: None,
        title: None,
        description: Some(format!("Resolved via tool: {resolution}")),
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
        return Err(NexusApiError::NotFound(format!("finding {finding_id}")));
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
            .ok_or_else(|| NexusApiError::NotFound(format!("pool entry for {work_id}")))?;

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
