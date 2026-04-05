//! Nexus Domain Errors
//!
//! Domain-level error types for all Nexus aggregates.
//! Uses `thiserror` for ergonomic error handling.

use thiserror::Error;

/// Domain error type covering all aggregate validation and state transition errors.
#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    // ── Permission errors ────────────────────────────────────────────
    /// Initiator lacks required permission for the operation.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Creator is not paired with a user (requires active pairing).
    #[error("creator is not paired with any user")]
    CreatorNotPaired,

    // ── State transition errors ──────────────────────────────────────
    /// Attempted an invalid state transition.
    #[error("invalid state transition: {from} → {to}")]
    InvalidTransition { from: String, to: String },

    /// Cannot modify an immutable confirmed state.
    #[error("cannot modify confirmed immutable state")]
    ImmutableConfirmedState,

    /// Entity is already in the target state.
    #[error("entity is already in state: {0}")]
    AlreadyInState(String),

    /// Entity is not in the expected state.
    #[error("entity is not in expected state: expected {expected}, got {actual}")]
    InvalidState { expected: String, actual: String },

    // ── Conflict errors ──────────────────────────────────────────────
    /// Unresolved hard conflict prevents the operation.
    #[error("unresolved hard conflict: {0}")]
    UnresolvedConflict(String),

    /// Timeline conflict would reorder canon sequence.
    #[error("timeline conflict: {0}")]
    TimelineConflict(String),

    /// Causality violation: cross-world causal reference.
    #[error("causality violation: {0}")]
    CausalityViolation(String),

    /// Version mismatch between client and server.
    #[error("revision mismatch: expected {expected}, got {actual}")]
    RevisionMismatch { expected: u64, actual: u64 },

    // ── Validation errors ────────────────────────────────────────────
    /// A required field is missing or invalid.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// Excerpt exceeds maximum allowed length.
    #[error("excerpt exceeds maximum length: {actual} > {max}")]
    ExcerptTooLong { actual: usize, max: usize },

    /// Invalid manuscript storage configuration.
    #[error("invalid storage configuration: {0}")]
    InvalidStorageConfig(String),

    /// Invalid fork write scope.
    #[error("invalid fork write scope: {0}")]
    InvalidForkWriteScope(String),

    /// Invalid URI format for the source type.
    #[error("invalid URI for source_type '{source_type}': {reason}")]
    InvalidUri { source_type: String, reason: String },

    /// Invalid phase transition.
    #[error("invalid phase transition: {from} → {to}")]
    InvalidPhaseTransition { from: String, to: String },

    /// Provisional records still exist before gate.
    #[error("provisional records exist: {count} outstanding")]
    ProvisionalRecordsExist { count: usize },

    /// Creator quota exceeded.
    #[error("creator quota exceeded: {0}")]
    CreatorQuotaExceeded(String),

    /// Invalid ID format.
    #[error("invalid ID format: {0}")]
    InvalidIdFormat(String),
}
