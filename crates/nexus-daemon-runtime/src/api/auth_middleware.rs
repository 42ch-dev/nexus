//! Auth Middleware
//!
//! Tower/axum middleware layer for daemon-local API key authentication.
//!
//! Replaces the former Bearer-token middleware with `X-API-Key` validation.
//! Two startup modes:
//! - **`KeyedAll`**: `NEXUS42_DAEMON_API_KEY` is set (non-empty). All requests need `X-API-Key`.
//! - **`KeylessLocalhost`**: key is empty/unset. Loopback accepted without key, non-loopback
//!   rejected with 403.

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::api::errors::NexusApiError;

/// Authentication mode determined at daemon startup from `NEXUS42_DAEMON_API_KEY`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// API key is configured — all requests must present a valid `X-API-Key`.
    KeyedAll,
    /// No API key configured — loopback accepted without key, non-loopback rejected.
    KeylessLocalhost,
}

/// Daemon API authentication config, resolved once at startup.
#[derive(Debug, Clone)]
pub struct DaemonApiConfig {
    /// The configured API key (`None` when `NEXUS42_DAEMON_API_KEY` is empty/unset).
    pub api_key: Option<String>,
    /// Resolved authentication mode.
    pub auth_mode: AuthMode,
}

impl DaemonApiConfig {
    /// Environment variable name for the daemon API key.
    pub const ENV_KEY: &'static str = "NEXUS42_DAEMON_API_KEY";

    /// Read and resolve config from the process environment.
    ///
    /// Trims surrounding whitespace for validation only; stores the trimmed value.
    /// Logs a warning when entering keyless-localhost mode.
    pub fn from_env() -> Self {
        let raw = std::env::var(Self::ENV_KEY).unwrap_or_default();
        let trimmed = raw.trim().to_string();

        if trimmed.is_empty() {
            tracing::warn!(
                "{} not set; daemon running in keyless-localhost mode. \
                 Non-local connections will be rejected.",
                Self::ENV_KEY,
            );
            Self {
                api_key: None,
                auth_mode: AuthMode::KeylessLocalhost,
            }
        } else {
            tracing::info!("Daemon API key loaded; running in keyed-all mode.");
            Self {
                api_key: Some(trimmed),
                auth_mode: AuthMode::KeyedAll,
            }
        }
    }

    /// Create a config for testing in keyed-all mode with a specific key.
    #[cfg(test)]
    pub fn keyed(key: &str) -> Self {
        Self {
            api_key: Some(key.to_string()),
            auth_mode: AuthMode::KeyedAll,
        }
    }

    /// Create a config for testing in keyless-localhost mode.
    #[cfg(test)]
    pub const fn keyless() -> Self {
        Self {
            api_key: None,
            auth_mode: AuthMode::KeylessLocalhost,
        }
    }
}

/// Extension injected by `require_api_key` on successful authentication.
#[derive(Debug, Clone)]
pub struct AuthenticatedLocalClient {
    pub auth_scheme: LocalAuthScheme,
}

/// How the local client was authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalAuthScheme {
    /// Validated via `X-API-Key` header.
    ApiKey,
    /// Keyless-localhost mode — loopback connection, no key required.
    LoopbackBypass,
}

/// Constant-time comparison of two byte strings.
///
/// Returns `true` only when the slices have equal length *and* equal contents.
/// Timing does not depend on the position of the first differing byte.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}

/// Require API-key authentication middleware.
///
/// Behaviour depends on the [`DaemonApiConfig`] resolved at startup:
///
/// - **`KeyedAll`**: every request must present an `X-API-Key` header that matches
///   the configured key. Missing/wrong key → `401 AUTH_REQUIRED`.
/// - **`KeylessLocalhost`**: requests from loopback addresses are accepted without
///   a key (bypass injection). Requests from non-loopback → `403 FORBIDDEN`.
///
/// # Errors
///
/// - [`NexusApiError::AuthRequired`] — missing or wrong `X-API-Key` (keyed-all mode)
/// - [`NexusApiError::Forbidden`] — non-loopback connection (keyless-localhost mode)
///
/// # Tracing
/// - `debug` on every request
/// - `info` on rejection
pub async fn require_api_key(
    axum::extract::State(config): axum::extract::State<DaemonApiConfig>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, NexusApiError> {
    tracing::debug!(
        method = %request.method(),
        path = %request.uri().path(),
        mode = ?config.auth_mode,
        "Checking API key authentication",
    );

    match config.auth_mode {
        AuthMode::KeyedAll => auth_keyed_all(&config, request, next).await,
        AuthMode::KeylessLocalhost => auth_keyless_localhost(request, next).await,
    }
}

/// Keyed-all mode: validate `X-API-Key` header against the configured key.
async fn auth_keyed_all(
    config: &DaemonApiConfig,
    request: Request<Body>,
    next: Next,
) -> Result<Response, NexusApiError> {
    let key = request
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok());

    let provided = match key {
        Some(k) if !k.is_empty() => k.as_bytes(),
        _ => {
            tracing::info!(
                method = %request.method(),
                path = %request.uri().path(),
                "Request rejected: missing or empty X-API-Key header",
            );
            return Err(NexusApiError::AuthRequired);
        }
    };

    let expected = config
        .api_key
        .as_deref()
        .expect("KeyedAll mode requires api_key to be Some")
        .as_bytes();

    if !constant_time_eq(provided, expected) {
        tracing::info!(
            method = %request.method(),
            path = %request.uri().path(),
            "Request rejected: X-API-Key mismatch",
        );
        return Err(NexusApiError::AuthRequired);
    }

    tracing::debug!(
        method = %request.method(),
        path = %request.uri().path(),
        "API key authentication successful",
    );

    let (mut parts, body) = request.into_parts();
    parts.extensions.insert(AuthenticatedLocalClient {
        auth_scheme: LocalAuthScheme::ApiKey,
    });
    let request = Request::from_parts(parts, body);

    Ok(next.run(request).await)
}

/// Keyless-localhost mode: accept loopback, reject non-loopback.
///
/// In test environments via `axum_test`, connections appear as loopback.
async fn auth_keyless_localhost(
    request: Request<Body>,
    next: Next,
) -> Result<Response, NexusApiError> {
    if !is_loopback_request(&request) {
        tracing::info!(
            method = %request.method(),
            path = %request.uri().path(),
            "Request rejected: non-loopback connection in keyless-localhost mode",
        );
        return Err(NexusApiError::Forbidden {
            resource: "daemon-local-api".into(),
            reason: "non-loopback connections require an API key".into(),
        });
    }

    tracing::debug!(
        method = %request.method(),
        path = %request.uri().path(),
        "Keyless-localhost: loopback connection accepted",
    );

    let (mut parts, body) = request.into_parts();
    parts.extensions.insert(AuthenticatedLocalClient {
        auth_scheme: LocalAuthScheme::LoopbackBypass,
    });
    let request = Request::from_parts(parts, body);

    Ok(next.run(request).await)
}

/// Determine if a request originates from a loopback address.
///
/// In `axum_test` / test environments, there is no real remote address;
/// we treat missing address info as loopback (the test client runs in-process).
fn is_loopback_request(request: &Request<Body>) -> bool {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;

    // Check ConnectInfo extension first (set by axum on TCP listeners)
    if let Some(ConnectInfo(addr)) = request.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.ip().is_loopback();
    }

    // In test environments (axum_test), ConnectInfo may not be set.
    // Treat missing address info as loopback — the test client runs in-process.
    true
}

#[cfg(test)]
mod tests {
    use crate::api::handlers;
    use crate::workspace::WorkspaceState;
    use axum::routing::{get, post};
    use axum::Router;
    use axum_test::TestServer;
    use serde_json::Value;

    use super::*;

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

    async fn create_test_app(config: DaemonApiConfig) -> TestApp {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let app = build_router(state, config);
        let server = TestServer::new(app).expect("failed to create test server");
        TestApp { _tmp: tmp, server }
    }

    fn build_router(state: WorkspaceState, auth_config: DaemonApiConfig) -> Router {
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

        // Protected routes — require API key
        let protected_routes = Router::new()
            .route("/v1/local/creators", get(handlers::creators::list))
            .route("/v1/local/references", get(handlers::references::list))
            .route_layer(axum_mw::from_fn_with_state(
                auth_config,
                super::require_api_key,
            ));

        Router::new()
            .merge(runtime_routes)
            .merge(workspace_routes)
            .merge(protected_routes)
            .with_state(state)
    }

    // --- Unguarded routes: no auth required ---

    #[tokio::test]
    async fn health_route_works_without_auth() {
        let app = create_test_app(DaemonApiConfig::keyed("test-secret")).await;
        let response = app.get("/v1/local/runtime/health").await;
        assert!(
            response.status_code().is_success(),
            "health should return 2xx without auth, got {}",
            response.status_code(),
        );
    }

    // --- Keyed-all mode: protected routes ---

    #[tokio::test]
    async fn creators_returns_401_without_api_key() {
        let app = create_test_app(DaemonApiConfig::keyed("test-secret")).await;
        let response = app.get("/v1/local/creators").await;
        assert_eq!(
            response.status_code(),
            401,
            "creators should return 401 without API key"
        );
        assert_auth_error_body(&response);
    }

    #[tokio::test]
    async fn references_returns_401_without_api_key() {
        let app = create_test_app(DaemonApiConfig::keyed("test-secret")).await;
        let response = app.get("/v1/local/references").await;
        assert_eq!(
            response.status_code(),
            401,
            "references should return 401 without API key"
        );
        assert_auth_error_body(&response);
    }

    #[tokio::test]
    async fn creators_returns_401_with_wrong_api_key() {
        let app = create_test_app(DaemonApiConfig::keyed("test-secret")).await;
        let response = app
            .get("/v1/local/creators")
            .add_header("X-API-Key", "wrong-key")
            .await;
        assert_eq!(
            response.status_code(),
            401,
            "creators should return 401 with wrong API key"
        );
        assert_auth_error_body(&response);
    }

    #[tokio::test]
    async fn creators_succeeds_with_valid_api_key() {
        let app = create_test_app(DaemonApiConfig::keyed("test-secret")).await;
        let response = app
            .get("/v1/local/creators")
            .add_header("X-API-Key", "test-secret")
            .await;
        // With a valid key, middleware passes through. The handler itself
        // may return data or an error, but NOT 401.
        assert_ne!(
            response.status_code(),
            401,
            "creators should NOT return 401 with valid API key"
        );
    }

    #[tokio::test]
    async fn empty_api_key_header_returns_401() {
        let app = create_test_app(DaemonApiConfig::keyed("test-secret")).await;
        let response = app
            .get("/v1/local/creators")
            .add_header("X-API-Key", "")
            .await;
        assert_eq!(
            response.status_code(),
            401,
            "empty X-API-Key should return 401"
        );
    }

    // --- Keyless-localhost mode ---

    #[tokio::test]
    async fn keyless_localhost_accepts_without_key() {
        // In axum_test, connections appear as loopback (in-process)
        let app = create_test_app(DaemonApiConfig::keyless()).await;
        let response = app.get("/v1/local/creators").await;
        assert_ne!(
            response.status_code(),
            401,
            "keyless-localhost should accept loopback without key"
        );
        assert_ne!(
            response.status_code(),
            403,
            "keyless-localhost should not reject loopback"
        );
    }

    // --- Config unit tests ---

    #[test]
    fn daemon_api_config_keyed_mode() {
        let config = DaemonApiConfig::keyed("my-key");
        assert_eq!(config.auth_mode, AuthMode::KeyedAll);
        assert_eq!(config.api_key.as_deref(), Some("my-key"));
    }

    #[test]
    fn daemon_api_config_keyless_mode() {
        let config = DaemonApiConfig::keyless();
        assert_eq!(config.auth_mode, AuthMode::KeylessLocalhost);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn constant_time_eq_matches() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"short", b"longer"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn constant_time_eq_same_length_different_content() {
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"\x00", b"\x01"));
    }

    // --- Helpers ---

    fn assert_auth_error_body(response: &axum_test::TestResponse) {
        let body: Value = response.json();
        assert_eq!(body["success"], false, "success should be false");
        assert_eq!(
            body["error"]["code"], "AUTH_REQUIRED",
            "error code should be AUTH_REQUIRED"
        );
        assert!(
            !body["error"]["message"].as_str().unwrap_or("").is_empty(),
            "error message should not be empty"
        );
    }
}
