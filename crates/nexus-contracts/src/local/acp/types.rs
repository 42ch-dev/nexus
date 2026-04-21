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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

// ── Session list ───────────────────────────────────────────────────────

/// Request to list sessions from the agent.
///
/// Mirrors the SDK `ListSessionsRequest` — supports filtering by cwd
/// and cursor-based pagination.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusListSessionsRequest {
    /// Filter sessions by working directory. Must be an absolute path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    /// Opaque cursor token from a previous response's nextCursor field
    /// for cursor-based pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl NexusListSessionsRequest {
    /// Create a new list sessions request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter sessions by working directory.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Set the pagination cursor.
    #[must_use]
    pub fn cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }
}

/// Information about a session returned by session/list.
///
/// Mirrors the SDK `SessionInfo` type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusSessionInfo {
    /// Unique identifier for the session.
    pub session_id: NexusSessionId,
    /// The working directory for this session. Must be an absolute path.
    pub cwd: PathBuf,
    /// Human-readable title for the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// ISO 8601 timestamp of last activity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl NexusSessionInfo {
    /// Create a new session info.
    #[must_use]
    pub fn new(session_id: NexusSessionId, cwd: impl Into<PathBuf>) -> Self {
        Self {
            session_id,
            cwd: cwd.into(),
            title: None,
            updated_at: None,
        }
    }

    /// Set the session title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the session title from an Option.
    #[must_use]
    pub fn title_opt(mut self, title: Option<String>) -> Self {
        self.title = title;
        self
    }

    /// Set the last activity timestamp.
    #[must_use]
    pub fn updated_at(mut self, updated_at: impl Into<String>) -> Self {
        self.updated_at = Some(updated_at.into());
        self
    }

    /// Set the last activity timestamp from an Option.
    #[must_use]
    pub fn updated_at_opt(mut self, updated_at: Option<String>) -> Self {
        self.updated_at = updated_at;
        self
    }
}

/// Response from listing sessions.
///
/// Mirrors the SDK `ListSessionsResponse` — contains session info
/// objects and optional pagination cursor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusListSessionsResponse {
    /// Array of session information objects.
    pub sessions: Vec<NexusSessionInfo>,
    /// Opaque cursor token. If present, pass this in the next request's
    /// cursor parameter to fetch the next page. If absent, there are no
    /// more results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl NexusListSessionsResponse {
    /// Create a new list sessions response.
    #[must_use]
    pub fn new(sessions: Vec<NexusSessionInfo>) -> Self {
        Self {
            sessions,
            next_cursor: None,
        }
    }

    /// Set the pagination cursor.
    #[must_use]
    pub fn next_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.next_cursor = Some(cursor.into());
        self
    }

    /// Set the pagination cursor from an Option.
    #[must_use]
    pub fn next_cursor_opt(mut self, cursor: Option<String>) -> Self {
        self.next_cursor = cursor;
        self
    }
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

    #[test]
    fn nexus_list_sessions_request_default() {
        let req = NexusListSessionsRequest::new();
        assert!(req.cwd.is_none());
        assert!(req.cursor.is_none());

        let req2 = NexusListSessionsRequest::default();
        assert_eq!(req.cwd, req2.cwd);
        assert_eq!(req.cursor, req2.cursor);
    }

    #[test]
    fn nexus_list_sessions_request_with_filters() {
        let req = NexusListSessionsRequest::new()
            .cwd("/tmp/workspace")
            .cursor("next-page-token");
        assert_eq!(req.cwd, Some(PathBuf::from("/tmp/workspace")));
        assert_eq!(req.cursor, Some("next-page-token".to_string()));
    }

    #[test]
    fn nexus_list_sessions_request_roundtrip() {
        let req = NexusListSessionsRequest::new()
            .cwd("/home/user/project")
            .cursor("abc123");

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: NexusListSessionsRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.cwd, deserialized.cwd);
        assert_eq!(req.cursor, deserialized.cursor);
    }

    #[test]
    fn nexus_list_sessions_request_skip_serializing_none() {
        let req = NexusListSessionsRequest::new();
        let json = serde_json::to_string(&req).unwrap();
        // Should not contain cwd or cursor fields when None
        assert!(!json.contains("cwd"));
        assert!(!json.contains("cursor"));
    }

    #[test]
    fn nexus_session_info_new() {
        let info = NexusSessionInfo::new(NexusSessionId::new("sess-1"), "/tmp/workspace");
        assert_eq!(info.session_id.0, "sess-1");
        assert_eq!(info.cwd, PathBuf::from("/tmp/workspace"));
        assert!(info.title.is_none());
        assert!(info.updated_at.is_none());
    }

    #[test]
    fn nexus_session_info_with_optional_fields() {
        let info = NexusSessionInfo::new(NexusSessionId::new("sess-2"), "/home/user/project")
            .title("My Project Session")
            .updated_at("2026-04-21T10:30:00Z");
        assert_eq!(info.title, Some("My Project Session".to_string()));
        assert_eq!(info.updated_at, Some("2026-04-21T10:30:00Z".to_string()));
    }

    #[test]
    fn nexus_session_info_roundtrip() {
        let info = NexusSessionInfo::new(NexusSessionId::new("sess-3"), "/var/app")
            .title("Production")
            .updated_at("2026-04-21T12:00:00Z");

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: NexusSessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info.session_id, deserialized.session_id);
        assert_eq!(info.cwd, deserialized.cwd);
        assert_eq!(info.title, deserialized.title);
        assert_eq!(info.updated_at, deserialized.updated_at);
    }

    #[test]
    fn nexus_list_sessions_response_new() {
        let sessions = vec![
            NexusSessionInfo::new(NexusSessionId::new("sess-1"), "/tmp/a"),
            NexusSessionInfo::new(NexusSessionId::new("sess-2"), "/tmp/b"),
        ];
        let resp = NexusListSessionsResponse::new(sessions.clone());
        assert_eq!(resp.sessions.len(), 2);
        assert_eq!(resp.sessions[0].session_id.0, "sess-1");
        assert!(resp.next_cursor.is_none());
    }

    #[test]
    fn nexus_list_sessions_response_with_cursor() {
        let sessions = vec![NexusSessionInfo::new(
            NexusSessionId::new("sess-x"),
            "/tmp/x",
        )];
        let resp = NexusListSessionsResponse::new(sessions).next_cursor("next-token");
        assert_eq!(resp.next_cursor, Some("next-token".to_string()));
    }

    #[test]
    fn nexus_list_sessions_response_roundtrip() {
        let resp = NexusListSessionsResponse::new(vec![
            NexusSessionInfo::new(NexusSessionId::new("sess-1"), "/tmp/a")
                .title("Session A")
                .updated_at("2026-04-21T09:00:00Z"),
            NexusSessionInfo::new(NexusSessionId::new("sess-2"), "/tmp/b"),
        ])
        .next_cursor("page2");

        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: NexusListSessionsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp.sessions.len(), deserialized.sessions.len());
        assert_eq!(
            resp.sessions[0].session_id,
            deserialized.sessions[0].session_id
        );
        assert_eq!(resp.next_cursor, deserialized.next_cursor);
    }
}
