//! Nexus API Error Types
//!
//! Standardized error type for all daemon API handlers.
//! Maps domain errors to proper HTTP status codes with structured JSON responses.
//!
//! # Error Code Design
//!
//! This module follows a two-tier error code strategy:
//!
//! 1. **`error_code()`** returns a **public, stable** error code in UPPER_SNAKE_CASE
//!    (e.g., `"INTERNAL"`, `"INVALID_INPUT"`). These codes are sent to API clients
//!    and must remain stable across versions. They intentionally group error categories
//!    at a coarse level to avoid leaking implementation details.
//!
//! 2. **`Internal.code`** holds an **internal classification** string (e.g., `"INTERNAL_ERROR"`,
//!    `"DATABASE_ERROR"`, `"DATABASE_UNAVAILABLE"`). This field is used for *internal*
//!    debugging, logging, and error categorization — it is NOT exposed as the `error_code`
//!    in the API response body (which always returns `"INTERNAL"` for all `Internal` variants).
//!
//! This design means `Internal.code` and `error_code()` serve different purposes:
//! - `Internal.code` → internal classification (fine-grained, for logging/diagnosis)
//! - `error_code()` → public contract (coarse-grained, for API consumers)
//!
//! If finer-grained public error codes are needed in the future, `error_code()` should be
//! updated to return the specific code rather than a generic one. See also:
//! `crates/nexus-sync/src/errors.rs` for the SyncError pattern which returns variant-specific codes.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Standardized API error response body
#[derive(Debug, Serialize, PartialEq)]
pub struct ApiErrorResponse {
    pub success: bool,
    pub error: ApiErrorDetail,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
}

/// Nexus API Error — unified error type for all daemon endpoints
#[derive(Debug, thiserror::Error)]
pub enum NexusApiError {
    /// Workspace has not been initialized
    #[error("Workspace not initialized")]
    Uninitialized,

    /// Invalid input provided
    #[error("Invalid input: {reason}")]
    InvalidInput { field: String, reason: String },

    /// Internal server error.
    ///
    /// The `code` field provides **internal classification** only (e.g., `"DATABASE_ERROR"`,
    /// `"INTERNAL_ERROR"`, `"DATABASE_UNAVAILABLE"`). It is used for logging and debugging,
    /// not for the public API error code. The public `error_code()` method always returns
    /// `"INTERNAL"` for this variant, intentionally hiding implementation details from
    /// API consumers. See module-level docs for the full rationale.
    #[error("Internal error: {message}")]
    Internal { code: String, message: String },

    /// Authentication required
    #[error("Authentication required")]
    AuthRequired,

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Feature not yet implemented
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Access forbidden (e.g., path outside workspace)
    #[error("Forbidden: {reason}")]
    Forbidden { resource: String, reason: String },

    /// Invalid API key format
    #[error("Invalid API key format")]
    InvalidApiKeyFormat,

    /// API key expired or revoked
    #[error("API key expired or revoked")]
    ApiKeyExpired,

    /// Insufficient permissions for the requested operation
    #[error("Insufficient permissions: required {required:?}")]
    InsufficientPermissions { required: Vec<String> },

    /// Session expired
    #[error("Session expired")]
    SessionExpired,
}

impl NexusApiError {
    /// Get the HTTP status code for this error variant
    pub fn status_code(&self) -> StatusCode {
        match self {
            NexusApiError::Uninitialized => StatusCode::CONFLICT,
            NexusApiError::InvalidInput { .. } => StatusCode::BAD_REQUEST,
            NexusApiError::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            NexusApiError::AuthRequired => StatusCode::UNAUTHORIZED,
            NexusApiError::NotFound(_) => StatusCode::NOT_FOUND,
            NexusApiError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
            NexusApiError::Forbidden { .. } => StatusCode::FORBIDDEN,
            NexusApiError::InvalidApiKeyFormat => StatusCode::BAD_REQUEST,
            NexusApiError::ApiKeyExpired => StatusCode::UNAUTHORIZED,
            NexusApiError::InsufficientPermissions { .. } => StatusCode::FORBIDDEN,
            NexusApiError::SessionExpired => StatusCode::UNAUTHORIZED,
        }
    }

    /// Get the error code string (UPPER_SNAKE_CASE)
    pub fn error_code(&self) -> &str {
        match self {
            NexusApiError::Uninitialized => "UNINITIALIZED",
            NexusApiError::InvalidInput { .. } => "INVALID_INPUT",
            NexusApiError::Internal { .. } => "INTERNAL",
            NexusApiError::AuthRequired => "AUTH_REQUIRED",
            NexusApiError::NotFound(_) => "NOT_FOUND",
            NexusApiError::NotImplemented(_) => "NOT_IMPLEMENTED",
            NexusApiError::Forbidden { .. } => "FORBIDDEN",
            NexusApiError::InvalidApiKeyFormat => "INVALID_API_KEY",
            NexusApiError::ApiKeyExpired => "API_KEY_EXPIRED",
            NexusApiError::InsufficientPermissions { .. } => "INSUFFICIENT_PERMISSIONS",
            NexusApiError::SessionExpired => "SESSION_EXPIRED",
        }
    }

    /// Build the full error response body
    pub fn to_response_body(&self) -> ApiErrorResponse {
        ApiErrorResponse {
            success: false,
            error: ApiErrorDetail {
                code: self.error_code().to_string(),
                message: self.to_string(),
            },
        }
    }
}

impl IntoResponse for NexusApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = self.to_response_body();
        (status, axum::Json(body)).into_response()
    }
}

impl From<anyhow::Error> for NexusApiError {
    fn from(err: anyhow::Error) -> Self {
        let message = match err.chain().collect::<Vec<_>>().as_slice() {
            [] => "unknown error".to_string(),
            [single] => single.to_string(),
            multiple => multiple
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(": "),
        };
        NexusApiError::Internal {
            code: "INTERNAL_ERROR".into(),
            message,
        }
    }
}

impl From<rusqlite::Error> for NexusApiError {
    fn from(err: rusqlite::Error) -> Self {
        NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn uninitialized_maps_to_409() {
        let err = NexusApiError::Uninitialized;
        assert_eq!(err.status_code(), StatusCode::CONFLICT);
        assert_eq!(err.error_code(), "UNINITIALIZED");
    }

    #[test]
    fn invalid_input_maps_to_400() {
        let err = NexusApiError::InvalidInput {
            field: "path".into(),
            reason: "must not be empty".into(),
        };
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(err.error_code(), "INVALID_INPUT");
    }

    #[test]
    fn internal_maps_to_500() {
        let err = NexusApiError::Internal {
            code: "DB_ERROR".into(),
            message: "connection failed".into(),
        };
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.error_code(), "INTERNAL");
    }

    #[test]
    fn auth_required_maps_to_401() {
        let err = NexusApiError::AuthRequired;
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(err.error_code(), "AUTH_REQUIRED");
    }

    #[test]
    fn not_found_maps_to_404() {
        let err = NexusApiError::NotFound("creator".into());
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(err.error_code(), "NOT_FOUND");
    }

    #[test]
    fn response_body_has_expected_structure() {
        let err = NexusApiError::Uninitialized;
        let body = err.to_response_body();
        assert!(!body.success);
        assert_eq!(body.error.code, "UNINITIALIZED");
        assert_eq!(body.error.message, "Workspace not initialized");
    }

    #[test]
    fn from_anyhow_captures_source_chain() {
        let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let anyhow_err = anyhow::anyhow!("workspace init failed").context(inner);
        let api_err: NexusApiError = anyhow_err.into();

        match api_err {
            NexusApiError::Internal { code, message } => {
                assert_eq!(code, "INTERNAL_ERROR");
                assert!(message.contains("workspace init failed"));
                assert!(message.contains("file not found"));
            }
            _ => panic!("Expected Internal variant"),
        }
    }

    #[test]
    fn from_rusqlite_maps_to_internal() {
        let db_err = rusqlite::Error::QueryReturnedNoRows;
        let display = db_err.to_string();
        let api_err: NexusApiError = db_err.into();

        match api_err {
            NexusApiError::Internal { code, message } => {
                assert_eq!(code, "DATABASE_ERROR");
                // The display message should contain the rusqlite error representation
                assert!(!message.is_empty(), "Error message should not be empty");
                assert_eq!(message, display);
            }
            _ => panic!("Expected Internal variant"),
        }
    }

    #[test]
    fn invalid_api_key_format_maps_to_400() {
        let err = NexusApiError::InvalidApiKeyFormat;
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(err.error_code(), "INVALID_API_KEY");
        assert_eq!(err.to_string(), "Invalid API key format");
    }

    #[test]
    fn api_key_expired_maps_to_401() {
        let err = NexusApiError::ApiKeyExpired;
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(err.error_code(), "API_KEY_EXPIRED");
        assert_eq!(err.to_string(), "API key expired or revoked");
    }

    #[test]
    fn insufficient_permissions_maps_to_403() {
        let err = NexusApiError::InsufficientPermissions {
            required: vec!["admin".into(), "write".into()],
        };
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(err.error_code(), "INSUFFICIENT_PERMISSIONS");
        let display = err.to_string();
        assert!(
            display.contains("admin") && display.contains("write"),
            "Display should contain required permissions: {display}"
        );
    }

    #[test]
    fn insufficient_permissions_empty_vec() {
        let err = NexusApiError::InsufficientPermissions {
            required: vec![],
        };
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(err.error_code(), "INSUFFICIENT_PERMISSIONS");
    }

    #[test]
    fn session_expired_maps_to_401() {
        let err = NexusApiError::SessionExpired;
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(err.error_code(), "SESSION_EXPIRED");
        assert_eq!(err.to_string(), "Session expired");
    }

    #[test]
    fn new_auth_variants_response_body_structure() {
        let err = NexusApiError::InvalidApiKeyFormat;
        let body = err.to_response_body();
        assert!(!body.success);
        assert_eq!(body.error.code, "INVALID_API_KEY");
        assert_eq!(body.error.message, "Invalid API key format");

        let err = NexusApiError::ApiKeyExpired;
        let body = err.to_response_body();
        assert!(!body.success);
        assert_eq!(body.error.code, "API_KEY_EXPIRED");
        assert_eq!(body.error.message, "API key expired or revoked");

        let err = NexusApiError::SessionExpired;
        let body = err.to_response_body();
        assert!(!body.success);
        assert_eq!(body.error.code, "SESSION_EXPIRED");
        assert_eq!(body.error.message, "Session expired");
    }

    /// Integration test: POST /v1/local/workspace/init with empty path → 400
    #[tokio::test]
    async fn init_workspace_with_empty_path_returns_400() {
        use crate::api::handlers::workspace::init_workspace;
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::extract::State;
        use axum::Json;

        let (_tmp, nexus_home, db_path) = create_test_workspace();
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None);

        let req = crate::api::handlers::workspace::InitWorkspaceRequest {
            path: "  ".to_string(), // whitespace-only = effectively empty
        };

        let result = init_workspace(State(state), Json(req)).await;

        match result {
            Ok(_) => panic!("Expected error for empty path, got Ok"),
            Err(err) => {
                let status = err.status_code();
                assert_eq!(
                    status,
                    StatusCode::BAD_REQUEST,
                    "Expected 400 for empty path"
                );
                let body = err.to_response_body();
                assert!(!body.success);
                assert_eq!(body.error.code, "INVALID_INPUT");
            }
        }
    }

    /// Integration test: GET /v1/local/creators when no workspace → returns empty list.
    ///
    /// The workspace initialization guard is enforced by middleware (require_workspace),
    /// not by the handler itself. Calling the handler directly without middleware
    /// simply queries the database and returns results.
    #[tokio::test]
    async fn creators_without_workspace_returns_empty_list() {
        use crate::api::handlers::creators::list;
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::extract::State;

        let (_tmp, nexus_home, db_path) = create_test_workspace();
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None);

        let result = list(State(state)).await;
        assert!(
            result.is_ok(),
            "Handler should succeed when called directly (no middleware)"
        );
        let body = result.unwrap();
        assert!(body.creators.is_empty());
    }

    /// Integration test: GET /v1/local/references when no workspace → returns empty list.
    ///
    /// Workspace initialization is enforced by middleware, not by the handler.
    #[tokio::test]
    async fn references_without_workspace_returns_empty_list() {
        use crate::api::handlers::references::list;
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::extract::State;

        let (_tmp, nexus_home, db_path) = create_test_workspace();
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None);

        let result = list(State(state)).await;
        assert!(
            result.is_ok(),
            "Handler should succeed when called directly (no middleware)"
        );
        let body = result.unwrap();
        assert!(body.references.is_empty());
    }

    /// Integration test: GET /v1/local/manuscript when called directly without workspace.
    ///
    /// Workspace initialization is enforced by middleware, not by the handler.
    /// Calling the handler directly (bypassing middleware) returns Ok with None fields
    /// because PooledConn::query_row returns Ok(None) when no rows match.
    ///
    /// Middleware-level rejection is tested in api::middleware::tests.
    #[tokio::test]
    async fn manuscript_without_workspace_returns_ok_when_called_directly() {
        use crate::api::handlers::manuscript::status;
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::extract::State;

        let (_tmp, nexus_home, db_path) = create_test_workspace();
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None);

        let result = status(State(state)).await;
        assert!(
            result.is_ok(),
            "Handler should succeed when called directly (no middleware)"
        );
        let body = result.unwrap();
        assert!(body.phase.is_none());
        assert!(body.active_manifest_id.is_none());
    }
}
