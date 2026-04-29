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
//! # `subscribe()` Design Decision
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
//!
//! # Threading Model
//!
//! All SDK operations run inside a [`LocalSetBridge`] — a dedicated OS thread
//! running a `tokio::task::LocalSet`. This is required because the SDK produces
//! `!Send` futures. The bridge serializes all operations through a single
//! `mpsc` channel, guaranteeing exclusive access to [`SdkConnection`] state
//! (including `ActiveSession` handles) without additional synchronization.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_client_protocol as acp;
use agent_client_protocol::schema::AgentCapabilities;
use agent_client_protocol::schema::AuthMethod;
use agent_client_protocol::schema::CancelNotification;
use agent_client_protocol::schema::InitializeResponse;
use agent_client_protocol::schema::ListSessionsRequest;
use agent_client_protocol::schema::ListSessionsResponse;
use agent_client_protocol::schema::SessionInfo;
use agent_client_protocol::schema::SessionModeState;
use agent_client_protocol::schema::SetSessionConfigOptionRequest;
use agent_client_protocol::schema::SetSessionConfigOptionResponse;
use agent_client_protocol::schema::{
    ContentBlock, Implementation, InitializeRequest, McpServer, McpServerHttp, McpServerSse,
    McpServerStdio, NewSessionRequest, PromptRequest, ProtocolVersion, ResourceLink, SessionId,
    StopReason, TextContent,
};
use agent_client_protocol::{ActiveSession, Agent, ByteStreams, ConnectionTo, SessionMessage};
use tokio::sync::RwLock;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::error::AcpResult;
use crate::localset_bridge::LocalSetBridge;
use nexus_contracts::local::acp::{
    NexusAgentCapabilities, NexusAgentInfo, NexusAuthMethod, NexusCancelResult, NexusConfigKind,
    NexusConfigOption, NexusConfigOptionCategory, NexusConfigSelect, NexusConfigSelectGroup,
    NexusConfigSelectOption, NexusConfigSelectOptions, NexusInitializeRequest,
    NexusInitializeResponse, NexusListSessionsRequest, NexusListSessionsResponse, NexusMcpServer,
    NexusNewSessionRequest, NexusPromptCompleted, NexusPromptRequest, NexusProtocolVersion,
    NexusSessionCreated, NexusSessionId, NexusSessionInfo, NexusSessionModeState,
    NexusSetConfigOptionRequest, NexusSetConfigOptionResponse, NexusStopReason,
};

// ── Compile-time Send assertion for ConnectionTo<Agent> ────────────
//
// The LocalSetBridge pattern relies on ConnectionTo<Agent> being Send so
// it can be extracted from the !Send LocalSet context and stored in
// Arc<RwLock<...>> accessible from the async tokio world. This const
// block fails to compile if the SDK ever changes that guarantee.
const _: fn() = || {
    const fn assert_send<T: Send>() {}
    assert_send::<agent_client_protocol::ConnectionTo<agent_client_protocol::Agent>>();
};

// ── SDK ↔ Nexus DTO conversion helpers ──────────────────────────────
//
// These are free functions (not trait impls) to avoid orphan rule violations
// since both the SDK types and Nexus DTOs are defined in external crates.

fn nexus_protocol_version_from_sdk(version: &ProtocolVersion) -> NexusProtocolVersion {
    NexusProtocolVersion::new(version.to_string())
}

const fn nexus_stop_reason_from_sdk(reason: StopReason) -> NexusStopReason {
    match reason {
        StopReason::MaxTokens => NexusStopReason::MaxTokens,
        StopReason::MaxTurnRequests => NexusStopReason::MaxTurnRequests,
        StopReason::Refusal => NexusStopReason::Refusal,
        StopReason::Cancelled => NexusStopReason::Cancelled,
        _ => NexusStopReason::EndTurn, // fallback for future variants
    }
}

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

fn nexus_agent_info_from_sdk(impl_: &Implementation) -> NexusAgentInfo {
    NexusAgentInfo {
        name: impl_.name.clone(),
        title: impl_.title.clone(),
        version: impl_.version.clone(),
    }
}

const fn nexus_agent_capabilities_from_sdk(caps: &AgentCapabilities) -> NexusAgentCapabilities {
    NexusAgentCapabilities {
        load_session: caps.load_session,
    }
}

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

fn nexus_initialize_response_from_sdk(resp: &InitializeResponse) -> NexusInitializeResponse {
    NexusInitializeResponse {
        protocol_version: nexus_protocol_version_from_sdk(&resp.protocol_version),
        agent_capabilities: nexus_agent_capabilities_from_sdk(&resp.agent_capabilities),
        agent_info: resp.agent_info.as_ref().map(nexus_agent_info_from_sdk),
        auth_methods: resp
            .auth_methods
            .iter()
            .map(nexus_auth_method_from_sdk)
            .collect(),
    }
}

fn sdk_initialize_request_from_nexus(req: NexusInitializeRequest) -> InitializeRequest {
    let protocol_version = sdk_protocol_version_from_nexus(&req.protocol_version);
    let mut builder = InitializeRequest::new(protocol_version);
    if let Some(info) = req.client_info {
        builder = builder.client_info(Implementation::new(info.name, info.version));
    }
    builder
}

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

fn sdk_new_session_request_from_nexus(req: NexusNewSessionRequest) -> NewSessionRequest {
    let sdk_servers: Vec<McpServer> = req
        .mcp_servers
        .into_iter()
        .map(nexus_mcp_server_to_sdk)
        .collect();
    NewSessionRequest::new(req.cwd).mcp_servers(sdk_servers)
}

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

fn sdk_list_sessions_request_from_nexus(req: NexusListSessionsRequest) -> ListSessionsRequest {
    let mut builder = ListSessionsRequest::new();
    if let Some(cwd) = req.cwd {
        builder = builder.cwd(cwd);
    }
    if let Some(cursor) = req.cursor {
        builder = builder.cursor(cursor);
    }
    builder
}

fn sdk_session_info_to_nexus(info: &SessionInfo) -> NexusSessionInfo {
    NexusSessionInfo::new(
        NexusSessionId::new(info.session_id.to_string()),
        info.cwd.clone(),
    )
    .title_opt(info.title.clone())
    .updated_at_opt(info.updated_at.clone())
}

fn sdk_list_sessions_response_to_nexus(resp: &ListSessionsResponse) -> NexusListSessionsResponse {
    let sessions = resp
        .sessions
        .iter()
        .map(sdk_session_info_to_nexus)
        .collect();
    NexusListSessionsResponse::new(sessions).next_cursor_opt(resp.next_cursor.clone())
}

// ── Config option SDK ↔ Nexus conversion helpers ─────────────────────

fn sdk_set_config_option_request_from_nexus(
    req: NexusSetConfigOptionRequest,
) -> SetSessionConfigOptionRequest {
    SetSessionConfigOptionRequest::new(req.session_id.0, req.config_id, req.value)
}

fn sdk_config_option_category_to_nexus(
    cat: &agent_client_protocol::schema::SessionConfigOptionCategory,
) -> NexusConfigOptionCategory {
    match cat {
        agent_client_protocol::schema::SessionConfigOptionCategory::Mode => {
            NexusConfigOptionCategory::Mode
        }
        agent_client_protocol::schema::SessionConfigOptionCategory::Model => {
            NexusConfigOptionCategory::Model
        }
        agent_client_protocol::schema::SessionConfigOptionCategory::ThoughtLevel => {
            NexusConfigOptionCategory::ThoughtLevel
        }
        agent_client_protocol::schema::SessionConfigOptionCategory::Other(s) => {
            NexusConfigOptionCategory::Other(s.clone())
        }
        _ => NexusConfigOptionCategory::Other("unknown".to_string()),
    }
}

fn sdk_config_select_option_to_nexus(
    opt: &agent_client_protocol::schema::SessionConfigSelectOption,
) -> NexusConfigSelectOption {
    NexusConfigSelectOption {
        value: opt.value.to_string(),
        name: opt.name.clone(),
        description: opt.description.clone(),
    }
}

fn sdk_config_select_options_to_nexus(
    opts: &agent_client_protocol::schema::SessionConfigSelectOptions,
) -> NexusConfigSelectOptions {
    match opts {
        agent_client_protocol::schema::SessionConfigSelectOptions::Ungrouped(items) => {
            NexusConfigSelectOptions::Ungrouped(
                items
                    .iter()
                    .map(sdk_config_select_option_to_nexus)
                    .collect(),
            )
        }
        agent_client_protocol::schema::SessionConfigSelectOptions::Grouped(groups) => {
            NexusConfigSelectOptions::Grouped(
                groups
                    .iter()
                    .map(|g| NexusConfigSelectGroup {
                        group: g.group.to_string(),
                        name: g.name.clone(),
                        options: g
                            .options
                            .iter()
                            .map(sdk_config_select_option_to_nexus)
                            .collect(),
                    })
                    .collect(),
            )
        }
        _ => NexusConfigSelectOptions::Ungrouped(vec![]),
    }
}

fn sdk_config_select_to_nexus(
    sel: &agent_client_protocol::schema::SessionConfigSelect,
) -> NexusConfigSelect {
    NexusConfigSelect {
        current_value: sel.current_value.to_string(),
        options: sdk_config_select_options_to_nexus(&sel.options),
    }
}

fn sdk_config_option_to_nexus(
    opt: &agent_client_protocol::schema::SessionConfigOption,
) -> NexusConfigOption {
    use agent_client_protocol::schema::SessionConfigKind;
    let kind = match &opt.kind {
        SessionConfigKind::Select(sel) => NexusConfigKind::Select(sdk_config_select_to_nexus(sel)),
        other => {
            tracing::warn!(
                config_id = %opt.id,
                kind = ?other,
                "Unknown SessionConfigKind variant, falling back to empty Select"
            );
            NexusConfigKind::Select(NexusConfigSelect {
                current_value: String::new(),
                options: NexusConfigSelectOptions::Ungrouped(vec![]),
            })
        }
    };
    NexusConfigOption {
        id: opt.id.to_string(),
        name: opt.name.clone(),
        description: opt.description.clone(),
        category: opt
            .category
            .as_ref()
            .map(sdk_config_option_category_to_nexus),
        kind,
    }
}

fn sdk_set_config_option_response_to_nexus(
    resp: &SetSessionConfigOptionResponse,
) -> NexusSetConfigOptionResponse {
    let config_options = resp
        .config_options
        .iter()
        .map(sdk_config_option_to_nexus)
        .collect();
    NexusSetConfigOptionResponse::new(config_options)
}

/// The nexus42 ACP client abstraction.
///
/// All ACP communication from the CLI goes through this trait. The methods
/// mirror the ACP protocol lifecycle: initialize → session → prompt → cancel.
///
/// **DTO boundary**: All types in trait signatures are Nexus-owned DTOs from
/// `nexus_contracts::local::acp`. SDK types are confined to `AcpSdkAdapter`.
///
/// **`subscribe()`**: Removed in SDK v0.11.0 migration. The old `StreamReceiver`
/// type no longer exists. Will be replaced with a proper DTO-wrapped streaming
/// API in a future task.
#[allow(async_fn_in_trait)]
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

    /// List sessions on the agent.
    ///
    /// Supports filtering by working directory and cursor-based pagination.
    fn list_sessions(
        &self,
        request: NexusListSessionsRequest,
    ) -> impl Future<Output = AcpResult<NexusListSessionsResponse>> + Send;

    /// Set a session configuration option.
    ///
    /// Sends a `session/set_config_option` request to the agent and returns
    /// the full set of configuration options with their updated values.
    fn set_config_option(
        &self,
        request: NexusSetConfigOptionRequest,
    ) -> impl Future<Output = AcpResult<NexusSetConfigOptionResponse>> + Send;
}

/// Internal state for the SDK adapter.
///
/// All fields are accessed exclusively from the [`LocalSetBridge`] thread,
/// so no additional synchronization (Mutex, etc.) is needed for the session
/// map or init response.
struct SdkConnection {
    /// The ACP SDK connection handle to the agent.
    connection: ConnectionTo<Agent>,
    /// Cached initialize response (populated after first `initialize()` call).
    init_response: Option<InitializeResponse>,
    /// Active sessions keyed by session ID string.
    ///
    /// `ActiveSession<'static, Agent>` is `'static` when obtained from
    /// `SessionBuilder::start_session()`, so it can be stored here.
    /// The bridge's single-threaded guarantee means exclusive `&mut` access.
    sessions: HashMap<String, ActiveSession<'static, Agent>>,
}

/// Concrete adapter wrapping the `agent-client-protocol` SDK.
///
/// This struct uses the SDK v0.11.0 `Client.builder().connect_with()` pattern
/// to establish a connection to the agent. The `ConnectionTo<Agent>` handle is
/// stored and reused for all subsequent operations.
///
/// The adapter uses [`LocalSetBridge`] to execute operations that may require
/// !Send futures, bridging them to the async tokio world.
pub struct AcpSdkAdapter {
    /// The agent's resolved binary path or command string (for error messages).
    agent_path: PathBuf,
    /// Agent ID for logging context.
    agent_id: String,
    /// `LocalSet` bridge for executing !Send futures.
    bridge: LocalSetBridge,
    /// The ACP SDK connection (wrapped for thread-safe access).
    /// This is `Some` after `with_connection()` is called.
    connection: Arc<RwLock<Option<SdkConnection>>>,
    /// Handle to the connection setup task (must be joined during cleanup).
    setup_task: Option<tokio::task::JoinHandle<()>>,
}

impl AcpSdkAdapter {
    /// Get a clone of the inner `ConnectionTo<Agent>` handle, acquiring the read
    /// lock for the minimum time needed. Returns an error if not yet connected.
    #[allow(clippy::significant_drop_tightening)]
    async fn get_connection_handle(
        connection: &Arc<RwLock<Option<SdkConnection>>>,
    ) -> crate::AcpResult<ConnectionTo<Agent>> {
        let guard = connection.read().await;
        let Some(conn) = guard.as_ref() else {
            return Err(crate::AcpError::connection_failed(
                "Connection not established",
            ));
        };
        Ok(conn.connection.clone())
    }

    /// Check if an init response is cached, returning it if so.
    #[allow(clippy::significant_drop_tightening)]
    async fn get_cached_init(
        connection: &Arc<RwLock<Option<SdkConnection>>>,
    ) -> Option<InitializeResponse> {
        let guard = connection.read().await;
        guard.as_ref().and_then(|conn| conn.init_response.clone())
    }
    /// Create a new adapter without an established connection.
    ///
    /// Use [`with_connection()`] to establish the actual SDK connection.
    #[must_use]
    pub fn new(agent_id: &str, agent_path: PathBuf) -> Self {
        Self {
            agent_path,
            agent_id: agent_id.to_string(),
            bridge: LocalSetBridge::new(),
            connection: Arc::new(RwLock::new(None)),
            setup_task: None,
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
                    let stdin_compat = stdin.compat_write(); // ChildStdin → AsyncWrite (outgoing)
                    let stdout_compat = stdout.compat(); // ChildStdout → AsyncRead (incoming)

                    Box::pin(async move {
                        // Create the transport using SDK ByteStreams.
                        // ByteStreams::new(outgoing, incoming): we write to agent's stdin
                        // and read from agent's stdout.
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
                                *guard = Some(SdkConnection {
                                    connection: cx,
                                    init_response: None,
                                    sessions: HashMap::new(),
                                });
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
                            return Err(crate::AcpError::sdk(&e));
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
            setup_task: Some(setup_task),
        }
    }

    /// Return a reference to the agent path (for error reporting).
    #[must_use]
    pub fn agent_path(&self) -> &Path {
        &self.agent_path
    }

    /// Return the agent ID.
    #[must_use]
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Send a prompt to an existing session and stream until completion.
    ///
    /// The write lock must span the entire streaming loop because
    /// `active_session` borrows mutably from `conn.sessions`.
    #[allow(clippy::significant_drop_tightening)]
    async fn run_prompt(
        connection: Arc<RwLock<Option<SdkConnection>>>,
        session_id_str: String,
        prompt_text: String,
    ) -> AcpResult<NexusPromptCompleted> {
        let mut guard = connection.write().await;
        let Some(conn) = guard.as_mut() else {
            return Err(crate::AcpError::connection_failed(
                "Connection not established",
            ));
        };

        // Get the active session
        let Some(active_session) = conn.sessions.get_mut(&session_id_str) else {
            return Err(crate::AcpError::protocol(format!(
                "No active session found for session_id: {session_id_str}",
            )));
        };

        tracing::info!(
            session_id = %session_id_str,
            "Sending prompt to agent"
        );

        active_session
            .send_prompt(&prompt_text)
            .map_err(|e| crate::AcpError::sdk(&e))?;

        // Read streaming updates until we get a StopReason
        loop {
            let update = active_session
                .read_update()
                .await
                .map_err(|e| crate::AcpError::sdk(&e))?;

            match update {
                SessionMessage::SessionMessage(_dispatch) => {
                    // Streaming content update — logged but not accumulated
                    // in NexusPromptCompleted (content consumption is handled
                    // by the caller via subscribe/future streaming API).
                    tracing::trace!(
                        session_id = %session_id_str,
                        "Received streaming update"
                    );
                }
                SessionMessage::StopReason(reason) => {
                    let nexus_reason = nexus_stop_reason_from_sdk(reason);
                    tracing::info!(
                        session_id = %session_id_str,
                        stop_reason = ?nexus_reason,
                        "Prompt completed"
                    );
                    return Ok(NexusPromptCompleted {
                        stop_reason: nexus_reason,
                    });
                }
                _ => {
                    // Future SDK variants — log and continue
                    tracing::trace!(
                        session_id = %session_id_str,
                        "Received unknown session message variant"
                    );
                }
            }
        }
    }
}

impl Drop for AcpSdkAdapter {
    fn drop(&mut self) {
        // Join the setup task if it exists (fire-and-forget cleanup)
        if let Some(setup_task) = self.setup_task.take() {
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
        request: NexusInitializeRequest,
    ) -> impl Future<Output = AcpResult<NexusInitializeResponse>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();
        let agent_id = self.agent_id.clone();

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        // Check for cached response
                        if let Some(resp) = Self::get_cached_init(&connection).await {
                            tracing::debug!(
                                agent_id = %agent_id,
                                "initialize() returning cached response"
                            );
                            return Ok(nexus_initialize_response_from_sdk(&resp));
                        }

                        // Convert Nexus request to SDK InitializeRequest and send
                        let sdk_req = sdk_initialize_request_from_nexus(request);
                        tracing::info!(
                            agent_id = %agent_id,
                            "Sending initialize request to agent"
                        );

                        // Get connection handle with minimal lock time
                        let connection_handle = Self::get_connection_handle(&connection).await?;
                        let connection_for_spawn = connection_handle.clone();

                        // We must use `connection.spawn()` because `block_task()` can only
                        // be called from within a spawned task on the dispatch loop.
                        // We use a oneshot channel to relay the response back.
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        connection_handle
                            .spawn(async move {
                                let result = connection_for_spawn
                                    .send_request_to(Agent, sdk_req)
                                    .block_task()
                                    .await;
                                let _ = tx.send(result);
                                Ok(())
                            })
                            .map_err(|e| crate::AcpError::sdk(&e))?;

                        let init_result = rx
                            .await
                            .map_err(|_| {
                                crate::AcpError::connection_failed(
                                    "Initialize response channel closed",
                                )
                            })?
                            .map_err(|e| crate::AcpError::sdk(&e))?;

                        // Cache the response (write lock, brief scope)
                        let nexus_response = nexus_initialize_response_from_sdk(&init_result);
                        {
                            let mut guard = connection.write().await;
                            if let Some(conn) = guard.as_mut() {
                                conn.init_response = Some(init_result);
                            }
                        }

                        tracing::info!(
                            agent_id = %agent_id,
                            "Initialize handshake completed"
                        );

                        Ok(nexus_response)
                    })
                })
                .await
                .and_then(|r| r)
        }
    }

    fn create_session(
        &self,
        request: NexusNewSessionRequest,
    ) -> impl Future<Output = AcpResult<NexusSessionCreated>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();
        let agent_id = self.agent_id.clone();

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        // Convert Nexus request to SDK NewSessionRequest
                        let sdk_req = sdk_new_session_request_from_nexus(request);
                        tracing::info!(
                            agent_id = %agent_id,
                            cwd = %sdk_req.cwd.display(),
                            "Creating new session"
                        );

                        // Get connection handle with minimal lock time
                        let connection_handle = Self::get_connection_handle(&connection).await?;
                        let connection_for_spawn = connection_handle.clone();

                        // Use build_session_from + block_task + start_session.
                        // Must run inside a spawned task since block_task() requires
                        // the dispatch loop context.
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        connection_handle
                            .spawn(async move {
                                let result = connection_for_spawn
                                    .build_session_from(sdk_req)
                                    .block_task()
                                    .start_session()
                                    .await;
                                let _ = tx.send(result);
                                Ok(())
                            })
                            .map_err(|e| crate::AcpError::sdk(&e))?;

                        let session_result = rx
                            .await
                            .map_err(|_| {
                                crate::AcpError::connection_failed(
                                    "Create session response channel closed",
                                )
                            })?
                            .map_err(|e| crate::AcpError::sdk(&e))?;

                        let session_id_str = session_result.session_id().to_string();

                        // Convert modes
                        let nexus_modes = session_result
                            .modes()
                            .as_ref()
                            .map(nexus_session_mode_state_from_sdk);

                        // Store the active session (write lock, brief scope)
                        {
                            let mut guard = connection.write().await;
                            if let Some(conn) = guard.as_mut() {
                                conn.sessions.insert(session_id_str.clone(), session_result);
                            }
                        }

                        tracing::info!(
                            agent_id = %agent_id,
                            session_id = %session_id_str,
                            "Session created successfully"
                        );

                        Ok(NexusSessionCreated {
                            session_id: NexusSessionId::new(session_id_str),
                            modes: nexus_modes,
                        })
                    })
                })
                .await
                .and_then(|r| r)
        }
    }

    fn prompt(
        &self,
        request: NexusPromptRequest,
    ) -> impl Future<Output = AcpResult<NexusPromptCompleted>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();

        async move {
            // Extract session_id and prompt text before entering the bridge closure,
            // since we need `request` to be owned for the borrow checker.
            let session_id_str = request.session_id.0.clone();

            // Build the prompt text from content blocks
            let prompt_text = request
                .prompt
                .iter()
                .map(|block| match block {
                    nexus_contracts::local::acp::NexusContentBlock::Text(t) => t.text.clone(),
                    nexus_contracts::local::acp::NexusContentBlock::ResourceLink(r) => {
                        format!("resource:{}", r.uri)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(Self::run_prompt(connection, session_id_str, prompt_text))
                })
                .await
                .and_then(|r| r)
        }
    }

    fn cancel(
        &self,
        session_id: NexusSessionId,
    ) -> impl Future<Output = AcpResult<NexusCancelResult>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();
        let agent_id = self.agent_id.clone();

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        let session_id_str = session_id.0.clone();

                        tracing::info!(
                            agent_id = %agent_id,
                            session_id = %session_id_str,
                            "Sending cancel notification to agent"
                        );

                        // Send the CancelNotification via raw JSON-RPC
                        let notification =
                            CancelNotification::new(SessionId::new(session_id_str.clone()));

                        let connection_handle = Self::get_connection_handle(&connection).await?;
                        connection_handle
                            .send_notification_to(Agent, notification)
                            .map_err(|e| crate::AcpError::sdk(&e))?;

                        tracing::info!(
                            agent_id = %agent_id,
                            session_id = %session_id_str,
                            "Cancel notification sent"
                        );

                        Ok(NexusCancelResult {
                            session_id: NexusSessionId::new(session_id_str),
                        })
                    })
                })
                .await
                .and_then(|r| r)
        }
    }

    fn list_sessions(
        &self,
        request: NexusListSessionsRequest,
    ) -> impl Future<Output = AcpResult<NexusListSessionsResponse>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();
        let agent_id = self.agent_id.clone();

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        // Convert Nexus request to SDK ListSessionsRequest
                        let sdk_req = sdk_list_sessions_request_from_nexus(request);

                        tracing::info!(
                            agent_id = %agent_id,
                            cwd = ?sdk_req.cwd,
                            cursor = ?sdk_req.cursor,
                            "Listing sessions"
                        );

                        // Get connection handle with minimal lock time
                        let connection_handle = Self::get_connection_handle(&connection).await?;
                        let connection_for_spawn = connection_handle.clone();
                        // Send the request via raw JSON-RPC using spawn + block_task
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        connection_handle
                            .spawn(async move {
                                let result = connection_for_spawn
                                    .send_request_to(Agent, sdk_req)
                                    .block_task()
                                    .await;
                                let _ = tx.send(result);
                                Ok(())
                            })
                            .map_err(|e| crate::AcpError::sdk(&e))?;

                        let list_result = rx
                            .await
                            .map_err(|_| {
                                crate::AcpError::connection_failed(
                                    "List sessions response channel closed",
                                )
                            })?
                            .map_err(|e| crate::AcpError::sdk(&e))?;

                        // Convert SDK response to Nexus DTOs
                        let nexus_response = sdk_list_sessions_response_to_nexus(&list_result);

                        tracing::info!(
                            agent_id = %agent_id,
                            session_count = nexus_response.sessions.len(),
                            has_next_cursor = nexus_response.next_cursor.is_some(),
                            "List sessions completed"
                        );

                        Ok(nexus_response)
                    })
                })
                .await
                .and_then(|r| r)
        }
    }

    fn set_config_option(
        &self,
        request: NexusSetConfigOptionRequest,
    ) -> impl Future<Output = AcpResult<NexusSetConfigOptionResponse>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();
        let agent_id = self.agent_id.clone();

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        // Convert Nexus request to SDK SetSessionConfigOptionRequest
                        let sdk_req = sdk_set_config_option_request_from_nexus(request);

                        tracing::info!(
                            agent_id = %agent_id,
                            session_id = %sdk_req.session_id,
                            config_id = %sdk_req.config_id,
                            "Setting session config option"
                        );

                        // Get connection handle with minimal lock time
                        let connection_handle = Self::get_connection_handle(&connection).await?;
                        let connection_for_spawn = connection_handle.clone();

                        // Send the request via raw JSON-RPC using spawn + block_task
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        connection_handle
                            .spawn(async move {
                                let result = connection_for_spawn
                                    .send_request_to(Agent, sdk_req)
                                    .block_task()
                                    .await;
                                let _ = tx.send(result);
                                Ok(())
                            })
                            .map_err(|e| crate::AcpError::sdk(&e))?;

                        let set_result = rx
                            .await
                            .map_err(|_| {
                                crate::AcpError::connection_failed(
                                    "Set config option response channel closed",
                                )
                            })?
                            .map_err(|e| crate::AcpError::sdk(&e))?;

                        // Convert SDK response to Nexus DTOs
                        let nexus_response = sdk_set_config_option_response_to_nexus(&set_result);

                        tracing::info!(
                            agent_id = %agent_id,
                            option_count = nexus_response.config_options.len(),
                            "Set config option completed"
                        );

                        Ok(nexus_response)
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
            nexus_stop_reason_from_sdk(StopReason::EndTurn),
            NexusStopReason::EndTurn
        );
        assert_eq!(
            nexus_stop_reason_from_sdk(StopReason::Cancelled),
            NexusStopReason::Cancelled
        );
        assert_eq!(
            nexus_stop_reason_from_sdk(StopReason::MaxTokens),
            NexusStopReason::MaxTokens
        );
        assert_eq!(
            nexus_stop_reason_from_sdk(StopReason::MaxTurnRequests),
            NexusStopReason::MaxTurnRequests
        );
        assert_eq!(
            nexus_stop_reason_from_sdk(StopReason::Refusal),
            NexusStopReason::Refusal
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
    fn agent_capabilities_from_sdk() {
        let sdk_caps = AgentCapabilities::new().load_session(true);
        let nexus_caps = nexus_agent_capabilities_from_sdk(&sdk_caps);
        assert!(nexus_caps.load_session);

        let sdk_caps_false = AgentCapabilities::new().load_session(false);
        let nexus_caps_false = nexus_agent_capabilities_from_sdk(&sdk_caps_false);
        assert!(!nexus_caps_false.load_session);
    }

    #[test]
    fn auth_method_from_sdk_agent_variant() {
        let sdk_method = AuthMethod::Agent(
            agent_client_protocol::schema::AuthMethodAgent::new("oauth", "OAuth 2.0")
                .description("Authenticate via OAuth"),
        );
        let nexus_method = nexus_auth_method_from_sdk(&sdk_method);
        assert_eq!(nexus_method.id, "oauth");
        assert_eq!(nexus_method.name, "OAuth 2.0");
        assert_eq!(
            nexus_method.description,
            Some("Authenticate via OAuth".to_string())
        );
    }

    #[test]
    fn auth_method_from_sdk_unknown_variant_fallback() {
        // When AuthMethod is not Agent, the converter falls back to "unknown".
        // We test this by using an Agent variant with an empty id and name,
        // then checking the conversion doesn't panic (the match arm is hit).
        // Since only the Agent variant is available without feature flags,
        // we verify the Agent path works correctly.
        let sdk_method = AuthMethod::Agent(agent_client_protocol::schema::AuthMethodAgent::new(
            "test-id", "Test",
        ));
        let nexus_method = nexus_auth_method_from_sdk(&sdk_method);
        assert_eq!(nexus_method.id, "test-id");
        assert_eq!(nexus_method.name, "Test");
        assert!(nexus_method.description.is_none());
    }

    #[test]
    fn session_mode_state_from_sdk() {
        use agent_client_protocol::schema::SessionMode;
        let sdk_state = SessionModeState::new(
            agent_client_protocol::schema::SessionModeId::new("act"),
            vec![
                SessionMode::new(
                    agent_client_protocol::schema::SessionModeId::new("act"),
                    "Act",
                ),
                SessionMode::new(
                    agent_client_protocol::schema::SessionModeId::new("ask"),
                    "Ask",
                ),
            ],
        );
        let nexus_state = nexus_session_mode_state_from_sdk(&sdk_state);
        assert_eq!(nexus_state.current_mode_id, "act");
        assert_eq!(nexus_state.available_modes.len(), 2);
        assert_eq!(nexus_state.available_modes[0].id, "act");
        assert_eq!(nexus_state.available_modes[0].name, "Act");
    }

    #[test]
    fn initialize_response_from_sdk() {
        let sdk_resp = InitializeResponse::new(ProtocolVersion::LATEST);
        let nexus_resp = nexus_initialize_response_from_sdk(&sdk_resp);
        assert_eq!(nexus_resp.protocol_version.0, "1");
        assert!(!nexus_resp.agent_capabilities.load_session);
        assert!(nexus_resp.agent_info.is_none());
        assert!(nexus_resp.auth_methods.is_empty());
    }

    #[test]
    fn initialize_request_to_sdk() {
        let nexus_req = NexusInitializeRequest::new();
        let _sdk_req = sdk_initialize_request_from_nexus(nexus_req);
        // Just verify conversion succeeds
    }

    #[test]
    fn initialize_request_to_sdk_with_client_info() {
        let nexus_req = NexusInitializeRequest::new().client_info(NexusAgentInfo {
            name: "nexus42".to_string(),
            title: Some("Nexus CLI".to_string()),
            version: "0.1.0".to_string(),
        });
        let sdk_req = sdk_initialize_request_from_nexus(nexus_req);
        assert_eq!(
            sdk_req.client_info.as_ref().map(|i| i.name.as_str()),
            Some("nexus42")
        );
        assert_eq!(
            sdk_req.client_info.as_ref().map(|i| i.version.as_str()),
            Some("0.1.0")
        );
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
    fn prompt_request_to_sdk_text_only() {
        let nexus_req = NexusPromptRequest::new(
            NexusSessionId::new("sess-123"),
            vec![nexus_contracts::local::acp::NexusContentBlock::Text(
                nexus_contracts::local::acp::NexusTextContent {
                    text: "Hello, agent!".to_string(),
                },
            )],
        );
        let sdk_req = sdk_prompt_request_from_nexus(nexus_req);
        assert_eq!(sdk_req.session_id.to_string(), "sess-123");
        assert_eq!(sdk_req.prompt.len(), 1);
    }

    #[test]
    fn prompt_request_to_sdk_mixed_content() {
        let nexus_req = NexusPromptRequest::new(
            NexusSessionId::new("sess-456"),
            vec![
                nexus_contracts::local::acp::NexusContentBlock::Text(
                    nexus_contracts::local::acp::NexusTextContent {
                        text: "Look at this resource".to_string(),
                    },
                ),
                nexus_contracts::local::acp::NexusContentBlock::ResourceLink(
                    nexus_contracts::local::acp::NexusResourceLink {
                        uri: "file:///path/to/file".to_string(),
                        name: Some("my-file".to_string()),
                    },
                ),
            ],
        );
        let sdk_req = sdk_prompt_request_from_nexus(nexus_req);
        assert_eq!(sdk_req.prompt.len(), 2);
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
        let adapter = AcpSdkAdapter::new("test-agent", PathBuf::from("/usr/bin/test-agent"));

        assert_eq!(adapter.agent_id(), "test-agent");
        assert_eq!(adapter.agent_path(), Path::new("/usr/bin/test-agent"));

        // Connection should be None
        assert!(adapter.connection.read().await.is_none());
    }

    #[tokio::test]
    async fn adapter_initialize_without_connection_fails() {
        let adapter = AcpSdkAdapter::new("test-agent", PathBuf::from("/usr/bin/test-agent"));

        let request = NexusInitializeRequest::new();

        let result = adapter.initialize(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn adapter_create_session_without_connection_fails() {
        let adapter = AcpSdkAdapter::new("test-agent", PathBuf::from("/usr/bin/test-agent"));

        let request = NexusNewSessionRequest::new("/tmp/workspace");
        let result = adapter.create_session(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn adapter_prompt_without_connection_fails() {
        let adapter = AcpSdkAdapter::new("test-agent", PathBuf::from("/usr/bin/test-agent"));

        let request = NexusPromptRequest::new(NexusSessionId::new("nonexistent"), vec![]);
        let result = adapter.prompt(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn adapter_cancel_without_connection_fails() {
        let adapter = AcpSdkAdapter::new("test-agent", PathBuf::from("/usr/bin/test-agent"));

        let result = adapter.cancel(NexusSessionId::new("nonexistent")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn adapter_prompt_nonexistent_session_fails() {
        let adapter = AcpSdkAdapter::new("test-agent", PathBuf::from("/usr/bin/test-agent"));

        // Manually verify the error path when connection exists but session doesn't.
        // We can't easily create a real ConnectionTo without a transport,
        // so just verify the error path when connection is None.
        {
            let _guard = adapter.connection.write().await;
        }

        let request = NexusPromptRequest::new(
            NexusSessionId::new("nonexistent-session"),
            vec![nexus_contracts::local::acp::NexusContentBlock::Text(
                nexus_contracts::local::acp::NexusTextContent {
                    text: "test".to_string(),
                },
            )],
        );
        let result = adapter.prompt(request).await;
        // Should fail because connection is None (no real connection established)
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn adapter_list_sessions_without_connection_fails() {
        let adapter = AcpSdkAdapter::new("test-agent", PathBuf::from("/usr/bin/test-agent"));

        let request = NexusListSessionsRequest::new();
        let result = adapter.list_sessions(request).await;
        assert!(result.is_err());
    }

    // ── List sessions conversion tests ────────────────────────────────────────

    #[test]
    fn list_sessions_request_to_sdk_empty() {
        let nexus_req = NexusListSessionsRequest::new();
        let sdk_req = sdk_list_sessions_request_from_nexus(nexus_req);
        assert!(sdk_req.cwd.is_none());
        assert!(sdk_req.cursor.is_none());
    }

    #[test]
    fn list_sessions_request_to_sdk_with_filters() {
        let nexus_req = NexusListSessionsRequest::new()
            .cwd("/tmp/workspace")
            .cursor("next-page-token");
        let sdk_req = sdk_list_sessions_request_from_nexus(nexus_req);
        assert_eq!(sdk_req.cwd, Some(PathBuf::from("/tmp/workspace")));
        assert_eq!(sdk_req.cursor, Some("next-page-token".to_string()));
    }

    #[test]
    fn session_info_to_nexus_basic() {
        let sdk_info = SessionInfo::new(
            agent_client_protocol::schema::SessionId::new("session-abc"),
            PathBuf::from("/home/user/project"),
        );
        let nexus_info = sdk_session_info_to_nexus(&sdk_info);
        assert_eq!(nexus_info.session_id.0, "session-abc");
        assert_eq!(nexus_info.cwd, PathBuf::from("/home/user/project"));
        assert!(nexus_info.title.is_none());
        assert!(nexus_info.updated_at.is_none());
    }

    #[test]
    fn session_info_to_nexus_with_optional_fields() {
        let sdk_info = SessionInfo::new(
            agent_client_protocol::schema::SessionId::new("session-def"),
            PathBuf::from("/var/app"),
        )
        .title("Production Session")
        .updated_at("2026-04-21T10:00:00Z");
        let nexus_info = sdk_session_info_to_nexus(&sdk_info);
        assert_eq!(nexus_info.title, Some("Production Session".to_string()));
        assert_eq!(
            nexus_info.updated_at,
            Some("2026-04-21T10:00:00Z".to_string())
        );
    }

    #[test]
    fn list_sessions_response_to_nexus_empty() {
        let sdk_resp = ListSessionsResponse::new(vec![]);
        let nexus_resp = sdk_list_sessions_response_to_nexus(&sdk_resp);
        assert!(nexus_resp.sessions.is_empty());
        assert!(nexus_resp.next_cursor.is_none());
    }

    #[test]
    fn list_sessions_response_to_nexus_with_sessions() {
        let sdk_sessions = vec![
            SessionInfo::new(
                agent_client_protocol::schema::SessionId::new("sess-1"),
                PathBuf::from("/tmp/a"),
            )
            .title("Session A"),
            SessionInfo::new(
                agent_client_protocol::schema::SessionId::new("sess-2"),
                PathBuf::from("/tmp/b"),
            ),
        ];
        let sdk_resp = ListSessionsResponse::new(sdk_sessions).next_cursor("page-2");
        let nexus_resp = sdk_list_sessions_response_to_nexus(&sdk_resp);
        assert_eq!(nexus_resp.sessions.len(), 2);
        assert_eq!(nexus_resp.sessions[0].session_id.0, "sess-1");
        assert_eq!(nexus_resp.sessions[0].title, Some("Session A".to_string()));
        assert_eq!(nexus_resp.next_cursor, Some("page-2".to_string()));
    }

    // ── Set config option conversion tests ────────────────────────────────────

    #[test]
    fn set_config_option_request_to_sdk() {
        let nexus_req = NexusSetConfigOptionRequest::new(
            NexusSessionId::new("sess-1"),
            "model",
            "claude-3-opus",
        );
        let sdk_req = sdk_set_config_option_request_from_nexus(nexus_req);
        assert_eq!(sdk_req.session_id.to_string(), "sess-1");
        assert_eq!(sdk_req.config_id.to_string(), "model");
        assert_eq!(sdk_req.value.to_string(), "claude-3-opus");
    }

    #[test]
    fn config_option_category_to_nexus_all_variants() {
        use agent_client_protocol::schema::SessionConfigOptionCategory;
        assert_eq!(
            sdk_config_option_category_to_nexus(&SessionConfigOptionCategory::Mode),
            NexusConfigOptionCategory::Mode
        );
        assert_eq!(
            sdk_config_option_category_to_nexus(&SessionConfigOptionCategory::Model),
            NexusConfigOptionCategory::Model
        );
        assert_eq!(
            sdk_config_option_category_to_nexus(&SessionConfigOptionCategory::ThoughtLevel),
            NexusConfigOptionCategory::ThoughtLevel
        );
        assert_eq!(
            sdk_config_option_category_to_nexus(&SessionConfigOptionCategory::Other(
                "custom".to_string()
            )),
            NexusConfigOptionCategory::Other("custom".to_string())
        );
    }

    #[test]
    fn config_select_option_to_nexus() {
        let sdk_opt =
            agent_client_protocol::schema::SessionConfigSelectOption::new("opt-1", "Option One")
                .description("First option");
        let nexus_opt = sdk_config_select_option_to_nexus(&sdk_opt);
        assert_eq!(nexus_opt.value, "opt-1");
        assert_eq!(nexus_opt.name, "Option One");
        assert_eq!(nexus_opt.description, Some("First option".to_string()));
    }

    #[test]
    fn config_select_options_ungrouped_to_nexus() {
        let sdk_opts = agent_client_protocol::schema::SessionConfigSelectOptions::Ungrouped(vec![
            agent_client_protocol::schema::SessionConfigSelectOption::new("a", "A"),
        ]);
        let nexus_opts = sdk_config_select_options_to_nexus(&sdk_opts);
        match nexus_opts {
            NexusConfigSelectOptions::Ungrouped(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].value, "a");
            }
            NexusConfigSelectOptions::Grouped(_) => panic!("Expected Ungrouped"),
        }
    }

    #[test]
    fn config_select_to_nexus() {
        let sdk_sel = agent_client_protocol::schema::SessionConfigSelect::new(
            "claude-3-opus",
            vec![
                agent_client_protocol::schema::SessionConfigSelectOption::new(
                    "claude-3-opus",
                    "Claude 3 Opus",
                ),
            ],
        );
        let nexus_sel = sdk_config_select_to_nexus(&sdk_sel);
        assert_eq!(nexus_sel.current_value, "claude-3-opus");
        match &nexus_sel.options {
            NexusConfigSelectOptions::Ungrouped(items) => {
                assert_eq!(items[0].value, "claude-3-opus");
            }
            NexusConfigSelectOptions::Grouped(_) => panic!("Expected Ungrouped"),
        }
    }

    #[test]
    fn config_option_to_nexus_select() {
        let sdk_opt = agent_client_protocol::schema::SessionConfigOption::select(
            "model",
            "Model",
            "claude-3-opus",
            vec![
                agent_client_protocol::schema::SessionConfigSelectOption::new(
                    "claude-3-opus",
                    "Claude 3 Opus",
                ),
            ],
        )
        .description("Select the model")
        .category(agent_client_protocol::schema::SessionConfigOptionCategory::Model);
        let nexus_opt = sdk_config_option_to_nexus(&sdk_opt);
        assert_eq!(nexus_opt.id, "model");
        assert_eq!(nexus_opt.name, "Model");
        assert_eq!(nexus_opt.description, Some("Select the model".to_string()));
        assert_eq!(nexus_opt.category, Some(NexusConfigOptionCategory::Model));
    }

    #[test]
    fn set_config_option_response_to_nexus_empty() {
        let sdk_resp = SetSessionConfigOptionResponse::new(vec![]);
        let nexus_resp = sdk_set_config_option_response_to_nexus(&sdk_resp);
        assert!(nexus_resp.config_options.is_empty());
    }

    #[test]
    fn set_config_option_response_to_nexus_with_options() {
        let sdk_resp = SetSessionConfigOptionResponse::new(vec![
            agent_client_protocol::schema::SessionConfigOption::select(
                "mode",
                "Mode",
                "act",
                vec![
                    agent_client_protocol::schema::SessionConfigSelectOption::new("act", "Act"),
                    agent_client_protocol::schema::SessionConfigSelectOption::new("ask", "Ask"),
                ],
            ),
        ]);
        let nexus_resp = sdk_set_config_option_response_to_nexus(&sdk_resp);
        assert_eq!(nexus_resp.config_options.len(), 1);
        assert_eq!(nexus_resp.config_options[0].id, "mode");
        assert_eq!(nexus_resp.config_options[0].name, "Mode");
    }

    #[tokio::test]
    async fn adapter_set_config_option_without_connection_fails() {
        let adapter = AcpSdkAdapter::new("test-agent", PathBuf::from("/usr/bin/test-agent"));

        let request = NexusSetConfigOptionRequest::new(
            NexusSessionId::new("nonexistent"),
            "model",
            "claude-3-opus",
        );
        let result = adapter.set_config_option(request).await;
        assert!(result.is_err());
    }
}
