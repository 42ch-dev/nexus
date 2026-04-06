//! Nexus CLI Error Types

use thiserror::Error;

/// Nexus CLI result type
pub type Result<T> = std::result::Result<T, CliError>;

/// Nexus CLI errors
#[derive(Debug, Error)]
pub enum CliError {
    #[allow(dead_code)]
    #[error("Workspace not initialized. Run `nexus42 init` first.")]
    WorkspaceNotInitialized,

    #[error("Daemon not running. Start it with `nexus42 daemon start`.")]
    DaemonNotRunning,

    #[error("Authentication required. Run `nexus42 auth login` first.")]
    AuthenticationRequired,

    #[error("Creator not selected. Run `nexus42 creator use <creator-ref>` first.")]
    CreatorNotSelected,

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("API error: {status} — {message}")]
    Api { status: u16, message: String },

    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for CliError {
    fn from(err: anyhow::Error) -> Self {
        CliError::Other(err.to_string())
    }
}

impl From<chrono::ParseError> for CliError {
    fn from(err: chrono::ParseError) -> Self {
        CliError::Other(format!("Date parse error: {}", err))
    }
}
