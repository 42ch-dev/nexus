//! Creator-domain error types.
//!
//! Subset of domain errors relevant to the Creator aggregate and local identity.

use thiserror::Error;

/// Error type for Creator aggregate and local identity operations.
#[derive(Debug, Error, PartialEq)]
pub enum CreatorError {
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

    /// Creator is not paired with a user (requires active pairing).
    #[error("creator is not paired with any user")]
    CreatorNotPaired,

    /// A required field is missing or invalid.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// Invalid ID format.
    #[error("invalid ID format: {0}")]
    InvalidIdFormat(String),

    /// Creator quota exceeded.
    #[error("creator quota exceeded: {0}")]
    CreatorQuotaExceeded(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_already_in_state() {
        let err = CreatorError::AlreadyInState("active".to_string());
        assert!(err.to_string().contains("already in state"));
    }

    #[test]
    fn test_display_invalid_state() {
        let err = CreatorError::InvalidState {
            expected: "active".to_string(),
            actual: "archived".to_string(),
        };
        assert!(err.to_string().contains("expected"));
        assert!(err.to_string().contains("active"));
    }

    #[test]
    fn test_display_creator_not_paired() {
        let err = CreatorError::CreatorNotPaired;
        assert!(err.to_string().contains("not paired"));
    }
}
