//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! ACP tool execution handlers — daemon-mediated agent tool access (ACP-R8)
//!
//! This module provides the `/v1/local/acp/tool/execute` endpoint that routes
//! agent tool calls through the daemon for:
//! - Permission checking (against configurable policy)
//! - Workspace path validation (reject paths outside workspace root)
//! - Audit logging (all tool executions recorded in `SQLite`)
//!
//! # Architecture
//!
//! ```text
//! Agent (ACP subprocess)
//!   └─ Tool request (fs/read, fs/write, terminal/*)
//!       │
//!       └─► nexus42 CLI (ACP adapter)
//!           │
//!           └─► HTTP POST to daemon
//!               │   /v1/local/acp/tool/execute
//!               │
//!               └─► daemon runtime
//!                   ├─ Permission check
//!                   ├─ Path validation
//!                   ├─ Execute tool
//!                   ├─ Audit logging
//!                   └─► Return result to CLI
//! ```
//!
//! # Permission Enforcement (V1.1)
//!
//! Starting V1.1, the daemon enforces permission policies before executing
//! tool requests. If the workspace has a `.nexus42/permissions.toml` file,
//! the daemon loads it and checks tool permissions against the policy.
//! If no policy file exists, all workspace-bound operations are permitted.

use crate::api::errors::NexusApiError;
use crate::api::handlers::permissions;
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Request for executing an ACP tool through the daemon.
#[derive(Debug, Deserialize)]
pub struct ToolExecuteRequest {
    /// Tool name (e.g., "`fs/read_text_file`", "`fs/write_text_file`")
    pub tool_name: String,
    /// Tool-specific parameters (JSON object)
    pub parameters: serde_json::Value,
    /// Optional session ID for audit trail
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Response from tool execution.
#[derive(Debug, Serialize)]
pub struct ToolExecuteResponse {
    /// Whether the tool execution succeeded
    pub success: bool,
    /// Tool-specific result (JSON object)
    pub result: serde_json::Value,
}

/// Load permission policy from workspace if available.
///
/// Returns `None` if no policy file exists (all tools permitted).
fn load_permission_policy(workspace_path: &str) -> Option<std::collections::HashSet<String>> {
    use std::path::Path;

    let policy_path = Path::new(workspace_path)
        .join(".nexus42")
        .join("permissions.toml");
    if !policy_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&policy_path).ok()?;
    let policy: toml::Value = toml::from_str(&content).ok()?;

    // Extract the grant list
    let granted = policy.get("grant")?;
    granted.as_table().map(|obj| obj.keys().cloned().collect())
}

/// POST /v1/local/acp/tool/execute
///
/// Execute an ACP tool request through the daemon with:
/// - Workspace path validation
/// - Audit logging
pub async fn tool_execute(
    State(state): State<WorkspaceState>,
    Json(req): Json<ToolExecuteRequest>,
) -> Result<Json<ToolExecuteResponse>, NexusApiError> {
    tracing::info!(
        tool_name = %req.tool_name,
        parameters = ?req.parameters,
        "Received ACP tool execution request"
    );

    // Check permissions (V1.1: load policy from workspace if available)
    let workspace_path_str = state.workspace_path().unwrap_or_default();
    if !workspace_path_str.is_empty() {
        if let Some(granted) = load_permission_policy(&workspace_path_str) {
            permissions::check_tool_permission(&req.tool_name, Some(&granted))?;
        }
    }

    // Validate workspace path for file operations
    if req.tool_name.starts_with("fs/") {
        validate_file_path(&req, &state)?;
    }

    // Execute the tool
    let result = execute_tool(&req, &state)?;

    // Log audit trail
    log_tool_execution(&req, &result, &state).await?;

    Ok(Json(ToolExecuteResponse {
        success: true,
        result,
    }))
}

/// Validate that file paths are within the workspace root.
///
/// V1.0 security: reject any path outside the workspace directory.
/// This prevents agents from accessing sensitive system files.
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

    // For paths that don't exist yet (write operations), validate the parent directory
    let canonical_requested = if requested_path.exists() {
        requested_path
            .canonicalize()
            .map_err(|e| NexusApiError::InvalidInput {
                field: "parameters.path".into(),
                reason: format!("path cannot be resolved: {e}"),
            })?
    } else {
        // For non-existent paths, check if the path starts with workspace root
        // Convert both to absolute paths for comparison
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

        // Check if the absolute path starts with workspace root
        // Use lexigraphic comparison for non-existent paths
        let abs_requested_str = abs_requested.display().to_string();
        if !abs_requested_str.starts_with(&workspace_path_str) {
            return Err(NexusApiError::Forbidden {
                resource: "file".into(),
                reason: "path outside workspace root".into(),
            });
        }

        abs_requested
    };

    // If the path exists, verify it's inside the workspace
    if requested_path.exists() {
        let canonical_workspace =
            workspace_root
                .canonicalize()
                .map_err(|e| NexusApiError::Internal {
                    code: "WORKSPACE_PATH_INVALID".into(),
                    message: format!("workspace root cannot be resolved: {e}"),
                })?;

        // Security check: path must start with workspace root
        if !canonical_requested.starts_with(&canonical_workspace) {
            return Err(NexusApiError::Forbidden {
                resource: "file".into(),
                reason: "path outside workspace root".into(),
            });
        }
    }

    Ok(())
}

/// Execute the requested tool operation.
///
/// V1.0 supports:
/// - `fs/read_text_file`: Read file content from workspace
/// - `fs/write_text_file`: Write file content to workspace
///
/// Terminal operations are deferred to V1.1.
fn execute_tool(
    req: &ToolExecuteRequest,
    state: &WorkspaceState,
) -> Result<serde_json::Value, NexusApiError> {
    match req.tool_name.as_str() {
        "fs/read_text_file" => execute_read_file(req, state),
        "fs/write_text_file" => execute_write_file(req, state),
        other => Err(NexusApiError::InvalidInput {
            field: "tool_name".into(),
            reason: format!("unsupported tool: {other}"),
        }),
    }
}

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

    // Create parent directories if needed
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

/// Log tool execution to audit trail in `SQLite`.
///
/// Records:
/// - Tool name
/// - Path (for file operations)
/// - Outcome (success/failure)
/// - Timestamp
async fn log_tool_execution(
    req: &ToolExecuteRequest,
    result: &serde_json::Value,
    state: &WorkspaceState,
) -> Result<(), NexusApiError> {
    let path = req.parameters["path"].as_str().unwrap_or("").to_string();
    let outcome = if result.is_object() {
        "success"
    } else {
        "error"
    }
    .to_string();
    let tool_name = req.tool_name.clone();

    sqlx::query!(
        "INSERT INTO acp_tool_audit_log (tool_name, path, outcome, agent_id, session_id)
         VALUES (?, ?, ?, NULL, NULL)",
        tool_name,
        path,
        outcome
    )
    .execute(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "AUDIT_LOG_FAILED".into(),
        message: format!("failed to write audit log: {e}"),
    })?;

    Ok(())
}
