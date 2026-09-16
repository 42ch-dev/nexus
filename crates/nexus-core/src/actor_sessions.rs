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
use std::sync::{Arc, Mutex};

use nexus_agent_host::{HostFacade, HostOperationId, HostSession, HostSessionId, SessionState};
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
use crate::service::CoreService;

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
#[derive(Debug, Clone)]
pub struct CharacterOperationSnapshot {
    pub owner_creator_id: String,
    pub ctx: AdmittedActorContext,
    pub session_id: HostSessionId,
    pub operation_id: HostOperationId,
    pub remember: bool,
    pub raw_prompt: String,
}

struct CharacterOperationRecord {
    owner_creator_id: String,
    session_id: HostSessionId,
    phase: OperationPhase,
    outcome: CharacterOperationResult,
    _seq: u64,
}

const MAX_NONTERMINAL_OPERATIONS: usize = 128;
const MAX_TERMINAL_OPERATIONS: usize = 1024;

fn running_outcome(snapshot: &CharacterOperationSnapshot) -> CharacterOperationResult {
    let capture = if snapshot.remember {
        NexusCharacterRunCaptureOutcome {
            status: NexusCharacterRunCaptureOutcomeStatus::Pending,
            pending_id: None,
            code: None,
        }
    } else {
        NexusCharacterRunCaptureOutcome {
            status: NexusCharacterRunCaptureOutcomeStatus::Disabled,
            pending_id: None,
            code: None,
        }
    };
    CharacterOperationResult {
        operation_id: snapshot.operation_id.to_string(),
        session_id: snapshot.session_id.to_string(),
        run_status: CharacterOperationResultRunStatus::Running,
        finish_reason: None,
        capture,
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
    closed: bool,
}

/// Process-lifetime maps: `key -> HostSessionId` and `HostSessionId -> context`.
#[derive(Clone)]
pub struct ActorSessionRegistry {
    maps: Arc<Mutex<RegistryMaps>>,
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
                closed: false,
            })),
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

    /// Build the exact tuple key from an admitted Actor context.
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
        })
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

    /// Remove an unstarted reservation after exec admission failure.
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
    pub fn commit_operation_terminal(
        &self,
        operation_id: &HostOperationId,
        outcome: CharacterOperationResult,
    ) {
        let mut maps = self.maps();
        if let Some(record) = maps.character_operations.get_mut(operation_id) {
            record.phase = OperationPhase::Terminal;
            record.outcome = outcome;
            maps.terminal_fifo.push_back(operation_id.clone());
            while maps.terminal_fifo.len() > MAX_TERMINAL_OPERATIONS {
                if let Some(evicted) = maps.terminal_fifo.pop_front() {
                    maps.character_operations.remove(&evicted);
                }
            }
        }
        drop(maps);
    }

    /// Commit a drained Host operation's terminal run status against the
    /// reserved record (authority-owned drain). The capture half of the
    /// outcome stays as reserved (`pending`/`disabled`) until a durable
    /// capture writer settles it.
    pub fn settle_operation_terminal(
        &self,
        operation_id: &HostOperationId,
        run_status: CharacterOperationResultRunStatus,
        finish_reason: Option<CharacterOperationResultFinishReason>,
    ) {
        let mut maps = self.maps();
        if let Some(record) = maps.character_operations.get_mut(operation_id) {
            record.phase = OperationPhase::Terminal;
            record.outcome.run_status = run_status;
            record.outcome.finish_reason = finish_reason;
            maps.terminal_fifo.push_back(operation_id.clone());
            while maps.terminal_fifo.len() > MAX_TERMINAL_OPERATIONS {
                if let Some(evicted) = maps.terminal_fifo.pop_front() {
                    maps.character_operations.remove(&evicted);
                }
            }
        }
        drop(maps);
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

    /// Overlay `actor_ref/viewpoint` for list/get: live indexed context first,
    /// then retired tombstones so leftover Host rows stay Actor-shaped.
    ///
    /// # Errors
    ///
    /// Returns `internal` when stored ids fail generated pattern checks.
    pub fn echo_actor_pair_for_session(
        &self,
        session_id: &HostSessionId,
    ) -> CoreResult<(Option<NexusActorRef>, Option<NexusSessionViewpoint>)> {
        if let Some(ctx) = self.context_for(session_id) {
            return echo_actor_pair(&ctx);
        }
        if let Some(tombstone) = self.retired_tombstone(session_id) {
            return echo_retired_pair(&tombstone);
        }
        Ok((None, None))
    }

    /// True once the process-lifetime maps are closed (authority shutdown).
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.maps().closed
    }

    /// Admit one side-effecting Character activity through the P2
    /// [`CoreService`] lease: owner-check before allocating a fence, then the
    /// status/epoch re-read under the fence. The caller holds the returned
    /// lease through every DB/file/provider/terminal-capture effect; drop
    /// releases the fence.
    ///
    /// # Errors
    ///
    /// Owner, inactive-Character, or busy/transition lease failures.
    pub async fn admit_character_activity(
        &self,
        core: &CoreService,
        principal: &crate::principal::Principal,
        actor: &AdmittedActor,
    ) -> CoreResult<crate::actor_fence::ActorActivityLease> {
        core.acquire_actor_activity(principal, actor).await
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
            if still_indexed {
                let mut maps = self.maps();
                Self::evict_locked(&mut maps, &key, &session_id);
            }
            drop(guard);
            self.reclaim(&key, &lock);
            Ok(())
        } else {
            host.shutdown_session(session_id)
                .await
                .map_err(|e| host_err(&e))
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
