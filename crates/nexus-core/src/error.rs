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
    #[error("closing")]
    Closing,
    #[error("interrupted")]
    Interrupted,
    /// Retained bearer-memory 403 shape: the daemon envelope splits a
    /// resource tag and a specific reason string (`Forbidden { resource,
    /// reason }`); the generic [`CoreError::Forbidden`] collapses the reason.
    #[error("forbidden: {resource} — {reason}")]
    ForbiddenReason { resource: String, reason: String },
    /// Retained truthful 503 ("no synthesis provider/capability registry");
    /// never a background-synthesis fallback.
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),
    /// Retained 400 narrative-quality rejection (`narrative_generation_failed`).
    #[error("narrative generation failed: {0}")]
    NarrativeRejected(String),
    /// Retained plain 409 conflict with a human message and no stable code
    /// (Moment Directive `set` without `replace`).
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal: {category}")]
    Internal { category: String },
    /// Stable actor-family wire conflict (409 at HTTP adapters) with its
    /// retained code and message (`character_busy`, `character_inactive`,
    /// `world_inactive`, `last_active_actor_world_binding`, …). The code is
    /// the contract (durable spec §11.1 stable codes); adapters re-render it
    /// verbatim.
    #[error("actor conflict: {code}")]
    ActorConflict { code: String, message: String },
    /// Stable actor-family wire rejection (400 `invalid_input` at HTTP
    /// adapters) carrying only the retained message — the family never
    /// attached structured `field` details to these rejections.
    #[error("{0}")]
    ActorInput(String),
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

/// Map a storage error onto the neutral actor-family taxonomy, preserving
/// the retained wire meaning per variant: stable contract conflicts keep
/// their code + message, not-found keeps the daemon `"{resource} {id} not
/// found"` message, and validation keeps the plain `invalid_input` message.
pub(crate) fn actor_db_err(e: LocalDbError) -> CoreError {
    match e {
        LocalDbError::ActorNotFound { resource, id } => CoreError::NotFound {
            resource: format!("{resource} {id} not found"),
        },
        LocalDbError::ActorContractConflict { code } => CoreError::ActorConflict {
            code: code.as_str().to_string(),
            message: code.message().to_string(),
        },
        LocalDbError::ValidationError(msg) => CoreError::ActorInput(msg),
        other => local_db_err(other),
    }
}

/// Map a storage error from a guarded actor-owned insert (knowledge entries):
/// the insert path renders not-found WITHOUT the ` not found` suffix and
/// treats constraint violations as plain `invalid_input` (retained daemon
/// `map_local_db_insert_err` texture).
pub(crate) fn actor_insert_db_err(e: LocalDbError) -> CoreError {
    if matches!(e, LocalDbError::ConstraintViolation { .. }) {
        return CoreError::ActorInput(e.to_string());
    }
    match e {
        LocalDbError::ActorNotFound { resource, id } => CoreError::NotFound {
            resource: format!("{resource} {id}"),
        },
        other => actor_db_err(other),
    }
}

/// Retained bearer-memory internal wire codes the daemon adapter re-sends
/// verbatim from `"{CODE}: {message}"` core categories (same convention as the
/// creator cache/config carriers in `creators.rs`).
pub const MEMORY_INTERNAL_CODES: &[&str] = &[
    "promote_to_long_term_failed",
    "pending_review_queue_advance_stale",
    "narrative_synthesis_error",
    "character_soul_read_error",
    "character_memory_list_error",
    "character_memory_load_error",
    "character_memory_render_error",
    "character_tom_wire_invalid",
    "character_tom_kb_failed",
    "character_tom_db_failed",
    "character_tom_scope_invalid",
    "pipeline_guard_mismatch",
    "directive_wire_invalid",
    "directive_row_serialize",
    "directive_response_decode",
    "inspector_packet_decode",
];
