//! Creator Memory — error types.

use thiserror::Error;

/// Error type for creator memory operations.
#[derive(Debug, Error)]
pub enum MemoryError {
    /// Entity is already in the target state.
    #[error("entity is already in state: {0}")]
    AlreadyInState(String),

    /// Validation error.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// Invalid ID format.
    #[error("invalid ID format: {0}")]
    InvalidIdFormat(String),

    /// Entity is not in the expected state.
    #[error("entity is not in expected state: expected {expected}, got {actual}")]
    InvalidState {
        /// Expected state.
        expected: String,
        /// Actual state.
        actual: String,
    },

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML parse error.
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// JSON error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// SOUL not found.
    #[error("SOUL.md not found for creator '{creator_id}' at {path}")]
    SoulNotFound {
        /// Creator ID.
        creator_id: String,
        /// Expected path.
        path: String,
    },

    /// SOUL missing required section.
    #[error("SOUL.md is missing required section '{section}'")]
    SoulMissingSection {
        /// Section name.
        section: String,
    },

    /// SOUL frontmatter error.
    #[error("SOUL.md frontmatter error: {0}")]
    SoulFrontmatterError(String),
}
