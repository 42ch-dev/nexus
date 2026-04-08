//! Auth Middleware
//!
//! Tower/axum middleware layer for authentication enforcement.
//! Validates `Authorization: Bearer <token>` against stored tokens in SQLite.
//! Applied to protected daemon routes.

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;

/// Extension containing the authenticated user identity.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
}

/// Auth status response for the status endpoint (includes expiry info).
#[derive(Debug, serde::Serialize)]
pub struct TokenStatusResponse {
    pub authenticated: bool,
    pub user_id: Option<String>,
    pub expires_at: Option<String>,
    pub needs_refresh: bool,
}

/// Extracted Bearer token from request, stored in extensions by `require_auth`.
#[derive(Debug, Clone)]
pub struct BearerToken(pub String);

/// Require authentication middleware.
///
/// Extracts `Authorization: Bearer <token>` header, validates against stored
/// tokens in the daemon's SQLite database, and injects `AuthenticatedUser`.
///
/// # Tracing
/// - `debug` on entry/exit for every request
/// - `info` on rejection (auth required)
pub async fn require_auth(
    axum::extract::State(state): axum::extract::State<WorkspaceState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, NexusApiError> {
    tracing::debug!(
        method = %request.method(),
        path = %request.uri().path(),
        "Checking authentication",
    );

    // Extract Authorization header
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with("Bearer ") => &header[7..],
        _ => {
            tracing::info!(
                method = %request.method(),
                path = %request.uri().path(),
                "Request rejected: missing or invalid Authorization header",
            );
            return Err(NexusApiError::AuthRequired);
        }
    };

    if token.is_empty() {
        tracing::info!(
            method = %request.method(),
            path = %request.uri().path(),
            "Request rejected: empty Bearer token",
        );
        return Err(NexusApiError::AuthRequired);
    }

    // Validate token against stored tokens
    let token_manager = crate::auth::token_manager::TokenManager::new(state.db_pool());
    let valid = token_manager
        .validate_token(token)
        .await
        .map_err(|_| NexusApiError::AuthRequired)?;

    if !valid {
        tracing::info!(
            method = %request.method(),
            path = %request.uri().path(),
            "Request rejected: invalid or expired token",
        );
        return Err(NexusApiError::AuthRequired);
    }

    // Get user_id and inject AuthenticatedUser
    let stored = token_manager
        .get_valid_token()
        .await
        .map_err(|_| NexusApiError::AuthRequired)?;

    let stored = stored.ok_or(NexusApiError::AuthRequired)?;

    tracing::debug!(
        method = %request.method(),
        path = %request.uri().path(),
        user_id = %stored.user_id,
        "Authentication successful",
    );

    let token_owned = token.to_string();
    let (mut parts, body) = request.into_parts();
    parts.extensions.insert(BearerToken(token_owned));
    parts.extensions.insert(AuthenticatedUser {
        user_id: stored.user_id,
    });
    let request = Request::from_parts(parts, body);

    Ok(next.run(request).await)
}

// Note: These tests remain inline because they use `crate::auth::token_manager::TokenManager`
// and other internal crate items. Integration tests in `tests/` can only access the
// crate's public API. The tests also rely on private test fixture helpers seeded in
// the database. Consider extracting to `tests/` once TokenManager and related auth
// internals are exposed publicly, or a public test fixture helper is added.
#[cfg(test)]
mod tests {
    use crate::api::handlers;
    use crate::db::schema::Schema;
    use crate::workspace::WorkspaceState;
    use axum::routing::{get, post};
    use axum::Router;
    use axum_test::TestServer;
    use chrono::Utc;
    use serde_json::Value;

    struct TestApp {
        _tmp: tempfile::TempDir,
        server: TestServer,
        state: WorkspaceState,
    }

    impl std::ops::Deref for TestApp {
        type Target = TestServer;
        fn deref(&self) -> &TestServer {
            &self.server
        }
    }

    fn create_test_app() -> TestApp {
        let tmp = tempfile::TempDir::new().unwrap();
        let nexus_home = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_home).unwrap();
        let db_path = nexus_home.join("state.db");

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        Schema::init(&conn).unwrap();
        drop(conn);

        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None);
        let app = build_router(state.clone());
        let server = TestServer::new(app).unwrap();
        TestApp {
            _tmp: tmp,
            server,
            state,
        }
    }

    fn build_router(state: WorkspaceState) -> Router {
        use axum::middleware as axum_mw;

        let runtime_routes = Router::new()
            .route("/v1/local/runtime/health", get(handlers::runtime::health))
            .route("/v1/local/runtime/status", get(handlers::runtime::status));

        let workspace_routes = Router::new()
            .route("/v1/local/workspace", get(handlers::workspace::info))
            .route(
                "/v1/local/workspace/init",
                post(handlers::workspace::init_workspace),
            );

        let auth_routes = Router::new()
            .route("/v1/local/auth/status", get(handlers::auth::status))
            .route(
                "/v1/local/auth/device",
                post(handlers::auth::device_authorization),
            )
            .route("/v1/local/auth/token", post(handlers::auth::exchange_token))
            .route("/v1/local/auth/logout", post(handlers::auth::logout));

        // Auth-protected routes (require valid Bearer token)
        let creator_routes = Router::new()
            .route("/v1/local/creators", get(handlers::creators::list))
            .route_layer(axum_mw::from_fn_with_state(
                state.clone(),
                super::require_auth,
            ));

        let manuscript_routes = Router::new()
            .route("/v1/local/manuscript", get(handlers::manuscript::status))
            .route_layer(axum_mw::from_fn_with_state(
                state.clone(),
                super::require_auth,
            ));

        let reference_routes = Router::new()
            .route("/v1/local/references", get(handlers::references::list))
            .route_layer(axum_mw::from_fn_with_state(
                state.clone(),
                super::require_auth,
            ));

        let context_routes = Router::new()
            .route(
                "/v1/local/context/assemble",
                post(handlers::context::assemble),
            )
            .route_layer(axum_mw::from_fn_with_state(
                state.clone(),
                super::require_auth,
            ));

        Router::new()
            .merge(runtime_routes)
            .merge(workspace_routes)
            .merge(auth_routes)
            .merge(creator_routes)
            .merge(manuscript_routes)
            .merge(reference_routes)
            .merge(context_routes)
            .with_state(state)
    }

    // --- Unguarded routes: no auth required ---

    #[tokio::test]
    async fn health_route_works_without_auth() {
        let app = create_test_app();
        let response = app.get("/v1/local/runtime/health").await;
        assert!(
            response.status_code().is_success(),
            "health should return 2xx without auth, got {}",
            response.status_code(),
        );
    }

    #[tokio::test]
    async fn auth_status_works_without_auth() {
        let app = create_test_app();
        let response = app.get("/v1/local/auth/status").await;
        assert!(
            response.status_code().is_success(),
            "auth status should return 2xx without auth, got {}",
            response.status_code(),
        );
    }

    #[tokio::test]
    async fn auth_device_works_without_auth() {
        let app = create_test_app();
        let response = app
            .post("/v1/local/auth/device")
            .json(&serde_json::json!({}))
            .await;
        assert!(
            response.status_code().is_success(),
            "auth device should return 2xx without auth, got {}",
            response.status_code(),
        );
    }

    // --- Auth-protected routes: should reject without token ---

    #[tokio::test]
    async fn creators_returns_401_without_token() {
        let app = create_test_app();
        let response = app.get("/v1/local/creators").await;
        assert_eq!(
            response.status_code(),
            401,
            "creators should return 401 without auth token"
        );
        assert_auth_error_body(&response);
    }

    #[tokio::test]
    async fn manuscript_returns_401_without_token() {
        let app = create_test_app();
        let response = app.get("/v1/local/manuscript").await;
        assert_eq!(
            response.status_code(),
            401,
            "manuscript should return 401 without auth token"
        );
        assert_auth_error_body(&response);
    }

    #[tokio::test]
    async fn references_returns_401_without_token() {
        let app = create_test_app();
        let response = app.get("/v1/local/references").await;
        assert_eq!(
            response.status_code(),
            401,
            "references should return 401 without auth token"
        );
        assert_auth_error_body(&response);
    }

    // --- Auth-protected routes: should reject with invalid token ---

    #[tokio::test]
    async fn creators_returns_401_with_invalid_token() {
        let app = create_test_app();
        let response = app
            .get("/v1/local/creators")
            .add_header("Authorization", "Bearer invalid-token")
            .await;
        assert_eq!(
            response.status_code(),
            401,
            "creators should return 401 with invalid token"
        );
    }

    // --- Auth-protected routes: should succeed with valid token ---

    #[tokio::test]
    async fn creators_succeeds_with_valid_token() {
        let app = create_test_app();

        // Seed a valid token in the database
        seed_valid_token(&app.state, "usr_test", "valid-access-token").await;

        let response = app
            .get("/v1/local/creators")
            .add_header("Authorization", "Bearer valid-access-token")
            .await;
        // With a valid token, middleware passes through. The handler itself
        // may return data or an error, but NOT 401.
        assert_ne!(
            response.status_code(),
            401,
            "creators should NOT return 401 with valid token"
        );
    }

    // --- Edge cases ---

    #[tokio::test]
    async fn empty_bearer_token_returns_401() {
        let app = create_test_app();
        let response = app
            .get("/v1/local/creators")
            .add_header("Authorization", "Bearer ")
            .await;
        assert_eq!(
            response.status_code(),
            401,
            "empty Bearer token should return 401"
        );
    }

    #[tokio::test]
    async fn malformed_auth_header_returns_401() {
        let app = create_test_app();
        let response = app
            .get("/v1/local/creators")
            .add_header("Authorization", "Basic dXNlcjpwYXNz")
            .await;
        assert_eq!(
            response.status_code(),
            401,
            "non-Bearer auth scheme should return 401"
        );
    }

    #[tokio::test]
    async fn expired_token_returns_401() {
        let app = create_test_app();

        // Seed an expired token
        seed_expired_token(&app.state, "usr_expired", "expired-token").await;

        let response = app
            .get("/v1/local/creators")
            .add_header("Authorization", "Bearer expired-token")
            .await;
        assert_eq!(
            response.status_code(),
            401,
            "expired token should return 401"
        );
    }

    // --- Helpers ---

    async fn seed_valid_token(state: &WorkspaceState, user_id: &str, access_token: &str) {
        let conn = state.db().await.unwrap();
        let expires_at = (Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        let created_at = Utc::now().to_rfc3339();
        let user_id = user_id.to_string();
        let access_token = access_token.to_string();

        conn.interact(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO auth_tokens (user_id, access_token, refresh_token, expires_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                [&user_id, &access_token, "rt_mock", &expires_at, &created_at],
            )
        })
        .await
        .unwrap()
        .unwrap();
    }

    async fn seed_expired_token(state: &WorkspaceState, user_id: &str, access_token: &str) {
        let conn = state.db().await.unwrap();
        let expires_at = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let created_at = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        let user_id = user_id.to_string();
        let access_token = access_token.to_string();

        conn.interact(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO auth_tokens (user_id, access_token, refresh_token, expires_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                [&user_id, &access_token, "rt_mock", &expires_at, &created_at],
            )
        })
        .await
        .unwrap()
        .unwrap();
    }

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
