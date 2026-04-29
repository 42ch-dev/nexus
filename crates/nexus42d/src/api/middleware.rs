//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! API Middleware
//!
//! Tower/axum middleware layers for request validation and lifecycle observability.
//!
//! # Workspace Initialization Race Window
//!
//! There is a theoretical race between `init_workspace()` and the first middleware
//! request. In practice, this window is negligible because:
//! 1. Workspace initialization happens during daemon startup (single-threaded)
//! 2. The HTTP listener is only bound AFTER initialization completes
//! 3. The in-memory mutex provides additional protection
//!
//! If this ever becomes a concern, consider using an `Arc<OnceCell<Workspace>>` pattern.

use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;

/// Workspace initialization guard middleware.
///
/// Rejects requests with 409 Conflict when the workspace has not been initialized.
/// Applied to all daemon routes except `/v1/local/workspace/init` and
/// `/v1/local/runtime/health`.
///
/// # Tracing
/// - `debug` on entry/exit for every request
/// - `info` on rejection (workspace not initialized)
pub async fn require_workspace(
    State(state): State<WorkspaceState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, NexusApiError> {
    tracing::debug!(
        method = %request.method(),
        path = %request.uri().path(),
        "Checking workspace initialization",
    );

    if !state.is_initialized() {
        tracing::info!(
            method = %request.method(),
            path = %request.uri().path(),
            "Request rejected: workspace not initialized",
        );
        return Err(NexusApiError::Uninitialized);
    }

    tracing::debug!(
        method = %request.method(),
        path = %request.uri().path(),
        "Workspace initialized, proceeding",
    );

    Ok(next.run(request).await)
}

// Note: These tests remain inline because they use `crate::test_utils` helpers and
// private router wiring; integration tests cannot easily access `require_workspace`.
// Integration tests in `tests/` cannot access `#[cfg(test)]` modules or internal
// crate items directly. The tests also use `super::*` to import private middleware
// helpers. Consider extracting to `tests/` once the tested items are pub or a public
// test fixture helper is added.
#[cfg(test)]
mod tests {
    use crate::api::handlers;
    use crate::api::middleware;
    use crate::workspace::WorkspaceState;
    use axum::routing::{get, post};
    use axum::Router;
    use axum_test::TestServer;
    use serde_json::Value;

    /// Keeps temp directory alive for the lifetime of the test server.
    struct TestApp {
        _tmp: crate::test_utils::TestTempRoot,
        server: TestServer,
    }

    impl std::ops::Deref for TestApp {
        type Target = TestServer;
        fn deref(&self) -> &TestServer {
            &self.server
        }
    }

    /// Build a test app with an uninitialized workspace (`workspace_path` = None).
    async fn create_uninitialized_app() -> TestApp {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let app = build_router(state);
        let server = TestServer::new(app).unwrap();
        TestApp { _tmp: tmp, server }
    }

    /// Build a test app with an initialized workspace.
    /// Seeds the database with workspace metadata so handlers return 2xx.
    async fn create_initialized_app() -> TestApp {
        let (tmp, nexus_home, db_path, workspace_dir) =
            crate::test_utils::create_initialized_test_workspace().await;
        let state = WorkspaceState::new_for_testing(
            nexus_home,
            db_path,
            Some(workspace_dir.display().to_string()),
        )
        .await;

        let app = build_router(state);
        let server = TestServer::new(app).unwrap();
        TestApp { _tmp: tmp, server }
    }

    /// Build a router that mirrors the production route setup with middleware applied.
    fn build_router(state: WorkspaceState) -> Router {
        use axum::middleware as axum_mw;

        let runtime_routes = Router::new()
            .route("/v1/local/runtime/health", get(handlers::runtime::health))
            .route("/v1/local/runtime/status", get(handlers::runtime::status))
            .route(
                "/v1/local/daemon/status",
                get(handlers::runtime::daemon_status),
            );

        let workspace_routes = Router::new()
            .route("/v1/local/workspace", get(handlers::workspace::info))
            .route(
                "/v1/local/workspace/init",
                post(handlers::workspace::init_workspace),
            );

        // Guarded routes — middleware applied
        let creator_routes = Router::new()
            .route("/v1/local/creators", get(handlers::creators::list))
            .route_layer(axum_mw::from_fn_with_state(
                state.clone(),
                middleware::require_workspace,
            ));

        let manuscript_routes = Router::new()
            .route("/v1/local/manuscript", get(handlers::manuscript::status))
            .route_layer(axum_mw::from_fn_with_state(
                state.clone(),
                middleware::require_workspace,
            ));

        let reference_routes = Router::new()
            .route("/v1/local/references", get(handlers::references::list))
            .route_layer(axum_mw::from_fn_with_state(
                state.clone(),
                middleware::require_workspace,
            ));

        Router::new()
            .merge(runtime_routes)
            .merge(workspace_routes)
            .merge(creator_routes)
            .merge(manuscript_routes)
            .merge(reference_routes)
            .with_state(state)
    }

    // --- Unguarded routes: should work without initialization ---

    #[tokio::test]
    async fn health_route_works_without_init() {
        let app = create_uninitialized_app().await;
        let response = app.get("/v1/local/runtime/health").await;
        assert!(
            response.status_code().is_success(),
            "health should return 2xx without init, got {}",
            response.status_code(),
        );
    }

    #[tokio::test]
    async fn runtime_status_works_without_init() {
        let app = create_uninitialized_app().await;
        let response = app.get("/v1/local/runtime/status").await;
        assert!(
            response.status_code().is_success(),
            "runtime status should return 2xx without init, got {}",
            response.status_code(),
        );
    }

    #[tokio::test]
    async fn daemon_status_works_without_init() {
        let app = create_uninitialized_app().await;
        let response = app.get("/v1/local/daemon/status").await;
        assert!(
            response.status_code().is_success(),
            "daemon status should return 2xx without init, got {}",
            response.status_code(),
        );
        let body: serde_json::Value = response.json();
        assert_eq!(body["lifecycle_state"], "running");
        assert!(body["version"].as_str().is_some());
    }

    #[tokio::test]
    async fn workspace_info_works_without_init() {
        let app = create_uninitialized_app().await;
        let response = app.get("/v1/local/workspace").await;
        assert!(
            response.status_code().is_success(),
            "workspace info should return 2xx without init, got {}",
            response.status_code(),
        );
    }

    // --- Guarded routes: should be rejected without initialization ---

    #[tokio::test]
    async fn creators_returns_409_without_init() {
        let app = create_uninitialized_app().await;
        let response = app.get("/v1/local/creators").await;
        assert_eq!(
            response.status_code(),
            409,
            "creators should return 409 without init"
        );
        assert_uninitialized_error_body(&response);
    }

    #[tokio::test]
    async fn manuscript_returns_409_without_init() {
        let app = create_uninitialized_app().await;
        let response = app.get("/v1/local/manuscript").await;
        assert_eq!(
            response.status_code(),
            409,
            "manuscript should return 409 without init"
        );
        assert_uninitialized_error_body(&response);
    }

    #[tokio::test]
    async fn references_returns_409_without_init() {
        let app = create_uninitialized_app().await;
        let response = app.get("/v1/local/references").await;
        assert_eq!(
            response.status_code(),
            409,
            "references should return 409 without init"
        );
        assert_uninitialized_error_body(&response);
    }

    // --- Guarded routes: should succeed after initialization ---

    #[tokio::test]
    async fn creators_succeeds_after_init() {
        let app = create_initialized_app().await;
        let response = app.get("/v1/local/creators").await;
        assert!(
            response.status_code().is_success(),
            "creators should return 2xx after init, got {}",
            response.status_code(),
        );
    }

    #[tokio::test]
    async fn manuscript_succeeds_after_init() {
        let app = create_initialized_app().await;
        let response = app.get("/v1/local/manuscript").await;
        assert!(
            response.status_code().is_success(),
            "manuscript should return 2xx after init, got {}",
            response.status_code(),
        );
    }

    #[tokio::test]
    async fn references_succeeds_after_init() {
        let app = create_initialized_app().await;
        let response = app.get("/v1/local/references").await;
        assert!(
            response.status_code().is_success(),
            "references should return 2xx after init, got {}",
            response.status_code(),
        );
    }

    // --- Helpers ---

    /// Assert the response body contains the standard UNINITIALIZED error structure.
    fn assert_uninitialized_error_body(response: &axum_test::TestResponse) {
        let body: Value = response.json();
        assert_eq!(body["success"], false, "success should be false");
        assert_eq!(
            body["error"]["code"], "UNINITIALIZED",
            "error code should be UNINITIALIZED"
        );
        assert!(
            !body["error"]["message"].as_str().unwrap_or("").is_empty(),
            "error message should not be empty"
        );
    }
}
