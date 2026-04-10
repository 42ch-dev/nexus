//! Integration Tests — Daemon HTTP API

use axum::http::StatusCode;
use axum::Router;
use axum_test::TestServer;
use nexus42d::{api::handlers, workspace::WorkspaceState};
use tempfile::TempDir;

/// Create a test workspace state with temp directory
fn create_test_state() -> (WorkspaceState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let nexus_home = tmp.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();

    let db_path = nexus_home.join("state.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    nexus42d::db::schema::Schema::init(&conn).unwrap();

    // Insert test data
    conn.execute(
        "INSERT INTO creators (creator_id, display_name, status, cached_at, data) VALUES ('ctr_test_001', 'Test Creator', 'active', '2026-04-06T00:00:00Z', '{}')",
        [],
    ).unwrap();

    conn.execute(
        "INSERT INTO reference_sources (reference_source_id, workspace_id, source_type, uri, title, scan_status, created_at) VALUES ('ref_test_001', 'local', 'pdf', 'References/test.pdf', 'Test Reference', 'scanned', '2026-04-06T00:00:00Z')",
        [],
    ).unwrap();

    drop(conn);

    let state = WorkspaceState::new_for_testing(
        nexus_home,
        db_path,
        Some("/tmp/test-workspace".to_string()),
    );

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
    let (state, _tmp) = create_test_state();
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
    let (state, _tmp) = create_test_state();
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
    let (state, _tmp) = create_test_state();
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
    let (state, _tmp) = create_test_state();
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
    let (state, _tmp) = create_test_state();
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();
    let response = server.get("/v1/local/auth/status").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["authenticated"], false);
}

#[tokio::test]
async fn creators_list_endpoint() {
    let (state, _tmp) = create_test_state();
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
    let (state, _tmp) = create_test_state();
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
    let (state, _tmp) = create_test_state();
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
    let (state, _tmp) = create_test_state();
    let app = build_test_app(state);

    let server = TestServer::new(app).unwrap();
    let payload = serde_json::json!({
        "request_id": "req_test_001",
        "workspace_id": "wrk_001",
        "creator_id": "ctr_001",
        "world_id": "wld_001"
    });
    let response = server
        .post("/v1/local/context/assemble")
        .json(&payload)
        .await;

    response.assert_status(StatusCode::NOT_IMPLEMENTED);
    let body: serde_json::Value = response.json();
    assert!(!body["success"].as_bool().unwrap());
    assert_eq!(body["error"]["code"], "NOT_IMPLEMENTED");
}

/// Integration test: concurrent handler requests all succeed
///
/// Verifies that the connection pool allows multiple concurrent requests
/// to different endpoints without deadlock or pool exhaustion.
#[tokio::test]
async fn concurrent_handler_requests_succeed() {
    let (state, _tmp) = create_test_state();
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
