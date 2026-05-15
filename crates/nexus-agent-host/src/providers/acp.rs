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

use nexus_acp_host::{AcpSdkAdapter, AcpStreamUpdate, NexusAcpClient};
use nexus_contracts::local::acp::{
    NexusConfigOption, NexusConfigOptionCategory, NexusContentBlock, NexusInitializeRequest,
    NexusNewSessionRequest, NexusPromptRequest, NexusSessionId, NexusSetConfigOptionRequest,
};

use crate::capability::model::{
    CapabilityDescriptor, FinishReason, HostContentBlock, HostEvent, HostEventStream,
    ManagedSessionHandle, OperationFailedEvent, OperationFinishedEvent, OperationStartedEvent,
    ProtocolKind, ProviderDescriptor, ProviderHealth, TextDeltaEvent,
};
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
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
}

impl AcpProvider {
    /// Create a new ACP provider adapter.
    ///
    /// The `client` should already have an established connection
    /// (via `AcpSdkAdapter::with_connection`).
    #[must_use]
    pub fn new(provider_id: ProviderId, display_name: String, client: AcpSdkAdapter) -> Self {
        Self {
            provider_id,
            display_name,
            client: Arc::new(client),
            sessions: Arc::new(RwLock::new(HashMap::new())),
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

        match self.client.initialize(init_request).await {
            Ok(_) => Ok(ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: true,
                latency_ms: None,
                message: None,
            }),
            Err(e) => Ok(ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: false,
                latency_ms: None,
                message: Some(format!("probe failed: {e}")),
            }),
        }
    }

    async fn launch(
        &self,
        spec: crate::capability::model::LaunchSpec,
    ) -> HostResult<ManagedSessionHandle> {
        // Create ACP session
        let acp_request = NexusNewSessionRequest::new(spec.cwd);

        let session_created = self.client.create_session(acp_request).await.map_err(|e| {
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

        // Build the prompt request
        let prompt_request = NexusPromptRequest {
            session_id: acp_session_id,
            prompt: Self::to_acp_content(&content_blocks),
        };

        // Start streaming
        let rx = self
            .client
            .stream_prompt(prompt_request)
            .await
            .map_err(|e| {
                HostError::protocol_error("ACP stream_prompt failed", Some(e.to_string()))
            })?;

        // Convert the mpsc::Receiver into a futures Stream of HostEvent
        let session_id = session.session_id.clone();
        let op_id_for_stream = op_id.clone();

        // First emit OpStarted, then forward the stream
        let stream = futures_util::stream::once({
            let op_id = op_id_for_stream.clone();
            let session_id = session_id.clone();
            async move {
                Ok(HostEvent::OpStarted(OperationStartedEvent {
                    op_id,
                    session_id,
                }))
            }
        })
        .chain({
            tokio_stream::wrappers::ReceiverStream::new(rx).map(move |update| {
                let sid = session_id.clone();
                let oid = op_id_for_stream.clone();
                Ok(Self::stream_update_to_event(update, &sid, &oid))
            })
        })
        .boxed();

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
}
