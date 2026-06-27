//! Nexus API Error Types
//!
//! Standardized error type for all daemon API handlers.
//! Maps domain errors to proper HTTP status codes with structured JSON responses.
//!
//! # Error Response Shape (V1.20+)
//!
//! All error responses follow a consistent structure:
//!
//! ```json
//! {
//!   "success": false,
//!   "error": {
//!     "code": "invalid_input",
//!     "message": "Human-readable description",
//!     "details": { "field": "workspace_slug", "reason": "must be a single path segment" },
//!     "request_id": "req_01h..."
//!   }
//! }
//! ```
//!
//! - `success`: always `false` for errors.
//! - `error.code`: stable public lowercase `snake_case` code (V1.67 ratification).
//! - `error.message`: human-readable, safe for CLI/UI display.
//! - `error.details`: optional structured data for field highlighting.
//! - `error.request_id`: correlation ID when request-tracing middleware is active.
//!
//! # Error Code Design
//!
//! This module follows a two-tier error code strategy:
//!
//! 1. **`error_code()`** returns a **public, stable** error code in lowercase `snake_case`
//!    (e.g., `"internal"`, `"invalid_input"`). These codes are sent to API clients
//!    and must remain stable across versions. They intentionally group error categories
//!    at a coarse level to avoid leaking implementation details.
//!
//! 2. **`Internal.code`** holds an **internal classification** string (e.g., `"INTERNAL_ERROR"`,
//!    `"DATABASE_ERROR"`, `"DATABASE_UNAVAILABLE"`). This field is used for *internal*
//!    debugging, logging, and error categorization — it is NOT exposed as the `error_code`
//!    in the API response body (which always returns `"internal"` for all `Internal` variants).
//!
//! This design means `Internal.code` and `error_code()` serve different purposes:
//! - `Internal.code` → internal classification (fine-grained, for logging/diagnosis)
//! - `error_code()` → public contract (coarse-grained, for API consumers)
//!
//! If finer-grained public error codes are needed in the future, `error_code()` should be
//! updated to return the specific code rather than a generic one. See also:
//! `crates/nexus-cloud-sync/src/errors.rs` for the `SyncError` pattern which returns variant-specific codes.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value;

/// Standardized API error response body.
///
/// Every error from the daemon API returns this shape.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ApiErrorResponse {
    pub success: bool,
    pub error: ApiErrorDetail,
}

/// Error detail with optional `details` and `request_id`.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// Request ID extension injected by `attach_request_id` middleware.
///
/// When present, error responses include this ID in `error.request_id`.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

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
    /// `"internal"` for this variant, intentionally hiding implementation details from
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

    /// Resource conflict (e.g., duplicate workspace, completion-lock)
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Resource locked by another process (e.g., `runtime_lock_holder`)
    /// DF-60 §4: HTTP 423 Locked
    #[error("Locked: {reason}")]
    Locked { resource: String, reason: String },

    /// Bad request with code and message (e.g., invalid stage value)
    #[error("Bad request: {message}")]
    BadRequest { code: String, message: String },

    /// Service unavailable (e.g., supervisor/engine not configured).
    #[error("Service unavailable: {message}")]
    ServiceUnavailable { message: String },

    /// Preset gate evaluation failed (semantic validation; HTTP 422).
    #[error("Preset gates failed")]
    PresetGatesFailed { details: serde_json::Value },

    /// Strategy canvas patch conflict (HTTP 409).
    ///
    /// Emitted when the client `base_revision` does not match the current
    /// on-disk revision, allowing the UI to fetch the latest graph and offer
    /// merge/retry affordances.
    #[error("Strategy conflict: {conflicting_path}")]
    StrategyConflict {
        current_revision: u64,
        node_id: String,
        conflicting_path: String,
        recovery_hint: String,
    },

    /// Strategy canvas patch validation failure (HTTP 422).
    ///
    /// Carries the same `validation_summary` shape as a successful patch so
    /// the canvas can surface per-field diagnostics.
    #[error("Strategy validation failed")]
    StrategyValidationFailed { details: serde_json::Value },
}

impl NexusApiError {
    /// Get the HTTP status code for this error variant.
    ///
    /// `BadRequest` with canonical tool-bridge code `policy_blocked` maps to
    /// 403 (spec §12.4: "403 or 409, P4 chooses one consistently").
    #[must_use]
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Uninitialized | Self::Conflict(_) | Self::StrategyConflict { .. } => {
                StatusCode::CONFLICT
            }
            Self::Locked { .. } => StatusCode::LOCKED,
            Self::InvalidInput { .. } | Self::InvalidApiKeyFormat => StatusCode::BAD_REQUEST,
            Self::ServiceUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::PresetGatesFailed { .. } | Self::StrategyValidationFailed { .. } => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::BadRequest { code, .. } => {
                match code.as_str() {
                    "policy_blocked" => StatusCode::FORBIDDEN,
                    // V1.40: world_id_required and invalid_world_id are semantic
                    // validation errors → 422 Unprocessable Entity (aligned with
                    // preset_gates_failed pattern per spec §3.5.1.2).
                    "world_id_required" | "invalid_world_id" | "world_clear_forbidden" => {
                        StatusCode::UNPROCESSABLE_ENTITY
                    }
                    // V1.49 F6 (findings-lifecycle.md §2.1): illegal lifecycle
                    // transitions return 422 with the stable `invalid_transition`
                    // code so callers can distinguish "no such finding" (404)
                    // from "finding exists but the move is not allowed".
                    // V1.49 P0 W-1: invalid PATCH enum values (severity /
                    // status membership / target_executor) return 422 with the
                    // stable `invalid_input` code, distinct from transitions.
                    "invalid_transition" | "invalid_input" => StatusCode::UNPROCESSABLE_ENTITY,
                    // V1.65: chapter bodies above the size cap return 413.
                    "chapter_body_too_large" => StatusCode::PAYLOAD_TOO_LARGE,
                    _ => StatusCode::BAD_REQUEST,
                }
            }
            Self::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::AuthRequired | Self::ApiKeyExpired | Self::SessionExpired => {
                StatusCode::UNAUTHORIZED
            }
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
            Self::Forbidden { .. } | Self::InsufficientPermissions { .. } => StatusCode::FORBIDDEN,
        }
    }

    /// Get the error code string (lowercase `snake_case`).
    ///
    /// For `BadRequest`, the inner `code` is returned when it matches one of
    /// the tool-bridge canonical codes (spec §12.4), so that HTTP and worker
    /// wire responses surface the *specific* code (e.g. `policy_blocked`,
    /// `not_supported`, `invalid_input`) instead of the generic `bad_request`.
    #[must_use]
    pub fn error_code(&self) -> &str {
        match self {
            Self::Uninitialized => "uninitialized",
            Self::InvalidInput { .. } => "invalid_input",
            Self::Internal { .. } => "internal",
            Self::AuthRequired => "auth_required",
            Self::NotFound(_) => "not_found",
            Self::NotImplemented(_) => "not_implemented",
            Self::Forbidden { .. } => "forbidden",
            Self::InvalidApiKeyFormat => "invalid_api_key",
            Self::ApiKeyExpired => "api_key_expired",
            Self::InsufficientPermissions { .. } => "insufficient_permissions",
            Self::BadRequest { code, .. } => {
                // Surface canonical tool-bridge codes (spec §12.4), plus
                // V1.40 world-binding and V1.49 F6 lifecycle codes, as-is.
                // V1.67 F-F1: resource-specific sort-invalid codes are also public.
                match code.as_str() {
                    "policy_blocked"
                    | "not_supported"
                    | "invalid_input"
                    | "invalid_transition"
                    | "world_id_required"
                    | "invalid_world_id"
                    | "world_clear_forbidden" => code.as_str(),
                    _ if code.ends_with("_sort_invalid") => code.as_str(),
                    _ => "bad_request",
                }
            }
            Self::StrategyConflict { .. } => "strategy_conflict",
            Self::StrategyValidationFailed { .. } => "strategy_validation_failed",
            Self::SessionExpired => "session_expired",
            Self::Conflict(_) => "conflict",
            Self::Locked { .. } => "locked",
            Self::ServiceUnavailable { .. } => "service_unavailable",
            Self::PresetGatesFailed { .. } => "preset_gates_failed",
        }
    }

    /// Build structured `details` from the error variant.
    ///
    /// Only variants that carry structured field data produce non-`None` details.
    #[must_use]
    pub fn error_details(&self) -> Option<Value> {
        match self {
            Self::InvalidInput { field, reason } => Some(serde_json::json!({
                "field": field,
                "reason": reason,
            })),
            Self::Forbidden { resource, reason } | Self::Locked { resource, reason } => {
                Some(serde_json::json!({
                    "resource": resource,
                    "reason": reason,
                }))
            }
            Self::PresetGatesFailed { details } | Self::StrategyValidationFailed { details } => {
                Some(details.clone())
            }
            Self::StrategyConflict {
                current_revision,
                node_id,
                conflicting_path,
                recovery_hint,
            } => Some(serde_json::json!({
                "current_revision": current_revision,
                "node_id": node_id,
                "conflicting_path": conflicting_path,
                "recovery_hint": recovery_hint,
            })),
            _ => None,
        }
    }

    /// Build a `service_unavailable` error from a message.
    #[must_use]
    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self::ServiceUnavailable {
            message: message.into(),
        }
    }

    /// Build a `strategy_conflict` error with structured recovery details.
    #[must_use]
    pub fn strategy_conflict(
        current_revision: u64,
        node_id: impl Into<String>,
        conflicting_path: impl Into<String>,
        recovery_hint: impl Into<String>,
    ) -> Self {
        Self::StrategyConflict {
            current_revision,
            node_id: node_id.into(),
            conflicting_path: conflicting_path.into(),
            recovery_hint: recovery_hint.into(),
        }
    }

    /// Build a `strategy_validation_failed` error from a validation summary.
    #[must_use]
    pub fn strategy_validation_failed(errors: &[String], warnings: &[String]) -> Self {
        Self::StrategyValidationFailed {
            details: serde_json::json!({
                "validation_summary": { "errors": errors, "warnings": warnings },
            }),
        }
    }

    /// Build a `preset_gates_failed` error from structured gate-failure details.
    #[must_use]
    pub fn preset_gates_failed(
        failure: &nexus_orchestration::preset_gates::PresetGatesFailed,
    ) -> Self {
        Self::PresetGatesFailed {
            details: serde_json::to_value(failure).unwrap_or_else(|_| serde_json::json!({})),
        }
    }

    /// Build the full error response body.
    #[must_use]
    pub fn to_response_body(&self) -> ApiErrorResponse {
        ApiErrorResponse {
            success: false,
            error: ApiErrorDetail {
                code: self.error_code().to_string(),
                message: self.to_string(),
                details: self.error_details(),
                request_id: None,
            },
        }
    }

    /// Build the full error response body with a request ID attached.
    #[must_use]
    pub fn to_response_body_with_request_id(&self, request_id: &str) -> ApiErrorResponse {
        let mut body = self.to_response_body();
        body.error.request_id = Some(request_id.to_string());
        body
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
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(": "),
        };
        Self::Internal {
            code: "INTERNAL_ERROR".into(),
            message,
        }
    }
}

impl From<sqlx::Error> for NexusApiError {
    fn from(err: sqlx::Error) -> Self {
        Self::Internal {
            code: "DATABASE_ERROR".into(),
            message: err.to_string(),
        }
    }
}

impl From<nexus_local_db::LocalDbError> for NexusApiError {
    fn from(err: nexus_local_db::LocalDbError) -> Self {
        Self::Internal {
            code: "DATABASE_ERROR".into(),
            message: err.to_string(),
        }
    }
}

// Note: These tests remain inline because they use `crate::test_utils::create_test_workspace`,
// which is a private test-only helper. Integration tests in `tests/` cannot access
// `#[cfg(test)]` modules. Consider extracting the pure unit tests (error mapping logic)
// to `tests/` once a public test fixture helper is added.
#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn uninitialized_maps_to_409() {
        let err = NexusApiError::Uninitialized;
        assert_eq!(err.status_code(), StatusCode::CONFLICT);
        assert_eq!(err.error_code(), "uninitialized");
    }

    #[test]
    fn invalid_input_maps_to_400() {
        let err = NexusApiError::InvalidInput {
            field: "path".into(),
            reason: "must not be empty".into(),
        };
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(err.error_code(), "invalid_input");
    }

    #[test]
    fn internal_maps_to_500() {
        let err = NexusApiError::Internal {
            code: "DB_ERROR".into(),
            message: "connection failed".into(),
        };
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.error_code(), "internal");
    }

    #[test]
    fn auth_required_maps_to_401() {
        let err = NexusApiError::AuthRequired;
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(err.error_code(), "auth_required");
    }

    #[test]
    fn not_found_maps_to_404() {
        let err = NexusApiError::NotFound("creator".into());
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(err.error_code(), "not_found");
    }

    #[test]
    fn response_body_has_expected_structure() {
        let err = NexusApiError::Uninitialized;
        let body = err.to_response_body();
        assert!(!body.success);
        assert_eq!(body.error.code, "uninitialized");
        assert_eq!(body.error.message, "Workspace not initialized");
        assert!(body.error.details.is_none());
        assert!(body.error.request_id.is_none());
    }

    #[test]
    fn response_body_includes_details_for_invalid_input() {
        let err = NexusApiError::InvalidInput {
            field: "workspace_slug".to_string(),
            reason: "must be a single path segment".to_string(),
        };
        let body = err.to_response_body();
        assert!(!body.success);
        assert_eq!(body.error.code, "invalid_input");
        let details = body.error.details.expect("details should be present");
        assert_eq!(details["field"], "workspace_slug");
        assert_eq!(details["reason"], "must be a single path segment");
    }

    #[test]
    fn response_body_includes_details_for_forbidden() {
        let err = NexusApiError::Forbidden {
            resource: "daemon-local-api".to_string(),
            reason: "non-loopback connections require an API key".to_string(),
        };
        let body = err.to_response_body();
        let details = body.error.details.expect("details should be present");
        assert_eq!(details["resource"], "daemon-local-api");
        assert_eq!(
            details["reason"],
            "non-loopback connections require an API key"
        );
    }

    #[test]
    fn response_body_with_request_id() {
        let err = NexusApiError::Uninitialized;
        let body = err.to_response_body_with_request_id("req_abc123");
        assert_eq!(body.error.request_id.as_deref(), Some("req_abc123"));
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
    fn from_sqlx_maps_to_internal() {
        let db_err = sqlx::Error::RowNotFound;
        let display = db_err.to_string();
        let api_err: NexusApiError = db_err.into();

        match api_err {
            NexusApiError::Internal { code, message } => {
                assert_eq!(code, "DATABASE_ERROR");
                // The display message should contain the sqlx error representation
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
        assert_eq!(err.error_code(), "invalid_api_key");
        assert_eq!(err.to_string(), "Invalid API key format");
    }

    #[test]
    fn api_key_expired_maps_to_401() {
        let err = NexusApiError::ApiKeyExpired;
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(err.error_code(), "api_key_expired");
        assert_eq!(err.to_string(), "API key expired or revoked");
    }

    #[test]
    fn insufficient_permissions_maps_to_403() {
        let err = NexusApiError::InsufficientPermissions {
            required: vec!["admin".into(), "write".into()],
        };
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(err.error_code(), "insufficient_permissions");
        let display = err.to_string();
        assert!(
            display.contains("admin") && display.contains("write"),
            "Display should contain required permissions: {display}"
        );
    }

    #[test]
    fn insufficient_permissions_empty_vec() {
        let err = NexusApiError::InsufficientPermissions { required: vec![] };
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(err.error_code(), "insufficient_permissions");
    }

    #[test]
    fn session_expired_maps_to_401() {
        let err = NexusApiError::SessionExpired;
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(err.error_code(), "session_expired");
        assert_eq!(err.to_string(), "Session expired");
    }

    #[test]
    fn new_auth_variants_response_body_structure() {
        let err = NexusApiError::InvalidApiKeyFormat;
        let body = err.to_response_body();
        assert!(!body.success);
        assert_eq!(body.error.code, "invalid_api_key");
        assert_eq!(body.error.message, "Invalid API key format");

        let err = NexusApiError::ApiKeyExpired;
        let body = err.to_response_body();
        assert!(!body.success);
        assert_eq!(body.error.code, "api_key_expired");
        assert_eq!(body.error.message, "API key expired or revoked");

        let err = NexusApiError::SessionExpired;
        let body = err.to_response_body();
        assert!(!body.success);
        assert_eq!(body.error.code, "session_expired");
        assert_eq!(body.error.message, "Session expired");
    }
    #[test]
    fn service_unavailable_maps_to_503() {
        let err = NexusApiError::service_unavailable("engine not available");
        assert_eq!(err.status_code(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(err.error_code(), "service_unavailable");
        assert_eq!(err.to_string(), "Service unavailable: engine not available");
    }

    #[test]
    fn preset_gates_failed_maps_to_422() {
        let failure = nexus_orchestration::preset_gates::PresetGatesFailed {
            error: "preset_gates_failed".to_string(),
            preset_id: "novel-writing".to_string(),
            work_id: "wrk_123".to_string(),
            failed_gates: vec![],
        };
        let err = NexusApiError::preset_gates_failed(&failure);
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.error_code(), "preset_gates_failed");
        let body = err.to_response_body();
        assert_eq!(body.error.details.unwrap()["preset_id"], "novel-writing");
    }

    #[test]
    fn sort_invalid_codes_are_public() {
        for code in [
            "work_sort_invalid",
            "schedule_sort_invalid",
            "session_sort_invalid",
            "capability_sort_invalid",
        ] {
            let err = NexusApiError::BadRequest {
                code: code.into(),
                message: "bad sort".into(),
            };
            assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
            assert_eq!(err.error_code(), code);
            assert_eq!(err.to_response_body().error.code, code);
        }
    }

    #[test]
    fn bad_request_passthrough_codes_are_lowercase() {
        let err = NexusApiError::BadRequest {
            code: "policy_blocked".into(),
            message: "denied".into(),
        };
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(err.error_code(), "policy_blocked");

        let err = NexusApiError::BadRequest {
            code: "not_supported".into(),
            message: "no".into(),
        };
        assert_eq!(err.error_code(), "not_supported");

        let err = NexusApiError::BadRequest {
            code: "invalid_transition".into(),
            message: "bad".into(),
        };
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.error_code(), "invalid_transition");

        let err = NexusApiError::BadRequest {
            code: "world_id_required".into(),
            message: "missing".into(),
        };
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.error_code(), "world_id_required");
    }

    /// Integration test: POST /v1/local/workspace/init with empty path → 400
    #[tokio::test]
    async fn init_workspace_with_empty_path_returns_400() {
        use crate::api::handlers::workspace::init_workspace;
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::extract::State;
        use axum::Json;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

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
                assert_eq!(body.error.code, "invalid_input");
            }
        }
    }

    /// Integration test: GET /v1/local/creators when no workspace → returns empty list.
    ///
    /// The workspace initialization guard is enforced by middleware (`require_workspace`),
    /// not by the handler itself. Calling the handler directly without middleware
    /// simply queries the database and returns results.
    #[tokio::test]
    async fn creators_without_workspace_returns_empty_list() {
        use crate::api::handlers::creators::list;
        use crate::api::handlers::creators::ListCreatorsQuery;
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;
        use axum::extract::State;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let query = ListCreatorsQuery {
            limit: 50,
            cursor: None,
        };
        let result = list(State(state), axum::extract::Query(query)).await;
        assert!(
            result.is_ok(),
            "Handler should succeed when called directly (no middleware)"
        );
        let body = result.expect("result should be Ok");
        assert!(body.items.is_empty());
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

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = list(State(state)).await;
        assert!(
            result.is_ok(),
            "Handler should succeed when called directly (no middleware)"
        );
        let body = result.expect("result should be Ok");
        assert!(body.references.is_empty());
    }
}
