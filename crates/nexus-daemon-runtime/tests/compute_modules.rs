//! Compute module handler integration tests.
//!
//! Moved out of the library source so that test assertions can use
//! `unwrap()`/`unwrap_err()` without tripping the crate-wide
//! `#![deny(clippy::unwrap_used)]` attribute.

#![allow(clippy::unwrap_used)]

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_daemon_runtime::api::handlers::compute_modules::{get_module, list_modules};
use nexus_daemon_runtime::test_utils::create_test_workspace;
use nexus_daemon_runtime::workspace::WorkspaceState;

#[tokio::test]
async fn list_modules_includes_basic_combat() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

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
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

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
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let result = get_module(State(state), Path("no-such-module".to_string())).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    assert!(err.error_code().contains("not_found"));

    std::mem::forget(tmp);
}
