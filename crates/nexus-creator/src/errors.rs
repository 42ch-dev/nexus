//! Creator-domain error types.
//!
//! Subset of domain errors relevant to the Creator aggregate and local identity.
//!
//! see crates/nexus-creator/AGENTS.md

use thiserror::Error;

/// note: error type for Creator aggregate and local identity operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CreatorError {
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

    /// note: creator is not paired with a user (requires active pairing).
    #[error("creator is not paired with any user")]
    CreatorNotPaired,

    /// note: a required field is missing or invalid.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// note: invalid ID format.
    #[error("invalid ID format: {0}")]
    InvalidIdFormat(String),

    /// note: creator quota exceeded.
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
