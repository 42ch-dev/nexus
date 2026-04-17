//! Tests for daemon_status v2 endpoint.
//!
//! Per plan §Task 5: verify v2 response shape and v1 wire-compat.

use axum_test::TestServer;
use nexus42d::api::create_router;
use nexus42d::test_utils::create_test_workspace;
use nexus42d::workspace::WorkspaceState;

/// Helper: spawn test daemon server.
async fn spawn_test_daemon() -> (TestServer, WorkspaceState) {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let router = create_router(state.clone());
    let server = TestServer::new(router).expect("Failed to create test server");
    (server, state)
}

/// Test: Happy path returns schema_version=2 and lifecycle_state=running.
#[tokio::test]
async fn status_endpoint_returns_schema_version_2_and_running_in_happy_path() {
    let (server, _state) = spawn_test_daemon().await;

    let response = server.get("/v1/local/daemon/status").await;

    response.assert_status_ok();

    let body = response.json::<serde_json::Value>();
    assert_eq!(body["schema_version"], 2);
    assert_eq!(body["lifecycle_state"], "running"); // Default when no lifecycle set
    assert!(body["version"].is_string());
    assert_eq!(body["implementation_scope"], "full-fsm (v2)");
    assert!(body["subsystems"].is_object());
    assert!(body["degraded"]["subsystems"]
        .as_array()
        .unwrap()
        .is_empty());
}

/// Test: v1 clients still see lifecycle_state field (wire-compat).
#[tokio::test]
async fn v1_client_still_sees_lifecycle_state_running() {
    let (server, _state) = spawn_test_daemon().await;

    let response = server.get("/v1/local/daemon/status").await;

    response.assert_status_ok();

    let body = response.json::<serde_json::Value>();
    // v1 clients only look at lifecycle_state
    assert_eq!(body["lifecycle_state"], "running");
    // They can ignore new fields
    assert!(body["schema_version"].is_number()); // v1 clients ignore this
    assert!(body["uptime_ms"].is_number()); // v1 clients ignore this
}

/// Test: Endpoint includes PID and uptime.
#[tokio::test]
async fn status_endpoint_includes_pid_and_uptime() {
    let (server, _state) = spawn_test_daemon().await;

    let response = server.get("/v1/local/daemon/status").await;

    response.assert_status_ok();

    let body = response.json::<serde_json::Value>();
    assert!(body["pid"].is_number());
    assert!(body["uptime_ms"].is_number());
}

/// Test: Subsystems section has all 6 entries.
#[tokio::test]
async fn subsystems_section_has_all_entries() {
    let (server, _state) = spawn_test_daemon().await;

    let response = server.get("/v1/local/daemon/status").await;

    response.assert_status_ok();

    let body = response.json::<serde_json::Value>();
    let subsystems = body["subsystems"].as_object().unwrap();

    // All 6 subsystems should be present
    assert!(subsystems.contains_key("http"));
    assert!(subsystems.contains_key("db"));
    assert!(subsystems.contains_key("sync"));
    assert!(subsystems.contains_key("engine"));
    assert!(subsystems.contains_key("worker_mgr"));
    assert!(subsystems.contains_key("acp_registry"));

    // Each should have status field
    for (name, entry) in subsystems {
        assert!(
            entry["status"].is_string(),
            "subsystem {} missing status",
            name
        );
    }
}
