//! Canonical error envelope end-to-end tests (R-V167P0-QC1-ENVELOPE-E2E).
//!
//! Asserts that `NexusApiError::IntoResponse` emits the single canonical
//! envelope shape for non-2xx responses across two representative variants:
//! - 503 `service_unavailable` (engine missing)
//! - 422 `preset_gates_failed` (gated preset without required work_id)

#![allow(clippy::unwrap_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_contracts::local::schedule::http::AddScheduleRequest;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::{AuthMode, DaemonApiConfig};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;
use serde_json::Value;
use std::sync::Arc;

#[tokio::test]
async fn service_unavailable_returns_canonical_envelope() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let auth_config = DaemonApiConfig {
        api_key: None,
        auth_mode: AuthMode::KeylessLocalhost,
    };
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    std::mem::forget(tmp);

    let resp = server
        .get("/v1/local/orchestration/sessions?creator_id=ctr_test")
        .await;

    resp.assert_status(StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = resp.json();
    assert_eq!(body["success"], false, "envelope success must be false");
    assert_eq!(
        body["error"]["code"], "service_unavailable",
        "wire code must be lowercase snake_case"
    );
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("engine"),
        "message should describe the missing engine"
    );
}

#[tokio::test]
async fn preset_gates_failed_returns_canonical_envelope() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let mut state =
        WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;

    let db_url = format!("sqlite:{}?mode=rw", db_path.display());
    let schedule_pool = Arc::new(sqlx::SqlitePool::connect(&db_url).await.unwrap());
    let supervisor = Arc::new(ScheduleSupervisor::new(schedule_pool));
    state.set_schedule_supervisor(supervisor);

    let registry = Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
    state.set_capability_registry(registry);

    let auth_config = DaemonApiConfig {
        api_key: None,
        auth_mode: AuthMode::KeylessLocalhost,
    };
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    std::mem::forget(tmp);

    let req = AddScheduleRequest {
        creator_id: "ctr_gate_test".to_string(),
        preset_id: "novel-writing".to_string(),
        seed: None,
        label: None,
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: None,
        force_gates: false,
        reason: None,
    };

    let resp = server
        .post("/v1/local/orchestration/schedules")
        .json(&req)
        .await;

    resp.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = resp.json();
    assert_eq!(body["success"], false, "envelope success must be false");
    assert_eq!(
        body["error"]["code"], "preset_gates_failed",
        "wire code must match variant"
    );
    assert!(
        body["error"]["details"]["failed_gates"].is_array(),
        "details should carry failed_gates array"
    );
}
