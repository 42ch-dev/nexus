//! Host-visible identifier types.
//!
//! All IDs are owned, `Hash`/`Eq`-compatible, and serializable for
//! event emission and telemetry.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a discovered provider.
///
/// Assigned by discovery or explicit config. ACP registry entries use the
/// agent ID from the registry; native CLI entries use distinct IDs like
/// `claude-native` to avoid collision (R-004, R-008).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub String);

impl ProviderId {
    /// Create a new provider ID from a string.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl From<String> for ProviderId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ProviderId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a host-managed session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostSessionId(pub Uuid);

impl HostSessionId {
    /// Create a new random session ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for HostSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for HostSessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a host operation within a session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostOperationId(pub Uuid);

impl HostOperationId {
    /// Create a new random operation ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for HostOperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for HostOperationId {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_id_display() {
        let id = ProviderId::new("claude-native");
        assert_eq!(id.to_string(), "claude-native");
    }

    #[test]
    fn provider_id_equality() {
        let a = ProviderId::new("claude");
        let b = ProviderId::new("claude");
        let c = ProviderId::new("codex");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn session_id_unique() {
        let a = HostSessionId::new();
        let b = HostSessionId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn operation_id_unique() {
        let a = HostOperationId::new();
        let b = HostOperationId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn ids_serialize_roundtrip() {
        let pid = ProviderId::new("test-provider");
        let sid = HostSessionId::new();
        let oid = HostOperationId::new();

        // Verify ProviderId serializes as newtype
        let pid_json = serde_json::to_string(&pid).unwrap();
        assert!(pid_json.contains("test-provider"));

        // Verify roundtrip
        let pid_back: ProviderId = serde_json::from_str(&pid_json).unwrap();
        assert_eq!(pid, pid_back);

        // Verify session/op IDs roundtrip
        let sid_json = serde_json::to_string(&sid).unwrap();
        let sid_back: HostSessionId = serde_json::from_str(&sid_json).unwrap();
        assert_eq!(sid, sid_back);

        let oid_json = serde_json::to_string(&oid).unwrap();
        let oid_back: HostOperationId = serde_json::from_str(&oid_json).unwrap();
        assert_eq!(oid, oid_back);
    }
}
