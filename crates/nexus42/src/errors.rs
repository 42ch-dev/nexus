//! Nexus CLI Error Types

use thiserror::Error;

use crate::acp::AcpError;

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

    #[error("Daemon error: {message}")]
    Daemon { message: String },

    #[error("Authentication required. Run `nexus42 auth login` first.")]
    #[allow(dead_code)]
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

    #[error("ACP error: {0}")]
    Acp(#[from] AcpError),

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

impl From<nexus_domain::errors::DomainError> for CliError {
    fn from(err: nexus_domain::errors::DomainError) -> Self {
        CliError::Other(format!("Domain error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_error_converts_to_cli_error() {
        let acp_err = AcpError::connection_failed("test connection error");
        let cli_err: CliError = acp_err.into();

        match cli_err {
            CliError::Acp(err) => {
                assert!(err.to_string().contains("test connection error"));
            }
            _ => panic!("Expected CliError::Acp variant"),
        }
    }

    #[test]
    fn acp_timeout_error_message() {
        let acp_err = AcpError::timeout("initialize", std::time::Duration::from_secs(30));
        let cli_err: CliError = acp_err.into();

        let message = cli_err.to_string();
        assert!(message.contains("ACP error"));
        assert!(message.contains("timed out"));
        assert!(message.contains("initialize"));
    }

    #[test]
    fn acp_not_installed_error_message() {
        let acp_err = AcpError::not_installed("claude-acp");
        let cli_err: CliError = acp_err.into();

        let message = cli_err.to_string();
        assert!(message.contains("not installed"));
        assert!(message.contains("claude-acp"));
    }
}
