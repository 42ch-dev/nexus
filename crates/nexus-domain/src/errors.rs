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
    // V1.1: wire into ID validation and quota checks
    #[allow(dead_code)]
    #[error("creator quota exceeded: {0}")]
    CreatorQuotaExceeded(String),

    /// Invalid ID format.
    // V1.1: wire into ID validation and quota checks
    #[allow(dead_code)]
    #[error("invalid ID format: {0}")]
    InvalidIdFormat(String),

    /// Unknown or invalid local identity type string.
    #[error("invalid identity type: {0}")]
    InvalidIdentityType(String),

    // ── Runtime mode errors ───────────────────────────────────────────
    /// Operation requires platform connectivity but current mode prohibits it.
    #[error("operation '{operation}' is not available in {mode} mode")]
    PlatformOperationProhibited { mode: String, operation: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_permission_denied() {
        let err = DomainError::PermissionDenied("sync key blocks".to_string());
        let msg = err.to_string();
        assert!(msg.contains("permission denied"), "msg: {msg}");
        assert!(msg.contains("sync key blocks"), "msg: {msg}");
    }

    #[test]
    fn test_display_creator_not_paired() {
        let err = DomainError::CreatorNotPaired;
        let msg = err.to_string();
        assert!(msg.contains("not paired"), "msg: {msg}");
    }

    #[test]
    fn test_display_invalid_transition() {
        let err = DomainError::InvalidTransition {
            from: "draft".to_string(),
            to: "published".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("invalid state transition"), "msg: {msg}");
        assert!(msg.contains("draft"), "msg: {msg}");
        assert!(msg.contains("published"), "msg: {msg}");
    }

    #[test]
    fn test_display_immutable_confirmed_state() {
        let err = DomainError::ImmutableConfirmedState;
        let msg = err.to_string();
        assert!(msg.contains("immutable"), "msg: {msg}");
    }

    #[test]
    fn test_display_already_in_state() {
        let err = DomainError::AlreadyInState("active".to_string());
        let msg = err.to_string();
        assert!(msg.contains("already in state"), "msg: {msg}");
        assert!(msg.contains("active"), "msg: {msg}");
    }

    #[test]
    fn test_display_invalid_state() {
        let err = DomainError::InvalidState {
            expected: "active".to_string(),
            actual: "archived".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("expected"), "msg: {msg}");
        assert!(msg.contains("active"), "msg: {msg}");
        assert!(msg.contains("archived"), "msg: {msg}");
    }

    #[test]
    fn test_display_unresolved_conflict() {
        let err = DomainError::UnresolvedConflict("block_42".to_string());
        let msg = err.to_string();
        assert!(msg.contains("unresolved hard conflict"), "msg: {msg}");
        assert!(msg.contains("block_42"), "msg: {msg}");
    }

    #[test]
    fn test_display_timeline_conflict() {
        let err = DomainError::TimelineConflict("event ordering".to_string());
        let msg = err.to_string();
        assert!(msg.contains("timeline conflict"), "msg: {msg}");
    }

    #[test]
    fn test_display_causality_violation() {
        let err = DomainError::CausalityViolation("cross-world ref".to_string());
        let msg = err.to_string();
        assert!(msg.contains("causality violation"), "msg: {msg}");
    }

    #[test]
    fn test_display_revision_mismatch() {
        let err = DomainError::RevisionMismatch {
            expected: 5,
            actual: 3,
        };
        let msg = err.to_string();
        assert!(msg.contains("revision mismatch"), "msg: {msg}");
        assert!(msg.contains("5"), "msg: {msg}");
        assert!(msg.contains("3"), "msg: {msg}");
    }

    #[test]
    fn test_display_validation_error() {
        let err = DomainError::ValidationError("empty title".to_string());
        let msg = err.to_string();
        assert!(msg.contains("validation error"), "msg: {msg}");
        assert!(msg.contains("empty title"), "msg: {msg}");
    }

    #[test]
    fn test_display_excerpt_too_long() {
        let err = DomainError::ExcerptTooLong {
            actual: 5000,
            max: 2000,
        };
        let msg = err.to_string();
        assert!(msg.contains("excerpt exceeds maximum length"), "msg: {msg}");
        assert!(msg.contains("5000"), "msg: {msg}");
        assert!(msg.contains("2000"), "msg: {msg}");
    }

    #[test]
    fn test_display_invalid_storage_config() {
        let err = DomainError::InvalidStorageConfig("missing bucket".to_string());
        let msg = err.to_string();
        assert!(msg.contains("invalid storage configuration"), "msg: {msg}");
    }

    #[test]
    fn test_display_invalid_fork_write_scope() {
        let err = DomainError::InvalidForkWriteScope("wrong branch".to_string());
        let msg = err.to_string();
        assert!(msg.contains("invalid fork write scope"), "msg: {msg}");
    }

    #[test]
    fn test_display_invalid_uri() {
        let err = DomainError::InvalidUri {
            source_type: "file".to_string(),
            reason: "not a valid URL".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("invalid URI"), "msg: {msg}");
        assert!(msg.contains("file"), "msg: {msg}");
        assert!(msg.contains("not a valid URL"), "msg: {msg}");
    }

    #[test]
    fn test_display_invalid_phase_transition() {
        let err = DomainError::InvalidPhaseTransition {
            from: "brainstorm".to_string(),
            to: "published".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("invalid phase transition"), "msg: {msg}");
        assert!(msg.contains("brainstorm"), "msg: {msg}");
        assert!(msg.contains("published"), "msg: {msg}");
    }

    #[test]
    fn test_display_provisional_records_exist() {
        let err = DomainError::ProvisionalRecordsExist { count: 7 };
        let msg = err.to_string();
        assert!(msg.contains("provisional records exist"), "msg: {msg}");
        assert!(msg.contains("7"), "msg: {msg}");
    }

    #[test]
    fn test_display_creator_quota_exceeded() {
        let err = DomainError::CreatorQuotaExceeded("max 10 worlds".to_string());
        let msg = err.to_string();
        assert!(msg.contains("creator quota exceeded"), "msg: {msg}");
    }

    #[test]
    fn test_display_invalid_id_format() {
        let err = DomainError::InvalidIdFormat("missing prefix".to_string());
        let msg = err.to_string();
        assert!(msg.contains("invalid ID format"), "msg: {msg}");
    }

    #[test]
    fn test_display_invalid_identity_type() {
        let err = DomainError::InvalidIdentityType("bogus_type".to_string());
        let msg = err.to_string();
        assert!(msg.contains("invalid identity type"), "msg: {msg}");
        assert!(msg.contains("bogus_type"), "msg: {msg}");
    }

    #[test]
    fn test_display_platform_operation_prohibited() {
        let err = DomainError::PlatformOperationProhibited {
            mode: "local_only".to_string(),
            operation: "sync".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("not available in local_only mode"),
            "msg: {msg}"
        );
    }
}
