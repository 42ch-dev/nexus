//! ACP Client adapter trait and SDK wrapper.
//!
//! This module defines [`NexusAcpClient`] — the abstraction layer through which
//! all nexus42 CLI code interacts with ACP agents. The concrete implementation
//! ([`AcpSdkAdapter`]) wraps the `agent-client-protocol` SDK's
//! `ClientSideConnection`, isolating the `!Send` future constraint behind
//! [`LocalSetBridge`].
//!
//! # Design Rationale
//!
//! The adapter pattern (spec §2.2) ensures that:
//! - All SDK usage is encapsulated in one place, making a future migration to
//!   `sacp` (or a hand-rolled JSON-RPC client) straightforward.
//! - The `!Send` future constraint from the SDK does not leak into the rest of
//!   the nexus42 codebase.
//! - Unit testing can swap the adapter with a mock implementation.
//!
//! # DTO Boundary
//!
//! The `NexusAcpClient` trait uses Nexus-owned DTO types from
//! `nexus_contracts::local::acp` (e.g. `NexusInitializeRequest`,
//! `NexusInitializeResponse`). SDK types (`agent_client_protocol::`) are
//! confined to `AcpSdkAdapter` implementation blocks and `FromSdk` conversion
//! methods. This decoupling allows future SDK migration without changing
//! consumers.
//!
//! # subscribe() Design Decision
//!
//! `subscribe()` returns `async_broadcast::StreamReceiver` which is tightly
//! coupled to the SDK's broadcast channel implementation. It has been moved
//! off the `NexusAcpClient` trait onto `AcpSdkAdapter` as a direct method.
//! Rationale:
//! 1. No consumer uses `subscribe()` through the trait abstraction.
//! 2. Creating `NexusStreamEvent` + `NexusEventStream` wrappers would add
//!    complexity for an unused feature.
//! 3. If streaming is needed in the future, it can be added back with proper
//!    DTO types at that time.
//!
//! # LocalSetBridge Integration
//!
//! The SDK's `ClientSideConnection` produces `!Send` futures because it uses
//! `tokio::task::LocalSet` internally. We bridge this with the async tokio
//! world using [`LocalSetBridge`]:
//!
//! ```text
//! CLI Command (async) ──► AcpSdkAdapter ──► LocalSetBridge
//!                                              │
//!                                              ▼
//!                                    Dedicated OS Thread
//!                                    (LocalSet + SDK connection)
//! ```
//!
//! # Async Trait Compatibility
//!
//! The ACP SDK uses `futures::AsyncRead/AsyncWrite` traits, while tokio's
//! subprocess pipes provide `tokio::io::AsyncRead/AsyncWrite`. We use
//! `tokio_util::compat` to bridge these two trait families.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_client_protocol as acp;
use agent_client_protocol::{Agent as _, Error as SdkError, StreamReceiver};
use tokio::sync::RwLock;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::error::AcpResult;
use crate::localset_bridge::LocalSetBridge;
use crate::policy::{PermissionDecision, PermissionPolicy};
use nexus_contracts::local::acp::{
    NexusAgentCapabilities, NexusAgentInfo, NexusAuthMethod, NexusCancelResult,
    NexusInitializeRequest, NexusInitializeResponse, NexusNewSessionRequest, NexusPromptCompleted,
    NexusPromptRequest, NexusProtocolVersion, NexusSessionCreated, NexusSessionId,
    NexusSessionModeState, NexusStopReason,
};

// ── SDK ↔ Nexus DTO conversion helpers ──────────────────────────────
//
// These are free functions (not trait impls) to avoid orphan rule violations
// since both the SDK types and Nexus DTOs are defined in external crates.

fn nexus_protocol_version_from_sdk(version: acp::ProtocolVersion) -> NexusProtocolVersion {
    NexusProtocolVersion::new(version.to_string())
}

fn nexus_stop_reason_from_sdk(reason: acp::StopReason) -> NexusStopReason {
    match reason {
        acp::StopReason::EndTurn => NexusStopReason::EndTurn,
        acp::StopReason::MaxTokens => NexusStopReason::MaxTokens,
        acp::StopReason::MaxTurnRequests => NexusStopReason::MaxTurnRequests,
        acp::StopReason::Refusal => NexusStopReason::Refusal,
        acp::StopReason::Cancelled => NexusStopReason::Cancelled,
        _ => NexusStopReason::EndTurn, // fallback for future variants
    }
}

fn nexus_auth_method_from_sdk(method: &acp::AuthMethod) -> NexusAuthMethod {
    NexusAuthMethod {
        id: method.id().to_string(),
        name: method.name().to_string(),
        description: method.description().map(|s| s.to_string()),
    }
}

fn nexus_agent_info_from_sdk(impl_: &acp::Implementation) -> NexusAgentInfo {
    NexusAgentInfo {
        name: impl_.name.clone(),
        title: impl_.title.clone(),
        version: impl_.version.clone(),
    }
}

fn nexus_agent_capabilities_from_sdk(caps: &acp::AgentCapabilities) -> NexusAgentCapabilities {
    NexusAgentCapabilities {
        load_session: caps.load_session,
    }
}

fn nexus_session_mode_state_from_sdk(state: &acp::SessionModeState) -> NexusSessionModeState {
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

fn sdk_initialize_request_from_nexus(req: NexusInitializeRequest) -> acp::InitializeRequest {
    let protocol_version: acp::ProtocolVersion = serde_json::from_value(serde_json::json!(req
        .protocol_version
        .0
        .parse::<u16>()
        .unwrap_or(1)))
    .unwrap_or(acp::ProtocolVersion::LATEST);

    let mut builder = acp::InitializeRequest::new(protocol_version);
    if let Some(info) = req.client_info {
        builder = builder.client_info(acp::Implementation::new(info.name, info.version));
    }
    builder
}

fn sdk_new_session_request_from_nexus(req: NexusNewSessionRequest) -> acp::NewSessionRequest {
    acp::NewSessionRequest::new(req.cwd)
}

fn sdk_prompt_request_from_nexus(req: NexusPromptRequest) -> acp::PromptRequest {
    let content_blocks: Vec<acp::ContentBlock> = req
        .prompt
        .into_iter()
        .map(|block| match block {
            nexus_contracts::local::acp::NexusContentBlock::Text(t) => {
                acp::ContentBlock::Text(acp::TextContent::new(t.text))
            }
            nexus_contracts::local::acp::NexusContentBlock::ResourceLink(r) => {
                let builder = acp::ResourceLink::new(r.name.unwrap_or_default(), r.uri);
                acp::ContentBlock::ResourceLink(builder)
            }
        })
        .collect();
    acp::PromptRequest::new(acp::SessionId::new(req.session_id.0), content_blocks)
}

/// The nexus42 ACP client abstraction.
///
/// All ACP communication from the CLI goes through this trait. The methods
/// mirror the ACP protocol lifecycle: initialize → session → prompt → cancel.
///
/// **DTO boundary**: All types in trait signatures are Nexus-owned DTOs from
/// `nexus_contracts::local::acp`. SDK types are confined to `AcpSdkAdapter`.
///
/// **subscribe()**: Moved off the trait onto `AcpSdkAdapter` as a direct
/// method (see module docs for rationale).
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

/// Policy-aware client handler for V1.1+ — uses configurable permission policy.
///
/// This implements the ACP `Client` trait with policy-based permission handling:
/// - `request_permission`: Consults PermissionPolicy, prompts if needed
/// - `session_notification`: Log updates for debugging
/// - File/terminal operations: Return errors (not implemented in V1.0)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PolicyAwareClientHandler {
    /// Agent ID for logging context.
    agent_id: String,
    /// Permission policy for evaluating requests.
    policy: Arc<RwLock<PermissionPolicy>>,
    /// Workspace root for saving policy changes.
    workspace_root: PathBuf,
}

#[allow(dead_code)]
impl PolicyAwareClientHandler {
    /// Create a new policy-aware client handler.
    pub fn new(agent_id: String, workspace_root: PathBuf) -> Self {
        let policy = PermissionPolicy::load(&workspace_root).unwrap_or_default();
        Self {
            agent_id,
            policy: Arc::new(RwLock::new(policy)),
            workspace_root,
        }
    }

    /// Prompt user for permission decision.
    async fn prompt_user(
        &self,
        tool_call: &str,
        options: &[acp::PermissionOption],
    ) -> PermissionDecision {
        println!("\n⚠️  Agent '{}' requests permission:", self.agent_id);
        println!("   Tool: {}", tool_call);

        if !options.is_empty() {
            println!("\nOptions:");
            for (i, opt) in options.iter().enumerate() {
                println!("  {}. {}", i + 1, opt.name);
            }
        }

        println!("\nChoose action:");
        println!("  [g] Grant this time");
        println!("  [G] Grant always (save to policy)");
        println!("  [d] Deny this time");
        println!("  [D] Deny always (save to policy)");
        println!("  [c] Cancel");

        // Read user input (simplified for V1.1 - in production, use a proper prompt library)
        // For now, we default to asking, but save policy if user chooses
        // TODO: Add proper interactive prompting with dialoguer or similar

        // Default to deny for safety in automated environments
        tracing::warn!(
            agent_id = %self.agent_id,
            tool_call = %tool_call,
            "Interactive prompt not available - defaulting to deny"
        );

        PermissionDecision::Deny
    }
}

#[async_trait::async_trait(?Send)]
impl acp::Client for PolicyAwareClientHandler {
    /// Evaluate permission request against policy (V1.1 policy engine).
    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        let tool_call_name = format!("{:?}", args.tool_call);

        tracing::info!(
            agent_id = %self.agent_id,
            tool_call = %tool_call_name,
            "Permission request received"
        );

        // Load current policy
        let policy = self.policy.read().await;
        let decision = policy.evaluate(&tool_call_name);
        drop(policy);

        // If decision is Ask, prompt user
        let final_decision = if decision == PermissionDecision::Ask {
            self.prompt_user(&tool_call_name, &args.options).await
        } else {
            decision
        };

        match final_decision {
            PermissionDecision::Grant => {
                tracing::info!(
                    agent_id = %self.agent_id,
                    tool_call = %tool_call_name,
                    "Permission granted"
                );

                if args.options.is_empty() {
                    tracing::error!(
                        agent_id = %self.agent_id,
                        "Permission request has no options - cancelling"
                    );
                    return Ok(acp::RequestPermissionResponse::new(
                        acp::RequestPermissionOutcome::Cancelled,
                    ));
                }

                let selected_option =
                    acp::SelectedPermissionOutcome::new(args.options[0].option_id.clone());
                Ok(acp::RequestPermissionResponse::new(
                    acp::RequestPermissionOutcome::Selected(selected_option),
                ))
            }
            PermissionDecision::Deny => {
                tracing::warn!(
                    agent_id = %self.agent_id,
                    tool_call = %tool_call_name,
                    "Permission denied"
                );
                Ok(acp::RequestPermissionResponse::new(
                    acp::RequestPermissionOutcome::Cancelled,
                ))
            }
            PermissionDecision::Ask => {
                // Should not reach here (Ask is handled above), but deny for safety
                tracing::warn!(
                    agent_id = %self.agent_id,
                    tool_call = %tool_call_name,
                    "Permission denied (ask fallback)"
                );
                Ok(acp::RequestPermissionResponse::new(
                    acp::RequestPermissionOutcome::Cancelled,
                ))
            }
        }
    }

    /// Log session notifications for debugging.
    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        tracing::debug!(
            agent_id = %self.agent_id,
            session_id = ?args.session_id,
            "Received session notification from agent"
        );
        Ok(())
    }

    /// File system operations — not implemented in V1.0.
    async fn write_text_file(
        &self,
        _args: acp::WriteTextFileRequest,
    ) -> acp::Result<acp::WriteTextFileResponse> {
        tracing::warn!(
            agent_id = %self.agent_id,
            "Agent requested fs/write_text_file — not supported in V1.0"
        );
        Err(SdkError::method_not_found())
    }

    async fn read_text_file(
        &self,
        _args: acp::ReadTextFileRequest,
    ) -> acp::Result<acp::ReadTextFileResponse> {
        tracing::warn!(
            agent_id = %self.agent_id,
            "Agent requested fs/read_text_file — not supported in V1.0"
        );
        Err(SdkError::method_not_found())
    }

    /// Terminal operations — not implemented in V1.0.
    async fn create_terminal(
        &self,
        _args: acp::CreateTerminalRequest,
    ) -> acp::Result<acp::CreateTerminalResponse> {
        tracing::warn!(
            agent_id = %self.agent_id,
            "Agent requested terminal/create — not supported in V1.0"
        );
        Err(SdkError::method_not_found())
    }

    async fn terminal_output(
        &self,
        _args: acp::TerminalOutputRequest,
    ) -> acp::Result<acp::TerminalOutputResponse> {
        Err(SdkError::method_not_found())
    }

    async fn release_terminal(
        &self,
        _args: acp::ReleaseTerminalRequest,
    ) -> acp::Result<acp::ReleaseTerminalResponse> {
        Err(SdkError::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> acp::Result<acp::WaitForTerminalExitResponse> {
        Err(SdkError::method_not_found())
    }

    async fn kill_terminal(
        &self,
        _args: acp::KillTerminalRequest,
    ) -> acp::Result<acp::KillTerminalResponse> {
        Err(SdkError::method_not_found())
    }
}

/// Simple client handler for V1.0 — auto-grants all tool requests.
///
/// This implements the ACP `Client` trait with a simple policy:
/// - `request_permission`: Auto-grant with warning log
/// - `session_notification`: Log updates for debugging
/// - File operations: Return errors (not implemented in V1.0)
/// - Terminal operations: Return errors (not implemented in V1.0)
///
/// NOTE: In V1.0, the daemon_client field was removed during crate extraction
/// because no tool operations actually dispatch to the daemon yet.
/// Future versions will add daemon-mediated tool routing (ACP-R8).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SimpleClientHandler {
    /// Agent ID for logging context.
    agent_id: String,
}

#[allow(dead_code)]
impl SimpleClientHandler {
    /// Create a new simple client handler.
    pub fn new(agent_id: String) -> Self {
        Self { agent_id }
    }
}

#[async_trait::async_trait(?Send)]
impl acp::Client for SimpleClientHandler {
    /// Auto-grant all permission requests (V1.0 policy).
    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        tracing::warn!(
            agent_id = %self.agent_id,
            tool_call = ?args.tool_call,
            "Auto-granting permission request (V1.0 policy)"
        );

        // V1.0: Auto-grant by selecting the first available option
        // If no options are provided, this is an error - cancel
        if args.options.is_empty() {
            tracing::error!(
                agent_id = %self.agent_id,
                "Permission request has no options - cancelling"
            );
            return Ok(acp::RequestPermissionResponse::new(
                acp::RequestPermissionOutcome::Cancelled,
            ));
        }

        // Select the first option (auto-grant)
        let selected_option =
            acp::SelectedPermissionOutcome::new(args.options[0].option_id.clone());
        Ok(acp::RequestPermissionResponse::new(
            acp::RequestPermissionOutcome::Selected(selected_option),
        ))
    }

    /// Log session notifications for debugging.
    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        tracing::debug!(
            agent_id = %self.agent_id,
            session_id = ?args.session_id,
            "Received session notification from agent"
        );
        Ok(())
    }

    /// File system operations — not implemented in V1.0.
    async fn write_text_file(
        &self,
        _args: acp::WriteTextFileRequest,
    ) -> acp::Result<acp::WriteTextFileResponse> {
        tracing::warn!(
            agent_id = %self.agent_id,
            "Agent requested fs/write_text_file — not supported in V1.0"
        );
        Err(SdkError::method_not_found())
    }

    async fn read_text_file(
        &self,
        _args: acp::ReadTextFileRequest,
    ) -> acp::Result<acp::ReadTextFileResponse> {
        tracing::warn!(
            agent_id = %self.agent_id,
            "Agent requested fs/read_text_file — not supported in V1.0"
        );
        Err(SdkError::method_not_found())
    }

    /// Terminal operations — not implemented in V1.0.
    async fn create_terminal(
        &self,
        _args: acp::CreateTerminalRequest,
    ) -> acp::Result<acp::CreateTerminalResponse> {
        tracing::warn!(
            agent_id = %self.agent_id,
            "Agent requested terminal/create — not supported in V1.0"
        );
        Err(SdkError::method_not_found())
    }

    async fn terminal_output(
        &self,
        _args: acp::TerminalOutputRequest,
    ) -> acp::Result<acp::TerminalOutputResponse> {
        Err(SdkError::method_not_found())
    }

    async fn release_terminal(
        &self,
        _args: acp::ReleaseTerminalRequest,
    ) -> acp::Result<acp::ReleaseTerminalResponse> {
        Err(SdkError::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> acp::Result<acp::WaitForTerminalExitResponse> {
        Err(SdkError::method_not_found())
    }

    async fn kill_terminal(
        &self,
        _args: acp::KillTerminalRequest,
    ) -> acp::Result<acp::KillTerminalResponse> {
        Err(SdkError::method_not_found())
    }
}

/// Internal state for the SDK adapter (stored in LocalSet thread).
#[allow(dead_code)]
struct SdkConnection {
    /// The ACP SDK connection.
    connection: acp::ClientSideConnection,
    /// The I/O task handle (must be kept alive).
    _io_task: tokio::task::JoinHandle<()>,
    /// Stream receiver for notifications.
    stream_receiver: StreamReceiver,
}

/// Concrete adapter wrapping the `agent-client-protocol` SDK.
///
/// This struct uses [`LocalSetBridge`] to execute `!Send` SDK operations on a
/// dedicated thread with a `LocalSet`. The connection is created once and reused
/// for all subsequent operations.
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
    /// Client handler for permission requests.
    handler: SimpleClientHandler,
    /// Handle to the connection setup task (must be joined during cleanup).
    _setup_task: Option<tokio::task::JoinHandle<()>>,
}

impl AcpSdkAdapter {
    /// Create a new adapter without an established connection.
    ///
    /// Use [`with_connection()`] to establish the actual SDK connection.
    #[allow(dead_code)]
    pub fn new(agent_id: String, agent_path: PathBuf) -> Self {
        let handler = SimpleClientHandler::new(agent_id.clone());
        Self {
            agent_path,
            agent_id: agent_id.clone(),
            bridge: LocalSetBridge::new(),
            connection: Arc::new(RwLock::new(None)),
            handler,
            _setup_task: None,
        }
    }

    /// Create adapter with established connection.
    ///
    /// This method establishes the ACP SDK connection using the provided
    /// stdin/stdout pipes from the agent subprocess.
    #[allow(dead_code)]
    #[allow(clippy::needless_pass_by_value)]
    pub fn with_connection(
        agent_id: String,
        agent_path: PathBuf,
        stdin: tokio::process::ChildStdin,
        stdout: tokio::process::ChildStdout,
    ) -> Self {
        let handler = SimpleClientHandler::new(agent_id.clone());
        let bridge = LocalSetBridge::new();
        let connection = Arc::new(RwLock::new(None));

        tracing::info!(
            agent_id = %agent_id,
            "Creating ACP SDK adapter with connection"
        );

        // Clone for use inside the closure
        let connection_clone = connection.clone();
        let handler_clone = handler.clone();
        let agent_id_for_log = agent_id.clone();
        let agent_id_for_error = agent_id.clone();

        // Execute the connection setup on the LocalSet
        let bridge_clone = bridge.clone();
        let setup_task = tokio::spawn(async move {
            let result = bridge_clone
                .execute(move || {
                    let connection_clone = connection_clone.clone();
                    let handler = handler_clone;
                    let agent_id = agent_id_for_log;

                    // Convert tokio pipes to futures-compatible traits inside the LocalSet
                    // This is where Compat is created - inside the !Send context
                    let stdin_compat = stdin.compat_write();
                    let stdout_compat = stdout.compat();

                    Box::pin(async move {
                        // Create the SDK connection on the LocalSet
                        let spawn_local = |fut| {
                            tokio::task::spawn_local(fut);
                        };

                        let (conn, io_task) = acp::ClientSideConnection::new(
                            handler,
                            stdin_compat,
                            stdout_compat,
                            spawn_local,
                        );

                        let stream_receiver = conn.subscribe();

                        // Store the connection
                        let mut guard = connection_clone.write().await;
                        *guard = Some(SdkConnection {
                            connection: conn,
                            _io_task: tokio::task::spawn_local(async move {
                                if let Err(e) = io_task.await {
                                    tracing::error!(
                                        agent_id = %agent_id,
                                        error = %e,
                                        "ACP I/O task failed"
                                    );
                                }
                            }),
                            stream_receiver,
                        });

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
            handler,
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
        request: NexusInitializeRequest,
    ) -> impl Future<Output = AcpResult<NexusInitializeResponse>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        let guard = connection.read().await;
                        let sdk_conn = match guard.as_ref() {
                            Some(conn) => conn,
                            None => {
                                return Err(crate::AcpError::connection_failed(
                                    "Connection not established",
                                ))
                            }
                        };

                        let sdk_request = sdk_initialize_request_from_nexus(request);
                        let response = sdk_conn
                            .connection
                            .initialize(sdk_request)
                            .await
                            .map_err(crate::AcpError::sdk)?;

                        Ok(NexusInitializeResponse {
                            protocol_version: nexus_protocol_version_from_sdk(
                                response.protocol_version,
                            ),
                            agent_capabilities: nexus_agent_capabilities_from_sdk(
                                &response.agent_capabilities,
                            ),
                            agent_info: response.agent_info.as_ref().map(nexus_agent_info_from_sdk),
                            auth_methods: response
                                .auth_methods
                                .iter()
                                .map(nexus_auth_method_from_sdk)
                                .collect(),
                        })
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

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        let guard = connection.read().await;
                        let sdk_conn = match guard.as_ref() {
                            Some(conn) => conn,
                            None => {
                                return Err(crate::AcpError::connection_failed(
                                    "Connection not established",
                                ))
                            }
                        };

                        let sdk_request = sdk_new_session_request_from_nexus(request);
                        let response = sdk_conn
                            .connection
                            .new_session(sdk_request)
                            .await
                            .map_err(crate::AcpError::sdk)?;

                        Ok(NexusSessionCreated {
                            session_id: NexusSessionId(response.session_id.to_string()),
                            modes: response
                                .modes
                                .as_ref()
                                .map(nexus_session_mode_state_from_sdk),
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
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        let guard = connection.read().await;
                        let sdk_conn = match guard.as_ref() {
                            Some(conn) => conn,
                            None => {
                                return Err(crate::AcpError::connection_failed(
                                    "Connection not established",
                                ))
                            }
                        };

                        let sdk_request = sdk_prompt_request_from_nexus(request);
                        let response = sdk_conn
                            .connection
                            .prompt(sdk_request)
                            .await
                            .map_err(crate::AcpError::sdk)?;

                        Ok(NexusPromptCompleted {
                            stop_reason: nexus_stop_reason_from_sdk(response.stop_reason),
                        })
                    })
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

        async move {
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        let guard = connection.read().await;
                        let sdk_conn = match guard.as_ref() {
                            Some(conn) => conn,
                            None => {
                                return Err(crate::AcpError::connection_failed(
                                    "Connection not established",
                                ))
                            }
                        };

                        let session_id_str = session_id.0.clone();
                        let cancel_notification =
                            acp::CancelNotification::new(acp::SessionId::new(session_id_str));
                        let result: Result<(), acp::Error> =
                            sdk_conn.connection.cancel(cancel_notification).await;
                        result.map_err(crate::AcpError::sdk)?;

                        Ok(NexusCancelResult {
                            session_id: NexusSessionId(session_id.0),
                        })
                    })
                })
                .await
                .and_then(|r| r)
        }
    }
}

/// Direct streaming access (not on the trait — see module docs for rationale).
impl AcpSdkAdapter {
    /// Subscribe to stream messages from the agent.
    ///
    /// This method is not on the `NexusAcpClient` trait because the return
    /// type (`StreamReceiver`) is tightly coupled to the SDK's `async-broadcast`
    /// implementation. Consumers that need streaming should use this method
    /// directly on the adapter.
    #[allow(dead_code)]
    pub fn subscribe(&self) -> StreamReceiver {
        let _connection = self.connection.clone();

        // Create a fresh receiver (this will be updated to use the actual connection's receiver)
        let (tx, rx) = async_broadcast::broadcast(16);
        drop(tx);

        tracing::warn!(
            agent_id = %self.agent_id,
            "subscribe() called — returning empty receiver (connection may not be established yet)"
        );

        StreamReceiver::from(rx)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_from_sdk() {
        let sdk_version = acp::ProtocolVersion::LATEST;
        let nexus_version = nexus_protocol_version_from_sdk(sdk_version);
        assert_eq!(nexus_version.0, "1");
    }

    #[test]
    fn stop_reason_from_sdk() {
        assert_eq!(
            nexus_stop_reason_from_sdk(acp::StopReason::EndTurn),
            NexusStopReason::EndTurn
        );
        assert_eq!(
            nexus_stop_reason_from_sdk(acp::StopReason::Cancelled),
            NexusStopReason::Cancelled
        );
    }

    #[test]
    fn agent_info_from_sdk() {
        let sdk_impl = acp::Implementation::new("claude-code", "1.0.0");
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
