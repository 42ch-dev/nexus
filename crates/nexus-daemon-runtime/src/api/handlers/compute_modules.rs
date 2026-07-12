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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;

    #[tokio::test]
    async fn list_modules_includes_basic_combat() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let (status, Json(resp)) = list_modules(State(state)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            resp.items.iter().any(|m| m.module_id == "basic-combat"),
            "basic-combat should appear in the registry list: {:?}",
            resp.items
        );
        assert!(!resp.has_more);

        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn get_basic_combat_returns_detail() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = get_module(State(state), Path("basic-combat".to_string())).await;
        assert!(result.is_ok());
        let detail = result.unwrap().0;
        assert_eq!(detail.module_id, "basic-combat");
        assert_eq!(detail.name, "Basic Combat");
        assert_eq!(detail.nexus_abi_version, 1);

        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn get_unknown_module_returns_404() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = get_module(State(state), Path("no-such-module".to_string())).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
        assert!(err.error_code().contains("not_found"));

        std::mem::forget(tmp);
    }
}
