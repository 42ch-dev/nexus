//! KB-domain error types.

use thiserror::Error;

/// Error type for KeyBlock and SourceAnchor operations.
#[derive(Debug, Error, PartialEq)]
pub enum KbError {
    /// Permission denied.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Cannot modify an immutable confirmed state.
    #[error("cannot modify confirmed immutable state")]
    ImmutableConfirmedState,

    /// Entity is already in the target state.
    #[error("entity is already in state: {0}")]
    AlreadyInState(String),

    /// Unresolved hard conflict.
    #[error("unresolved hard conflict: {0}")]
    UnresolvedConflict(String),

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

    /// Excerpt exceeds maximum length.
    #[error("excerpt exceeds maximum length: {actual} > {max}")]
    ExcerptTooLong {
        /// Actual length.
        actual: usize,
        /// Maximum allowed length.
        max: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_permission_denied() {
        let err = KbError::PermissionDenied("sync key blocks".to_string());
        assert!(err.to_string().contains("permission denied"));
    }

    #[test]
    fn test_display_revision_mismatch() {
        let err = KbError::RevisionMismatch {
            expected: 5,
            actual: 3,
        };
        assert!(err.to_string().contains("5"));
        assert!(err.to_string().contains("3"));
    }
}
