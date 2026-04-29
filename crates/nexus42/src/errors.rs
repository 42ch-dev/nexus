//! Nexus CLI Error Types

use std::fmt;

use nexus_acp_host::AcpError;

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

    Database(sqlx::Error),

    Io(std::io::Error),

    Json(serde_json::Error),

    Config(String),

    Api {
        status: u16,
        message: String,
    },

    Acp(AcpError),

    /// Operation requires platform connectivity but current mode prohibits it.
    PlatformOperationProhibited {
        /// Runtime mode that blocked the operation (e.g. "`local_only`")
        mode: String,
        /// Operation that was blocked (e.g. "sync push")
        operation: String,
    },

    /// Challenge solving failed during creator registration.
    ChallengeFailed {
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// Creator registration failed on the platform.
    CreatorRegistrationFailed {
        /// HTTP status code (if available).
        status: u16,
        /// Human-readable message.
        message: String,
    },

    /// Creator verification failed.
    CreatorVerificationFailed {
        /// Verification status from the platform (e.g. "`wrong_answer`", "expired", "locked").
        status: String,
        /// Human-readable message with next steps.
        message: String,
    },

    /// Challenge has expired before solving could complete.
    ChallengeExpired {
        /// Expiry timestamp.
        expires_at: String,
    },

    /// Invalid creator handle format.
    InvalidHandle {
        /// The invalid handle value.
        handle: String,
        /// Human-readable reason.
        reason: String,
    },

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
            Self::ChallengeFailed { reason } => {
                write!(
                    f,
                    "Challenge solving failed: {reason}\n\n  Suggestion: \
                     Try registering again with `nexus42 creator register <name>`. \
                     If the problem persists, the challenge format may be unsupported."
                )
            }
            Self::CreatorRegistrationFailed { status, message } => {
                write!(
                    f,
                    "Creator registration failed (HTTP {status}): {message}\n\n  Suggestion: \
                     Check your authentication with `nexus42 auth status` and try again."
                )
            }
            Self::CreatorVerificationFailed { status, message } => {
                write!(
                    f,
                    "Creator verification failed ({}): {}\n\n  Suggestion: \
                     {}",
                    status, message,
                    match status.as_str() {
                        "wrong_answer" => "The auto-retry has been exhausted. Register again with `nexus42 creator register <name>`.",
                        "expired" => "The challenge timed out. Register again with `nexus42 creator register <name>`.",
                        "locked" => "Your account has been permanently locked due to too many failed attempts. Contact support.",
                        _ => "Try registering again.",
                    }
                )
            }
            Self::ChallengeExpired { expires_at } => {
                write!(
                    f,
                    "Challenge expired at {expires_at}. Register again with `nexus42 creator register <name>`."
                )
            }
            Self::InvalidHandle { handle, reason } => {
                write!(
                    f,
                    "Invalid handle {handle:?}: {reason}\n\n  Suggestion: \
                     Handle must be 4–15 characters, start and end with a lowercase letter or digit, \
                     and contain only lowercase letters, digits, dots, hyphens, and underscores."
                )
            }
            Self::DaemonNotReachable { message, suggestion }
            | Self::AgentNotFound { message, suggestion, .. }
            | Self::SessionExpired { message, suggestion, .. }
            | Self::PermissionDenied { message, suggestion, .. } => {
                write!(f, "{message}\n\n  Suggestion: {suggestion}")
            }

            // Use #[error] messages for other variants
            Self::Daemon { message } => write!(f, "Daemon error: {message}"),
            Self::Network(err) => write!(f, "Network error: {err}"),
            Self::Database(err) => write!(f, "Database error: {err}"),
            Self::Io(err) => write!(f, "IO error: {err}"),
            Self::Json(err) => write!(f, "JSON error: {err}"),
            Self::Config(msg) => write!(f, "Configuration error: {msg}"),
            Self::Api { status, message } => write!(f, "API error: {status} — {message}"),
            Self::Acp(err) => write!(f, "ACP error: {err}"),
            Self::PlatformOperationProhibited { mode, operation } => {
                write!(
                    f,
                    "Operation '{operation}' is not available in {mode} mode.\n\n  Suggestion: \
                     This operation requires platform connectivity. Switch to \
                     `local_first` or `cloud_enhanced` mode with `nexus42 config set runtime_mode <mode>`."
                )
            }
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

// Helper constructors for enhanced error variants
impl CliError {
    /// Create a `DaemonNotReachable` error with suggestion
    #[allow(dead_code)]
    pub fn daemon_not_reachable(suggestion: impl Into<String>) -> Self {
        Self::DaemonNotReachable {
            message: "The nexus42 daemon is not reachable.".to_string(),
            suggestion: suggestion.into(),
        }
    }

    /// Create an `AgentNotFound` error with agent ID
    #[allow(dead_code)]
    pub fn agent_not_found(agent_id: impl Into<String>) -> Self {
        let agent_id = agent_id.into();
        Self::AgentNotFound {
            agent_id: agent_id.clone(),
            message: format!("Agent '{agent_id}' not found."),
            suggestion: "List available agents with `nexus42 agent list`.".to_string(),
        }
    }

    /// Create a `SessionExpired` error with session ID
    #[allow(dead_code)]
    pub fn session_expired(session_id: impl Into<String>) -> Self {
        let session_id = session_id.into();
        Self::SessionExpired {
            session_id: session_id.clone(),
            message: format!("Session '{session_id}' has expired."),
            suggestion: "Create a new session with `nexus42 agent connect <agent>`.".to_string(),
        }
    }

    /// Create a `PermissionDenied` error for tool execution
    #[allow(dead_code)]
    pub fn permission_denied(tool: impl Into<String>, reason: impl Into<String>) -> Self {
        let tool = tool.into();
        let reason = reason.into();
        Self::PermissionDenied {
            tool: tool.clone(),
            reason,
            message: format!("Permission denied for tool: {tool}"),
            suggestion: "Check your permissions with `nexus42 auth status`.".to_string(),
        }
    }
}

impl From<anyhow::Error> for CliError {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(err.to_string())
    }
}

impl From<chrono::ParseError> for CliError {
    fn from(err: chrono::ParseError) -> Self {
        Self::Other(format!("Date parse error: {err}"))
    }
}

impl From<nexus_domain::errors::DomainError> for CliError {
    fn from(err: nexus_domain::errors::DomainError) -> Self {
        match err {
            nexus_domain::errors::DomainError::PlatformOperationProhibited { mode, operation } => {
                Self::PlatformOperationProhibited { mode, operation }
            }
            other => Self::Other(format!("Domain error: {other}")),
        }
    }
}

impl From<reqwest::Error> for CliError {
    fn from(err: reqwest::Error) -> Self {
        Self::Network(err)
    }
}

impl From<sqlx::Error> for CliError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err)
    }
}

impl From<std::io::Error> for CliError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<AcpError> for CliError {
    fn from(err: AcpError) -> Self {
        Self::Acp(err)
    }
}

impl From<nexus_local_db::LocalDbError> for CliError {
    fn from(err: nexus_local_db::LocalDbError) -> Self {
        Self::Other(format!("local database error: {err}"))
    }
}

impl From<nexus_sync::errors::SyncError> for CliError {
    fn from(err: nexus_sync::errors::SyncError) -> Self {
        match err {
            nexus_sync::errors::SyncError::PlatformError { status, body } => {
                Self::CreatorRegistrationFailed {
                    status,
                    message: body,
                }
            }
            nexus_sync::errors::SyncError::SyncNotConfigured(msg) => Self::Config(msg),
            nexus_sync::errors::SyncError::HttpError(e) => Self::Network(e),
            other => Self::Other(format!("sync error: {other}")),
        }
    }
}

impl CliError {
    /// Convert a [`SyncError`] into a `CreatorVerificationFailed` error.
    ///
    /// Use this instead of `SyncError::into()` when the error occurs during
    /// the verification step (as opposed to registration), so callers can
    /// distinguish registration failures from verification failures.
    #[must_use] 
    pub fn verify_creator_error(err: nexus_sync::errors::SyncError) -> Self {
        match err {
            nexus_sync::errors::SyncError::PlatformError { status, body } => {
                Self::CreatorVerificationFailed {
                    status: status.to_string(),
                    message: body,
                }
            }
            nexus_sync::errors::SyncError::SyncNotConfigured(msg) => Self::Config(msg),
            nexus_sync::errors::SyncError::HttpError(e) => Self::Network(e),
            other => Self::Other(format!("sync error: {other}")),
        }
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
        let display = format!("{err}");

        assert!(display.contains("daemon is not reachable"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("Check if the daemon process is running"));
    }

    #[test]
    fn agent_not_found_error_with_suggestion() {
        let err = CliError::agent_not_found("agent-123");
        let display = format!("{err}");

        assert!(display.contains("Agent 'agent-123' not found"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("nexus42 agent list"));
    }

    #[test]
    fn session_expired_error_with_suggestion() {
        let err = CliError::session_expired("sess-abc");
        let display = format!("{err}");

        assert!(display.contains("Session 'sess-abc' has expired"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("nexus42 agent connect"));
    }

    #[test]
    fn permission_denied_error_with_suggestion() {
        let err = CliError::permission_denied("file_write", "Workspace policy denies write");
        let display = format!("{err}");

        assert!(display.contains("Permission denied for tool: file_write"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("nexus42 auth status"));
    }

    #[test]
    fn workspace_not_initialized_with_suggestion() {
        let err = CliError::WorkspaceNotInitialized;
        let display = format!("{err}");

        assert!(display.contains("Workspace not initialized"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("nexus42 init"));
    }

    #[test]
    fn daemon_not_running_with_suggestion() {
        let err = CliError::DaemonNotRunning;
        let display = format!("{err}");

        assert!(display.contains("Daemon not running"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("nexus42 daemon start"));
    }

    #[test]
    fn platform_operation_prohibited_display() {
        let err = CliError::PlatformOperationProhibited {
            mode: "local_only".to_string(),
            operation: "sync push".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("not available in local_only mode"));
        assert!(display.contains("sync push"));
        assert!(display.contains("Suggestion:"));
    }

    #[test]
    fn domain_error_platform_prohibited_maps_to_cli_variant() {
        let domain_err = nexus_domain::errors::DomainError::PlatformOperationProhibited {
            mode: "local_only".to_string(),
            operation: "sync push".to_string(),
        };
        let cli_err: CliError = domain_err.into();
        match cli_err {
            CliError::PlatformOperationProhibited { mode, operation } => {
                assert_eq!(mode, "local_only");
                assert_eq!(operation, "sync push");
            }
            _ => panic!("Expected CliError::PlatformOperationProhibited variant"),
        }
    }

    #[test]
    fn domain_error_other_maps_to_cli_other() {
        let domain_err = nexus_domain::errors::DomainError::ValidationError("test".to_string());
        let cli_err: CliError = domain_err.into();
        match cli_err {
            CliError::Other(msg) => {
                assert!(msg.contains("Domain error:"));
                assert!(msg.contains("validation error"));
            }
            _ => panic!("Expected CliError::Other variant"),
        }
    }

    #[test]
    fn verify_creator_error_maps_platform_error_to_verification_failed() {
        let sync_err = nexus_sync::errors::SyncError::PlatformError {
            status: 403,
            body: "verification token invalid".to_string(),
        };
        let cli_err = CliError::verify_creator_error(sync_err);
        match cli_err {
            CliError::CreatorVerificationFailed { status, message } => {
                assert_eq!(status, "403");
                assert_eq!(message, "verification token invalid");
            }
            _ => panic!("Expected CreatorVerificationFailed variant"),
        }
    }

    #[test]
    fn verify_creator_error_maps_not_configured_to_config() {
        let sync_err = nexus_sync::errors::SyncError::SyncNotConfigured(
            "platform_base_url is required".to_string(),
        );
        let cli_err = CliError::verify_creator_error(sync_err);
        assert!(matches!(cli_err, CliError::Config(_)));
    }
}
