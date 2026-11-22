//! Capability model: host operations, events, descriptors, and DTOs. See docs/specs/agents/agent-host.md.
//!
//! Defines the normalized host operation and event types used across all provider
//! adapters, the capability descriptor for negotiation, and supporting DTOs.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::HostResult;
use crate::ids::{HostOperationId, HostSessionId, ProviderId};

/// note: A stream of host events from an operation.
pub type HostEventStream =
    std::pin::Pin<Box<dyn futures_util::Stream<Item = HostResult<HostEvent>> + Send + 'static>>;

/// note: protocol kind for a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolKind {
    /// note: ACP protocol provider (via `nexus-acp-host`).
    Acp,
    /// note: native CLI provider (subprocess, limited capabilities).
    NativeCli,
}

/// note: narrowing permission scope for a Host prompt operation (A1).
///
/// Intersected with Host configuration per operation; a provider unable to
/// honor a requested scope refuses `not_supported` rather than silently
/// discarding the policy. `None` preserves the standalone Host/Character
/// policy and never means unrestricted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptPermissionScope {
    /// note: read-only tools allowed.
    pub allow_read: bool,
    /// note: write tools allowed.
    pub allow_write: bool,
    /// note: destructive tools allowed.
    pub allow_destructive: bool,
}

/// note: host operation — only execution-scoped variants.
///
/// Cancel flows through `HostFacade::cancel()` / `ProviderAdapter::cancel()`.
/// Health is a separate `HostFacade::health()` query.
/// See PM Review Note R-002.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostOperation {
    /// note: send a prompt with content blocks.
    Prompt {
        /// note: unique operation ID for tracking.
        op_id: HostOperationId,
        /// note: content blocks comprising the prompt.
        content: Vec<HostContentBlock>,
        /// note: narrowing permission scope (A1). `None` preserves the standalone
        /// Host/Character policy; a provider unable to honor a requested
        /// scope refuses `not_supported`.
        permission_scope: Option<PromptPermissionScope>,
    },
    /// note: switch the model for the current session.
    SetModel {
        /// note: model identifier.
        model: String,
    },
    /// note: switch the mode for the current session.
    SetMode {
        /// note: mode identifier.
        mode: String,
    },
}

/// note: content block for host prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostContentBlock {
    /// note: plain text content.
    Text {
        /// note: the text content.
        text: String,
    },
    /// note: resource link content.
    ResourceLink {
        /// note: resource name.
        name: Option<String>,
        /// note: resource URI.
        uri: String,
    },
}

/// note: host event — terminal and intermediate events from provider execution.
///
/// Every operation emits exactly one terminal event (`OpFinished` or `OpFailed`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HostEvent {
    /// note: session successfully created.
    SessionCreated(SessionCreatedEvent),
    /// note: operation started executing.
    OpStarted(OperationStartedEvent),
    /// note: thinking/reasoning delta.
    ThoughtDelta(TextDeltaEvent),
    /// note: message text delta.
    MessageDelta(TextDeltaEvent),
    /// note: tool call initiated.
    ToolCall(ToolCallEvent),
    /// note: tool call status update.
    ToolCallUpdate(ToolCallUpdateEvent),
    /// Plan/status update.
    PlanUpdate(PlanUpdateEvent),
    /// note: status message (warning, info).
    Status(StatusEvent),
    /// note: operation completed successfully.
    OpFinished(OperationFinishedEvent),
    /// note: operation failed.
    OpFailed(OperationFailedEvent),
    /// note: session stopped (graceful or error).
    SessionStopped(SessionStoppedEvent),
}

/// note: session created event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreatedEvent {
    /// note: the session ID.
    pub session_id: HostSessionId,
    /// note: the provider ID.
    pub provider_id: ProviderId,
}

/// note: operation started event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStartedEvent {
    /// note: the operation ID.
    pub op_id: HostOperationId,
    /// note: the session ID.
    pub session_id: HostSessionId,
}

/// note: text delta event (used for both thought and message deltas).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDeltaEvent {
    /// note: the session ID.
    pub session_id: HostSessionId,
    /// note: the operation ID.
    pub op_id: HostOperationId,
    /// note: delta text content.
    pub text: String,
}

/// note: tool call event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallEvent {
    /// note: the session ID.
    pub session_id: HostSessionId,
    /// note: the operation ID.
    pub op_id: HostOperationId,
    /// note: tool call ID (provider-assigned).
    pub tool_call_id: String,
    /// note: tool name.
    pub tool_name: String,
}

/// note: tool call update event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallUpdateEvent {
    /// note: the session ID.
    pub session_id: HostSessionId,
    /// note: the operation ID.
    pub op_id: HostOperationId,
    /// note: tool call ID.
    pub tool_call_id: String,
    /// note: update content (partial result or status).
    pub content: String,
}

/// note: plan update event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanUpdateEvent {
    /// note: the session ID.
    pub session_id: HostSessionId,
    /// note: the operation ID.
    pub op_id: HostOperationId,
    /// note: plan content (structured or free text).
    pub content: String,
}

/// note: status event payload (warnings, info, non-error messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEvent {
    /// note: the session ID (if applicable).
    pub session_id: Option<HostSessionId>,
    /// note: status level.
    pub level: StatusLevel,
    /// note: status message.
    pub message: String,
}

/// note: status level for status events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusLevel {
    /// note: informational status.
    Info,
    /// note: warning status.
    Warning,
    /// note: error status (non-terminal).
    Error,
}

/// note: operation finished event payload (terminal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationFinishedEvent {
    /// note: the session ID.
    pub session_id: HostSessionId,
    /// note: the operation ID.
    pub op_id: HostOperationId,
    /// note: finish reason.
    pub reason: FinishReason,
}

/// note: reason for operation completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// note: normal end-of-turn completion.
    EndTurn,
    /// note: hit token limit.
    MaxTokens,
    /// note: hit turn request limit.
    MaxTurnRequests,
    /// note: agent refused the request.
    Refusal,
    /// note: the turn was cancelled by the client (I-004). Typed non-success:
    /// never mapped to `EndTurn`/succeeded for workflow or Character
    /// consumers.
    Cancelled,
}

/// note: operation failed event payload (terminal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationFailedEvent {
    /// note: the session ID.
    pub session_id: HostSessionId,
    /// note: the operation ID.
    pub op_id: HostOperationId,
    /// note: error category.
    pub error_category: String,
    /// note: error message.
    pub error_message: String,
}

/// note: session stopped event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStoppedEvent {
    /// note: the session ID.
    pub session_id: HostSessionId,
    /// note: stop reason.
    pub reason: SessionStopReason,
}

/// note: reason for session stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStopReason {
    /// note: graceful shutdown requested.
    GracefulShutdown,
    /// note: provider process exited.
    ProviderExit,
    /// note: error caused session termination.
    Error,
    /// note: session was cancelled.
    Cancelled,
}

/// note: configuration for starting the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostStartConfig {
    /// note: path to the config file.
    pub config_path: PathBuf,
    /// note: workspace root for the host.
    pub workspace_root: PathBuf,
    /// note: maximum concurrent sessions.
    pub max_sessions: usize,
    /// note: maximum concurrent operations per session.
    pub max_ops_per_session: usize,
    /// note: timeout configuration.
    pub timeouts: crate::config::TimeoutConfig,
    /// note: pre-validated config retained from boot (avoids a second disk read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_config: Option<crate::config::AgentHostConfig>,
    /// note: pre-admitted provider catalog from open-time PATH/recipe discovery.
    ///
    /// When set, [`crate::HostManager::start`] materializes adapters from this
    /// snapshot and does not re-run PATH discovery.
    #[serde(skip)]
    pub admitted_catalog: Option<Vec<crate::ProviderCatalogEntry>>,
    /// note: verified owner for bounded readiness probes at start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_owner: Option<SessionOwner>,
}

/// note: verified owner metadata bound to a Host session.
///
/// Constructed by Host/Character admission from existing verified scope
/// (never from request-body assertions). The canonical workspace cwd is
/// validated against this owner before any subprocess spawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOwner {
    /// note: the verified Creator ID that owns this session.
    pub creator_id: String,
    /// note: canonical workspace root of the Creator (absolute path).
    pub workspace_root: PathBuf,
    /// note: optional orchestration run ID (set by the trusted orchestration
    /// adapter; `None` for standalone Character/public Host sessions).
    pub orchestration_run_id: Option<String>,
}

/// note: request to create a new managed session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    /// note: provider to use for this session.
    pub provider_id: ProviderId,
    /// note: working directory for the session.
    pub cwd: PathBuf,
    /// note: optional model override.
    pub model: Option<String>,
    /// note: optional mode override.
    pub mode: Option<String>,
    /// note: MCP server configurations for the session.
    pub mcp_servers: Vec<McpServerConfig>,
    /// note: additional metadata (opaque to host).
    pub metadata: serde_json::Value,
    /// note: verified owner metadata (Creator + canonical workspace).
    pub owner: SessionOwner,
}

/// note: MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpServerConfig {
    /// note: HTTP-based MCP server.
    Http {
        /// note: server name.
        name: String,
        /// note: server URL.
        url: String,
    },
    /// note: SSE-based MCP server.
    Sse {
        /// note: server name.
        name: String,
        /// note: server URL.
        url: String,
    },
    /// note: stdio-based MCP server.
    Stdio {
        /// note: server name.
        name: String,
        /// note: command to execute.
        command: PathBuf,
        /// note: command arguments.
        args: Vec<String>,
        /// note: environment variables.
        env: std::collections::HashMap<String, String>,
    },
}

/// note: descriptor for a provider's static capabilities.
///
/// This struct intentionally uses individual bool fields rather than bitflags
/// to match the capability negotiation protocol (compass §Capability negotiation)
/// and maintain serde compatibility. Each field maps to a named ACP capability.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    /// note: supports text prompt input.
    pub text_prompt: bool,
    /// note: supports streaming events.
    pub streaming: bool,
    /// note: supports operation cancellation.
    pub cancellation: bool,
    /// note: supports session restore (load).
    pub session_restore: bool,
    /// note: supports structured tool call events.
    pub structured_tool_calls: bool,
    /// note: supports HTTP MCP transport.
    pub mcp_http: bool,
    /// note: supports SSE MCP transport.
    pub mcp_sse: bool,
    /// note: supports stdio MCP transport.
    pub mcp_stdio: bool,
    /// note: supports image input.
    pub images: bool,
    /// note: supports audio input.
    pub audio: bool,
    /// note: supports embedded context injection.
    pub embedded_context: bool,
    /// note: supports model switching.
    pub set_model: bool,
    /// note: supports mode switching.
    pub set_mode: bool,
    /// note: supports diagnostic/reporting events.
    pub diagnostics: bool,
}

impl CapabilityDescriptor {
    /// note: ACP full capability descriptor.
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

    /// note: native CLI capability descriptor with multi-turn session restore.
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

    /// note: native CLI descriptor for the `DeepSeek Harness` runtime
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

    /// note: disabled descriptor — all capabilities off.
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

/// note: static descriptor for a discovered provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    /// note: provider ID.
    pub provider_id: ProviderId,
    /// note: human-readable display name.
    pub display_name: String,
    /// note: protocol kind.
    pub protocol_kind: ProtocolKind,
    /// note: static capabilities.
    pub capabilities: CapabilityDescriptor,
}

/// note: provider health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    /// note: provider ID.
    pub provider_id: ProviderId,
    /// note: whether the provider is available.
    pub available: bool,
    /// note: optional latency hint (milliseconds).
    pub latency_ms: Option<u64>,
    /// note: optional status message.
    pub message: Option<String>,
}

/// note: probe request for checking provider availability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeRequest {
    /// note: timeout in milliseconds.
    pub timeout_ms: u64,
    /// note: canonical cwd for the bounded handshake probe.
    pub cwd: PathBuf,
    /// note: verified session owner (Creator + workspace) for the probe child.
    pub owner: SessionOwner,
}

/// note: launch specification for starting a provider session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchSpec {
    /// note: working directory for the session.
    pub cwd: PathBuf,
    /// note: optional model override.
    pub model: Option<String>,
    /// note: optional mode override.
    pub mode: Option<String>,
    /// note: MCP server configurations.
    pub mcp_servers: Vec<McpServerConfig>,
    /// note: verified owner metadata (Creator + canonical workspace).
    pub owner: SessionOwner,
}

/// note: handle to a managed provider session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedSessionHandle {
    /// note: provider ID.
    pub provider_id: ProviderId,
    /// note: host session ID.
    pub session_id: HostSessionId,
    /// note: negotiated capabilities for this session.
    pub capabilities: CapabilityDescriptor,
    /// note: opaque owned-process identity (PID + platform birth + owned group)
    /// captured at launch (A5). `None` when the platform cannot establish a
    /// birth token; cleanup must then be reported unconfirmed. Never a
    /// transport handle — the daemon persists this fingerprint only.
    pub process_identity: Option<OwnedProcessIdentity>,
}

/// note: opaque owned-process identity (A5) — PID plus process-birth and group.
///
/// Never identified by PID alone. Mirrors the durable
/// `nexus_session_storage::run_state::OwnedProcessIdentity` shape so the daemon
/// can persist the fingerprint without importing provider crates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedProcessIdentity {
    /// note: process id.
    pub pid: u32,
    /// note: platform process-birth identity (e.g. start time) for reuse detection.
    pub process_birth: Option<String>,
    /// note: owned process-group identity for tree cleanup.
    pub group_id: Option<String>,
}

/// note: host-level health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostHealth {
    /// note: whether the host is running.
    pub running: bool,
    /// note: number of active sessions.
    pub active_sessions: usize,
    /// note: number of active operations.
    pub active_operations: usize,
}

/// note: host session view (returned from `create_session`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostSession {
    /// note: session ID.
    pub session_id: HostSessionId,
    /// note: provider ID.
    pub provider_id: ProviderId,
    /// note: negotiated capabilities.
    pub capabilities: CapabilityDescriptor,
    /// note: session state.
    pub state: SessionState,
    /// note: opaque owned-process identity (PID + birth + group) captured at
    /// launch (A5). `None` when the platform cannot establish a birth token.
    pub process_identity: Option<OwnedProcessIdentity>,
}

/// note: session state in the host state machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// note: session created, not yet started.
    Created,
    /// note: session starting (provider launch in progress).
    Starting,
    /// note: session ready for operations.
    Ready,
    /// note: session busy with an operation.
    Busy,
    /// note: session cancelling an operation.
    Cancelling,
    /// note: session stopping (shutdown in progress).
    Stopping,
    /// note: session stopped (terminal).
    Stopped,
    /// note: recoverable error state.
    ErrorRecoverable,
    /// note: terminal error state.
    ErrorTerminal,
}

#[cfg(test)]
mod descriptor_audit_tests {
    use super::*;

    // note: ── DF-20: Capability truthfulness audit (AH2.1 / AH2.2) ──────────
    //
    // These tests verify that `CapabilityDescriptor::acp_full()` claims
    // match the `AcpProvider` implementation. The spec (agent-host.md §4.1.4
    // D-003) requires the descriptor to never claim capabilities the adapter
    // cannot deliver.

    /// note: `set_model` must be `false` in the static descriptor because model
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

    /// note: `set_mode` must be `true` in the static descriptor because the ACP
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

    /// note: `native_cli_limited` must not claim `set_model` or `set_mode` — native
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

    /// note: `native_cli_limited` must not claim `structured_tool_calls` because
    /// native CLI output is unstructured (agent-host.md §4.2.2).
    #[test]
    fn native_cli_limited_no_structured_tool_calls() {
        let desc = CapabilityDescriptor::native_cli_limited();
        assert!(
            !desc.structured_tool_calls,
            "native_cli_limited must not claim structured_tool_calls"
        );
    }

    /// note: `dsh_limited` is the documented narrower descriptor (locks § AR-6):
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

    /// note: `disabled` descriptor must have all capabilities off.
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

    /// note: `acp_full` must claim core capabilities that every ACP session
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
