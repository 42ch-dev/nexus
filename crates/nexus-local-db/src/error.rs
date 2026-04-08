//! Local database error types
//!
//! Provides descriptive errors for validation and version reading operations.

use std::fmt;

/// Local database errors with actionable descriptions
#[derive(Debug)]
pub enum LocalDbError {
    /// workspace_meta table does not exist
    MissingWorkspaceMetaTable,
    /// Required version key is missing from workspace_meta
    MissingVersionKey { key: String },
    /// Version value is not a valid u32 integer
    InvalidVersionValue {
        key: String,
        value: String,
        reason: String,
    },
    /// Rusqlite operation failed
    Rusqlite(rusqlite::Error),
}

impl fmt::Display for LocalDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingWorkspaceMetaTable => {
                write!(
                    f,
                    "workspace_meta table does not exist - database may not be initialized; call init() first"
                )
            }
            Self::MissingVersionKey { key } => {
                write!(
                    f,
                    "required key '{}' is missing from workspace_meta - database schema may be incomplete or corrupted; call init() to seed version keys",
                    key
                )
            }
            Self::InvalidVersionValue { key, value, reason } => {
                write!(
                    f,
                    "version key '{}' has invalid value '{}' - {}; database schema may be corrupted, consider re-initializing",
                    key, value, reason
                )
            }
            Self::Rusqlite(err) => {
                write!(f, "database operation failed: {}", err)
            }
        }
    }
}

impl std::error::Error for LocalDbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rusqlite(err) => Some(err),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for LocalDbError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Rusqlite(err)
    }
}
