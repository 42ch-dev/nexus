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

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::api::errors::NexusApiError;
use crate::api::errors::RequestId;
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
    axum::extract::State(state): axum::extract::State<WorkspaceState>,
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

/// Request ID middleware (T43).
///
/// Reads an optional `X-Request-Id` header from the incoming request.
/// If present and non-empty, uses it as the request ID.
/// Otherwise, generates a new ULID-style ID (`req_<base62>`).
///
/// Injects a [`RequestId`] extension into the request so downstream handlers
/// can access it. Also intercepts error responses and injects `request_id`
/// into the `error.request_id` field of the JSON body.
///
/// # Tracing
/// - `debug` on every request with the resolved request ID
pub async fn attach_request_id(request: Request<Body>, next: Next) -> Response {
    let request_id = request
        .headers()
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map_or_else(generate_request_id, std::string::ToString::to_string);

    tracing::debug!(
        method = %request.method(),
        path = %request.uri().path(),
        request_id = %request_id,
        "Request ID attached",
    );

    let (mut parts, body) = request.into_parts();
    parts.extensions.insert(RequestId(request_id.clone()));
    let request = Request::from_parts(parts, body);

    let response = next.run(request).await;

    // Inject request_id into error response bodies.
    // Only attempt on 4xx/5xx responses with JSON content type.
    let status = response.status();
    if status.is_client_error() || status.is_server_error() {
        inject_request_id_into_error(response, &request_id).await
    } else {
        response
    }
}

/// Attempt to inject `request_id` into an error response body.
///
/// Reads the response body, parses it as JSON, sets `error.request_id`,
/// and returns a new response with the modified body.
async fn inject_request_id_into_error(mut response: Response, request_id: &str) -> Response {
    use axum::body::Bytes;
    use http_body_util::BodyExt;

    let Ok(bytes) = response
        .body_mut()
        .collect()
        .await
        .map(http_body_util::Collected::to_bytes)
    else {
        return response;
    };

    // Try to parse as JSON and inject request_id
    let modified = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|mut json| {
            let error_obj = json.get_mut("error")?.as_object_mut()?;
            error_obj.insert(
                "request_id".to_string(),
                serde_json::Value::String(request_id.to_string()),
            );
            Some(json)
        })
        .and_then(|json| serde_json::to_vec(&json).ok())
        .unwrap_or_else(|| bytes.to_vec());

    // Reconstruct response with modified body, preserving status and headers
    let status = response.status();
    let mut builder = axum::http::Response::builder().status(status);
    if let Some(content_type) = response.headers().get("content-type").cloned() {
        builder = builder.header("content-type", content_type);
    }
    builder
        .body(Body::from(Bytes::from(modified)))
        .unwrap_or(response)
}

/// Generate a short, unique request ID.
///
/// Format: `req_<8-char-base62>` using a simple counter + timestamp mix.
fn generate_request_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    // Mix counter + timestamp for uniqueness (truncate to u64 is fine for ID purposes)
    #[allow(clippy::cast_possible_truncation)]
    let mixed = count.wrapping_add(millis as u64);
    format!("req_{:013}", base62_encode(mixed))
}

/// Encode a u64 as base62 (0-9, a-z, A-Z).
fn base62_encode(mut n: u64) -> String {
    const CHARSET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    if n == 0 {
        return "0".to_string();
    }
    let mut buf = [0u8; 11]; // u64 max fits in 11 base62 digits
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = CHARSET[(n % 62) as usize];
        n /= 62;
    }
    std::str::from_utf8(&buf[i..]).unwrap_or("0").to_string()
}
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
        let server = TestServer::new(app).expect("TestServer should initialize");
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
        let server = TestServer::new(app).expect("TestServer should initialize");
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

    // --- Request ID middleware tests ---

    #[tokio::test]
    async fn request_id_generated_when_not_provided() {
        use axum::middleware as axum_mw;

        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let routes = Router::new()
            .route("/v1/local/creators", get(handlers::creators::list))
            .route_layer(axum_mw::from_fn_with_state(
                state.clone(),
                super::require_workspace,
            ))
            .route_layer(axum_mw::from_fn(super::attach_request_id))
            .with_state(state);

        let server = TestServer::new(routes).expect("TestServer should initialize");
        let _guard = tmp;
        let response = server.get("/v1/local/creators").await;

        // Should return 409 UNINITIALIZED
        assert_eq!(response.status_code(), 409);
        let body: Value = response.json();
        // request_id should be auto-generated
        let req_id = body["error"]["request_id"]
            .as_str()
            .expect("request_id should be present");
        assert!(
            req_id.starts_with("req_"),
            "auto-generated request_id should start with req_: {req_id}"
        );
    }

    #[tokio::test]
    async fn request_id_from_header_is_preserved() {
        use axum::middleware as axum_mw;

        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let routes = Router::new()
            .route("/v1/local/creators", get(handlers::creators::list))
            .route_layer(axum_mw::from_fn_with_state(
                state.clone(),
                super::require_workspace,
            ))
            .route_layer(axum_mw::from_fn(super::attach_request_id))
            .with_state(state);

        let server = TestServer::new(routes).expect("TestServer should initialize");
        let _guard = tmp;
        let response = server
            .get("/v1/local/creators")
            .add_header("X-Request-Id", "my-custom-id-123")
            .await;

        assert_eq!(response.status_code(), 409);
        let body: Value = response.json();
        assert_eq!(
            body["error"]["request_id"], "my-custom-id-123",
            "request_id from header should be preserved"
        );
    }

    #[test]
    fn base62_encode_produces_valid_output() {
        assert_eq!(super::base62_encode(0), "0");
        assert_eq!(super::base62_encode(1), "1");
        assert_eq!(super::base62_encode(61), "Z");
        assert_eq!(super::base62_encode(62), "10");
        let encoded = super::base62_encode(u64::MAX);
        assert!(!encoded.is_empty());
    }

    #[test]
    fn generate_request_id_format() {
        let id = super::generate_request_id();
        assert!(id.starts_with("req_"), "ID should start with req_: {id}");
        // Second call should produce a different ID (counter increments)
        let id2 = super::generate_request_id();
        assert_ne!(id, id2, "consecutive IDs should differ");
    }
}
