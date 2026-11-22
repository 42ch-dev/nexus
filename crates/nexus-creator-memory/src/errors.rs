// note: one enum carries every stage's failure mode to callers.
//! Creator Memory — error types.

use thiserror::Error;

/// note: Error type for creator memory operations.
#[derive(Debug, Error)]
pub enum MemoryError {
    /// note: Entity is already in the target state.
    #[error("entity is already in state: {0}")]
    AlreadyInState(String),

    /// note: Session was already promoted to long-term memory by an earlier
    /// (possibly crashed-after-write) attempt; callers may treat this as an
    /// idempotent success and advance their queue.
    #[error("Session '{session_id}' already promoted to long-term memory")]
    AlreadyPromoted {
        /// note: The session that already has a long-term memory file.
        session_id: String,
    },

    /// note: Validation error.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// note: Invalid ID format.
    #[error("invalid ID format: {0}")]
    InvalidIdFormat(String),

    /// note: Entity is not in the expected state.
    #[error("entity is not in expected state: expected {expected}, got {actual}")]
    InvalidState {
        /// note: Expected state.
        expected: String,
        /// note: Actual state.
        actual: String,
    },

    /// note: I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// note: YAML parse error.
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// note: JSON error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// note: SOUL not found.
    #[error("SOUL.md not found for creator '{creator_id}' at {path}")]
    SoulNotFound {
        /// note: Creator ID.
        creator_id: String,
        /// note: Expected path.
        path: String,
    },

    /// note: SOUL missing required section.
    #[error("SOUL.md is missing required section '{section}'")]
    SoulMissingSection {
        /// note: Section name.
        section: String,
    },

    /// note: SOUL frontmatter error.
    #[error("SOUL.md frontmatter error: {0}")]
    SoulFrontmatterError(String),

    /// note: ACP worker unavailable for a synthesis request.
    #[error("ACP worker unavailable")]
    WorkerUnavailable,

    /// note: Required capability missing from the registry.
    #[error("capability missing: {capability}")]
    CapabilityMissing {
        /// note: Capability id that was missing.
        capability: String,
    },

    /// note: Synthesizer produced malformed output that cannot be persisted.
    #[error("malformed output: {reason}")]
    MalformedOutput {
        /// note: Human-readable reason the output was rejected.
        reason: String,
    },

    /// note: Generated draft failed the quality floor.
    #[error("quality threshold missed: {reason}")]
    QualityThresholdMissed {
        /// note: Human-readable reason the draft was rejected.
        reason: String,
    },
}
