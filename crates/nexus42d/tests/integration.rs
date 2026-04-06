//! Integration Tests — Daemon HTTP API

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
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS creators (creator_id TEXT PRIMARY KEY, display_name TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'active', cached_at TEXT NOT NULL, data TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS reference_sources (reference_source_id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL DEFAULT 'local', source_type TEXT NOT NULL, uri TEXT NOT NULL, title TEXT NOT NULL, scan_status TEXT NOT NULL DEFAULT 'pending', created_at TEXT NOT NULL);"
    ).unwrap();

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
    assert_eq!(body["user_authenticated"], false);
    assert_eq!(body["creator_tokens"], 0);
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
