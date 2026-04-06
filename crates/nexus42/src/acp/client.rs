//! ACP Client adapter trait and SDK wrapper.
//!
//! This module defines [`NexusAcpClient`] — the abstraction layer through which
//! all nexus42 CLI code interacts with ACP agents. The concrete implementation
//! ([`AcpSdkAdapter`]) wraps the `agent-client-protocol` SDK's
//! `ClientSideConnection`, isolating the `!Send` future constraint behind
//! `tokio::task::LocalSet` + `spawn_local`.
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
//! # V1.0 Client Handler
//!
//! The [`SimpleClientHandler`] implements the ACP `Client` trait with a
//! permissive auto-grant policy for V1.0:
//! - All `request_permission` requests are auto-granted with a warning log
//! - File system operations return errors (no workspace access yet)
//! - Terminal operations return errors (deferred to V1.1+)
//! - Session notifications are logged for debugging

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_client_protocol as acp;
use agent_client_protocol::{ClientSideConnection, Error, StreamReceiver};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::RwLock;

use crate::acp::error::AcpResult;

// Re-export commonly used SDK types for convenience.
#[allow(unused_imports)]
pub use acp::{
    AgentCapabilities, CancelNotification, Client, ClientCapabilities, ContentBlock,
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
///
/// # `!Send` Isolation
///
/// The underlying SDK produces `!Send` futures, which require
/// `tokio::task::LocalSet`. This trait exposes **`Send`** futures so callers
/// don't need to worry about runtime constraints. The concrete adapter
/// ([`AcpSdkAdapter`]) internally bridges the gap.
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

/// Concrete adapter wrapping the `agent-client-protocol` SDK.
///
/// This struct owns the `ClientSideConnection` wrapped in `RwLock` to allow
/// interior mutability. The SDK's `ClientSideConnection` is `!Send`, so it
/// must be used within a `LocalSet`. We achieve this by running the LocalSet
/// in a dedicated thread and using message passing for SDK calls.
///
/// # Architecture Note
///
/// For Task 4, this implementation provides the structure but defers the full
/// LocalSet thread integration to keep the codebase buildable. The trait methods
/// currently return placeholder responses. Full integration requires:
/// 1. Spawning a dedicated OS thread for the LocalSet
/// 2. Using channels to send requests and receive responses
/// 3. Managing the lifetime of the LocalSet thread
///
/// This is tracked as a follow-up refinement.
#[allow(dead_code)]
pub struct AcpSdkAdapter {
    /// The agent's resolved binary path or command string (for error messages).
    agent_path: PathBuf,
    /// Agent ID for logging context.
    agent_id: String,
    /// The ACP SDK connection (wrapped for interior mutability).
    connection: Arc<RwLock<Option<ClientSideConnection>>>,
}

#[allow(dead_code)]
impl AcpSdkAdapter {
    /// Create a new adapter with placeholder connection.
    ///
    /// For Task 4, this creates the adapter structure without establishing
    /// the actual SDK connection. The full LocalSet-based integration will
    /// be added in a follow-up refinement to handle the `!Send` constraint.
    pub fn new(agent_id: String, agent_path: PathBuf) -> Self {
        Self {
            agent_path,
            agent_id,
            connection: Arc::new(RwLock::new(None)),
        }
    }

    /// Create adapter with established connection (for future use).
    ///
    /// This method will be used when the full LocalSet thread integration
    /// is implemented. Currently marked as future work.
    #[allow(dead_code)]
    pub fn with_connection(
        agent_id: String,
        agent_path: PathBuf,
        _stdin: impl AsyncWrite + Unpin + 'static,
        _stdout: impl AsyncRead + Unpin + 'static,
    ) -> Self {
        tracing::warn!(
            agent_id = %agent_id,
            "AcpSdkAdapter::with_connection() called — full LocalSet integration pending"
        );

        // TODO: Implement LocalSet-based thread for !Send futures
        // For now, return placeholder adapter
        Self::new(agent_id, agent_path)
    }

    /// Return a reference to the agent path (for error reporting).
    pub fn agent_path(&self) -> &Path {
        &self.agent_path
    }

    /// Return the agent ID.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
}

impl NexusAcpClient for AcpSdkAdapter {
    fn initialize(
        &self,
        _request: InitializeRequest,
    ) -> impl Future<Output = AcpResult<InitializedSession>> + Send {
        let agent_id = self.agent_id.clone();

        async move {
            tracing::warn!(
                agent_id = %agent_id,
                "AcpSdkAdapter::initialize() — full SDK integration pending LocalSet thread implementation"
            );

            // Placeholder response for Task 4
            // TODO: Implement LocalSet thread + channel-based SDK calls
            Ok(InitializedSession {
                protocol_version: ProtocolVersion::LATEST,
                agent_capabilities: AgentCapabilities::default(),
                agent_info: None,
                auth_methods: Vec::new(),
            })
        }
    }

    fn create_session(
        &self,
        _request: NewSessionRequest,
    ) -> impl Future<Output = AcpResult<SessionCreated>> + Send {
        let agent_id = self.agent_id.clone();

        async move {
            tracing::warn!(
                agent_id = %agent_id,
                "AcpSdkAdapter::create_session() — full SDK integration pending"
            );

            Ok(SessionCreated {
                session_id: SessionId::new("placeholder-session-id"),
                modes: None,
            })
        }
    }

    fn prompt(
        &self,
        _request: PromptRequest,
    ) -> impl Future<Output = AcpResult<PromptCompleted>> + Send {
        let agent_id = self.agent_id.clone();

        async move {
            tracing::warn!(
                agent_id = %agent_id,
                "AcpSdkAdapter::prompt() — full SDK integration pending"
            );

            Ok(PromptCompleted {
                stop_reason: StopReason::EndTurn,
            })
        }
    }

    fn cancel(&self, _session_id: SessionId) -> impl Future<Output = AcpResult<()>> + Send {
        let agent_id = self.agent_id.clone();

        async move {
            tracing::warn!(
                agent_id = %agent_id,
                "AcpSdkAdapter::cancel() — full SDK integration pending"
            );

            Ok(())
        }
    }

    fn subscribe(&self) -> StreamReceiver {
        // TODO: Implement actual stream subscription when full LocalSet integration is ready
        tracing::warn!(
            agent_id = %self.agent_id,
            "subscribe() called — stream receiver not fully implemented in placeholder"
        );

        // For the placeholder, we can't easily create a StreamReceiver without
        // an actual connection. This is a known limitation.
        // When the full LocalSet integration is implemented, this will be
        // connection.subscribe()
        unimplemented!("StreamReceiver requires active connection — pending LocalSet integration")
    }
}

#[cfg(test)]
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
    async fn adapter_placeholder_initialize() {
        let adapter = AcpSdkAdapter::new(
            "test-agent".to_string(),
            PathBuf::from("/usr/bin/test-agent"),
        );

        let request = InitializeRequest::new(ProtocolVersion::LATEST);

        let result: AcpResult<InitializedSession> = adapter.initialize(request).await;
        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.protocol_version, ProtocolVersion::LATEST);
    }
}
