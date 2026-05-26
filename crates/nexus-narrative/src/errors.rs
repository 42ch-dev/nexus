//! Narrative-domain error types.

use thiserror::Error;

/// Error type for Narrative aggregate operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum NarrativeError {
    /// Permission denied.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Invalid state transition.
    #[error("invalid state transition: {from} → {to}")]
    InvalidTransition {
        /// Source state.
        from: String,
        /// Target state.
        to: String,
    },

    /// Entity is already in the target state.
    #[error("entity is already in state: {0}")]
    AlreadyInState(String),

    /// Entity is not in the expected state.
    #[error("entity is not in expected state: expected {expected}, got {actual}")]
    InvalidState {
        /// Expected state.
        expected: String,
        /// Actual state.
        actual: String,
    },

    /// Unresolved hard conflict prevents the operation.
    #[error("unresolved hard conflict: {0}")]
    UnresolvedConflict(String),

    /// Timeline conflict would reorder canon sequence.
    #[error("timeline conflict: {0}")]
    TimelineConflict(String),

    /// Causality violation: cross-world causal reference.
    #[error("causality violation: {0}")]
    CausalityViolation(String),

    /// Version mismatch.
    #[error("revision mismatch: expected {expected}, got {actual}")]
    RevisionMismatch {
        /// Expected revision.
        expected: u64,
        /// Actual revision.
        actual: u64,
    },

    /// Validation error.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// Invalid manuscript storage configuration.
    #[error("invalid storage configuration: {0}")]
    InvalidStorageConfig(String),

    /// Invalid fork write scope.
    #[error("invalid fork write scope: {0}")]
    InvalidForkWriteScope(String),

    /// Invalid phase transition.
    #[error("invalid phase transition: {from} → {to}")]
    InvalidPhaseTransition {
        /// Source phase.
        from: String,
        /// Target phase.
        to: String,
    },

    /// Provisional records still exist.
    #[error("provisional records exist: {count} outstanding")]
    ProvisionalRecordsExist {
        /// Count of outstanding records.
        count: usize,
    },

    /// Excerpt exceeds maximum length.
    #[error("excerpt exceeds maximum length: {actual} > {max}")]
    ExcerptTooLong {
        /// Actual length.
        actual: usize,
        /// Maximum length.
        max: usize,
    },

    /// Storage backend error (database, I/O, etc.).
    #[error("storage error: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_timeline_conflict() {
        let err = NarrativeError::TimelineConflict("event ordering".to_string());
        assert!(err.to_string().contains("timeline conflict"));
    }

    #[test]
    fn test_display_causality_violation() {
        let err = NarrativeError::CausalityViolation("cross-world ref".to_string());
        assert!(err.to_string().contains("causality violation"));
    }

    #[test]
    fn test_display_invalid_transition() {
        let err = NarrativeError::InvalidTransition {
            from: "draft".to_string(),
            to: "published".to_string(),
        };
        assert!(err.to_string().contains("draft"));
        assert!(err.to_string().contains("published"));
    }
}
