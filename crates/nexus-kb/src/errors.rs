//! KB-domain error types.

use std::fmt;
use thiserror::Error;

/// Kind of validation failure for structured error handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationKind {
    /// `body.attributes.novel_category` is missing.
    MissingNovelCategory,
    /// `body.attributes.novel_category` is not one of the seven valid values.
    InvalidNovelCategory,
    /// `body.attributes` is missing for a novel-profile `KeyBlock`.
    MissingAttributes,
    /// `body` is `None` for a novel-profile `KeyBlock`.
    MissingBody,
    /// `body.attributes` exists but is not a JSON object.
    NonObjectAttributes,
    /// `body.attributes.novel_category` exists but is not a string.
    NonStringNovelCategory,
    /// `canonical_name` fails format/safety validation.
    InvalidCanonicalName,
}

impl fmt::Display for ValidationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingNovelCategory => write!(f, "missing_novel_category"),
            Self::InvalidNovelCategory => write!(f, "invalid_novel_category"),
            Self::MissingAttributes => write!(f, "missing_attributes"),
            Self::MissingBody => write!(f, "missing_body"),
            Self::NonObjectAttributes => write!(f, "non_object_attributes"),
            Self::NonStringNovelCategory => write!(f, "non_string_novel_category"),
            Self::InvalidCanonicalName => write!(f, "invalid_canonical_name"),
        }
    }
}

/// Structured validation error with kind, optional field, and human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Categorised failure kind.
    pub kind: ValidationKind,
    /// Optional field path (e.g., `"body.attributes.novel_category"`).
    pub field: Option<String>,
    /// Human-readable message.
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref field) = self.field {
            write!(f, "{} ({}): {}", self.kind, field, self.message)
        } else {
            write!(f, "{}: {}", self.kind, self.message)
        }
    }
}

impl std::error::Error for ValidationError {}

/// Error type for `KeyBlock` and `SourceAnchor` operations.
#[derive(Debug, Error, PartialEq, Eq)]
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

    /// Structured validation error (taxonomy / canonical-name rules).
    #[error("validation error: {0}")]
    Validation(ValidationError),

    /// Validation error with opaque message (legacy / non-structured paths).
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
