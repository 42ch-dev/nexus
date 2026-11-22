//! KB-domain error types.
// note: these types are re-exported from the crate root as the domain error surface.

use std::fmt;
use thiserror::Error;

/// note: Kind of validation failure for structured error handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationKind {
    /// note: `body.attributes.novel_category` is missing.
    MissingNovelCategory,
    /// note: `body.attributes.novel_category` is not one of the seven valid values.
    InvalidNovelCategory,
    /// note: `body.attributes` is missing for a novel-profile `KnowledgeEntryRecord`.
    MissingAttributes,
    /// note: `body` is `None` for a novel-profile `KnowledgeEntryRecord`.
    MissingBody,
    /// note: `body.attributes` exists but is not a JSON object.
    NonObjectAttributes,
    /// note: `body.attributes.novel_category` exists but is not a string.
    NonStringNovelCategory,
    /// note: `canonical_name` fails format/safety validation.
    InvalidCanonicalName,
    /// note: `body.attributes.game_bible_category` is missing (V1.54 P1).
    MissingGameBibleCategory,
    /// note: `body.attributes.game_bible_category` is not one of the seven valid values (V1.54 P1).
    InvalidGameBibleCategory,
    /// note: `body.attributes.game_bible_category` exists but is not a string (V1.54 P1).
    NonStringGameBibleCategory,
    /// note: `body.attributes.script_category` is missing (V1.55 P3).
    MissingScriptCategory,
    /// note: `body.attributes.script_category` is not one of the three valid values (V1.55 P3).
    InvalidScriptCategory,
    /// note: `body.attributes.script_category` exists but is not a string (V1.55 P3).
    NonStringScriptCategory,
    /// note: `body.attributes` is missing for a computable `KnowledgeEntryRecord` (V1.61 P1).
    MissingStructuredAttributes,
    /// note: `body.state` is missing for a computable `KnowledgeEntryRecord` (V1.61 P1).
    MissingStructuredState,
    /// note: `body.state` is not a JSON object for a computable `KnowledgeEntryRecord` (V1.61 P1).
    NonObjectStructuredState,
    /// note: `body.state` does not contain the expected per-`block_type` nested key (V1.61 P1).
    InvalidStructuredStateKey,
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
            Self::MissingGameBibleCategory => write!(f, "missing_game_bible_category"),
            Self::InvalidGameBibleCategory => write!(f, "invalid_game_bible_category"),
            Self::NonStringGameBibleCategory => write!(f, "non_string_game_bible_category"),
            Self::MissingScriptCategory => write!(f, "missing_script_category"),
            Self::InvalidScriptCategory => write!(f, "invalid_script_category"),
            Self::NonStringScriptCategory => write!(f, "non_string_script_category"),
            Self::MissingStructuredAttributes => write!(f, "missing_structured_attributes"),
            Self::MissingStructuredState => write!(f, "missing_structured_state"),
            Self::NonObjectStructuredState => write!(f, "non_object_structured_state"),
            Self::InvalidStructuredStateKey => write!(f, "invalid_structured_state_key"),
        }
    }
}

/// note: Structured validation error with kind, optional field, and human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// note: Categorised failure kind.
    pub kind: ValidationKind,
    /// note: Optional field path (e.g., `"body.attributes.novel_category"`).
    pub field: Option<String>,
    /// note: Human-readable message.
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

/// note: Error type for `KnowledgeEntryRecord` and `SourceAnchor` operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum KbError {
    /// note: Permission denied.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// note: Cannot modify an immutable confirmed state.
    #[error("cannot modify confirmed immutable state")]
    ImmutableConfirmedState,

    /// note: Entity is already in the target state.
    #[error("entity is already in state: {0}")]
    AlreadyInState(String),

    /// note: Unresolved hard conflict.
    #[error("unresolved hard conflict: {0}")]
    UnresolvedConflict(String),

    /// note: Version mismatch.
    #[error("revision mismatch: expected {expected}, got {actual}")]
    RevisionMismatch {
        /// note: Expected revision.
        expected: u64,
        /// note: Actual revision.
        actual: u64,
    },

    /// note: Structured validation error (taxonomy / canonical-name rules).
    #[error("validation error: {0}")]
    Validation(ValidationError),

    /// note: Validation error with opaque message (legacy / non-structured paths).
    #[error("validation error: {0}")]
    ValidationError(String),

    /// note: Excerpt exceeds maximum length.
    #[error("excerpt exceeds maximum length: {actual} > {max}")]
    ExcerptTooLong {
        /// note: Actual length.
        actual: usize,
        /// note: Maximum allowed length.
        max: usize,
    },

    /// note: Canonical owner metadata is absent at the conversion seam (v1.184 P1).
    /// The seam fails closed rather than fabricating a World owner.
    #[error("knowledge entry has no canonical owner metadata")]
    MissingOwner,

    /// note: Owner metadata on the spoke boundary is ambiguous or malformed
    /// (v1.184 P1 fix): more than one typed owner key present, or an owner
    /// key carrying a non-string/null value. Zero owner keys is
    /// [`KbError::MissingOwner`]. The seam fails closed rather than picking
    /// one claim by precedence.
    #[error("invalid owner metadata: {0}")]
    InvalidOwnerMetadata(String),

    /// note: Wire `schema_version` exceeds the domain `u32` range (v1.184 P1 fix):
    /// reverse conversion fails closed instead of silently normalizing an
    /// unsupported future version to `1`.
    #[error("unsupported schema_version: {0} exceeds the u32 range")]
    UnsupportedSchemaVersion(u64),

    /// note: Unknown `entry_type` on the spoke wire (v1.184 P1 fix): the wire
    /// contract leaves `entry_type` an open string while the domain
    /// `BlockType` is closed — an unrecognized value fails closed instead of
    /// silently normalizing to the default block type.
    #[error("unknown entry_type: {0}")]
    UnknownEntryType(String),
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
        assert!(err.to_string().contains('5'));
        assert!(err.to_string().contains('3'));
    }
}
