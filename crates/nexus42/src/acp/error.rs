//! ACP-specific error types for the nexus42 CLI.
//!
//! Covers all error scenarios arising from ACP communication:
//! connection failures, timeouts, protocol errors, agent crashes,
//! and missing agent installations.

use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

/// ACP result type alias.
#[allow(dead_code)]
pub type AcpResult<T> = std::result::Result<T, AcpError>;

/// Errors that can occur during ACP client operations.
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum AcpError {
    /// Failed to establish a connection to the agent subprocess.
    #[error("failed to connect to agent: {message}")]
    ConnectionFailed {
        /// Human-readable error detail.
        message: String,
        /// Optional underlying I/O error.
        #[source]
        source: Option<std::io::Error>,
    },

    /// An ACP operation timed out.
    #[error("ACP operation timed out after {duration:?}: {operation}")]
    Timeout {
        /// Which operation timed out.
        operation: String,
        /// The configured timeout duration.
        duration: Duration,
    },

    /// The agent returned an invalid or unexpected protocol response.
    #[error("ACP protocol error: {message}")]
    Protocol {
        /// Human-readable error detail.
        message: String,
    },

    /// The agent subprocess terminated unexpectedly (non-zero exit, signal, etc.).
    #[error("agent crashed: {details}")]
    AgentCrashed {
        /// Exit code of the subprocess (None if killed by signal).
        exit_code: Option<i32>,
        /// Path to the agent binary or command that crashed.
        agent_path: PathBuf,
        /// Captured stderr output (if available).
        stderr_output: Option<String>,
        /// Formatted details string (computed).
        details: String,
    },

    /// The requested agent is not installed and cannot be resolved.
    #[error("agent not installed: {agent_id}. Run `nexus42 agent run {agent_id}` to start, or install it first.")]
    NotInstalled {
        /// Agent identifier that was not found.
        agent_id: String,
    },

    /// The agent executable (e.g. `npx`, `node`) was not found on PATH.
    #[error("required executable not found on PATH: {executable}")]
    ExecutableNotFound {
        /// Name of the missing executable.
        executable: String,
    },

    /// An error from the underlying ACP SDK.
    #[error("ACP SDK error: {0}")]
    Sdk(String),

    /// A general I/O error during ACP operations.
    #[error("I/O error during ACP operation: {0}")]
    Io(#[from] std::io::Error),

    /// A JSON serialization/deserialization error.
    #[error("JSON error during ACP operation: {0}")]
    Json(#[from] serde_json::Error),
}

// ── Constructors ──────────────────────────────────────────────────────

#[allow(dead_code)]
impl AcpError {
    /// Create a connection-failed error from a message string.
    pub fn connection_failed(message: impl Into<String>) -> Self {
        Self::ConnectionFailed {
            message: message.into(),
            source: None,
        }
    }

    /// Create a connection-failed error wrapping an I/O error.
    pub fn connection_io(err: std::io::Error) -> Self {
        Self::ConnectionFailed {
            message: err.to_string(),
            source: Some(err),
        }
    }

    /// Create a timeout error for a named operation.
    pub fn timeout(operation: impl Into<String>, duration: Duration) -> Self {
        Self::Timeout {
            operation: operation.into(),
            duration,
        }
    }

    /// Create a protocol error from a message.
    pub fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol {
            message: message.into(),
        }
    }

    /// Create an agent-crashed error.
    pub fn agent_crashed(
        exit_code: Option<i32>,
        agent_path: PathBuf,
        stderr_output: Option<String>,
    ) -> Self {
        let mut details = String::new();
        match exit_code {
            Some(code) => details.push_str(&format!("exit code {}", code)),
            None => details.push_str("killed by signal"),
        }
        details.push_str(&format!(" ({})", agent_path.display()));
        if let Some(ref stderr) = stderr_output {
            if !stderr.is_empty() {
                let truncated = if stderr.len() > 200 {
                    format!("{}...", &stderr[..200])
                } else {
                    stderr.clone()
                };
                details.push_str(&format!("\n  stderr: {}", truncated));
            }
        }
        Self::AgentCrashed {
            exit_code,
            agent_path,
            stderr_output,
            details,
        }
    }

    /// Create a not-installed error for the given agent ID.
    pub fn not_installed(agent_id: impl Into<String>) -> Self {
        Self::NotInstalled {
            agent_id: agent_id.into(),
        }
    }

    /// Create an executable-not-found error.
    pub fn executable_not_found(executable: impl Into<String>) -> Self {
        Self::ExecutableNotFound {
            executable: executable.into(),
        }
    }

    /// Wrap an ACP SDK error.
    pub fn sdk(err: agent_client_protocol::Error) -> Self {
        Self::Sdk(err.to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn connection_failed_display() {
        let err = AcpError::connection_failed("pipe broken");
        let msg = err.to_string();
        assert!(msg.contains("failed to connect to agent"));
        assert!(msg.contains("pipe broken"));
    }

    #[test]
    fn timeout_display() {
        let err = AcpError::timeout("initialize", Duration::from_secs(30));
        let msg = err.to_string();
        assert!(msg.contains("timed out"));
        assert!(msg.contains("initialize"));
        assert!(msg.contains("30s"));
    }

    #[test]
    fn protocol_error_display() {
        let err = AcpError::protocol("unexpected response type");
        let msg = err.to_string();
        assert!(msg.contains("protocol error"));
        assert!(msg.contains("unexpected response type"));
    }

    #[test]
    fn agent_crashed_display() {
        let err = AcpError::agent_crashed(
            Some(137),
            PathBuf::from("/usr/bin/claude-acp"),
            Some("segfault\n".to_string()),
        );
        let msg = err.to_string();
        assert!(msg.contains("agent crashed"));
        assert!(msg.contains("137"));
        assert!(msg.contains("segfault"));
    }

    #[test]
    fn agent_crashed_no_stderr() {
        let err = AcpError::agent_crashed(None, PathBuf::from("npx"), None);
        let msg = err.to_string();
        assert!(msg.contains("agent crashed"));
        // Should not have a stderr hint line
        assert!(!msg.contains("stderr"));
    }

    #[test]
    fn not_installed_display() {
        let err = AcpError::not_installed("claude-acp");
        let msg = err.to_string();
        assert!(msg.contains("not installed"));
        assert!(msg.contains("claude-acp"));
    }

    #[test]
    fn executable_not_found_display() {
        let err = AcpError::executable_not_found("npx");
        let msg = err.to_string();
        assert!(msg.contains("not found on PATH"));
        assert!(msg.contains("npx"));
    }

    #[test]
    fn io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
        let err: AcpError = io_err.into();
        let msg = err.to_string();
        assert!(msg.contains("I/O error"));
        assert!(msg.contains("pipe broken"));
    }

    #[test]
    fn json_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err: AcpError = json_err.into();
        let msg = err.to_string();
        assert!(msg.contains("JSON error"));
    }

    #[test]
    fn stderr_hint_truncation() {
        let long_stderr = "x".repeat(300);
        let err = AcpError::agent_crashed(Some(1), PathBuf::from("agent"), Some(long_stderr));
        let msg = err.to_string();
        // Should contain truncation marker
        assert!(msg.contains("..."));
    }
}
