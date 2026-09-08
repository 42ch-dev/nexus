//! Daemon-owned `HostPromptExecutor` — the production `PromptExecutor` (A1).
//!
//! Bridges the orchestration prompt seam to the existing `HostFacade`:
//!
//! - Resolves the trusted frozen run metadata (creator, workspace, provider
//!   binding) from the P0 `WorkflowStateStore` descriptor — never from
//!   arbitrary context JSON.
//! - Lazily creates/reuses a Host session per `(run, role)`; the same
//!   run+role reuses its session serially, another run/Creator never does.
//! - Allocates the Host operation ID and persists the P0 `PromptAttempt`
//!   dispatch intent BEFORE the external Host effect; updates it to
//!   `Active` once the Host session/operation IDs are known.
//! - Drains the Host event stream, collects `MessageDelta` only, and accepts
//!   exactly `HostEvent::OpFinished` with `FinishReason::EndTurn`. Refusal,
//!   EOF, denial, timeout, non-EndTurn stop and cancellation are typed
//!   failures — never partial-output success.
//! - Listens to the coordinator cancellation token concurrently with the
//!   Host stream; calls `HostFacade::cancel(op_id)` then bounds the session
//!   cleanup through the T1 Host lifecycle.
//! - No provider handle ever enters graph context/checkpoints.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use nexus_agent_host::capability::model::{
    CreateSessionRequest, HostContentBlock, HostEvent, HostOperation, SessionOwner,
};
use nexus_agent_host::config::TimeoutConfig;
use nexus_agent_host::{HostFacade, HostOperationId, ProviderId};
use nexus_orchestration::capability::{
    CapabilityError, PromptExecutor, PromptRequest, PromptResult, ToolPolicy,
};
use nexus_orchestration::run_state::{PromptAttempt, PromptPhase, WorkflowStateStore};
use nexus_orchestration::SessionId;
use tokio_util::sync::CancellationToken;

/// Key for a lazily-reused Host session: the same run+role reuses its
/// session serially; another run/Creator never shares it (A5).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct HostSessionKey {
    run_id: String,
    role: String,
}

/// Production prompt executor over the existing `HostFacade` (A1).
///
/// Constructed once at daemon boot with the host facade, the durable
/// workflow store, the session storage (for context identity), and the
/// shared per-run cancellation tokens.
pub struct HostPromptExecutor {
    /// The daemon's single Host plane.
    host: Arc<dyn HostFacade>,
    /// Durable run store (P0) — trusted descriptor + prompt-attempt intent.
    workflow_store: Arc<dyn WorkflowStateStore>,
    /// Host timeouts (prompt bound + shutdown bound for cancel cleanup).
    timeouts: TimeoutConfig,
    /// Lazily-created Host sessions keyed by (run, role).
    sessions: tokio::sync::RwLock<HashMap<HostSessionKey, nexus_agent_host::HostSessionId>>,
}

impl HostPromptExecutor {
    /// Construct the executor.
    #[must_use]
    pub fn new(
        host: Arc<dyn HostFacade>,
        workflow_store: Arc<dyn WorkflowStateStore>,
        timeouts: TimeoutConfig,
    ) -> Self {
        Self {
            host,
            workflow_store,
            timeouts,
            sessions: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Resolve the trusted frozen run metadata for a run (A1/A2).
    ///
    /// Returns `(creator_id, workspace_root, provider_id, model)` from the
    /// frozen descriptor. The `default` role binding is used when the
    /// request carries no agent ref; unresolved bindings refuse before any
    /// external effect.
    async fn resolve_run_metadata(
        &self,
        run_id: &str,
        agent_ref: Option<&str>,
    ) -> Result<(String, std::path::PathBuf, String, Option<String>), CapabilityError> {
        let record = self
            .workflow_store
            .load_run(&SessionId(run_id.to_string()))
            .await
            .map_err(|e| CapabilityError::Internal(format!("load run descriptor: {e}")))?
            .ok_or_else(|| {
                CapabilityError::Internal(format!("run '{run_id}' has no durable record"))
            })?;
        let descriptor = record.descriptor.ok_or_else(|| {
            CapabilityError::Internal(format!(
                "run '{run_id}' has no frozen descriptor (legacy/unverified)"
            ))
        })?;

        let role = agent_ref.unwrap_or("default");
        let binding = descriptor.agent_bindings.get(role).ok_or_else(|| {
            CapabilityError::Forbidden(format!(
                "run '{run_id}' has no provider binding for role '{role}'"
            ))
        })?;
        Ok((
            descriptor.creator_id.clone(),
            descriptor.workspace_root.clone(),
            binding.provider_id.clone(),
            binding.model.clone(),
        ))
    }

    /// Lazily obtain the Host session for a (run, role) key (A5).
    ///
    /// The same run+role reuses its session serially; a different run or
    /// Creator never shares it. The session is created with the verified
    /// Creator workspace cwd and the trusted owner.
    async fn host_session(
        &self,
        key: &HostSessionKey,
        creator_id: &str,
        workspace_root: &std::path::Path,
        provider_id: &str,
        model: Option<&str>,
    ) -> Result<nexus_agent_host::HostSessionId, CapabilityError> {
        {
            let sessions = self.sessions.read().await;
            if let Some(sid) = sessions.get(key) {
                return Ok(sid.clone());
            }
        }

        let session = self
            .host
            .create_session(CreateSessionRequest {
                provider_id: ProviderId::new(provider_id),
                cwd: workspace_root.to_path_buf(),
                model: model.map(str::to_string),
                mode: None,
                mcp_servers: vec![],
                metadata: serde_json::Value::Null,
                owner: SessionOwner {
                    creator_id: creator_id.to_string(),
                    workspace_root: workspace_root.to_path_buf(),
                    orchestration_run_id: Some(key.run_id.clone()),
                },
            })
            .await
            .map_err(|e| {
                CapabilityError::TransientExternal(format!(
                    "host session creation failed: {e}"
                ))
            })?;

        let sid = session.session_id;
        self.sessions
            .write()
            .await
            .insert(key.clone(), sid.clone());
        Ok(sid)
    }

    /// Persist the durable dispatch intent BEFORE the external Host effect
    /// (A2/A5), then update it to `Active` once the Host IDs are known.
    async fn persist_attempt(
        &self,
        run_id: &str,
        task_id: &str,
        host_session_id: Option<String>,
        operation_id: Option<String>,
        phase: PromptPhase,
    ) -> Result<(), CapabilityError> {
        let attempt = PromptAttempt {
            attempt_id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            phase,
            host_session_id,
            operation_id,
            process_identity: None,
        };
        self.workflow_store
            .persist_prompt_attempt(&SessionId(run_id.to_string()), &attempt)
            .await
            .map_err(|e| {
                CapabilityError::Internal(format!("persist prompt attempt: {e}"))
            })
    }
}

#[async_trait]
impl PromptExecutor for HostPromptExecutor {
    async fn execute(&self, request: PromptRequest) -> Result<PromptResult, CapabilityError> {
        // 1. Resolve the trusted frozen run metadata (A1/A2) — refuses
        //    before any external effect when the binding is missing.
        let (creator_id, workspace_root, provider_id, model) = self
            .resolve_run_metadata(&request.run_id, request.agent_ref.as_deref())
            .await?;

        let role = request.agent_ref.clone().unwrap_or_else(|| "default".to_string());
        let key = HostSessionKey {
            run_id: request.run_id.clone(),
            role,
        };

        // 2. Persist the dispatching intent BEFORE the external Host effect
        //    (A2/A5): a crash after the effect but before the result
        //    checkpoint leaves the run interrupted, never auto-replayed.
        self.persist_attempt(
            &request.run_id,
            &request.task_id,
            None,
            None,
            PromptPhase::Dispatching,
        )
        .await?;

        // 3. Lazily obtain the (run, role) Host session.
        let host_session_id = self
            .host_session(
                &key,
                &creator_id,
                &workspace_root,
                &provider_id,
                model.as_deref(),
            )
            .await?;

        // 4. Allocate the Host operation ID and mark the attempt Active.
        let op_id = HostOperationId::new();
        self.persist_attempt(
            &request.run_id,
            &request.task_id,
            Some(host_session_id.to_string()),
            Some(op_id.to_string()),
            PromptPhase::Active,
        )
        .await?;

        // 5. Execute the prompt through the Host plane with the narrowing
        //    permission scope (A1). The orchestration scope type converts to
        //    the Host's own scope type (identical fields; the Host cannot
        //    depend on orchestration).
        let permission_scope = request.tool_policy.permission_scope().map(|scope| {
            nexus_agent_host::capability::model::PromptPermissionScope {
                allow_read: scope.allow_read,
                allow_write: scope.allow_write,
                allow_destructive: scope.allow_destructive,
            }
        });
        let stream = self
            .host
            .exec(
                host_session_id.clone(),
                HostOperation::Prompt {
                    op_id: op_id.clone(),
                    content: vec![HostContentBlock::Text {
                        text: request.prompt.clone(),
                    }],
                    permission_scope,
                },
            )
            .await
            .map_err(|e| {
                CapabilityError::TransientExternal(format!("host exec failed: {e}"))
            })?;

        // 6. Drain the stream, collecting MessageDelta only, while listening
        //    to the coordinator cancellation token concurrently (A5).
        let mut full_text = String::new();
        let mut terminal: Option<Result<PromptResult, CapabilityError>> = None;
        let mut stream = std::pin::pin!(stream);
        let cancel = request.cancellation.clone();

        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    // Coordinator cancellation: cancel the owned operation,
                    // then bound the session cleanup through the T1 Host
                    // lifecycle. The stream's own terminal event (or the
                    // bounded cleanup) guarantees a typed failure.
                    let _ = self.host.cancel(op_id.clone()).await;
                    terminal = Some(Err(CapabilityError::Cancelled));
                    break;
                }
                event = stream.next() => {
                    match event {
                        Some(Ok(HostEvent::MessageDelta(delta))) => {
                            full_text.push_str(&delta.text);
                        }
                        Some(Ok(HostEvent::OpFinished(finished))) => {
                            if finished.reason == nexus_agent_host::capability::model::FinishReason::EndTurn {
                                terminal = Some(Ok(PromptResult {
                                    full_text,
                                    host_session_id: host_session_id.to_string(),
                                    operation_id: op_id.to_string(),
                                }));
                            } else {
                                terminal = Some(Err(CapabilityError::TransientExternal(format!(
                                    "agent stopped with non-EndTurn reason {:?}",
                                    finished.reason
                                ))));
                            }
                            break;
                        }
                        Some(Ok(HostEvent::OpFailed(failed))) => {
                            terminal = Some(Err(CapabilityError::TransientExternal(format!(
                                "host operation failed: {} ({})",
                                failed.error_category, failed.error_message
                            ))));
                            break;
                        }
                        Some(Ok(_)) => {
                            // Ignore non-message events (thoughts, tool calls,
                            // status, plan updates).
                        }
                        Some(Err(e)) => {
                            terminal = Some(Err(CapabilityError::TransientExternal(format!(
                                "host event stream error: {e}"
                            ))));
                            break;
                        }
                        None => {
                            // EOF without a terminal event is a typed failure
                            // (A1: only MessageDelta + EndTurn succeeds).
                            terminal = Some(Err(CapabilityError::TransientExternal(
                                "host event stream closed without a terminal event".to_string(),
                            )));
                            break;
                        }
                    }
                }
            }
        }

        // 7. On cancellation, bound the session cleanup through the T1 Host
        //    lifecycle (A5): shutdown drains and reaps the owned process.
        if request.cancellation.is_cancelled() {
            let _ = tokio::time::timeout(
                self.timeouts.shutdown_duration(),
                self.host.shutdown_session(host_session_id.clone()),
            )
            .await;
        }

        terminal.ok_or_else(|| {
            CapabilityError::Internal("prompt stream ended without a terminal outcome".to_string())
        })?
    }
}
