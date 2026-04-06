//! Nexus Sync Errors
//!
//! Sync-layer error types for outbox, bundle building, sync client,
//! conflict resolution, and precheck operations.
//! Uses `thiserror` following the domain layer pattern.

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
    SyncConflict { conflict_type: String },

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

/// Result type alias for sync operations.
pub type SyncResult<T> = Result<T, SyncError>;
