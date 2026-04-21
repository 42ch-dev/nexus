//! ACP Client adapter trait and SDK wrapper.
//!
//! This module defines [`NexusAcpClient`] — the abstraction layer through which
//! all nexus42 CLI code interacts with ACP agents. The concrete implementation
//! ([`AcpSdkAdapter`]) wraps the `agent-client-protocol` SDK's
//! `ConnectionTo<Agent>`, isolating the internal connection model behind
//! the trait boundary.
//!
//! # Design Rationale
//!
//! The adapter pattern (spec §2.2) ensures that:
//! - All SDK usage is encapsulated in one place, making a future migration to
//!   `sacp` (or a hand-rolled JSON-RPC client) straightforward.
//! - The SDK's internal connection model does not leak into the rest of
//!   the nexus42 codebase.
//! - Unit testing can swap the adapter with a mock implementation.
//!
//! # DTO Boundary
//!
//! The `NexusAcpClient` trait uses Nexus-owned DTO types from
//! `nexus_contracts::local::acp` (e.g. `NexusInitializeRequest`,
//! `NexusInitializeResponse`). SDK types (`agent_client_protocol::schema::`) are
//! confined to `AcpSdkAdapter` implementation blocks and `FromSdk` conversion
//! methods. This decoupling allows future SDK migration without changing
//! consumers.
//!
//! # subscribe() Design Decision
//!
//! `subscribe()` returned `async_broadcast::StreamReceiver` which was tightly
//! coupled to the old SDK's broadcast channel implementation. In SDK v0.11.0,
//! the streaming model changed to `ActiveSession::read_update()`. The subscribe
//! mechanism has been removed and will be replaced with a proper DTO-wrapped
//! streaming API in a future task.
//!
//! # SDK v0.11.0 Architecture
//!
//! The ACP SDK v0.11.0 uses a component/channel-based architecture:
//! - `Client` is a zero-sized role struct (no longer a trait).
//! - Connections are created via `Client.builder().connect_with(transport, |cx| {...})`.
//! - The `ConnectionTo<Agent>` handle is Clone + Send, allowing it to be stored
//!   and used from outside the connection callback.
//! - The old `Client` trait (for handling agent requests) is replaced by
//!   `Builder::on_receive_request_from` handlers.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_client_protocol as acp;
use agent_client_protocol::schema::AgentCapabilities;
use agent_client_protocol::schema::AuthMethod;
use agent_client_protocol::schema::SessionModeState;
use agent_client_protocol::schema::{
    ContentBlock, Implementation, InitializeRequest, McpServer, McpServerHttp, McpServerSse,
    McpServerStdio, NewSessionRequest, PromptRequest, ProtocolVersion, ResourceLink, SessionId,
    StopReason, TextContent,
};
use agent_client_protocol::{Agent, ByteStreams, ConnectionTo};
use tokio::sync::RwLock;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::error::AcpResult;
use crate::localset_bridge::LocalSetBridge;
use nexus_contracts::local::acp::{
    NexusAgentCapabilities, NexusAgentInfo, NexusAuthMethod, NexusCancelResult,
    NexusInitializeRequest, NexusInitializeResponse, NexusMcpServer, NexusNewSessionRequest,
    NexusPromptCompleted, NexusPromptRequest, NexusProtocolVersion, NexusSessionCreated,
    NexusSessionId, NexusSessionModeState, NexusStopReason,
};

// ── SDK ↔ Nexus DTO conversion helpers ──────────────────────────────
//
// These are free functions (not trait impls) to avoid orphan rule violations
// since both the SDK types and Nexus DTOs are defined in external crates.

#[allow(dead_code)]
fn nexus_protocol_version_from_sdk(version: &ProtocolVersion) -> NexusProtocolVersion {
    NexusProtocolVersion::new(version.to_string())
}

#[allow(dead_code)]
fn nexus_stop_reason_from_sdk(reason: &StopReason) -> NexusStopReason {
    match reason {
        StopReason::EndTurn => NexusStopReason::EndTurn,
        StopReason::MaxTokens => NexusStopReason::MaxTokens,
        StopReason::MaxTurnRequests => NexusStopReason::MaxTurnRequests,
        StopReason::Refusal => NexusStopReason::Refusal,
        StopReason::Cancelled => NexusStopReason::Cancelled,
        _ => NexusStopReason::EndTurn, // fallback for future variants
    }
}

#[allow(dead_code)]
fn nexus_auth_method_from_sdk(method: &AuthMethod) -> NexusAuthMethod {
    match method {
        AuthMethod::Agent(agent) => NexusAuthMethod {
            id: agent.id.to_string(),
            name: agent.name.clone(),
            description: agent.description.clone(),
        },
        _ => NexusAuthMethod {
            id: "unknown".to_string(),
            name: "unknown".to_string(),
            description: None,
        },
    }
}

#[allow(dead_code)]
fn nexus_agent_info_from_sdk(impl_: &Implementation) -> NexusAgentInfo {
    NexusAgentInfo {
        name: impl_.name.clone(),
        title: impl_.title.clone(),
        version: impl_.version.clone(),
    }
}

#[allow(dead_code)]
fn nexus_agent_capabilities_from_sdk(caps: &AgentCapabilities) -> NexusAgentCapabilities {
    NexusAgentCapabilities {
        load_session: caps.load_session,
    }
}

#[allow(dead_code)]
fn nexus_session_mode_state_from_sdk(state: &SessionModeState) -> NexusSessionModeState {
    NexusSessionModeState {
        current_mode_id: state.current_mode_id.to_string(),
        available_modes: state
            .available_modes
            .iter()
            .map(|m| nexus_contracts::local::acp::NexusSessionMode {
                id: m.id.to_string(),
                name: m.name.clone(),
                description: m.description.clone(),
            })
            .collect(),
    }
}

#[allow(dead_code)]
fn sdk_initialize_request_from_nexus(req: NexusInitializeRequest) -> InitializeRequest {
    let protocol_version = sdk_protocol_version_from_nexus(&req.protocol_version);
    let mut builder = InitializeRequest::new(protocol_version);
    if let Some(info) = req.client_info {
        builder = builder.client_info(Implementation::new(info.name, info.version));
    }
    builder
}

#[allow(dead_code)]
fn sdk_protocol_version_from_nexus(version: &NexusProtocolVersion) -> ProtocolVersion {
    match version.0.parse::<u16>() {
        Ok(v) => serde_json::from_value(serde_json::json!(v)).unwrap_or(ProtocolVersion::LATEST),
        Err(e) => {
            tracing::warn!(
                version = %version.0,
                error = %e,
                "Failed to parse protocol version, defaulting to LATEST"
            );
            ProtocolVersion::LATEST
        }
    }
}

#[allow(dead_code)]
fn sdk_new_session_request_from_nexus(req: NexusNewSessionRequest) -> NewSessionRequest {
    let sdk_servers: Vec<McpServer> = req
        .mcp_servers
        .into_iter()
        .map(nexus_mcp_server_to_sdk)
        .collect();
    NewSessionRequest::new(req.cwd).mcp_servers(sdk_servers)
}

#[allow(dead_code)]
fn nexus_mcp_server_to_sdk(server: NexusMcpServer) -> McpServer {
    match server {
        NexusMcpServer::Http(h) => McpServer::Http(McpServerHttp::new(h.name, h.url)),
        NexusMcpServer::Sse(s) => McpServer::Sse(McpServerSse::new(s.name, s.url)),
        NexusMcpServer::Stdio(s) => McpServer::Stdio(McpServerStdio::new(s.name, s.command)),
    }
}

#[allow(dead_code)]
fn sdk_prompt_request_from_nexus(req: NexusPromptRequest) -> PromptRequest {
    let content_blocks: Vec<ContentBlock> = req
        .prompt
        .into_iter()
        .map(|block| match block {
            nexus_contracts::local::acp::NexusContentBlock::Text(t) => {
                ContentBlock::Text(TextContent::new(t.text))
            }
            nexus_contracts::local::acp::NexusContentBlock::ResourceLink(r) => {
                let builder = ResourceLink::new(r.name.unwrap_or_default(), r.uri);
                ContentBlock::ResourceLink(builder)
            }
        })
        .collect();
    PromptRequest::new(SessionId::new(req.session_id.0), content_blocks)
}

/// The nexus42 ACP client abstraction.
///
/// All ACP communication from the CLI goes through this trait. The methods
/// mirror the ACP protocol lifecycle: initialize → session → prompt → cancel.
///
/// **DTO boundary**: All types in trait signatures are Nexus-owned DTOs from
/// `nexus_contracts::local::acp`. SDK types are confined to `AcpSdkAdapter`.
///
/// **subscribe()**: Removed in SDK v0.11.0 migration. The old `StreamReceiver`
/// type no longer exists. Will be replaced with a proper DTO-wrapped streaming
/// API in a future task.
#[allow(async_fn_in_trait)]
#[allow(dead_code)]
pub trait NexusAcpClient: Send + Sync {
    /// Perform the ACP `initialize` handshake with the agent.
    fn initialize(
        &self,
        request: NexusInitializeRequest,
    ) -> impl Future<Output = AcpResult<NexusInitializeResponse>> + Send;

    /// Create a new ACP session on the agent.
    fn create_session(
        &self,
        request: NexusNewSessionRequest,
    ) -> impl Future<Output = AcpResult<NexusSessionCreated>> + Send;

    /// Send a prompt to the agent within an existing session.
    fn prompt(
        &self,
        request: NexusPromptRequest,
    ) -> impl Future<Output = AcpResult<NexusPromptCompleted>> + Send;

    /// Cancel an in-progress prompt operation.
    fn cancel(
        &self,
        session_id: NexusSessionId,
    ) -> impl Future<Output = AcpResult<NexusCancelResult>> + Send;
}

/// Internal state for the SDK adapter (stored in LocalSet thread).
#[allow(dead_code)]
struct SdkConnection {
    /// The ACP SDK connection handle to the agent.
    connection: ConnectionTo<Agent>,
}

/// Concrete adapter wrapping the `agent-client-protocol` SDK.
///
/// This struct uses the SDK v0.11.0 `Client.builder().connect_with()` pattern
/// to establish a connection to the agent. The `ConnectionTo<Agent>` handle is
/// stored and reused for all subsequent operations.
///
/// The adapter uses [`LocalSetBridge`] to execute operations that may require
/// !Send futures, bridging them to the async tokio world.
#[allow(dead_code)]
pub struct AcpSdkAdapter {
    /// The agent's resolved binary path or command string (for error messages).
    agent_path: PathBuf,
    /// Agent ID for logging context.
    agent_id: String,
    /// LocalSet bridge for executing !Send futures.
    bridge: LocalSetBridge,
    /// The ACP SDK connection (wrapped for thread-safe access).
    /// This is `Some` after `with_connection()` is called.
    connection: Arc<RwLock<Option<SdkConnection>>>,
    /// Handle to the connection setup task (must be joined during cleanup).
    _setup_task: Option<tokio::task::JoinHandle<()>>,
}

impl AcpSdkAdapter {
    /// Create a new adapter without an established connection.
    ///
    /// Use [`with_connection()`] to establish the actual SDK connection.
    #[allow(dead_code)]
    pub fn new(agent_id: String, agent_path: PathBuf) -> Self {
        Self {
            agent_path,
            agent_id: agent_id.clone(),
            bridge: LocalSetBridge::new(),
            connection: Arc::new(RwLock::new(None)),
            _setup_task: None,
        }
    }

    /// Create adapter with established connection.
    ///
    /// This method establishes the ACP SDK connection using the provided
    /// stdin/stdout pipes from the agent subprocess. The connection uses
    /// the SDK's `Client.builder().connect_with()` pattern.
    ///
    /// In SDK v0.11.0, the connection lifecycle is managed by `connect_with`
    /// which provides a `ConnectionTo<Agent>` handle inside the callback.
    /// We store this handle for use by trait methods. The connection is kept
    /// alive by the callback returning a pending future.
    #[allow(dead_code)]
    #[allow(clippy::needless_pass_by_value)]
    pub fn with_connection(
        agent_id: String,
        agent_path: PathBuf,
        stdin: tokio::process::ChildStdin,
        stdout: tokio::process::ChildStdout,
    ) -> Self {
        let bridge = LocalSetBridge::new();
        let connection = Arc::new(RwLock::new(None));

        tracing::info!(
            agent_id = %agent_id,
            "Creating ACP SDK adapter with connection (v0.11.0)"
        );

        let connection_clone = connection.clone();
        let agent_id_for_log = agent_id.clone();
        let agent_id_for_error = agent_id.clone();

        let bridge_clone = bridge.clone();
        let setup_task = tokio::spawn(async move {
            let result = bridge_clone
                .execute(move || {
                    let connection_clone = connection_clone.clone();
                    let agent_id = agent_id_for_log;

                    // Convert tokio pipes to futures-compatible traits inside the LocalSet
                    let stdin_compat = stdin.compat_write();
                    let stdout_compat = stdout.compat();

                    Box::pin(async move {
                        // Create the transport using SDK ByteStreams
                        let transport = ByteStreams::new(stdin_compat, stdout_compat);

                        // Build the Client with a no-op handler (auto-grant all permissions)
                        let builder = acp::Client.builder().name(&agent_id);

                        // Connect with the transport.
                        // The callback receives ConnectionTo<Agent> which we clone into
                        // our shared state. We then await a pending future to keep the
                        // connection alive indefinitely.
                        let connection_for_callback = connection_clone.clone();
                        let agent_id_for_connect = agent_id.clone();
                        let connect_result = builder
                            .connect_with(transport, async move |cx| {
                                // Store the connection handle for use by trait methods
                                let mut guard = connection_for_callback.write().await;
                                *guard = Some(SdkConnection { connection: cx });
                                drop(guard);

                                tracing::info!(
                                    agent_id = %agent_id_for_connect,
                                    "ACP SDK connection established, ConnectionTo<Agent> stored"
                                );

                                // Keep the connection alive by awaiting a pending future
                                std::future::pending::<()>().await;

                                Ok(())
                            })
                            .await;

                        if let Err(e) = connect_result {
                            tracing::error!(
                                agent_id = %agent_id,
                                error = %e,
                                "ACP SDK connection failed"
                            );
                            return Err(crate::AcpError::sdk(e));
                        }

                        Ok::<(), crate::AcpError>(())
                    })
                })
                .await;

            if let Err(e) = result {
                tracing::error!(
                    agent_id = %agent_id_for_error,
                    error = %e,
                    "Failed to establish ACP connection"
                );
            }
        });

        Self {
            agent_path,
            agent_id,
            bridge,
            connection,
            _setup_task: Some(setup_task),
        }
    }

    /// Return a reference to the agent path (for error reporting).
    #[allow(dead_code)]
    pub fn agent_path(&self) -> &Path {
        &self.agent_path
    }

    /// Return the agent ID.
    #[allow(dead_code)]
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
}

impl Drop for AcpSdkAdapter {
    fn drop(&mut self) {
        // Join the setup task if it exists (fire-and-forget cleanup)
        if let Some(setup_task) = self._setup_task.take() {
            // Use tokio::spawn to join in an async context
            // We can't block in Drop, so we spawn a cleanup task
            // Check if a tokio runtime is available to avoid panic during shutdown
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = setup_task.await;
                });
            }
            // If no runtime is available, skip — the process is shutting down anyway
        }
    }
}

impl NexusAcpClient for AcpSdkAdapter {
    fn initialize(
        &self,
        _request: NexusInitializeRequest,
    ) -> impl Future<Output = AcpResult<NexusInitializeResponse>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();
        let agent_id = self.agent_id.clone();

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        let guard = connection.read().await;
                        let _sdk_conn = match guard.as_ref() {
                            Some(conn) => conn,
                            None => {
                                return Err(crate::AcpError::connection_failed(
                                    "Connection not established — SDK v0.11.0 migration in progress",
                                ))
                            }
                        };

                        tracing::warn!(
                            agent_id = %agent_id,
                            "initialize() called — SDK v0.11.0 connection architecture migration pending"
                        );

                        // The full initialize flow will be implemented once the
                        // connection architecture is fully migrated to use ConnectionTo<Agent>.
                        Err(crate::AcpError::connection_failed(
                            "SDK v0.11.0 adapter not fully implemented",
                        ))
                    })
                })
                .await
                .and_then(|r| r)
        }
    }

    fn create_session(
        &self,
        _request: NexusNewSessionRequest,
    ) -> impl Future<Output = AcpResult<NexusSessionCreated>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();
        let agent_id = self.agent_id.clone();

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        let guard = connection.read().await;
                        let _sdk_conn = match guard.as_ref() {
                            Some(conn) => conn,
                            None => {
                                return Err(crate::AcpError::connection_failed(
                                    "Connection not established — SDK v0.11.0 migration in progress",
                                ))
                            }
                        };

                        tracing::warn!(
                            agent_id = %agent_id,
                            "create_session() called — SDK v0.11.0 connection architecture migration pending"
                        );

                        Err(crate::AcpError::connection_failed(
                            "SDK v0.11.0 adapter not fully implemented",
                        ))
                    })
                })
                .await
                .and_then(|r| r)
        }
    }

    fn prompt(
        &self,
        _request: NexusPromptRequest,
    ) -> impl Future<Output = AcpResult<NexusPromptCompleted>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();
        let agent_id = self.agent_id.clone();

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        let guard = connection.read().await;
                        let _sdk_conn = match guard.as_ref() {
                            Some(conn) => conn,
                            None => {
                                return Err(crate::AcpError::connection_failed(
                                    "Connection not established — SDK v0.11.0 migration in progress",
                                ))
                            }
                        };

                        tracing::warn!(
                            agent_id = %agent_id,
                            "prompt() called — SDK v0.11.0 connection architecture migration pending"
                        );

                        Err(crate::AcpError::connection_failed(
                            "SDK v0.11.0 adapter not fully implemented",
                        ))
                    })
                })
                .await
                .and_then(|r| r)
        }
    }

    fn cancel(
        &self,
        _session_id: NexusSessionId,
    ) -> impl Future<Output = AcpResult<NexusCancelResult>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();
        let agent_id = self.agent_id.clone();

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        let guard = connection.read().await;
                        let _sdk_conn = match guard.as_ref() {
                            Some(conn) => conn,
                            None => {
                                return Err(crate::AcpError::connection_failed(
                                    "Connection not established — SDK v0.11.0 migration in progress",
                                ))
                            }
                        };

                        tracing::warn!(
                            agent_id = %agent_id,
                            "cancel() called — SDK v0.11.0 connection architecture migration pending"
                        );

                        Err(crate::AcpError::connection_failed(
                            "SDK v0.11.0 adapter not fully implemented",
                        ))
                    })
                })
                .await
                .and_then(|r| r)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_from_sdk() {
        let sdk_version = ProtocolVersion::LATEST;
        let nexus_version = nexus_protocol_version_from_sdk(&sdk_version);
        assert_eq!(nexus_version.0, "1");
    }

    #[test]
    fn stop_reason_from_sdk() {
        use agent_client_protocol::schema::StopReason;
        assert_eq!(
            nexus_stop_reason_from_sdk(&StopReason::EndTurn),
            NexusStopReason::EndTurn
        );
        assert_eq!(
            nexus_stop_reason_from_sdk(&StopReason::Cancelled),
            NexusStopReason::Cancelled
        );
    }

    #[test]
    fn agent_info_from_sdk() {
        let sdk_impl = Implementation::new("claude-code", "1.0.0");
        let nexus_info = nexus_agent_info_from_sdk(&sdk_impl);
        assert_eq!(nexus_info.name, "claude-code");
        assert_eq!(nexus_info.version, "1.0.0");
    }

    #[test]
    fn initialize_request_to_sdk() {
        let nexus_req = NexusInitializeRequest::new();
        let _sdk_req = sdk_initialize_request_from_nexus(nexus_req);
        // Just verify conversion succeeds
    }

    #[test]
    fn new_session_request_propagates_mcp_servers() {
        let nexus_req = NexusNewSessionRequest::new("/tmp/workspace").mcp_servers(vec![
            NexusMcpServer::Http(nexus_contracts::local::acp::NexusMcpServerHttp {
                name: "http-server".to_string(),
                url: "https://example.com/mcp".to_string(),
            }),
            NexusMcpServer::Sse(nexus_contracts::local::acp::NexusMcpServerSse {
                name: "sse-server".to_string(),
                url: "https://example.com/sse".to_string(),
            }),
            NexusMcpServer::Stdio(nexus_contracts::local::acp::NexusMcpServerStdio {
                name: "local-server".to_string(),
                command: std::path::PathBuf::from("/usr/bin/mcp-server"),
            }),
        ]);

        let sdk_req = sdk_new_session_request_from_nexus(nexus_req);
        assert_eq!(sdk_req.mcp_servers.len(), 3);

        // Verify HTTP server
        match &sdk_req.mcp_servers[0] {
            McpServer::Http(h) => {
                assert_eq!(h.name, "http-server");
                assert_eq!(h.url, "https://example.com/mcp");
                assert!(h.headers.is_empty());
            }
            _ => panic!("Expected Http variant"),
        }

        // Verify SSE server
        match &sdk_req.mcp_servers[1] {
            McpServer::Sse(s) => {
                assert_eq!(s.name, "sse-server");
                assert_eq!(s.url, "https://example.com/sse");
            }
            _ => panic!("Expected Sse variant"),
        }

        // Verify Stdio server
        match &sdk_req.mcp_servers[2] {
            McpServer::Stdio(s) => {
                assert_eq!(s.name, "local-server");
                assert_eq!(s.command, std::path::PathBuf::from("/usr/bin/mcp-server"));
            }
            _ => panic!("Expected Stdio variant"),
        }
    }

    #[test]
    fn new_session_request_empty_mcp_servers() {
        let nexus_req = NexusNewSessionRequest::new("/tmp/workspace");
        let sdk_req = sdk_new_session_request_from_nexus(nexus_req);
        assert!(sdk_req.mcp_servers.is_empty());
    }

    #[test]
    fn protocol_version_valid_string() {
        let version = NexusProtocolVersion::new("1");
        let sdk_version = sdk_protocol_version_from_nexus(&version);
        assert_eq!(sdk_version.to_string(), "1");
    }

    #[test]
    fn protocol_version_invalid_string_defaults_to_latest() {
        let version = NexusProtocolVersion::new("not-a-number");
        let sdk_version = sdk_protocol_version_from_nexus(&version);
        assert_eq!(sdk_version, ProtocolVersion::LATEST);
    }

    #[test]
    fn protocol_version_empty_string_defaults_to_latest() {
        let version = NexusProtocolVersion::new("");
        let sdk_version = sdk_protocol_version_from_nexus(&version);
        assert_eq!(sdk_version, ProtocolVersion::LATEST);
    }

    #[tokio::test]
    async fn adapter_new_creates_bridge() {
        let adapter = AcpSdkAdapter::new(
            "test-agent".to_string(),
            PathBuf::from("/usr/bin/test-agent"),
        );

        assert_eq!(adapter.agent_id(), "test-agent");
        assert_eq!(adapter.agent_path(), Path::new("/usr/bin/test-agent"));

        // Connection should be None
        let guard = adapter.connection.read().await;
        assert!(guard.is_none());
    }

    #[tokio::test]
    async fn adapter_initialize_without_connection_fails() {
        let adapter = AcpSdkAdapter::new(
            "test-agent".to_string(),
            PathBuf::from("/usr/bin/test-agent"),
        );

        let request = NexusInitializeRequest::new();

        let result = adapter.initialize(request).await;
        assert!(result.is_err());
    }
}
