//! Host error taxonomy and result type.
//!
//! Maps all host-visible error categories from the delivery compass §Error handling.
//! Each variant carries relevant context (`provider_id`, `session_id`, `op_id`) for
//! structured logging and telemetry.

use std::fmt;

use crate::ids::{HostOperationId, HostSessionId, ProviderId};

/// Result type for all host operations.
pub type HostResult<T> = Result<T, HostError>;

/// Host error taxonomy.
///
/// Maps concrete error categories for both ACP and native providers.
/// Each variant carries relevant context fields for diagnostics.
#[derive(Debug)]
pub enum HostError {
    /// Provider unavailable: registry entry missing, command not on PATH,
    /// or health probe failed.
    ProviderUnavailable {
        provider_id: ProviderId,
        message: String,
    },

    /// Provider launch failed: spawn/stdio/connection setup error.
    LaunchFailed {
        provider_id: ProviderId,
        message: String,
        source: Option<String>,
    },

    /// Requested capability not supported by the provider.
    CapabilityUnsupported {
        provider_id: ProviderId,
        capability: String,
        message: String,
    },

    /// Operation denied by host policy.
    PolicyDenied {
        provider_id: Option<ProviderId>,
        session_id: Option<HostSessionId>,
        message: String,
    },

    /// Stage-level timeout exceeded.
    OperationTimeout {
        provider_id: Option<ProviderId>,
        session_id: Option<HostSessionId>,
        op_id: Option<HostOperationId>,
        stage: String,
        message: String,
    },

    /// Operation cancelled by host or user.
    OperationCancelled {
        provider_id: Option<ProviderId>,
        session_id: Option<HostSessionId>,
        op_id: Option<HostOperationId>,
        message: String,
    },

    /// Protocol-level error from the provider.
    ProviderProtocolError {
        provider_id: Option<ProviderId>,
        session_id: Option<HostSessionId>,
        op_id: Option<HostOperationId>,
        message: String,
        source: Option<String>,
    },

    /// Internal host invariant violation or unexpected state.
    InternalHostError {
        message: String,
        source: Option<String>,
    },
}

impl HostError {
    /// Create a provider-unavailable error.
    #[must_use]
    pub fn provider_unavailable(
        provider_id: impl Into<ProviderId>,
        message: impl fmt::Display,
    ) -> Self {
        Self::ProviderUnavailable {
            provider_id: provider_id.into(),
            message: message.to_string(),
        }
    }

    /// Create a launch-failed error.
    #[must_use]
    pub fn launch_failed(
        provider_id: impl Into<ProviderId>,
        message: impl fmt::Display,
        source: Option<String>,
    ) -> Self {
        Self::LaunchFailed {
            provider_id: provider_id.into(),
            message: message.to_string(),
            source,
        }
    }

    /// Create a capability-unsupported error.
    #[must_use]
    pub fn capability_unsupported(
        provider_id: impl Into<ProviderId>,
        capability: impl fmt::Display,
        message: impl fmt::Display,
    ) -> Self {
        Self::CapabilityUnsupported {
            provider_id: provider_id.into(),
            capability: capability.to_string(),
            message: message.to_string(),
        }
    }

    /// Create a policy-denied error.
    #[must_use]
    pub fn policy_denied(message: impl fmt::Display) -> Self {
        Self::PolicyDenied {
            provider_id: None,
            session_id: None,
            message: message.to_string(),
        }
    }

    /// Create an operation-timeout error.
    #[must_use]
    pub fn timeout(stage: impl fmt::Display, message: impl fmt::Display) -> Self {
        Self::OperationTimeout {
            provider_id: None,
            session_id: None,
            op_id: None,
            stage: stage.to_string(),
            message: message.to_string(),
        }
    }

    /// Create an operation-cancelled error.
    #[must_use]
    pub fn cancelled(message: impl fmt::Display) -> Self {
        Self::OperationCancelled {
            provider_id: None,
            session_id: None,
            op_id: None,
            message: message.to_string(),
        }
    }

    /// Create a provider-protocol error.
    #[must_use]
    pub fn protocol_error(message: impl fmt::Display, source: Option<String>) -> Self {
        Self::ProviderProtocolError {
            provider_id: None,
            session_id: None,
            op_id: None,
            message: message.to_string(),
            source,
        }
    }

    /// Create an internal host error.
    #[must_use]
    pub fn internal(message: impl fmt::Display) -> Self {
        Self::InternalHostError {
            message: message.to_string(),
            source: None,
        }
    }

    /// Attach provider context to an error.
    #[must_use]
    pub fn with_provider(mut self, provider_id: ProviderId) -> Self {
        match &mut self {
            Self::ProviderUnavailable {
                provider_id: pid, ..
            }
            | Self::LaunchFailed {
                provider_id: pid, ..
            }
            | Self::CapabilityUnsupported {
                provider_id: pid, ..
            } => {
                *pid = provider_id;
            }
            Self::PolicyDenied {
                provider_id: pid, ..
            }
            | Self::OperationTimeout {
                provider_id: pid, ..
            }
            | Self::ProviderProtocolError {
                provider_id: pid, ..
            } => {
                *pid = Some(provider_id);
            }
            _ => {}
        }
        self
    }

    /// Attach session context to an error.
    #[must_use]
    pub const fn with_session(mut self, session_id: HostSessionId) -> Self {
        match &mut self {
            Self::PolicyDenied {
                session_id: sid, ..
            }
            | Self::OperationTimeout {
                session_id: sid, ..
            }
            | Self::OperationCancelled {
                session_id: sid, ..
            }
            | Self::ProviderProtocolError {
                session_id: sid, ..
            } => {
                *sid = Some(session_id);
            }
            _ => {}
        }
        self
    }

    /// Attach operation context to an error.
    #[must_use]
    pub const fn with_op(mut self, op_id: HostOperationId) -> Self {
        match &mut self {
            Self::OperationTimeout { op_id: oid, .. }
            | Self::OperationCancelled { op_id: oid, .. }
            | Self::ProviderProtocolError { op_id: oid, .. } => {
                *oid = Some(op_id);
            }
            _ => {}
        }
        self
    }

    /// Return the error category name for telemetry.
    #[must_use]
    pub const fn category(&self) -> &'static str {
        match self {
            Self::ProviderUnavailable { .. } => "provider_unavailable",
            Self::LaunchFailed { .. } => "launch_failed",
            Self::CapabilityUnsupported { .. } => "capability_unsupported",
            Self::PolicyDenied { .. } => "policy_denied",
            Self::OperationTimeout { .. } => "operation_timeout",
            Self::OperationCancelled { .. } => "operation_cancelled",
            Self::ProviderProtocolError { .. } => "provider_protocol_error",
            Self::InternalHostError { .. } => "internal_host_error",
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProviderUnavailable {
                provider_id,
                message,
            } => {
                write!(f, "provider unavailable [{provider_id}]: {message}")
            }
            Self::LaunchFailed {
                provider_id,
                message,
                source,
            } => {
                write!(f, "launch failed [{provider_id}]: {message}")?;
                if let Some(src) = source {
                    write!(f, " (source: {src})")?;
                }
                Ok(())
            }
            Self::CapabilityUnsupported {
                provider_id,
                capability,
                message,
            } => {
                write!(
                    f,
                    "capability unsupported [{provider_id}/{capability}]: {message}"
                )
            }
            Self::PolicyDenied { message, .. } => {
                write!(f, "policy denied: {message}")
            }
            Self::OperationTimeout { stage, message, .. } => {
                write!(f, "timeout [{stage}]: {message}")
            }
            Self::OperationCancelled { message, .. } => {
                write!(f, "cancelled: {message}")
            }
            Self::ProviderProtocolError {
                message, source, ..
            } => {
                write!(f, "protocol error: {message}")?;
                if let Some(src) = source {
                    write!(f, " (source: {src})")?;
                }
                Ok(())
            }
            Self::InternalHostError { message, source } => {
                write!(f, "internal host error: {message}")?;
                if let Some(src) = source {
                    write!(f, " (source: {src})")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for HostError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_provider_unavailable() {
        let err = HostError::provider_unavailable("claude", "command not found on PATH");
        let msg = err.to_string();
        assert!(msg.contains("provider unavailable"));
        assert!(msg.contains("claude"));
        assert!(msg.contains("command not found"));
    }

    #[test]
    fn display_launch_failed() {
        let err =
            HostError::launch_failed("claude", "spawn failed", Some("permission denied".into()));
        let msg = err.to_string();
        assert!(msg.contains("launch failed"));
        assert!(msg.contains("source: permission denied"));
    }

    #[test]
    fn display_capability_unsupported() {
        let err = HostError::capability_unsupported("claude-native", "streaming", "not available");
        assert!(err.to_string().contains("capability unsupported"));
    }

    #[test]
    fn display_policy_denied() {
        let err = HostError::policy_denied("unknown provider rejected");
        assert!(err.to_string().contains("policy denied"));
    }

    #[test]
    fn display_timeout() {
        let err = HostError::timeout("initialize", "15s exceeded");
        assert!(err.to_string().contains("timeout [initialize]"));
    }

    #[test]
    fn display_cancelled() {
        let err = HostError::cancelled("user requested");
        assert!(err.to_string().contains("cancelled"));
    }

    #[test]
    fn display_protocol_error() {
        let err =
            HostError::protocol_error("invalid JSON-RPC response", Some("parse error".into()));
        assert!(err.to_string().contains("protocol error"));
    }

    #[test]
    fn display_internal() {
        let err = HostError::internal("session registry invariant violated");
        assert!(err.to_string().contains("internal host error"));
    }

    #[test]
    fn category_names() {
        assert_eq!(
            HostError::provider_unavailable("x", "y").category(),
            "provider_unavailable"
        );
        assert_eq!(
            HostError::launch_failed("x", "y", None).category(),
            "launch_failed"
        );
        assert_eq!(
            HostError::capability_unsupported("x", "y", "z").category(),
            "capability_unsupported"
        );
        assert_eq!(HostError::policy_denied("y").category(), "policy_denied");
        assert_eq!(HostError::timeout("s", "m").category(), "operation_timeout");
        assert_eq!(HostError::cancelled("m").category(), "operation_cancelled");
        assert_eq!(
            HostError::protocol_error("m", None).category(),
            "provider_protocol_error"
        );
        assert_eq!(HostError::internal("m").category(), "internal_host_error");
    }

    #[test]
    fn with_provider_attaches_context() {
        let err = HostError::timeout("prompt", "exceeded").with_provider(ProviderId::new("claude"));
        match err {
            HostError::OperationTimeout { provider_id, .. } => {
                assert_eq!(provider_id.unwrap().0, "claude");
            }
            _ => panic!("expected OperationTimeout"),
        }
    }

    #[test]
    fn with_session_attaches_context() {
        let sid = HostSessionId::new();
        let err = HostError::cancelled("user requested").with_session(sid.clone());
        match err {
            HostError::OperationCancelled { session_id, .. } => {
                assert_eq!(session_id.unwrap(), sid);
            }
            _ => panic!("expected OperationCancelled"),
        }
    }

    #[test]
    fn with_op_attaches_context() {
        let oid = HostOperationId::new();
        let err = HostError::protocol_error("bad response", None).with_op(oid.clone());
        match err {
            HostError::ProviderProtocolError { op_id, .. } => {
                assert_eq!(op_id.unwrap(), oid);
            }
            _ => panic!("expected ProviderProtocolError"),
        }
    }
}
