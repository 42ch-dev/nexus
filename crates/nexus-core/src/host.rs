//! Core Host authority (P4-T2): a single session owner over the embedded
//! [`HostManager`], admitting Actor effects against stored ownership and
//! holding the P2 activity lease through terminal capture.
//!
//! `CoreService::open_host` composes the authority once per open;
//! `CoreService::attach_host` adopts an already-started manager instead of
//! starting a second Host, and both share the one established-owner slot. The
//! transport layers (napi addon, daemon HTTP handlers, TS service) route
//! session create/execute/query/close through [`HostHandle`] instead of
//! speaking to the `HostManager` or the provider port directly. Valid
//! Actor/viewpoint creates and `set_model`/`set_mode` operations route into
//! the real Host authority here — there is no `not_migrated` fallback.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use nexus_agent_host::capability::model::HostOperation;
use nexus_agent_host::capability::model::{
    HostEvent, HostEventStream, HostStartConfig, SessionOwner,
};
use nexus_agent_host::capability::CreateSessionRequest as HostCreateRequest;
use nexus_agent_host::config::{load_config_from_path, validate_workspace_path, AgentHostConfig};
use nexus_agent_host::core::readiness::discover_provider_catalog;
use nexus_agent_host::core::session::HostSession as RegistryHostSession;
use nexus_agent_host::discovery::path_scan;
use nexus_agent_host::providers::port::{
    operation_status_wire, protocol_kind_wire, ProviderEventReader,
};
use nexus_agent_host::{HostFacade, HostManager, HostOperationId, HostSessionId, LaunchStrategy};
use nexus_contracts::core_host_query::{CoreHostQuery, CoreHostQueryFormat, CoreHostQueryQuery};
use nexus_contracts::core_host_query_response::{
    CoreHostQueryResponse, CoreHostQueryResponseCatalog, CoreHostQueryResponseCatalogProvidersItem,
    CoreHostQueryResponseHealth, CoreHostQueryResponseScan, NexusAgentHostOperationResponse,
    NexusAgentHostSessionListResponse, NexusAgentHostSessionResponse, NexusAgentScanEntry,
    NexusPaginationInfo,
};
use nexus_contracts::generated::daemon_api::agent_host::character_operation_result::CharacterOperationResult;
// Only the `test-hooks`-gated direct-settlement seam names these, so they are
// gated with it: a production build carries no unused import
// (see `HostHandle::settle_character_terminal`).
#[cfg(any(test, feature = "test-hooks"))]
use nexus_contracts::generated::daemon_api::agent_host::character_operation_result::{
    CharacterOperationResultFinishReason, CharacterOperationResultRunStatus,
};
use nexus_contracts::generated::daemon_api::agent_host::{
    CancelOperationResponse, CreateSessionRequest, ExecuteOperationRequest, OperationResponse,
    SessionResponse, ShutdownSessionResponse,
};
use nexus_contracts::{CoreCloseReport, CoreCloseReportState, ProviderEventBatch};
use nexus_local_db::narrative_gateway::SqliteNarrativeGateway;
use nexus_local_db::SqliteKnowledgeStore;
use nexus_moment_context_assembly::{
    assemble_moment, CharacterViewInput, MomentActorContext, MomentRequest, Stage0Assembly,
    DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET,
};
use nexus_provider_ports::ProviderPort;
use nexus_spoke_adapter::SpokeBackedKbStore;
use uuid::Uuid;

use crate::actor_sessions::{
    actor_session_stale, cancelled_outcome, character_terminal_for, character_terminal_outcome,
    echo_actor_pair, ActorSessionKey, ActorSessionRegistry, CharacterOperationSnapshot,
    CharacterTerminal, KnowledgeReuse,
};
use crate::actors::{
    classify_pair, ActorPairMode, ActorViewpoint, AdmittedActor, CoreActorAdmission,
};
use crate::error::{CoreError, CoreResult};
use crate::principal::Principal;
use crate::service::CoreService;
use crate::soul::CoreCharacterMind;

impl std::fmt::Debug for HostHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately non-exhaustive without member detail: no provider,
        // catalog, or profile state leaks through Debug.
        f.debug_struct("HostHandle").finish_non_exhaustive()
    }
}

#[allow(clippy::needless_pass_by_value)] // callers move the owned payload in
fn host_err(err: nexus_agent_host::HostError) -> CoreError {
    CoreError::Internal {
        category: format!("agent_host: {err}"),
    }
}

#[allow(clippy::needless_pass_by_value)] // callers move the owned payload in
fn config_err(err: nexus_agent_host::HostError) -> CoreError {
    CoreError::Internal {
        category: format!("agent_host_config: {err}"),
    }
}

/// Lifecycle identity and admission barrier of ONE open Host authority.
///
/// Cloning a [`HostHandle`] shares this state; every open/attach mints a fresh
/// one. That identity — not the shared slot boolean — is what makes a closed
/// handle dead: after `close` it can neither act on nor observe a later
/// authority that took the established-owner slot.
///
/// The same identity carries the close/admission barrier: closing and
/// admitting are decided under one lock, and the admission count is what
/// `close` reads to prove that every operation it admitted is accounted for —
/// either failed before creating a drain or transferred that drain to the
/// registry. A drain handle alone is not that proof: an operation reaches
/// registration only after asynchronous admission and Host execution, so a
/// `close` that trusted the registered handles would release the established
/// slot while admitted work was still in flight.
struct AuthorityState {
    gate: Mutex<AuthorityGate>,
    /// The retained report of the last close attempt: a confirmed cleanup
    /// makes a repeated close idempotent, an unsettled one is re-attempted.
    close_report: Mutex<Option<CoreCloseReport>>,
    /// The admission barrier's settlement signal: every retirement wakes EVERY
    /// joiner, so a quiesce that latched behind an admitted operation can wait
    /// for the drain that operation is about to transfer instead of reading a
    /// zero drain count as quiescence — and two concurrent quiesces on clones
    /// of this authority both observe the same retirement.
    admissions_settled: tokio::sync::Notify,
    /// One-shot release of the service-wide established-owner slot for THIS
    /// authority. Shared by every clone, so two concurrent confirmed closes
    /// free the slot exactly once and the retired authority can never clear a
    /// successor's claim.
    slot_released: Mutex<bool>,
    /// Test-only: how many times this authority freed the established-owner
    /// slot. The exactly-once property is otherwise unobservable from outside
    /// (a second release writes the same `false` again), so the concurrent-close
    /// regression counts it. Absent from a production build.
    #[cfg(any(test, feature = "test-hooks"))]
    slot_release_count: std::sync::atomic::AtomicUsize,
}

/// The authority's close/admission gate.
struct AuthorityGate {
    /// Latched by the first [`HostHandle::close`]: this authority is closing
    /// and stays closed, so every later call on the handle is `closing`
    /// instead of reaching a shared slot, registry or Host.
    closing: bool,
    /// Latched by [`HostHandle::quiesce_actor_sessions`]: no further operation
    /// may be admitted against this authority. A frozen authority still serves
    /// reads — the quiesce is the Actor-side stop of the ordered close, not a
    /// retired handle — while `closing` retires the handle as well.
    admission_frozen: bool,
    /// Operations admitted against this authority and not yet retired: neither
    /// failed before creating a drain nor finished transferring one to the
    /// registry. Frozen by the closing latch (nothing can be admitted after
    /// it), then only decreasing, so the value `close` reads under the latch
    /// bounds every operation it must account for.
    in_flight: usize,
}

/// One admitted operation's slot in the authority barrier.
///
/// Retiring the admission is what lets a close confirm; the operation must
/// have transferred its drain to the registry (or failed before creating one)
/// by then, so the guard is held across the whole operation and dropped last.
struct AuthorityAdmission<'a> {
    state: &'a AuthorityState,
}

impl Drop for AuthorityAdmission<'_> {
    fn drop(&mut self) {
        self.state.retire_admission();
    }
}

impl AuthorityState {
    const fn new() -> Self {
        Self {
            gate: Mutex::new(AuthorityGate {
                closing: false,
                admission_frozen: false,
                in_flight: 0,
            }),
            close_report: Mutex::new(None),
            admissions_settled: tokio::sync::Notify::const_new(),
            slot_released: Mutex::new(false),
            #[cfg(any(test, feature = "test-hooks"))]
            slot_release_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn gate(&self) -> std::sync::MutexGuard<'_, AuthorityGate> {
        self.gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn is_closed(&self) -> bool {
        self.gate().closing
    }

    /// Admit one operation: refused once this authority is closing or has
    /// frozen operation admission, otherwise counted until the returned guard
    /// is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Closing`] when this authority has begun closing or
    /// has frozen admission.
    fn admit(&self) -> CoreResult<AuthorityAdmission<'_>> {
        let mut gate = self.gate();
        if gate.closing || gate.admission_frozen {
            return Err(CoreError::Closing);
        }
        gate.in_flight += 1;
        drop(gate);
        Ok(AuthorityAdmission { state: self })
    }

    /// Freeze operation admission without retiring the handle: the Actor-side
    /// stop the quiesce latches before it joins. Reads keep working, and the
    /// closing latch still retires the handle as a whole.
    fn freeze_admissions(&self) {
        self.gate().admission_frozen = true;
    }

    /// Latch the closing identity and report how many operations were admitted
    /// and not yet retired at that instant — the admission accounting `close`
    /// must settle before it may confirm a cleanup.
    fn latch_closing(&self) -> usize {
        let mut gate = self.gate();
        gate.closing = true;
        gate.in_flight
    }

    fn retire_admission(&self) {
        let mut gate = self.gate();
        // A double retire would under-count and let a `close` confirm around
        // live work, so this fails loudly (debug/tests) or wraps far away from
        // zero (release) — never silently to a confirmable state.
        gate.in_flight -= 1;
        drop(gate);
        // Publish AFTER the count dropped, so a joiner that registered and then
        // read a non-zero count is still woken by this retirement.
        //
        // Wake EVERY joiner: the admission join is entered by every concurrent
        // quiesce on this authority, and the last retirement is the only wakeup
        // they will ever see. A `notify_one` handoff would let one joiner
        // recheck zero while a second stayed suspended forever.
        self.admissions_settled.notify_waiters();
    }

    /// Wait until every operation admitted before the admission latch (the
    /// closing latch, or the quiesce's freeze) has retired.
    ///
    /// This is the authority-wide half of the ordered close: an operation
    /// admitted before the latch registers its drain only after its asynchronous
    /// admission and Host execution, so a quiesce that trusted the registered
    /// drains alone could read a zero count and confirm while that operation was
    /// still about to own a drain. Cancellation-safe — the latch keeps admission
    /// frozen, so a caller that runs out of its outer deadline loses nothing and
    /// a retry joins the same admissions.
    ///
    /// Cancellation-safe AND multi-waiter: the join is entered by EVERY
    /// concurrent quiesce on this authority, so the waiter registers with the
    /// notification BEFORE it reads the count (`Notified::enable`, the same
    /// register-first handoff [`Self::close`]'s waiters use) and the retirement
    /// wakes all of them. `notify_one`'s single stored permit would leave a
    /// second joiner suspended on a retirement that already happened.
    async fn join_admissions(&self) {
        loop {
            let settled = self.admissions_settled.notified();
            tokio::pin!(settled);
            settled.as_mut().enable();
            if self.gate().in_flight == 0 {
                return;
            }
            settled.await;
        }
    }

    /// Claim the ONE release of the service-wide established-owner slot this
    /// authority is entitled to. Returns `false` when a concurrent (or
    /// earlier) confirmed close on a clone already claimed it, so the retired
    /// authority cannot clear a successor's claim with a second release.
    fn claim_slot_release(&self) -> bool {
        {
            let mut released = self
                .slot_released
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if *released {
                return false;
            }
            *released = true;
        }
        #[cfg(any(test, feature = "test-hooks"))]
        self.slot_release_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        true
    }

    fn retained_close_report(&self) -> Option<CoreCloseReport> {
        self.close_report
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn retain_close_report(&self, report: &CoreCloseReport) {
        *self
            .close_report
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(report.clone());
    }
}

/// One open Host authority: the started [`HostManager`], its composed
/// provider port, and the process-lifetime Actor session registry. Cloning
/// shares the same authority.
#[derive(Clone)]
pub struct HostHandle {
    core: CoreService,
    host: Arc<HostManager>,
    port: Arc<dyn ProviderPort>,
    registry: ActorSessionRegistry,
    authority: Arc<AuthorityState>,
}

impl CoreService {
    /// Open the Host authority against the active nexus home, composing the
    /// given (already selected/admitted) provider port as its effect boundary.
    /// A failed start never publishes the authority.
    ///
    /// # Errors
    /// Returns [`CoreError::Closing`] when the service is closing, and an
    /// `internal` error when the agent-host config cannot be loaded, the
    /// catalog cannot be discovered, or the [`HostManager`] fails to start.
    pub async fn open_host(&self, port: Arc<dyn ProviderPort>) -> CoreResult<HostHandle> {
        self.ensure_open()?;
        // Established-owner admission: at most one authority per open
        // service; a second start is a typed busy rejection, never a
        // second engine.
        self.claim_host_authority()?;
        match self.open_host_inner(port).await {
            Ok(handle) => Ok(handle),
            Err(err) => {
                // A failed start never owns the slot: retry is allowed.
                self.release_host_authority();
                Err(err)
            }
        }
    }

    /// Adopt an **already started** [`HostManager`] as this service's Host
    /// authority. The exact supplied manager and provider port are retained:
    /// attach never loads a config, discovers a catalog, starts a Host or
    /// probes readiness, so the native open keeps its single manager, its
    /// pinned-root readiness and its provider-only lane.
    ///
    /// The supplied manager is the authority's effect boundary once this
    /// returns; the standalone [`Self::open_host`] constructor stays the
    /// library-only entry.
    ///
    /// # Errors
    /// Returns [`CoreError::Closing`] when the service is closing (the
    /// established-owner slot is never claimed on that path) and
    /// [`CoreError::OwnerBusy`] when an authority is already established for
    /// this open — the identical typed rejection [`Self::open_host`] gives.
    pub fn attach_host(
        &self,
        host: Arc<HostManager>,
        port: Arc<dyn ProviderPort>,
    ) -> CoreResult<HostHandle> {
        self.ensure_open()?;
        self.claim_host_authority()?;
        Ok(HostHandle {
            core: self.clone(),
            host,
            port,
            registry: ActorSessionRegistry::new(),
            authority: Arc::new(AuthorityState::new()),
        })
    }

    /// Claim the one established-owner Host authority slot for this open
    /// service. Shared by the standalone and the adopting constructor, so
    /// both reject a second owner identically.
    fn claim_host_authority(&self) -> CoreResult<()> {
        let mut established = self
            .inner
            .host_authority_established
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *established {
            return Err(CoreError::OwnerBusy);
        }
        *established = true;
        drop(established);
        Ok(())
    }

    /// Free a claimed slot: only a failed open or a confirmed close releases
    /// the authority for a later open/attach.
    fn release_host_authority(&self) {
        *self
            .inner
            .host_authority_established
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
    }

    async fn open_host_inner(&self, port: Arc<dyn ProviderPort>) -> CoreResult<HostHandle> {
        // The stored `nexus_home` is already the canonical Nexus root
        // (`<user_home>/.nexus42`), so the agent-host config is one
        // `agent-host/config.toml` below it — the same file the native boot
        // reaches through `agent_host_config_path(user_home)`. Passing this
        // root to that helper nests `.nexus42` twice and reads a file nothing
        // writes.
        let nexus_root = self.inner.nexus_home.clone();
        validate_workspace_path(&nexus_root).map_err(config_err)?;
        let config_path = nexus_root.join("agent-host").join("config.toml");
        let host_config: AgentHostConfig =
            load_config_from_path(&config_path).map_err(config_err)?;
        let admitted_catalog = discover_provider_catalog(&host_config).map_err(config_err)?;
        let host = Arc::new(HostManager::new());
        host.start(HostStartConfig {
            config_path,
            workspace_root: nexus_root,
            max_sessions: host_config.max_sessions,
            max_ops_per_session: host_config.max_ops_per_session,
            timeouts: host_config.timeouts.clone(),
            host_config: Some(host_config),
            admitted_catalog: Some(admitted_catalog),
            probe_owner: None,
        })
        .await
        .map_err(host_err)?;
        Ok(HostHandle {
            core: self.clone(),
            host,
            port,
            registry: ActorSessionRegistry::new(),
            authority: Arc::new(AuthorityState::new()),
        })
    }
}

fn invalid(field: &str, reason: impl Into<String>) -> CoreError {
    CoreError::InvalidInput {
        field: field.to_string(),
        reason: reason.into(),
    }
}

impl HostHandle {
    /// The composed provider port backing this authority (effect-boundary
    /// access for the owning transport; never a second admission path).
    #[must_use]
    pub fn provider_port(&self) -> Arc<dyn ProviderPort> {
        self.port.clone()
    }

    /// The exact [`HostManager`] this authority was opened or attached over.
    ///
    /// Internal Rust composition access for the single lifetime owner — never
    /// a transport/napi handle, and never a second Host.
    #[must_use]
    pub fn manager(&self) -> Arc<HostManager> {
        self.host.clone()
    }

    /// The owner-scoped authoritative outcome of one Character operation.
    ///
    /// Detailed outcomes are process-lifetime (technical contract §5): a
    /// foreign, unknown, evicted or post-restart id is a MISSING outcome, never
    /// an inferred success from a generic session state or a provider journal
    /// row.
    ///
    /// # Errors
    /// Returns `auth_required` for a foreign or drifted principal,
    /// `invalid_input` for a malformed id, [`CoreError::Closing`] on a closed
    /// authority, and `not_found` for a missing or foreign operation. The
    /// registry lookup itself is synchronous; the signature is async to match
    /// the authority's other reads.
    #[allow(clippy::unused_async_trait_impl)] // async matches the authority's other reads; the body has no await today
    #[allow(clippy::unused_async)] // async is the await-symmetric public signature; the body is a map read today
    pub async fn character_operation(
        &self,
        principal: &Principal,
        operation_id: String,
    ) -> CoreResult<CharacterOperationResult> {
        self.ensure_authority_open()?;
        self.core.verify_principal(principal)?;
        let uuid = Uuid::parse_str(&operation_id)
            .map_err(|_| invalid("operation_id", "operation_id must be a valid UUID"))?;
        self.registry
            .character_operation_result(principal.creator_id(), &HostOperationId(uuid))
    }

    /// Observe one Character operation started by this authority: the NEXT
    /// bounded batch of events and the authoritative status they belong to
    /// (technical contract §5, "Delivery versus authority").
    ///
    /// The source is the SAME Host's event broadcast, selected to the exact
    /// `(session_id, operation_id)` pair, so the authority's own drain of the
    /// original exec stream is untouched: a consumer that never calls this (or
    /// one that lags it) blocks neither settlement nor another delivery. Core
    /// status stays independent of the HTTP consumer's backpressure.
    ///
    /// Detailed observation is process-lifetime like the outcome itself: an
    /// unknown, foreign, cross-session, evicted or post-reopen operation is
    /// `not_found`, and a lost observation source is a typed `lagging` resync
    /// gap (never a fabricated completion), so a consumer re-reads
    /// [`Self::character_operation`] instead of inferring a terminal.
    ///
    /// # Errors
    /// `auth_required` for a foreign/drifted principal, `invalid_input` for a
    /// malformed id or a non-positive bound, `closing` on a closed authority,
    /// and `not_found` for a missing/foreign/cross-session operation or an
    /// operation with no retained observation.
    pub async fn next_events(
        &self,
        principal: &Principal,
        session_id: String,
        operation_id: String,
        max_events: u32,
        max_bytes: u32,
    ) -> CoreResult<ProviderEventBatch> {
        self.ensure_authority_open()?;
        self.core.verify_principal(principal)?;
        let sid = HostSessionId(
            Uuid::parse_str(&session_id)
                .map_err(|_| invalid("session_id", "session_id must be a valid UUID"))?,
        );
        let op_id = HostOperationId(
            Uuid::parse_str(&operation_id)
                .map_err(|_| invalid("operation_id", "operation_id must be a valid UUID"))?,
        );
        if max_events == 0 {
            return Err(invalid("max_events", "max_events must be positive"));
        }
        if max_bytes == 0 {
            return Err(invalid("max_bytes", "max_bytes must be positive"));
        }
        // Owner + session/operation association BEFORE any read: an operation
        // that this authority did not record, or one recorded for a different
        // session, is `not_found` — observation is never a cross-session leak.
        let recorded_session = self
            .registry
            .operation_session_id(&op_id)
            .ok_or_else(|| operation_not_found(&op_id))?;
        if recorded_session != sid {
            return Err(CoreError::NotFound {
                resource: format!("operation {op_id}"),
            });
        }
        if !self
            .registry
            .operation_owned_by(principal.creator_id(), &op_id)
        {
            return Err(operation_not_found(&op_id));
        }
        if self
            .registry
            .stored_session_owner(&sid)
            .is_some_and(|(owner, _, _)| owner != principal.creator_id())
        {
            return Err(CoreError::NotFound {
                resource: format!("session {sid}"),
            });
        }
        let reader = self
            .registry
            .observation(&op_id)
            .ok_or_else(|| CoreError::NotFound {
                resource: format!("operation {op_id} observation"),
            })?;
        reader
            .next(op_id.to_string(), max_events, max_bytes)
            .await
            .map_err(port_err)
    }

    /// Cancel one owner-authorized Character operation through the SAME
    /// manager (technical contract §2/§3).
    ///
    /// Order is the contract: the stored owner and the session/operation
    /// association are verified, the provider's negotiated cancellation
    /// capability is checked BEFORE any intent is latched (an unsupported
    /// provider — DSH — is refused `not_supported` and never records cancel
    /// intent), the cancellation is then latched under the existing phase lock,
    /// and only that same manager's `cancel` is invoked. A provider refusal
    /// rolls the latched intent back and is reported honestly: success is never
    /// fabricated after a refusal.
    ///
    /// # Errors
    /// `auth_required` for a foreign/drifted principal, `invalid_input` for a
    /// malformed id, `closing` on a closed authority, `not_found` for a
    /// missing/foreign operation or one whose session is no longer on the
    /// manager, `not_supported` for a provider that cannot cancel, and
    /// `actor_conflict actor_operation_finished` once the operation is
    /// finalizing or terminal.
    pub async fn cancel_operation(
        &self,
        principal: &Principal,
        operation_id: String,
    ) -> CoreResult<CancelOperationResponse> {
        self.ensure_authority_open()?;
        self.core.verify_principal(principal)?;
        let op_id = HostOperationId(
            Uuid::parse_str(&operation_id)
                .map_err(|_| invalid("operation_id", "operation_id must be a valid UUID"))?,
        );
        let session_id = self
            .registry
            .operation_session_id(&op_id)
            .ok_or_else(|| operation_not_found(&op_id))?;
        if !self
            .registry
            .operation_owned_by(principal.creator_id(), &op_id)
        {
            return Err(operation_not_found(&op_id));
        }
        if self
            .registry
            .stored_session_owner(&session_id)
            .is_some_and(|(owner, _, _)| owner != principal.creator_id())
        {
            return Err(CoreError::NotFound {
                resource: format!("session {session_id}"),
            });
        }
        self.request_cancel(&op_id).await?;
        Ok(CancelOperationResponse {
            operation_id: op_id.to_string(),
            status: "cancelled".to_string(),
        })
    }

    /// Shut one session down under the stored-owner gate (technical contract
    /// §2/§3): the session's active Actor work is cancelled through the same
    /// manager, the manager's per-session shutdown is called, and only then is
    /// the Actor reuse state retired — after the session's own drains have
    /// settled, because the manager release does not settle this authority's
    /// drain of that session's exec stream. Success is reported only once all
    /// three hold; an unconfirmed cleanup surfaces as an error, never as a
    /// shutdown.
    ///
    /// # Errors
    /// `auth_required` for a foreign/drifted principal, `invalid_input` for a
    /// malformed id, `closing` on a closed authority, `not_found` for a
    /// foreign Actor session or an unknown session, the mapped refusal of a
    /// cancellation the provider did not accept (see below), and the mapped
    /// host error when the manager cannot confirm the session's release.
    ///
    /// A provider that cannot cancel leaves its run's record at the phase the
    /// authority last observed: terminal truth stays the drain's, so no
    /// synthetic outcome is written for a session whose own stream has not (or
    /// may never) report one.
    pub async fn shutdown_session(
        &self,
        principal: &Principal,
        session_id: String,
    ) -> CoreResult<ShutdownSessionResponse> {
        self.ensure_authority_open()?;
        self.core.verify_principal(principal)?;
        let sid = HostSessionId(
            Uuid::parse_str(&session_id)
                .map_err(|_| invalid("session_id", "session_id must be a valid UUID"))?,
        );
        // Owner scoping first, then existence: a foreign Actor session and a
        // session unknown to BOTH the Actor index and the manager are the same
        // `not_found`, never an internal host failure.
        let missing_session = || CoreError::NotFound {
            resource: format!("session {sid}"),
        };
        let known_to_index = match self.registry.stored_session_owner(&sid) {
            Some((owner, _, _)) => {
                if owner != principal.creator_id() {
                    return Err(missing_session());
                }
                true
            }
            None => false,
        };
        if !known_to_index && self.session_cancellation_supported(&sid).await?.is_none() {
            return Err(missing_session());
        }
        // Cancel the session's active work first. Every refusal is ACCOUNTED,
        // never discarded: a provider that cannot cancel, a session the manager
        // no longer knows, and an operation that finalized before the cancel
        // landed all latch no intent, so the manager release below stays the
        // authoritative teardown. Any OTHER refusal — a provider that
        // advertised cancellation and then failed it — is surfaced, because
        // this call must not report a teardown whose active work is still
        // running.
        let mut refusal: Option<CoreError> = None;
        for op_id in self.registry.nonterminal_operations_for_session(&sid) {
            if let Err(err) = self.request_cancel(&op_id).await {
                if is_unsupported_cancellation(&err)
                    || is_missing_session(&err)
                    || is_finished_operation(&err)
                {
                    continue;
                }
                refusal.get_or_insert(err);
            }
        }
        if let Some(err) = refusal {
            return Err(err);
        }
        self.registry
            .shutdown_session(sid.clone(), self.host.as_ref())
            .await?;
        Ok(ShutdownSessionResponse {
            session_id: sid.to_string(),
            status: "shutdown".to_string(),
        })
    }

    /// Request cancellation for one RECORDED operation: capability refusal
    /// before intent, latch under the phase lock, the same manager's cancel,
    /// and — on an accepted cancel — the §5 terminal the cancel won.
    ///
    /// Shared by the owner-authorized [`Self::cancel_operation`], the session
    /// shutdown and the Actor-only quiesce so all three control paths share ONE
    /// refusal/race/commit order.
    async fn request_cancel(&self, operation_id: &HostOperationId) -> CoreResult<()> {
        let session_id = self
            .registry
            .operation_session_id(operation_id)
            .ok_or_else(|| operation_not_found(operation_id))?;
        let owner = self
            .registry
            .operation_owner(operation_id)
            .ok_or_else(|| operation_not_found(operation_id))?;
        match self.session_cancellation_supported(&session_id).await? {
            // An unsupported provider is refused BEFORE any intent: DSH's
            // adapter-level no-op is not a cancellation, and it must never
            // latch cancel intent.
            Some(false) => {
                return Err(CoreError::Coded {
                    code: "not_supported".into(),
                    message: "provider does not support cancellation".into(),
                });
            }
            Some(true) => {}
            None => {
                return Err(CoreError::NotFound {
                    resource: format!("session {session_id}"),
                });
            }
        }
        // Latch a PROVISIONAL intent under the registry lock: the phase race (a
        // drain that finalizes first) is decided here, and a finished operation
        // refuses `409` — but the intent is not confirmed until the provider
        // answers, so a drain that finalizes while the call is in flight
        // records the run's own terminal, never a `cancelled` the provider may
        // still refuse.
        self.registry
            .latch_provisional_operation_cancel(&owner, operation_id)?;
        if let Err(err) = self.host.cancel(operation_id.clone()).await {
            // The provider refused: undo the intent so a refusal can never be
            // recorded as an accepted cancellation.
            self.registry.rollback_operation_cancel(operation_id);
            return Err(host_err(err));
        }
        // Accepted: the §5 cancelled row IS this operation's truth, recorded
        // even if no drain ever settles it. A drain that settled first stays
        // authoritative (first-writer-wins at the registry).
        self.registry
            .commit_operation_terminal(operation_id, cancelled_outcome(&session_id, operation_id));
        self.registry.clear_indexed_operation(operation_id);
        Ok(())
    }

    /// The negotiated cancellation capability of an operation's session;
    /// `None` when the session is no longer on the manager (nothing left to
    /// cancel there).
    async fn session_cancellation_supported(
        &self,
        session_id: &HostSessionId,
    ) -> CoreResult<Option<bool>> {
        let sessions = self.host.list_sessions().await.map_err(host_err)?;
        Ok(sessions
            .iter()
            .find(|session| &session.id == session_id)
            .map(|session| session.negotiated_capabilities.cancellation))
    }

    /// The authority-level open gate: the service is open AND this handle's
    /// own authority has not begun closing. A handle retired by `close` stays
    /// refused even after another authority opens over the same service.
    fn ensure_authority_open(&self) -> CoreResult<()> {
        self.core.ensure_open()?;
        if self.authority.is_closed() {
            return Err(CoreError::Closing);
        }
        Ok(())
    }

    /// Admit one Host operation against this authority's close barrier: the
    /// service must be open and this authority must not be closing, and the
    /// admission stays counted until the returned guard drops — which happens
    /// only after the operation has either failed before creating a drain or
    /// transferred its drain to the registry. A `close` that latches in
    /// between therefore observes the admission instead of releasing the
    /// established-owner slot over unaccounted work.
    fn admit_operation(&self) -> CoreResult<AuthorityAdmission<'_>> {
        self.core.ensure_open()?;
        self.authority.admit()
    }

    /// Whether `session_id` is an Actor-mode session this authority indexes
    /// (live) or has retired (tombstoned) — the authority's own pure read, so a
    /// consumer that needs the raw provider-lane guard takes this instead of the
    /// registry itself.
    #[must_use]
    pub fn owns_actor_session(&self, session_id: &HostSessionId) -> bool {
        self.registry.is_actor_session(session_id)
    }

    /// Whether `operation_id` belongs to a Character operation this authority
    /// reserved (a recorded outcome) or registered before the Host bound it to
    /// its session — the operation half of [`Self::owns_actor_session`].
    #[must_use]
    pub fn owns_actor_operation(&self, operation_id: &HostOperationId) -> bool {
        self.registry.operation_session_id(operation_id).is_some()
            || self
                .registry
                .resolve_indexed_operation_session(operation_id)
                .is_some()
    }

    /// The Actor session registry (owner/tombstone/epoch index).
    ///
    /// Test-only surface: the mutable registry is what lets a caller reserve a
    /// record the authority's result path would then serve as authoritative, so
    /// a production build exposes no accessor at all and the registry's
    /// reservation/lifecycle methods are reachable only from this crate's own
    /// `execute`/drain path. Reads that production needs go through the narrow
    /// authority methods above.
    #[cfg(any(test, feature = "test-hooks"))]
    #[must_use]
    pub const fn actor_sessions(&self) -> &ActorSessionRegistry {
        &self.registry
    }

    /// Test-only: how many times this authority freed the service-wide
    /// established-owner slot, so the concurrent-close regression can assert the
    /// release is exactly-once per authority epoch. Absent from a production
    /// build with the counter it reads.
    #[cfg(any(test, feature = "test-hooks"))]
    #[must_use]
    pub fn slot_release_count(&self) -> usize {
        self.authority
            .slot_release_count
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Test-only reservation seam: the exact reservation `execute` performs,
    /// over a caller-supplied snapshot, so the drain, cancel and bounded
    /// observation paths are provable without a provider process.
    ///
    /// Compiled out of the production build for the same reason as
    /// [`Self::actor_sessions`]: reservation is the only way to create an
    /// operation record, so with no production entry the result path can only
    /// ever serve records `execute` minted.
    ///
    /// # Errors
    ///
    /// Returns capacity or shutdown conflicts.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn reserve_character_operation(
        &self,
        snapshot: &CharacterOperationSnapshot,
    ) -> CoreResult<()> {
        self.registry.reserve_character_operation(snapshot)
    }

    /// Create a Host session. A valid `actor_ref`/`viewpoint` pair is admitted
    /// against stored ownership (P2 admission + activity lease); a legacy
    /// create routes to the Host directly with the verified owner.
    ///
    /// # Errors
    /// Pair-shape violations are `invalid_input`; stored ownership failures
    /// keep their taxonomy (`not_found`, `actor_conflict`); host failures are
    /// `internal`.
    pub async fn create_session(
        &self,
        principal: &Principal,
        request: CreateSessionRequest,
    ) -> CoreResult<SessionResponse> {
        self.ensure_authority_open()?;
        self.core.verify_principal(principal)?;
        let pair = classify_pair(request.actor_ref.is_some(), request.viewpoint.is_some())?;
        let creator_id = principal.creator_id().to_string();
        if pair == ActorPairMode::Actor {
            let (Some(actor_ref), Some(viewpoint)) =
                (request.actor_ref.as_ref(), request.viewpoint.as_ref())
            else {
                return Err(invalid(
                    "actor_ref",
                    "actor_ref and viewpoint must both be present or both absent",
                ));
            };
            let actor = match actor_ref {
                nexus_contracts::generated::daemon_api::agent_host::create_session_request::NexusActorRef::CreatorActorRef { creator_id, .. } => {
                    AdmittedActor::Creator {
                        creator_id: creator_id.to_string(),
                    }
                }
                nexus_contracts::generated::daemon_api::agent_host::create_session_request::NexusActorRef::CharacterActorRef { character_id, .. } => {
                    AdmittedActor::Character {
                        character_id: character_id.to_string(),
                    }
                }
            };
            // A Character session runs at the admission's own creative-root
            // pin; a Creator Actor session keeps the pre-existing cwd
            // boundary (D11 selects only the Character root). Both are
            // resolved before any Host, provider or memory effect.
            let canonical_root = match &actor {
                AdmittedActor::Character { .. } => character_session_cwd(&request, &self.core)?,
                AdmittedActor::Creator { .. } => session_cwd(&request, &self.core)?,
            };
            let admission = CoreActorAdmission::new(self.core.inner.pool.clone());
            let viewpoint = ActorViewpoint {
                world_id: viewpoint.world_id.to_string(),
                binding_id: viewpoint.binding_id.as_ref().map(|id| (**id).clone()),
                branch_id: viewpoint.branch_id.as_ref().map(|id| (**id).clone()),
                event_id: viewpoint.event_id.as_ref().map(|id| (**id).clone()),
            };
            // Admit the operation-scoped knowledge context: it owns the
            // per-Character activity lease and the shared World/Character
            // knowledge leases (durable §4.3), and is held through Host
            // creation AND registry insertion, so neither an archive nor a
            // governance edit can race an in-flight create into a reusable
            // session.
            let knowledge = self
                .core
                .admit_actor_knowledge_view(principal, &actor, viewpoint.clone())
                .await?;
            let ctx = admission.admit(&creator_id, actor, viewpoint).await?;
            let key = ActorSessionKey::for_context(
                &request.provider_id,
                &canonical_root,
                request.model.clone(),
                request.mode.clone(),
                &ctx,
                knowledge.identity(),
            )?;
            let host_req =
                Self::host_create_request(&request, &canonical_root, &ctx.owner_creator_id);
            let host_for_create = self.host.clone();
            let session = self
                .registry
                .resolve_or_create(key, ctx.clone(), self.host.as_ref(), move || {
                    let host_for_create = host_for_create;
                    let host_req = host_req;
                    async move {
                        host_for_create
                            .create_session(host_req)
                            .await
                            .map_err(host_err)
                    }
                })
                .await?;
            drop(knowledge);
            let (actor_ref, viewpoint) = echo_actor_pair(&ctx)?;
            return Ok(session_wire(
                session.id.to_string(),
                session.provider_id.to_string(),
                format!("{:?}", session.state),
                None,
                request.model.clone(),
                actor_ref,
                viewpoint,
            ));
        }
        let canonical_root = session_cwd(&request, &self.core)?;
        let host_req = Self::host_create_request(&request, &canonical_root, &creator_id);
        let model = request.model.clone();
        let session = self.host.create_session(host_req).await.map_err(host_err)?;
        Ok(session_wire(
            session.id.to_string(),
            session.provider_id.to_string(),
            format!("{:?}", session.state),
            None,
            model,
            None,
            None,
        ))
    }

    #[allow(clippy::significant_drop_tightening)] // the guard deliberately spans the whole operation
    /// Execute a normalized host operation (`prompt`, `set_model`, `set_mode`)
    /// through the real Host authority. Actor-indexed sessions re-admit the
    /// stored Actor, deny a stale `lifecycle_epoch` before any provider
    /// effect, and hold the P2 activity lease through the server-owned drain.
    ///
    /// # Errors
    /// See the per-branch taxonomy; a stale epoch is `actor_conflict
    /// actor_session_stale` and never reaches the provider.
    ///
    /// # Panics
    ///
    /// Panics only when a character session is registered but has no captured
    /// context entry — a registry invariant maintained on session creation, so
    /// a missing entry is a programming error rather than a caller error.
    #[allow(clippy::too_many_lines)]
    pub async fn execute(
        &self,
        principal: &Principal,
        session_id: String,
        request: ExecuteOperationRequest,
    ) -> CoreResult<OperationResponse> {
        // The barrier admission is taken here, before the first await, and held
        // until the operation returns: by then it has either failed before
        // creating a drain or handed its drain to the registry, so a `close`
        // that latches in between can never confirm around it.
        let _admission = self.admit_operation()?;
        self.core.verify_principal(principal)?;
        let uuid = Uuid::parse_str(&session_id)
            .map_err(|_| invalid("session_id", "session_id must be a valid UUID"))?;
        let sid = HostSessionId(uuid);
        // The SESSION-scoped half of the admission above, and the counterpart
        // of the registry transfer below: taken under the registry's own lock
        // before this operation's first await, and retired only after its drain
        // has been registered. A concurrent session shutdown joins through that
        // same live-work accounting, so it cannot observe "this session has no
        // work" while this admitted operation can still register the drain that
        // shutdown is retiring — the registered drains alone are not that proof,
        // because registration happens after asynchronous admission and Host
        // execution.
        let _session_admission = self.registry.admit_session_operation(sid.clone());
        // Stored owner/tombstone gate: an indexed or retired Actor session is
        // owner-scoped before any Host access, whatever the operation kind.
        if let Some((owner, _, _)) = self.registry.stored_session_owner(&sid) {
            if owner != principal.creator_id() {
                return Err(CoreError::NotFound {
                    resource: format!("session {sid}"),
                });
            }
        }
        if self.registry.is_actor_session(&sid)
            && matches!(
                request,
                ExecuteOperationRequest::SetModel { .. } | ExecuteOperationRequest::SetMode { .. }
            )
        {
            return Err(CoreError::ActorConflict {
                code: "actor_session_immutable".into(),
                message: "SetModel and SetMode are rejected on Actor-indexed sessions".into(),
            });
        }
        let op_id = HostOperationId::new();
        match request {
            ExecuteOperationRequest::Prompt { content, remember } => {
                let remember = remember.unwrap_or(false);
                let indexed = self.registry.context_for(&sid);
                let is_character = indexed.as_ref().map_or_else(
                    || self.registry.is_actor_session(&sid),
                    |ctx| matches!(ctx.actor, AdmittedActor::Character { .. }),
                );
                if remember && !is_character {
                    return Err(invalid(
                        "remember",
                        "remember requires an admitted stored Character session with an active binding",
                    ));
                }
                if remember {
                    // Contract §5: this Host has no complete run-capture
                    // writer, so a Character capture is refused as a typed
                    // `not_supported` BEFORE the operation reserves an outcome,
                    // admits an activity/knowledge context or reaches the
                    // provider. No pending or captured result may be promised.
                    return Err(CoreError::Coded {
                        code: "not_supported".into(),
                        message: "character run capture is not supported by this host".into(),
                    });
                }
                let (assembled, fenced) = match indexed {
                    None => {
                        // A retired (tombstoned) Actor id is never legacy: it
                        // must stay a stale refusal instead of falling back to
                        // raw prompt bytes.
                        if self.registry.is_actor_session(&sid) {
                            return Err(actor_session_stale(&sid.to_string()));
                        }
                        // Never-indexed legacy path: unchanged raw prompt
                        // bytes, no knowledge fence (nothing is
                        // selection-scoped here).
                        (content, None)
                    }
                    Some(ctx) => {
                        if ctx.owner_creator_id != principal.creator_id() {
                            return Err(CoreError::NotFound {
                                resource: format!("session {sid}"),
                            });
                        }
                        let viewpoint = ActorViewpoint {
                            world_id: ctx.world_id.clone(),
                            binding_id: ctx.binding_id.clone(),
                            branch_id: ctx.branch_id.clone(),
                            event_id: ctx.event_id.clone(),
                        };
                        // Activity admission BEFORE MCA/Host so an archive
                        // cannot race the prompt into a stale session, and the
                        // knowledge fences are read/held under it (durable
                        // §4.3). The context is held through terminal capture
                        // in the server-owned drain.
                        let knowledge = self
                            .core
                            .admit_actor_knowledge_view(principal, &ctx.actor, viewpoint.clone())
                            .await?;
                        if ctx.character_epoch != knowledge.activity_epoch() {
                            return Err(actor_session_stale(&sid.to_string()));
                        }
                        // Stored knowledge revisions were re-read under the
                        // leases: a material governance change (or a policy
                        // switch) retires this session and admits a fresh
                        // context on the next create, never a stale reuse.
                        match self
                            .registry
                            .revalidate_knowledge(&sid, &knowledge.identity())?
                        {
                            KnowledgeReuse::Reusable => {}
                            KnowledgeReuse::Retired(retired) => {
                                let _ = self.host.shutdown_session(retired).await;
                                return Err(actor_session_stale(&sid.to_string()));
                            }
                        }
                        let text = if is_character {
                            let admission = CoreActorAdmission::new(self.core.inner.pool.clone());
                            let re_admitted = admission
                                .admit(principal.creator_id(), ctx.actor.clone(), viewpoint)
                                .await?;
                            self.assemble_admitted_prompt(principal, &re_admitted, content)
                                .await?
                        } else {
                            content
                        };
                        (text, Some(knowledge))
                    }
                };
                let snapshot = if is_character {
                    let snap = CharacterOperationSnapshot {
                        owner_creator_id: principal.creator_id().to_string(),
                        session_id: sid.clone(),
                        operation_id: op_id.clone(),
                    };
                    self.registry.reserve_character_operation(&snap)?;
                    self.registry
                        .register_indexed_operation(op_id.clone(), sid.clone());
                    // Observation is subscribed BEFORE Host execution and
                    // retained ON the reserved record (technical contract §5):
                    // a Character operation is observed over the SAME manager's
                    // event broadcast, so the observation can miss no event of
                    // it, while the original exec stream below stays this
                    // authority's own drain — observation never consumes it, and
                    // a consumer that never pulls the observation (or lags it)
                    // blocks nothing. One bounded reader per reserved operation,
                    // dropped with its record.
                    let observation = self.host.subscribe(&sid);
                    if let Err(err) = self.registry.retain_observation(
                        &op_id,
                        ProviderEventReader::observe(observation_stream(
                            observation,
                            sid.clone(),
                            op_id.clone(),
                        )),
                    ) {
                        self.registry.remove_operation_reservation(&op_id);
                        self.registry.clear_indexed_operation(&op_id);
                        return Err(err);
                    }
                    Some(snap)
                } else {
                    None
                };
                let host_op = HostOperation::Prompt {
                    op_id: op_id.clone(),
                    content: vec![
                        nexus_agent_host::capability::model::HostContentBlock::Text {
                            text: assembled,
                        },
                    ],
                    permission_scope: None,
                };
                let stream = match self.host.exec(sid.clone(), host_op).await {
                    Ok(stream) => stream,
                    Err(err) => {
                        if snapshot.is_some() {
                            self.registry.remove_operation_reservation(&op_id);
                            self.registry.clear_indexed_operation(&op_id);
                        }
                        return Err(host_err(err));
                    }
                };
                if let Some(snapshot) = snapshot {
                    let registry = self.registry.clone();
                    let session_id = snapshot.session_id.clone();
                    // Authority-owned drain: the handle stays with the registry
                    // — accounted to its session, so a session shutdown joins
                    // the work it retires — so close never drops live
                    // lease/drain ownership on the floor.
                    self.registry.spawn_actor_drain(session_id, async move {
                        drain_character_operation(registry, stream, snapshot, fenced).await;
                    });
                } else {
                    self.registry
                        .spawn_actor_drain(sid.clone(), drain_plain(stream, fenced));
                }
                Ok(OperationResponse {
                    operation_id: op_id.to_string(),
                    session_id: sid.to_string(),
                    status: "started".to_string(),
                    capture: None,
                })
            }
            ExecuteOperationRequest::SetModel { model } => {
                self.exec_non_prompt(sid, HostOperation::SetModel { model }, op_id)
                    .await
            }
            ExecuteOperationRequest::SetMode { mode } => {
                self.exec_non_prompt(sid, HostOperation::SetMode { mode }, op_id)
                    .await
            }
        }
    }

    async fn exec_non_prompt(
        &self,
        sid: HostSessionId,
        host_op: HostOperation,
        op_id: HostOperationId,
    ) -> CoreResult<OperationResponse> {
        let stream = self
            .host
            .exec(sid.clone(), host_op)
            .await
            .map_err(host_err)?;
        self.registry
            .spawn_actor_drain(sid.clone(), drain_plain(stream, None));
        Ok(OperationResponse {
            operation_id: op_id.to_string(),
            session_id: sid.to_string(),
            status: "started".to_string(),
            capture: None,
        })
    }

    /// Query the authority: health, catalog (+ PATH scan), session list, and
    /// session/operation lookup with the durable journal restart fallback.
    ///
    /// Legacy owner semantics (P2 admission contract): legacy sessions are
    /// never indexed, so they carry no per-session stored owner. The
    /// authority is single-owner per open — `verify_principal` proves the
    /// caller is the open's verified creator AND that the on-disk selection
    /// still matches, so any admitted caller IS the owner of the legacy
    /// surface; a foreign or drifted identity is `auth_required` before any
    /// Host access. Indexed/retired Actor sessions additionally carry their
    /// own stored owner gate below.
    ///
    /// # Errors
    /// `auth_required` for foreign/drifted principals, `invalid_input` for
    /// malformed queries; host failures are `internal`.
    #[allow(clippy::too_many_lines)]
    pub async fn query(
        &self,
        principal: &Principal,
        request: CoreHostQuery,
    ) -> CoreResult<CoreHostQueryResponse> {
        self.ensure_authority_open()?;
        self.core.verify_principal(principal)?;
        match request.query {
            CoreHostQueryQuery::Health => {
                let health = self.host.health().await.map_err(host_err)?;
                Ok(CoreHostQueryResponse {
                    health: Some(CoreHostQueryResponseHealth {
                        running: health.running,
                        active_sessions: u64::try_from(health.active_sessions)
                            .map_err(|_| internal("health sessions"))?,
                        active_operations: u64::try_from(health.active_operations)
                            .map_err(|_| internal("health operations"))?,
                    }),
                    catalog: None,
                    sessions: None,
                    session: None,
                    operation: None,
                    scan: None,
                })
            }
            CoreHostQueryQuery::Catalog => {
                let format = request.format.unwrap_or(CoreHostQueryFormat::Catalog);
                match format {
                    CoreHostQueryFormat::Catalog => {
                        let mut catalog = self.host.provider_catalog().await.map_err(host_err)?;
                        catalog.entries.sort_by_key(|a| a.provider_id.to_string());
                        Ok(CoreHostQueryResponse {
                            catalog: Some(CoreHostQueryResponseCatalog {
                                providers: catalog
                                    .entries
                                    .into_iter()
                                    .map(|entry| CoreHostQueryResponseCatalogProvidersItem {
                                        provider_id: entry.provider_id.to_string(),
                                        display_name: entry.display_name,
                                        protocol_kind: protocol_kind_wire(entry.protocol_kind),
                                    })
                                    .collect(),
                            }),
                            health: None,
                            sessions: None,
                            session: None,
                            operation: None,
                            scan: None,
                        })
                    }
                    CoreHostQueryFormat::Scan => {
                        let config = HostManager::agent_config(&self.host).await;
                        let probe_dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
                            .map(|path| std::env::split_paths(&path).collect())
                            .unwrap_or_default();
                        let native_entries =
                            path_scan::scan_path_in(&config, &[], &probe_dirs).map_err(host_err)?;
                        let entries: Vec<NexusAgentScanEntry> = native_entries
                            .into_iter()
                            .map(|entry| {
                                let launch_command = match &entry.launch {
                                    LaunchStrategy::Acp { command, .. }
                                    | LaunchStrategy::NativeCli { command, .. } => {
                                        Some(command.clone())
                                    }
                                };
                                NexusAgentScanEntry {
                                    name: entry.display_name,
                                    installed: entry.health.available,
                                    launch_command,
                                    description: entry.health.message,
                                    icon_url: None,
                                    registry_agent_id: None,
                                    version: None,
                                }
                            })
                            .collect();
                        Ok(CoreHostQueryResponse {
                            scan: Some(CoreHostQueryResponseScan { entries }),
                            health: None,
                            catalog: None,
                            sessions: None,
                            session: None,
                            operation: None,
                        })
                    }
                }
            }
            CoreHostQueryQuery::ListSessions => {
                let native = self.sorted_sessions().await?;
                let limit = request
                    .limit
                    .map_or(50, std::num::NonZero::get)
                    .clamp(1, 250);
                let limit_us = usize::try_from(limit).unwrap_or(250);
                let owner_scoped: CoreResult<Vec<NexusAgentHostSessionResponse>> = native
                    .iter()
                    // Owner-scoped listing: a foreign indexed/retired Actor
                    // session is invisible to this principal (legacy sessions
                    // are never indexed and stay unchanged).
                    .filter(|s| {
                        self.registry
                            .stored_session_owner(&s.id)
                            .is_none_or(|(owner, _, _)| owner == principal.creator_id())
                    })
                    .map(|s| self.session_response_wire(s))
                    .collect();
                let items: Vec<NexusAgentHostSessionResponse> = owner_scoped?
                    .into_iter()
                    .skip_while(|s| {
                        request
                            .cursor
                            .as_ref()
                            .is_some_and(|cursor| s.session_id.as_str() <= cursor.as_str())
                    })
                    .take(limit_us)
                    .collect();
                let next_cursor = if items.len() == limit_us {
                    items.last().map(|i| i.session_id.clone())
                } else {
                    None
                };
                Ok(CoreHostQueryResponse {
                    sessions: Some(NexusAgentHostSessionListResponse {
                        items,
                        pagination: NexusPaginationInfo {
                            limit: i64::try_from(limit)
                                .map_err(|_| internal("pagination limit"))?,
                            has_more: next_cursor.is_some(),
                            next_cursor,
                        },
                    }),
                    health: None,
                    catalog: None,
                    session: None,
                    operation: None,
                    scan: None,
                })
            }
            CoreHostQueryQuery::GetSession => {
                let raw = request
                    .session_id
                    .as_deref()
                    .ok_or_else(|| invalid("session_id", "get_session requires session_id"))?;
                if let Ok(uuid) = Uuid::parse_str(raw) {
                    let native = self.sorted_sessions().await?;
                    if let Some(session) = native.iter().find(|s| {
                        s.id == HostSessionId(uuid)
                            && self
                                .registry
                                .stored_session_owner(&s.id)
                                .is_none_or(|(owner, _, _)| owner == principal.creator_id())
                    }) {
                        return Ok(CoreHostQueryResponse {
                            session: Some(self.session_response_wire(session)?),
                            health: None,
                            catalog: None,
                            sessions: None,
                            operation: None,
                            scan: None,
                        });
                    }
                }
                Err(CoreError::NotFound {
                    resource: format!("session {raw}"),
                })
            }
            CoreHostQueryQuery::GetOperation => {
                let raw = request.operation_id.as_deref().ok_or_else(|| {
                    invalid("operation_id", "get_operation requires operation_id")
                })?;
                if let Ok(uuid) = Uuid::parse_str(raw) {
                    let op_id = HostOperationId(uuid);
                    let native = self.sorted_sessions().await?;
                    if let Some(session) = native.iter().find(|s| {
                        s.state.active_op_id() == Some(&op_id)
                            && self
                                .registry
                                .stored_session_owner(&s.id)
                                .is_none_or(|(owner, _, _)| owner == principal.creator_id())
                    }) {
                        return Ok(CoreHostQueryResponse {
                            operation: Some(NexusAgentHostOperationResponse {
                                operation_id: op_id.to_string(),
                                session_id: session.id.to_string(),
                                status: operation_status_wire(&session.state, &op_id).to_string(),
                                capture: None,
                            }),
                            health: None,
                            catalog: None,
                            sessions: None,
                            session: None,
                            scan: None,
                        });
                    }
                }
                // Durable journal fallback: after a restart the in-memory
                // state is gone, but a previously active op is still queryable
                // as interrupted — read through the CoreService-owned journal.
                if let Ok(Some((operation_id, session_id, _provider_id, status))) =
                    self.core.provider_operation_row_internal(raw).await
                {
                    return Ok(CoreHostQueryResponse {
                        operation: Some(NexusAgentHostOperationResponse {
                            operation_id,
                            session_id,
                            status,
                            capture: None,
                        }),
                        health: None,
                        catalog: None,
                        sessions: None,
                        session: None,
                        scan: None,
                    });
                }
                Err(CoreError::NotFound {
                    resource: format!("operation {raw}"),
                })
            }
        }
    }

    /// Test-only settlement seam: run the authority-owned Character drain over
    /// a supplied host event stream — the exact settlement `execute` spawns —
    /// with no provider process, so the terminal contract (§5) is provable
    /// against real streams.
    ///
    /// Compiled out of the production build: `test-hooks` is default-off and is
    /// enabled only by this package's dev-dependency self-reference, the same
    /// rule as `execution::test_hooks` and `CoreService::pool`. Production
    /// settlement therefore consumes only the stream `execute` owns — a
    /// production build has no entry that accepts a caller-supplied stream, so
    /// no caller can fabricate terminal truth through one.
    #[cfg(any(test, feature = "test-hooks"))]
    pub async fn settle_character_stream(
        &self,
        snapshot: CharacterOperationSnapshot,
        stream: HostEventStream,
        fenced: Option<crate::actor_knowledge::AdmittedKnowledgeContext>,
    ) {
        drain_character_operation(self.registry.clone(), stream, snapshot, fenced).await;
    }

    /// Test-only settlement seam: record a Character operation's terminal
    /// status/reason directly, with no provider stream — the duplicate
    /// settlement the first-writer rule turns into a no-op and the bounded
    /// terminal retention window both need a terminal that already exists.
    ///
    /// Compiled out of the production build for the same reason as
    /// [`Self::settle_character_stream`]: the registry's own writer is
    /// crate-visible, so this is the test build's only entry into it and a
    /// production caller has none — not even through
    /// [`Self::actor_sessions`].
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn settle_character_terminal(
        &self,
        operation_id: &HostOperationId,
        run_status: CharacterOperationResultRunStatus,
        finish_reason: Option<CharacterOperationResultFinishReason>,
    ) {
        self.registry
            .settle_operation_terminal(operation_id, run_status, finish_reason);
    }

    /// Close the authority. Retired Actor sessions get one bounded shutdown
    /// attempt each; the registry maps close (in-flight creates cannot
    /// repopulate). Cleanup ownership is only released on a confirmed close:
    /// any unconfirmed drain — Host session, still-running Actor drain, or an
    /// admitted operation that has not transferred its drain yet — keeps the
    /// guards *and* the authority slot, and reports `interrupted`.
    /// A dead JS callback bridge is never awaited.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Closing`] when the service is already closing. A
    /// repeated close on this handle is idempotent after a confirmed cleanup
    /// and re-attempts an unsettled one.
    pub async fn close(&self) -> CoreResult<CoreCloseReport> {
        if let Some(report) = self
            .authority
            .retained_close_report()
            .filter(|report| report.cleanup_confirmed)
        {
            return Ok(report);
        }
        self.core.ensure_open()?;
        // Closing identity latches before the first await: later calls on THIS
        // handle are refused even while the cleanup is still settling, and a
        // later authority over the same service cannot be mistaken for it. The
        // same latch freezes admission, so the count it returns is every
        // admitted operation this close must account for.
        let in_flight = self.authority.latch_closing();
        let mut pending: Vec<String> = Vec::new();
        if in_flight > 0 {
            // An admitted operation that has not transferred its drain yet is
            // unaccounted cleanup: report it and keep the authority instead of
            // releasing the slot over work still in flight. Not waiting for it
            // here is deliberate — `quiesce_actor_sessions`/`close_before`
            // (contract §3) own the bounded join, and a repeated close
            // re-attempts this report.
            pending.push(format!("actor-admissions: {in_flight} in flight"));
        }
        if let Err(err) = self.registry.drain_host_sessions(self.host.as_ref()).await {
            pending.push(format!("actor-session-drain: {err}"));
        }
        self.registry.close();
        // The MANAGER + LocalSet settlement is one shared implementation, used
        // by this standalone close and by the native-ordered `close_before`.
        pending.extend(self.settle_shared_host(None).await);
        // Actor drains this authority minted stay owned here: settled handles
        // are dropped, an unsettled drain keeps the authority — and the
        // knowledge leases it holds — instead of being detached and forgotten.
        let unsettled_drains = self.registry.prune_settled_drains();
        if unsettled_drains > 0 {
            pending.push(format!("actor-drains: {unsettled_drains} unsettled"));
        }
        let report = close_report(pending);
        if report.cleanup_confirmed {
            // Close/release coordination with the established-owner slot: a
            // confirmed close frees the authority for a later open/attach; an
            // unconfirmed close keeps the slot held. The claim is one-shot for
            // this authority, so a concurrent confirmed close on a clone cannot
            // free the slot a second time and clear a successor's claim.
            self.release_authority_slot();
        }
        self.authority.retain_close_report(&report);
        Ok(report)
    }

    /// Actor-only quiesce of the close order (technical contract §3): latch the
    /// authority's own admission gate, close Actor admission, request supported
    /// cancellation for every active Actor operation, join the operations
    /// admitted before the latch, and join the retained Actor drains.
    ///
    /// Its confirmation is therefore the ordered-close proof the native owner
    /// needs before it may close core storage: every admitted operation has
    /// either failed or transferred its drain (the admission join), and every
    /// transferred drain has settled (the drain join). The zero registered-drain
    /// count alone is NOT that proof — registration happens after asynchronous
    /// admission and Host execution — so an admitted-but-untransferred operation
    /// keeps this quiesce from confirming until it has transferred and settled
    /// its drain, instead of being read as quiescence.
    ///
    /// It deliberately settles NOTHING outside the Actor side: it does not shut
    /// the manager used by workflows, does not settle the shared `LocalSet`, and
    /// does not release the Host authority slot. The native close owner runs it
    /// while the core authority is still open, then closes the existing core
    /// execution/service owner, and only then calls [`Self::close_before`] with
    /// the same outer deadline.
    ///
    /// The joins are deliberately unbounded here — the native owner bounds them
    /// with its own deadline — and they never close storage out from under a
    /// live drain: the handles stay owned by the registry, so a caller that
    /// runs out of budget retains the whole authority and retries instead of
    /// detaching live work.
    ///
    /// # Errors
    ///
    /// Never returns an error for an unsupported provider (its session is torn
    /// down by the settlement that follows); other cancellation failures are
    /// reported in the returned report's pending entries.
    pub async fn quiesce_actor_sessions(&self) -> CoreResult<CoreCloseReport> {
        let mut pending: Vec<String> = Vec::new();
        // Freeze this authority's own operation admission FIRST — the same
        // barrier `close` latches — so no create or execute can be admitted
        // while the quiesce runs, and the operations counted at that instant are
        // exactly the ones this quiesce must account for. The registry latch
        // below covers the Actor maps and the quiesce's own reads stay legal.
        self.authority.freeze_admissions();
        self.registry.close();
        // Join the admitted operations BEFORE the drains they become: an
        // operation admitted before the freeze registers its drain only after
        // asynchronous admission and Host execution, so the registered drains
        // alone are not proof of quiescence. Cancellation-safe, like the drain
        // join below: the native owner bounds this with its own deadline and a
        // retry joins the same admissions.
        self.authority.join_admissions().await;
        for operation_id in self.registry.nonterminal_operations() {
            if let Err(err) = self.request_cancel(&operation_id).await {
                // Neither an unsupported provider (DSH) nor an operation whose
                // session is already gone is a quiesce failure: nothing latched
                // intent in either case, and the manager settlement that
                // follows is the authoritative teardown.
                if !is_unsupported_cancellation(&err) && !is_missing_session(&err) {
                    pending.push(format!("actor-cancel: {operation_id}: {err}"));
                }
            }
        }
        let unsettled = self.registry.join_actor_drains().await;
        if unsettled > 0 {
            pending.push(format!("actor-drains: {unsettled} unsettled"));
        }
        Ok(close_report(pending))
    }

    /// Settle the shared manager and its `LocalSet` inside `deadline`, releasing
    /// the Host authority slot only on confirmed settlement (technical
    /// contract §3).
    ///
    /// This is the native close owner's step after the core service closed —
    /// so, unlike [`Self::close`], it never calls `ensure_open`: a closed
    /// service must not turn a legitimate cleanup into a `closing` refusal.
    /// It performs no second native shutdown; it settles the manager this
    /// authority was opened or attached over, once.
    ///
    /// # Errors
    ///
    /// Returns the report; unfinished work keeps the authority (slot + drain
    /// ownership) and reports `interrupted` rather than confirming a cleanup it
    /// cannot prove.
    pub async fn close_before(&self, deadline: Instant) -> CoreResult<CoreCloseReport> {
        if let Some(report) = self
            .authority
            .retained_close_report()
            .filter(|report| report.cleanup_confirmed)
        {
            return Ok(report);
        }
        let in_flight = self.authority.latch_closing();
        let mut pending: Vec<String> = Vec::new();
        if in_flight > 0 {
            pending.push(format!("actor-admissions: {in_flight} in flight"));
        }
        pending.extend(self.settle_shared_host(Some(deadline)).await);
        let unsettled_drains = self.registry.prune_settled_drains();
        if unsettled_drains > 0 {
            pending.push(format!("actor-drains: {unsettled_drains} unsettled"));
        }
        let report = close_report(pending);
        if report.cleanup_confirmed {
            // Same one-shot claim as [`Self::close`]: two confirmed closes on
            // clones of one authority free the established-owner slot once.
            self.release_authority_slot();
        }
        self.authority.retain_close_report(&report);
        Ok(report)
    }

    /// Free the service-wide established-owner slot for THIS authority, at
    /// most once.
    fn release_authority_slot(&self) {
        if self.authority.claim_slot_release() {
            self.core.release_host_authority();
        }
    }

    /// The ONE manager + `LocalSet` settlement shared by [`Self::close`] and
    /// [`Self::close_before`]; returns the pending entries that keep the
    /// cleanup unconfirmed.
    ///
    /// Order: freeze the shared `LocalSet` and settle it first, then shut the
    /// manager down. A live or panicking `LocalSet` thread is never a clean
    /// cleanup (a thread that panicked also reports `thread_alive == false`), so
    /// the manager shutdown behind it is not attempted — a shutdown behind a
    /// full `LocalSet` queue would block instead of settling. `None` keeps the
    /// standalone close's existing unbounded settle (the bridge's own join
    /// budget and the manager's per-session shutdown timeout stay the only
    /// policies); `Some(deadline)` is the native close's outer deadline.
    async fn settle_shared_host(&self, deadline: Option<Instant>) -> Vec<String> {
        let mut pending: Vec<String> = Vec::new();
        let bridge = self.host.localset_bridge();
        bridge.begin_drain();
        let evidence = match deadline {
            Some(deadline) => bridge.shutdown_before(deadline).await,
            None => bridge.shutdown().await,
        };
        if !evidence.is_settled() {
            pending.extend(evidence.unsettled_pending());
            return pending;
        }
        let shutdown = match deadline {
            Some(deadline) if Instant::now() >= deadline => None,
            Some(deadline) => tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                self.host.shutdown(),
            )
            .await
            .ok(),
            None => Some(self.host.shutdown().await),
        };
        match shutdown {
            Some(Ok(())) => {}
            Some(Err(err)) => pending.push(format!("host-shutdown: {err}")),
            None => pending.push("host-shutdown-deadline".to_string()),
        }
        pending
    }

    async fn sorted_sessions(&self) -> CoreResult<Vec<RegistryHostSession>> {
        let mut sessions = self.host.list_sessions().await.map_err(host_err)?;
        sessions.sort_by_key(|a| a.id.to_string());
        Ok(sessions)
    }

    /// Session list/get row: the Host row plus the Actor pair this authority
    /// indexes for it — the live admitted context first, the retired tombstone
    /// second — so a cached or native read never turns an Actor session into a
    /// provider-only session.
    ///
    /// # Errors
    /// Returns `internal` when a stored id fails its generated pattern check.
    fn session_response_wire(
        &self,
        session: &RegistryHostSession,
    ) -> CoreResult<NexusAgentHostSessionResponse> {
        let (actor_ref, viewpoint) = self.registry.echo_actor_pair_for_session(&session.id)?;
        Ok(NexusAgentHostSessionResponse {
            session_id: session.id.to_string(),
            provider_id: session.provider_id.to_string(),
            state: format!("{:?}", session.state),
            active_op_id: session
                .active_op_id
                .as_ref()
                .map(std::string::ToString::to_string),
            model: None,
            actor_ref,
            viewpoint,
        })
    }

    fn host_create_request(
        request: &CreateSessionRequest,
        canonical_root: &std::path::Path,
        owner_creator_id: &str,
    ) -> HostCreateRequest {
        HostCreateRequest {
            provider_id: nexus_agent_host::ProviderId::new(&request.provider_id),
            cwd: canonical_root.to_path_buf(),
            model: request.model.clone(),
            mode: request.mode.clone(),
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: SessionOwner {
                creator_id: owner_creator_id.to_string(),
                workspace_root: canonical_root.to_path_buf(),
                orchestration_run_id: None,
            },
        }
    }

    /// Rebuild prompt text for an Actor session from the re-admitted context:
    /// the admitted view plus, for a Character, the admitted Character's
    /// SOUL/Memory mind projection in the reserved mind slots.
    async fn assemble_admitted_prompt(
        &self,
        principal: &Principal,
        ctx: &crate::actors::AdmittedActorContext,
        user_prompt: String,
    ) -> CoreResult<String> {
        let creator_id = principal.creator_id();
        let pool = self.core.inner.pool.clone();
        let view = CharacterViewInput::from_entries(ctx.view.items.clone());
        let actor = match &ctx.actor {
            AdmittedActor::Character { character_id } => {
                let binding_id = ctx.binding_id.as_deref().ok_or_else(|| {
                    invalid(
                        "binding_id",
                        "Character host launch requires an active binding",
                    )
                })?;
                let mind = CoreCharacterMind::new(pool.clone(), self.core.inner.nexus_home.clone())
                    .projection_with_tom(creator_id, character_id, &ctx.world_id, binding_id)
                    .await?;
                MomentActorContext::character_with_mind(view, mind)
            }
            AdmittedActor::Creator { .. } => MomentActorContext::creator_with_view(view),
        };
        let mut request = MomentRequest::new(Stage0Assembly {
            user_prompt,
            ..Stage0Assembly::default()
        })
        .with_world(ctx.world_id.clone())
        .with_actor(actor)
        .with_max_tokens(DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET);
        if let Some(branch) = ctx.branch_id.clone() {
            request = request.with_branch(branch);
        }
        if let Some(event) = ctx.event_id.clone() {
            request = request.with_event(event);
        }
        if matches!(ctx.actor, AdmittedActor::Creator { .. }) {
            request = request.with_user(creator_id);
        }
        let narrative = SqliteNarrativeGateway::new(pool.clone());
        // Durable §4.2: the host prompt consumes the same complete ActorView
        // snapshot its admitted context was built from — the exact admitted
        // holder plus the authorized containers, never a management selection.
        let view_scope = self
            .core
            .actor_view_read_scope(
                principal,
                &ctx.actor,
                &ctx.world_id,
                ctx.binding_id.as_deref(),
            )
            .await?;
        let kb = SpokeBackedKbStore::new(pool.clone(), view_scope);
        let knowledge = SqliteKnowledgeStore::new(pool);
        let assembled = assemble_moment(&request, &narrative, &kb, &knowledge).await;
        Ok(assembled.to_full_context())
    }
}

fn internal(category: impl Into<String>) -> CoreError {
    CoreError::Internal {
        category: category.into(),
    }
}

/// The NOT-FOUND refusal of an operation this authority did not record (or
/// recorded for another owner): one shape, so a foreign id and a missing one
/// stay indistinguishable.
fn operation_not_found(operation_id: &HostOperationId) -> CoreError {
    CoreError::NotFound {
        resource: format!("operation {operation_id}"),
    }
}

/// Whether a cancel refusal is the `not_supported` capability refusal — a
/// provider (DSH) that cannot cancel at all, which is not a quiesce failure.
fn is_unsupported_cancellation(err: &CoreError) -> bool {
    matches!(err, CoreError::Coded { code, .. } if code == "not_supported")
}

/// Whether a cancel refusal is an operation whose session already left the
/// manager: nothing is left to cancel there, so the quiesce is not incomplete.
const fn is_missing_session(err: &CoreError) -> bool {
    matches!(err, CoreError::NotFound { .. })
}

/// Whether a cancel refusal is the `409 actor_operation_finished` phase race —
/// the operation finalized between the caller's nonterminal snapshot and the
/// cancel latch. Its recorded terminal is already its truth, so nothing is left
/// to cancel and the teardown is not incomplete.
fn is_finished_operation(err: &CoreError) -> bool {
    matches!(err, CoreError::ActorConflict { code, .. } if code == "actor_operation_finished")
}

/// The one `CoreCloseReport` shape every close path shares: a report only
/// claims a confirmed cleanup when it has no pending entries. Releasing the
/// established-owner slot stays with the caller, so the Actor-only quiesce can
/// report Actor-side settlement without touching the Host lifetime.
fn close_report(pending: Vec<String>) -> CoreCloseReport {
    if pending.is_empty() {
        CoreCloseReport {
            state: CoreCloseReportState::Closed,
            cleanup_confirmed: true,
            pending_operations: vec![],
            reason: None,
        }
    } else {
        CoreCloseReport {
            state: CoreCloseReportState::Interrupted,
            cleanup_confirmed: false,
            pending_operations: pending,
            // CoreCloseReportReason has no unconfirmed/drain-failure variant
            // (UserRequested/EngineReplaced/SchemaMismatch/WriterFenced);
            // omitting `reason` instead of mislabelling the cause.
            reason: None,
        }
    }
}

/// Map one provider-port wire error onto the core taxonomy.
///
/// The bounded observation reader is the only wire-typed component the
/// authority calls; its failures are transport-shaped (a concurrent pull or a
/// lost stream slot), so the taxonomy keeps `busy`/`not_found` and reports
/// anything else as an internal host-port failure.
fn port_err(err: nexus_contracts::CoreError) -> CoreError {
    match err.code {
        nexus_contracts::CoreErrorCode::NotFound => CoreError::NotFound {
            resource: err.message,
        },
        nexus_contracts::CoreErrorCode::Busy => CoreError::Busy,
        code => CoreError::Internal {
            category: format!("agent_host_port: {code:?}: {}", err.message),
        },
    }
}

fn session_cwd(request: &CreateSessionRequest, core: &CoreService) -> CoreResult<PathBuf> {
    // The request body never supplies the workspace root: the canonical open
    // boundary is the fallback.
    request.cwd.as_ref().map_or_else(
        || {
            validate_workspace_path(&core.inner.nexus_home)
                .map_err(|e| invalid("cwd", e.to_string()))
        },
        |cwd| {
            validate_workspace_path(std::path::Path::new(cwd))
                .map_err(|e| invalid("cwd", e.to_string()))
        },
    )
}

/// The session cwd for a Character Actor create: the admission's own
/// creative-root pin, never a second root authority.
///
/// The pin is the canonical root the engine-owner admission resolved ONCE at
/// open from the selected workspace's `meta.json` (§A2: no second
/// selected-metadata read). An omitted `cwd` uses that pin, and an explicit
/// `cwd` is accepted only when its canonical form — symlink aliases included —
/// is exactly equal to it. A descendant, a foreign or Nexus root, the user
/// home, a pin that is absent or was removed after open, and an unresolvable
/// explicit path are all `invalid_input` on `cwd`, refused here, before any
/// Host, provider or memory effect.
///
/// Compiled only with the `execution` edge that produces the pin: without it
/// the admission pins nothing, so the same refusal is the only honest answer
/// and `provider-host` still compiles on its own.
#[cfg(feature = "execution")]
fn character_session_cwd(
    request: &CreateSessionRequest,
    core: &CoreService,
) -> CoreResult<PathBuf> {
    let Some(pin) = core.admission_creative_root() else {
        return Err(invalid(
            "cwd",
            "a Character session requires an admitted creative root",
        ));
    };
    let pin = validate_workspace_path(pin).map_err(|e| invalid("cwd", e.to_string()))?;
    let Some(cwd) = request.cwd.as_ref() else {
        return Ok(pin);
    };
    let canonical = validate_workspace_path(std::path::Path::new(cwd))
        .map_err(|e| invalid("cwd", e.to_string()))?;
    if canonical == pin {
        return Ok(canonical);
    }
    Err(invalid(
        "cwd",
        format!(
            "a Character session cwd must be the admitted creative root '{}'",
            pin.display()
        ),
    ))
}

/// Character-root pin without the `execution` edge: this admission holds no
/// admitted creative root, so the create is refused rather than falling back
/// to a caller-supplied or default root.
#[cfg(not(feature = "execution"))]
fn character_session_cwd(
    _request: &CreateSessionRequest,
    _core: &CoreService,
) -> CoreResult<PathBuf> {
    Err(invalid(
        "cwd",
        "a Character session requires an admitted creative root",
    ))
}

const fn session_wire(
    session_id: String,
    provider_id: String,
    state: String,
    active_op_id: Option<String>,
    model: Option<String>,
    actor_ref: Option<
        nexus_contracts::generated::daemon_api::agent_host::session_response::NexusActorRef,
    >,
    viewpoint: Option<
        nexus_contracts::generated::daemon_api::agent_host::session_response::NexusSessionViewpoint,
    >,
) -> SessionResponse {
    SessionResponse {
        session_id,
        provider_id,
        state,
        active_op_id,
        model,
        actor_ref,
        viewpoint,
    }
}

/// Whether a broadcast event of the SAME Host belongs to this operation.
///
/// The manager broadcasts one session's events to every subscriber, so the
/// observation source is selected down to the exact `(session_id,
/// operation_id)` pair (technical contract §5) before the bounded reader ever
/// sees it: an unrelated operation or session event is dropped by the filter,
/// never deferred into the reader's pending window.
fn observation_matches(
    event: &HostEvent,
    session_id: &HostSessionId,
    operation_id: &HostOperationId,
) -> bool {
    // An operation-scoped event is selected by its exact operation id; a
    // session-scoped event (no operation id at all — session created/stopped
    // and status) is selected by the session the observation came from.
    event_operation_id(event).map_or_else(
        || event_session_id(event) == Some(session_id),
        |event_operation| event_operation == operation_id,
    )
}

/// The operation an event belongs to, when it carries one.
// The arms cannot be merged: each one binds its OWN payload type, so an
// or-pattern would not type-check even though the bodies read alike.
#[allow(clippy::match_same_arms)]
const fn event_operation_id(event: &HostEvent) -> Option<&HostOperationId> {
    match event {
        HostEvent::OpStarted(event) => Some(&event.op_id),
        HostEvent::ThoughtDelta(event) => Some(&event.op_id),
        HostEvent::MessageDelta(event) => Some(&event.op_id),
        HostEvent::ToolCall(event) => Some(&event.op_id),
        HostEvent::ToolCallUpdate(event) => Some(&event.op_id),
        HostEvent::PlanUpdate(event) => Some(&event.op_id),
        HostEvent::OpFinished(event) => Some(&event.op_id),
        HostEvent::OpFailed(event) => Some(&event.op_id),
        HostEvent::SessionCreated(_) | HostEvent::SessionStopped(_) | HostEvent::Status(_) => None,
    }
}

/// The session an event is scoped to, when it carries one.
#[allow(clippy::match_same_arms)] // one distinct payload type per arm, as above
const fn event_session_id(event: &HostEvent) -> Option<&HostSessionId> {
    match event {
        HostEvent::SessionCreated(event) => Some(&event.session_id),
        HostEvent::SessionStopped(event) => Some(&event.session_id),
        HostEvent::Status(event) => event.session_id.as_ref(),
        HostEvent::OpStarted(_)
        | HostEvent::ThoughtDelta(_)
        | HostEvent::MessageDelta(_)
        | HostEvent::ToolCall(_)
        | HostEvent::ToolCallUpdate(_)
        | HostEvent::PlanUpdate(_)
        | HostEvent::OpFinished(_)
        | HostEvent::OpFailed(_) => None,
    }
}

/// The observation source for one operation: the SAME Host's event broadcast,
/// selected to this exact `(session_id, operation_id)` pair.
///
/// A broadcast receiver lag or the loss of the broadcaster reaches the reader
/// as a source error / end, which the reader (in observation mode) reports as
/// the typed `lagging` resync gap — never as an EOF that a consumer could read
/// as a terminal. Nothing here reads the operation's own exec stream: that
/// stream stays the authority's drain, so observation and settlement are
/// independent.
fn observation_stream(
    receiver: tokio::sync::broadcast::Receiver<HostEvent>,
    session_id: HostSessionId,
    operation_id: HostOperationId,
) -> HostEventStream {
    Box::pin(futures_util::stream::unfold(
        (receiver, session_id, operation_id),
        |(mut receiver, session_id, operation_id)| async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        if observation_matches(&event, &session_id, &operation_id) {
                            return Some((Ok(event), (receiver, session_id, operation_id)));
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        return Some((
                            Err(nexus_agent_host::HostError::internal(
                                "host event observation lagged",
                            )),
                            (receiver, session_id, operation_id),
                        ));
                    }
                    // The broadcaster is gone: the source ends. The reader
                    // turns that into the typed resync gap.
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    ))
}

/// Server-owned drain for a Character operation: the admitted knowledge
/// context is held until settlement — its Character activity lease and its
/// World/Character shared knowledge leases (durable §4.3) — and released at the
/// FIRST matching terminal/fault (technical contract §5), never after unrelated
/// trailing producer events.
async fn drain_character_operation(
    registry: ActorSessionRegistry,
    mut stream: impl futures_util::Stream<Item = Result<HostEvent, nexus_agent_host::HostError>> + Unpin,
    snapshot: CharacterOperationSnapshot,
    fenced: Option<crate::actor_knowledge::AdmittedKnowledgeContext>,
) {
    use futures_util::StreamExt;
    // Authoritative terminal: the first observation of the ORIGINAL exec stream
    // about this exact `(session_id, operation_id)` pair. Unrelated
    // operation/session events are ignored, and EOF (or a stream error) before
    // any terminal is a fault — never an inferred success.
    let terminal = loop {
        match stream.next().await {
            Some(Ok(event)) => {
                if let Some(terminal) =
                    character_terminal_for(&event, &snapshot.session_id, &snapshot.operation_id)
                {
                    break terminal;
                }
            }
            // A stream error item and an exhausted stream are the same fault:
            // the run ended without this operation's observed terminal.
            Some(Err(_)) | None => break CharacterTerminal::Fault,
        }
    };
    // Finalizing is where the cancel/terminal race is decided under the
    // registry lock: a cancel that latched first won the race and is the
    // operation's truth.
    let cancel_won_the_race = registry.begin_operation_finalizing(&snapshot.operation_id);
    let (run_status, finish_reason) = character_terminal_outcome(&terminal, cancel_won_the_race);
    registry.settle_operation_terminal(&snapshot.operation_id, run_status, finish_reason);
    // The fences are released AT settlement: the activity guard and the shared
    // knowledge leases must not outlive the recorded outcome waiting for
    // trailing producer events.
    drop(fenced);
}

/// Server-owned drain for non-capture operations; events are broadcast by the
/// `HostManager`, draining drives the state machine. An indexed Actor session's
/// admitted knowledge context stays held for the whole stream.
async fn drain_plain(
    mut stream: impl futures_util::Stream<
            Item = Result<
                nexus_agent_host::capability::model::HostEvent,
                nexus_agent_host::HostError,
            >,
        > + Unpin,
    _fenced: Option<crate::actor_knowledge::AdmittedKnowledgeContext>,
) {
    use futures_util::StreamExt;
    while let Some(_result) = stream.next().await {}
}

impl ActorSessionKey {
    fn for_context(
        provider_id: &str,
        cwd: &std::path::Path,
        model: Option<String>,
        mode: Option<String>,
        ctx: &crate::actors::AdmittedActorContext,
        knowledge: crate::actor_knowledge::ActorKnowledgeIdentity,
    ) -> CoreResult<Self> {
        crate::actor_sessions::ActorSessionRegistry::key_for(
            provider_id,
            cwd,
            model,
            mode,
            ctx,
            knowledge,
        )
    }
}
