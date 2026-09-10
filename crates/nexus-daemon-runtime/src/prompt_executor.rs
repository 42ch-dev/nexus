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
    creation_locks: std::sync::Mutex<HashMap<HostSessionKey, Arc<tokio::sync::Mutex<()>>>>,
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
        let mut locks = self
            .op_locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let expected_step = record.state.as_ref().and_then(|s| s.step_in_flight.clone());
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
    #[allow(clippy::too_many_arguments)] // each argument is a distinct durable field of the attempt write; grouping into one struct would hide field provenance
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
                other => CapabilityError::Internal(format!("persist prompt attempt: {other}")),
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
                return Ok((session.id.clone(), session.process_identity.clone(), false));
            }
        }

        // Get or create the per-key serialization lock.
        let key_lock = {
            let mut locks = self
                .creation_locks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
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
                return Ok((session.id.clone(), session.process_identity.clone(), false));
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
                CapabilityError::TransientExternal(format!("host session creation failed: {e}"))
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
    async fn cleanup_created_session(&self, key: &HostSessionKey, created_by_us: bool) {
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
            .is_ok_and(|r| r.is_ok());
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

    /// Test-only: seed a cached Host session for a `(run, role)` key so the
    /// real `finalize_run` path can be exercised against a scripted Host
    /// facade without driving a full prompt operation (Finding 3, round 3).
    #[cfg(test)]
    pub(crate) async fn seed_cached_session_for_test(
        &self,
        run_id: &str,
        role: &str,
        session_id: nexus_agent_host::HostSessionId,
    ) {
        self.sessions.write().await.insert(
            HostSessionKey {
                run_id: run_id.to_string(),
                role: role.to_string(),
            },
            CachedHostSession {
                id: session_id,
                process_identity: None,
            },
        );
    }
}

#[async_trait]
impl PromptExecutor for HostPromptExecutor {
    #[allow(clippy::too_many_lines)] // the full prompt lifecycle is one sequential flow; per-phase helpers would re-enter shared state
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

        let role = request
            .agent_ref
            .clone()
            .unwrap_or_else(|| "default".to_string());
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
        let permission_scope = if let Some(scope) = request.tool_policy.permission_scope() {
            Some(nexus_agent_host::capability::model::PromptPermissionScope {
                allow_read: scope.allow_read,
                allow_write: scope.allow_write,
                allow_destructive: scope.allow_destructive,
            })
        } else {
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
        };
        let cancel_at_exec = request.cancellation.clone();
        let stream = tokio::select! {
            biased;
            () = cancel_at_exec.cancelled() => {
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
        .map_err(|e| CapabilityError::TransientExternal(format!("host exec failed: {e}")))?;

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
        let mut cancel_unconfirmed = false;
        let terminal = loop {
            tokio::select! {
                biased;
                () = cancel.cancelled(), if !cancel_triggered => {
                    cancel_triggered = true;
                    // P1 (rereview-4): the Host cancel request is bounded by
                    // the configured shutdown timeout. A provider that fails
                    // or blocks `cancel` must not leave `execute` (and the
                    // engine's `finalize_run`) waiting indefinitely: on
                    // timeout/error the cooperative drain is abandoned and
                    // the run proceeds to the bounded owned-session/process
                    // cleanup path below, retaining an honest
                    // `Interrupted` disposition when cleanup cannot be
                    // confirmed — never a false successful cancellation.
                    let cancel_result = tokio::time::timeout(
                        self.timeouts.shutdown_duration(),
                        self.host.cancel(op_id.clone()),
                    )
                    .await;
                    match cancel_result {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => {
                            tracing::warn!(
                                run_id = %request.run_id,
                                op_id = %op_id,
                                error = %e,
                                "host cancel returned an error; abandoning cooperative drain"
                            );
                            cancel_unconfirmed = true;
                            break Err(CapabilityError::Cancelled);
                        }
                        Err(_) => {
                            tracing::warn!(
                                run_id = %request.run_id,
                                op_id = %op_id,
                                "host cancel timed out; abandoning cooperative drain"
                            );
                            cancel_unconfirmed = true;
                            break Err(CapabilityError::Cancelled);
                        }
                    }
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
        //
        //    P1 (rereview-4): the post-cancel stream drain is bounded by the
        //    same shutdown timeout. A provider that ignores cancellation and
        //    never emits a terminal event must not leave `execute` (and the
        //    engine's `finalize_run`) waiting indefinitely: the drain is
        //    abandoned on timeout and the run proceeds to the bounded
        //    owned-session/process cleanup path, retaining an honest
        //    `Interrupted` disposition when cleanup cannot be confirmed —
        //    never a false successful cancellation.
        if request.cancellation.is_cancelled() {
            if !cancel_unconfirmed {
                let drain_result = tokio::time::timeout(self.timeouts.shutdown_duration(), async {
                    let mut drain = std::pin::pin!(stream);
                    while let Some(event) = drain.next().await {
                        if let Ok(HostEvent::OpFinished(_) | HostEvent::OpFailed(_)) = event {
                            break;
                        }
                    }
                })
                .await;
                if drain_result.is_err() {
                    tracing::warn!(
                        run_id = %request.run_id,
                        "post-cancel stream drain timed out; abandoning cooperative drain"
                    );
                }
            }
            let confirmed = tokio::time::timeout(
                self.timeouts.shutdown_duration(),
                self.host.shutdown_session(host_session_id.clone()),
            )
            .await
            .is_ok_and(|r| r.is_ok());
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

// ---------------------------------------------------------------------------
// T2 fix round 3 — Finding 3: real HostPromptExecutor cancellation/reap
// proof with a production-shaped Host/ACP fixture
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_agent_host::capability::model::{
        CreateSessionRequest, FinishReason, HostEvent, HostEventStream, HostHealth, HostOperation,
        HostStartConfig, OperationFinishedEvent, SessionOwner,
    };
    use nexus_agent_host::{HostFacade, HostResult, HostSession, ProviderCatalog, SessionState};
    use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
    use nexus_orchestration::OrchestrationEngine;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    /// `graph_flow::SessionStorage` — the storage trait the engine consumes
    /// for root snapshots (the test executor's SQLite store implements it).
    use graph_flow::SessionStorage;

    /// Production-shaped scripted Host facade: creates real `HostSession`
    /// rows, records cancel/shutdown effects, and can fail shutdowns
    /// deterministically (Finding 3 — the executor's `finalize_run` must
    /// reach the Host cancel/shutdown/reap effects, not a no-op fake).
    struct ScriptedHost {
        sessions: Mutex<HashMap<nexus_agent_host::HostSessionId, HostSession>>,
        creates: AtomicU64,
        cancels: AtomicU64,
        shutdowns: AtomicU64,
        fail_shutdown_remaining: AtomicU64,
        /// Non-cooperative cancel hook (P1, round 5): when set, `cancel`
        /// blocks forever — the executor's bounded cancel must still return
        /// within the shutdown timeout and never report successful
        /// cancellation without confirmed cleanup.
        block_cancel: AtomicU64,
        /// Non-cooperative shutdown hook (P1, round 5): when set,
        /// `shutdown_session` always fails — cleanup can never be confirmed,
        /// so the run must stay `Interrupted`, never falsely `Cancelled`.
        fail_shutdown_forever: AtomicU64,
        /// Active operations: op id → (session id, release signal). `exec`
        /// parks the returned stream until the operation is cancelled or
        /// released, so a real `HostPromptExecutor::execute` drains a
        /// deterministic blocking ACP-shaped stream (Finding 3, round 4).
        active_ops: Mutex<
            HashMap<
                nexus_agent_host::HostOperationId,
                (
                    nexus_agent_host::HostSessionId,
                    tokio::sync::oneshot::Sender<()>,
                ),
            >,
        >,
    }

    impl ScriptedHost {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                sessions: Mutex::new(HashMap::new()),
                creates: AtomicU64::new(0),
                cancels: AtomicU64::new(0),
                shutdowns: AtomicU64::new(0),
                fail_shutdown_remaining: AtomicU64::new(0),
                block_cancel: AtomicU64::new(0),
                fail_shutdown_forever: AtomicU64::new(0),
                active_ops: Mutex::new(HashMap::new()),
            })
        }

        fn fail_next_shutdowns(&self, count: u64) {
            self.fail_shutdown_remaining.store(count, Ordering::SeqCst);
        }

        /// Make `cancel` block forever (non-cooperative provider).
        fn block_cancel_forever(&self) {
            self.block_cancel.store(1, Ordering::SeqCst);
        }

        /// Make `shutdown_session` always fail (cleanup can never confirm).
        fn fail_shutdown_forever(&self) {
            self.fail_shutdown_forever.store(1, Ordering::SeqCst);
        }

        /// Register a Host session row the host would have created (the
        /// executor's cached entry must correspond to a real owned Host
        /// session for `shutdown_session` to confirm the reap).
        fn seed_session(&self, id: nexus_agent_host::HostSessionId) {
            let session = HostSession {
                id: id.clone(),
                provider_id: nexus_agent_host::ProviderId::new("scripted"),
                state: SessionState::Ready,
                created_at: chrono::Utc::now(),
                active_op_id: None,
                negotiated_capabilities:
                    nexus_agent_host::capability::model::CapabilityDescriptor::native_cli_limited(),
                owner: SessionOwner {
                    creator_id: "test-creator".to_string(),
                    workspace_root: std::path::PathBuf::from("/tmp/test-workspace"),
                    orchestration_run_id: None,
                },
                process_identity: None,
            };
            self.sessions.lock().expect("sessions").insert(id, session);
        }

        fn session_count(&self) -> usize {
            self.sessions.lock().expect("sessions").len()
        }

        /// Number of currently parked (active) operations.
        fn active_op_count(&self) -> usize {
            self.active_ops.lock().expect("active_ops").len()
        }
    }

    #[async_trait]
    impl HostFacade for ScriptedHost {
        async fn start(&self, _config: HostStartConfig) -> HostResult<()> {
            Ok(())
        }

        async fn create_session(&self, request: CreateSessionRequest) -> HostResult<HostSession> {
            self.creates.fetch_add(1, Ordering::SeqCst);
            let session = HostSession {
                id: nexus_agent_host::HostSessionId::new(),
                provider_id: request.provider_id,
                state: SessionState::Ready,
                created_at: chrono::Utc::now(),
                active_op_id: None,
                negotiated_capabilities:
                    nexus_agent_host::capability::model::CapabilityDescriptor::native_cli_limited(),
                owner: request.owner,
                process_identity: None,
            };
            self.sessions
                .lock()
                .expect("sessions")
                .insert(session.id.clone(), session.clone());
            Ok(session)
        }

        async fn exec(
            &self,
            session_id: nexus_agent_host::HostSessionId,
            op: HostOperation,
        ) -> HostResult<HostEventStream> {
            let op_id = match &op {
                HostOperation::Prompt { op_id, .. } => op_id.clone(),
                HostOperation::SetModel { .. } | HostOperation::SetMode { .. } => {
                    return Err(nexus_agent_host::HostError::internal("unused"));
                }
            };
            // Park the operation: the returned stream yields nothing until
            // the operation is cancelled (the executor's out-of-band Host
            // cancel) or released by the test. This is the deterministic
            // blocking ACP-shaped stream the executor drains concurrently
            // with the coordinator cancellation token (Finding 3, round 4).
            let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
            self.active_ops
                .lock()
                .expect("active_ops")
                .insert(op_id.clone(), (session_id.clone(), release_tx));
            let stream = futures_util::stream::once(async move {
                let _ = release_rx.await;
                Ok(HostEvent::OpFinished(OperationFinishedEvent {
                    session_id,
                    op_id,
                    reason: FinishReason::Cancelled,
                }))
            });
            Ok(Box::pin(stream))
        }

        async fn cancel(&self, op_id: nexus_agent_host::HostOperationId) -> HostResult<()> {
            self.cancels.fetch_add(1, Ordering::SeqCst);
            // Non-cooperative provider hook (P1, round 5): block forever —
            // the executor's bounded cancel must still return within the
            // shutdown timeout.
            if self.block_cancel.load(Ordering::SeqCst) > 0 {
                std::future::pending::<()>().await;
            }
            // Release the parked operation stream so the executor's drain
            // observes the terminal `OpFinished(Cancelled)` event — the
            // bounded stream/operation termination the cancel path awaits.
            let removed = self.active_ops.lock().expect("active_ops").remove(&op_id);
            if let Some((_, release)) = removed {
                let _ = release.send(());
            }
            Ok(())
        }

        async fn health(&self) -> HostResult<HostHealth> {
            Ok(HostHealth {
                running: true,
                active_sessions: self.sessions.lock().expect("sessions").len(),
                active_operations: 0,
            })
        }

        async fn shutdown(&self) -> HostResult<()> {
            self.sessions.lock().expect("sessions").clear();
            Ok(())
        }

        async fn shutdown_session(
            &self,
            session_id: nexus_agent_host::HostSessionId,
        ) -> HostResult<()> {
            // Non-cooperative provider hook (P1, round 5): cleanup can never
            // be confirmed — the run must stay `Interrupted`, never falsely
            // `Cancelled`.
            if self.fail_shutdown_forever.load(Ordering::SeqCst) > 0 {
                return Err(nexus_agent_host::HostError::internal(
                    "injected permanent shutdown failure",
                ));
            }
            let remaining = self.fail_shutdown_remaining.load(Ordering::SeqCst);
            if remaining > 0 {
                self.fail_shutdown_remaining
                    .store(remaining.saturating_sub(1), Ordering::SeqCst);
                return Err(nexus_agent_host::HostError::internal(
                    "injected shutdown failure",
                ));
            }
            self.shutdowns.fetch_add(1, Ordering::SeqCst);
            self.sessions
                .lock()
                .expect("sessions")
                .remove(&session_id)
                .ok_or_else(|| nexus_agent_host::HostError::internal("session not found"))?;
            Ok(())
        }

        async fn list_sessions(&self) -> HostResult<Vec<HostSession>> {
            Ok(self
                .sessions
                .lock()
                .expect("sessions")
                .values()
                .cloned()
                .collect())
        }

        async fn provider_catalog(&self) -> HostResult<ProviderCatalog> {
            Ok(ProviderCatalog::new())
        }

        fn subscribe_events(
            &self,
            _session_id: nexus_agent_host::HostSessionId,
        ) -> tokio::sync::broadcast::Receiver<HostEvent> {
            let (tx, _) = tokio::sync::broadcast::channel(16);
            tx.subscribe()
        }
    }

    /// Build a real `HostPromptExecutor` over the scripted Host with a
    /// real SQLite workflow store (production-shaped).
    async fn test_executor(
        host: Arc<ScriptedHost>,
    ) -> (
        Arc<HostPromptExecutor>,
        Arc<SqliteSessionStorage>,
        Arc<dyn SessionStorage>,
        Arc<sqlx::SqlitePool>,
        tempfile::NamedTempFile,
    ) {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let executor = Arc::new(HostPromptExecutor::new(
            host,
            sqlite.clone(),
            nexus_agent_host::config::TimeoutConfig::default(),
        ));
        (executor, sqlite, storage, pool, db)
    }

    /// Deterministic mark-step-wins cancellation proof (Finding 3): a real
    /// `HostPromptExecutor` with a cached owned Host session for the run is
    /// wired into the engine; `mark_step_in_flight` wins immediately before
    /// Cancel; the cancel path fires the token, reaches the executor's
    /// `finalize_run`, and the Host shutdown effect is observed. The run
    /// settles Cancelled.
    #[tokio::test]
    async fn real_executor_cancel_reaches_host_shutdown() {
        let host = ScriptedHost::new();
        let (executor, sqlite, storage, _pool, _db) = test_executor(host.clone()).await;
        let store: Arc<dyn nexus_orchestration::run_state::WorkflowStateStore> = sqlite.clone();

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), session_cancels.clone());

        let graph = Arc::new(graph_flow::Graph::new("real-cancel"));
        let graph = Arc::new(
        graph_flow::GraphBuilder::new("reap-retry")
            .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
            .build()
            .expect("test graph build"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Seed a cached owned Host session for the run (production-shaped:
        // the executor's session map holds a real Host session row).
        let host_session_id = nexus_agent_host::HostSessionId::new();
        host.seed_session(host_session_id.clone());
        executor
            .seed_cached_session_for_test(&session_id.0, "default", host_session_id.clone())
            .await;

        // mark_step_in_flight wins immediately before Cancel (revision R →
        // R+1): the cancel fence must reload, re-fence, and still reach the
        // Host teardown.
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let pre_step_root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let in_flight_state = nexus_orchestration::run_state::RunStateV1 {
            step_in_flight: Some(pre_step_root.current_task_id.clone()),
            ..nexus_orchestration::run_state::RunStateV1::default()
        };
        store
            .mark_step_in_flight(
                &session_id,
                record.state_revision,
                Some(record.graph_version),
                nexus_orchestration::run_state::RunCheckpoint {
                    root: &pre_step_root,
                    children: &[],
                },
                &in_flight_state,
            )
            .await
            .expect("mark_step_in_flight wins the CAS");

        // Cancel: the fence reloads, re-fences, fires the token, and the
        // real executor's finalize_run shuts down the owned Host session.
        engine
            .signal(
                &session_id,
                nexus_orchestration::engine::EngineSignal::Cancel,
            )
            .await
            .expect("cancel must settle cancelled");
        assert_eq!(
            host.shutdowns.load(Ordering::SeqCst),
            1,
            "the Host shutdown effect must be observed exactly once"
        );
        assert_eq!(
            host.session_count(),
            0,
            "the owned Host session must be reaped"
        );
        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            final_record.status,
            nexus_orchestration::engine::SessionStatus::Cancelled
        );
    }

    /// Active Host operation cancellation/reap proof (Finding 3, round 4):
    /// a REAL `HostPromptExecutor::execute` runs against a deterministic
    /// blocking Host/ACP stream. `mark_step_in_flight` wins immediately
    /// before Cancel; the cancel fires the coordinator token out-of-band;
    /// the executor observes it, calls `HostFacade::cancel`, drains the
    /// stream to its terminal event, shuts down the owned Host session, and
    /// returns `Cancelled`. The engine's cancel path then confirms the reap
    /// through `finalize_run` and settles the run `Cancelled`. This proves
    /// the load-bearing acceptance contract: an operation made active
    /// immediately before Cancel receives out-of-band Host cancellation,
    /// drains/terminates, and is then reaped.
    #[tokio::test]
    #[allow(clippy::too_many_lines)] // full cancel-path integration scenario in one linear flow
    async fn real_executor_active_operation_cancel_reaches_host_and_reaps() {
        let host = ScriptedHost::new();
        let (executor, sqlite, storage, pool, _db) = test_executor(host.clone()).await;
        let store: Arc<dyn nexus_orchestration::run_state::WorkflowStateStore> = sqlite.clone();

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), session_cancels.clone());

        let graph = Arc::new(graph_flow::Graph::new("active-cancel"));
        let graph = Arc::new(
        graph_flow::GraphBuilder::new("reap-retry")
            .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
            .build()
            .expect("test graph build"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Freeze a `default` provider binding into the v1 descriptor so the
        // real executor's `resolve_run_metadata` admits the prompt (the
        // engine start seam leaves bindings empty).
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let mut descriptor = record.descriptor.clone().expect("v1 descriptor present");
        descriptor.agent_bindings.insert(
            "default".to_string(),
            nexus_orchestration::run_state::AgentBinding {
                provider_id: "scripted".to_string(),
                model: None,
            },
        );
        let descriptor_bytes = serde_json::to_vec(&descriptor).expect("serialize descriptor");
        sqlx::query(
            "UPDATE orchestration_sessions SET run_descriptor_json = ? WHERE session_id = ?",
        )
        .bind(&descriptor_bytes)
        .bind(&session_id.0)
        .execute(&*pool)
        .await
        .expect("patch descriptor binding");

        // mark_step_in_flight wins immediately before Cancel (revision R →
        // R+1): the step is durably in flight when the prompt admits and
        // when the cancel lands.
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let pre_step_root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let in_flight_state = nexus_orchestration::run_state::RunStateV1 {
            step_in_flight: Some(pre_step_root.current_task_id.clone()),
            ..nexus_orchestration::run_state::RunStateV1::default()
        };
        store
            .mark_step_in_flight(
                &session_id,
                record.state_revision,
                Some(record.graph_version),
                nexus_orchestration::run_state::RunCheckpoint {
                    root: &pre_step_root,
                    children: &[],
                },
                &in_flight_state,
            )
            .await
            .expect("mark_step_in_flight wins the CAS");

        // Drive a REAL execute against the deterministic blocking Host
        // stream. The operation admits (revision R+1 + step marker), the
        // Host session is created, and the Host operation parks.
        let token = {
            let cancels = session_cancels
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cancels
                .get(&session_id.0)
                .cloned()
                .expect("coordinator token registered")
        };
        let executor2 = executor.clone();
        let run_id = session_id.0.clone();
        let task_id = pre_step_root.current_task_id.clone();
        let execute_handle = tokio::spawn(async move {
            executor2
                .execute(nexus_orchestration::capability::PromptRequest {
                    run_id,
                    task_id,
                    agent_ref: None,
                    prompt: "hello".to_string(),
                    tool_policy: nexus_orchestration::capability::ToolPolicy::AutoGrantAll,
                    cancellation: token,
                })
                .await
        });

        // Wait until the Host operation is ACTIVE (the parked stream is
        // live) — the operation is made active immediately before Cancel.
        let mut attempts = 0;
        while host.active_op_count() == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            attempts += 1;
            assert!(attempts < 200, "host operation must become active");
        }
        assert_eq!(host.active_op_count(), 1, "exactly one active operation");

        // Cancel: the engine fires the coordinator token out-of-band. The
        // executor observes it, calls HostFacade::cancel, drains the stream
        // to its terminal event, shuts down the owned session, and returns
        // Cancelled; the engine's finalize_run confirms the reap and the
        // run settles Cancelled.
        engine
            .signal(
                &session_id,
                nexus_orchestration::engine::EngineSignal::Cancel,
            )
            .await
            .expect("cancel must settle cancelled");

        // The active operation received out-of-band Host cancellation.
        assert_eq!(
            host.cancels.load(Ordering::SeqCst),
            1,
            "the Host cancel effect must be observed exactly once"
        );
        assert_eq!(
            host.active_op_count(),
            0,
            "the operation stream must terminate (bounded drain)"
        );

        // The execute future terminated with the typed cancellation.
        let execute_result = execute_handle.await.expect("execute task joined");
        assert!(
            matches!(
                execute_result,
                Err(nexus_orchestration::capability::CapabilityError::Cancelled)
            ),
            "execute must return the typed cancellation, got {execute_result:?}"
        );

        // The owned Host session was shut down and reaped.
        assert_eq!(
            host.shutdowns.load(Ordering::SeqCst),
            1,
            "the Host shutdown effect must be observed exactly once"
        );
        assert_eq!(
            host.session_count(),
            0,
            "the owned Host session must be reaped"
        );

        // The run settled Cancelled with the durable cancel intent.
        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            final_record.status,
            nexus_orchestration::engine::SessionStatus::Cancelled
        );
        assert!(
            final_record
                .state
                .as_ref()
                .is_some_and(|s| s.cancel_requested),
            "cancelled run must carry cancel_requested"
        );
    }

    /// Non-cooperative Host cancellation/drain bound proof (P1, round 5):
    /// the provider BLOCKS `cancel` forever and its `shutdown_session`
    /// always fails — the exact shape the round-4 fixture could not create
    /// (its `ScriptedHost` always released the parked stream from `cancel`).
    /// The executor's bounded cancel must still return within the shutdown
    /// timeout, the engine's cancel must return within a bounded window, and
    /// the run must land `Interrupted` (cleanup unconfirmed) — never a false
    /// successful `Cancelled`.
    #[tokio::test]
    #[allow(clippy::too_many_lines)] // bounded cancel-path integration scenario in one linear flow
    async fn non_cooperative_host_cancel_is_bounded_and_never_falsely_cancelled() {
        let host = ScriptedHost::new();
        let (executor, sqlite, storage, pool, _db) = test_executor(host.clone()).await;
        let store: Arc<dyn nexus_orchestration::run_state::WorkflowStateStore> = sqlite.clone();

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), session_cancels.clone());

        let graph = Arc::new(graph_flow::Graph::new("non-cooperative-cancel"));
        let graph = Arc::new(
        graph_flow::GraphBuilder::new("reap-retry")
            .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
            .build()
            .expect("test graph build"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Freeze a `default` provider binding into the v1 descriptor so the
        // real executor's `resolve_run_metadata` admits the prompt.
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let mut descriptor = record.descriptor.clone().expect("v1 descriptor present");
        descriptor.agent_bindings.insert(
            "default".to_string(),
            nexus_orchestration::run_state::AgentBinding {
                provider_id: "scripted".to_string(),
                model: None,
            },
        );
        let descriptor_bytes = serde_json::to_vec(&descriptor).expect("serialize descriptor");
        sqlx::query(
            "UPDATE orchestration_sessions SET run_descriptor_json = ? WHERE session_id = ?",
        )
        .bind(&descriptor_bytes)
        .bind(&session_id.0)
        .execute(&*pool)
        .await
        .expect("patch descriptor binding");

        // mark_step_in_flight wins immediately before Cancel.
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let pre_step_root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let in_flight_state = nexus_orchestration::run_state::RunStateV1 {
            step_in_flight: Some(pre_step_root.current_task_id.clone()),
            ..nexus_orchestration::run_state::RunStateV1::default()
        };
        store
            .mark_step_in_flight(
                &session_id,
                record.state_revision,
                Some(record.graph_version),
                nexus_orchestration::run_state::RunCheckpoint {
                    root: &pre_step_root,
                    children: &[],
                },
                &in_flight_state,
            )
            .await
            .expect("mark_step_in_flight wins the CAS");

        // Drive a REAL execute against the deterministic blocking Host
        // stream; the Host operation parks.
        let token = {
            let cancels = session_cancels
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cancels
                .get(&session_id.0)
                .cloned()
                .expect("coordinator token registered")
        };
        let executor2 = executor.clone();
        let run_id = session_id.0.clone();
        let task_id = pre_step_root.current_task_id.clone();
        let execute_handle = tokio::spawn(async move {
            executor2
                .execute(nexus_orchestration::capability::PromptRequest {
                    run_id,
                    task_id,
                    agent_ref: None,
                    prompt: "hello".to_string(),
                    tool_policy: nexus_orchestration::capability::ToolPolicy::AutoGrantAll,
                    cancellation: token,
                })
                .await
        });

        let mut attempts = 0;
        while host.active_op_count() == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            attempts += 1;
            assert!(attempts < 200, "host operation must become active");
        }
        assert_eq!(host.active_op_count(), 1, "exactly one active operation");

        // Make the provider non-cooperative: `cancel` blocks forever and
        // `shutdown_session` always fails — cleanup can never be confirmed.
        host.block_cancel_forever();
        host.fail_shutdown_forever();

        // Cancel: the bounded cancel must return within a bounded window
        // (the configured shutdown timeout bounds the cancel request, the
        // drain is abandoned, and the failing shutdown keeps the run
        // Interrupted — never a false successful Cancelled).
        let cancel_start = std::time::Instant::now();
        let signal_result = engine
            .signal(
                &session_id,
                nexus_orchestration::engine::EngineSignal::Cancel,
            )
            .await;
        let cancel_elapsed = cancel_start.elapsed();
        assert!(
            signal_result.is_err(),
            "unconfirmed cleanup must surface the error, got {signal_result:?}"
        );
        assert!(
            cancel_elapsed < std::time::Duration::from_secs(20),
            "cancel must return within the bounded window, took {cancel_elapsed:?}"
        );

        // The Host cancel was attempted exactly once (and blocked).
        assert_eq!(
            host.cancels.load(Ordering::SeqCst),
            1,
            "the Host cancel effect must be attempted exactly once"
        );

        // The execute future terminated with the typed cancellation within
        // the bound — never stuck behind the blocking provider.
        let execute_result = execute_handle.await.expect("execute task joined");
        assert!(
            matches!(
                execute_result,
                Err(nexus_orchestration::capability::CapabilityError::Cancelled)
            ),
            "execute must return the typed cancellation, got {execute_result:?}"
        );

        // Cleanup was NOT confirmed: the owned Host session is retained for
        // a later `finalize_run` retry (I-002), never reported reaped.
        assert_eq!(
            host.shutdowns.load(Ordering::SeqCst),
            0,
            "no shutdown may be confirmed against the failing provider"
        );
        assert_eq!(
            host.session_count(),
            1,
            "the owned Host session must be retained (cleanup unconfirmed)"
        );

        // The run landed Interrupted with the durable cancel intent and the
        // unconfirmed-cleanup failure — never a false successful Cancelled.
        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            final_record.status,
            nexus_orchestration::engine::SessionStatus::Interrupted,
            "unconfirmed cleanup must keep the run Interrupted, got {:?}",
            final_record.status
        );
        assert!(
            final_record
                .state
                .as_ref()
                .is_some_and(|s| s.cancel_requested),
            "interrupted run must carry the durable cancel intent"
        );
        assert_eq!(
            final_record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .map(|f| f.code.as_str()),
            Some("cancel_cleanup_unconfirmed"),
            "the run must record the unconfirmed-cleanup failure"
        );
    }

    /// Deterministic parked child/grandchild Host session reap proof
    /// (Finding 3): child and grandchild cached Host sessions (parked at a
    /// human wait, no active operation) are reaped by the real executor's
    /// `finalize_run` through the engine's recursive descendant closure.
    #[tokio::test]
    async fn real_executor_reaps_parked_child_and_grandchild_sessions() {
        let host = ScriptedHost::new();
        let (executor, sqlite, storage, _pool, _db) = test_executor(host.clone()).await;
        let store: Arc<dyn nexus_orchestration::run_state::WorkflowStateStore> = sqlite.clone();

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), session_cancels.clone());

        let graph = Arc::new(graph_flow::Graph::new("reap-nested"));
        let graph = Arc::new(
        graph_flow::GraphBuilder::new("reap-retry")
            .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
            .build()
            .expect("test graph build"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Spawn child and grandchild.
        let inner = Arc::new(graph_flow::Graph::new("inner-a"));
        let inner = Arc::new(
        graph_flow::GraphBuilder::new("inner-a")
            .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
            .build()
            .expect("test graph build"),
        );
        let child_id = engine
            .spawn_child_session(nexus_orchestration::engine::ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        let inner2 = Arc::new(
        graph_flow::GraphBuilder::new("inner-b")
            .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
            .build()
            .expect("test graph build"),
        );
        let grandchild_id = engine
            .spawn_child_session(nexus_orchestration::engine::ChildSessionParams {
                parent_session_id: child_id.0.clone(),
                inner_graph: inner2,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn grandchild");

        // Seed cached Host sessions for the child and grandchild (parked at
        // a human wait — no active operation to observe a fired token).
        let child_host_session = nexus_agent_host::HostSessionId::new();
        let grandchild_host_session = nexus_agent_host::HostSessionId::new();
        host.seed_session(child_host_session.clone());
        host.seed_session(grandchild_host_session.clone());
        executor
            .seed_cached_session_for_test(&child_id.0, "default", child_host_session.clone())
            .await;
        executor
            .seed_cached_session_for_test(
                &grandchild_id.0,
                "default",
                grandchild_host_session.clone(),
            )
            .await;

        // Cancel: the recursive closure finalizes root, child, and
        // grandchild — every owned Host session is shut down and reaped.
        engine
            .signal(
                &session_id,
                nexus_orchestration::engine::EngineSignal::Cancel,
            )
            .await
            .expect("cancel must settle cancelled");
        assert_eq!(
            host.shutdowns.load(Ordering::SeqCst),
            2,
            "child and grandchild Host sessions must each be shut down"
        );
        assert_eq!(
            host.session_count(),
            0,
            "all owned Host sessions must be reaped"
        );
    }

    /// Deterministic failed-shutdown retry retention proof (Finding 3): the
    /// first `finalize_run` cannot confirm the child's Host shutdown → the
    /// run is `Interrupted` and the child's cached session is retained; the
    /// retry reaps it and settles Cancelled.
    #[tokio::test]
    async fn real_executor_failed_shutdown_retains_for_retry() {
        let host = ScriptedHost::new();
        let (executor, sqlite, storage, _pool, _db) = test_executor(host.clone()).await;
        let store: Arc<dyn nexus_orchestration::run_state::WorkflowStateStore> = sqlite.clone();

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let mut engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), session_cancels.clone());

        let graph = Arc::new(
        graph_flow::GraphBuilder::new("reap-retry")
            .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
            .build()
            .expect("test graph build"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let inner = Arc::new(
        graph_flow::GraphBuilder::new("inner-a")
            .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
            .build()
            .expect("test graph build"),
        );
        let child_id = engine
            .spawn_child_session(nexus_orchestration::engine::ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        // Seed a cached Host session for the child; the first shutdown fails.
        let child_host_session = nexus_agent_host::HostSessionId::new();
        host.seed_session(child_host_session.clone());
        executor
            .seed_cached_session_for_test(&child_id.0, "default", child_host_session.clone())
            .await;
        host.fail_next_shutdowns(1);

        // First cancel: the child's shutdown fails → Interrupted; the
        // child's cached session is retained for retry.
        let err = engine
            .signal(
                &session_id,
                nexus_orchestration::engine::EngineSignal::Cancel,
            )
            .await
            .expect_err("first cancel must surface the unconfirmed cleanup");
        assert!(matches!(
            err,
            nexus_orchestration::engine::EngineError::GraphFlow(_)
        ));
        let interrupted = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            interrupted.status,
            nexus_orchestration::engine::SessionStatus::Interrupted
        );
        assert_eq!(
            host.session_count(),
            1,
            "the child's Host session must be retained after the failed shutdown"
        );

        // Retry cancel: the child's shutdown succeeds → Cancelled; the
        // session is reaped.
        engine
            .signal(
                &session_id,
                nexus_orchestration::engine::EngineSignal::Cancel,
            )
            .await
            .expect("retry cancel must settle cancelled");
        assert_eq!(
            host.session_count(),
            0,
            "the child's Host session must be reaped on the retry"
        );
        assert_eq!(
            host.shutdowns.load(Ordering::SeqCst),
            1,
            "exactly one confirmed shutdown"
        );
    }
}
