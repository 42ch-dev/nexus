//! Cloud-domain error types.

use thiserror::Error;

/// Error type for User and Pairing domain operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CloudDomainError {
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

    /// Validation error.
    #[error("validation error: {0}")]
    ValidationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_already_in_state() {
        let err = CloudDomainError::AlreadyInState("active".to_string());
        assert!(err.to_string().contains("already in state"));
    }

    #[test]
    fn test_display_invalid_state() {
        let err = CloudDomainError::InvalidState {
            expected: "active".to_string(),
            actual: "archived".to_string(),
        };
        assert!(err.to_string().contains("expected"));
    }
}
