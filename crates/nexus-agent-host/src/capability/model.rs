//! Capability model: host operations, events, descriptors, and DTOs.
//!
//! Defines the normalized host operation and event types used across all provider
//! adapters, the capability descriptor for negotiation, and supporting DTOs.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::HostResult;
use crate::ids::{HostOperationId, HostSessionId, ProviderId};

/// A stream of host events from an operation.
pub type HostEventStream =
    std::pin::Pin<Box<dyn futures_util::Stream<Item = HostResult<HostEvent>> + Send + 'static>>;

/// Protocol kind for a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolKind {
    /// ACP protocol provider (via `nexus-acp-host`).
    Acp,
    /// Native CLI provider (subprocess, limited capabilities).
    NativeCli,
}

/// Narrowing permission scope for a Host prompt operation (A1).
///
/// Intersected with Host configuration per operation; a provider unable to
/// honor a requested scope refuses `not_supported` rather than silently
/// discarding the policy. `None` preserves the standalone Host/Character
/// policy and never means unrestricted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptPermissionScope {
    /// Read-only tools allowed.
    pub allow_read: bool,
    /// Write tools allowed.
    pub allow_write: bool,
    /// Destructive tools allowed.
    pub allow_destructive: bool,
}

/// Host operation — only execution-scoped variants.
///
/// Cancel flows through `HostFacade::cancel()` / `ProviderAdapter::cancel()`.
/// Health is a separate `HostFacade::health()` query.
/// See PM Review Note R-002.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostOperation {
    /// Send a prompt with content blocks.
    Prompt {
        /// Unique operation ID for tracking.
        op_id: HostOperationId,
        /// Content blocks comprising the prompt.
        content: Vec<HostContentBlock>,
        /// Narrowing permission scope (A1). `None` preserves the standalone
        /// Host/Character policy; a provider unable to honor a requested
        /// scope refuses `not_supported`.
        permission_scope: Option<PromptPermissionScope>,
    },
    /// Switch the model for the current session.
    SetModel {
        /// Model identifier.
        model: String,
    },
    /// Switch the mode for the current session.
    SetMode {
        /// Mode identifier.
        mode: String,
    },
}

/// Content block for host prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostContentBlock {
    /// Plain text content.
    Text {
        /// The text content.
        text: String,
    },
    /// Resource link content.
    ResourceLink {
        /// Resource name.
        name: Option<String>,
        /// Resource URI.
        uri: String,
    },
}

/// Host event — terminal and intermediate events from provider execution.
///
/// Every operation emits exactly one terminal event (`OpFinished` or `OpFailed`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostEvent {
    /// Session successfully created.
    SessionCreated(SessionCreatedEvent),
    /// Operation started executing.
    OpStarted(OperationStartedEvent),
    /// Thinking/reasoning delta.
    ThoughtDelta(TextDeltaEvent),
    /// Message text delta.
    MessageDelta(TextDeltaEvent),
    /// Tool call initiated.
    ToolCall(ToolCallEvent),
    /// Tool call status update.
    ToolCallUpdate(ToolCallUpdateEvent),
    /// Plan/status update.
    PlanUpdate(PlanUpdateEvent),
    /// Status message (warning, info).
    Status(StatusEvent),
    /// Operation completed successfully.
    OpFinished(OperationFinishedEvent),
    /// Operation failed.
    OpFailed(OperationFailedEvent),
    /// Session stopped (graceful or error).
    SessionStopped(SessionStoppedEvent),
}

/// Session created event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreatedEvent {
    /// The session ID.
    pub session_id: HostSessionId,
    /// The provider ID.
    pub provider_id: ProviderId,
}

/// Operation started event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStartedEvent {
    /// The operation ID.
    pub op_id: HostOperationId,
    /// The session ID.
    pub session_id: HostSessionId,
}

/// Text delta event (used for both thought and message deltas).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDeltaEvent {
    /// The session ID.
    pub session_id: HostSessionId,
    /// The operation ID.
    pub op_id: HostOperationId,
    /// Delta text content.
    pub text: String,
}

/// Tool call event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallEvent {
    /// The session ID.
    pub session_id: HostSessionId,
    /// The operation ID.
    pub op_id: HostOperationId,
    /// Tool call ID (provider-assigned).
    pub tool_call_id: String,
    /// Tool name.
    pub tool_name: String,
}

/// Tool call update event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallUpdateEvent {
    /// The session ID.
    pub session_id: HostSessionId,
    /// The operation ID.
    pub op_id: HostOperationId,
    /// Tool call ID.
    pub tool_call_id: String,
    /// Update content (partial result or status).
    pub content: String,
}

/// Plan update event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanUpdateEvent {
    /// The session ID.
    pub session_id: HostSessionId,
    /// The operation ID.
    pub op_id: HostOperationId,
    /// Plan content (structured or free text).
    pub content: String,
}

/// Status event payload (warnings, info, non-error messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEvent {
    /// The session ID (if applicable).
    pub session_id: Option<HostSessionId>,
    /// Status level.
    pub level: StatusLevel,
    /// Status message.
    pub message: String,
}

/// Status level for status events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusLevel {
    /// Informational status.
    Info,
    /// Warning status.
    Warning,
    /// Error status (non-terminal).
    Error,
}

/// Operation finished event payload (terminal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationFinishedEvent {
    /// The session ID.
    pub session_id: HostSessionId,
    /// The operation ID.
    pub op_id: HostOperationId,
    /// Finish reason.
    pub reason: FinishReason,
}

/// Reason for operation completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Normal end-of-turn completion.
    EndTurn,
    /// Hit token limit.
    MaxTokens,
    /// Hit turn request limit.
    MaxTurnRequests,
    /// Agent refused the request.
    Refusal,
    /// The turn was cancelled by the client (I-004). Typed non-success:
    /// never mapped to `EndTurn`/succeeded for workflow or Character
    /// consumers.
    Cancelled,
}

/// Operation failed event payload (terminal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationFailedEvent {
    /// The session ID.
    pub session_id: HostSessionId,
    /// The operation ID.
    pub op_id: HostOperationId,
    /// Error category.
    pub error_category: String,
    /// Error message.
    pub error_message: String,
}

/// Session stopped event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStoppedEvent {
    /// The session ID.
    pub session_id: HostSessionId,
    /// Stop reason.
    pub reason: SessionStopReason,
}

/// Reason for session stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStopReason {
    /// Graceful shutdown requested.
    GracefulShutdown,
    /// Provider process exited.
    ProviderExit,
    /// Error caused session termination.
    Error,
    /// Session was cancelled.
    Cancelled,
}

/// Configuration for starting the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostStartConfig {
    /// Path to the config file.
    pub config_path: PathBuf,
    /// Workspace root for the host.
    pub workspace_root: PathBuf,
    /// Maximum concurrent sessions.
    pub max_sessions: usize,
    /// Maximum concurrent operations per session.
    pub max_ops_per_session: usize,
    /// Timeout configuration.
    pub timeouts: crate::config::TimeoutConfig,
    /// Pre-validated config retained from boot (avoids a second disk read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_config: Option<crate::config::AgentHostConfig>,
    /// Verified owner for bounded readiness probes at start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_owner: Option<SessionOwner>,
}

/// Verified owner metadata bound to a Host session.
///
/// Constructed by Host/Character admission from existing verified scope
/// (never from request-body assertions). The canonical workspace cwd is
/// validated against this owner before any subprocess spawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOwner {
    /// The verified Creator ID that owns this session.
    pub creator_id: String,
    /// Canonical workspace root of the Creator (absolute path).
    pub workspace_root: PathBuf,
    /// Optional orchestration run ID (set by the trusted orchestration
    /// adapter; `None` for standalone Character/public Host sessions).
    pub orchestration_run_id: Option<String>,
}

/// Request to create a new managed session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    /// Provider to use for this session.
    pub provider_id: ProviderId,
    /// Working directory for the session.
    pub cwd: PathBuf,
    /// Optional model override.
    pub model: Option<String>,
    /// Optional mode override.
    pub mode: Option<String>,
    /// MCP server configurations for the session.
    pub mcp_servers: Vec<McpServerConfig>,
    /// Additional metadata (opaque to host).
    pub metadata: serde_json::Value,
    /// Verified owner metadata (Creator + canonical workspace).
    pub owner: SessionOwner,
}

/// MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpServerConfig {
    /// HTTP-based MCP server.
    Http {
        /// Server name.
        name: String,
        /// Server URL.
        url: String,
    },
    /// SSE-based MCP server.
    Sse {
        /// Server name.
        name: String,
        /// Server URL.
        url: String,
    },
    /// Stdio-based MCP server.
    Stdio {
        /// Server name.
        name: String,
        /// Command to execute.
        command: PathBuf,
        /// Command arguments.
        args: Vec<String>,
        /// Environment variables.
        env: std::collections::HashMap<String, String>,
    },
}

/// Descriptor for a provider's static capabilities.
///
/// This struct intentionally uses individual bool fields rather than bitflags
/// to match the capability negotiation protocol (compass §Capability negotiation)
/// and maintain serde compatibility. Each field maps to a named ACP capability.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    /// Supports text prompt input.
    pub text_prompt: bool,
    /// Supports streaming events.
    pub streaming: bool,
    /// Supports operation cancellation.
    pub cancellation: bool,
    /// Supports session restore (load).
    pub session_restore: bool,
    /// Supports structured tool call events.
    pub structured_tool_calls: bool,
    /// Supports HTTP MCP transport.
    pub mcp_http: bool,
    /// Supports SSE MCP transport.
    pub mcp_sse: bool,
    /// Supports stdio MCP transport.
    pub mcp_stdio: bool,
    /// Supports image input.
    pub images: bool,
    /// Supports audio input.
    pub audio: bool,
    /// Supports embedded context injection.
    pub embedded_context: bool,
    /// Supports model switching.
    pub set_model: bool,
    /// Supports mode switching.
    pub set_mode: bool,
    /// Supports diagnostic/reporting events.
    pub diagnostics: bool,
}

impl CapabilityDescriptor {
    /// ACP full capability descriptor.
    ///
    /// Claims `set_mode = true` because the ACP protocol provides a stable
    /// `session/set_mode` RPC. Sets `set_model = false` because model
    /// switching depends on dynamic discovery of agent-specific config
    /// options (not guaranteed). If a model config option is discovered at
    /// runtime, `SetModel` will succeed; otherwise it returns
    /// `CapabilityUnsupported`.
    #[must_use]
    pub const fn acp_full() -> Self {
        Self {
            text_prompt: true,
            streaming: true,
            cancellation: true,
            session_restore: true,
            structured_tool_calls: true,
            mcp_http: true,
            mcp_sse: true,
            mcp_stdio: true,
            images: true,
            audio: true,
            embedded_context: true,
            set_model: false,
            set_mode: true,
            diagnostics: true,
        }
    }

    /// Native CLI capability descriptor with multi-turn session restore.
    ///
    /// Supports `session_restore = true` because the Claude CLI provides
    /// `--session-id` / `--resume` flags for conversation continuity across
    /// process invocations.
    #[must_use]
    pub const fn native_cli_limited() -> Self {
        Self {
            text_prompt: true,
            streaming: true,
            cancellation: true,
            session_restore: true,
            structured_tool_calls: false,
            mcp_http: false,
            mcp_sse: false,
            mcp_stdio: false,
            images: false,
            audio: false,
            embedded_context: false,
            set_model: false,
            set_mode: false,
            diagnostics: false,
        }
    }

    /// Native CLI descriptor for the `DeepSeek Harness` runtime
    /// (`dsh-native`) — the documented narrower descriptor (locks § AR-6).
    ///
    /// `streaming` is `true` after v1.188 P1: root `assistant/message`
    /// notifications are mapped into `MessageDelta` while `Session::run`
    /// is in flight (message-level units, not token/chunk streaming).
    /// `cancellation` stays honest `false` — the SDK has no cancel/session-close
    /// RPC (`cancel()` is a documented no-op).
    ///
    /// `session_restore` stays true (AR-2/AR-5): `start_session(Some(id))`
    /// reuses the host-generated session id across executes, and a session
    /// id unknown to the runtime lazily creates the agent+session pair,
    /// so later executes resume. Every other field matches
    /// [`native_cli_limited`].
    #[must_use]
    pub const fn dsh_limited() -> Self {
        Self {
            text_prompt: true,
            streaming: true,
            cancellation: false,
            session_restore: true,
            structured_tool_calls: false,
            mcp_http: false,
            mcp_sse: false,
            mcp_stdio: false,
            images: false,
            audio: false,
            embedded_context: false,
            set_model: false,
            set_mode: false,
            diagnostics: false,
        }
    }

    /// Disabled descriptor — all capabilities off.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            text_prompt: false,
            streaming: false,
            cancellation: false,
            session_restore: false,
            structured_tool_calls: false,
            mcp_http: false,
            mcp_sse: false,
            mcp_stdio: false,
            images: false,
            audio: false,
            embedded_context: false,
            set_model: false,
            set_mode: false,
            diagnostics: false,
        }
    }
}

/// Static descriptor for a discovered provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    /// Provider ID.
    pub provider_id: ProviderId,
    /// Human-readable display name.
    pub display_name: String,
    /// Protocol kind.
    pub protocol_kind: ProtocolKind,
    /// Static capabilities.
    pub capabilities: CapabilityDescriptor,
}

/// Provider health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    /// Provider ID.
    pub provider_id: ProviderId,
    /// Whether the provider is available.
    pub available: bool,
    /// Optional latency hint (milliseconds).
    pub latency_ms: Option<u64>,
    /// Optional status message.
    pub message: Option<String>,
}

/// Probe request for checking provider availability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeRequest {
    /// Timeout in milliseconds.
    pub timeout_ms: u64,
    /// Canonical cwd for the bounded handshake probe.
    pub cwd: PathBuf,
    /// Verified session owner (Creator + workspace) for the probe child.
    pub owner: SessionOwner,
}

/// Launch specification for starting a provider session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchSpec {
    /// Working directory for the session.
    pub cwd: PathBuf,
    /// Optional model override.
    pub model: Option<String>,
    /// Optional mode override.
    pub mode: Option<String>,
    /// MCP server configurations.
    pub mcp_servers: Vec<McpServerConfig>,
    /// Verified owner metadata (Creator + canonical workspace).
    pub owner: SessionOwner,
}

/// Handle to a managed provider session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedSessionHandle {
    /// Provider ID.
    pub provider_id: ProviderId,
    /// Host session ID.
    pub session_id: HostSessionId,
    /// Negotiated capabilities for this session.
    pub capabilities: CapabilityDescriptor,
    /// Opaque owned-process identity (PID + platform birth + owned group)
    /// captured at launch (A5). `None` when the platform cannot establish a
    /// birth token; cleanup must then be reported unconfirmed. Never a
    /// transport handle — the daemon persists this fingerprint only.
    pub process_identity: Option<OwnedProcessIdentity>,
}

/// Opaque owned-process identity (A5) — PID plus process-birth and group.
///
/// Never identified by PID alone. Mirrors the durable
/// `nexus_orchestration::run_state::OwnedProcessIdentity` shape so the daemon
/// can persist the fingerprint without importing provider crates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedProcessIdentity {
    /// Process id.
    pub pid: u32,
    /// Platform process-birth identity (e.g. start time) for reuse detection.
    pub process_birth: Option<String>,
    /// Owned process-group identity for tree cleanup.
    pub group_id: Option<String>,
}

/// Host-level health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostHealth {
    /// Whether the host is running.
    pub running: bool,
    /// Number of active sessions.
    pub active_sessions: usize,
    /// Number of active operations.
    pub active_operations: usize,
}

/// Host session view (returned from `create_session`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostSession {
    /// Session ID.
    pub session_id: HostSessionId,
    /// Provider ID.
    pub provider_id: ProviderId,
    /// Negotiated capabilities.
    pub capabilities: CapabilityDescriptor,
    /// Session state.
    pub state: SessionState,
    /// Opaque owned-process identity (PID + birth + group) captured at
    /// launch (A5). `None` when the platform cannot establish a birth token.
    pub process_identity: Option<OwnedProcessIdentity>,
}

/// Session state in the host state machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session created, not yet started.
    Created,
    /// Session starting (provider launch in progress).
    Starting,
    /// Session ready for operations.
    Ready,
    /// Session busy with an operation.
    Busy,
    /// Session cancelling an operation.
    Cancelling,
    /// Session stopping (shutdown in progress).
    Stopping,
    /// Session stopped (terminal).
    Stopped,
    /// Recoverable error state.
    ErrorRecoverable,
    /// Terminal error state.
    ErrorTerminal,
}

#[cfg(test)]
mod descriptor_audit_tests {
    use super::*;

    // ── DF-20: Capability truthfulness audit (AH2.1 / AH2.2) ──────────
    //
    // These tests verify that `CapabilityDescriptor::acp_full()` claims
    // match the `AcpProvider` implementation. The spec (agent-host.md §4.1.4
    // D-003) requires the descriptor to never claim capabilities the adapter
    // cannot deliver.

    /// `set_model` must be `false` in the static descriptor because model
    /// switching depends on dynamic discovery of agent-specific config
    /// options. The `AcpProvider::handle_set_model()` method returns
    /// `CapabilityUnsupported` when no model config option is found.
    #[test]
    fn acp_full_set_model_is_false_dynamic_discovery() {
        let desc = CapabilityDescriptor::acp_full();
        assert!(
            !desc.set_model,
            "acp_full() must claim set_model = false because model switching \
             depends on dynamic config-option discovery, not a guaranteed SDK path"
        );
    }

    /// `set_mode` must be `true` in the static descriptor because the ACP
    /// protocol provides a stable `session/set_mode` RPC that the
    /// `AcpProvider::handle_set_mode()` method calls directly.
    #[test]
    fn acp_full_set_mode_is_true_stable_rpc() {
        let desc = CapabilityDescriptor::acp_full();
        assert!(
            desc.set_mode,
            "acp_full() must claim set_mode = true because the ACP protocol \
             provides a stable session/set_mode RPC"
        );
    }

    /// `native_cli_limited` must not claim `set_model` or `set_mode` — native
    /// CLI providers use subprocess invocation, not protocol-level control.
    #[test]
    fn native_cli_limited_no_model_or_mode() {
        let desc = CapabilityDescriptor::native_cli_limited();
        assert!(
            !desc.set_model,
            "native_cli_limited must not claim set_model"
        );
        assert!(!desc.set_mode, "native_cli_limited must not claim set_mode");
    }

    /// `native_cli_limited` must not claim `structured_tool_calls` because
    /// native CLI output is unstructured (agent-host.md §4.2.2).
    #[test]
    fn native_cli_limited_no_structured_tool_calls() {
        let desc = CapabilityDescriptor::native_cli_limited();
        assert!(
            !desc.structured_tool_calls,
            "native_cli_limited must not claim structured_tool_calls"
        );
    }

    /// `dsh_limited` is the documented narrower descriptor (locks § AR-6):
    /// `cancellation` must be honest `false` (no cancel RPC on the
    /// `Session::run` surface); `streaming` is `true` since v1.188 P1 —
    /// committed root `assistant/message` notifications are forwarded as
    /// message-level deltas, proven against the actual dsh runtime in T3.
    /// `text_prompt` / `session_restore` stay true and every other field
    /// matches `native_cli_limited` — never silently claim more or less.
    #[test]
    fn dsh_limited_is_narrower_descriptor() {
        let desc = CapabilityDescriptor::dsh_limited();
        assert!(desc.text_prompt, "dsh_limited must support text_prompt");
        assert!(
            desc.streaming,
            "dsh_limited claims message-level streaming after v1.188 P1 timing proof"
        );
        assert!(
            !desc.cancellation,
            "dsh_limited must not claim cancellation: the SDK has no cancel \
             RPC (AR-6)"
        );
        assert!(
            desc.session_restore,
            "dsh_limited must claim session_restore: the host-generated \
             session id is reused across executes (AR-2/AR-5)"
        );
        let native = CapabilityDescriptor::native_cli_limited();
        assert_eq!(desc.structured_tool_calls, native.structured_tool_calls);
        assert_eq!(desc.mcp_http, native.mcp_http);
        assert_eq!(desc.mcp_sse, native.mcp_sse);
        assert_eq!(desc.mcp_stdio, native.mcp_stdio);
        assert_eq!(desc.images, native.images);
        assert_eq!(desc.audio, native.audio);
        assert_eq!(desc.embedded_context, native.embedded_context);
        assert_eq!(desc.set_model, native.set_model);
        assert_eq!(desc.set_mode, native.set_mode);
        assert_eq!(desc.diagnostics, native.diagnostics);
    }

    /// `disabled` descriptor must have all capabilities off.
    #[test]
    fn disabled_all_off() {
        let desc = CapabilityDescriptor::disabled();
        assert!(!desc.text_prompt);
        assert!(!desc.streaming);
        assert!(!desc.cancellation);
        assert!(!desc.session_restore);
        assert!(!desc.structured_tool_calls);
        assert!(!desc.mcp_http);
        assert!(!desc.mcp_sse);
        assert!(!desc.mcp_stdio);
        assert!(!desc.images);
        assert!(!desc.audio);
        assert!(!desc.embedded_context);
        assert!(!desc.set_model);
        assert!(!desc.set_mode);
        assert!(!desc.diagnostics);
    }

    /// `acp_full` must claim core capabilities that every ACP session
    /// is expected to support (text prompt, streaming, cancellation).
    #[test]
    fn acp_full_core_capabilities_present() {
        let desc = CapabilityDescriptor::acp_full();
        assert!(desc.text_prompt, "ACP must support text_prompt");
        assert!(desc.streaming, "ACP must support streaming");
        assert!(desc.cancellation, "ACP must support cancellation");
        assert!(desc.session_restore, "ACP must support session_restore");
        assert!(
            desc.structured_tool_calls,
            "ACP must support structured_tool_calls"
        );
    }
}
