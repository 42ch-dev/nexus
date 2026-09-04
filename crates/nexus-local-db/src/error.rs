//! Local database error types
//!
//! Provides descriptive errors for validation and version reading operations.

use std::fmt;

/// Local database errors with actionable descriptions
#[derive(Debug)]
pub enum LocalDbError {
    /// `workspace_meta` table does not exist
    MissingWorkspaceMetaTable,
    /// Required version key is missing from `workspace_meta`
    MissingVersionKey { key: String },
    /// Version value is not a valid u32 integer
    InvalidVersionValue {
        key: String,
        value: String,
        reason: String,
    },
    /// Local identity does not exist
    IdentityNotFound { creator_id: String },
    /// Local identity is already linked to a platform creator
    IdentityAlreadyLinked { creator_id: String },
    /// Local identity is not linked to any platform creator
    IdentityNotLinked { creator_id: String },
    /// I/O error with descriptive message (used by inspiration scaffold)
    Io(String),
    /// I/O error (file system operations with path context)
    IoWithPath {
        path: String,
        source: std::io::Error,
    },
    /// sqlx operation failed
    Sqlx(sqlx::Error),
    /// sqlx migration failed
    Migrate(sqlx::migrate::MigrateError),
    /// A database constraint was violated (e.g., TOCTOU race detected)
    ConstraintViolation { table: String, constraint: String },
    /// V1.49 P0 W-1 (findings-lifecycle): an illegal lifecycle transition
    /// was attempted (e.g. `resolved → open`). Emitted by findings
    /// [`enforce_status_transition`](crate::findings::enforce_status_transition)
    /// so the API layer can map it to a precise public code
    /// (`INVALID_TRANSITION`) without string-prefix sniffing. Other callers
    /// continue to use [`ConstraintViolation`](Self::ConstraintViolation).
    IllegalTransition { from: String, to: String },
    /// V1.49 P0 W-1 (findings-lifecycle): a field value is not a member of
    /// its allowed enum (invalid `severity` / `status` / `target_executor`
    /// on the findings PATCH path). Emitted instead of
    /// [`ConstraintViolation`](Self::ConstraintViolation) on the PATCH
    /// surface so the API can map it to a distinct code (`INVALID_INPUT`).
    /// The create path and shared validators still use `ConstraintViolation`.
    InvalidEnum {
        field: &'static str,
        value: String,
        allowed: &'static [&'static str],
    },
    /// Path escapes its expected parent directory (defense-in-depth)
    PathEscape { path: String, prefix: String },
    /// V1.51 T-B P1: OCC version mismatch — the row's version changed between
    /// the caller's read and its UPDATE (CAS check failed). The caller should
    /// surface this as `E_VERSION` (exit 76) and advise retrying.
    VersionMismatch {
        table: String,
        id: String,
        expected: i64,
        actual: Option<i64>,
    },
    /// V1.154 P2 (R3 closure): the world-aware CAS predicate
    /// (`WHERE … AND world_id = ?`) missed because the row's stored
    /// `world_id` differs from the world the caller verified — a
    /// cross-process writer moved the row between the caller's check and the
    /// UPDATE. This is an internal classification; hosts surface it with the
    /// fixed `world_conflict` wire code (spec §3.2), never as a generic OCC
    /// version mismatch.
    WorldConflict {
        table: String,
        id: String,
        expected_world: String,
        actual_world: String,
    },
    /// Input validation failed before reaching the database.
    ValidationError(String),
    /// Character / binding / world row is missing for the scoped caller.
    ActorNotFound {
        resource: &'static str,
        id: String,
    },
    /// Stable actor-contract product conflict (HTTP 409 at the Daemon).
    ActorContractConflict { code: ActorContractConflict },
}

/// Stable actor-contract conflict codes (wire `error.code` at HTTP 409).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorContractConflict {
    LastActiveBinding,
    DuplicateActiveBinding,
    DuplicateCharacterDisplayName,
    InvalidWorldSheet,
    WorldHasActorBindings,
    BindingHasOwnedKnowledge,
}

impl ActorContractConflict {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LastActiveBinding => "last_active_actor_world_binding",
            Self::DuplicateActiveBinding => "duplicate_active_actor_world_binding",
            Self::DuplicateCharacterDisplayName => "duplicate_character_display_name",
            Self::InvalidWorldSheet => "invalid_world_sheet",
            Self::WorldHasActorBindings => "world_has_actor_bindings",
            Self::BindingHasOwnedKnowledge => "binding_has_owned_knowledge",
        }
    }

    /// Human-readable API message for this conflict code.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::LastActiveBinding => {
                "Cannot remove the last active world binding from a Character"
            }
            Self::DuplicateActiveBinding => {
                "An active binding already exists for this Character and World"
            }
            Self::DuplicateCharacterDisplayName => {
                "An active Character with this display name already exists for this Creator"
            }
            Self::InvalidWorldSheet => {
                "WorldSheet is missing, deleted, the wrong type, or belongs to another World"
            }
            Self::WorldHasActorBindings => {
                "World has Character bindings that prevent deletion"
            }
            Self::BindingHasOwnedKnowledge => {
                "Cannot remove a binding that still owns KnowledgeEntry rows"
            }
        }
    }
}

impl LocalDbError {
    /// Display the [`VersionMismatch`](Self::VersionMismatch) arm. The two
    /// OCC conflict arms are hoisted out of the main `Display` match so
    /// the impl stays under clippy's `too_many_lines` ceiling (the
    /// V1.154 `WorldConflict` arm pushed the `fmt` body over 100 lines).
    fn fmt_version_mismatch(
        f: &mut fmt::Formatter<'_>,
        table: &str,
        id: &str,
        expected: i64,
        actual: Option<i64>,
    ) -> fmt::Result {
        write!(
            f,
            "version mismatch on '{table}' row '{id}': expected v{expected}, actual v{} — \
             row was modified by another writer; retry",
            actual.map_or_else(|| "?".to_string(), |v| v.to_string())
        )
    }

    /// Display the [`WorldConflict`](Self::WorldConflict) arm — see
    /// [`fmt_version_mismatch`].
    fn fmt_world_conflict(
        f: &mut fmt::Formatter<'_>,
        table: &str,
        id: &str,
        expected_world: &str,
        actual_world: &str,
    ) -> fmt::Result {
        write!(
            f,
            "world conflict on '{table}' row '{id}': expected world '{expected_world}', \
             actual world '{actual_world}' — the row was moved to another world by \
             another writer; re-read it in its stored world",
        )
    }
}

impl fmt::Display for LocalDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingWorkspaceMetaTable => {
                write!(
                    f,
                    "workspace_meta table does not exist - database may not be initialized; call open_pool() and run_migrations() first"
                )
            }
            Self::MissingVersionKey { key } => {
                write!(
                    f,
                    "required key '{key}' is missing from workspace_meta - database schema may be incomplete or corrupted; call seed_versions() to seed version keys",
                )
            }
            Self::InvalidVersionValue { key, value, reason } => {
                write!(
                    f,
                    "version key '{key}' has invalid value '{value}' - {reason}; database schema may be corrupted, consider re-initializing",
                )
            }
            Self::IdentityNotFound { creator_id } => {
                write!(
                    f,
                    "local identity '{creator_id}' not found; run `nexus42 identity create --persistent` to create one or `nexus42 identity list` to see available identities",
                )
            }
            Self::IdentityAlreadyLinked { creator_id } => {
                write!(
                    f,
                    "local identity '{creator_id}' is already linked to a platform creator; cannot link again",
                )
            }
            Self::IdentityNotLinked { creator_id } => {
                write!(
                    f,
                    "local identity '{creator_id}' is not linked to any platform creator; nothing to unlink",
                )
            }
            Self::Io(msg) => {
                write!(f, "I/O error: {msg}")
            }
            Self::IoWithPath { path, source } => {
                write!(f, "I/O error on '{path}': {source}")
            }
            Self::Sqlx(err) => {
                write!(f, "database operation failed: {err}")
            }
            Self::Migrate(err) => {
                write!(f, "database migration failed: {err}")
            }
            Self::ConstraintViolation { table, constraint } => {
                write!(f, "constraint violation on '{table}': {constraint}")
            }
            Self::IllegalTransition { from, to } => {
                write!(f, "invalid status transition '{from}' → '{to}'")
            }
            Self::InvalidEnum {
                field,
                value,
                allowed,
            } => {
                write!(
                    f,
                    "invalid {field} value '{value}'; allowed: {}",
                    allowed.join(", ")
                )
            }
            Self::PathEscape { path, prefix } => {
                write!(
                    f,
                    "path '{path}' escapes expected prefix '{prefix}' — possible path traversal"
                )
            }
            Self::VersionMismatch {
                table,
                id,
                expected,
                actual,
            } => Self::fmt_version_mismatch(f, table, id, *expected, *actual),
            Self::WorldConflict {
                table,
                id,
                expected_world,
                actual_world,
            } => Self::fmt_world_conflict(f, table, id, expected_world, actual_world),
            Self::ValidationError(msg) => {
                write!(f, "validation error: {msg}")
            }
            Self::ActorNotFound { resource, id } => {
                write!(f, "{resource} '{id}' not found")
            }
            Self::ActorContractConflict { code } => {
                write!(f, "{}", code.message())
            }
        }
    }
}

impl std::error::Error for LocalDbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::IoWithPath { source, .. } => Some(source),
            Self::Sqlx(err) => Some(err),
            Self::Migrate(err) => Some(err),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for LocalDbError {
    fn from(err: sqlx::Error) -> Self {
        Self::Sqlx(err)
    }
}

impl From<sqlx::migrate::MigrateError> for LocalDbError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        Self::Migrate(err)
    }
}
