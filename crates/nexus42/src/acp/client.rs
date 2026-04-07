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
use agent_client_protocol::{Error, StreamReceiver};
use tokio::sync::RwLock;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::acp::error::AcpResult;
use crate::acp::localset_bridge::LocalSetBridge;
use crate::acp::policy::{PermissionDecision, PermissionPolicy};

// Re-export commonly used SDK types for convenience.
#[allow(unused_imports)]
pub use acp::{
    Agent, AgentCapabilities, CancelNotification, Client, ClientCapabilities, ContentBlock,
    Implementation, InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse,
    PromptRequest, PromptResponse, SessionId, StopReason,
};

/// Protocol version constant — re-exported from the SDK's schema crate.
pub use acp::ProtocolVersion;

/// Result of an ACP initialization handshake.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct InitializedSession {
    /// Protocol version agreed upon.
    pub protocol_version: ProtocolVersion,
    /// Capabilities reported by the agent.
    pub agent_capabilities: AgentCapabilities,
    /// Metadata about the agent (name, version, etc.).
    pub agent_info: Option<Implementation>,
    /// Authentication methods required by the agent (empty if none).
    pub auth_methods: Vec<acp::AuthMethod>,
}

impl InitializedSession {
    /// Convert from SDK's InitializeResponse.
    #[allow(dead_code)]
    pub fn from_sdk_response(response: InitializeResponse) -> Self {
        Self {
            protocol_version: response.protocol_version,
            agent_capabilities: response.agent_capabilities,
            agent_info: response.agent_info,
            auth_methods: response.auth_methods,
        }
    }
}

/// Result of creating a new ACP session.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SessionCreated {
    /// The unique session identifier.
    pub session_id: SessionId,
    /// Available interaction modes (if the agent reports them).
    pub modes: Option<acp::SessionModeState>,
}

impl SessionCreated {
    /// Convert from SDK's NewSessionResponse.
    #[allow(dead_code)]
    pub fn from_sdk_response(response: NewSessionResponse) -> Self {
        Self {
            session_id: response.session_id,
            modes: response.modes,
        }
    }
}

/// Result of prompting an agent.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PromptCompleted {
    /// Why the agent stopped generating.
    pub stop_reason: StopReason,
}

impl PromptCompleted {
    /// Convert from SDK's PromptResponse.
    #[allow(dead_code)]
    pub fn from_sdk_response(response: PromptResponse) -> Self {
        Self {
            stop_reason: response.stop_reason,
        }
    }
}

/// The nexus42 ACP client abstraction.
///
/// All ACP communication from the CLI goes through this trait. The methods
/// mirror the ACP protocol lifecycle: initialize → session → prompt → cancel.
#[allow(async_fn_in_trait)]
#[allow(dead_code)]
pub trait NexusAcpClient: Send + Sync {
    /// Perform the ACP `initialize` handshake with the agent.
    fn initialize(
        &self,
        request: InitializeRequest,
    ) -> impl Future<Output = AcpResult<InitializedSession>> + Send;

    /// Create a new ACP session on the agent.
    fn create_session(
        &self,
        request: NewSessionRequest,
    ) -> impl Future<Output = AcpResult<SessionCreated>> + Send;

    /// Send a prompt to the agent within an existing session.
    fn prompt(
        &self,
        request: PromptRequest,
    ) -> impl Future<Output = AcpResult<PromptCompleted>> + Send;

    /// Cancel an in-progress prompt operation.
    fn cancel(&self, session_id: SessionId) -> impl Future<Output = AcpResult<()>> + Send;

    /// Subscribe to stream messages from the agent.
    fn subscribe(&self) -> StreamReceiver;
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
        Err(Error::method_not_found())
    }

    async fn read_text_file(
        &self,
        _args: acp::ReadTextFileRequest,
    ) -> acp::Result<acp::ReadTextFileResponse> {
        tracing::warn!(
            agent_id = %self.agent_id,
            "Agent requested fs/read_text_file — not supported in V1.0"
        );
        Err(Error::method_not_found())
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
        Err(Error::method_not_found())
    }

    async fn terminal_output(
        &self,
        _args: acp::TerminalOutputRequest,
    ) -> acp::Result<acp::TerminalOutputResponse> {
        Err(Error::method_not_found())
    }

    async fn release_terminal(
        &self,
        _args: acp::ReleaseTerminalRequest,
    ) -> acp::Result<acp::ReleaseTerminalResponse> {
        Err(Error::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> acp::Result<acp::WaitForTerminalExitResponse> {
        Err(Error::method_not_found())
    }

    async fn kill_terminal(
        &self,
        _args: acp::KillTerminalRequest,
    ) -> acp::Result<acp::KillTerminalResponse> {
        Err(Error::method_not_found())
    }
}

/// Simple client handler for V1.0 — auto-grants all permissions.
///
/// This implements the ACP `Client` trait with a permissive policy:
/// - `request_permission`: Auto-grant with warning log
/// - `session_notification`: Log updates for debugging
/// - File/terminal operations: Return errors (not implemented in V1.0)
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
        Err(Error::method_not_found())
    }

    async fn read_text_file(
        &self,
        _args: acp::ReadTextFileRequest,
    ) -> acp::Result<acp::ReadTextFileResponse> {
        tracing::warn!(
            agent_id = %self.agent_id,
            "Agent requested fs/read_text_file — not supported in V1.0"
        );
        Err(Error::method_not_found())
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
        Err(Error::method_not_found())
    }

    async fn terminal_output(
        &self,
        _args: acp::TerminalOutputRequest,
    ) -> acp::Result<acp::TerminalOutputResponse> {
        Err(Error::method_not_found())
    }

    async fn release_terminal(
        &self,
        _args: acp::ReleaseTerminalRequest,
    ) -> acp::Result<acp::ReleaseTerminalResponse> {
        Err(Error::method_not_found())
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> acp::Result<acp::WaitForTerminalExitResponse> {
        Err(Error::method_not_found())
    }

    async fn kill_terminal(
        &self,
        _args: acp::KillTerminalRequest,
    ) -> acp::Result<acp::KillTerminalResponse> {
        Err(Error::method_not_found())
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

                        Ok::<(), crate::acp::AcpError>(())
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
        request: InitializeRequest,
    ) -> impl Future<Output = AcpResult<InitializedSession>> + Send {
        let connection = self.connection.clone();
        let bridge = self.bridge.clone();

        async move {
            // execute returns AcpResult<Result<T, AcpError>>
            // Flatten with and_then
            bridge
                .execute(move || {
                    let connection = connection.clone();

                    Box::pin(async move {
                        let guard = connection.read().await;
                        let sdk_conn = match guard.as_ref() {
                            Some(conn) => conn,
                            None => {
                                return Err(crate::acp::AcpError::connection_failed(
                                    "Connection not established",
                                ))
                            }
                        };

                        let response = sdk_conn
                            .connection
                            .initialize(request)
                            .await
                            .map_err(crate::acp::AcpError::sdk)?;

                        Ok(InitializedSession::from_sdk_response(response))
                    })
                })
                .await
                .and_then(|r| r)
        }
    }

    fn create_session(
        &self,
        request: NewSessionRequest,
    ) -> impl Future<Output = AcpResult<SessionCreated>> + Send {
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
                                return Err(crate::acp::AcpError::connection_failed(
                                    "Connection not established",
                                ))
                            }
                        };

                        let response = sdk_conn
                            .connection
                            .new_session(request)
                            .await
                            .map_err(crate::acp::AcpError::sdk)?;

                        Ok(SessionCreated::from_sdk_response(response))
                    })
                })
                .await
                .and_then(|r| r)
        }
    }

    fn prompt(
        &self,
        request: PromptRequest,
    ) -> impl Future<Output = AcpResult<PromptCompleted>> + Send {
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
                                return Err(crate::acp::AcpError::connection_failed(
                                    "Connection not established",
                                ))
                            }
                        };

                        let response = sdk_conn
                            .connection
                            .prompt(request)
                            .await
                            .map_err(crate::acp::AcpError::sdk)?;

                        Ok(PromptCompleted::from_sdk_response(response))
                    })
                })
                .await
                .and_then(|r| r)
        }
    }

    fn cancel(&self, session_id: SessionId) -> impl Future<Output = AcpResult<()>> + Send {
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
                                return Err(crate::acp::AcpError::connection_failed(
                                    "Connection not established",
                                ))
                            }
                        };

                        let cancel_notification = CancelNotification::new(session_id);
                        let result: Result<(), acp::Error> =
                            sdk_conn.connection.cancel(cancel_notification).await;
                        result.map_err(crate::acp::AcpError::sdk)?;

                        Ok(())
                    })
                })
                .await
                .and_then(|r| r)
        }
    }

    fn subscribe(&self) -> StreamReceiver {
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
    fn initialized_session_conversion() {
        let response = InitializeResponse::new(ProtocolVersion::LATEST);

        let session = InitializedSession::from_sdk_response(response);
        assert_eq!(session.protocol_version, ProtocolVersion::LATEST);
        assert!(session.agent_info.is_none());
    }

    #[test]
    fn session_created_conversion() {
        let session_id = SessionId::new("test-session");
        let response = NewSessionResponse::new(session_id.clone());

        let created = SessionCreated::from_sdk_response(response);
        assert_eq!(created.session_id, session_id);
        assert!(created.modes.is_none());
    }

    #[test]
    fn prompt_completed_conversion() {
        let response = PromptResponse::new(StopReason::EndTurn);

        let completed = PromptCompleted::from_sdk_response(response);
        assert_eq!(completed.stop_reason, StopReason::EndTurn);
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

        let request = InitializeRequest::new(ProtocolVersion::LATEST);

        let result = adapter.initialize(request).await;
        // Since the closure returns Result<T, AcpError> and execute wraps it,
        // we get AcpResult<AcpResult<InitializedSession>>
        // The ? will flatten outer errors, inner result contains connection error
        assert!(result.is_err());
    }
}
