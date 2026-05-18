#![allow(clippy::missing_errors_doc)]
//! ACP tool execution handler — thin HTTP layer delegating to `HostToolExecutor`.
//!
//! The `/v1/local/acp/tool/execute` (legacy) and
//! `/v1/local/agent-host/internal/tool-executions` routes both delegate to
//! [`HostToolExecutor::execute`] for permission checking, path validation,
//! tool dispatch, and audit logging.
//!
//! See [`host_tool_executor`] module for the actual service implementation.

use crate::api::errors::NexusApiError;
use crate::api::handlers::host_tool_executor::{
    HostToolExecutor, ToolExecuteRequest, ToolExecuteResponse,
};
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;

/// POST /v1/local/acp/tool/execute (legacy route)
///
/// Execute an ACP tool request through the daemon.
/// Delegates to [`HostToolExecutor::execute`].
pub async fn tool_execute(
    State(state): State<WorkspaceState>,
    Json(req): Json<ToolExecuteRequest>,
) -> Result<Json<ToolExecuteResponse>, NexusApiError> {
    let result = HostToolExecutor::execute(&req, &state).await?;

    Ok(Json(ToolExecuteResponse {
        success: true,
        result,
    }))
}
