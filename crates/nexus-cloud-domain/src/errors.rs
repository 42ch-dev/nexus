//! Cloud-domain error types.
// note: one error enum is shared by the User and Pairing aggregates.

use thiserror::Error;

/// note: error type for User and Pairing domain operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CloudDomainError {
    /// note: entity is already in the target state.
    #[error("entity is already in state: {0}")]
    AlreadyInState(String),

    /// note: entity is not in the expected state.
    #[error("entity is not in expected state: expected {expected}, got {actual}")]
    InvalidState {
        /// note: expected state.
        expected: String,
        /// note: actual state.
        actual: String,
    },

    /// note: validation error.
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
