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
            } => {
                write!(
                    f,
                    "version mismatch on '{table}' row '{id}': expected v{expected}, actual v{} — \
                     row was modified by another writer; retry",
                    actual.map_or("?".to_string(), |v| v.to_string())
                )
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
