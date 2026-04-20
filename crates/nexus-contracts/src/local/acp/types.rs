//! Nexus-owned ACP DTO type definitions.
//!
//! Each type has `From<acp_sdk_type>` conversions in the same module
//! (or in `nexus-acp-host/src/client.rs` for types that depend on the SDK crate).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Newtype wrappers ────────────────────────────────────────────────

/// Nexus-owned session identifier.
///
/// Wraps an opaque string. The inner value is opaque to consumers — it is
/// only produced by the SDK adapter and passed back to it for protocol
/// operations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NexusSessionId(pub String);

impl NexusSessionId {
    /// Create a new session ID from a string.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for NexusSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for NexusSessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NexusSessionId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Nexus-owned protocol version identifier.
///
/// Wraps the negotiated protocol version as a string for flexibility
/// across SDK versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NexusProtocolVersion(pub String);

impl NexusProtocolVersion {
    /// Create a new protocol version from a string.
    #[must_use]
    pub fn new(version: impl Into<String>) -> Self {
        Self(version.into())
    }

    /// Protocol version "1" (current ACP spec).
    pub fn v1() -> Self {
        Self(String::from("1"))
    }

    /// The latest supported protocol version.
    pub fn latest() -> Self {
        Self::v1()
    }
}

impl std::fmt::Display for NexusProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Stop reason ─────────────────────────────────────────────────────

/// Why the agent stopped processing a prompt turn.
///
/// Mirrors the ACP spec `StopReason` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NexusStopReason {
    /// The turn ended successfully.
    EndTurn,
    /// The agent reached the maximum token count.
    MaxTokens,
    /// The agent reached the maximum number of allowed requests.
    MaxTurnRequests,
    /// The agent refused to continue.
    Refusal,
    /// The turn was cancelled by the client.
    Cancelled,
}

// ── Auth method ─────────────────────────────────────────────────────

/// An authentication method reported by the agent during initialization.
///
/// This is a simplified view — only the fields consumers actually inspect
/// (id, name) are exposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusAuthMethod {
    /// Unique identifier for this authentication method.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ── Agent info ──────────────────────────────────────────────────────

/// Metadata about the agent implementation (name, version).
///
/// Mirrors SDK `Implementation` — consumers read `name` and `version`
/// for display and logging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusAgentInfo {
    /// Programmatic identifier (e.g. "claude-code").
    pub name: String,
    /// Optional human-readable title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Version string (e.g. "1.0.0").
    pub version: String,
}

// ── Agent capabilities ──────────────────────────────────────────────

/// Capabilities reported by the agent during initialization.
///
/// This is a simplified view exposing only the fields Nexus consumers use.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusAgentCapabilities {
    /// Whether the agent supports `session/load`.
    #[serde(default)]
    pub load_session: bool,
}

// ── Session mode ────────────────────────────────────────────────────

/// A mode the agent can operate in (e.g. "act" / "ask").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusSessionMode {
    /// Mode identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The agent's current session mode state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusSessionModeState {
    /// The current mode the agent is in.
    pub current_mode_id: String,
    /// The set of modes the agent can operate in.
    pub available_modes: Vec<NexusSessionMode>,
}

// ── Content block ───────────────────────────────────────────────────

/// A content block in a prompt or response.
///
/// This is a simplified view — only `Text` and `ResourceLink` variants
/// are needed by Nexus consumers today. New variants can be added as
/// needed without breaking the SDK boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NexusContentBlock {
    /// Text content.
    Text(NexusTextContent),
    /// Reference to a resource.
    ResourceLink(NexusResourceLink),
}

/// Text content within a content block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NexusTextContent {
    /// The text content.
    pub text: String,
}

/// A reference to a resource within a content block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusResourceLink {
    /// The resource URI.
    pub uri: String,
    /// Optional resource name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

// ── MCP server ──────────────────────────────────────────────────────

/// An MCP server configuration for a new session.
///
/// Mirrors the SDK `McpServer` enum — consumers construct these for
/// `NexusNewSessionRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NexusMcpServer {
    /// HTTP-based MCP server.
    #[serde(rename = "http")]
    Http(NexusMcpServerHttp),
    /// SSE-based MCP server.
    #[serde(rename = "sse")]
    Sse(NexusMcpServerSse),
    /// Stdio-based MCP server.
    #[serde(rename = "stdio")]
    Stdio(NexusMcpServerStdio),
}

/// HTTP MCP server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusMcpServerHttp {
    /// Server name.
    pub name: String,
    /// Server URL.
    pub url: String,
}

/// SSE MCP server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusMcpServerSse {
    /// Server name.
    pub name: String,
    /// Server URL.
    pub url: String,
}

/// Stdio MCP server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusMcpServerStdio {
    /// Server name.
    pub name: String,
    /// Command to execute.
    pub command: PathBuf,
}

// ── Request DTOs ────────────────────────────────────────────────────

/// Request for the ACP `initialize` handshake.
///
/// This replaces the SDK `InitializeRequest` in trait signatures.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusInitializeRequest {
    /// Protocol version the client supports.
    pub protocol_version: NexusProtocolVersion,
    /// Client capabilities (simplified view).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_info: Option<NexusAgentInfo>,
}

impl NexusInitializeRequest {
    /// Create a new initialize request with the latest protocol version.
    #[must_use]
    pub fn new() -> Self {
        Self {
            protocol_version: NexusProtocolVersion::latest(),
            client_info: None,
        }
    }

    /// Set client info.
    #[must_use]
    pub fn client_info(mut self, info: NexusAgentInfo) -> Self {
        self.client_info = Some(info);
        self
    }
}

impl Default for NexusInitializeRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Request to create a new ACP session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusNewSessionRequest {
    /// Working directory for the session.
    pub cwd: PathBuf,
    /// MCP servers to connect to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<NexusMcpServer>,
}

impl NexusNewSessionRequest {
    /// Create a new session request with the given working directory.
    #[must_use]
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            mcp_servers: vec![],
        }
    }

    /// Add MCP servers.
    #[must_use]
    pub fn mcp_servers(mut self, servers: Vec<NexusMcpServer>) -> Self {
        self.mcp_servers = servers;
        self
    }
}

/// Request to send a prompt to the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusPromptRequest {
    /// Session ID to send the prompt to.
    pub session_id: NexusSessionId,
    /// Content blocks composing the user's message.
    pub prompt: Vec<NexusContentBlock>,
}

impl NexusPromptRequest {
    /// Create a new prompt request.
    #[must_use]
    pub fn new(session_id: NexusSessionId, prompt: Vec<NexusContentBlock>) -> Self {
        Self { session_id, prompt }
    }
}

// ── Response DTOs ───────────────────────────────────────────────────

/// Response from the ACP `initialize` handshake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusInitializeResponse {
    /// Negotiated protocol version.
    pub protocol_version: NexusProtocolVersion,
    /// Capabilities reported by the agent.
    pub agent_capabilities: NexusAgentCapabilities,
    /// Agent metadata (name, version).
    pub agent_info: Option<NexusAgentInfo>,
    /// Authentication methods supported by the agent.
    pub auth_methods: Vec<NexusAuthMethod>,
}

/// Response from creating a new ACP session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusSessionCreated {
    /// Unique session identifier.
    pub session_id: NexusSessionId,
    /// Session mode state (if the agent reports modes).
    pub modes: Option<NexusSessionModeState>,
}

/// Response from sending a prompt to the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusPromptCompleted {
    /// Why the agent stopped processing the turn.
    pub stop_reason: NexusStopReason,
}

/// Result of a cancel operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusCancelResult {
    /// The session that was cancelled.
    pub session_id: NexusSessionId,
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nexus_session_id_roundtrip() {
        let id = NexusSessionId::new("test-session-123");
        assert_eq!(id.0, "test-session-123");
        assert_eq!(format!("{id}"), "test-session-123");

        let json = serde_json::to_string(&id).unwrap();
        let deserialized: NexusSessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn nexus_protocol_version_roundtrip() {
        let v = NexusProtocolVersion::latest();
        assert_eq!(v.0, "1");

        let json = serde_json::to_string(&v).unwrap();
        let deserialized: NexusProtocolVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(v, deserialized);
    }

    #[test]
    fn nexus_stop_reason_roundtrip() {
        for reason in [
            NexusStopReason::EndTurn,
            NexusStopReason::MaxTokens,
            NexusStopReason::MaxTurnRequests,
            NexusStopReason::Refusal,
            NexusStopReason::Cancelled,
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            let deserialized: NexusStopReason = serde_json::from_str(&json).unwrap();
            assert_eq!(reason, deserialized);
        }
    }

    #[test]
    fn nexus_initialize_request_default() {
        let req = NexusInitializeRequest::new();
        assert_eq!(req.protocol_version, NexusProtocolVersion::latest());
        assert!(req.client_info.is_none());

        // Default trait
        let req2 = NexusInitializeRequest::default();
        assert_eq!(req.protocol_version, req2.protocol_version);
    }

    #[test]
    fn nexus_initialize_request_with_info() {
        let req = NexusInitializeRequest::new().client_info(NexusAgentInfo {
            name: "nexus42".to_string(),
            title: Some("Nexus 42 CLI".to_string()),
            version: "0.1.0".to_string(),
        });
        assert!(req.client_info.is_some());
    }

    #[test]
    fn nexus_new_session_request() {
        let req = NexusNewSessionRequest::new("/tmp/workspace");
        assert_eq!(req.cwd, PathBuf::from("/tmp/workspace"));
        assert!(req.mcp_servers.is_empty());
    }

    #[test]
    fn nexus_prompt_request() {
        let req = NexusPromptRequest::new(
            NexusSessionId::new("sess-1"),
            vec![NexusContentBlock::Text(NexusTextContent {
                text: "Hello".to_string(),
            })],
        );
        assert_eq!(req.session_id.0, "sess-1");
        assert_eq!(req.prompt.len(), 1);
    }

    #[test]
    fn nexus_initialize_response_roundtrip() {
        let resp = NexusInitializeResponse {
            protocol_version: NexusProtocolVersion::v1(),
            agent_capabilities: NexusAgentCapabilities { load_session: true },
            agent_info: Some(NexusAgentInfo {
                name: "claude-code".to_string(),
                title: None,
                version: "1.0.0".to_string(),
            }),
            auth_methods: vec![NexusAuthMethod {
                id: "oauth".to_string(),
                name: "OAuth".to_string(),
                description: None,
            }],
        };

        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: NexusInitializeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, deserialized);
    }

    #[test]
    fn nexus_session_mode_state_roundtrip() {
        let state = NexusSessionModeState {
            current_mode_id: "ask".to_string(),
            available_modes: vec![
                NexusSessionMode {
                    id: "ask".to_string(),
                    name: "Ask".to_string(),
                    description: None,
                },
                NexusSessionMode {
                    id: "act".to_string(),
                    name: "Act".to_string(),
                    description: Some("Auto-execute".to_string()),
                },
            ],
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: NexusSessionModeState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, deserialized);
    }

    #[test]
    fn nexus_mcp_server_roundtrip() {
        let servers = vec![
            NexusMcpServer::Http(NexusMcpServerHttp {
                name: "my-server".to_string(),
                url: "https://example.com/mcp".to_string(),
            }),
            NexusMcpServer::Stdio(NexusMcpServerStdio {
                name: "local-server".to_string(),
                command: PathBuf::from("/usr/bin/mcp-server"),
            }),
        ];

        for server in &servers {
            let json = serde_json::to_string(server).unwrap();
            let deserialized: NexusMcpServer = serde_json::from_str(&json).unwrap();
            assert_eq!(*server, deserialized);
        }
    }
}
