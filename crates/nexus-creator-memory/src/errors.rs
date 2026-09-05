//! Creator Memory — error types.

use thiserror::Error;

/// Error type for creator memory operations.
#[derive(Debug, Error)]
pub enum MemoryError {
    /// Entity is already in the target state.
    #[error("entity is already in state: {0}")]
    AlreadyInState(String),

    /// Session was already promoted to long-term memory by an earlier
    /// (possibly crashed-after-write) attempt; callers may treat this as an
    /// idempotent success and advance their queue.
    #[error("Session '{session_id}' already promoted to long-term memory")]
    AlreadyPromoted {
        /// The session that already has a long-term memory file.
        session_id: String,
    },

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

    /// ACP worker unavailable for a synthesis request.
    #[error("ACP worker unavailable")]
    WorkerUnavailable,

    /// Required capability missing from the registry.
    #[error("capability missing: {capability}")]
    CapabilityMissing {
        /// Capability id that was missing.
        capability: String,
    },

    /// Synthesizer produced malformed output that cannot be persisted.
    #[error("malformed output: {reason}")]
    MalformedOutput {
        /// Human-readable reason the output was rejected.
        reason: String,
    },

    /// Generated draft failed the quality floor.
    #[error("quality threshold missed: {reason}")]
    QualityThresholdMissed {
        /// Human-readable reason the draft was rejected.
        reason: String,
    },
}
