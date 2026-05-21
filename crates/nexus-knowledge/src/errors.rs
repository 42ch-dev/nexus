//! Knowledge-domain error types.

use thiserror::Error;

/// Error type for Knowledge operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum KnowledgeError {
    /// Validation error.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// Invalid URI format.
    #[error("invalid URI for source_type '{source_type}': {reason}")]
    InvalidUri {
        /// Source type.
        source_type: String,
        /// Reason.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_invalid_uri() {
        let err = KnowledgeError::InvalidUri {
            source_type: "file".to_string(),
            reason: "not a valid URL".to_string(),
        };
        assert!(err.to_string().contains("invalid URI"));
        assert!(err.to_string().contains("file"));
    }
}
