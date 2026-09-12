//! Neutral core error taxonomy (mapped to HTTP at the adapter boundary).

use nexus_contracts::{
    WorldKbConflictError, WorldKbValidationError, WorldKbValidationErrorValidationSummary,
};

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum CoreError {
    #[error("workspace not initialized")]
    Uninitialized,
    #[error("authentication required")]
    AuthRequired,
    #[error("forbidden: {resource}")]
    Forbidden { resource: String },
    #[error("not found: {resource}")]
    NotFound { resource: String },
    #[error("invalid input: {field} — {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("world kb conflict")]
    WorldKbConflict(WorldKbConflictError),
    #[error("world kb validation failed")]
    WorldKbValidation(WorldKbValidationError),
    #[error("writer owner busy")]
    OwnerBusy,
    #[error("writer fenced")]
    WriterFenced,
    #[error("schema mismatch")]
    SchemaMismatch,
    #[error("busy")]
    Busy,
    #[error("closing")]
    Closing,
    #[error("interrupted")]
    Interrupted,
    #[error("internal: {category}")]
    Internal { category: String },
}

impl CoreError {
    pub fn world_kb_conflict(
        current_version: u64,
        entity_id: impl Into<String>,
        conflicting_path: impl Into<String>,
        recovery_hint: impl Into<String>,
    ) -> Self {
        Self::WorldKbConflict(WorldKbConflictError {
            current_version,
            entity_id: entity_id.into(),
            conflicting_path: conflicting_path.into(),
            recovery_hint: recovery_hint.into(),
        })
    }

    pub fn world_kb_validation_failed(errors: &[String], warnings: &[String]) -> Self {
        Self::WorldKbValidation(WorldKbValidationError {
            validation_summary: WorldKbValidationErrorValidationSummary {
                errors: errors.to_vec(),
                warnings: warnings.to_vec(),
            },
        })
    }
}
