//! Daemon-owned `HostPromptExecutor` — the production `PromptExecutor` (A1).
//!
//! Bridges the orchestration prompt seam to the existing `HostFacade`:
//!
//! - Resolves the trusted frozen run metadata (creator, workspace, provider
//!   binding) from the P0 `WorkflowStateStore` descriptor — never from
//!   arbitrary context JSON.
//! - Fences cancellation before any external effect: a pre-cancelled request
//!   (or a run already terminal / cancel-requested) refuses with a typed
//!   `Cancelled`/`RevisionMismatch` failure at admission, before the durable
//!   intent write and before the Host session launch. A second revision fence
//!   runs immediately before `exec` — the `Active` intent write CASes on the
//!   admission revision/step, so a concurrent cancellation/control transition
//!   that passed none of the earlier fences still fails the write and the
//!   prompt never launches.
//! - Lazily creates/reuses a Host session per `(run, role)`; same-run+role
//!   session creation is serialized per key (single-flight) with a re-check
//!   before publishing, so concurrent same-key requests never create distinct
//!   owned subprocesses.
//! - Allocates the Host operation ID and persists the P0 `PromptAttempt`
//!   dispatch intent BEFORE the external Host effect; updates it to
//!   `Active` once the Host session/operation IDs are known. The intent
//!   writes are revision-linearized (`state_revision = expected` CAS, plus
//!   the current step marker) so a stale prompt invocation can never
//!   overwrite state that a control transition advanced.
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
    CapabilityError, PromptExecutor, PromptRequest, PromptResult,
};
use nexus_orchestration::engine::EngineError;
use nexus_orchestration::run_state::{PromptAttempt, PromptPhase, WorkflowStateStore};
use nexus_orchestration::SessionId;

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
    /// Per-key creation serialization (single-flight): `(run, role)` → lock.
    /// The same key's concurrent creators serialize here, re-check the
    /// session map under their own lock, and publish exactly one session;
    /// a losing duplicate is impossible by construction (only the lock
    /// holder publishes).
    creation_locks:
        std::sync::Mutex<HashMap<HostSessionKey, Arc<tokio::sync::Mutex<()>>>>,
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
            creation_locks: std::sync::Mutex::new(HashMap::new()),
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

    /// Admission/cancellation fence BEFORE any external effect (A5).
    ///
    /// Loads the durable run record and refuses with a typed failure when:
    /// - the run is already terminal (a control transition won) or
    ///   `cancel_requested` (a coordinated cancellation landed before this
    ///   prompt's admission) — `Cancelled`/`RevisionMismatch`;
    /// - the coordinator token is already cancelled — `Cancelled`.
    ///
    /// Returns the captured admission anchor `(state_revision, current
    /// step marker)` used to revision-linearize the prompt-intent writes:
    /// both `Dispatching` and `Active` compare-and-set on the same revision
    /// (and, when the engine marked a step, the same `step_in_flight`), so a
    /// control transition that advances the revision between fence and
    /// launch fails the CAS and the prompt is discarded — never launched on
    /// newer state.
    async fn admission_fence(
        &self,
        run_id: &str,
        cancellation: &tokio_util::sync::CancellationToken,
    ) -> Result<(u64, Option<String>), CapabilityError> {
        if cancellation.is_cancelled() {
            return Err(CapabilityError::Cancelled);
        }
        let record = self
            .workflow_store
            .load_run(&SessionId(run_id.to_string()))
            .await
            .map_err(|e| CapabilityError::Internal(format!("load run for admission: {e}")))?
            .ok_or_else(|| {
                CapabilityError::Internal(format!("run '{run_id}' has no durable record"))
            })?;
        if record.status.is_terminal() {
            return Err(CapabilityError::Cancelled);
        }
        if record.state.as_ref().is_some_and(|s| s.cancel_requested) {
            return Err(CapabilityError::Cancelled);
        }
        let expected_step = record
            .state
            .as_ref()
            .and_then(|s| s.step_in_flight.clone());
        Ok((record.state_revision, expected_step))
    }

    /// Persist the durable dispatch intent BEFORE the external Host effect
    /// (A2/A5), then update it to `Active` once the Host IDs are known.
    ///
    /// Revision-linearized: the write CASes on the admission anchor
    /// (`expected_revision`, plus the current step marker when the engine
    /// marked one) WITHOUT advancing the revision, so the engine's step
    /// transition CAS anchor is preserved while a stale prompt invocation
    /// can never overwrite state a control transition advanced.
    async fn persist_attempt(
        &self,
        run_id: &str,
        task_id: &str,
        host_session_id: Option<String>,
        operation_id: Option<String>,
        phase: PromptPhase,
        expected_revision: u64,
        expected_step: Option<&str>,
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
            .persist_prompt_attempt(
                &SessionId(run_id.to_string()),
                expected_revision,
                expected_step,
                &attempt,
            )
            .await
            .map_err(|e| match e {
                // A revision/step/status fence failure means a control
                // transition or a newer step won since admission — the
                // prompt must not launch and the caller sees the same typed
                // cancellation a post-fence cancellation produces.
                EngineError::RevisionMismatch { .. }
                | EngineError::TerminalState(_)
                | EngineError::SessionNotFound(_) => CapabilityError::Cancelled,
                other => {
                    CapabilityError::Internal(format!("persist prompt attempt: {other}"))
                }
            })?;
        Ok(())
    }

    /// Lazily obtain the Host session for a (run, role) key (A5).
    ///
    /// Same key creation is serialized per key (single-flight): each key
    /// owns an async mutex; the lock holder re-checks the session map before
    /// creating, so two concurrent same-key requests publish exactly one
    /// owned subprocess. The session is created with the verified Creator
    /// workspace cwd and the trusted owner; a pre-cancelled request is
    /// re-checked before the external spawn.
    async fn host_session(
        &self,
        key: &HostSessionKey,
        creator_id: &str,
        workspace_root: &std::path::Path,
        provider_id: &str,
        model: Option<&str>,
        cancellation: &tokio_util::sync::CancellationToken,
    ) -> Result<nexus_agent_host::HostSessionId, CapabilityError> {
        // Fast path: an already-published session is returned without
        // touching the per-key creation lock.
        {
            let sessions = self.sessions.read().await;
            if let Some(sid) = sessions.get(key) {
                return Ok(sid.clone());
            }
        }

        // Get or create the per-key serialization lock.
        let key_lock = {
            let mut locks = self
                .creation_locks
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            locks
                .entry(key.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _guard = key_lock.lock().await;

        // Re-check after acquiring the per-key lock: the first creator
        // published while we waited, or the request was cancelled in the
        // window — either way no second spawn.
        {
            let sessions = self.sessions.read().await;
            if let Some(sid) = sessions.get(key) {
                return Ok(sid.clone());
            }
        }
        if cancellation.is_cancelled() {
            return Err(CapabilityError::Cancelled);
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

        let sid = session.id;
        self.sessions
            .write()
            .await
            .insert(key.clone(), sid.clone());
        Ok(sid)
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

        // 2. Admission/cancellation fence BEFORE any external effect (A5):
        //    a pre-cancelled request, a terminal run, or a run with
        //    `cancel_requested` refuses here — no durable intent write, no
        //    Host session launch, no prompt. The returned anchor
        //    (revision + step marker) linearizes the intent persistence
        //    with the engine's control transitions.
        let (expected_revision, expected_step) = self
            .admission_fence(&request.run_id, &request.cancellation)
            .await?;

        let role = request.agent_ref.clone().unwrap_or_else(|| "default".to_string());
        let key = HostSessionKey {
            run_id: request.run_id.clone(),
            role,
        };

        // 3. Persist the dispatching intent BEFORE the external Host effect
        //    (A2/A5), revision-fenced on the admission anchor: a crash after
        //    the effect but before the result checkpoint leaves the run
        //    interrupted, never auto-replayed. A control transition that
        //    advanced the revision since admission fails the CAS here.
        self.persist_attempt(
            &request.run_id,
            &request.task_id,
            None,
            None,
            PromptPhase::Dispatching,
            expected_revision,
            expected_step.as_deref(),
        )
        .await?;

        // 4. Allocate the Host operation ID BEFORE any external session
        //    launch (A5): the ID is part of the durable Active intent and is
        //    ready before `exec` regardless of session creation latency.
        let op_id = HostOperationId::new();

        // 5. Lazily obtain the (run, role) Host session (single-flight per
        //    key; the re-check and cancellation fence run under the key
        //    lock, immediately before the external spawn).
        let host_session_id = self
            .host_session(
                &key,
                &creator_id,
                &workspace_root,
                &provider_id,
                model.as_deref(),
                &request.cancellation,
            )
            .await?;

        // 6. Mark the attempt Active with the now-known Host IDs. This write
        //    is the second revision fence, immediately before `exec`: it
        //    CASes on the same admission anchor, so a cancellation/control
        //    transition that passed the earlier fences still fails here and
        //    the prompt is never launched.
        self.persist_attempt(
            &request.run_id,
            &request.task_id,
            Some(host_session_id.to_string()),
            Some(op_id.to_string()),
            PromptPhase::Active,
            expected_revision,
            expected_step.as_deref(),
        )
        .await?;

        // 7. Execute the prompt through the Host plane with the narrowing
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

        // 8. Drain the stream, collecting MessageDelta only, while listening
        //    to the coordinator cancellation token concurrently (A5).
        let mut full_text = String::new();
        let mut stream = std::pin::pin!(stream);
        let cancel = request.cancellation.clone();
        let terminal = loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    // Coordinator cancellation: cancel the owned operation,
                    // then bound the session cleanup through the T1 Host
                    // lifecycle. The stream's own terminal event (or the
                    // bounded cleanup) guarantees a typed failure.
                    let _ = self.host.cancel(op_id.clone()).await;
                    break Err(CapabilityError::Cancelled);
                }
                event = stream.next() => {
                    match event {
                        Some(Ok(HostEvent::MessageDelta(delta))) => {
                            full_text.push_str(&delta.text);
                        }
                        Some(Ok(HostEvent::OpFinished(finished))) => {
                            if finished.reason == nexus_agent_host::capability::model::FinishReason::EndTurn {
                                break Ok(PromptResult {
                                    full_text,
                                    host_session_id: host_session_id.to_string(),
                                    operation_id: op_id.to_string(),
                                });
                            }
                            break Err(CapabilityError::TransientExternal(format!(
                                "agent stopped with non-EndTurn reason {:?}",
                                finished.reason
                            )));
                        }
                        Some(Ok(HostEvent::OpFailed(failed))) => {
                            break Err(CapabilityError::TransientExternal(format!(
                                "host operation failed: {} ({})",
                                failed.error_category, failed.error_message
                            )));
                        }
                        Some(Ok(_)) => {
                            // Ignore non-message events (thoughts, tool calls,
                            // status, plan updates).
                        }
                        Some(Err(e)) => {
                            break Err(CapabilityError::TransientExternal(format!(
                                "host event stream error: {e}"
                            )));
                        }
                        None => {
                            // EOF without a terminal event is a typed failure
                            // (A1: only MessageDelta + EndTurn succeeds).
                            break Err(CapabilityError::TransientExternal(
                                "host event stream closed without a terminal event".to_string(),
                            ));
                        }
                    }
                }
            }
        };

        // 9. On cancellation, bound the session cleanup through the T1 Host
        //    lifecycle (A5): shutdown drains and reaps the owned process.
        if request.cancellation.is_cancelled() {
            let _ = tokio::time::timeout(
                self.timeouts.shutdown_duration(),
                self.host.shutdown_session(host_session_id.clone()),
            )
            .await;
        }

        terminal
    }
}
