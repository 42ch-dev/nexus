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
    /// sqlx operation failed
    Sqlx(sqlx::Error),
    /// sqlx migration failed
    Migrate(sqlx::migrate::MigrateError),
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
            Self::Sqlx(err) => {
                write!(f, "database operation failed: {err}")
            }
            Self::Migrate(err) => {
                write!(f, "database migration failed: {err}")
            }
        }
    }
}

impl std::error::Error for LocalDbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
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
