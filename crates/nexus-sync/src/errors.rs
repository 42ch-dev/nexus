//! Nexus Sync Errors
//!
//! Sync-layer error types for outbox, bundle building, sync client,
//! conflict resolution, and precheck operations.
//! Uses `thiserror` following the domain layer pattern.
//!
//! # Error Code Strategy (DEBT-X3)
//!
//! Each `SyncError` variant has a standardized error code (UPPER_SNAKE_CASE)
//! matching the pattern used by `NexusApiError` in the daemon layer.
//!
//! Error codes are used for:
//! - Structured logging and monitoring
//! - Client-side error categorization
//! - Mapping to HTTP status codes when crossing layer boundaries

use thiserror::Error;

/// Sync error type covering all sync-layer operations.
#[derive(Debug, Error)]
pub enum SyncError {
    // ── Outbox errors ──────────────────────────────────────────────
    /// Failed to open or initialize the SQLite outbox database.
    #[error("outbox database error: {0}")]
    OutboxDatabase(String),

    /// Outbox entry not found by the given ID.
    #[error("outbox entry not found: {id}")]
    OutboxEntryNotFound { id: String },

    /// Outbox entry is in an unexpected state for this operation.
    #[error("outbox invalid state transition: expected {expected}, got {actual}")]
    OutboxInvalidState { expected: String, actual: String },

    /// Outbox entry has exceeded the maximum retry count.
    #[error("outbox max retries exceeded: {id} (retried {retries} times)")]
    OutboxMaxRetriesExceeded { id: String, retries: u64 },

    // ── Bundle builder errors ──────────────────────────────────────
    /// Bundle validation failed.
    #[error("bundle validation failed: {0}")]
    BundleValidation(String),

    /// A required bundle field is missing.
    #[error("bundle missing required field: {field}")]
    BundleMissingField { field: String },

    /// Delta sequence is not monotonic.
    #[error("bundle delta sequence not monotonic: expected >= {expected}, got {actual}")]
    BundleSequenceNotMonotonic { expected: u64, actual: u64 },

    /// Bundle has no deltas.
    #[error("bundle must contain at least one delta")]
    BundleEmptyDeltas,

    // ── Sync client errors ────────────────────────────────────────
    /// HTTP request to the platform failed.
    #[error("sync HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    /// Platform returned a non-success status code.
    #[error("platform error: status {status}, body: {body}")]
    PlatformError { status: u16, body: String },

    /// Platform returned a conflict response.
    #[error("sync conflict: {conflict_type}")]
    SyncConflict {
        conflict_type: String,
        /// The full parsed conflict response, if available (SYNC-R11).
        /// Allows callers to access `retry_after` and other fields.
        response: Option<Box<crate::conflict::ConflictResponse>>,
    },

    /// Sync client is not configured (missing base URL or auth token).
    #[error("sync client not configured: {0}")]
    SyncNotConfigured(String),

    /// Sync request timed out.
    #[error("sync request timed out after {seconds}s")]
    SyncTimeout { seconds: u64 },

    // ── Conflict resolution errors ────────────────────────────────
    /// Conflict cannot be automatically resolved.
    #[error("unresolvable conflict: {0}")]
    UnresolvableConflict(String),

    /// Manual review required for a conflict.
    #[error("manual review required: {0}")]
    ManualReviewRequired(String),

    // ── Partial apply errors ──────────────────────────────────────
    /// Partial apply state is corrupted.
    #[error("partial apply state corrupted: {0}")]
    PartialApplyStateError(String),

    /// All deltas in the bundle failed.
    #[error("all deltas failed: {failed_count} of {total_count}")]
    AllDeltasFailed {
        failed_count: usize,
        total_count: usize,
    },

    // ── Precheck errors ───────────────────────────────────────────
    /// Local precheck failed before upload.
    #[error("precheck failed: {0}")]
    PrecheckFailed(String),

    /// Precheck detected a version mismatch with local state.
    #[error("precheck version mismatch: expected revision {expected}, got {actual}")]
    PrecheckVersionMismatch { expected: u64, actual: u64 },

    /// Precheck detected inconsistent commands.
    #[error("precheck command inconsistency: {0}")]
    PrecheckCommandInconsistency(String),

    /// Precheck detected schema non-compliance.
    #[error("precheck schema violation: {0}")]
    PrecheckSchemaViolation(String),

    // ── Serialization errors ──────────────────────────────────────
    /// JSON serialization/deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl SyncError {
    /// Get the error code string (UPPER_SNAKE_CASE)
    ///
    /// Error codes are standardized across the nexus codebase for
    /// consistent error tracking and monitoring.
    pub fn error_code(&self) -> &str {
        match self {
            // Outbox errors
            SyncError::OutboxDatabase(_) => "OUTBOX_DATABASE_ERROR",
            SyncError::OutboxEntryNotFound { .. } => "OUTBOX_ENTRY_NOT_FOUND",
            SyncError::OutboxInvalidState { .. } => "OUTBOX_INVALID_STATE",
            SyncError::OutboxMaxRetriesExceeded { .. } => "OUTBOX_MAX_RETRIES_EXCEEDED",

            // Bundle errors
            SyncError::BundleValidation(_) => "BUNDLE_VALIDATION_ERROR",
            SyncError::BundleMissingField { .. } => "BUNDLE_MISSING_FIELD",
            SyncError::BundleSequenceNotMonotonic { .. } => "BUNDLE_SEQUENCE_NOT_MONOTONIC",
            SyncError::BundleEmptyDeltas => "BUNDLE_EMPTY_DELTAS",

            // Sync client errors
            SyncError::HttpError(_) => "HTTP_ERROR",
            SyncError::PlatformError { .. } => "PLATFORM_ERROR",
            SyncError::SyncConflict { .. } => "SYNC_CONFLICT",
            SyncError::SyncNotConfigured(_) => "SYNC_NOT_CONFIGURED",
            SyncError::SyncTimeout { .. } => "SYNC_TIMEOUT",

            // Conflict resolution errors
            SyncError::UnresolvableConflict(_) => "UNRESOLVABLE_CONFLICT",
            SyncError::ManualReviewRequired(_) => "MANUAL_REVIEW_REQUIRED",

            // Partial apply errors
            SyncError::PartialApplyStateError(_) => "PARTIAL_APPLY_STATE_ERROR",
            SyncError::AllDeltasFailed { .. } => "ALL_DELTAS_FAILED",

            // Precheck errors
            SyncError::PrecheckFailed(_) => "PRECHECK_FAILED",
            SyncError::PrecheckVersionMismatch { .. } => "PRECHECK_VERSION_MISMATCH",
            SyncError::PrecheckCommandInconsistency(_) => "PRECHECK_COMMAND_INCONSISTENCY",
            SyncError::PrecheckSchemaViolation(_) => "PRECHECK_SCHEMA_VIOLATION",

            // Serialization errors
            SyncError::Serialization(_) => "SERIALIZATION_ERROR",
        }
    }
}

impl From<serde_json::Error> for SyncError {
    fn from(err: serde_json::Error) -> Self {
        SyncError::Serialization(err.to_string())
    }
}

impl From<rusqlite::Error> for SyncError {
    fn from(err: rusqlite::Error) -> Self {
        SyncError::OutboxDatabase(err.to_string())
    }
}

impl From<deadpool_sqlite::BuildError> for SyncError {
    fn from(err: deadpool_sqlite::BuildError) -> Self {
        SyncError::OutboxDatabase(err.to_string())
    }
}

impl From<deadpool_sqlite::PoolError> for SyncError {
    fn from(err: deadpool_sqlite::PoolError) -> Self {
        SyncError::OutboxDatabase(err.to_string())
    }
}

/// Result type alias for sync operations.
pub type SyncResult<T> = Result<T, SyncError>;
