//! Tests for ACP daemon-mediated tool access (ACP-R8)
//!
//! These tests verify the `/v1/local/acp/tool/execute` endpoint that routes
//! agent tool calls through the daemon for:
//! - Workspace path validation
//! - Permission checking
//! - Audit logging

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::post,
    Router,
};
use nexus42d::api::handlers::acp::tool_execute;
use nexus42d::test_utils::create_test_workspace;
use nexus42d::workspace::WorkspaceState;
use serde_json::json;
use tower::ServiceExt;

/// Helper to create a test workspace state (ADR-014 layout).
async fn create_test_workspace_state() -> (WorkspaceState, nexus42d::test_utils::TestTempRoot) {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let workspace_path = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace_path).expect("Failed to create workspace dir");

    let state = WorkspaceState::new_for_testing(
        nexus_home,
        db_path,
        Some(workspace_path.display().to_string()),
    )
    .await;

    (state, tmp)
}

// RED: Test 1 - Endpoint exists and accepts tool requests
#[tokio::test]
async fn test_tool_execute_endpoint_exists() {
    let (workspace, _tmp) = create_test_workspace_state().await;
    let app = Router::new()
        .route("/v1/local/acp/tool/execute", post(tool_execute))
        .with_state(workspace.clone());

    // Get the workspace path from the state
    let workspace_path = workspace.workspace_path().expect("Workspace path not set");
    let test_file_path = std::path::Path::new(&workspace_path).join("src/main.rs");

    // Create the test file
    std::fs::create_dir_all(test_file_path.parent().unwrap())
        .expect("Failed to create test directory");
    std::fs::write(&test_file_path, "fn main() {}").expect("Failed to write test file");

    let request = Request::builder()
        .method("POST")
        .uri("/v1/local/acp/tool/execute")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "tool_name": "fs/read_text_file",
                "parameters": {
                    "path": test_file_path.display().to_string()
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Print response body for debugging
    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();

    // We expect this to return 200 OK after implementation
    assert_eq!(status, StatusCode::OK, "Response body: {body_str}");
}

// RED: Test 2 - Workspace path validation rejects paths outside workspace
#[tokio::test]
async fn test_tool_execute_rejects_path_outside_workspace() {
    let (workspace, _tmp) = create_test_workspace_state().await;
    let app = Router::new()
        .route("/v1/local/acp/tool/execute", post(tool_execute))
        .with_state(workspace);

    // Try to read a file outside the workspace root (using /etc/passwd as example)
    let request = Request::builder()
        .method("POST")
        .uri("/v1/local/acp/tool/execute")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "tool_name": "fs/read_text_file",
                "parameters": {
                    "path": "/etc/passwd" // Clearly outside workspace
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should return 403 Forbidden for path validation failure
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// RED: Test 3 - Tool execution succeeds for valid workspace path
#[tokio::test]
async fn test_tool_execute_success_for_valid_path() {
    let (workspace, _tmp) = create_test_workspace_state().await;
    let app = Router::new()
        .route("/v1/local/acp/tool/execute", post(tool_execute))
        .with_state(workspace.clone());

    // Get the workspace path from the state
    let workspace_path = workspace.workspace_path().expect("Workspace path not set");
    let test_file_path = std::path::Path::new(&workspace_path).join("src/main.rs");

    // Create a test file in the workspace
    std::fs::create_dir_all(test_file_path.parent().unwrap()).ok();
    std::fs::write(&test_file_path, "fn main() {}").ok();

    let request = Request::builder()
        .method("POST")
        .uri("/v1/local/acp/tool/execute")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "tool_name": "fs/read_text_file",
                "parameters": {
                    "path": test_file_path.display().to_string()
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should return 200 OK with file content
    assert_eq!(response.status(), StatusCode::OK);
}

// RED: Test 4 - Audit log entry created for tool execution
#[tokio::test]
async fn test_tool_execute_creates_audit_log_entry() {
    let (workspace, _tmp) = create_test_workspace_state().await;
    let app = Router::new()
        .route("/v1/local/acp/tool/execute", post(tool_execute))
        .with_state(workspace.clone());

    // Get the workspace path from the state
    let workspace_path = workspace.workspace_path().expect("Workspace path not set");
    let test_file_path = std::path::Path::new(&workspace_path).join("src/test.rs");

    // Create a test file
    std::fs::create_dir_all(test_file_path.parent().unwrap()).ok();
    std::fs::write(&test_file_path, "pub fn test() {}").ok();

    let request = Request::builder()
        .method("POST")
        .uri("/v1/local/acp/tool/execute")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "tool_name": "fs/read_text_file",
                "parameters": {
                    "path": test_file_path.display().to_string()
                }
            })
            .to_string(),
        ))
        .unwrap();

    let _response = app.oneshot(request).await.unwrap();

    // Check audit log in database
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM acp_tool_audit_log WHERE tool_name = 'fs/read_text_file'",
    )
    .fetch_one(workspace.pool())
    .await
    .unwrap();

    assert_eq!(count.0, 1, "Audit log entry should be created");
}
