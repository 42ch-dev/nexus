//! Compute module registry handlers.
//!
//! Read-only endpoints for discovering installed WASM compute modules.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use nexus_contracts::generated::daemon_api::compute::{
    list_modules_response::ListModulesResponse, module_detail::ModuleDetail,
};

/// `GET /v1/daemon/compute/modules`
///
/// Returns a summary of every installed compute module.
pub async fn list_modules(
    State(_state): State<WorkspaceState>,
) -> (StatusCode, Json<ListModulesResponse>) {
    let items = nexus_wasm_host::list_modules();
    (
        StatusCode::OK,
        Json(ListModulesResponse {
            items,
            has_more: false,
        }),
    )
}

/// `GET /v1/daemon/compute/modules/{module_id}`
///
/// Returns the full manifest.json shape for a single module.
///
/// # Errors
///
/// Returns `NexusApiError::NotFound` if the module id is not installed.
pub async fn get_module(
    State(_state): State<WorkspaceState>,
    Path(module_id): Path<String>,
) -> Result<Json<ModuleDetail>, NexusApiError> {
    nexus_wasm_host::get_module(&module_id)
        .map(Json)
        .ok_or_else(|| NexusApiError::NotFound(format!("module '{module_id}' not found")))
}
