//! Neutral core error taxonomy (mapped to HTTP at the adapter boundary).

use nexus_contracts::{
    WorldKbConflictError, WorldKbValidationError, WorldKbValidationErrorValidationSummary,
};
use nexus_local_db::LocalDbError;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum CoreError {
    #[error("workspace not initialized")]
    Uninitialized,
    #[error("authentication required")]
    AuthRequired,
    #[error("forbidden: {resource}")]
    Forbidden { resource: String },
    /// World-ownership guard denial (`narrative_worlds.owner_creator_id`).
    /// The world id and refusal reason stay split so each transport family
    /// renders its retained 403 envelope verbatim (`resource: "world {id}"`
    /// plus the family reason) at the adapter boundary.
    #[error("forbidden: world {world_id} — {reason}")]
    WorldOwnerDenied { world_id: String, reason: String },
    #[error("not found: {resource}")]
    NotFound { resource: String },
    #[error("invalid input: {field} — {reason}")]
    InvalidInput { field: String, reason: String },
    #[error(transparent)]
    Preset(#[from] crate::presets::PresetError),
    #[error("outline conflict")]
    OutlineConflict(OutlineConflictError),
    #[error("outline validation failed")]
    OutlineValidation(OutlineValidationError),
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
    /// A refusal carrying the transport-retained `(code, message)` pair.
    ///
    /// The neutral taxonomy names the DOMAIN class; several retained surfaces
    /// additionally promise a finer lowercase wire code (`policy_blocked`,
    /// `not_supported`, `invalid_state`, `compute_fuel_exhausted`, …) whose
    /// HTTP status the adapter derives from that code. The core has no closed
    /// enum slot for an open set of codes, so it carries the pair and the
    /// adapter family that promised the code renders it verbatim.
    ///
    /// This is NOT a transport leak: the code is part of the retained
    /// contract, and the domain is the only layer that knows which refusal
    /// occurred. The adapter decides the status; the core decides the code.
    #[error("{code}: {message}")]
    Coded { code: String, message: String },
    /// A peer's refusal, preserving the peer's own lowercase wire code.
    ///
    /// The spine's public code stays `not_supported` (the same answer an
    /// unknown builtin gives); the peer's finer code (`op_unsupported`,
    /// `capability_missing`, …) travels separately so the adapter can render
    /// it in `details.wire_code` — verbatim, never re-parsed from the message.
    #[error("peer denied {code}: {message}")]
    PeerDenied {
        /// The spine's public code.
        code: String,
        /// The peer's own wire code.
        wire_code: String,
        /// The peer's message.
        message: String,
    },
    #[error("closing")]
    Closing,
    #[error("interrupted")]
    Interrupted,
    #[error("internal: {category}")]
    Internal { category: String },
}

/// Structured outline-canvas OCC conflict payload (HTTP 409 at the adapter).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineConflictError {
    pub current_revision: u64,
    pub node_id: String,
    pub conflicting_path: String,
    pub recovery_hint: String,
}

/// Structured outline-canvas validation payload (HTTP 422 at the adapter).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineValidationError {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
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

    #[must_use]
    pub fn world_kb_validation_failed(errors: &[String], warnings: &[String]) -> Self {
        Self::WorldKbValidation(WorldKbValidationError {
            validation_summary: WorldKbValidationErrorValidationSummary {
                errors: errors.to_vec(),
                warnings: warnings.to_vec(),
            },
        })
    }

    /// Build an `outline_conflict` error with structured recovery details.
    #[must_use]
    pub fn outline_conflict(
        current_revision: u64,
        node_id: impl Into<String>,
        conflicting_path: impl Into<String>,
        recovery_hint: impl Into<String>,
    ) -> Self {
        Self::OutlineConflict(OutlineConflictError {
            current_revision,
            node_id: node_id.into(),
            conflicting_path: conflicting_path.into(),
            recovery_hint: recovery_hint.into(),
        })
    }

    /// Build an `outline_validation_failed` error from a validation summary.
    #[must_use]
    pub fn outline_validation_failed(errors: &[String], warnings: &[String]) -> Self {
        Self::OutlineValidation(OutlineValidationError {
            errors: errors.to_vec(),
            warnings: warnings.to_vec(),
        })
    }
}

/// True for SQLite lock-contention errors (`SQLITE_BUSY` family).
pub(crate) fn is_sqlite_busy(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => {
            db.code().as_deref() == Some("5")
                || db.message().contains("database is locked")
                || db.message().contains("SQLITE_BUSY")
        }
        _ => false,
    }
}

/// Map a `nexus_local_db` storage error onto the core taxonomy.
pub(crate) fn local_db_err(e: LocalDbError) -> CoreError {
    match e {
        LocalDbError::OwnerBusy { .. } | LocalDbError::Sqlx(sqlx::Error::PoolTimedOut) => {
            CoreError::OwnerBusy
        }
        LocalDbError::WriterFenced { .. } => CoreError::WriterFenced,
        LocalDbError::SchemaMismatch { .. } => CoreError::SchemaMismatch,
        LocalDbError::Sqlx(err) if is_sqlite_busy(&err) => CoreError::Busy,
        other => CoreError::Internal {
            category: format!("database_error: {other}"),
        },
    }
}

/// Map a raw SQLite driver error onto the core taxonomy (M1 precedent:
/// lock contention → `Busy`, pool timeout → `OwnerBusy`).
pub(crate) fn db_err(e: &sqlx::Error) -> CoreError {
    if is_sqlite_busy(e) {
        CoreError::Busy
    } else if matches!(e, sqlx::Error::PoolTimedOut) {
        CoreError::OwnerBusy
    } else {
        CoreError::Internal {
            category: format!("database_error: {e}"),
        }
    }
}
