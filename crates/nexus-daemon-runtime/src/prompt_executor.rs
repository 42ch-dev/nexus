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
#[derive(Debug, Clone)]
struct CachedHostSession {
    id: nexus_agent_host::HostSessionId,
    process_identity: Option<nexus_agent_host::capability::model::OwnedProcessIdentity>,
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
    sessions: tokio::sync::RwLock<HashMap<HostSessionKey, CachedHostSession>>,
    /// Per-key creation serialization (single-flight): `(run, role)` → lock.
    /// The same key's concurrent creators serialize here, re-check the
    /// session map under their own lock, and publish exactly one session;
    /// a losing duplicate is impossible by construction (only the lock
    /// holder publishes).
    creation_locks:
        std::sync::Mutex<HashMap<HostSessionKey, Arc<tokio::sync::Mutex<()>>>>,
    /// Per-run operation admission serialization (Important): one `run_id`
    /// → one mutex. A prompt operation holds this mutex from BEFORE the
    /// durable Dispatching claim through the external `exec` admission and
    /// the stream drain, so concurrent prompts for the same run cannot
    /// overlap in the Active-CAS→exec window, and a cancelled/lost-fence
    /// request cannot race a successor's launch. The future P2 coordinator
    /// shares this mechanism as its per-run operation admission lock.
    op_locks: std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
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
            op_locks: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Serialize prompt operations for one run (Important). The caller holds
    /// the returned guard from BEFORE the durable Dispatching claim through
    /// the external Host `exec` admission and the stream drain, so a second
    /// same-run prompt can never interleave in the Active-CAS→exec window or
    /// race the first operation's cancellation/cleanup. The future P2
    /// coordinator uses this same per-run admission lock to couple control
    /// signals with prompt operations (biased: whoever holds the lock wins
    /// the CAS/exec race).
    ///
    /// The lock object lives in the shared `op_locks` map; the caller keeps
    /// both the Arc and the guard alive for the operation's tenure.
    fn run_op_lock(&self, run_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.op_locks.lock().unwrap_or_else(|e| e.into_inner());
        locks
            .entry(run_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
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
    ///
    /// Durable attempt ownership: `expected_attempt_id` names THIS
    /// operation's own admission — the Dispatching write claims the empty
    /// `in_flight` slot and the Active write updates only the same
    /// operation's attempt id. A concurrent same-anchor prompt can never
    /// replace the durable in-flight operation with another attempt's ids.
    async fn persist_attempt(
        &self,
        run_id: &str,
        task_id: &str,
        host_session_id: Option<String>,
        operation_id: Option<String>,
        phase: PromptPhase,
        expected_revision: u64,
        expected_step: Option<&str>,
        attempt_id: &str,
        expected_attempt_id: Option<&str>,
        process_identity: Option<nexus_agent_host::capability::model::OwnedProcessIdentity>,
    ) -> Result<(), CapabilityError> {
        let attempt = PromptAttempt {
            attempt_id: attempt_id.to_string(),
            task_id: task_id.to_string(),
            phase,
            host_session_id,
            operation_id,
            // Convert the Host's opaque fingerprint to the orchestration
            // durable shape (identical fields; the daemon persists the
            // fingerprint without importing provider crates).
            process_identity: process_identity.map(|pi| {
                nexus_orchestration::run_state::OwnedProcessIdentity {
                    pid: pi.pid,
                    process_birth: pi.process_birth,
                    group_id: pi.group_id,
                }
            }),
        };
        self.workflow_store
            .persist_prompt_attempt(
                &SessionId(run_id.to_string()),
                expected_revision,
                expected_step,
                expected_attempt_id,
                &attempt,
            )
            .await
            .map_err(|e| match e {
                // A revision/step/ownership fence failure means a control
                // transition, a newer step, or a concurrent same-anchor
                // operation won since admission — the prompt must not
                // launch and the caller sees the same typed cancellation a
                // post-fence cancellation produces.
                EngineError::RevisionMismatch { .. }
                | EngineError::TerminalState(_)
                | EngineError::SessionNotFound(_) => CapabilityError::Cancelled,
                other => {
                    CapabilityError::Internal(format!("persist prompt attempt: {other}"))
                }
            })?;
        Ok(())
    }

    /// Clear the durable `in_flight` marker for this operation's attempt id
    /// after the operation terminated. Sequential same-run capability
    /// callers then claim an empty slot for the next operation; the engine's
    /// own `commit_transition` clears the marker for graph steps. Fence
    /// misses (marker already cleared / state advanced) are idempotent.
    async fn clear_attempt(
        &self,
        run_id: &str,
        expected_revision: u64,
        expected_step: Option<&str>,
        attempt_id: &str,
    ) {
        let _ = self
            .workflow_store
            .clear_prompt_attempt(
                &SessionId(run_id.to_string()),
                expected_revision,
                expected_step,
                attempt_id,
            )
            .await;
    }

    /// Lazily obtain the Host session for a (run, role) key (A5).
    ///
    /// Same key creation is serialized per key (single-flight): each key
    /// owns an async mutex; the lock holder re-checks the session map before
    /// creating, so two concurrent same-key requests publish exactly one
    /// owned subprocess. The session is created with the verified Creator
    /// workspace cwd and the trusted owner; a pre-cancelled request is
    /// re-checked before the external spawn.
    ///
    /// Returns `(session_id, process_identity, created_by_us)` —
    /// `created_by_us` is `true` only when THIS call published a new owned
    /// session. A post-session fence failure must shut down and evict
    /// exactly the session this request created; a pre-existing reused
    /// session is never shutdown nor evicted by a losing request (A5
    /// ownership). The opaque owned-process identity (PID + birth + group)
    /// is carried from the Host allocation so the durable `PromptAttempt`
    /// can persist it (I-002); `None` when the platform cannot establish a
    /// birth token (cleanup must then be reported unconfirmed).
    async fn host_session(
        &self,
        key: &HostSessionKey,
        creator_id: &str,
        workspace_root: &std::path::Path,
        provider_id: &str,
        model: Option<&str>,
        cancellation: &tokio_util::sync::CancellationToken,
    ) -> Result<
        (
            nexus_agent_host::HostSessionId,
            Option<nexus_agent_host::capability::model::OwnedProcessIdentity>,
            bool,
        ),
        CapabilityError,
    > {
        // Fast path: an already-published session is returned without
        // touching the per-key creation lock.
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(key) {
                return Ok((
                    session.id.clone(),
                    session.process_identity.clone(),
                    false,
                ));
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
            if let Some(session) = sessions.get(key) {
                return Ok((
                    session.id.clone(),
                    session.process_identity.clone(),
                    false,
                ));
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
        let process_identity = session.process_identity.clone();
        self.sessions.write().await.insert(
            key.clone(),
            CachedHostSession {
                id: sid.clone(),
                process_identity: process_identity.clone(),
            },
        );
        Ok((sid, process_identity, true))
    }

    /// Fail closed when a post-session fence loses (the Active ownership CAS
    /// failed, a concurrent transition won, or the run was cancelled): shut
    /// down the Host session THIS request created, then evict the session
    /// map entry so no successor can reuse a session whose owned process may
    /// be gone (A5 bounded cleanup/lifetime). A pre-existing reused session
    /// is never shutdown nor evicted by the losing request.
    ///
    /// I-002: the map entry is evicted ONLY after the shutdown is confirmed
    /// (`Ok(Ok(()))`). On timeout/error the entry is kept: the Host still
    /// owns the session, and a later `finalize_run` retry can reap it —
    /// evicting first would orphan the owned process group.
    async fn cleanup_created_session(
        &self,
        key: &HostSessionKey,
        created_by_us: bool,
    ) {
        if !created_by_us {
            return;
        }
        let sid = {
            let sessions = self.sessions.read().await;
            sessions.get(key).map(|session| session.id.clone())
        };
        if let Some(sid) = sid {
            let confirmed = tokio::time::timeout(
                self.timeouts.shutdown_duration(),
                self.host.shutdown_session(sid),
            )
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false);
            if confirmed {
                self.sessions.write().await.remove(key);
            } else {
                tracing::warn!(
                    run_id = %key.run_id,
                    role = %key.role,
                    "session shutdown unconfirmed; keeping session map entry for retry (I-002)"
                );
            }
        }
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

        // 1b. Serialize prompt operations for this run (Important). The
        //     guard is held from BEFORE the durable Dispatching claim
        //     through the external exec admission and the stream drain, so
        //     concurrent same-run prompts cannot overlap in the
        //     Active-CAS→exec window and a cancellation cannot race a
        //     successor's launch or cleanup.
        let op_lock = self.run_op_lock(&request.run_id);
        let _op_guard = op_lock.lock().await;

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

        // 3. Allocate the durable attempt id and the Host operation ID
        //    BEFORE any external session launch (A5). The Host op id is part
        //    of the durable Active intent and is ready before `exec`
        //    regardless of session creation latency; the attempt id names
        //    this operation's durable `in_flight` ownership.
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let op_id = HostOperationId::new();

        // 4. Persist the dispatching intent BEFORE the external Host effect
        //    (A2/A5), revision-fenced on the admission anchor AND CLAIMING
        //    the empty `in_flight` slot with this attempt's id (ownership
        //    CAS: the claim succeeds only while the slot is empty or already
        //    owned by this attempt): a crash after the effect but before the
        //    result checkpoint leaves the run interrupted, never
        //    auto-replayed. A control transition, a still-owning earlier
        //    operation, or a concurrent same-anchor operation that advanced
        //    the state since admission fails the CAS here.
        self.persist_attempt(
            &request.run_id,
            &request.task_id,
            None,
            None,
            PromptPhase::Dispatching,
            expected_revision,
            expected_step.as_deref(),
            &attempt_id,
            Some(&attempt_id),
            None,
        )
        .await?;

        // 5. Lazily obtain the (run, role) Host session (single-flight per
        //    key; the re-check and cancellation fence run under the key
        //    lock, immediately before the external spawn).
        let host_session_result = self
            .host_session(
                &key,
                &creator_id,
                &workspace_root,
                &provider_id,
                model.as_deref(),
                &request.cancellation,
            )
            .await;

        // 6. Mark the attempt Active with the now-known Host IDs. This write
        //    is the second revision fence, immediately before `exec`: it
        //    CASes on the same admission anchor AND on this operation's own
        //    attempt id, so a cancellation/control transition that passed
        //    the earlier fences still fails here and the prompt is never
        //    launched — and a concurrent same-anchor prompt can never
        //    replace this operation's durable marker.
        let (host_session_id, process_identity, created_by_us) = match host_session_result {
            Ok(triple) => triple,
            Err(e) => {
                // The session launch itself refused (pre-cancel re-check,
                // launch failure, EOF). Any session created by this request
                // is shut down and evicted (A5 — no leaked owned session),
                // and the durable attempt ownership + marker are dropped.
                self.clear_attempt(
                    &request.run_id,
                    expected_revision,
                    expected_step.as_deref(),
                    &attempt_id,
                )
                .await;
                return Err(e);
            }
        };

        // The fence check runs again immediately before the Active write.
        if request.cancellation.is_cancelled() {
            self.cleanup_created_session(&key, created_by_us).await;
            self.clear_attempt(
                &request.run_id,
                expected_revision,
                expected_step.as_deref(),
                &attempt_id,
            )
            .await;
            return Err(CapabilityError::Cancelled);
        }
        // The Active write CASes on the same admission anchor AND on this
        // operation's own attempt id. A failure means a concurrent
        // cancellation/control transition won (or the ownership CAS failed):
        // no prompt may launch, and the session THIS request created is shut
        // down and evicted so no successor reuses a session whose owned
        // process may be gone (A5 bounded cleanup — no leaked Host session).
        let active_fence = self
            .persist_attempt(
                &request.run_id,
                &request.task_id,
                Some(host_session_id.to_string()),
                Some(op_id.to_string()),
                PromptPhase::Active,
                expected_revision,
                expected_step.as_deref(),
                &attempt_id,
                Some(&attempt_id),
                process_identity,
            )
            .await;
        if let Err(e) = active_fence {
            self.cleanup_created_session(&key, created_by_us).await;
            self.clear_attempt(
                &request.run_id,
                expected_revision,
                expected_step.as_deref(),
                &attempt_id,
            )
            .await;
            return Err(e);
        }
        // The Active fence won: the owned session is now durably owned by
        // this request's operation; if the operation is later abandoned the
        // session stays for serial reuse, never leaked.
        let _ = created_by_us;

        // 7. Execute the prompt through the Host plane with the narrowing
        //    permission scope (A1). The orchestration scope type converts to
        //    the Host's own scope type (identical fields; the Host cannot
        //    depend on orchestration).
        //
        //    M-001: workflow prompt execution must fail closed unless it
        //    carries a narrowing scope. `RequestPolicy` maps to `None` (no
        //    narrowing scope) — for workflow prompts that would let the
        //    Host's own policy apply un-narrowed, which is not acceptable
        //    for orchestration. Standalone public Host/Character `None`
        //    semantics remain unchanged (they do not go through this
        //    executor).
        //
        //    The launch is arbitrated against the coordinator token in a
        //    biased select: if the token is already cancelled at this
        //    admission point, the prompt is REFUSED — the external launch
        //    never starts. This is the atomic cancellation-vs-exec
        //    admission at the launch boundary (the token and the exec
        //    future race; a cancel observed here deterministically wins
        //    because the biased branch is polled first). Combined with the
        //    per-run operation lock, the Active-CAS→exec window has no
        //    un-serialized launch path: a cancellation observed before
        //    admission refuses the launch; one observed during the stream
        //    cancels the owned operation and bounds cleanup (step 8-9).
        let permission_scope = match request.tool_policy.permission_scope() {
            Some(scope) => Some(nexus_agent_host::capability::model::PromptPermissionScope {
                allow_read: scope.allow_read,
                allow_write: scope.allow_write,
                allow_destructive: scope.allow_destructive,
            }),
            None => {
                // M-001: RequestPolicy (or any policy without a narrowing
                // scope) refuses for workflow prompt execution — fail
                // closed before the external Host effect.
                self.cleanup_created_session(&key, created_by_us).await;
                self.clear_attempt(
                    &request.run_id,
                    expected_revision,
                    expected_step.as_deref(),
                    &attempt_id,
                )
                .await;
                return Err(CapabilityError::Forbidden(
                    "workflow prompt requires a narrowing permission scope (RequestPolicy is not supported for orchestration prompts)"
                        .to_string(),
                ));
            }
        };
        let cancel_at_exec = request.cancellation.clone();
        let stream = tokio::select! {
            biased;
            _ = cancel_at_exec.cancelled() => {
                self.cleanup_created_session(&key, created_by_us).await;
                self.clear_attempt(
                    &request.run_id,
                    expected_revision,
                    expected_step.as_deref(),
                    &attempt_id,
                )
                .await;
                return Err(CapabilityError::Cancelled);
            }
            r = self.host.exec(
                host_session_id.clone(),
                HostOperation::Prompt {
                    op_id: op_id.clone(),
                    content: vec![HostContentBlock::Text {
                        text: request.prompt.clone(),
                    }],
                    permission_scope,
                },
            ) => r,
        }
        .map_err(|e| {
            CapabilityError::TransientExternal(format!("host exec failed: {e}"))
        })?;

        // 8. Drain the stream, collecting MessageDelta only, while listening
        //    to the coordinator cancellation token concurrently (A5).
        let mut full_text = String::new();
        let mut stream = std::pin::pin!(stream);
        let cancel = request.cancellation.clone();
        // Once the coordinator token fires, this operation is cancelled: the
        // Host operation is cancelled and the stream is drained until its
        // terminal event (bounded by provider timeouts). Consuming the
        // terminal event lets the Host's wrapped stream transition the
        // session Cancelling → Ready, so step 9's bounded shutdown can
        // remove the session — no leaked Host session.
        let mut cancel_triggered = false;
        let terminal = loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled(), if !cancel_triggered => {
                    cancel_triggered = true;
                    let _ = self.host.cancel(op_id.clone()).await;
                }
                event = stream.next() => {
                    match event {
                        Some(Ok(HostEvent::MessageDelta(delta))) => {
                            if !cancel_triggered {
                                full_text.push_str(&delta.text);
                            }
                        }
                        Some(Ok(HostEvent::OpFinished(finished))) => {
                            if cancel_triggered {
                                break Err(CapabilityError::Cancelled);
                            }
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
                        Some(Ok(HostEvent::OpFailed(_))) => {
                            if cancel_triggered {
                                break Err(CapabilityError::Cancelled);
                            }
                            break Err(CapabilityError::TransientExternal(
                                "host operation failed".to_string(),
                            ));
                        }
                        Some(Ok(_)) => {
                            // Ignore non-message events (thoughts, tool calls,
                            // status, plan updates).
                        }
                        Some(Err(e)) => {
                            if cancel_triggered {
                                break Err(CapabilityError::Cancelled);
                            }
                            break Err(CapabilityError::TransientExternal(format!(
                                "host event stream error: {e}"
                            )));
                        }
                        None => {
                            // EOF without a terminal event is a typed failure
                            // (A1: only MessageDelta + EndTurn succeeds).
                            if cancel_triggered {
                                break Err(CapabilityError::Cancelled);
                            }
                            break Err(CapabilityError::TransientExternal(
                                "host event stream closed without a terminal event".to_string(),
                            ));
                        }
                    }
                }
            }
        };

        // 9. On cancellation, bound the session cleanup through the T1 Host
        //    lifecycle (A5): shutdown drains and reaps the owned process,
        //    and the executor's session map entry is evicted so a later
        //    admission never reuses a session whose owned process is gone.
        //    I-002: the entry is evicted ONLY on a confirmed shutdown; on
        //    timeout/error the Host still owns the session, so the entry is
        //    kept for a later `finalize_run` retry to reap.
        if request.cancellation.is_cancelled() {
            let confirmed = tokio::time::timeout(
                self.timeouts.shutdown_duration(),
                self.host.shutdown_session(host_session_id.clone()),
            )
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false);
            if confirmed {
                self.sessions.write().await.remove(&key);
            } else {
                tracing::warn!(
                    run_id = %request.run_id,
                    role = %key.role,
                    "cancel-path session shutdown unconfirmed; keeping session map entry for retry (I-002)"
                );
            }
        }

        // 10. The operation terminated: drop the durable attempt ownership
        //     and clear this operation's durable `in_flight` marker so the
        //     next admitted operation of the same run claims an empty slot
        //     (sequential same-run capability callers). A successful graph
        //     step's `commit_transition` also clears the marker — this is
        //     idempotent (fence misses are benign).
        self.clear_attempt(
            &request.run_id,
            expected_revision,
            expected_step.as_deref(),
            &attempt_id,
        )
        .await;

        terminal
    }

    /// Finalize a run's owned Host sessions after the run reached a
    /// confirmed terminal state (A5 / I-001). Waits (bounded) for any
    /// in-flight operation's cleanup to complete, then shuts down and
    /// evicts every `(run, role)` session. Returns `Ok(())` only when
    /// cleanup is confirmed; `Err` means cleanup-unconfirmed (the run
    /// must remain non-terminal/actionable).
    async fn finalize_run(&self, run_id: &str) -> Result<(), CapabilityError> {
        // Serialize with any in-flight prompt operation for this run: the
        // operation's own cleanup (step 9) must complete before we shut
        // down its session, and no new operation may admit after the run
        // is terminal (the admission fence refuses terminal runs).
        let op_lock = self.run_op_lock(run_id);
        let _op_guard = op_lock.lock().await;

        // Collect every (run, role) session under the read lock. I-002:
        // entries are evicted ONLY after their shutdown is confirmed; on
        // timeout/error the Host still owns the session, so the entry is
        // kept for a later retry to reap.
        let sessions: Vec<(HostSessionKey, CachedHostSession)> = {
            let map = self.sessions.read().await;
            map.iter()
                .filter(|(k, _)| k.run_id == run_id)
                .map(|(k, session)| (k.clone(), session.clone()))
                .collect()
        };

        // Shut down each owned session with the bounded Host lifecycle.
        // A typed error or outer timeout means the exact owned process
        // cleanup was not confirmed: the session must remain visibly
        // interrupted (never marked/removed as cleanly stopped) and the
        // run must not be persisted terminal.
        let mut unconfirmed: Vec<String> = Vec::new();
        for (key, session) in sessions {
            let sid = session.id;
            match tokio::time::timeout(
                self.timeouts.shutdown_duration(),
                self.host.shutdown_session(sid.clone()),
            )
            .await
            {
                Ok(Ok(())) => {
                    self.sessions.write().await.remove(&key);
                    tracing::info!(
                        run_id = %run_id,
                        role = %key.role,
                        session_id = %sid,
                        "run-terminal Host session finalized"
                    );
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        run_id = %run_id,
                        role = %key.role,
                        session_id = %sid,
                        error = %e,
                        "run-terminal session shutdown returned error (cleanup unconfirmed; entry kept for retry)"
                    );
                    unconfirmed.push(key.role.clone());
                }
                Err(_) => {
                    tracing::warn!(
                        run_id = %run_id,
                        role = %key.role,
                        session_id = %sid,
                        "run-terminal session shutdown timed out (cleanup unconfirmed; entry kept for retry)"
                    );
                    unconfirmed.push(key.role.clone());
                }
            }
        }

        if !unconfirmed.is_empty() {
            return Err(CapabilityError::TransientExternal(format!(
                "run '{run_id}' terminal cleanup unconfirmed for role(s): {}",
                unconfirmed.join(", ")
            )));
        }
        Ok(())
    }
}
