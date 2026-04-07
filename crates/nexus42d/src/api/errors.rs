//! Nexus API Error Types
//!
//! Standardized error type for all daemon API handlers.
//! Maps domain errors to proper HTTP status codes with structured JSON responses.

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

    /// Internal server error
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

    /// Integration test: POST /v1/local/workspace/init with empty path → 400
    #[tokio::test]
    async fn init_workspace_with_empty_path_returns_400() {
        use crate::api::handlers::workspace::init_workspace;
        use crate::workspace::WorkspaceState;
        use axum::extract::State;
        use axum::Json;

        let tmp = tempfile::TempDir::new().unwrap();
        let nexus_home = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_home).unwrap();
        let db_path = nexus_home.join("state.db");

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        crate::db::schema::Schema::init(&conn).unwrap();
        drop(conn);

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
        use crate::workspace::WorkspaceState;
        use axum::extract::State;

        let tmp = tempfile::TempDir::new().unwrap();
        let nexus_home = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_home).unwrap();
        let db_path = nexus_home.join("state.db");

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        crate::db::schema::Schema::init(&conn).unwrap();
        drop(conn);

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
        use crate::workspace::WorkspaceState;
        use axum::extract::State;

        let tmp = tempfile::TempDir::new().unwrap();
        let nexus_home = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_home).unwrap();
        let db_path = nexus_home.join("state.db");

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        crate::db::schema::Schema::init(&conn).unwrap();
        drop(conn);

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
        use crate::workspace::WorkspaceState;
        use axum::extract::State;

        let tmp = tempfile::TempDir::new().unwrap();
        let nexus_home = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_home).unwrap();
        let db_path = nexus_home.join("state.db");

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        crate::db::schema::Schema::init(&conn).unwrap();
        drop(conn);

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
