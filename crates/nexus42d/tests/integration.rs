//! Integration Tests — Daemon HTTP API
//!
//! E9: Integration tests for daemon HTTP endpoints

use axum::http::StatusCode;
use axum::Router;
use axum_test::TestServer;
use nexus42d::{api::handlers, test_utils::create_test_workspace, workspace::WorkspaceState};

/// Create a test workspace state with temp directory (ADR-014 layout under `HOME`).
async fn create_test_state() -> (WorkspaceState, nexus42d::test_utils::TestTempRoot) {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;

    // Insert test data
    let pool = nexus_local_db::open_pool(std::path::Path::new(&db_path))
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO creators (creator_id, display_name, status, cached_at, data) VALUES ('ctr_test_001', 'Test Creator', 'active', '2026-04-06T00:00:00Z', '{}')"
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO reference_sources (reference_source_id, workspace_id, source_type, uri, title, scan_status, created_at) VALUES ('ref_test_001', 'local', 'pdf', 'References/test.pdf', 'Test Reference', 'scanned', '2026-04-06T00:00:00Z')"
    )
    .execute(&pool)
    .await
    .unwrap();

    let state = WorkspaceState::new_for_testing(
        nexus_home,
        db_path,
        Some("/tmp/test-workspace".to_string()),
    )
    .await;

    (state, tmp)
}

fn build_test_app(state: WorkspaceState) -> Router {
    Router::new()
        .route(
            "/v1/local/runtime/health",
            axum::routing::get(handlers::runtime::health),
        )
        .route(
            "/v1/local/runtime/status",
            axum::routing::get(handlers::runtime::status),
        )
        .route(
            "/v1/local/daemon/status",
            axum::routing::get(handlers::runtime::daemon_status),
        )
        .route(
            "/v1/local/workspace",
            axum::routing::get(handlers::workspace::info),
        )
        .route(
            "/v1/local/auth/status",
            axum::routing::get(handlers::auth::status),
        )
        .route(
            "/v1/local/creators",
            axum::routing::get(handlers::creators::list),
        )
        .route(
            "/v1/local/manuscript",
            axum::routing::get(handlers::manuscript::status),
        )
        .route(
            "/v1/local/references",
            axum::routing::get(handlers::references::list),
        )
        .route(
            "/v1/local/context/assemble",
            axum::routing::post(handlers::context::assemble),
        )
        .with_state(state)
}

#[tokio::test]
async fn health_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();
    let response = server.get("/v1/local/runtime/health").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], "0.1.0");
}

#[tokio::test]
async fn status_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();
    let response = server.get("/v1/local/runtime/status").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["version"], "0.1.0");
    assert!(body["uptime_seconds"].as_u64().is_some());
    assert_eq!(body["workspace_initialized"], true);
}

#[tokio::test]
async fn daemon_status_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();
    let response = server.get("/v1/local/daemon/status").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["lifecycle_state"], "running");
    assert_eq!(body["version"], "0.1.0");
    assert!(body["implementation_scope"].as_str().unwrap().len() > 0);
}

#[tokio::test]
async fn workspace_info_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();
    let response = server.get("/v1/local/workspace").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["initialized"], true);
    assert_eq!(body["workspace_path"], "/tmp/test-workspace");
    assert!(body["database_path"].as_str().unwrap().contains("state.db"));
}

#[tokio::test]
async fn auth_status_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();
    let response = server.get("/v1/local/auth/status").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["authenticated"], false);
}

#[tokio::test]
async fn creators_list_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();
    let response = server.get("/v1/local/creators").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let creators = body["creators"].as_array().unwrap();
    assert_eq!(creators.len(), 1);
    assert_eq!(creators[0]["creator_id"], "ctr_test_001");
    assert_eq!(creators[0]["display_name"], "Test Creator");
}

#[tokio::test]
async fn manuscript_status_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();
    let response = server.get("/v1/local/manuscript").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["phase"].is_null());
    assert!(body["active_manifest_id"].is_null());
}

#[tokio::test]
async fn references_list_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();
    let response = server.get("/v1/local/references").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let refs = body["references"].as_array().unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["reference_source_id"], "ref_test_001");
    assert_eq!(refs[0]["source_type"], "pdf");
    assert_eq!(refs[0]["title"], "Test Reference");
}

#[tokio::test]
async fn context_assemble_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();

    // Test: local_first mode returns mock response
    let payload = serde_json::json!({
        "creator_id": "ctr_001",
        "workspace_slug": "default",
        "runtime_mode": "local_first",
        "prompt_hint": "Test prompt"
    });
    let response = server
        .post("/v1/local/context/assemble")
        .json(&payload)
        .await;

    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body["memory_items"].is_array());
    assert!(body["kb"].is_array());
    assert!(body["timeline"].is_array());
    assert!(body["metadata"]["assembled_at"].is_string());
}

/// Integration test: context/assemble returns 403 for local_only mode
#[tokio::test]
async fn context_assemble_blocked_for_local_only() {
    let (state, _tmp) = create_test_state().await;
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();

    let payload = serde_json::json!({
        "creator_id": "ctr_001",
        "workspace_slug": "default",
        "runtime_mode": "local_only"
    });
    let response = server
        .post("/v1/local/context/assemble")
        .json(&payload)
        .await;

    response.assert_status(StatusCode::FORBIDDEN);
    let body: serde_json::Value = response.json();
    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["error"]["code"], "FORBIDDEN");
}

/// Integration test: concurrent handler requests all succeed
///
/// Verifies that the connection pool allows multiple concurrent requests
/// to different endpoints without deadlock or pool exhaustion.
#[tokio::test]
async fn concurrent_handler_requests_succeed() {
    let (state, _tmp) = create_test_state().await;
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();

    // Fire 5 concurrent requests to different endpoints
    let (health, workspace, creators, manuscript, references) = tokio::join!(
        async { server.get("/v1/local/runtime/health").await.status_code() },
        async { server.get("/v1/local/workspace").await.status_code() },
        async { server.get("/v1/local/creators").await.status_code() },
        async { server.get("/v1/local/manuscript").await.status_code() },
        async { server.get("/v1/local/references").await.status_code() },
    );

    assert_eq!(health, 200, "health endpoint returned {}", health);
    assert_eq!(workspace, 200, "workspace endpoint returned {}", workspace);
    assert_eq!(creators, 200, "creators endpoint returned {}", creators);
    assert_eq!(
        manuscript, 200,
        "manuscript endpoint returned {}",
        manuscript
    );
    assert_eq!(
        references, 200,
        "references endpoint returned {}",
        references
    );
}

// =============================================================================
// E9: Additional daemon HTTP endpoint tests
// =============================================================================

/// Create a test app with all E9-relevant routes for basic endpoint tests.
/// Uses WorkspaceState without outbox (sync operations will return SYNC_NOT_CONFIGURED).
fn build_extended_test_app(state: WorkspaceState) -> Router {
    Router::new()
        .route(
            "/v1/local/runtime/health",
            axum::routing::get(handlers::runtime::health),
        )
        .route(
            "/v1/local/runtime/status",
            axum::routing::get(handlers::runtime::status),
        )
        .route(
            "/v1/local/daemon/status",
            axum::routing::get(handlers::runtime::daemon_status),
        )
        .route(
            "/v1/local/workspace",
            axum::routing::get(handlers::workspace::info),
        )
        .route(
            "/v1/local/auth/status",
            axum::routing::get(handlers::auth::status),
        )
        .route(
            "/v1/local/creators",
            axum::routing::get(handlers::creators::list),
        )
        .route(
            "/v1/local/manuscript",
            axum::routing::get(handlers::manuscript::status),
        )
        .route(
            "/v1/local/references",
            axum::routing::get(handlers::references::list),
        )
        .route(
            "/v1/local/context/assemble",
            axum::routing::post(handlers::context::assemble),
        )
        // E9: Memory/KB endpoints
        .route(
            "/v1/local/memory/pending-review",
            axum::routing::post(handlers::memory::create_pending_review),
        )
        .route(
            "/v1/local/memory/pending-review/count",
            axum::routing::get(handlers::memory::count_pending_reviews),
        )
        .route(
            "/v1/local/memory/pending-review/:id",
            axum::routing::delete(handlers::memory::delete_pending_review),
        )
        // E9: ACP session endpoints
        .route(
            "/v1/local/acp/sessions",
            axum::routing::get(handlers::sessions::list_sessions),
        )
        .route(
            "/v1/local/acp/sessions/:id",
            axum::routing::delete(handlers::sessions::delete_session),
        )
        // E9: ACP tool execute endpoint
        .route(
            "/v1/local/acp/tool/execute",
            axum::routing::post(handlers::acp::tool_execute),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// E9: Memory/KB endpoint tests
// ---------------------------------------------------------------------------

/// Test: create pending review endpoint
#[tokio::test]
async fn memory_create_pending_review_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    let payload = serde_json::json!({
        "pending_id": "mem_test_001",
        "session_id": "sess_test_001",
        "creator_id": "ctr_test001",
        "world_id": "wld_test_001",
        "task_kind": "brainstorm",
        "raw_digest": "Test digest content for pending review"
    });
    let response = server
        .post("/v1/local/memory/pending-review")
        .json(&payload)
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["pending_id"], "mem_test_001");
}

/// Test: create pending review with idempotent retry (same pending_id)
#[tokio::test]
async fn memory_create_pending_review_idempotent_retry() {
    let (state, _tmp) = create_test_state().await;
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    let payload = serde_json::json!({
        "pending_id": "mem_idempotent_001",
        "session_id": "sess_idempotent_001",
        "creator_id": "ctr_test001",
        "raw_digest": "Digest content"
    });

    // First request
    let response1 = server
        .post("/v1/local/memory/pending-review")
        .json(&payload)
        .await;
    response1.assert_status_ok();

    // Retry same request (should be idempotent - INSERT OR IGNORE)
    let response2 = server
        .post("/v1/local/memory/pending-review")
        .json(&payload)
        .await;
    response2.assert_status_ok();
}

/// Test: create pending review rejects invalid creator_id (must match ctr_<alphanumeric>)
#[tokio::test]
async fn memory_create_pending_review_rejects_invalid_creator_id() {
    let (state, _tmp) = create_test_state().await;
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    let payload = serde_json::json!({
        "pending_id": "mem_invalid_001",
        "session_id": "sess_invalid_001",
        "creator_id": "invalid_creator",  // Must start with ctr_
        "raw_digest": "Test digest"
    });
    let response = server
        .post("/v1/local/memory/pending-review")
        .json(&payload)
        .await;

    // Should return 400 Bad Request
    response.assert_status(StatusCode::BAD_REQUEST);
}

/// Test: create pending review rejects empty pending_id
#[tokio::test]
async fn memory_create_pending_review_rejects_empty_pending_id() {
    let (state, _tmp) = create_test_state().await;
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    let payload = serde_json::json!({
        "pending_id": "",
        "session_id": "sess_001",
        "creator_id": "ctr_test001",
        "raw_digest": "Test digest"
    });
    let response = server
        .post("/v1/local/memory/pending-review")
        .json(&payload)
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

/// Test: count pending reviews endpoint
#[tokio::test]
async fn memory_count_pending_reviews_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let db_path = state.database_path();
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    // Insert a pending review first
    let pool = nexus_local_db::open_pool(std::path::Path::new(&db_path))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO memory_pending_review (pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at)
         VALUES ('mem_count_001', 'sess_count_001', 'ctr_test001', 'wld_001', 'brainstorm', 'digest content', '2026-04-15T00:00:00Z')"
    )
    .execute(&pool)
    .await
    .unwrap();

    let response = server
        .get("/v1/local/memory/pending-review/count?creator_id=ctr_test001")
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["count"].as_u64().is_some());
}

/// Test: delete pending review endpoint
#[tokio::test]
async fn memory_delete_pending_review_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let db_path = state.database_path();
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    // Insert a pending review first
    let pool = nexus_local_db::open_pool(std::path::Path::new(&db_path))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO memory_pending_review (pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at)
         VALUES ('mem_delete_001', 'sess_delete_001', 'ctr_test001', 'wld_001', 'brainstorm', 'digest content', '2026-04-15T00:00:00Z')"
    )
    .execute(&pool)
    .await
    .unwrap();

    let response = server
        .delete("/v1/local/memory/pending-review/mem_delete_001?creator_id=ctr_test001")
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["pending_id"], "mem_delete_001");
}

/// Test: delete pending review returns 404 for non-existent id
#[tokio::test]
async fn memory_delete_pending_review_not_found() {
    let (state, _tmp) = create_test_state().await;
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    let response = server
        .delete("/v1/local/memory/pending-review/nonexistent_id?creator_id=ctr_test001")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// E9: ACP session endpoint tests
// ---------------------------------------------------------------------------

/// Test: list ACP sessions endpoint
#[tokio::test]
async fn acp_sessions_list_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let db_path = state.database_path();
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    // Insert test sessions directly into the database
    let pool = nexus_local_db::open_pool(std::path::Path::new(&db_path))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO acp_sessions (session_id, agent_id, created_at, last_active, workspace_hint)
         VALUES ('sess_list_001', 'claude-acp', '2026-04-15T10:00:00Z', '2026-04-15T12:00:00Z', '/tmp/test')"
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO acp_sessions (session_id, agent_id, created_at, last_active, workspace_hint)
         VALUES ('sess_list_002', 'codex-acp', '2026-04-15T11:00:00Z', '2026-04-15T13:00:00Z', '/tmp/test2')"
    )
    .execute(&pool)
    .await
    .unwrap();

    let response = server.get("/v1/local/acp/sessions").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 2);
    let sessions = body["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2);
}

/// Test: list ACP sessions returns empty array when no sessions
#[tokio::test]
async fn acp_sessions_list_empty() {
    let (state, _tmp) = create_test_state().await;
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    let response = server.get("/v1/local/acp/sessions").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 0);
    let sessions = body["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 0);
}

/// Test: delete ACP session endpoint
#[tokio::test]
async fn acp_sessions_delete_endpoint() {
    let (state, _tmp) = create_test_state().await;
    let db_path = state.database_path();
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    // Insert a test session
    let pool = nexus_local_db::open_pool(std::path::Path::new(&db_path))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO acp_sessions (session_id, agent_id, created_at, last_active, workspace_hint)
         VALUES ('sess_delete_001', 'claude-acp', '2026-04-15T10:00:00Z', '2026-04-15T12:00:00Z', '/tmp/test')"
    )
    .execute(&pool)
    .await
    .unwrap();

    let response = server
        .delete("/v1/local/acp/sessions/sess_delete_001")
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["deleted"], true);
    assert_eq!(body["session_id"], "sess_delete_001");
}

/// Test: delete ACP session returns deleted=false for non-existent session
#[tokio::test]
async fn acp_sessions_delete_not_found() {
    let (state, _tmp) = create_test_state().await;
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    let response = server
        .delete("/v1/local/acp/sessions/nonexistent_session")
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["deleted"], false);
}

// ---------------------------------------------------------------------------
// E9: ACP tool execute endpoint tests
// ---------------------------------------------------------------------------

// Note: ACP tool execute path validation tests are covered in internal unit tests
// (crates/nexus42d/tests/acp_tool.rs). The external integration tests here use
// temp paths outside workspace which correctly return 403 Forbidden.
/// Test: ACP tool execute endpoint - fs/read_text_file success
/// Uses workspace-safe fixture path (under workspace root) to pass path validation.
#[tokio::test]
async fn acp_tool_execute_read_file_success() {
    let (state, _tmp) = create_test_state().await;
    let app = build_extended_test_app(state.clone());

    let server = TestServer::new(app).unwrap();

    // Create a test file inside the workspace root to pass path validation
    let workspace_root = state.workspace_path().unwrap_or_default();
    let workspace_dir = std::path::Path::new(&workspace_root);
    std::fs::create_dir_all(workspace_dir).ok();
    let test_file = workspace_dir.join("tmp").join("acp-tool-test-read.txt");
    std::fs::create_dir_all(test_file.parent().unwrap()).unwrap();
    std::fs::write(&test_file, "Hello, Nexus test!").unwrap();

    let payload = serde_json::json!({
        "tool_name": "fs/read_text_file",
        "parameters": {
            "path": test_file.to_str().unwrap()
        },
        "session_id": "sess_tool_001"
    });
    let response = server
        .post("/v1/local/acp/tool/execute")
        .json(&payload)
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["result"]["content"], "Hello, Nexus test!");

    // Cleanup
    std::fs::remove_file(&test_file).ok();
}

/// Test: ACP tool execute endpoint - fs/write_text_file success
/// Uses workspace-safe fixture path (under workspace root) to pass path validation.
#[tokio::test]
async fn acp_tool_execute_write_file_success() {
    let (state, _tmp) = create_test_state().await;
    let app = build_extended_test_app(state.clone());

    let server = TestServer::new(app).unwrap();

    // Create a test file path inside the workspace root to pass path validation
    let workspace_root = state.workspace_path().unwrap_or_default();
    let workspace_dir = std::path::Path::new(&workspace_root);
    std::fs::create_dir_all(workspace_dir).ok();
    let test_file = workspace_dir.join("tmp").join("acp-tool-test-write.txt");
    std::fs::create_dir_all(test_file.parent().unwrap()).unwrap();

    let payload = serde_json::json!({
        "tool_name": "fs/write_text_file",
        "parameters": {
            "path": test_file.to_str().unwrap(),
            "content": "Written by Nexus ACP tool test"
        },
        "session_id": "sess_tool_002"
    });
    let response = server
        .post("/v1/local/acp/tool/execute")
        .json(&payload)
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["result"]["written"], true);

    // Verify file was written
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(content, "Written by Nexus ACP tool test");

    // Cleanup
    std::fs::remove_file(&test_file).ok();
}

/// Test: ACP tool execute endpoint - rejects unsupported tool
#[tokio::test]
async fn acp_tool_execute_unsupported_tool() {
    let (state, _tmp) = create_test_state().await;
    let app = build_extended_test_app(state);

    let server = TestServer::new(app).unwrap();

    let payload = serde_json::json!({
        "tool_name": "unsupported/tool",
        "parameters": {}
    });
    let response = server
        .post("/v1/local/acp/tool/execute")
        .json(&payload)
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// E9: Sync endpoint tests (require outbox)
// ---------------------------------------------------------------------------

/// Test: sync status endpoint returns zeroed status when outbox not configured
#[tokio::test]
async fn sync_status_endpoint_no_outbox() {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let state =
        WorkspaceState::new_for_testing(nexus_home, db_path, Some("/tmp/test".to_string())).await;
    let state_clone = state.clone();
    let app = build_extended_test_app(state);

    let _server = TestServer::new(app).unwrap();

    // Sync status without outbox - the extended app uses basic state (no outbox)
    // But we need to use the sync-specific app builder for sync endpoints
    // This test verifies sync status returns proper error without outbox
    let sync_app = Router::new()
        .route(
            "/v1/local/sync/status",
            axum::routing::get(handlers::sync::status),
        )
        .with_state(state_clone);

    let sync_server = TestServer::new(sync_app).unwrap();
    let response = sync_server.get("/v1/local/sync/status").await;

    // Should return OK with zeroed counts (outbox is None → returns empty status)
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["staged_count"], 0);
    assert_eq!(body["ready_count"], 0);
    assert_eq!(body["sent_count"], 0);
    assert_eq!(body["acked_count"], 0);
    assert_eq!(body["conflicted_count"], 0);
    assert_eq!(body["failed_count"], 0);
    assert!(body["last_sync_at"].is_null());

    let _ = tmp; // keep alive
}

// Note: sync tests that require outbox initialization are defined as
// inline #[cfg(test)] modules within the crate (see sync.rs tests).
// External integration tests use WorkspaceState without outbox.
