//! Narrative-domain error types.
// note: one variant per failure category the narrative aggregates report.
use thiserror::Error;

/// note: Error type for Narrative aggregate operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum NarrativeError {
    /// note: Permission denied.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// note: Invalid state transition.
    #[error("invalid state transition: {from} → {to}")]
    InvalidTransition {
        /// note: Source state.
        from: String,
        /// note: Target state.
        to: String,
    },

    /// note: Entity is already in the target state.
    #[error("entity is already in state: {0}")]
    AlreadyInState(String),

    /// note: Entity is not in the expected state.
    #[error("entity is not in expected state: expected {expected}, got {actual}")]
    InvalidState {
        /// note: Expected state.
        expected: String,
        /// note: Actual state.
        actual: String,
    },

    /// note: Unresolved hard conflict prevents the operation.
    #[error("unresolved hard conflict: {0}")]
    UnresolvedConflict(String),

    /// note: Timeline conflict would reorder canon sequence.
    #[error("timeline conflict: {0}")]
    TimelineConflict(String),

    /// note: Causality violation: cross-world causal reference.
    #[error("causality violation: {0}")]
    CausalityViolation(String),

    /// note: Version mismatch.
    #[error("revision mismatch: expected {expected}, got {actual}")]
    RevisionMismatch {
        /// note: Expected revision.
        expected: u64,
        /// note: Actual revision.
        actual: u64,
    },

    /// note: Validation error.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// note: Invalid manuscript storage configuration.
    #[error("invalid storage configuration: {0}")]
    InvalidStorageConfig(String),

    /// note: Invalid fork write scope.
    #[error("invalid fork write scope: {0}")]
    InvalidForkWriteScope(String),

    /// note: Invalid phase transition.
    #[error("invalid phase transition: {from} → {to}")]
    InvalidPhaseTransition {
        /// note: Source phase.
        from: String,
        /// note: Target phase.
        to: String,
    },

    /// note: Provisional records still exist.
    #[error("provisional records exist: {count} outstanding")]
    ProvisionalRecordsExist {
        /// note: Count of outstanding records.
        count: usize,
    },

    /// note: Excerpt exceeds maximum length.
    #[error("excerpt exceeds maximum length: {actual} > {max}")]
    ExcerptTooLong {
        /// note: Actual length.
        actual: usize,
        /// note: Maximum length.
        max: usize,
    },

    /// note: Requested entity does not exist.
    ///
    /// Typed replacement for the former `ValidationError("… not found: …")`
    /// convention (V1.206 P2): consumers classify absence on the variant, not
    /// on message text. `resource` is the opaque identity (e.g. `world wld_1`,
    /// `event evt_1`) — never a formatted sentence, so the consumer chooses the
    /// external rendering.
    #[error("not found: {resource}")]
    NotFound {
        /// note: Opaque identity of the missing entity.
        resource: String,
    },

    /// note: Storage backend error (database, I/O, etc.).
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
    fn test_display_not_found() {
        let err = NarrativeError::NotFound {
            resource: "world wld_1".to_string(),
        };
        assert_eq!(err.to_string(), "not found: world wld_1");
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
