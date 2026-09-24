//! Process-lifetime Actor session registry, owned by the core Host authority
//! (P4-T2).
//!
//! Migrated from the daemon transport shim: registry IDs and storage are
//! transport-neutral here. Errors are [`CoreError`]; there is no HTTP error
//! type, no SQL pool parameter, and no per-registry fence table — Character
//! activity and transition fencing is delegated to the P2 [`CoreService`]
//! leases, so owner, tombstone and `lifecycle_epoch` are indexed exactly
//! once and the P2 lease is held through terminal capture by the caller.
//!
//! Indexes only Actor-mode sessions. Legacy creates never enter these maps.
//! Concurrent creates for one exact key serialize on a per-key lock.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use nexus_agent_host::capability::model::{FinishReason, HostEvent};
use nexus_agent_host::providers::port::ProviderEventReader;
use nexus_agent_host::{HostFacade, HostOperationId, HostSession, HostSessionId, SessionState};
use nexus_contracts::generated::core::core_host_query_response::{
    NexusActorRef as QueryActorRef, NexusSessionViewpoint as QueryViewpoint,
};
use nexus_contracts::generated::daemon_api::agent_host::character_operation_result::{
    CharacterOperationResult, CharacterOperationResultFinishReason,
    CharacterOperationResultRunStatus, NexusCharacterRunCaptureOutcome,
    NexusCharacterRunCaptureOutcomeStatus,
};
use nexus_contracts::generated::daemon_api::agent_host::session_response::{
    NexusActorRef, NexusSessionViewpoint, NexusSessionViewpointBindingId,
    NexusSessionViewpointBranchId, NexusSessionViewpointEventId,
};
use tokio::sync::Mutex as AsyncMutex;

use crate::actors::{AdmittedActor, AdmittedActorContext};
use crate::error::{CoreError, CoreResult};

/// Discriminant participating in exact Actor session equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActorSessionKind {
    Creator,
    Character,
}

/// Canonical exact-match key for an Actor-mode host session.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActorSessionKey {
    pub provider_id: String,
    pub canonical_cwd: PathBuf,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub actor_kind: ActorSessionKind,
    pub actor_id: String,
    pub world_id: String,
    pub binding_id: Option<String>,
    pub branch_id: Option<String>,
    pub event_id: Option<String>,
    /// Stored Character `lifecycle_epoch` at admission (`None` for Creator).
    /// A material archive/restore bumps the epoch, so a post-transition admit
    /// can never reuse a pre-transition session.
    pub character_epoch: Option<i64>,
    /// Knowledge fingerprint at admission (durable §4.3): the server-chosen
    /// read-policy kind plus the stored World/Character `knowledge_revision`
    /// pair. A material governance change (or a policy switch) retires the
    /// session on its next use instead of reusing a context that no longer
    /// describes the stored governance.
    pub knowledge: crate::actor_knowledge::ActorKnowledgeIdentity,
}

/// Outcome of revalidating one indexed session against a freshly admitted
/// knowledge fingerprint (durable §4.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnowledgeReuse {
    /// The recorded fingerprint still matches; the session stays indexed and
    /// its reuse key stays live.
    Reusable,
    /// The fingerprint diverged: the session was retired in place through the
    /// tombstone machinery and must never be reused. The id is returned for
    /// one physical Host shutdown attempt.
    Retired(HostSessionId),
}

/// The retained `409 actor_session_stale` refusal for an indexed/retired Actor
/// session whose admission no longer describes stored state.
pub(crate) fn actor_session_stale(session_id: &str) -> CoreError {
    CoreError::ActorConflict {
        code: "actor_session_stale".into(),
        message: format!("actor session {session_id} is stale"),
    }
}

struct IndexedActorSession {
    key: ActorSessionKey,
    ctx: AdmittedActorContext,
}

/// Retired-session tombstone: retains owner + Actor identity so retired ids
/// stay recognizably Actor-mode (never legacy) and authorize owner-only
/// safety/observation operations.
#[derive(Debug, Clone)]
struct RetiredActorSession {
    owner_creator_id: String,
    actor_kind: ActorSessionKind,
    actor_id: String,
    world_id: String,
    binding_id: Option<String>,
    branch_id: Option<String>,
    event_id: Option<String>,
}

/// Internal phase for per-operation cancel/finalize ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperationPhase {
    Running,
    CancelRequested,
    Finalizing,
    Terminal,
}

/// Immutable admission snapshot for one Character Host operation.
///
/// Character capture is refused before reservation (technical contract §5), so
/// a snapshot describes only an uncaptured run: there is no `remember` request
/// and no prompt digest to carry.
#[derive(Debug, Clone)]
pub struct CharacterOperationSnapshot {
    pub owner_creator_id: String,
    pub ctx: AdmittedActorContext,
    pub session_id: HostSessionId,
    pub operation_id: HostOperationId,
}

struct CharacterOperationRecord {
    owner_creator_id: String,
    session_id: HostSessionId,
    phase: OperationPhase,
    outcome: CharacterOperationResult,
    /// The operation's bounded observation reader, once installed.
    ///
    /// It lives INSIDE the record on purpose. Reservation is the only way to
    /// create a record, so a reader can neither be installed for an id that
    /// holds no reserved Character operation nor outnumber the records; and
    /// every path that drops a record — reservation removal after a failed exec
    /// admission, terminal-FIFO eviction — drops its reader with it. `close`
    /// and session shutdown clear the slot of the records they retire.
    observation: Option<Arc<ProviderEventReader>>,
    _seq: u64,
}

/// Live work of ONE session plus its settlement signal — the session-scoped
/// twin of [`ActorSessionRegistry`]'s registry-wide `live_drains`/`drain_settled`
/// pair, so a session shutdown joins exactly the work it is retiring instead of
/// the whole authority's.
///
/// `live` counts BOTH halves of the authority's admission/close barrier, at
/// session scope: the retained drains of the session, and the operations
/// ADMITTED for it ([`SessionAdmission`]) whose drain has not transferred to
/// the registry yet. The authority-wide barrier keeps the two apart because
/// `close` REPORTS them apart; a session join only has to wait for both, so
/// they share one counter. An entry exists only while that session has live
/// work: the session's last live unit removes it.
#[derive(Default)]
struct SessionLiveness {
    live: AtomicUsize,
    settled: tokio::sync::Notify,
}

/// One admitted operation's slot in its SESSION's live-work accounting — the
/// session-scoped arm of the same barrier the Host handle's `AuthorityAdmission`
/// holds authority-wide.
///
/// The guard is taken before the operation's first await and retired only after
/// its drain has transferred to this same accounting (or after it failed before
/// creating one), so a joiner observes either the admission or the drain it
/// became: no window remains in which a session reads as having no live work
/// while an operation admitted for it can still register a drain.
pub(crate) struct SessionAdmission {
    maps: Arc<Mutex<RegistryMaps>>,
    session_id: HostSessionId,
    liveness: Arc<SessionLiveness>,
}

impl Drop for SessionAdmission {
    fn drop(&mut self) {
        retire_session_liveness(&self.maps, &self.session_id, &self.liveness);
    }
}

const MAX_NONTERMINAL_OPERATIONS: usize = 128;
const MAX_TERMINAL_OPERATIONS: usize = 1024;

/// The reserved, still-running outcome of one Character operation.
///
/// Capture is always `disabled` (technical contract §5): a Character
/// `remember:true` prompt is refused before reservation, so no reservation can
/// describe a pending capture and no result may claim one.
fn running_outcome(snapshot: &CharacterOperationSnapshot) -> CharacterOperationResult {
    CharacterOperationResult {
        operation_id: snapshot.operation_id.to_string(),
        session_id: snapshot.session_id.to_string(),
        run_status: CharacterOperationResultRunStatus::Running,
        finish_reason: None,
        capture: NexusCharacterRunCaptureOutcome {
            status: NexusCharacterRunCaptureOutcomeStatus::Disabled,
            pending_id: None,
            code: None,
        },
    }
}

/// The owner-authorized accepted cancellation's terminal (§5): an accepted
/// local cancel classifies `cancelled`, never a rollback claim about provider
/// effects. Capture stays `disabled` like every Character result in this batch.
pub(crate) fn cancelled_outcome(
    session_id: &HostSessionId,
    operation_id: &HostOperationId,
) -> CharacterOperationResult {
    CharacterOperationResult {
        operation_id: operation_id.to_string(),
        session_id: session_id.to_string(),
        run_status: CharacterOperationResultRunStatus::Cancelled,
        finish_reason: Some(CharacterOperationResultFinishReason::Cancelled),
        capture: NexusCharacterRunCaptureOutcome {
            status: NexusCharacterRunCaptureOutcomeStatus::Disabled,
            pending_id: None,
            code: None,
        },
    }
}

/// Authoritative terminal truth of one Character exec stream (technical
/// contract §5): what the FIRST matching observation of the original
/// `HostFacade::exec` stream says about the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CharacterTerminal {
    /// A matching `OpFinished`: the provider named the stop reason.
    Finished(FinishReason),
    /// A matching fault: a matching `OpFailed` of ANY error category, a stream
    /// error, a matching `SessionStopped`, or EOF before any terminal.
    Fault,
}

/// Fold one host event into this operation's terminal truth.
///
/// `None` means the observation is not about the exact
/// `(session_id, operation_id)` pair — an unrelated operation or session event,
/// which contract §5 requires the drain to ignore — or is a non-terminal
/// progress event of this operation.
pub(crate) fn character_terminal_for(
    event: &HostEvent,
    session_id: &HostSessionId,
    operation_id: &HostOperationId,
) -> Option<CharacterTerminal> {
    match event {
        HostEvent::OpFinished(finished)
            if &finished.session_id == session_id && &finished.op_id == operation_id =>
        {
            Some(CharacterTerminal::Finished(finished.reason.clone()))
        }
        // Contract §5 has ONE row for `OpFailed` — failed with no finish
        // reason — and no error-category exception. A named provider stop
        // reason reaches the `incomplete` row only as an explicit
        // `OpFinished(MaxTokens | MaxTurnRequests | Refusal)`. An adapter that
        // reports such a reason as an `OpFailed` category (e.g.
        // `providers/acp.rs`) is therefore read as the failure it is: the
        // adapter's representation may not amend the terminal table.
        HostEvent::OpFailed(failed)
            if &failed.session_id == session_id && &failed.op_id == operation_id =>
        {
            Some(CharacterTerminal::Fault)
        }
        // The session this operation runs on stopped before a terminal: the run
        // can no longer complete honestly, so it is a fault.
        HostEvent::SessionStopped(stopped) if &stopped.session_id == session_id => {
            Some(CharacterTerminal::Fault)
        }
        _ => None,
    }
}

/// Contract §5 classification of a drained terminal, honouring a cancellation
/// that won the phase race (the finalizing transition found `CancelRequested`):
/// an accepted local cancel is the operation's truth, never a rollback claim
/// about provider effects.
#[must_use]
pub(crate) const fn character_terminal_outcome(
    terminal: &CharacterTerminal,
    cancel_won_the_race: bool,
) -> (
    CharacterOperationResultRunStatus,
    Option<CharacterOperationResultFinishReason>,
) {
    if cancel_won_the_race {
        return (
            CharacterOperationResultRunStatus::Cancelled,
            Some(CharacterOperationResultFinishReason::Cancelled),
        );
    }
    match terminal {
        CharacterTerminal::Finished(FinishReason::EndTurn) => (
            CharacterOperationResultRunStatus::Succeeded,
            Some(CharacterOperationResultFinishReason::EndTurn),
        ),
        CharacterTerminal::Finished(FinishReason::MaxTokens) => (
            CharacterOperationResultRunStatus::Incomplete,
            Some(CharacterOperationResultFinishReason::MaxTokens),
        ),
        CharacterTerminal::Finished(FinishReason::MaxTurnRequests) => (
            CharacterOperationResultRunStatus::Incomplete,
            Some(CharacterOperationResultFinishReason::MaxTurnRequests),
        ),
        CharacterTerminal::Finished(FinishReason::Refusal) => (
            CharacterOperationResultRunStatus::Incomplete,
            Some(CharacterOperationResultFinishReason::Refusal),
        ),
        CharacterTerminal::Finished(FinishReason::Cancelled) => (
            CharacterOperationResultRunStatus::Cancelled,
            Some(CharacterOperationResultFinishReason::Cancelled),
        ),
        CharacterTerminal::Fault => (CharacterOperationResultRunStatus::Failed, None),
    }
}

struct RegistryMaps {
    by_key: HashMap<ActorSessionKey, HostSessionId>,
    by_session: HashMap<HostSessionId, IndexedActorSession>,
    key_locks: HashMap<ActorSessionKey, Arc<AsyncMutex<()>>>,
    retired: HashMap<HostSessionId, RetiredActorSession>,
    /// Indexed Actor operations whose Host session may not yet expose
    /// `active_op_id` (fail-closed cancel authorization).
    indexed_operations: HashMap<HostOperationId, HostSessionId>,
    character_operations: HashMap<HostOperationId, CharacterOperationRecord>,
    terminal_fifo: VecDeque<HostOperationId>,
    operation_seq: u64,
    /// Live-work accounting per session, so a session shutdown joins the
    /// drains AND the admitted operations of the session it retires — the
    /// registered drains alone are not that proof, because an operation
    /// registers its drain only after asynchronous admission. An entry exists
    /// only while that session has live work: the session's last live unit
    /// removes it.
    session_liveness: HashMap<HostSessionId, Arc<SessionLiveness>>,
    /// Actor drains this authority minted, in spawn order. Retained until they
    /// settle, so an authority close that cannot confirm them keeps owning
    /// them — and the knowledge leases they hold — instead of detaching live
    /// work and forgetting it.
    drains: Vec<tokio::task::JoinHandle<()>>,
    closed: bool,
}

/// Process-lifetime maps: `key -> HostSessionId` and `HostSessionId -> context`.
#[derive(Clone)]
pub struct ActorSessionRegistry {
    maps: Arc<Mutex<RegistryMaps>>,
    /// Actor drains minted by this authority that have not settled. The
    /// authority-owned join (`join_actor_drains`) reads it, so a join that is
    /// cancelled by an outer deadline loses nothing: the handle stays in the
    /// registry and a retry joins it.
    live_drains: Arc<AtomicUsize>,
    /// Signalled once per settled drain, so the join waits on an event instead
    /// of polling task handles.
    drain_settled: Arc<tokio::sync::Notify>,
}

impl Default for ActorSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

const fn shutting_down() -> CoreError {
    CoreError::Interrupted
}

fn host_err(err: &nexus_agent_host::HostError) -> CoreError {
    CoreError::Internal {
        category: format!("agent_host: {err}"),
    }
}

/// Publish ONE unit of a session's live work settling: drop its count, wake the
/// joiners, and let the session's accounting entry die with its last unit.
///
/// Both live units of a session — a settling drain and a retired admission —
/// retire through this one path, so they cannot disagree about publication
/// order: the count drops BEFORE the wakeup, so a joiner that armed its
/// notification and then read a non-zero count still gets the permit this
/// retirement publishes.
fn retire_session_liveness(
    maps: &Mutex<RegistryMaps>,
    session_id: &HostSessionId,
    liveness: &Arc<SessionLiveness>,
) {
    liveness.live.fetch_sub(1, Ordering::AcqRel);
    liveness.settled.notify_one();
    let mut maps = maps.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("actor_sessions mutex poisoned, recovering");
        poisoned.into_inner()
    });
    if liveness.live.load(Ordering::Acquire) == 0
        && maps
            .session_liveness
            .get(session_id)
            .is_some_and(|stored| Arc::ptr_eq(stored, liveness))
    {
        maps.session_liveness.remove(session_id);
    }
}

/// Settle one reserved Character operation exactly once, under the registry
/// lock: the first writer records the terminal phase, the outcome update and
/// the retention slot; every later settlement is a no-op, so the recorded truth
/// is immutable and the terminal FIFO never sees a duplicate.
fn settle_terminal_locked(
    maps: &mut RegistryMaps,
    operation_id: &HostOperationId,
    update: impl FnOnce(&mut CharacterOperationRecord),
) {
    let Some(record) = maps.character_operations.get_mut(operation_id) else {
        return;
    };
    if matches!(record.phase, OperationPhase::Terminal) {
        return;
    }
    update(record);
    record.phase = OperationPhase::Terminal;
    maps.terminal_fifo.push_back(operation_id.clone());
    while maps.terminal_fifo.len() > MAX_TERMINAL_OPERATIONS {
        if let Some(evicted) = maps.terminal_fifo.pop_front() {
            // Removing the record drops its observation reader with it: an
            // evicted operation has no detailed outcome left to observe.
            maps.character_operations.remove(&evicted);
        }
    }
}

impl ActorSessionRegistry {
    /// Empty process-lifetime registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            maps: Arc::new(Mutex::new(RegistryMaps {
                by_key: HashMap::new(),
                by_session: HashMap::new(),
                key_locks: HashMap::new(),
                retired: HashMap::new(),
                indexed_operations: HashMap::new(),
                character_operations: HashMap::new(),
                terminal_fifo: VecDeque::new(),
                operation_seq: 0,
                session_liveness: HashMap::new(),
                drains: Vec::new(),
                closed: false,
            })),
            live_drains: Arc::new(AtomicUsize::new(0)),
            drain_settled: Arc::new(tokio::sync::Notify::new()),
        }
    }

    fn maps(&self) -> std::sync::MutexGuard<'_, RegistryMaps> {
        self.maps.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("actor_sessions mutex poisoned, recovering");
            poisoned.into_inner()
        })
    }

    /// Canonicalize cwd with the Agent Host workspace-root helper.
    ///
    /// # Errors
    ///
    /// Returns `invalid_input` when the path is relative, traverses, or
    /// cannot be resolved.
    pub fn canonicalize_cwd(cwd: &Path) -> CoreResult<PathBuf> {
        nexus_agent_host::config::validate_workspace_path(cwd).map_err(|e| {
            CoreError::InvalidInput {
                field: "cwd".into(),
                reason: e.to_string(),
            }
        })
    }

    /// Build the exact tuple key from an admitted Actor context and its
    /// admitted knowledge fingerprint.
    ///
    /// # Errors
    ///
    /// Returns `invalid_input` when `cwd` cannot be canonicalized.
    pub fn key_for(
        provider_id: &str,
        cwd: &Path,
        model: Option<String>,
        mode: Option<String>,
        ctx: &AdmittedActorContext,
        knowledge: crate::actor_knowledge::ActorKnowledgeIdentity,
    ) -> CoreResult<ActorSessionKey> {
        let (actor_kind, actor_id) = match &ctx.actor {
            AdmittedActor::Creator { creator_id } => {
                (ActorSessionKind::Creator, creator_id.clone())
            }
            AdmittedActor::Character { character_id } => {
                (ActorSessionKind::Character, character_id.clone())
            }
        };
        Ok(ActorSessionKey {
            provider_id: provider_id.to_string(),
            canonical_cwd: Self::canonicalize_cwd(cwd)?,
            model,
            mode,
            actor_kind,
            actor_id,
            world_id: ctx.world_id.clone(),
            binding_id: ctx.binding_id.clone(),
            branch_id: ctx.branch_id.clone(),
            event_id: ctx.event_id.clone(),
            character_epoch: ctx.character_epoch,
            knowledge,
        })
    }

    /// Revalidate one indexed Actor session against a freshly admitted
    /// knowledge fingerprint (durable §4.3): the session is reusable only
    /// while the read-policy kind and both stored knowledge revisions still
    /// match. On divergence the session is retired in place through the
    /// existing tombstone machinery so its reuse key can never match again,
    /// and the caller admits a fresh context under the new fingerprint.
    ///
    /// # Errors
    ///
    /// Returns `actor_session_stale` when the id is not a live indexed Actor
    /// session (retired ids and legacy ids are never reusable).
    #[allow(clippy::significant_drop_tightening)] // the session map guard is held across the identity comparison and the retire
    pub fn revalidate_knowledge(
        &self,
        session_id: &HostSessionId,
        fresh: &crate::actor_knowledge::ActorKnowledgeIdentity,
    ) -> CoreResult<KnowledgeReuse> {
        let mut maps = self.maps();
        let stored = maps
            .by_session
            .get(session_id)
            .map(|row| (row.key.clone(), row.key.knowledge == *fresh));
        let Some((key, matches)) = stored else {
            return Err(actor_session_stale(&session_id.to_string()));
        };
        if matches {
            return Ok(KnowledgeReuse::Reusable);
        }
        Self::evict_locked(&mut maps, &key, session_id);
        maps.indexed_operations.retain(|_, sid| sid != session_id);
        Ok(KnowledgeReuse::Retired(session_id.clone()))
    }

    /// Admitted context for an indexed Actor session, if any.
    #[must_use]
    pub fn context_for(&self, session_id: &HostSessionId) -> Option<AdmittedActorContext> {
        self.maps()
            .by_session
            .get(session_id)
            .map(|row| row.ctx.clone())
    }

    /// True when the id is currently indexed or was retired (never treated as legacy).
    #[must_use]
    pub fn is_actor_session(&self, session_id: &HostSessionId) -> bool {
        let maps = self.maps();
        maps.by_session.contains_key(session_id) || maps.retired.contains_key(session_id)
    }

    /// Stored owner + retirement state of an indexed or retired Actor session.
    ///
    /// `None` means never indexed (legacy/Creator-direct path, unchanged
    /// behavior). Retired tombstones retain owner identity so a foreign id is
    /// denied before any Host access.
    #[must_use]
    pub fn stored_session_owner(
        &self,
        session_id: &HostSessionId,
    ) -> Option<(String, ActorSessionKind, bool)> {
        let maps = self.maps();
        if let Some(row) = maps.by_session.get(session_id) {
            return Some((row.ctx.owner_creator_id.clone(), row.key.actor_kind, false));
        }
        maps.retired
            .get(session_id)
            .map(|t| (t.owner_creator_id.clone(), t.actor_kind, true))
    }

    #[must_use]
    fn retired_tombstone(&self, session_id: &HostSessionId) -> Option<RetiredActorSession> {
        self.maps().retired.get(session_id).cloned()
    }

    /// Record an indexed Actor operation for fail-closed cancel authorization
    /// when the Host has not yet bound `active_op_id`.
    pub fn register_indexed_operation(&self, op_id: HostOperationId, session_id: HostSessionId) {
        if self.is_actor_session(&session_id) {
            self.maps().indexed_operations.insert(op_id, session_id);
        }
    }

    /// Resolve a cancel target to its indexed Actor session when Host listing
    /// has not yet bound `active_op_id`.
    #[must_use]
    pub fn resolve_indexed_operation_session(
        &self,
        op_id: &HostOperationId,
    ) -> Option<HostSessionId> {
        self.maps().indexed_operations.get(op_id).cloned()
    }

    /// Install the bounded observation reader ON an already-reserved Character
    /// operation, before the Host starts executing it.
    ///
    /// The reader is a field of the operation record, so retention cannot be
    /// minted for an id that holds no reservation nor outnumber the records:
    /// an unknown (unreserved, foreign or already evicted) id is `not_found`
    /// and a closing registry is `interrupted`, instead of a silent arbitrary
    /// insert. One reader per reserved operation, dropped with its record on
    /// reservation removal, terminal eviction, session shutdown or close.
    ///
    /// # Errors
    ///
    /// Returns `not_found` when no record holds the id, and `interrupted` once
    /// the registry is closing.
    pub fn retain_observation(
        &self,
        operation_id: &HostOperationId,
        reader: ProviderEventReader,
    ) -> CoreResult<()> {
        let mut maps = self.maps();
        Self::reject_if_closed(&maps)?;
        let Some(record) = maps.character_operations.get_mut(operation_id) else {
            return Err(CoreError::NotFound {
                resource: format!("operation {operation_id} reservation"),
            });
        };
        record.observation = Some(Arc::new(reader));
        Ok(())
    }

    /// The observation reader of one operation, if it is still retained.
    #[must_use]
    pub fn observation(&self, operation_id: &HostOperationId) -> Option<Arc<ProviderEventReader>> {
        self.maps()
            .character_operations
            .get(operation_id)
            .and_then(|record| record.observation.clone())
    }

    /// Bounded observation-window occupancy (tests / diagnostics): the records
    /// that carry a reader, which can never outnumber the operation records.
    #[must_use]
    pub fn observation_count(&self) -> usize {
        self.maps()
            .character_operations
            .values()
            .filter(|record| record.observation.is_some())
            .count()
    }

    /// Reserve a Character operation outcome before Host exec.
    ///
    /// # Errors
    ///
    /// Returns capacity or shutdown conflicts.
    pub fn reserve_character_operation(
        &self,
        snapshot: &CharacterOperationSnapshot,
    ) -> CoreResult<()> {
        let mut maps = self.maps();
        Self::reject_if_closed(&maps)?;
        let nonterminal = maps
            .character_operations
            .values()
            .filter(|r| !matches!(r.phase, OperationPhase::Terminal))
            .count();
        if nonterminal >= MAX_NONTERMINAL_OPERATIONS {
            return Err(CoreError::Busy);
        }
        maps.operation_seq += 1;
        let seq = maps.operation_seq;
        maps.character_operations.insert(
            snapshot.operation_id.clone(),
            CharacterOperationRecord {
                owner_creator_id: snapshot.owner_creator_id.clone(),
                session_id: snapshot.session_id.clone(),
                phase: OperationPhase::Running,
                outcome: running_outcome(snapshot),
                observation: None,
                _seq: seq,
            },
        );
        drop(maps);
        Ok(())
    }

    /// Owner-scoped authoritative operation outcome.
    ///
    /// # Errors
    ///
    /// Returns `NotFound` when the operation is missing or foreign.
    pub fn character_operation_result(
        &self,
        owner_creator_id: &str,
        operation_id: &HostOperationId,
    ) -> CoreResult<CharacterOperationResult> {
        let maps = self.maps();
        let record =
            maps.character_operations
                .get(operation_id)
                .ok_or_else(|| CoreError::NotFound {
                    resource: format!("operation {operation_id}"),
                })?;
        if record.owner_creator_id != owner_creator_id {
            drop(maps);
            return Err(CoreError::NotFound {
                resource: format!("operation {operation_id}"),
            });
        }
        let outcome = record.outcome.clone();
        drop(maps);
        Ok(outcome)
    }

    /// Whether `operation_id` is a recorded Character operation owned by
    /// `owner_creator_id` — the owner-scoped gate the control path applies
    /// before any mutation. A missing or foreign id is `false`, never a leak.
    #[must_use]
    pub fn operation_owned_by(
        &self,
        owner_creator_id: &str,
        operation_id: &HostOperationId,
    ) -> bool {
        self.maps()
            .character_operations
            .get(operation_id)
            .is_some_and(|record| record.owner_creator_id == owner_creator_id)
    }

    /// Every recorded operation whose phase is not terminal yet — the work an
    /// Actor-only quiesce must request cancellation for.
    #[must_use]
    pub fn nonterminal_operations(&self) -> Vec<HostOperationId> {
        self.maps()
            .character_operations
            .iter()
            .filter(|(_, record)| !matches!(record.phase, OperationPhase::Terminal))
            .map(|(operation_id, _)| operation_id.clone())
            .collect()
    }

    /// The nonterminal operations recorded for one session — the work a
    /// session shutdown cancels before it tears the session down.
    #[must_use]
    pub fn nonterminal_operations_for_session(
        &self,
        session_id: &HostSessionId,
    ) -> Vec<HostOperationId> {
        self.maps()
            .character_operations
            .iter()
            .filter(|(_, record)| {
                &record.session_id == session_id
                    && !matches!(record.phase, OperationPhase::Terminal)
            })
            .map(|(operation_id, _)| operation_id.clone())
            .collect()
    }

    /// Remove an unstarted reservation after exec admission failure, dropping
    /// the reader it had already installed with the record.
    pub fn remove_operation_reservation(&self, operation_id: &HostOperationId) {
        self.maps().character_operations.remove(operation_id);
    }

    /// Latch cancel intent before awaiting Host cancel.
    ///
    /// # Errors
    ///
    /// Returns `NotFound` or `interrupted` (`actor_operation_finished`) when
    /// cancel is invalid.
    pub fn request_operation_cancel(
        &self,
        owner_creator_id: &str,
        operation_id: &HostOperationId,
    ) -> CoreResult<()> {
        let mut maps = self.maps();
        let record = maps
            .character_operations
            .get_mut(operation_id)
            .ok_or_else(|| CoreError::NotFound {
                resource: format!("operation {operation_id}"),
            })?;
        if record.owner_creator_id != owner_creator_id {
            return Err(CoreError::NotFound {
                resource: format!("operation {operation_id}"),
            });
        }
        let result = match record.phase {
            OperationPhase::Running => {
                record.phase = OperationPhase::CancelRequested;
                Ok(())
            }
            OperationPhase::CancelRequested => Ok(()),
            OperationPhase::Finalizing | OperationPhase::Terminal => {
                Err(CoreError::ActorConflict {
                    code: "actor_operation_finished".into(),
                    message: format!("operation {operation_id} already finished"),
                })
            }
        };
        drop(maps);
        result
    }

    /// Undo a latched cancel intent after the provider refused it, so a
    /// refusal can never be recorded as an accepted cancellation.
    ///
    /// Only a still-unfinalized `CancelRequested` returns to `Running`: once
    /// the terminal race has moved the phase on, the recorded outcome is
    /// decided by the phase rules, not by this rollback: once the race has
    /// moved the phase on, this is a no-op.
    pub fn rollback_operation_cancel(&self, operation_id: &HostOperationId) {
        let mut maps = self.maps();
        let phase = maps.character_operations.get_mut(operation_id);
        if let Some(record) = phase {
            if matches!(record.phase, OperationPhase::CancelRequested) {
                record.phase = OperationPhase::Running;
            }
        }
        drop(maps);
    }

    /// Move a draining operation into finalization and return whether cancel
    /// intent was latched. The cancel read and phase transition are atomic
    /// under the registry lock so no window can observe stale cancel state.
    #[must_use]
    pub fn begin_operation_finalizing(&self, operation_id: &HostOperationId) -> bool {
        let mut maps = self.maps();
        if let Some(record) = maps.character_operations.get_mut(operation_id) {
            let cancel_requested = matches!(record.phase, OperationPhase::CancelRequested);
            if matches!(
                record.phase,
                OperationPhase::Running | OperationPhase::CancelRequested
            ) {
                record.phase = OperationPhase::Finalizing;
            }
            cancel_requested
        } else {
            false
        }
    }

    /// Commit terminal outcome and enforce terminal FIFO retention.
    ///
    /// Crate-visible because terminal truth is written only from inside the
    /// authority that owns the original exec stream ([`crate::host`]): the
    /// `execute`-spawned drain and the owner-authorized cancel path. Outside
    /// this crate the public [`ActorSessionRegistry`] accessor
    /// ([`crate::HostHandle::actor_sessions`]) therefore exposes no terminal
    /// writer at all; a test build reaches one only through the
    /// `test-hooks`-gated seam on [`crate::HostHandle`].
    ///
    /// The owner-authorized accepted cancel is this writer's in-crate
    /// consumer: the provider accepted the cancellation, so the §5 cancelled
    /// row IS the operation's truth even if no drain ever settles (a lost or
    /// silent stream). A drain that settled first stays authoritative.
    ///
    /// The FIRST terminal/fault settlement wins: a later settlement of the same
    /// operation (a duplicate terminal, a trailing producer fault, or a cancel
    /// that lost the phase race) leaves the recorded truth untouched and never
    /// re-enters the retention FIFO.
    pub(crate) fn commit_operation_terminal(
        &self,
        operation_id: &HostOperationId,
        outcome: CharacterOperationResult,
    ) {
        let mut maps = self.maps();
        settle_terminal_locked(&mut maps, operation_id, |record| {
            record.outcome = outcome;
        });
    }

    /// Commit a drained Host operation's terminal run status against the
    /// reserved record (authority-owned drain). The capture half stays the
    /// reserved `disabled` value: this batch has no capture writer to settle it
    /// (technical contract §5).
    ///
    /// Crate-visible for the same reason as
    /// [`Self::commit_operation_terminal`]: the authority-owned drain is the
    /// only production writer, so a caller outside this crate cannot record
    /// terminal status/reason through the public registry accessor.
    ///
    /// Settles once, like [`Self::commit_operation_terminal`].
    pub(crate) fn settle_operation_terminal(
        &self,
        operation_id: &HostOperationId,
        run_status: CharacterOperationResultRunStatus,
        finish_reason: Option<CharacterOperationResultFinishReason>,
    ) {
        let mut maps = self.maps();
        settle_terminal_locked(&mut maps, operation_id, |record| {
            record.outcome.run_status = run_status;
            record.outcome.finish_reason = finish_reason;
        });
    }

    #[must_use]
    pub fn operation_owner(&self, operation_id: &HostOperationId) -> Option<String> {
        self.maps()
            .character_operations
            .get(operation_id)
            .map(|r| r.owner_creator_id.clone())
    }

    #[must_use]
    pub fn operation_session_id(&self, operation_id: &HostOperationId) -> Option<HostSessionId> {
        self.maps()
            .character_operations
            .get(operation_id)
            .map(|r| r.session_id.clone())
    }

    /// Drop a registered indexed operation after cancel or terminal completion.
    pub fn clear_indexed_operation(&self, op_id: &HostOperationId) {
        self.maps().indexed_operations.remove(op_id);
    }

    /// Overlay `actor_ref/viewpoint` for the native list/get rows: the live
    /// indexed context first, then its retired tombstone, so a leftover Host
    /// row stays recognizable as an Actor session.
    ///
    /// The core-query DTO family carries its own generated `ActorRef` /
    /// `Viewpoint` types, so the authority's wire echo is re-created through
    /// their checked constructors instead of being reinterpreted.
    ///
    /// # Errors
    ///
    /// Returns `internal` when stored ids fail generated pattern checks.
    pub fn echo_actor_pair_for_session(
        &self,
        session_id: &HostSessionId,
    ) -> CoreResult<(Option<QueryActorRef>, Option<QueryViewpoint>)> {
        let echoed = if let Some(ctx) = self.context_for(session_id) {
            echo_actor_pair(&ctx)?
        } else if let Some(tombstone) = self.retired_tombstone(session_id) {
            echo_retired_pair(&tombstone)?
        } else {
            return Ok((None, None));
        };
        Ok((
            echoed.0.map(query_actor_ref).transpose()?,
            echoed
                .1
                .map(|viewpoint| query_viewpoint(&viewpoint))
                .transpose()?,
        ))
    }

    /// True once the process-lifetime maps are closed (authority shutdown).
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.maps().closed
    }

    /// Retire every indexed session of a Character after a **material**
    /// committed transition, while the exclusive lease is still held:
    /// remove old-epoch reuse keys and move their ids to owner-retaining
    /// tombstones. Returns the retired ids for one physical Host shutdown
    /// attempt each, outside registry locks.
    #[must_use]
    pub fn retire_character_sessions(&self, character_id: &str) -> Vec<HostSessionId> {
        let mut maps = self.maps();
        let keys: Vec<ActorSessionKey> = maps
            .by_key
            .keys()
            .filter(|k| k.actor_kind == ActorSessionKind::Character && k.actor_id == character_id)
            .cloned()
            .collect();
        let mut retired_ids = Vec::new();
        for key in keys {
            if let Some(id) = maps.by_key.remove(&key) {
                if let Some(row) = maps.by_session.remove(&id) {
                    maps.retired.insert(id.clone(), Self::tombstone_of(&row));
                    retired_ids.push(id.clone());
                    maps.indexed_operations.retain(|_, sid| sid != &id);
                }
            }
        }
        retired_ids
    }

    /// Close the process-lifetime maps (authority shutdown). In-flight creates
    /// cannot repopulate.
    pub fn close(&self) {
        let mut maps = self.maps();
        maps.closed = true;
        let ids: Vec<_> = maps.by_session.keys().cloned().collect();
        for id in ids {
            if let Some(row) = maps.by_session.remove(&id) {
                maps.retired.insert(id.clone(), Self::tombstone_of(&row));
            }
        }
        maps.by_key.clear();
        maps.by_session.clear();
        maps.key_locks.clear();
        maps.indexed_operations.clear();
        // The observation readers die with the authority's Actor side: a
        // closed registry exposes no stream to pull. The records themselves
        // stay (their terminal outcomes are read owner-scoped until the
        // authority is gone), so the reader slot is what is cleared.
        for record in maps.character_operations.values_mut() {
            record.observation = None;
        }
    }

    /// Spawn an authority-owned Actor drain.
    ///
    /// This is the ONE spawn path for core-owned Actor drains: the spawned
    /// task's handle is retained in the authority's registry instead of being
    /// dropped on the floor, so a close that cannot settle it keeps owning the
    /// live drain (and the admitted knowledge leases it holds) rather than
    /// detaching it and reporting a cleanup it cannot confirm.
    ///
    /// Registration is the *transfer* half of the authority's admission/close
    /// barrier: an operation retires its admission only after its drain is
    /// registered here, so a concurrent `close` observes either the admission
    /// still in flight or this retained handle. A registration that lands
    /// after the closing latch is retained all the same — which is why a
    /// drained operation can never be dropped on the floor and why the
    /// repeated close still refuses to confirm while one is unsettled.
    ///
    /// `#[doc(hidden)]` integration seam, the same convention as
    /// [`Self::insert_indexed_entry`].
    ///
    /// The drain is also accounted to the `session_id` it runs for — in the
    /// same live-work entry an admitted operation holds before it transfers a
    /// drain ([`Self::admit_session_operation`]) — so a session shutdown joins
    /// exactly the work it is retiring with [`Self::join_session_drains`]
    /// instead of the whole authority's drains.
    #[doc(hidden)]
    pub fn spawn_actor_drain<F>(&self, session_id: HostSessionId, drain: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.live_drains.fetch_add(1, Ordering::AcqRel);
        let live = Arc::clone(&self.live_drains);
        let settled = Arc::clone(&self.drain_settled);
        // The session entry and its increment are taken under the maps lock, so
        // a joiner observes either this drain or an entry that already counts
        // it — an entry is never removed while an increment is in flight.
        let session = {
            let mut maps = self.maps();
            let session = Arc::clone(maps.session_liveness.entry(session_id.clone()).or_default());
            session.live.fetch_add(1, Ordering::AcqRel);
            session
        };
        let session_settled = Arc::clone(&session);
        let maps_handle = Arc::clone(&self.maps);
        let handle = tokio::spawn(async move {
            drain.await;
            // Publication order matters: the count drops BEFORE the wakeup, so
            // a joiner that armed its notification and then read a non-zero
            // count still gets the permit this drain publishes.
            live.fetch_sub(1, Ordering::AcqRel);
            settled.notify_one();
            // The session's accounting entry dies with its last live unit —
            // which is this drain, or an admission that has not retired yet.
            retire_session_liveness(&maps_handle, &session_id, &session_settled);
        });
        let mut maps = self.maps();
        maps.drains.retain(|settled| !settled.is_finished());
        maps.drains.push(handle);
    }

    /// Await every retained Actor drain and return how many were unsettled
    /// when the join returned (`0` when the Actor side settled).
    ///
    /// Cancellation-safe: the drain handles stay in the registry, so a caller
    /// that runs out of its outer deadline (the native close owner bounds this
    /// join with its own deadline) loses nothing — the authority still owns
    /// the live drains and a retry joins them instead of reporting a cleanup
    /// it cannot confirm.
    pub async fn join_actor_drains(&self) -> usize {
        loop {
            // Arm the notification BEFORE reading the count: a drain that
            // settles in between leaves a permit, so the wait can never miss
            // its only wakeup.
            let settled = self.drain_settled.notified();
            let live = self.live_drains.load(Ordering::Acquire);
            if live == 0 {
                return 0;
            }
            settled.await;
        }
    }

    /// Admit one operation against its SESSION's live-work accounting.
    ///
    /// The count is taken under the maps lock before the operation's first
    /// await — so a joiner observes either this admission or an entry that
    /// already counts it — and stays live until the returned guard drops, which
    /// happens only after the operation's drain has transferred to the same
    /// accounting (or after it failed before creating one). That is the
    /// session-scoped half of the authority's admission/close barrier, and it
    /// is what makes [`Self::join_session_drains`] a proof instead of a
    /// snapshot: an operation registers its drain only after asynchronous
    /// admission and Host execution, so a join that trusted the registered
    /// drains ALONE could read "this session has no live work" in that window
    /// and let the session retire before the drain it is about to own.
    ///
    /// Crate-internal by construction: the accounting is the Host handle's to
    /// participate in, not a new public surface.
    pub(crate) fn admit_session_operation(&self, session_id: HostSessionId) -> SessionAdmission {
        let mut maps = self.maps();
        let liveness = Arc::clone(maps.session_liveness.entry(session_id.clone()).or_default());
        liveness.live.fetch_add(1, Ordering::AcqRel);
        drop(maps);
        SessionAdmission {
            maps: Arc::clone(&self.maps),
            session_id,
            liveness,
        }
    }

    /// Await every retained drain of ONE session — the join a session shutdown
    /// runs before it retires that session and before it reports success.
    ///
    /// Cancellation-safe like [`Self::join_actor_drains`]: the handles stay
    /// owned by the registry, so a caller that runs out of budget loses nothing
    /// and a retry joins the same drains. An entry that is gone is that
    /// session's last live unit settling, so the join returns.
    ///
    /// The entry it waits on carries the session's ADMITTED operations as well
    /// as its drains ([`Self::admit_session_operation`]), so this join cannot
    /// conclude in the window between "no drain registered yet" and the drain
    /// registration of an operation already admitted for this session.
    pub async fn join_session_drains(&self, session_id: &HostSessionId) {
        loop {
            let session = {
                let maps = self.maps();
                maps.session_liveness.get(session_id).cloned()
            };
            let Some(session) = session else {
                return;
            };
            // Arm the notification BEFORE reading the count, exactly like the
            // authority-wide join: a drain that settles in between leaves a
            // permit, so the wait can never miss its only wakeup.
            let settled = session.settled.notified();
            if session.live.load(Ordering::Acquire) == 0 {
                return;
            }
            settled.await;
        }
    }

    /// Drop settled drain handles, retain unsettled ones, and report how many
    /// remain live — the drain ownership an unsettled authority close keeps.
    #[must_use]
    pub fn prune_settled_drains(&self) -> usize {
        let mut maps = self.maps();
        maps.drains.retain(|drain| !drain.is_finished());
        maps.drains.len()
    }

    /// Count the retained Actor drains that have not settled yet.
    #[must_use]
    pub fn unsettled_drain_count(&self) -> usize {
        self.live_drains.load(Ordering::Acquire)
    }

    /// Shut down every retired Actor host session (authority drain).
    ///
    /// # Errors
    ///
    /// Returns the first host shutdown error after attempting remaining ids.
    pub async fn drain_host_sessions(&self, host: &dyn HostFacade) -> CoreResult<()> {
        let ids = {
            let maps = self.maps();
            maps.retired.keys().cloned().collect::<Vec<_>>()
        };
        let mut first_err = None;
        for id in ids {
            if let Err(err) = host.shutdown_session(id).await {
                let _ = first_err.get_or_insert_with(|| host_err(&err));
            }
        }
        first_err.map_or(Ok(()), Err)
    }

    /// Drop the process-lifetime maps (authority shutdown).
    pub fn clear(&self) {
        self.close();
    }

    /// Count indexed Actor sessions (tests / diagnostics).
    #[must_use]
    pub fn len(&self) -> usize {
        self.maps().by_key.len()
    }

    /// Count reserved and retained Character operation records (tests /
    /// diagnostics): running reservations plus the terminal records still
    /// inside the retention window.
    #[must_use]
    pub fn character_operation_count(&self) -> usize {
        self.maps().character_operations.len()
    }

    /// True when no Actor sessions are indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.maps().by_key.is_empty()
    }

    /// Count retained per-key locks (tests).
    #[must_use]
    pub fn lock_entry_count(&self) -> usize {
        self.maps().key_locks.len()
    }

    fn lock_for_key(&self, key: &ActorSessionKey) -> Arc<AsyncMutex<()>> {
        let mut maps = self.maps();
        if maps.closed {
            return maps
                .key_locks
                .get(key)
                .cloned()
                .unwrap_or_else(|| Arc::new(AsyncMutex::new(())));
        }
        Arc::clone(
            maps.key_locks
                .entry(key.clone())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    fn tombstone_of(row: &IndexedActorSession) -> RetiredActorSession {
        RetiredActorSession {
            owner_creator_id: row.ctx.owner_creator_id.clone(),
            actor_kind: row.key.actor_kind,
            actor_id: row.key.actor_id.clone(),
            world_id: row.key.world_id.clone(),
            binding_id: row.key.binding_id.clone(),
            branch_id: row.key.branch_id.clone(),
            event_id: row.key.event_id.clone(),
        }
    }

    fn evict_locked(maps: &mut RegistryMaps, key: &ActorSessionKey, session_id: &HostSessionId) {
        maps.by_key.remove(key);
        if let Some(row) = maps.by_session.remove(session_id) {
            maps.retired
                .insert(session_id.clone(), Self::tombstone_of(&row));
        }
    }

    fn reclaim(&self, key: &ActorSessionKey, held: &Arc<AsyncMutex<()>>) {
        let mut maps = self.maps();
        if !maps.by_key.contains_key(key) {
            if let Some(stored) = maps.key_locks.get(key) {
                if Arc::ptr_eq(stored, held) && Arc::strong_count(stored) == 2 {
                    maps.key_locks.remove(key);
                }
            }
        }
    }

    const fn reject_if_closed(maps: &RegistryMaps) -> CoreResult<()> {
        if maps.closed {
            Err(shutting_down())
        } else {
            Ok(())
        }
    }

    /// Shut down a host session under the Actor key lock when indexed.
    ///
    /// The manager release does not settle this authority's own drain of that
    /// session's exec stream, so the session's retained drains are JOINED here —
    /// after the release, before the session is retired and before this call
    /// reports success (technical contract §3, "no success before confirmed
    /// release"). The join waits on the session's live-work entry, which counts
    /// the operations ADMITTED for the session as well as its drains, so it
    /// cannot conclude in the window between an admitted operation and the
    /// drain registration that operation is still to make. An unsettled drain
    /// therefore keeps the session indexed and keeps the call in flight instead
    /// of reporting a shutdown it cannot confirm. The join is deliberately
    /// unbounded: contract §3 adds no timeout policy, so no fabricated deadline
    /// decides when a live run is retired.
    ///
    /// # Errors
    ///
    /// Host shutdown errors are surfaced as `internal` with the host reason.
    pub async fn shutdown_session(
        &self,
        session_id: HostSessionId,
        host: &dyn HostFacade,
    ) -> CoreResult<()> {
        let key = self
            .maps()
            .by_session
            .get(&session_id)
            .map(|row| row.key.clone());
        if let Some(key) = key {
            let lock = self.lock_for_key(&key);
            let guard = lock.lock().await;
            let still_indexed = self
                .maps()
                .by_session
                .get(&session_id)
                .is_some_and(|row| row.key == key);
            if let Err(err) = host.shutdown_session(session_id.clone()).await {
                drop(guard);
                self.reclaim(&key, &lock);
                return Err(host_err(&err));
            }
            // Settle the session's own drains before anything about it is
            // retired: the manager released the session, while the drain of the
            // session's original exec stream — and the knowledge leases it
            // holds — is still this authority's to join.
            self.join_session_drains(&session_id).await;
            if still_indexed {
                let mut maps = self.maps();
                Self::evict_locked(&mut maps, &key, &session_id);
                // The shutdown removes the session: its indexed-operation
                // fallback entries go with it (a later cancel must not resolve
                // a session that no longer exists), and so do the observation
                // readers of its operations — a released session exposes no
                // stream, while its recorded outcomes stay readable until the
                // terminal retention drops them.
                maps.indexed_operations.retain(|_, sid| sid != &session_id);
                for record in maps.character_operations.values_mut() {
                    if record.session_id == session_id {
                        record.observation = None;
                    }
                }
            }
            drop(guard);
            self.reclaim(&key, &lock);
            Ok(())
        } else {
            host.shutdown_session(session_id.clone())
                .await
                .map_err(|e| host_err(&e))?;
            self.join_session_drains(&session_id).await;
            Ok(())
        }
    }

    /// Reuse a `Ready` exact match, reject `Busy`, or mint a replacement after
    /// stale eviction.
    ///
    /// # Errors
    ///
    /// `actor_session_busy` when the Host session is `Busy`; host errors otherwise.
    pub async fn resolve_or_create<F, Fut>(
        &self,
        key: ActorSessionKey,
        ctx: AdmittedActorContext,
        host: &dyn HostFacade,
        create: F,
    ) -> CoreResult<HostSession>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = CoreResult<HostSession>> + Send,
    {
        {
            let maps = self.maps();
            Self::reject_if_closed(&maps)?;
        }
        let lock = self.lock_for_key(&key);
        let guard = lock.lock().await;

        let existing = {
            let maps = self.maps();
            maps.by_key.get(&key).cloned()
        };
        if let Some(existing) = existing {
            let listed = host.list_sessions().await.map_err(|e| host_err(&e))?;
            {
                let maps = self.maps();
                if let Err(err) = Self::reject_if_closed(&maps) {
                    drop(maps);
                    drop(guard);
                    self.reclaim(&key, &lock);
                    return Err(err);
                }
            }
            match listed.into_iter().find(|session| session.id == existing) {
                Some(session) if matches!(session.state, SessionState::Ready) => {
                    drop(guard);
                    self.reclaim(&key, &lock);
                    return Ok(session);
                }
                Some(session) if session.state.is_busy() => {
                    drop(guard);
                    self.reclaim(&key, &lock);
                    return Err(CoreError::ActorConflict {
                        code: "actor_session_busy".into(),
                        message: "actor session is busy".into(),
                    });
                }
                Some(session) => {
                    host.shutdown_session(session.id.clone())
                        .await
                        .map_err(|e| host_err(&e))?;
                    let mut maps = self.maps();
                    Self::evict_locked(&mut maps, &key, &session.id);
                }
                None => {
                    let mut maps = self.maps();
                    Self::evict_locked(&mut maps, &key, &existing);
                }
            }
        }

        let session = match create().await {
            Ok(session) => session,
            Err(err) => {
                drop(guard);
                self.reclaim(&key, &lock);
                return Err(err);
            }
        };
        let inserted = {
            let mut maps = self.maps();
            if maps.closed {
                false
            } else {
                maps.by_key.insert(key.clone(), session.id.clone());
                maps.by_session.insert(
                    session.id.clone(),
                    IndexedActorSession {
                        key: key.clone(),
                        ctx,
                    },
                );
                true
            }
        };
        if !inserted {
            let cleanup = Self::teardown_minted_host(host, session.id.clone()).await;
            drop(guard);
            self.reclaim(&key, &lock);
            return match cleanup {
                Ok(()) => Err(shutting_down()),
                Err(err) => Err(err),
            };
        }
        drop(guard);
        self.reclaim(&key, &lock);
        Ok(session)
    }

    /// Test/integration seam: index an already-known Host session id without a
    /// live Host create. The Host authority acceptance anchor uses this to
    /// prove stale-epoch denial happens before any provider effect; production
    /// indexing only happens inside [`Self::resolve_or_create`].
    #[doc(hidden)]
    pub fn insert_indexed_entry(
        &self,
        key: ActorSessionKey,
        ctx: AdmittedActorContext,
        session_id: HostSessionId,
    ) {
        let mut maps = self.maps();
        if maps.closed {
            return;
        }
        maps.by_key.insert(key.clone(), session_id.clone());
        maps.by_session
            .insert(session_id, IndexedActorSession { key, ctx });
    }

    async fn teardown_minted_host(
        host: &dyn HostFacade,
        session_id: HostSessionId,
    ) -> CoreResult<()> {
        const ATTEMPTS: u32 = 3;
        let mut last = None;
        for _ in 0..ATTEMPTS {
            match host.shutdown_session(session_id.clone()).await {
                Ok(()) => return Ok(()),
                Err(err) => last = Some(err),
            }
        }
        Err(host_err(&last.expect("minted host cleanup attempted")))
    }
}

fn echo_conversion(code: &str, e: impl std::fmt::Display) -> CoreError {
    CoreError::Internal {
        category: format!("{code}: {e}"),
    }
}

/// Re-parse one echoed wire id through a core-query DTO checked constructor.
///
/// # Errors
///
/// Returns `internal` when the id fails the query family's pattern check.
fn query_id<T>(id: &str) -> CoreResult<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    id.parse().map_err(|e| echo_conversion("QUERY_ECHO", e))
}

/// Re-create the core-query `ActorRef` sum from the authority's echo.
///
/// # Errors
///
/// Returns `internal` when an id fails the query family's pattern check.
fn query_actor_ref(actor_ref: NexusActorRef) -> CoreResult<QueryActorRef> {
    match actor_ref {
        NexusActorRef::CreatorActorRef { creator_id, .. } => Ok(QueryActorRef::CreatorActorRef {
            actor_kind: query_id("creator")?,
            creator_id: query_id(creator_id.as_str())?,
        }),
        NexusActorRef::CharacterActorRef { character_id, .. } => {
            Ok(QueryActorRef::CharacterActorRef {
                actor_kind: query_id("character")?,
                character_id: query_id(character_id.as_str())?,
            })
        }
    }
}

/// Re-create the core-query `Viewpoint` from the authority's echo.
///
/// # Errors
///
/// Returns `internal` when an id fails the query family's pattern check.
fn query_viewpoint(viewpoint: &NexusSessionViewpoint) -> CoreResult<QueryViewpoint> {
    let binding_id = viewpoint
        .binding_id
        .as_ref()
        .map(|id| query_id(id.as_str()))
        .transpose()?;
    let branch_id = viewpoint
        .branch_id
        .as_ref()
        .map(|id| query_id(id.as_str()))
        .transpose()?;
    let event_id = viewpoint
        .event_id
        .as_ref()
        .map(|id| query_id(id.as_str()))
        .transpose()?;
    Ok(QueryViewpoint {
        world_id: query_id(viewpoint.world_id.as_str())?,
        binding_id,
        branch_id,
        event_id,
    })
}

/// Map a retired-session tombstone onto generated session response optionals
/// so leftover Host rows stay recognizably Actor-mode.
///
/// # Errors
///
/// Returns `internal` if stored ids fail generated pattern checks (should not happen).
fn echo_retired_pair(
    tombstone: &RetiredActorSession,
) -> CoreResult<(Option<NexusActorRef>, Option<NexusSessionViewpoint>)> {
    use nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError;
    let actor_ref = match tombstone.actor_kind {
        ActorSessionKind::Creator => NexusActorRef::CreatorActorRef {
            actor_kind: "creator"
                .parse()
                .map_err(|e: ConversionError| echo_conversion("ACTOR_REF_ECHO", e))?,
            creator_id: tombstone
                .actor_id
                .parse()
                .map_err(|e: ConversionError| echo_conversion("ACTOR_REF_ECHO", e))?,
        },
        ActorSessionKind::Character => NexusActorRef::CharacterActorRef {
            actor_kind: "character"
                .parse()
                .map_err(|e: ConversionError| echo_conversion("ACTOR_REF_ECHO", e))?,
            character_id: tombstone
                .actor_id
                .parse()
                .map_err(|e: ConversionError| echo_conversion("ACTOR_REF_ECHO", e))?,
        },
    };
    Ok((
        Some(actor_ref),
        Some(viewpoint_from_parts(
            &tombstone.world_id,
            tombstone.binding_id.as_deref(),
            tombstone.branch_id.as_deref(),
            tombstone.event_id.as_deref(),
        )?),
    ))
}

/// # Errors
///
/// Returns `internal` if stored ids fail generated pattern checks.
pub fn echo_actor_pair(
    ctx: &AdmittedActorContext,
) -> CoreResult<(Option<NexusActorRef>, Option<NexusSessionViewpoint>)> {
    use nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError;
    let actor_ref = match &ctx.actor {
        AdmittedActor::Creator { creator_id } => NexusActorRef::CreatorActorRef {
            actor_kind: "creator"
                .parse()
                .map_err(|e: ConversionError| echo_conversion("ACTOR_REF_ECHO", e))?,
            creator_id: creator_id
                .parse()
                .map_err(|e: ConversionError| echo_conversion("ACTOR_REF_ECHO", e))?,
        },
        AdmittedActor::Character { character_id } => NexusActorRef::CharacterActorRef {
            actor_kind: "character"
                .parse()
                .map_err(|e: ConversionError| echo_conversion("ACTOR_REF_ECHO", e))?,
            character_id: character_id
                .parse()
                .map_err(|e: ConversionError| echo_conversion("ACTOR_REF_ECHO", e))?,
        },
    };
    Ok((
        Some(actor_ref),
        Some(viewpoint_from_parts(
            &ctx.world_id,
            ctx.binding_id.as_deref(),
            ctx.branch_id.as_deref(),
            ctx.event_id.as_deref(),
        )?),
    ))
}

fn viewpoint_from_parts(
    world_id: &str,
    binding_id: Option<&str>,
    branch_id: Option<&str>,
    event_id: Option<&str>,
) -> CoreResult<NexusSessionViewpoint> {
    use nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError;
    let parse = |v: Option<&str>| -> CoreResult<Option<NexusSessionViewpointBindingId>> {
        v.map(|id| {
            id.parse()
                .map_err(|e: ConversionError| echo_conversion("VIEWPOINT_ECHO", e))
        })
        .transpose()
    };
    let parse_branch = |v: Option<&str>| -> CoreResult<Option<NexusSessionViewpointBranchId>> {
        v.map(|id| {
            id.parse()
                .map_err(|e: ConversionError| echo_conversion("VIEWPOINT_ECHO", e))
        })
        .transpose()
    };
    let parse_event = |v: Option<&str>| -> CoreResult<Option<NexusSessionViewpointEventId>> {
        v.map(|id| {
            id.parse()
                .map_err(|e: ConversionError| echo_conversion("VIEWPOINT_ECHO", e))
        })
        .transpose()
    };
    Ok(NexusSessionViewpoint {
        world_id: world_id
            .parse()
            .map_err(|e: ConversionError| echo_conversion("VIEWPOINT_ECHO", e))?,
        binding_id: parse(binding_id)?,
        branch_id: parse_branch(branch_id)?,
        event_id: parse_event(event_id)?,
    })
}
