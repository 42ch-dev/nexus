#![allow(clippy::missing_errors_doc)]
//! Host Tool Executor — internal service for daemon-mediated agent tool access.
//!
//! Encapsulates permission checking, workspace path validation, tool execution,
//! and audit logging for ACP tool requests. Extracted from `handlers::acp` so
//! that the internal route `POST /agent-host/internal/tool-executions` and any
//! future callers share a single service.
//!
//! # Architecture
//!
//! ```text
//! HTTP handler (acp::tool_execute or internal route)
//!   └─► HostToolExecutor::execute(req, state)
//!       ├─ Permission check (permissions.toml)
//!       ├─ Path validation (workspace boundary)
//!       ├─ Tool dispatch (fs/read, fs/write)
//!       └─ Audit logging (SQLite)
//! ```

use crate::api::errors::NexusApiError;
use crate::api::handlers::permissions;
use crate::workspace::WorkspaceState;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Request for executing a host tool through the daemon.
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

/// Internal service for executing host tools.
///
/// Stateless — all methods take `&WorkspaceState` as input.
/// Safe to construct on every request or store as a singleton.
pub struct HostToolExecutor;

impl HostToolExecutor {
    /// Execute a host tool request end-to-end:
    /// 1. Permission check
    /// 2. Workspace path validation (for `fs/*` tools)
    /// 3. Tool dispatch
    /// 4. Audit logging
    pub async fn execute(
        req: &ToolExecuteRequest,
        state: &WorkspaceState,
    ) -> Result<serde_json::Value, NexusApiError> {
        tracing::info!(
            tool_name = %req.tool_name,
            parameters = ?req.parameters,
            "HostToolExecutor: executing tool"
        );

        // 1. Permission check
        let workspace_path_str = state.workspace_path().unwrap_or_default();
        if !workspace_path_str.is_empty() {
            if let Some(granted) = load_permission_policy(&workspace_path_str) {
                permissions::check_tool_permission(&req.tool_name, Some(&granted))?;
            }
        }

        // 2. Workspace path validation for file operations
        if req.tool_name.starts_with("fs/") && !workspace_path_str.is_empty() {
            validate_file_path(req, state)?;
        }

        // 3. Tool dispatch
        let result = dispatch_tool(req, state)?;

        // 4. Audit logging
        log_tool_execution(req, &result, state).await?;

        Ok(result)
    }
}

/// Load permission policy from workspace if available.
///
/// Returns `None` if no policy file exists (all tools permitted).
fn load_permission_policy(workspace_path: &str) -> Option<std::collections::HashSet<String>> {
    let policy_path = Path::new(workspace_path)
        .join(".nexus42")
        .join("permissions.toml");
    if !policy_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&policy_path).ok()?;
    let policy: toml::Value = toml::from_str(&content).ok()?;

    let granted = policy.get("grant")?;
    granted.as_table().map(|obj| obj.keys().cloned().collect())
}

/// Validate that file paths are within the workspace root.
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

/// Dispatch the tool request to the appropriate executor.
fn dispatch_tool(
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
        };
        let result = HostToolExecutor::execute(&req, &state).await;
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&path).expect("read back");
        assert_eq!(content, "written!");
    }
}
