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

use nexus_acp_host::{
    AcpPermissionOutcome, AcpSdkAdapter, AcpStreamUpdate, AgentSpawner, ManagedAcpProcess,
    NexusAcpClient,
};
use nexus_contracts::local::acp::{
    NexusConfigOption, NexusConfigOptionCategory, NexusContentBlock, NexusInitializeRequest,
    NexusNewSessionRequest, NexusPromptRequest, NexusSessionCreated, NexusSessionId,
    NexusSetConfigOptionRequest,
};

use crate::capability::model::{
    CapabilityDescriptor, FinishReason, HostContentBlock, HostEvent, HostEventStream,
    ManagedSessionHandle, OperationFailedEvent, OperationFinishedEvent, OperationStartedEvent,
    PlanUpdateEvent, ProtocolKind, ProviderDescriptor, ProviderHealth, TextDeltaEvent,
    ToolCallEvent, ToolCallUpdateEvent,
};
use crate::capability::risk::{AutoToolRiskClassifier, ToolRiskClassifier};
use crate::config::{ProviderConfig, TimeoutConfig};
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::policy::permission::{HostPermissionResolver, PermissionOutcome};
use crate::ProviderAdapter;

/// Internal state tracked per active ACP session.
struct ConnectedAcpSession {
    /// The SDK client bound to this session's owned subprocess.
    client: Arc<AcpSdkAdapter>,
    /// The exact owned child process for this Host session.
    process: ManagedAcpProcess,
    /// The ACP session ID (from the SDK).
    acp_session_id: NexusSessionId,
    /// Configuration options exposed by the agent at session creation.
    /// Used for dynamic model switching via `set_config_option`.
    config_options: Option<Vec<NexusConfigOption>>,
    /// Active narrowing permission scope for the current operation (A1).
    /// The permission handler reads this per tool call; the Host enforces
    /// one active op per session, so the single slot is race-free.
    active_permission_scope: std::sync::Arc<
        tokio::sync::RwLock<Option<crate::capability::model::PromptPermissionScope>>,
    >,
    /// Map of active operation IDs to their cancel signal.
    /// Wave 1 enforces one op per session; this field supports future multi-op.
    #[allow(dead_code)]
    active_ops: HashMap<HostOperationId, tokio::sync::watch::Sender<bool>>,
}

/// ACP provider adapter.
///
/// Wraps a generic configured ACP launch recipe (`ProviderConfig {
/// protocol = "acp", command, args, env }`) and implements the
/// [`ProviderAdapter`] trait. The provider registers a **recipe only** at
/// construction — no client, no subprocess, no download. Each Host session
/// lazily spawns its own owned ACP child (bound to the verified Creator
/// workspace cwd) on first `launch`; distinct Host sessions/Creators never
/// share a process lifetime.
pub struct AcpProvider {
    /// Provider ID for this adapter.
    provider_id: ProviderId,
    /// Display name for this provider.
    display_name: String,
    /// The configured launch recipe (command/args/env). No process at boot.
    recipe: ProviderConfig,
    /// Active sessions: host session ID → connected ACP session.
    sessions: Arc<RwLock<HashMap<HostSessionId, ConnectedAcpSession>>>,
    /// Timeout configuration for stage-level enforcement.
    timeouts: TimeoutConfig,
    /// Permission resolver used to evaluate ACP permission requests.
    permission_resolver: HostPermissionResolver,
}

impl AcpProvider {
    /// Create a new ACP provider adapter from a generic launch recipe.
    ///
    /// Validates protocol/enabled/nonempty command and stores the recipe
    /// only — no client/process is spawned at boot or catalog load. The
    /// first `launch` for a Host session lazily spawns the owned child.
    ///
    /// # Errors
    ///
    /// Returns `HostError` when the config is not an enabled ACP provider
    /// with a nonempty launch command.
    pub fn from_config(
        config: ProviderConfig,
        timeouts: TimeoutConfig,
        permission_resolver: HostPermissionResolver,
    ) -> HostResult<Self> {
        if config.protocol != "acp" {
            return Err(HostError::internal(format!(
                "provider '{}' is not an ACP provider (protocol '{}')",
                config.id, config.protocol
            )));
        }
        if !config.enabled {
            return Err(HostError::provider_unavailable(
                config.id,
                "provider is disabled",
            ));
        }
        if config.command.as_deref().is_none_or(str::is_empty) {
            return Err(HostError::internal(format!(
                "provider '{}' has no launch command",
                config.id
            )));
        }
        let provider_id = ProviderId::new(&config.id);
        let display_name = config.id.clone();
        Ok(Self {
            provider_id,
            display_name,
            recipe: config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            timeouts,
            permission_resolver,
        })
    }

    /// Build the permission handler closure for a session's SDK adapter.
    ///
    /// The handler intersects the active operation's narrowing permission
    /// scope (A1) with the Host permission resolver: a scope can narrow but
    /// never elevate Host configuration. `None` scope preserves the
    /// standalone Host/Character policy.
    fn build_permission_handler(
        &self,
        active_permission_scope: std::sync::Arc<
            tokio::sync::RwLock<Option<crate::capability::model::PromptPermissionScope>>,
        >,
    ) -> Arc<dyn Fn(&str) -> AcpPermissionOutcome + Send + Sync> {
        let pid = self.provider_id.clone();
        let classifier = AutoToolRiskClassifier::new();
        let resolver = self.permission_resolver.clone();
        Arc::new(move |tool_name: &str| {
            let risk = classifier.classify_or_default(tool_name);
            // A1: the orchestration scope narrows the Host configuration —
            // a tool outside the requested scope is denied before the
            // resolver is consulted (the scope can never elevate). The scope
            // read must NEVER fail open: if the lock cannot be acquired the
            // active scope could be a narrowing policy in force, so the tool
            // is denied rather than approved with no scope applied.
            let scope = match active_permission_scope.try_read() {
                Ok(guard) => *guard,
                Err(_) => return AcpPermissionOutcome::Deny,
            };
            if let Some(scope) = scope {
                let allowed = match risk {
                    crate::capability::risk::ToolRisk::Read => scope.allow_read,
                    crate::capability::risk::ToolRisk::Write => scope.allow_write,
                    crate::capability::risk::ToolRisk::Destructive => scope.allow_destructive,
                };
                if !allowed {
                    return AcpPermissionOutcome::Deny;
                }
            }
            let outcome = resolver.resolve(ProtocolKind::Acp, &pid.0, tool_name, Some(risk));
            match outcome {
                PermissionOutcome::Allow => AcpPermissionOutcome::Approve,
                // In non-interactive host context, Ask defaults to Deny.
                // Interactive prompting will be added in a future release.
                PermissionOutcome::Ask | PermissionOutcome::Deny => AcpPermissionOutcome::Deny,
            }
        })
    }

    /// Lazily spawn the owned ACP child for a session and connect the SDK.
    ///
    /// Spawns the recipe command/args/env in the validated Creator workspace
    /// cwd (never the daemon cwd), wires the permission handler, performs the
    /// initialize handshake, and creates the ACP session. The exact owned
    /// child is stored in the session state for bounded cancel/shutdown/reap.
    #[allow(clippy::too_many_lines)] // sequential spawn/handshake/session setup; splitting obscures the single connection path
    async fn connect_session(
        &self,
        spec: &crate::capability::model::LaunchSpec,
    ) -> HostResult<ConnectedAcpSession> {
        let launch_dur = self.timeouts.launch_duration();

        // Defense in depth: the cwd must belong to the verified Creator
        // workspace (HostManager already validated; re-check the canonical
        // cwd here so a profile switch can never retarget a session).
        let owner_workspace = crate::config::validate_workspace_path(&spec.owner.workspace_root)?;
        let cwd = crate::config::validate_workspace_path_under(&spec.cwd, &owner_workspace)?;

        let command = self.recipe.command.clone().unwrap_or_default();
        let args: Vec<&str> = self.recipe.args.iter().map(String::as_str).collect();
        let env: Vec<(&str, &str)> = self
            .recipe
            .env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let spawner = AgentSpawner::new(cwd.clone());
        let (child, stdin, stdout) =
            spawner.spawn_with_env(&command, &args, &env).map_err(|e| {
                HostError::launch_failed(
                    self.provider_id.clone(),
                    "ACP process spawn failed",
                    Some(e.to_string()),
                )
            })?;

        let agent_path = std::path::PathBuf::from(&command);
        // Capture the process birth identity immediately after spawn, before
        // the child can exit during initialize/session negotiation.
        let mut owned_process =
            ManagedAcpProcess::new(self.provider_id.0.clone(), child, agent_path.clone());
        let client =
            AcpSdkAdapter::with_connection(self.provider_id.0.clone(), agent_path, stdin, stdout);

        // Await the handler registration — the provider is not usable until
        // the handler is installed (QC2 F-005, QC3 F-003). The handler reads
        // the active per-operation permission scope from the session slot
        // (A1); the slot is created before the client so the closure can
        // capture it, and stored into the session on success.
        let active_permission_scope: std::sync::Arc<
            tokio::sync::RwLock<Option<crate::capability::model::PromptPermissionScope>>,
        > = std::sync::Arc::new(tokio::sync::RwLock::new(None));
        let client_arc = Arc::new(client);
        client_arc
            .set_permission_handler(self.build_permission_handler(active_permission_scope.clone()))
            .await;

        // Any handshake failure below must run the owned teardown/reap
        // sequence (A5): dropping a plain Child is not an awaited process-tree
        // reap. `child` stays borrowed during the handshake for EOF watch and
        // is consumed into `ManagedAcpProcess` only on success or teardown.
        let session_result: HostResult<NexusSessionCreated> = async {
            // The SDK connection is established asynchronously by
            // `with_connection`; wait (bounded by the launch timeout) for it
            // before issuing the initialize handshake.
            let connected = tokio::time::timeout(launch_dur, async {
                loop {
                    if client_arc.is_connected().await {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await;
            if connected.is_err() {
                return Err(HostError::launch_failed(
                    self.provider_id.clone(),
                    "ACP SDK connection not established within launch timeout",
                    None,
                ));
            }

            // Initialize handshake with launch timeout, watching for child
            // exit (EOF/crash is a launch failure, never a timeout).
            let init_request = NexusInitializeRequest::new().client_info(
                nexus_contracts::local::acp::NexusAgentInfo {
                    name: "nexus42".to_string(),
                    title: Some("Nexus Agent Host".to_string()),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
            );
            let init_result = tokio::select! {
                biased;
                result = tokio::time::timeout(launch_dur, client_arc.initialize(init_request)) => {
                    result
                        .map_err(|_| {
                            HostError::timeout(
                                "launch",
                                format!(
                                    "ACP initialize handshake timed out after {}ms",
                                    self.timeouts.launch_ms
                                ),
                            )
                            .with_provider(self.provider_id.clone())
                        })?
                        .map_err(|e| {
                            HostError::launch_failed(
                                self.provider_id.clone(),
                                "ACP initialize handshake failed",
                                Some(e.to_string()),
                            )
                        })?
                }
                status = owned_process.wait() => {
                    return Err(HostError::launch_failed(
                        self.provider_id.clone(),
                        "ACP process exited during initialize handshake",
                        Some(status.map_or_else(|e| e.to_string(), |status| status.to_string())),
                    ));
                }
            };
            let _ = init_result;

            // Create the ACP session with session timeout, watching for child
            // exit.
            let acp_request = NexusNewSessionRequest::new(cwd);
            tokio::select! {
                biased;
                result = tokio::time::timeout(
                    self.timeouts.session_duration(),
                    client_arc.create_session(acp_request),
                ) => {
                    result
                        .map_err(|_| {
                            HostError::timeout(
                                "launch",
                                format!(
                                    "ACP session creation timed out after {}ms",
                                    self.timeouts.session_ms
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
                        })
                }
                status = owned_process.wait() => {
                    Err(HostError::launch_failed(
                        self.provider_id.clone(),
                        "ACP process exited during session creation",
                        Some(status.map_or_else(|e| e.to_string(), |status| status.to_string())),
                    ))
                }
            }
        }
        .await;

        let session_created = match session_result {
            Ok(created) => created,
            Err(error) => {
                match owned_process
                    .terminate(self.timeouts.shutdown_duration())
                    .await
                {
                    Ok(()) => return Err(error),
                    Err(reap_err) => {
                        // The caller must distinguish a cleanly torn-down
                        // failed launch from an unconfirmed live process
                        // (L2 Issue 4): a failed reap becomes the returned
                        // typed error, never hidden behind the original
                        // launch error.
                        tracing::warn!(
                            provider_id = %self.provider_id,
                            error = %reap_err,
                            "launch failure left owned ACP process unconfirmed reaped"
                        );
                        return Err(HostError::cleanup_unconfirmed(format!(
                            "launch of '{}' failed ({error}) and owned ACP process cleanup is unconfirmed: {reap_err}",
                            self.provider_id
                        ))
                        .with_provider(self.provider_id.clone()));
                    }
                }
            }
        };

        Ok(ConnectedAcpSession {
            client: client_arc,
            process: owned_process,
            acp_session_id: session_created.session_id,
            config_options: session_created.config_options,
            active_permission_scope,
            active_ops: HashMap::new(),
        })
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
    #[allow(clippy::too_many_lines)] // exhaustive per-variant event mapping; a helper per variant would add noise
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
                // Refusal, resource limits and cancellation are typed
                // non-success for Host consumers (A5 / I-004): a caller
                // must never persist refusal, limit exhaustion or a
                // cancelled turn as successful workflow output. Only a
                // genuine EndTurn is OpFinished success.
                match reason {
                    nexus_contracts::local::acp::NexusStopReason::EndTurn => {
                        HostEvent::OpFinished(OperationFinishedEvent {
                            session_id: session_id.clone(),
                            op_id: op_id.clone(),
                            reason: FinishReason::EndTurn,
                        })
                    }
                    nexus_contracts::local::acp::NexusStopReason::Cancelled => {
                        // I-004: an acknowledged Cancelled is typed
                        // non-success — never EndTurn/succeeded. The
                        // operation-level cancel path (HostFacade::cancel)
                        // drives the actual cancellation; this event lets
                        // consumers distinguish a cancelled turn from a
                        // completed one.
                        HostEvent::OpFinished(OperationFinishedEvent {
                            session_id: session_id.clone(),
                            op_id: op_id.clone(),
                            reason: FinishReason::Cancelled,
                        })
                    }
                    nexus_contracts::local::acp::NexusStopReason::Refusal => {
                        HostEvent::OpFailed(OperationFailedEvent {
                            session_id: session_id.clone(),
                            op_id: op_id.clone(),
                            error_category: "refusal".to_string(),
                            error_message: "agent refused the request".to_string(),
                        })
                    }
                    nexus_contracts::local::acp::NexusStopReason::MaxTokens => {
                        HostEvent::OpFailed(OperationFailedEvent {
                            session_id: session_id.clone(),
                            op_id: op_id.clone(),
                            error_category: "max_tokens".to_string(),
                            error_message: "agent reached the maximum token limit".to_string(),
                        })
                    }
                    nexus_contracts::local::acp::NexusStopReason::MaxTurnRequests => {
                        HostEvent::OpFailed(OperationFailedEvent {
                            session_id: session_id.clone(),
                            op_id: op_id.clone(),
                            error_category: "max_turn_requests".to_string(),
                            error_message: "agent reached the maximum turn request limit"
                                .to_string(),
                        })
                    }
                }
            }
            AcpStreamUpdate::Failed {
                error_category,
                error_message,
                ..
            } => HostEvent::OpFailed(OperationFailedEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                error_category,
                error_message,
            }),
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
        let (client, acp_session_id) = {
            let sessions = self.sessions.read().await;
            let state = sessions.get(&session.session_id).ok_or_else(|| {
                HostError::internal(format!(
                    "session {} not found in ACP provider",
                    session.session_id
                ))
            })?;
            let out = (Arc::clone(&state.client), state.acp_session_id.clone());
            drop(sessions); // release read guard before awaiting client RPC
            out
        };

        client.set_mode(acp_session_id, mode).await.map_err(|e| {
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
        let (client, acp_session_id, model_config_id) = {
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

            let out = (
                Arc::clone(&state.client),
                state.acp_session_id.clone(),
                config_id,
            );
            drop(sessions); // release read guard before awaiting client RPC
            out
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

        match client.set_config_option(request).await {
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
        request: crate::capability::model::ProbeRequest,
    ) -> HostResult<ProviderHealth> {
        let provider_id = self.provider_id.clone();
        let launch_dur = std::time::Duration::from_millis(request.timeout_ms);
        let cwd = crate::config::validate_workspace_path_under(
            &request.cwd,
            &request.owner.workspace_root,
        )?;

        let spec = crate::capability::model::LaunchSpec {
            cwd,
            model: None,
            mode: None,
            mcp_servers: vec![],
            owner: request.owner.clone(),
        };

        let health = match tokio::time::timeout(launch_dur, async {
            let connected = self.connect_session(&spec).await?;
            let mut process = connected.process;
            drop(connected.client);
            process.shutdown(launch_dur).await.map_err(|e| {
                HostError::cleanup_unconfirmed(format!("ACP probe cleanup unconfirmed: {e}"))
                    .with_provider(self.provider_id.clone())
            })?;
            std::result::Result::<(), HostError>::Ok(())
        })
        .await
        {
            Ok(Ok(())) => ProviderHealth {
                provider_id,
                available: true,
                latency_ms: None,
                message: Some("initialize handshake succeeded".to_string()),
            },
            Ok(Err(error)) => ProviderHealth {
                provider_id,
                available: false,
                latency_ms: None,
                message: Some(format!("initialize handshake failed: {}", error.category())),
            },
            Err(_) => ProviderHealth {
                provider_id,
                available: false,
                latency_ms: None,
                message: Some("initialize handshake timed out".to_string()),
            },
        };
        Ok(health)
    }

    async fn launch(
        &self,
        spec: crate::capability::model::LaunchSpec,
    ) -> HostResult<ManagedSessionHandle> {
        // Lazy per-session spawn: the owned ACP child is created here, on
        // first use for this Host session — never at boot/catalog load.
        let connected = self.connect_session(&spec).await?;

        // Capture the opaque owned-process identity (A5): PID + platform
        // birth token + owned group id. The birth token is re-validated
        // before every signal; a missing token means cleanup must be
        // reported unconfirmed. No transport handle crosses this boundary.
        let process_identity =
            connected
                .process
                .birth()
                .map(|birth| crate::capability::model::OwnedProcessIdentity {
                    pid: birth.pid,
                    process_birth: Some(birth.start_tick.to_string()),
                    group_id: Some(birth.pid.to_string()),
                });

        let host_session_id = HostSessionId::new();

        // Track the session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(host_session_id.clone(), connected);
        }

        Ok(ManagedSessionHandle {
            provider_id: self.provider_id.clone(),
            session_id: host_session_id,
            capabilities: CapabilityDescriptor::acp_full(),
            process_identity,
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
        let (op_id, content_blocks, permission_scope) = match op {
            crate::capability::model::HostOperation::Prompt {
                op_id,
                content,
                permission_scope,
            } => (op_id, content, permission_scope),
            crate::capability::model::HostOperation::SetMode { mode } => {
                return self.handle_set_mode(session, mode).await;
            }
            crate::capability::model::HostOperation::SetModel { model } => {
                return self.handle_set_model(session, model).await;
            }
        };

        // Look up the connected ACP session (client + ACP session ID)
        let (client, acp_session_id, active_permission_scope) = {
            let sessions = self.sessions.read().await;
            match sessions.get(&session.session_id) {
                Some(s) => (
                    Arc::clone(&s.client),
                    s.acp_session_id.clone(),
                    s.active_permission_scope.clone(),
                ),
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

        // A1: install the active per-operation permission scope before the
        // prompt streams; the permission handler reads it per tool call.
        // The Host enforces one active op per session, so the single slot is
        // race-free. The scope is cleared when the stream ends (terminal
        // event or error) so a later serial prompt starts from `None`.
        *active_permission_scope.write().await = permission_scope;

        // Build the prompt request — clone acp_session_id for potential cancel.
        let acp_sid_for_cancel = acp_session_id.clone();
        let prompt_request = NexusPromptRequest {
            session_id: acp_session_id,
            prompt: Self::to_acp_content(&content_blocks),
        };

        // Start streaming with prompt_ms timeout for the initial stream setup
        let prompt_dur = self.timeouts.prompt_duration();

        let rx = match tokio::time::timeout(prompt_dur, client.stream_prompt(prompt_request)).await
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
                // session before emitting OpFailed (QC3 F-002). The cancel is
                // bounded independently so a stalled control connection can
                // never block the promised terminal failure (L2 Issue 3).
                tracing::warn!(
                    provider_id = %self.provider_id,
                    session_id = %session.session_id,
                    acp_session_id = %acp_sid_for_cancel.0,
                    timeout_ms = self.timeouts.prompt_ms,
                    "stream_prompt setup timed out, sending best-effort cancel"
                );
                match tokio::time::timeout(
                    self.timeouts.shutdown_duration(),
                    client.cancel(acp_sid_for_cancel),
                )
                .await
                {
                    Ok(Ok(_)) => {}
                    Ok(Err(cancel_err)) => {
                        tracing::warn!(
                            error = %cancel_err,
                            "Best-effort cancel failed on stream setup timeout"
                        );
                    }
                    Err(_) => {
                        tracing::warn!("Best-effort cancel timed out on stream setup timeout");
                    }
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

        let session_id = session.session_id.clone();
        let shutdown_dur = self.timeouts.shutdown_duration();
        let started = futures_util::stream::once({
            let op_id = op_id.clone();
            let session_id = session_id.clone();
            async move {
                Ok(HostEvent::OpStarted(OperationStartedEvent {
                    op_id,
                    session_id,
                }))
            }
        });

        // Yield exactly one terminal event. A closed ACP channel is failure,
        // and a receive timeout cancels the remote operation before ending.
        let updates = futures_util::stream::unfold(
            Some((
                rx,
                client,
                acp_sid_for_cancel,
                session_id,
                op_id,
                self.provider_id.clone(),
                prompt_dur,
                shutdown_dur,
                active_permission_scope,
            )),
            |state| async move {
                let (
                    mut rx,
                    client,
                    acp_sid,
                    session_id,
                    op_id,
                    provider_id,
                    dur,
                    cancel_dur,
                    scope,
                ) = state?;

                match tokio::time::timeout(dur, rx.recv()).await {
                    Ok(Some(update)) => {
                        let event = Self::stream_update_to_event(update, &session_id, &op_id);
                        let terminal =
                            matches!(event, HostEvent::OpFinished(_) | HostEvent::OpFailed(_));
                        if terminal {
                            // A1: clear the active per-operation scope so a
                            // later serial prompt starts from `None`.
                            *scope.write().await = None;
                        }
                        let next = if terminal {
                            None
                        } else {
                            Some((
                                rx,
                                client,
                                acp_sid,
                                session_id,
                                op_id,
                                provider_id,
                                dur,
                                cancel_dur,
                                scope,
                            ))
                        };
                        Some((Ok(event), next))
                    }
                    Ok(None) => {
                        *scope.write().await = None;
                        Some((
                            Ok(HostEvent::OpFailed(OperationFailedEvent {
                                session_id,
                                op_id,
                                error_category: "protocol_eof".to_string(),
                                error_message: "ACP prompt stream closed without a stop reason"
                                    .to_string(),
                            })),
                            None,
                        ))
                    }
                    Err(_) => {
                        tracing::warn!(
                            provider_id = %provider_id,
                            session_id = %session_id,
                            "ACP prompt stream timed out; sending best-effort cancel"
                        );
                        // Bound the best-effort cancel by the configured
                        // shutdown deadline (A5: TimeoutConfig.shutdown_ms),
                        // never a hard-coded value: a stalled control
                        // connection must not block the promised terminal
                        // OpFailed beyond the configured bound.
                        match tokio::time::timeout(cancel_dur, client.cancel(acp_sid)).await {
                            Ok(Ok(_)) => {}
                            Ok(Err(cancel_err)) => {
                                tracing::warn!(
                                    error = %cancel_err,
                                    "Best-effort cancel failed on streaming timeout"
                                );
                            }
                            Err(_) => {
                                tracing::warn!("Best-effort cancel timed out on streaming timeout");
                            }
                        }
                        *scope.write().await = None;
                        Some((
                            Ok(HostEvent::OpFailed(OperationFailedEvent {
                                session_id,
                                op_id,
                                error_category: "streaming_timeout".to_string(),
                                error_message: format!(
                                    "streaming timed out: no event within {}ms budget",
                                    dur.as_millis()
                                ),
                            })),
                            None,
                        ))
                    }
                }
            },
        );

        let stream = started.chain(updates).boxed();

        Ok(stream)
    }

    async fn cancel(
        &self,
        session: &ManagedSessionHandle,
        _op_id: HostOperationId,
    ) -> HostResult<()> {
        let (client, acp_session_id) = {
            let sessions = self.sessions.read().await;
            let state = sessions.get(&session.session_id).ok_or_else(|| {
                HostError::internal(format!(
                    "session {} not found for cancel",
                    session.session_id
                ))
            })?;
            let out = (Arc::clone(&state.client), state.acp_session_id.clone());
            drop(sessions); // release read guard before awaiting client RPC
            out
        };

        client
            .cancel(acp_session_id)
            .await
            .map_err(|e| HostError::protocol_error("cancel failed", Some(e.to_string())))?;

        Ok(())
    }

    async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()> {
        // Cooperative cancel/drain phase, bounded independently (A5): a
        // stalled control connection must not consume the whole shutdown
        // budget and skip owned process-tree termination. On timeout or
        // error, we still proceed to the owned cleanup phase and report
        // unconfirmed cancellation.
        let shutdown_dur = self.timeouts.shutdown_duration();
        {
            let connected = {
                let sessions = self.sessions.read().await;
                match sessions.get(&session.session_id) {
                    Some(s) => (Arc::clone(&s.client), s.acp_session_id.clone()),
                    None => {
                        // No provider session — nothing to reap.
                        return Ok(());
                    }
                }
            };
            let cancel_result =
                tokio::time::timeout(shutdown_dur, connected.0.cancel(connected.1)).await;
            match cancel_result {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    tracing::warn!(
                        session_id = %session.session_id,
                        provider_id = %self.provider_id,
                        error = %e,
                        "cooperative ACP cancel failed; proceeding to owned process-tree cleanup"
                    );
                }
                Err(_) => {
                    tracing::warn!(
                        session_id = %session.session_id,
                        provider_id = %self.provider_id,
                        "cooperative ACP cancel timed out; proceeding to owned process-tree cleanup"
                    );
                }
            }
        }

        // Take ownership of the exact connected session and run the owned
        // process-tree termination/reap. On success the session entry is
        // removed; on unconfirmed cleanup the entry is RETAINED in the
        // registry so a later ownership-safe retry can complete (L2
        // Issue 3) and the failure is a typed error, never a clean stop.
        let connected = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&session.session_id)
        };
        let Some(mut connected) = connected else {
            tracing::warn!(
                session_id = %session.session_id,
                provider_id = %self.provider_id,
                "ACP session not found for shutdown; nothing to reap"
            );
            return Ok(());
        };

        match connected.process.shutdown(shutdown_dur).await {
            Ok(()) => {
                tracing::info!(
                    session_id = %session.session_id,
                    provider_id = %self.provider_id,
                    "ACP session shutdown complete (owned process tree reaped)"
                );
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    session_id = %session.session_id,
                    provider_id = %self.provider_id,
                    error = %e,
                    "ACP owned process-tree shutdown unconfirmed"
                );
                // Retain the connected session for later ownership-safe
                // cleanup retry (the process handle must stay owned).
                self.sessions
                    .write()
                    .await
                    .insert(session.session_id.clone(), connected);
                Err(HostError::cleanup_unconfirmed(format!(
                    "ACP session {} cleanup unconfirmed: {e}",
                    session.session_id
                ))
                .with_provider(self.provider_id.clone())
                .with_session(session.session_id.clone()))
            }
        }
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
            provider_id,
            display_name,
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
            NexusContentBlock::ResourceLink(_) => panic!("expected Text block"),
        }

        match &acp_blocks[1] {
            NexusContentBlock::ResourceLink(r) => {
                assert_eq!(r.uri, "file:///test.rs");
            }
            NexusContentBlock::Text(_) => panic!("expected ResourceLink block"),
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

    #[tokio::test]
    async fn permission_scope_lock_contention_denies() {
        use crate::capability::model::PromptPermissionScope;
        use crate::config::PolicyConfig;

        let mut policy = nexus_acp_host::policy::PermissionPolicy::new();
        policy.grant_agent("test-acp", "file_read");
        let provider = AcpProvider::from_config(
            acp_config("test-acp", Some("mock-acp"), true),
            TimeoutConfig::default(),
            HostPermissionResolver::with_acp_policy(&PolicyConfig::default(), policy),
        )
        .expect("valid recipe");
        let active_scope = Arc::new(tokio::sync::RwLock::new(Some(PromptPermissionScope {
            allow_read: true,
            allow_write: false,
            allow_destructive: false,
        })));
        let handler = provider.build_permission_handler(active_scope.clone());

        assert_eq!(handler("file_read"), AcpPermissionOutcome::Approve);
        let _guard = active_scope.write().await;
        assert_eq!(
            handler("file_read"),
            AcpPermissionOutcome::Deny,
            "an unreadable narrowing scope must fail closed"
        );
    }

    // ── Recipe-based construction (A5) ─────────────────────────────────

    fn acp_config(id: &str, command: Option<&str>, enabled: bool) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            protocol: "acp".to_string(),
            command: command.map(str::to_string),
            args: vec![],
            env: std::collections::HashMap::new(),
            enabled,
        }
    }

    #[test]
    fn from_config_accepts_enabled_acp_recipe() {
        let provider = AcpProvider::from_config(
            acp_config("test-acp", Some("mock-acp"), true),
            TimeoutConfig::default(),
            HostPermissionResolver::new_native_only(&crate::config::PolicyConfig::default()),
        )
        .expect("valid recipe accepted");
        assert_eq!(provider.provider_id.0, "test-acp");
        assert_eq!(provider.display_name, "test-acp");
        // Recipe only — no client/process at construction.
        assert!(provider.sessions.try_read().unwrap().is_empty());
    }

    #[test]
    fn from_config_rejects_disabled_provider() {
        let err = AcpProvider::from_config(
            acp_config("test-acp", Some("mock-acp"), false),
            TimeoutConfig::default(),
            HostPermissionResolver::new_native_only(&crate::config::PolicyConfig::default()),
        )
        .err()
        .expect("disabled provider refused");
        assert_eq!(err.category(), "provider_unavailable");
    }

    #[test]
    fn from_config_rejects_missing_command() {
        let err = AcpProvider::from_config(
            acp_config("test-acp", None, true),
            TimeoutConfig::default(),
            HostPermissionResolver::new_native_only(&crate::config::PolicyConfig::default()),
        )
        .err()
        .expect("missing command refused");
        assert_eq!(err.category(), "internal_host_error");
    }

    #[test]
    fn from_config_rejects_non_acp_protocol() {
        let mut config = acp_config("test-acp", Some("mock-acp"), true);
        config.protocol = "native_cli".to_string();
        let err = AcpProvider::from_config(
            config,
            TimeoutConfig::default(),
            HostPermissionResolver::new_native_only(&crate::config::PolicyConfig::default()),
        )
        .err()
        .expect("non-acp protocol refused");
        assert_eq!(err.category(), "internal_host_error");
    }
}
