//! Nexus CLI Error Types

use std::fmt;

use crate::acp::AcpError;

/// Nexus CLI result type
pub type Result<T> = std::result::Result<T, CliError>;

/// Nexus CLI errors
#[derive(Debug)]
#[allow(dead_code)]
pub enum CliError {
    #[allow(dead_code)]
    WorkspaceNotInitialized,

    DaemonNotRunning,

    /// Daemon not reachable with suggestion
    DaemonNotReachable {
        /// User-friendly error message
        message: String,
        /// Suggested fix
        suggestion: String,
    },

    Daemon {
        message: String,
    },

    #[allow(dead_code)]
    AuthenticationRequired,

    CreatorNotSelected,

    Network(reqwest::Error),

    Database(rusqlite::Error),

    Io(std::io::Error),

    Json(serde_json::Error),

    Config(String),

    Api {
        status: u16,
        message: String,
    },

    Acp(AcpError),

    /// Agent not found with ID
    AgentNotFound {
        /// Agent ID that was requested
        agent_id: String,
        /// User-friendly message
        message: String,
        /// Suggested fix
        suggestion: String,
    },

    /// Session expired
    SessionExpired {
        /// Session ID that expired
        session_id: String,
        /// User-friendly message
        message: String,
        /// Suggested fix
        suggestion: String,
    },

    /// Permission denied for tool execution
    PermissionDenied {
        /// Tool that was blocked
        tool: String,
        /// Reason for denial
        reason: String,
        /// User-friendly message
        message: String,
        /// Suggested fix
        suggestion: String,
    },

    Other(String),
}

// Implement std::error::Error for CliError
impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Network(err) => Some(err),
            Self::Database(err) => Some(err),
            Self::Io(err) => Some(err),
            Self::Json(err) => Some(err),
            Self::Acp(err) => Some(err),
            _ => None,
        }
    }
}

// Custom Display for enhanced error variants with suggestions
impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Use default #[error] messages for simple variants
            Self::WorkspaceNotInitialized => write!(f, "Workspace not initialized.\n\n  Suggestion: Run `nexus42 init` first."),
            Self::DaemonNotRunning => write!(f, "Daemon not running.\n\n  Suggestion: Start it with `nexus42 daemon start`."),
            Self::AuthenticationRequired => write!(f, "Authentication required.\n\n  Suggestion: Run `nexus42 auth login` first."),
            Self::CreatorNotSelected => write!(f, "Creator not selected.\n\n  Suggestion: Run `nexus42 creator use <creator-ref>` first."),

            // Enhanced error variants with suggestions
            Self::DaemonNotReachable { message, suggestion } => {
                write!(f, "{}\n\n  Suggestion: {}", message, suggestion)
            }
            Self::AgentNotFound { message, suggestion, .. } => {
                write!(f, "{}\n\n  Suggestion: {}", message, suggestion)
            }
            Self::SessionExpired { message, suggestion, .. } => {
                write!(f, "{}\n\n  Suggestion: {}", message, suggestion)
            }
            Self::PermissionDenied { message, suggestion, .. } => {
                write!(f, "{}\n\n  Suggestion: {}", message, suggestion)
            }

            // Use #[error] messages for other variants
            Self::Daemon { message } => write!(f, "Daemon error: {}", message),
            Self::Network(err) => write!(f, "Network error: {}", err),
            Self::Database(err) => write!(f, "Database error: {}", err),
            Self::Io(err) => write!(f, "IO error: {}", err),
            Self::Json(err) => write!(f, "JSON error: {}", err),
            Self::Config(msg) => write!(f, "Configuration error: {}", msg),
            Self::Api { status, message } => write!(f, "API error: {} — {}", status, message),
            Self::Acp(err) => write!(f, "ACP error: {}", err),
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

// Helper constructors for enhanced error variants
impl CliError {
    /// Create a DaemonNotReachable error with suggestion
    #[allow(dead_code)]
    pub fn daemon_not_reachable(suggestion: impl Into<String>) -> Self {
        Self::DaemonNotReachable {
            message: "The nexus42 daemon is not reachable.".to_string(),
            suggestion: suggestion.into(),
        }
    }

    /// Create an AgentNotFound error with agent ID
    #[allow(dead_code)]
    pub fn agent_not_found(agent_id: impl Into<String>) -> Self {
        let agent_id = agent_id.into();
        Self::AgentNotFound {
            agent_id: agent_id.clone(),
            message: format!("Agent '{}' not found.", agent_id),
            suggestion: "List available agents with `nexus42 agent list`.".to_string(),
        }
    }

    /// Create a SessionExpired error with session ID
    #[allow(dead_code)]
    pub fn session_expired(session_id: impl Into<String>) -> Self {
        let session_id = session_id.into();
        Self::SessionExpired {
            session_id: session_id.clone(),
            message: format!("Session '{}' has expired.", session_id),
            suggestion: "Create a new session with `nexus42 agent connect <agent>`.".to_string(),
        }
    }

    /// Create a PermissionDenied error for tool execution
    #[allow(dead_code)]
    pub fn permission_denied(tool: impl Into<String>, reason: impl Into<String>) -> Self {
        let tool = tool.into();
        let reason = reason.into();
        Self::PermissionDenied {
            tool: tool.clone(),
            reason,
            message: format!("Permission denied for tool: {}", tool),
            suggestion: "Check your permissions with `nexus42 auth status`.".to_string(),
        }
    }
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

impl From<reqwest::Error> for CliError {
    fn from(err: reqwest::Error) -> Self {
        CliError::Network(err)
    }
}

impl From<rusqlite::Error> for CliError {
    fn from(err: rusqlite::Error) -> Self {
        CliError::Database(err)
    }
}

impl From<std::io::Error> for CliError {
    fn from(err: std::io::Error) -> Self {
        CliError::Io(err)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(err: serde_json::Error) -> Self {
        CliError::Json(err)
    }
}

impl From<AcpError> for CliError {
    fn from(err: AcpError) -> Self {
        CliError::Acp(err)
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

    #[test]
    fn daemon_not_reachable_error_with_suggestion() {
        let err = CliError::daemon_not_reachable("Check if the daemon process is running");
        let display = format!("{}", err);

        assert!(display.contains("daemon is not reachable"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("Check if the daemon process is running"));
    }

    #[test]
    fn agent_not_found_error_with_suggestion() {
        let err = CliError::agent_not_found("agent-123");
        let display = format!("{}", err);

        assert!(display.contains("Agent 'agent-123' not found"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("nexus42 agent list"));
    }

    #[test]
    fn session_expired_error_with_suggestion() {
        let err = CliError::session_expired("sess-abc");
        let display = format!("{}", err);

        assert!(display.contains("Session 'sess-abc' has expired"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("nexus42 agent connect"));
    }

    #[test]
    fn permission_denied_error_with_suggestion() {
        let err = CliError::permission_denied("file_write", "Workspace policy denies write");
        let display = format!("{}", err);

        assert!(display.contains("Permission denied for tool: file_write"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("nexus42 auth status"));
    }

    #[test]
    fn workspace_not_initialized_with_suggestion() {
        let err = CliError::WorkspaceNotInitialized;
        let display = format!("{}", err);

        assert!(display.contains("Workspace not initialized"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("nexus42 init"));
    }

    #[test]
    fn daemon_not_running_with_suggestion() {
        let err = CliError::DaemonNotRunning;
        let display = format!("{}", err);

        assert!(display.contains("Daemon not running"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("nexus42 daemon start"));
    }
}
