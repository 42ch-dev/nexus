//! Nexus-owned ACP DTO type definitions.
//!
//! Each type has `From<acp_sdk_type>` conversions in the same module
//! (or in `nexus-acp-host/src/client.rs` for types that depend on the SDK crate).
//!
//! # Naming convention
//!
//! Types in this module intentionally mirror ACP SDK types but use a
//! `Nexus`-prefixed name (e.g. `NexusAgentInfo` mirrors SDK `Implementation`,
//! `NexusStopReason` mirrors SDK `StopReason`). The prefix keeps the wire DTO
//! namespace distinct from the SDK types consumed by `nexus-acp-host` and makes
//! the conversion boundary explicit in caller code. When adding a new ACP
//! mirror type, preserve the `Nexus` prefix and add a doc comment naming the
//! SDK type it mirrors.
// why: keep the wire DTO namespace distinct from the SDK types `nexus-acp-host` consumes.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// note: Nexus-owned session identifier.
///
/// Wraps an opaque string. The inner value is opaque to consumers — it is
/// only produced by the SDK adapter and passed back to it for protocol
/// operations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NexusSessionId(pub String);

impl NexusSessionId {
    /// note: create a new session ID from a string.
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

/// note: Nexus-owned protocol version identifier.
///
/// Wraps the negotiated protocol version as a string for flexibility
/// across SDK versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NexusProtocolVersion(pub String);

impl NexusProtocolVersion {
    /// note: create a new protocol version from a string.
    #[must_use]
    pub fn new(version: impl Into<String>) -> Self {
        Self(version.into())
    }

    /// note: protocol version "1" (current ACP spec).
    #[must_use]
    pub fn v1() -> Self {
        Self(String::from("1"))
    }

    /// note: the latest supported protocol version.
    #[must_use]
    pub fn latest() -> Self {
        Self::v1()
    }
}

impl std::fmt::Display for NexusProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// note: why the agent stopped processing a prompt turn.
///
/// Mirrors the ACP spec `StopReason` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NexusStopReason {
    /// note: the turn ended successfully.
    EndTurn,
    /// note: the agent reached the maximum token count.
    MaxTokens,
    /// note: the agent reached the maximum number of allowed requests.
    MaxTurnRequests,
    /// note: the agent refused to continue.
    Refusal,
    /// note: the turn was cancelled by the client.
    Cancelled,
}

/// note: an authentication method reported by the agent during initialization.
///
/// This is a simplified view — only the fields consumers actually inspect
/// (id, name) are exposed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusAuthMethod {
    /// note: unique identifier for this authentication method.
    pub id: String,
    /// note: human-readable name.
    pub name: String,
    /// note: optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// note: metadata about the agent implementation (name, version).
///
/// Mirrors SDK `Implementation` — consumers read `name` and `version`
/// for display and logging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusAgentInfo {
    /// note: programmatic identifier (e.g. "claude-code").
    pub name: String,
    /// note: optional human-readable title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// note: version string (e.g. "1.0.0").
    pub version: String,
}

/// note: capabilities reported by the agent during initialization.
///
/// This is a simplified view exposing only the fields Nexus consumers use.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusAgentCapabilities {
    /// note: whether the agent supports `session/load`.
    #[serde(default)]
    pub load_session: bool,
}

/// note: a mode the agent can operate in (e.g. "act" / "ask").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusSessionMode {
    /// note: mode identifier.
    pub id: String,
    /// note: human-readable name.
    pub name: String,
    /// note: optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// note: the agent's current session mode state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusSessionModeState {
    /// note: the current mode the agent is in.
    pub current_mode_id: String,
    /// note: the set of modes the agent can operate in.
    pub available_modes: Vec<NexusSessionMode>,
}

/// note: a content block in a prompt or response.
///
/// This is a simplified view — only `Text` and `ResourceLink` variants
/// are needed by Nexus consumers today. New variants can be added as
/// needed without breaking the SDK boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NexusContentBlock {
    /// note: text content.
    Text(NexusTextContent),
    /// note: reference to a resource.
    ResourceLink(NexusResourceLink),
}

/// note: text content within a content block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NexusTextContent {
    /// note: the text content.
    pub text: String,
}

/// note: a reference to a resource within a content block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusResourceLink {
    /// note: the resource URI.
    pub uri: String,
    /// note: optional resource name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// note: an MCP server configuration for a new session.
///
/// Mirrors the SDK `McpServer` enum — consumers construct these for
/// `NexusNewSessionRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NexusMcpServer {
    /// note: HTTP-based MCP server.
    #[serde(rename = "http")]
    Http(NexusMcpServerHttp),
    /// note: SSE-based MCP server.
    #[serde(rename = "sse")]
    Sse(NexusMcpServerSse),
    /// note: stdio-based MCP server.
    #[serde(rename = "stdio")]
    Stdio(NexusMcpServerStdio),
}

/// note: HTTP MCP server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusMcpServerHttp {
    /// note: server name.
    pub name: String,
    /// note: server URL.
    pub url: String,
}

/// note: SSE MCP server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusMcpServerSse {
    /// note: server name.
    pub name: String,
    /// note: server URL.
    pub url: String,
}

/// note: stdio MCP server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusMcpServerStdio {
    /// note: server name.
    pub name: String,
    /// note: command to execute.
    pub command: PathBuf,
    /// note: arguments passed to the command (e.g. `["mcp", "serve"]`).
    #[serde(default)]
    pub args: Vec<String>,
}

/// note: request for the ACP `initialize` handshake.
///
/// This replaces the SDK `InitializeRequest` in trait signatures.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusInitializeRequest {
    /// note: protocol version the client supports.
    pub protocol_version: NexusProtocolVersion,
    /// note: client capabilities (simplified view).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_info: Option<NexusAgentInfo>,
}

impl NexusInitializeRequest {
    /// note: create a new initialize request with the latest protocol version.
    #[must_use]
    pub fn new() -> Self {
        Self {
            protocol_version: NexusProtocolVersion::latest(),
            client_info: None,
        }
    }

    /// note: set client info.
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

/// note: request to create a new ACP session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusNewSessionRequest {
    /// note: working directory for the session.
    pub cwd: PathBuf,
    /// note: MCP servers to connect to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<NexusMcpServer>,
}

impl NexusNewSessionRequest {
    /// note: create a new session request with the given working directory.
    #[must_use]
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            mcp_servers: vec![],
        }
    }

    /// note: add MCP servers.
    #[must_use]
    pub fn mcp_servers(mut self, servers: Vec<NexusMcpServer>) -> Self {
        self.mcp_servers = servers;
        self
    }
}

/// note: request to send a prompt to the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusPromptRequest {
    /// note: session ID to send the prompt to.
    pub session_id: NexusSessionId,
    /// note: content blocks composing the user's message.
    pub prompt: Vec<NexusContentBlock>,
}

impl NexusPromptRequest {
    /// note: create a new prompt request.
    #[must_use]
    pub const fn new(session_id: NexusSessionId, prompt: Vec<NexusContentBlock>) -> Self {
        Self { session_id, prompt }
    }
}

/// note: response from the ACP `initialize` handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusInitializeResponse {
    /// note: negotiated protocol version.
    pub protocol_version: NexusProtocolVersion,
    /// note: capabilities reported by the agent.
    pub agent_capabilities: NexusAgentCapabilities,
    /// note: agent metadata (name, version).
    pub agent_info: Option<NexusAgentInfo>,
    /// note: authentication methods supported by the agent.
    pub auth_methods: Vec<NexusAuthMethod>,
}

/// note: response from creating a new ACP session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusSessionCreated {
    /// note: unique session identifier.
    pub session_id: NexusSessionId,
    /// note: session mode state (if the agent reports modes).
    pub modes: Option<NexusSessionModeState>,
    /// note: configuration options exposed by the agent at session creation.
    ///
    /// Used for dynamic capability discovery (e.g. model switching via
    /// `set_config_option`). May be `None` if the agent does not report
    /// config options or the response did not include them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_options: Option<Vec<NexusConfigOption>>,
}

/// note: response from sending a prompt to the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusPromptCompleted {
    /// note: why the agent stopped processing the turn.
    pub stop_reason: NexusStopReason,
}

/// note: result of a cancel operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusCancelResult {
    /// note: the session that was cancelled.
    pub session_id: NexusSessionId,
}

/// note: request to set a session configuration option.
///
/// Mirrors the SDK `SetSessionConfigOptionRequest` — identifies the session,
/// the config key, and the new value (a string newtype for value IDs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusSetConfigOptionRequest {
    /// note: the session to update.
    pub session_id: NexusSessionId,
    /// note: the configuration option to set.
    pub config_id: String,
    /// note: the value to set (a config value ID string).
    pub value: String,
}

impl NexusSetConfigOptionRequest {
    /// note: create a new set-config-option request.
    #[must_use]
    pub fn new(
        session_id: NexusSessionId,
        config_id: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            config_id: config_id.into(),
            value: value.into(),
        }
    }
}

/// note: category of a session configuration option (UX hint).
///
/// Mirrors the SDK `SessionConfigOptionCategory` enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NexusConfigOptionCategory {
    /// note: session mode selector.
    Mode,
    /// note: model selector.
    Model,
    /// note: thought/reasoning level selector.
    ThoughtLevel,
    /// note: unknown / uncategorized.
    Other(String),
}

/// note: a possible value in a select configuration option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusConfigSelectOption {
    /// note: unique identifier for this option value.
    pub value: String,
    /// note: human-readable label.
    pub name: String,
    /// note: optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// note: a group of selectable options in a configuration option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusConfigSelectGroup {
    /// note: unique identifier for this group.
    pub group: String,
    /// note: human-readable label for this group.
    pub name: String,
    /// note: the set of option values in this group.
    pub options: Vec<NexusConfigSelectOption>,
}

/// note: options layout for a select configuration option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NexusConfigSelectOptions {
    /// note: a flat list of options with no grouping.
    Ungrouped(Vec<NexusConfigSelectOption>),
    /// note: a list of options grouped under headers.
    Grouped(Vec<NexusConfigSelectGroup>),
}

/// note: the payload for a select-type configuration option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusConfigSelect {
    /// note: the currently selected value.
    pub current_value: String,
    /// note: the selectable options.
    pub options: NexusConfigSelectOptions,
}

/// note: type-specific payload for a session configuration option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NexusConfigKind {
    /// note: single-value selector (dropdown).
    Select(NexusConfigSelect),
}

/// note: a session configuration option returned by the agent.
///
/// Mirrors the SDK `SessionConfigOption` struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusConfigOption {
    /// note: unique identifier for the configuration option.
    pub id: String,
    /// note: human-readable label.
    pub name: String,
    /// note: optional description for display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// note: optional semantic category for the option (UX hint).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<NexusConfigOptionCategory>,
    /// note: type-specific payload.
    #[serde(flatten)]
    pub kind: NexusConfigKind,
}

/// note: response from setting a session configuration option.
///
/// Mirrors the SDK `SetSessionConfigOptionResponse` — returns the full set of
/// configuration options and their current values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusSetConfigOptionResponse {
    /// note: the full set of configuration options with their current values.
    pub config_options: Vec<NexusConfigOption>,
}

impl NexusSetConfigOptionResponse {
    /// note: create a new set-config-option response.
    #[must_use]
    pub const fn new(config_options: Vec<NexusConfigOption>) -> Self {
        Self { config_options }
    }
}

/// note: request to list sessions from the agent.
///
/// Mirrors the SDK `ListSessionsRequest` — supports filtering by cwd
/// and cursor-based pagination.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusListSessionsRequest {
    /// note: filter sessions by working directory. Must be an absolute path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    /// note: opaque cursor token from a previous response's nextCursor field
    /// for cursor-based pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl NexusListSessionsRequest {
    /// note: create a new list sessions request with no filters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// note: filter sessions by working directory.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// note: set the pagination cursor.
    #[must_use]
    pub fn cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }
}

/// note: information about a session returned by session/list.
///
/// Mirrors the SDK `SessionInfo` type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusSessionInfo {
    /// note: unique identifier for the session.
    pub session_id: NexusSessionId,
    /// note: the working directory for this session. Must be an absolute path.
    pub cwd: PathBuf,
    /// note: human-readable title for the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// note: ISO 8601 timestamp of last activity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl NexusSessionInfo {
    /// note: create a new session info.
    #[must_use]
    pub fn new(session_id: NexusSessionId, cwd: impl Into<PathBuf>) -> Self {
        Self {
            session_id,
            cwd: cwd.into(),
            title: None,
            updated_at: None,
        }
    }

    /// note: set the session title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// note: set the session title from an Option.
    #[must_use]
    pub fn title_opt(mut self, title: Option<String>) -> Self {
        self.title = title;
        self
    }

    /// note: set the last activity timestamp.
    #[must_use]
    pub fn updated_at(mut self, updated_at: impl Into<String>) -> Self {
        self.updated_at = Some(updated_at.into());
        self
    }

    /// note: set the last activity timestamp from an Option.
    #[must_use]
    pub fn updated_at_opt(mut self, updated_at: Option<String>) -> Self {
        self.updated_at = updated_at;
        self
    }
}

/// note: response from listing sessions.
///
/// Mirrors the SDK `ListSessionsResponse` — contains session info
/// objects and optional pagination cursor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusListSessionsResponse {
    /// note: array of session information objects.
    pub sessions: Vec<NexusSessionInfo>,
    /// note: opaque cursor token. If present, pass this in the next request's
    /// cursor parameter to fetch the next page. If absent, there are no
    /// more results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl NexusListSessionsResponse {
    /// note: create a new list sessions response.
    #[must_use]
    pub const fn new(sessions: Vec<NexusSessionInfo>) -> Self {
        Self {
            sessions,
            next_cursor: None,
        }
    }

    /// note: set the pagination cursor.
    #[must_use]
    pub fn next_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.next_cursor = Some(cursor.into());
        self
    }

    /// note: set the pagination cursor from an Option.
    #[must_use]
    pub fn next_cursor_opt(mut self, cursor: Option<String>) -> Self {
        self.next_cursor = cursor;
        self
    }
}

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
        assert_eq!(req.mcp_servers, Vec::new());
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
                args: Vec::new(),
            }),
        ];

        for server in &servers {
            let json = serde_json::to_string(server).unwrap();
            let deserialized: NexusMcpServer = serde_json::from_str(&json).unwrap();
            assert_eq!(*server, deserialized);
        }
    }

    #[test]
    fn nexus_set_config_option_request_new() {
        let req = NexusSetConfigOptionRequest::new(
            NexusSessionId::new("sess-1"),
            "model",
            "claude-3-opus",
        );
        assert_eq!(req.session_id.0, "sess-1");
        assert_eq!(req.config_id, "model");
        assert_eq!(req.value, "claude-3-opus");
    }

    #[test]
    fn nexus_set_config_option_request_roundtrip() {
        let req = NexusSetConfigOptionRequest::new(
            NexusSessionId::new("sess-42"),
            "thought_level",
            "high",
        );
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: NexusSetConfigOptionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.session_id, deserialized.session_id);
        assert_eq!(req.config_id, deserialized.config_id);
        assert_eq!(req.value, deserialized.value);
    }

    #[test]
    fn nexus_set_config_option_request_camel_case() {
        let req = NexusSetConfigOptionRequest::new(NexusSessionId::new("s1"), "cfg_id", "val");
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("sessionId"));
        assert!(json.contains("configId"));
    }

    #[test]
    fn nexus_config_option_category_roundtrip() {
        for cat in [
            NexusConfigOptionCategory::Mode,
            NexusConfigOptionCategory::Model,
            NexusConfigOptionCategory::ThoughtLevel,
            NexusConfigOptionCategory::Other("custom".to_string()),
        ] {
            let json = serde_json::to_string(&cat).unwrap();
            let deserialized: NexusConfigOptionCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(cat, deserialized);
        }
    }

    #[test]
    fn nexus_config_select_option_roundtrip() {
        let opt = NexusConfigSelectOption {
            value: "opt-1".to_string(),
            name: "Option One".to_string(),
            description: Some("First option".to_string()),
        };
        let json = serde_json::to_string(&opt).unwrap();
        let deserialized: NexusConfigSelectOption = serde_json::from_str(&json).unwrap();
        assert_eq!(opt, deserialized);

        let opt_no_desc = NexusConfigSelectOption {
            value: "opt-2".to_string(),
            name: "Two".to_string(),
            description: None,
        };
        let json2 = serde_json::to_string(&opt_no_desc).unwrap();
        assert!(!json2.contains("description"));
    }

    #[test]
    fn nexus_config_select_options_roundtrip() {
        let ungrouped = NexusConfigSelectOptions::Ungrouped(vec![NexusConfigSelectOption {
            value: "a".to_string(),
            name: "A".to_string(),
            description: None,
        }]);
        let json = serde_json::to_string(&ungrouped).unwrap();
        let d: NexusConfigSelectOptions = serde_json::from_str(&json).unwrap();
        assert_eq!(ungrouped, d);

        let grouped = NexusConfigSelectOptions::Grouped(vec![NexusConfigSelectGroup {
            group: "g1".to_string(),
            name: "Group 1".to_string(),
            options: vec![],
        }]);
        let json2 = serde_json::to_string(&grouped).unwrap();
        let d2: NexusConfigSelectOptions = serde_json::from_str(&json2).unwrap();
        assert_eq!(grouped, d2);
    }

    #[test]
    fn nexus_config_kind_select_roundtrip() {
        let kind = NexusConfigKind::Select(NexusConfigSelect {
            current_value: "claude-3-opus".to_string(),
            options: NexusConfigSelectOptions::Ungrouped(vec![NexusConfigSelectOption {
                value: "claude-3-opus".to_string(),
                name: "Claude 3 Opus".to_string(),
                description: None,
            }]),
        });
        let json = serde_json::to_string(&kind).unwrap();
        let d: NexusConfigKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, d);
    }

    #[test]
    fn nexus_config_option_full_roundtrip() {
        let opt = NexusConfigOption {
            id: "model".to_string(),
            name: "Model".to_string(),
            description: Some("Select the AI model".to_string()),
            category: Some(NexusConfigOptionCategory::Model),
            kind: NexusConfigKind::Select(NexusConfigSelect {
                current_value: "claude-3-opus".to_string(),
                options: NexusConfigSelectOptions::Ungrouped(vec![
                    NexusConfigSelectOption {
                        value: "claude-3-opus".to_string(),
                        name: "Claude 3 Opus".to_string(),
                        description: None,
                    },
                    NexusConfigSelectOption {
                        value: "claude-3-sonnet".to_string(),
                        name: "Claude 3 Sonnet".to_string(),
                        description: Some("Faster model".to_string()),
                    },
                ]),
            }),
        };
        let json = serde_json::to_string(&opt).unwrap();
        let deserialized: NexusConfigOption = serde_json::from_str(&json).unwrap();
        assert_eq!(opt, deserialized);
    }

    #[test]
    fn nexus_set_config_option_response_new() {
        let resp = NexusSetConfigOptionResponse::new(vec![NexusConfigOption {
            id: "model".to_string(),
            name: "Model".to_string(),
            description: None,
            category: None,
            kind: NexusConfigKind::Select(NexusConfigSelect {
                current_value: "claude-3-opus".to_string(),
                options: NexusConfigSelectOptions::Ungrouped(vec![]),
            }),
        }]);
        assert_eq!(resp.config_options.len(), 1);
    }

    #[test]
    fn nexus_set_config_option_response_roundtrip() {
        let resp = NexusSetConfigOptionResponse::new(vec![NexusConfigOption {
            id: "mode".to_string(),
            name: "Mode".to_string(),
            description: None,
            category: Some(NexusConfigOptionCategory::Mode),
            kind: NexusConfigKind::Select(NexusConfigSelect {
                current_value: "act".to_string(),
                options: NexusConfigSelectOptions::Ungrouped(vec![NexusConfigSelectOption {
                    value: "act".to_string(),
                    name: "Act".to_string(),
                    description: None,
                }]),
            }),
        }]);
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: NexusSetConfigOptionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, deserialized);
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
        let resp = NexusListSessionsResponse::new(sessions);
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
