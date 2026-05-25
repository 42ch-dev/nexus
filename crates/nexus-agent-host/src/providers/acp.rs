//! ACP provider adapter — wraps `nexus-acp-host` behind `ProviderAdapter`.
//!
//! Translates ACP SDK lifecycle into the normalized `ProviderAdapter` trait:
//!
//! ```text
//! probe       → health check (initialize handshake + immediate teardown)
//! launch      → initialize + create_session
//! execute     → stream_prompt → HostEvent stream
//! cancel      → NexusAcpClient::cancel
//! shutdown    → drop session
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::sync::RwLock;

use nexus_acp_host::{AcpPermissionOutcome, AcpSdkAdapter, AcpStreamUpdate, NexusAcpClient};
use nexus_contracts::local::acp::{
    NexusConfigOption, NexusConfigOptionCategory, NexusContentBlock, NexusInitializeRequest,
    NexusNewSessionRequest, NexusPromptRequest, NexusSessionId, NexusSetConfigOptionRequest,
};

use crate::capability::model::{
    CapabilityDescriptor, FinishReason, HostContentBlock, HostEvent, HostEventStream,
    ManagedSessionHandle, OperationFailedEvent, OperationFinishedEvent, OperationStartedEvent,
    PlanUpdateEvent, ProtocolKind, ProviderDescriptor, ProviderHealth, TextDeltaEvent,
    ToolCallEvent, ToolCallUpdateEvent,
};
use crate::capability::risk::{AutoToolRiskClassifier, ToolRiskClassifier};
use crate::config::TimeoutConfig;
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::policy::permission::{HostPermissionResolver, PermissionOutcome};
use crate::ProviderAdapter;

/// Internal state tracked per active ACP session.
#[derive(Debug)]
struct AcpSessionState {
    /// The ACP session ID (from the SDK).
    acp_session_id: NexusSessionId,
    /// Configuration options exposed by the agent at session creation.
    /// Used for dynamic model switching via `set_config_option`.
    config_options: Option<Vec<NexusConfigOption>>,
    /// Map of active operation IDs to their cancel signal.
    /// Wave 1 enforces one op per session; this field supports future multi-op.
    #[allow(dead_code)]
    active_ops: HashMap<HostOperationId, tokio::sync::watch::Sender<bool>>,
}

/// ACP provider adapter.
///
/// Wraps an [`AcpSdkAdapter`] and implements the [`ProviderAdapter`] trait.
/// Each instance is bound to a single provider ID and manages session state
/// through the ACP SDK lifecycle.
pub struct AcpProvider {
    /// Provider ID for this adapter.
    provider_id: ProviderId,
    /// Display name for this provider.
    display_name: String,
    /// The underlying ACP SDK adapter.
    client: Arc<AcpSdkAdapter>,
    /// Active sessions: host session ID → ACP session state.
    sessions: Arc<RwLock<HashMap<HostSessionId, AcpSessionState>>>,
    /// Timeout configuration for stage-level enforcement.
    timeouts: TimeoutConfig,
}

impl AcpProvider {
    /// Create a new ACP provider adapter.
    ///
    /// The `client` should already have an established connection
    /// (via `AcpSdkAdapter::with_connection`). The `permission_resolver`
    /// is used to evaluate ACP permission requests against policy.
    ///
    /// This constructor awaits the permission handler registration so
    /// the provider is ready to use immediately upon return (QC2 F-005,
    /// QC3 F-003: no spawn-without-await race).
    pub async fn new(
        provider_id: ProviderId,
        display_name: String,
        client: AcpSdkAdapter,
        timeouts: TimeoutConfig,
        permission_resolver: HostPermissionResolver,
    ) -> Self {
        // Wire up the permission handler on the SDK adapter.
        // The handler evaluates each tool permission request by:
        // 1. Classifying the tool risk using AutoToolRiskClassifier
        // 2. Resolving the permission via HostPermissionResolver
        let pid = provider_id.clone();
        let classifier = AutoToolRiskClassifier::new();

        let handler: Arc<dyn Fn(&str) -> AcpPermissionOutcome + Send + Sync> =
            Arc::new(move |tool_name: &str| {
                let risk = classifier.classify_or_default(tool_name);
                let outcome =
                    permission_resolver.resolve(ProtocolKind::Acp, &pid.0, tool_name, Some(risk));
                match outcome {
                    PermissionOutcome::Allow => AcpPermissionOutcome::Approve,
                    // In non-interactive host context, Ask defaults to Deny.
                    // Interactive prompting will be added in a future release.
                    PermissionOutcome::Ask | PermissionOutcome::Deny => AcpPermissionOutcome::Deny,
                }
            });

        // Await the handler registration — the provider is not usable until
        // the handler is installed (QC2 F-005, QC3 F-003).
        let client_arc = Arc::new(client);
        client_arc.set_permission_handler(handler).await;

        Self {
            provider_id,
            display_name,
            client: client_arc,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            timeouts,
        }
    }

    /// Convert host content blocks to ACP content blocks.
    fn to_acp_content(blocks: &[HostContentBlock]) -> Vec<NexusContentBlock> {
        blocks
            .iter()
            .map(|block| match block {
                HostContentBlock::Text { text } => {
                    NexusContentBlock::Text(nexus_contracts::local::acp::NexusTextContent {
                        text: text.clone(),
                    })
                }
                HostContentBlock::ResourceLink { name, uri } => NexusContentBlock::ResourceLink(
                    nexus_contracts::local::acp::NexusResourceLink {
                        name: name.clone(),
                        uri: uri.clone(),
                    },
                ),
            })
            .collect()
    }

    /// Create a stream that emits `OpStarted` followed by `OpFailed`.
    ///
    /// Used when `execute()` encounters an error (timeout, protocol error)
    /// to ensure the session state machine always receives a terminal event.
    /// Without this, the session stays stuck in `Busy` (QC3 F-001).
    fn make_error_stream(
        session_id: HostSessionId,
        op_id: HostOperationId,
        error_category: &str,
        error_message: String,
    ) -> HostEventStream {
        futures_util::stream::iter(vec![
            Ok(HostEvent::OpStarted(OperationStartedEvent {
                op_id: op_id.clone(),
                session_id: session_id.clone(),
            })),
            Ok(HostEvent::OpFailed(OperationFailedEvent {
                session_id,
                op_id,
                error_category: error_category.to_string(),
                error_message,
            })),
        ])
        .boxed()
    }

    /// Convert an `AcpStreamUpdate` to a `HostEvent`.
    fn stream_update_to_event(
        update: AcpStreamUpdate,
        session_id: &HostSessionId,
        op_id: &HostOperationId,
    ) -> HostEvent {
        match update {
            AcpStreamUpdate::TextDelta { text, .. } => HostEvent::MessageDelta(TextDeltaEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                text,
            }),
            AcpStreamUpdate::ThoughtDelta { text, .. } => HostEvent::ThoughtDelta(TextDeltaEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                text,
            }),
            AcpStreamUpdate::ToolCall {
                tool_call_id,
                tool_name,
                ..
            } => HostEvent::ToolCall(ToolCallEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                tool_call_id,
                tool_name,
            }),
            AcpStreamUpdate::ToolCallUpdate {
                tool_call_id,
                content,
                ..
            } => HostEvent::ToolCallUpdate(ToolCallUpdateEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                tool_call_id,
                content,
            }),
            AcpStreamUpdate::PlanUpdate { content, .. } => HostEvent::PlanUpdate(PlanUpdateEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                content,
            }),
            AcpStreamUpdate::Stopped {
                stop_reason: reason,
                ..
            } => {
                let finish = match reason {
                    nexus_contracts::local::acp::NexusStopReason::MaxTokens => {
                        FinishReason::MaxTokens
                    }
                    nexus_contracts::local::acp::NexusStopReason::MaxTurnRequests => {
                        FinishReason::MaxTurnRequests
                    }
                    nexus_contracts::local::acp::NexusStopReason::Refusal => FinishReason::Refusal,
                    nexus_contracts::local::acp::NexusStopReason::EndTurn
                    | nexus_contracts::local::acp::NexusStopReason::Cancelled => {
                        FinishReason::EndTurn
                    }
                };
                HostEvent::OpFinished(OperationFinishedEvent {
                    session_id: session_id.clone(),
                    op_id: op_id.clone(),
                    reason: finish,
                })
            }
            AcpStreamUpdate::PermissionResult {
                tool_name,
                approved,
                ..
            } => {
                // Emit a ToolCallUpdate for the permission decision.
                // This provides observability into tool permission evaluation.
                HostEvent::ToolCallUpdate(ToolCallUpdateEvent {
                    session_id: session_id.clone(),
                    op_id: op_id.clone(),
                    tool_call_id: format!("perm-{tool_name}"),
                    content: if approved {
                        format!("Permission approved: {tool_name}")
                    } else {
                        format!("Permission denied: {tool_name}")
                    },
                })
            }
        }
    }

    /// Handle `SetMode` operation via the stable `session/set_mode` RPC.
    async fn handle_set_mode(
        &self,
        session: &ManagedSessionHandle,
        mode: String,
    ) -> HostResult<HostEventStream> {
        let acp_session_id = {
            let sessions = self.sessions.read().await;
            sessions
                .get(&session.session_id)
                .map(|s| s.acp_session_id.clone())
                .ok_or_else(|| {
                    HostError::internal(format!(
                        "session {} not found in ACP provider",
                        session.session_id
                    ))
                })?
        };

        self.client
            .set_mode(acp_session_id, mode)
            .await
            .map_err(|e| {
                HostError::capability_unsupported(
                    self.provider_id.clone(),
                    "set_mode",
                    format!("ACP set_mode failed: {e}"),
                )
            })?;

        // Emit a single OpFinished event to signal success.
        let op_id = HostOperationId::new();
        let stream =
            futures_util::stream::iter(vec![Ok(HostEvent::OpFinished(OperationFinishedEvent {
                session_id: session.session_id.clone(),
                op_id,
                reason: FinishReason::EndTurn,
            }))])
            .boxed();

        Ok(stream)
    }

    /// Handle `SetModel` operation via `set_config_option` with dynamic discovery.
    ///
    /// Searches the session's `config_options` for an option with
    /// `category == Model`. If found, uses its `id` as the `config_id` in
    /// `set_config_option`. If not found (agent does not expose model config),
    /// returns `CapabilityUnsupported`.
    async fn handle_set_model(
        &self,
        session: &ManagedSessionHandle,
        model: String,
    ) -> HostResult<HostEventStream> {
        let (acp_session_id, model_config_id) = {
            let sessions = self.sessions.read().await;
            let state = sessions.get(&session.session_id).ok_or_else(|| {
                HostError::internal(format!(
                    "session {} not found in ACP provider",
                    session.session_id
                ))
            })?;

            // Find the model config option by category
            let config_id = state.config_options.as_ref().and_then(|opts| {
                opts.iter().find_map(|opt| {
                    if opt.category.as_ref()? == &NexusConfigOptionCategory::Model {
                        Some(opt.id.clone())
                    } else {
                        None
                    }
                })
            });

            let result = (state.acp_session_id.clone(), config_id);
            drop(sessions);
            result
        };

        let Some(config_id) = model_config_id else {
            return Err(HostError::capability_unsupported(
                self.provider_id.clone(),
                "set_model",
                "No model config option discovered for this session's agent",
            ));
        };

        // Attempt to set the model config option
        let request = NexusSetConfigOptionRequest::new(acp_session_id, config_id, model);

        match self.client.set_config_option(request).await {
            Ok(_) => {
                // Emit a single OpFinished event to signal success.
                let op_id = HostOperationId::new();
                let stream = futures_util::stream::iter(vec![Ok(HostEvent::OpFinished(
                    OperationFinishedEvent {
                        session_id: session.session_id.clone(),
                        op_id,
                        reason: FinishReason::EndTurn,
                    },
                ))])
                .boxed();

                Ok(stream)
            }
            Err(e) => {
                // Graceful fallback: emit a Status warning and then OpFailed.
                let op_id = HostOperationId::new();
                let session_id = session.session_id.clone();
                let provider_id = self.provider_id.clone();
                let error_msg = e.to_string();

                tracing::warn!(
                    provider_id = %provider_id,
                    session_id = %session_id,
                    error = %error_msg,
                    "set_config_option for model failed"
                );

                let stream = futures_util::stream::iter(vec![
                    Ok(HostEvent::Status(crate::capability::model::StatusEvent {
                        session_id: Some(session_id.clone()),
                        level: crate::capability::model::StatusLevel::Warning,
                        message: format!("SetModel failed: {error_msg}"),
                    })),
                    Ok(HostEvent::OpFailed(OperationFailedEvent {
                        session_id,
                        op_id,
                        error_category: "set_model_failed".to_string(),
                        error_message: format!("set_config_option for model failed: {error_msg}"),
                    })),
                ])
                .boxed();

                Ok(stream)
            }
        }
    }
}

#[async_trait]
impl ProviderAdapter for AcpProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: self.provider_id.clone(),
            display_name: self.display_name.clone(),
            protocol_kind: ProtocolKind::Acp,
            capabilities: CapabilityDescriptor::acp_full(),
        }
    }

    async fn probe(
        &self,
        _request: crate::capability::model::ProbeRequest,
    ) -> HostResult<ProviderHealth> {
        // Attempt an initialize handshake to verify the agent is responsive.
        let init_request = NexusInitializeRequest::new().client_info(
            nexus_contracts::local::acp::NexusAgentInfo {
                name: "nexus42".to_string(),
                title: Some("Nexus Agent Host".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        );

        let launch_dur = self.timeouts.launch_duration();

        match tokio::time::timeout(launch_dur, self.client.initialize(init_request)).await {
            Ok(Ok(_)) => Ok(ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: true,
                latency_ms: None,
                message: None,
            }),
            Ok(Err(e)) => Ok(ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: false,
                latency_ms: None,
                message: Some(format!("probe failed: {e}")),
            }),
            Err(_) => Ok(ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: false,
                latency_ms: None,
                message: Some(format!(
                    "probe timed out after {}ms",
                    self.timeouts.launch_ms
                )),
            }),
        }
    }

    async fn launch(
        &self,
        spec: crate::capability::model::LaunchSpec,
    ) -> HostResult<ManagedSessionHandle> {
        let launch_dur = self.timeouts.launch_duration();

        // Create ACP session with launch timeout
        let acp_request = NexusNewSessionRequest::new(spec.cwd);

        let session_created =
            tokio::time::timeout(launch_dur, self.client.create_session(acp_request))
                .await
                .map_err(|_| {
                    HostError::timeout(
                        "launch",
                        format!(
                            "ACP session creation timed out after {}ms",
                            self.timeouts.launch_ms
                        ),
                    )
                    .with_provider(self.provider_id.clone())
                })?
                .map_err(|e| {
                    HostError::launch_failed(
                        self.provider_id.clone(),
                        "ACP session creation failed",
                        Some(e.to_string()),
                    )
                })?;

        let host_session_id = HostSessionId::new();

        // Track the session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                host_session_id.clone(),
                AcpSessionState {
                    acp_session_id: session_created.session_id,
                    config_options: session_created.config_options,
                    active_ops: HashMap::new(),
                },
            );
        }

        Ok(ManagedSessionHandle {
            provider_id: self.provider_id.clone(),
            session_id: host_session_id,
            capabilities: CapabilityDescriptor::acp_full(),
        })
    }

    // execute() exceeds 100-line limit due to streaming timeout handling (D-004).
    // Splitting would reduce clarity of timeout/error path logic.
    #[allow(clippy::too_many_lines)]
    async fn execute(
        &self,
        session: &ManagedSessionHandle,
        op: crate::capability::model::HostOperation,
    ) -> HostResult<HostEventStream> {
        let (op_id, content_blocks) = match op {
            crate::capability::model::HostOperation::Prompt { op_id, content } => (op_id, content),
            crate::capability::model::HostOperation::SetMode { mode } => {
                return self.handle_set_mode(session, mode).await;
            }
            crate::capability::model::HostOperation::SetModel { model } => {
                return self.handle_set_model(session, model).await;
            }
        };

        // Look up the ACP session ID
        let acp_session_id = {
            let sessions = self.sessions.read().await;
            match sessions.get(&session.session_id) {
                Some(s) => s.acp_session_id.clone(),
                None => {
                    // Session not found — emit OpStarted+OpFailed stream so the
                    // session state machine transitions back to Ready (QC3 F-001).
                    return Ok(Self::make_error_stream(
                        session.session_id.clone(),
                        op_id,
                        "session_not_found",
                        format!("session {} not found in ACP provider", session.session_id),
                    ));
                }
            }
        };

        // Build the prompt request — clone acp_session_id for potential cancel.
        let acp_sid_for_cancel = acp_session_id.clone();
        let prompt_request = NexusPromptRequest {
            session_id: acp_session_id,
            prompt: Self::to_acp_content(&content_blocks),
        };

        // Start streaming with prompt_ms timeout for the initial stream setup
        let prompt_dur = self.timeouts.prompt_duration();

        let rx = match tokio::time::timeout(prompt_dur, self.client.stream_prompt(prompt_request))
            .await
        {
            Ok(Ok(receiver)) => receiver,
            Ok(Err(e)) => {
                // Protocol error — emit OpStarted+OpFailed so the manager's
                // stream wrapper sees OpFailed and transitions back to Ready.
                tracing::warn!(
                    provider_id = %self.provider_id,
                    session_id = %session.session_id,
                    error = %e,
                    "ACP stream_prompt failed"
                );
                return Ok(Self::make_error_stream(
                    session.session_id.clone(),
                    op_id,
                    "protocol_error",
                    format!("ACP stream_prompt failed: {e}"),
                ));
            }
            Err(_) => {
                // Timeout on stream setup — best-effort cancel the orphaned ACP
                // session before emitting OpFailed (QC3 F-002).
                tracing::warn!(
                    provider_id = %self.provider_id,
                    session_id = %session.session_id,
                    acp_session_id = %acp_sid_for_cancel.0,
                    timeout_ms = self.timeouts.prompt_ms,
                    "stream_prompt setup timed out, sending best-effort cancel"
                );
                if let Err(cancel_err) = self.client.cancel(acp_sid_for_cancel).await {
                    tracing::warn!(
                        error = %cancel_err,
                        "Best-effort cancel failed on stream setup timeout"
                    );
                }
                return Ok(Self::make_error_stream(
                    session.session_id.clone(),
                    op_id,
                    "operation_timeout",
                    format!(
                        "stream_prompt setup timed out after {}ms",
                        self.timeouts.prompt_ms
                    ),
                ));
            }
        };

        // Convert the mpsc::Receiver into a futures Stream of HostEvent
        let session_id = session.session_id.clone();
        let op_id_for_stream = op_id.clone();
        let prompt_dur_for_stream = prompt_dur;
        let provider_id_for_stream = self.provider_id.clone();

        // Clones for the timeout fallback closure (inner_stream closure
        // consumes session_id and op_id_for_stream via move).
        let session_id_for_timeout = session_id.clone();
        let op_id_for_timeout = op_id_for_stream.clone();

        // Clone client and ACP session ID for best-effort cancel on streaming
        // timeout (QC3 F-002).
        let client_for_cancel = Arc::clone(&self.client);
        let acp_sid_for_stream_cancel = acp_sid_for_cancel;

        // First emit OpStarted, then forward the stream with a cumulative
        // streaming timeout (QC2 F-001). The timeout budget covers the entire
        // execute duration from stream start; if no event arrives within the
        // remaining budget, we emit OpFailed and end the stream.
        let started = futures_util::stream::once({
            let op_id = op_id_for_stream.clone();
            let session_id = session_id.clone();
            async move {
                Ok(HostEvent::OpStarted(OperationStartedEvent {
                    op_id,
                    session_id,
                }))
            }
        });

        let inner_stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(move |update| {
            let sid = session_id.clone();
            let oid = op_id_for_stream.clone();
            Ok(Self::stream_update_to_event(update, &sid, &oid))
        });

        // Wrap with a cumulative timeout: if the stream produces no event
        // within `prompt_dur` from this point, abort with OpFailed.
        // Use tokio_stream::StreamExt::timeout (not futures_util) for per-item
        // deadline enforcement (QC2 F-001).
        //
        // On streaming timeout, send a best-effort ACP cancel to avoid
        // orphaned sessions (QC3 F-002). We use .then() to await the cancel
        // before yielding the terminal OpFailed.
        let timeout_stream = tokio_stream::StreamExt::timeout(inner_stream, prompt_dur_for_stream)
            .then(move |result| {
                let client = client_for_cancel.clone();
                let acp_sid = acp_sid_for_stream_cancel.clone();
                let sid = session_id_for_timeout.clone();
                let oid = op_id_for_timeout.clone();
                let provider_id = provider_id_for_stream.clone();
                let dur = prompt_dur_for_stream;

                async move {
                    if let Ok(event) = result {
                        event
                    } else {
                        // Elapsed — no event arrived within the budget.
                        // Best-effort cancel the orphaned ACP session (QC3 F-002).
                        tracing::warn!(
                            provider_id = %provider_id,
                            session_id = %sid,
                            "Streaming timed out: no event within prompt_ms budget, sending best-effort cancel"
                        );
                        if let Err(cancel_err) = client.cancel(acp_sid).await {
                            tracing::warn!(
                                error = %cancel_err,
                                "Best-effort cancel failed on streaming timeout"
                            );
                        }
                        Ok(HostEvent::OpFailed(OperationFailedEvent {
                            session_id: sid,
                            op_id: oid,
                            error_category: "streaming_timeout".to_string(),
                            error_message: format!(
                                "streaming timed out: no event within {}ms budget",
                                dur.as_millis()
                            ),
                        }))
                    }
                }
            });

        let stream = started.chain(timeout_stream).boxed();

        Ok(stream)
    }

    async fn cancel(
        &self,
        session: &ManagedSessionHandle,
        _op_id: HostOperationId,
    ) -> HostResult<()> {
        let acp_session_id = {
            let sessions = self.sessions.read().await;
            sessions
                .get(&session.session_id)
                .map(|s| s.acp_session_id.clone())
                .ok_or_else(|| {
                    HostError::internal(format!(
                        "session {} not found for cancel",
                        session.session_id
                    ))
                })?
        };

        self.client
            .cancel(acp_session_id)
            .await
            .map_err(|e| HostError::protocol_error("cancel failed", Some(e.to_string())))?;

        Ok(())
    }

    async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()> {
        // Remove the session from tracking. The ACP SDK adapter handles
        // the underlying session cleanup when dropped.
        {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&session.session_id);
        }
        tracing::info!(
            session_id = %session.session_id,
            provider_id = %self.provider_id,
            "ACP session removed from provider"
        );
        Ok(())
    }

    fn capabilities(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::acp_full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_provider_id() -> ProviderId {
        ProviderId::new("test-acp")
    }

    fn test_display_name() -> String {
        "Test ACP Provider".to_string()
    }

    #[test]
    fn descriptor_returns_acp_full() {
        // We can't create a real AcpSdkAdapter without a subprocess,
        // but we can verify the type construction.
        let provider_id = test_provider_id();
        let display_name = test_display_name();

        // Verify descriptor fields are correct
        let expected_descriptor = ProviderDescriptor {
            provider_id: provider_id.clone(),
            display_name: display_name.clone(),
            protocol_kind: ProtocolKind::Acp,
            capabilities: CapabilityDescriptor::acp_full(),
        };

        assert_eq!(expected_descriptor.protocol_kind, ProtocolKind::Acp);
        assert!(expected_descriptor.capabilities.streaming);
        assert!(expected_descriptor.capabilities.cancellation);
        assert!(expected_descriptor.capabilities.text_prompt);
    }

    #[test]
    fn content_block_conversion() {
        let host_blocks = vec![
            HostContentBlock::Text {
                text: "Hello".to_string(),
            },
            HostContentBlock::ResourceLink {
                name: Some("file.rs".to_string()),
                uri: "file:///test.rs".to_string(),
            },
        ];

        let acp_blocks = AcpProvider::to_acp_content(&host_blocks);
        assert_eq!(acp_blocks.len(), 2);

        match &acp_blocks[0] {
            NexusContentBlock::Text(t) => assert_eq!(t.text, "Hello"),
            _ => panic!("expected Text block"),
        }

        match &acp_blocks[1] {
            NexusContentBlock::ResourceLink(r) => {
                assert_eq!(r.uri, "file:///test.rs");
            }
            _ => panic!("expected ResourceLink block"),
        }
    }

    #[test]
    fn stream_update_text_delta_to_event() {
        let session_id = HostSessionId::new();
        let op_id = HostOperationId::new();

        let update = AcpStreamUpdate::TextDelta {
            session_id: "test".to_string(),
            text: "Hello world".to_string(),
        };

        let event = AcpProvider::stream_update_to_event(update, &session_id, &op_id);

        match event {
            HostEvent::MessageDelta(delta) => {
                assert_eq!(delta.text, "Hello world");
                assert_eq!(delta.session_id, session_id);
                assert_eq!(delta.op_id, op_id);
            }
            _ => panic!("expected MessageDelta event"),
        }
    }

    #[test]
    fn stream_update_stopped_to_event() {
        let session_id = HostSessionId::new();
        let op_id = HostOperationId::new();

        let update = AcpStreamUpdate::Stopped {
            session_id: "test".to_string(),
            stop_reason: nexus_contracts::local::acp::NexusStopReason::EndTurn,
        };

        let event = AcpProvider::stream_update_to_event(update, &session_id, &op_id);

        match event {
            HostEvent::OpFinished(finished) => {
                assert_eq!(finished.reason, FinishReason::EndTurn);
            }
            _ => panic!("expected OpFinished event"),
        }
    }

    #[test]
    fn stream_update_permission_approved_to_event() {
        let session_id = HostSessionId::new();
        let op_id = HostOperationId::new();

        let update = AcpStreamUpdate::PermissionResult {
            session_id: "test".to_string(),
            tool_name: "file_read".to_string(),
            approved: true,
        };

        let event = AcpProvider::stream_update_to_event(update, &session_id, &op_id);

        match event {
            HostEvent::ToolCallUpdate(tc) => {
                assert_eq!(tc.session_id, session_id);
                assert_eq!(tc.op_id, op_id);
                assert_eq!(tc.tool_call_id, "perm-file_read");
                assert!(tc.content.contains("approved"));
                assert!(tc.content.contains("file_read"));
            }
            _ => panic!("expected ToolCallUpdate event"),
        }
    }

    #[test]
    fn stream_update_permission_denied_to_event() {
        let session_id = HostSessionId::new();
        let op_id = HostOperationId::new();

        let update = AcpStreamUpdate::PermissionResult {
            session_id: "test".to_string(),
            tool_name: "file_delete".to_string(),
            approved: false,
        };

        let event = AcpProvider::stream_update_to_event(update, &session_id, &op_id);

        match event {
            HostEvent::ToolCallUpdate(tc) => {
                assert_eq!(tc.tool_call_id, "perm-file_delete");
                assert!(tc.content.contains("denied"));
                assert!(tc.content.contains("file_delete"));
            }
            _ => panic!("expected ToolCallUpdate event"),
        }
    }

    #[test]
    fn stream_update_thought_delta_to_event() {
        let session_id = HostSessionId::new();
        let op_id = HostOperationId::new();

        let update = AcpStreamUpdate::ThoughtDelta {
            session_id: "test".to_string(),
            text: "Let me think about this...".to_string(),
        };

        let event = AcpProvider::stream_update_to_event(update, &session_id, &op_id);

        match event {
            HostEvent::ThoughtDelta(delta) => {
                assert_eq!(delta.text, "Let me think about this...");
                assert_eq!(delta.session_id, session_id);
                assert_eq!(delta.op_id, op_id);
            }
            _ => panic!("expected ThoughtDelta event"),
        }
    }

    #[test]
    fn stream_update_tool_call_to_event() {
        let session_id = HostSessionId::new();
        let op_id = HostOperationId::new();

        let update = AcpStreamUpdate::ToolCall {
            session_id: "test".to_string(),
            tool_call_id: "tc-123".to_string(),
            tool_name: "file_read".to_string(),
        };

        let event = AcpProvider::stream_update_to_event(update, &session_id, &op_id);

        match event {
            HostEvent::ToolCall(tc) => {
                assert_eq!(tc.tool_call_id, "tc-123");
                assert_eq!(tc.tool_name, "file_read");
                assert_eq!(tc.session_id, session_id);
                assert_eq!(tc.op_id, op_id);
            }
            _ => panic!("expected ToolCall event"),
        }
    }

    #[test]
    fn stream_update_tool_call_update_to_event() {
        let session_id = HostSessionId::new();
        let op_id = HostOperationId::new();

        let update = AcpStreamUpdate::ToolCallUpdate {
            session_id: "test".to_string(),
            tool_call_id: "tc-456".to_string(),
            content: "File contents: ...".to_string(),
        };

        let event = AcpProvider::stream_update_to_event(update, &session_id, &op_id);

        match event {
            HostEvent::ToolCallUpdate(tc) => {
                assert_eq!(tc.tool_call_id, "tc-456");
                assert_eq!(tc.content, "File contents: ...");
            }
            _ => panic!("expected ToolCallUpdate event"),
        }
    }

    #[test]
    fn stream_update_plan_update_to_event() {
        let session_id = HostSessionId::new();
        let op_id = HostOperationId::new();

        let update = AcpStreamUpdate::PlanUpdate {
            session_id: "test".to_string(),
            content: "Step 1; Step 2; Step 3".to_string(),
        };

        let event = AcpProvider::stream_update_to_event(update, &session_id, &op_id);

        match event {
            HostEvent::PlanUpdate(plan) => {
                assert_eq!(plan.content, "Step 1; Step 2; Step 3");
                assert_eq!(plan.session_id, session_id);
                assert_eq!(plan.op_id, op_id);
            }
            _ => panic!("expected PlanUpdate event"),
        }
    }

    // ── DF-19 permission handling tests (AH1.1 / AH1.2) ──────────────
    //
    // The permission handler is wired in `AcpProvider::new()` and delegates
    // to `HostPermissionResolver::resolve()`. The handler is synchronous
    // (`Fn(&str) -> AcpPermissionOutcome`) so it CANNOT hang or timeout —
    // it returns immediately with a structured allow/deny/default outcome.
    //
    // The SDK's `on_receive_request` handler calls this closure on the
    // LocalSet bridge thread and sends the response back to the agent.
    // No infinite wait is possible because:
    // 1. The handler callback is `Fn(&str) -> AcpPermissionOutcome` (sync)
    // 2. If no handler is registered, the SDK denies by default
    // 3. The `stream_prompt` streaming path uses cumulative timeouts (D-004)
    //
    // Timeout/cancel path: If the agent does not process the permission
    // response in time, the streaming timeout (prompt_ms) fires and emits
    // OpFailed. The host then sends a best-effort cancel (QC3 F-002).

    /// Verify that the permission handler closure is synchronous and cannot
    /// hang — it must return an outcome immediately.
    #[test]
    fn permission_handler_returns_immediately() {
        use crate::config::PolicyConfig;
        use crate::policy::permission::{HostPermissionResolver, PermissionOutcome};

        let config = PolicyConfig::default();
        let resolver = HostPermissionResolver::new_native_only(&config);

        // Build the same handler closure that AcpProvider::new() creates
        let classifier = AutoToolRiskClassifier::new();
        let provider_id = ProviderId::new("test-acp");
        let handler: Box<dyn Fn(&str) -> AcpPermissionOutcome + Send + Sync> =
            Box::new(move |tool_name: &str| {
                let risk = classifier.classify_or_default(tool_name);
                let outcome =
                    resolver.resolve(ProtocolKind::Acp, &provider_id.0, tool_name, Some(risk));
                match outcome {
                    PermissionOutcome::Allow => AcpPermissionOutcome::Approve,
                    PermissionOutcome::Ask | PermissionOutcome::Deny => AcpPermissionOutcome::Deny,
                }
            });

        // Verify the handler returns immediately for various tool names
        // (no async, no blocking, no timeout possible)
        assert_eq!(handler("file_read"), AcpPermissionOutcome::Deny); // no ACP policy → deny
        assert_eq!(handler("file_delete"), AcpPermissionOutcome::Deny); // destructive → deny
        assert_eq!(handler("unknown_tool"), AcpPermissionOutcome::Deny); // unknown → deny
    }

    /// Verify that the permission handler for ACP without a loaded policy
    /// defaults to Deny (safe default — no infinite wait, no hang).
    #[test]
    fn permission_handler_no_policy_defaults_deny() {
        use crate::config::PolicyConfig;
        use crate::policy::permission::{HostPermissionResolver, PermissionOutcome};

        let config = PolicyConfig::default();
        let resolver = HostPermissionResolver::new_native_only(&config);
        let _classifier = AutoToolRiskClassifier::new(); // present for structural parity but not used in this test
        let provider_id = ProviderId::new("test-acp");

        // Without ACP policy loaded, all ACP permission requests default to Deny
        let outcome = resolver.resolve(
            ProtocolKind::Acp,
            &provider_id.0,
            "terminal.create",
            Some(crate::capability::risk::ToolRisk::Write),
        );
        assert_eq!(
            outcome,
            PermissionOutcome::Deny,
            "ACP without policy must default to Deny (no hang)"
        );
    }
}
