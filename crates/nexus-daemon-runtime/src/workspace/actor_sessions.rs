//! Process-lifetime Actor session registry over `HostFacade`.
//!
//! Indexes only Actor-mode sessions. Legacy creates never enter these maps.
//! Concurrent creates for one exact key serialize on a per-key lock.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nexus_agent_host::{HostFacade, HostOperationId, HostSession, HostSessionId, SessionState};
use nexus_contracts::generated::daemon_api::agent_host::session_response::{
    NexusActorRef, NexusSessionViewpoint,
};
use sqlx::SqlitePool;
use tokio::sync::{Mutex as AsyncMutex, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

use nexus_contracts::generated::daemon_api::agent_host::character_operation_result::{
    CharacterOperationResult, CharacterOperationResultRunStatus, NexusCharacterRunCaptureOutcome,
    NexusCharacterRunCaptureOutcomeStatus,
};

use crate::actor_admission::AdmittedActorContext;
use crate::actor_knowledge_view::AdmittedActor;
use crate::api::errors::NexusApiError;

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
    /// Stored Character lifecycle epoch at admission (`None` for Creator).
    /// A material archive/restore bumps the epoch, so a post-transition admit
    /// can never reuse a pre-transition session (v1.185 P0 Task 2).
    pub character_epoch: Option<i64>,
}

struct IndexedActorSession {
    key: ActorSessionKey,
    ctx: AdmittedActorContext,
}

/// Retired-session tombstone: retains owner + Actor identity so retired ids
/// stay recognizably Actor-mode (never legacy) and authorize owner-only
/// safety/observation operations (v1.185 P0 Task 2).
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

/// Internal phase for per-operation cancel/finalize ordering (§11.6).
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
    /// `active_op_id` (fail-closed cancel authorization, durable §11.3.4).
    indexed_operations: HashMap<HostOperationId, HostSessionId>,
    /// Reclaimable per-Character activity fences (durable §11.3). An entry
    /// with no live guard (`Arc::strong_count == 1`) is swept on admission.
    character_fences: HashMap<String, Arc<RwLock<()>>>,
    character_operations: HashMap<HostOperationId, CharacterOperationRecord>,
    terminal_fifo: VecDeque<HostOperationId>,
    operation_seq: u64,
    closed: bool,
}
/// Test-only hook invoked after a session lock is reclaimed.
#[cfg(test)]
type AfterReclaimHook = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;

/// Process-lifetime maps: `key -> HostSessionId` and `HostSessionId -> context`.
#[derive(Clone)]
pub struct ActorSessionRegistry {
    maps: Arc<Mutex<RegistryMaps>>,
    #[cfg(test)]
    after_reclaim: AfterReclaimHook,
}

impl Default for ActorSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn shutting_down() -> NexusApiError {
    NexusApiError::ServiceUnavailable {
        message: "daemon is shutting down".into(),
    }
}

/// Owned per-Character activity fence (durable §11.3).
///
/// Private, owned, non-Copy: obtain only via [`ActorSessionRegistry::admit_character_activity`], which owner-checks and re-reads status/epoch under the fence. Hold through every DB/file/provider/terminal-capture effect; drop releases the fence (and for a Prompt, only after terminal finalization in the server-owned drain).
pub struct CharacterActivityGuard {
    _read: OwnedRwLockReadGuard<()>,
    owner_creator_id: String,
    character_id: String,
    epoch: i64,
}

impl CharacterActivityGuard {
    /// Trusted owner (active Creator) admitted with this activity.
    #[must_use]
    pub fn owner_creator_id(&self) -> &str {
        &self.owner_creator_id
    }

    /// Character this activity fence covers.
    #[must_use]
    pub fn character_id(&self) -> &str {
        &self.character_id
    }

    /// Stored lifecycle epoch re-read under the fence at admission.
    #[must_use]
    pub const fn epoch(&self) -> i64 {
        self.epoch
    }
}

impl std::fmt::Debug for CharacterActivityGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CharacterActivityGuard")
            .field("owner_creator_id", &self.owner_creator_id)
            .field("character_id", &self.character_id)
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

/// Exclusive per-Character lifecycle fence for archive/restore (durable §11.3.2).
///
/// Obtained only via [`ActorSessionRegistry::try_character_transition`] (busy refusal). Stores the lifecycle epoch re-read under the exclusive fence: retire session reuse keys only when the committed transition returns a different epoch (a same-state CAS no-op retires nothing).
pub struct CharacterTransitionGuard {
    _write: OwnedRwLockWriteGuard<()>,
    owner_creator_id: String,
    character_id: String,
    epoch: i64,
}

impl CharacterTransitionGuard {
    /// Trusted owner (active Creator) admitted for the transition.
    #[must_use]
    pub fn owner_creator_id(&self) -> &str {
        &self.owner_creator_id
    }

    /// Character this transition fence covers.
    #[must_use]
    pub fn character_id(&self) -> &str {
        &self.character_id
    }

    /// Stored lifecycle epoch re-read under the exclusive fence at transition start.
    #[must_use]
    pub const fn epoch(&self) -> i64 {
        self.epoch
    }
}

impl std::fmt::Debug for CharacterTransitionGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CharacterTransitionGuard")
            .field("owner_creator_id", &self.owner_creator_id)
            .field("character_id", &self.character_id)
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
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
                character_fences: HashMap::new(),
                character_operations: HashMap::new(),
                terminal_fifo: VecDeque::new(),
                operation_seq: 0,
                closed: false,
            })),
            #[cfg(test)]
            after_reclaim: Arc::new(Mutex::new(None)),
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
    /// Returns a policy-mapped API error when the path is relative, traverses,
    /// or cannot be resolved.
    pub fn canonicalize_cwd(cwd: &Path) -> Result<PathBuf, NexusApiError> {
        nexus_agent_host::config::validate_workspace_path(cwd).map_err(|e| map_policy(&e))
    }

    /// Build the exact tuple key from an admitted Actor context.
    ///
    /// # Errors
    ///
    /// Returns a policy-mapped API error when `cwd` cannot be canonicalized.
    pub fn key_for(
        provider_id: &str,
        cwd: &Path,
        model: Option<String>,
        mode: Option<String>,
        ctx: &AdmittedActorContext,
    ) -> Result<ActorSessionKey, NexusApiError> {
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
    /// a 404 before any Host access.
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

    /// Tombstone retained after a material transition for overlay/authorization.
    #[must_use]
    fn retired_tombstone(&self, session_id: &HostSessionId) -> Option<RetiredActorSession> {
        self.maps().retired.get(session_id).cloned()
    }

    /// Record an indexed Actor operation for fail-closed cancel authorization
    /// when the Host has not yet surfaced `active_op_id`.
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

    /// Reserve a Character operation outcome before Host exec (§11.6).
    ///
    /// # Errors
    ///
    /// Returns capacity or shutdown conflicts.
    #[allow(clippy::needless_pass_by_value)] // snapshot is stored by value in the operation map
    pub fn reserve_character_operation(
        &self,
        snapshot: CharacterOperationSnapshot,
    ) -> Result<(), NexusApiError> {
        let mut maps = self.maps();
        Self::reject_if_closed(&maps)?;
        let nonterminal = maps
            .character_operations
            .values()
            .filter(|r| !matches!(r.phase, OperationPhase::Terminal))
            .count();
        if nonterminal >= MAX_NONTERMINAL_OPERATIONS {
            return Err(crate::api::errors::actor_operation_capacity());
        }
        maps.operation_seq += 1;
        let seq = maps.operation_seq;
        maps.character_operations.insert(
            snapshot.operation_id.clone(),
            CharacterOperationRecord {
                owner_creator_id: snapshot.owner_creator_id.clone(),
                session_id: snapshot.session_id.clone(),
                phase: OperationPhase::Running,
                outcome: running_outcome(&snapshot),
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
    ) -> Result<CharacterOperationResult, NexusApiError> {
        let maps = self.maps();
        let record = maps
            .character_operations
            .get(operation_id)
            .ok_or_else(|| NexusApiError::NotFound(format!("operation {operation_id}")))?;
        if record.owner_creator_id != owner_creator_id {
            drop(maps);
            return Err(NexusApiError::NotFound(format!("operation {operation_id}")));
        }
        let outcome = record.outcome.clone();
        drop(maps);
        Ok(outcome)
    }

    /// Remove an unstarted reservation after exec admission failure.
    pub fn remove_operation_reservation(&self, operation_id: &HostOperationId) {
        self.maps().character_operations.remove(operation_id);
    }

    /// Latch cancel intent before awaiting Host.cancel.
    ///
    /// # Errors
    ///
    /// Returns `NotFound` or `actor_operation_finished` when cancel is invalid.
    pub fn request_operation_cancel(
        &self,
        owner_creator_id: &str,
        operation_id: &HostOperationId,
    ) -> Result<(), NexusApiError> {
        let mut maps = self.maps();
        let record = maps
            .character_operations
            .get_mut(operation_id)
            .ok_or_else(|| NexusApiError::NotFound(format!("operation {operation_id}")))?;
        if record.owner_creator_id != owner_creator_id {
            return Err(NexusApiError::NotFound(format!("operation {operation_id}")));
        }
        let result = match record.phase {
            OperationPhase::Running => {
                record.phase = OperationPhase::CancelRequested;
                Ok(())
            }
            OperationPhase::CancelRequested => Ok(()),
            OperationPhase::Finalizing | OperationPhase::Terminal => {
                Err(crate::api::errors::actor_operation_finished())
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
    ) -> Result<(Option<NexusActorRef>, Option<NexusSessionViewpoint>), NexusApiError> {
        if let Some(ctx) = self.context_for(session_id) {
            return echo_actor_pair(&ctx);
        }
        if let Some(tombstone) = self.retired_tombstone(session_id) {
            return echo_retired_pair(&tombstone);
        }
        Ok((None, None))
    }

    /// True once the process-lifetime maps are closed (daemon shutdown).
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.maps().closed
    }

    /// Sweep fence entries with no live guard (live-Arc reclaim, §11.3.6).
    fn sweep_fences(maps: &mut RegistryMaps) {
        maps.character_fences
            .retain(|_, fence| Arc::strong_count(fence) > 1);
    }

    /// Fetch or create the per-Character fence. Callers owner-check first.
    fn fence_for(&self, character_id: &str) -> Result<Arc<RwLock<()>>, NexusApiError> {
        let mut maps = self.maps();
        Self::reject_if_closed(&maps)?;
        Self::sweep_fences(&mut maps);
        Ok(Arc::clone(
            maps.character_fences
                .entry(character_id.to_string())
                .or_insert_with(|| Arc::new(RwLock::new(()))),
        ))
    }

    /// Admit one side-effecting Character activity (mutation, memory pipeline,
    /// session create/Prompt): owner-check before allocating a fence, hold an
    /// owned read guard, then re-read status/epoch under the fence (durable
    /// §11.3.1). The caller holds the guard through every DB/file/provider/
    /// terminal-capture effect.
    ///
    /// # Errors
    ///
    /// 404 for a missing/foreign Character; 409 `character_inactive` for an
    /// owned archived Character; 503 once the registry is closed.
    pub async fn admit_character_activity(
        &self,
        pool: &SqlitePool,
        owner_creator_id: &str,
        character_id: &str,
    ) -> Result<CharacterActivityGuard, NexusApiError> {
        // Owner-check before allocating a fence (no fence state leaks existence).
        nexus_local_db::get_character(pool, owner_creator_id, character_id)
            .await?
            .ok_or_else(|| NexusApiError::NotFound(format!("character {character_id}")))?;
        let fence = self.fence_for(character_id)?;
        let read = fence.read_owned().await;
        // Re-read under the fence: a transition holds the exclusive write
        // guard, so this status/epoch pair is exact for the guard's lifetime.
        let stored = nexus_local_db::get_character(pool, owner_creator_id, character_id)
            .await?
            .ok_or_else(|| NexusApiError::NotFound(format!("character {character_id}")))?;
        if stored.status != "active" {
            return Err(NexusApiError::ConflictCoded {
                code: "character_inactive".into(),
                message: format!("character {character_id} is {}", stored.status),
            });
        }
        Ok(CharacterActivityGuard {
            _read: read,
            owner_creator_id: owner_creator_id.to_string(),
            character_id: character_id.to_string(),
            epoch: stored.lifecycle_epoch,
        })
    }

    /// Try to take the exclusive lifecycle fence for archive/restore
    /// (durable §11.3.2): busy refusal, never forced cancellation or waiting
    /// on a provider. Allows an already-archived Character (restore).
    ///
    /// # Errors
    ///
    /// 404 for a missing/foreign Character; 409 `character_busy` when any
    /// Character activity is outstanding; 503 once the registry is closed.
    pub async fn try_character_transition(
        &self,
        pool: &SqlitePool,
        owner_creator_id: &str,
        character_id: &str,
    ) -> Result<CharacterTransitionGuard, NexusApiError> {
        // Owner-check before allocating a fence (no fence state leaks existence).
        nexus_local_db::get_character(pool, owner_creator_id, character_id)
            .await?
            .ok_or_else(|| NexusApiError::NotFound(format!("character {character_id}")))?;
        let fence = self.fence_for(character_id)?;
        let write = fence
            .try_write_owned()
            .map_err(|_| crate::api::errors::character_busy(character_id))?;
        // Re-read under the exclusive fence so pre-transition epoch is exact for
        // material-vs-no-op session retirement (durable §11.3.2–§11.3.3).
        let stored = nexus_local_db::get_character(pool, owner_creator_id, character_id)
            .await?
            .ok_or_else(|| NexusApiError::NotFound(format!("character {character_id}")))?;
        Ok(CharacterTransitionGuard {
            _write: write,
            owner_creator_id: owner_creator_id.to_string(),
            character_id: character_id.to_string(),
            epoch: stored.lifecycle_epoch,
        })
    }

    /// Retire every indexed session of a Character after a **material**
    /// committed transition, while the exclusive fence is still held
    /// (durable §11.3.3): remove old-epoch reuse keys and move their ids to
    /// owner-retaining tombstones. Returns the retired ids for one physical
    /// Host shutdown attempt each, outside DB/registry locks.
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

    /// Close the process-lifetime maps (daemon shutdown). In-flight creates cannot repopulate.
    ///
    /// `key_locks` is cleared unconditionally. Closed admission never reinserts a
    /// replacement map entry; in-flight local `Arc`s keep serializing waiters.
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
        maps.character_fences.clear();
        maps.indexed_operations.clear();
    }

    /// Shut down every retired Actor host session (daemon drain).
    ///
    /// # Errors
    ///
    /// Returns the first host shutdown error after attempting remaining ids.
    pub async fn drain_host_sessions(&self, host: &dyn HostFacade) -> Result<(), NexusApiError> {
        let ids = {
            let maps = self.maps();
            maps.retired.keys().cloned().collect::<Vec<_>>()
        };
        let mut first_err = None;
        for id in ids {
            if let Err(err) = host.shutdown_session(id).await {
                let _ = first_err.get_or_insert_with(|| map_host(&err));
            }
        }
        first_err.map_or_else(|| Ok(()), Err)
    }

    /// Drop the process-lifetime maps (daemon shutdown).
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
        {
            let mut maps = self.maps();
            if !maps.by_key.contains_key(key) {
                if let Some(stored) = maps.key_locks.get(key) {
                    if Arc::ptr_eq(stored, held) && Arc::strong_count(stored) == 2 {
                        maps.key_locks.remove(key);
                    }
                }
            }
        }
        #[cfg(test)]
        self.fire_after_reclaim();
    }

    #[cfg(test)]
    fn set_after_reclaim(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.after_reclaim.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("actor_sessions after_reclaim mutex poisoned, recovering");
            poisoned.into_inner()
        }) = Some(Arc::new(hook));
    }

    #[cfg(test)]
    fn fire_after_reclaim(&self) {
        let hook = self
            .after_reclaim
            .lock()
            .unwrap_or_else(|poisoned| {
                tracing::warn!("actor_sessions after_reclaim mutex poisoned, recovering");
                poisoned.into_inner()
            })
            .take();
        if let Some(hook) = hook {
            hook();
        }
    }

    fn reject_if_closed(maps: &RegistryMaps) -> Result<(), NexusApiError> {
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
    /// Host shutdown errors are mapped to API errors.
    pub async fn shutdown_session(
        &self,
        session_id: HostSessionId,
        host: &dyn HostFacade,
    ) -> Result<(), NexusApiError> {
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
                return Err(map_host(&err));
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
                .map_err(|e| map_host(&e))
        }
    }

    /// Reuse a `Ready` exact match, reject `Busy`, or mint a replacement after stale eviction.
    ///
    /// # Errors
    ///
    /// `actor_session_busy` when the `HostFacade` session is `Busy`; host list/create errors otherwise.
    #[allow(clippy::too_many_lines)] // single reconcile loop with early-return branches
    pub async fn resolve_or_create<F, Fut>(
        &self,
        key: ActorSessionKey,
        ctx: AdmittedActorContext,
        host: &dyn HostFacade,
        create: F,
    ) -> Result<HostSession, NexusApiError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<HostSession, NexusApiError>> + Send,
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
            let listed = match host.list_sessions().await {
                Ok(listed) => listed,
                Err(err) => {
                    drop(guard);
                    self.reclaim(&key, &lock);
                    return Err(map_host(&err));
                }
            };
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
                    return Err(NexusApiError::ConflictCoded {
                        code: "actor_session_busy".into(),
                        message: "actor session is busy".into(),
                    });
                }
                Some(session) => {
                    if let Err(err) = host.shutdown_session(session.id.clone()).await {
                        drop(guard);
                        self.reclaim(&key, &lock);
                        return Err(map_host(&err));
                    }
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

    async fn teardown_minted_host(
        host: &dyn HostFacade,
        session_id: HostSessionId,
    ) -> Result<(), NexusApiError> {
        const ATTEMPTS: u32 = 3;
        let mut last = None;
        for _ in 0..ATTEMPTS {
            match host.shutdown_session(session_id.clone()).await {
                Ok(()) => return Ok(()),
                Err(err) => last = Some(err),
            }
        }
        Err(map_host(&last.expect("minted host cleanup attempted")))
    }
}

/// Map a retired-session tombstone onto generated session response optionals
/// so leftover Host rows stay recognizably Actor-mode (durable §11.3.3).
///
/// # Errors
///
/// Returns `internal` if stored ids fail generated pattern checks (should not happen).
fn echo_retired_pair(
    tombstone: &RetiredActorSession,
) -> Result<(Option<NexusActorRef>, Option<NexusSessionViewpoint>), NexusApiError> {
    let actor_ref = match tombstone.actor_kind {
        ActorSessionKind::Creator => NexusActorRef::CreatorActorRef {
            actor_kind: "creator"
                .parse()
                .map_err(|e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                })?,
            creator_id: tombstone.actor_id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?,
        },
        ActorSessionKind::Character => NexusActorRef::CharacterActorRef {
            actor_kind: "character"
                .parse()
                .map_err(|e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                })?,
            character_id: tombstone.actor_id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?,
        },
    };
    let viewpoint = NexusSessionViewpoint {
        world_id: tombstone.world_id.parse().map_err(
            |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                NexusApiError::Internal {
                    code: "VIEWPOINT_ECHO".into(),
                    message: e.to_string(),
                }
            },
        )?,
        binding_id: match &tombstone.binding_id {
            Some(id) => Some(id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "VIEWPOINT_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?),
            None => None,
        },
        branch_id: match &tombstone.branch_id {
            Some(id) => Some(id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "VIEWPOINT_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?),
            None => None,
        },
        event_id: match &tombstone.event_id {
            Some(id) => Some(id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "VIEWPOINT_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?),
            None => None,
        },
    };
    Ok((Some(actor_ref), Some(viewpoint)))
}

/// # Errors
///
/// Returns `internal` if stored ids fail generated pattern checks.
pub fn echo_actor_pair(
    ctx: &AdmittedActorContext,
) -> Result<(Option<NexusActorRef>, Option<NexusSessionViewpoint>), NexusApiError> {
    let actor_ref = match &ctx.actor {
        AdmittedActor::Creator { creator_id } => NexusActorRef::CreatorActorRef {
            actor_kind: "creator"
                .parse()
                .map_err(|e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                })?,
            creator_id: creator_id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?,
        },
        AdmittedActor::Character { character_id } => NexusActorRef::CharacterActorRef {
            actor_kind: "character"
                .parse()
                .map_err(|e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                })?,
            character_id: character_id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "ACTOR_REF_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?,
        },
    };
    let viewpoint = NexusSessionViewpoint {
        world_id: ctx.world_id.parse().map_err(
            |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                NexusApiError::Internal {
                    code: "VIEWPOINT_ECHO".into(),
                    message: e.to_string(),
                }
            },
        )?,
        binding_id: match &ctx.binding_id {
            Some(id) => Some(id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "VIEWPOINT_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?),
            None => None,
        },
        branch_id: match &ctx.branch_id {
            Some(id) => Some(id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "VIEWPOINT_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?),
            None => None,
        },
        event_id: match &ctx.event_id {
            Some(id) => Some(id.parse().map_err(
                |e: nexus_contracts::generated::daemon_api::agent_host::session_response::error::ConversionError| {
                    NexusApiError::Internal {
                        code: "VIEWPOINT_ECHO".into(),
                        message: e.to_string(),
                    }
                },
            )?),
            None => None,
        },
    };
    Ok((Some(actor_ref), Some(viewpoint)))
}

fn map_policy(err: &nexus_agent_host::HostError) -> NexusApiError {
    NexusApiError::Forbidden {
        resource: "agent_host".into(),
        reason: err.to_string(),
    }
}

fn map_host(err: &nexus_agent_host::HostError) -> NexusApiError {
    match err.category() {
        "provider_unavailable" => NexusApiError::NotFound(err.to_string()),
        "capability_unsupported" => NexusApiError::InvalidInput {
            field: "operation".into(),
            reason: err.to_string(),
        },
        "policy_denied" => NexusApiError::Forbidden {
            resource: "agent_host".into(),
            reason: err.to_string(),
        },
        _ => NexusApiError::Internal {
            code: "AGENT_HOST_ERROR".into(),
            message: err.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor_knowledge_view::ActorKnowledgePage;
    use async_trait::async_trait;
    use nexus_agent_host::capability::model::{
        CapabilityDescriptor, CreateSessionRequest, HostEvent, HostEventStream, HostHealth,
        HostOperation, HostStartConfig,
    };
    use nexus_agent_host::{HostOperationId, ProviderCatalog};
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::sync::broadcast;

    fn empty_view() -> ActorKnowledgePage {
        ActorKnowledgePage {
            items: Vec::new(),
            limit: 50,
            has_more: false,
            next_cursor: None,
        }
    }

    fn sample_ctx(
        actor: AdmittedActor,
        world: &str,
        binding: Option<&str>,
    ) -> AdmittedActorContext {
        let owner_creator_id = match &actor {
            AdmittedActor::Creator { creator_id } => creator_id.clone(),
            AdmittedActor::Character { .. } => "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        };
        let character_epoch = match &actor {
            AdmittedActor::Creator { .. } => None,
            AdmittedActor::Character { .. } => Some(0),
        };
        AdmittedActorContext {
            actor,
            owner_creator_id,
            world_id: world.to_string(),
            binding_id: binding.map(str::to_string),
            branch_id: None,
            event_id: None,
            character_epoch,
            view: empty_view(),
        }
    }

    fn base_character_ctx() -> AdmittedActorContext {
        sample_ctx(
            AdmittedActor::Character {
                character_id: "chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            },
            "wld_worldA",
            Some("awb_cccccccccccccccccccccccccccccccc"),
        )
    }

    fn key_with(
        ctx: &AdmittedActorContext,
        provider: &str,
        cwd: &Path,
        model: Option<&str>,
        mode: Option<&str>,
    ) -> ActorSessionKey {
        ActorSessionRegistry::key_for(
            provider,
            cwd,
            model.map(str::to_string),
            mode.map(str::to_string),
            ctx,
        )
        .expect("canonical cwd")
    }

    fn host_req() -> CreateSessionRequest {
        CreateSessionRequest {
            provider_id: nexus_agent_host::ProviderId::new("prov"),
            cwd: PathBuf::from("/tmp"),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: nexus_agent_host::capability::model::SessionOwner {
                creator_id: "ctr_test".to_string(),
                workspace_root: PathBuf::from("/tmp"),
                orchestration_run_id: None,
            },
        }
    }

    struct ScriptedHost {
        sessions: Mutex<HashMap<HostSessionId, HostSession>>,
        creates: AtomicU64,
        fail_create: AtomicBool,
        fail_list: AtomicBool,
        fail_shutdown_remaining: AtomicU64,
        create_delay: Mutex<Duration>,
        list_delay: Mutex<Duration>,
        shutdown_delay: Mutex<Duration>,
        events: broadcast::Sender<HostEvent>,
    }

    impl ScriptedHost {
        fn new() -> Arc<Self> {
            let (events, _) = broadcast::channel(16);
            Arc::new(Self {
                sessions: Mutex::new(HashMap::new()),
                creates: AtomicU64::new(0),
                fail_create: AtomicBool::new(false),
                fail_list: AtomicBool::new(false),
                fail_shutdown_remaining: AtomicU64::new(0),
                create_delay: Mutex::new(Duration::from_millis(0)),
                list_delay: Mutex::new(Duration::from_millis(0)),
                shutdown_delay: Mutex::new(Duration::from_millis(0)),
                events,
            })
        }

        fn set_delay(&self, delay: Duration) {
            *self.create_delay.lock().expect("delay") = delay;
        }

        fn set_list_delay(&self, delay: Duration) {
            *self.list_delay.lock().expect("list delay") = delay;
        }

        fn set_shutdown_delay(&self, delay: Duration) {
            *self.shutdown_delay.lock().expect("shutdown delay") = delay;
        }

        fn fail_next_create(&self) {
            self.fail_create.store(true, Ordering::SeqCst);
        }

        fn fail_next_list(&self) {
            self.fail_list.store(true, Ordering::SeqCst);
        }

        fn fail_next_shutdowns(&self, count: u64) {
            self.fail_shutdown_remaining.store(count, Ordering::SeqCst);
        }

        fn set_state(&self, id: &HostSessionId, state: &SessionState) {
            let mut sessions = self.sessions.lock().expect("sessions");
            if let Some(session) = sessions.get_mut(id) {
                session.state = state.clone();
                session.active_op_id = state.active_op_id().cloned();
            }
        }
    }

    #[async_trait]
    impl HostFacade for ScriptedHost {
        async fn start(&self, _config: HostStartConfig) -> nexus_agent_host::HostResult<()> {
            Ok(())
        }

        async fn create_session(
            &self,
            request: CreateSessionRequest,
        ) -> nexus_agent_host::HostResult<HostSession> {
            let delay = *self.create_delay.lock().expect("delay");
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if self.fail_create.swap(false, Ordering::SeqCst) {
                return Err(nexus_agent_host::HostError::internal(
                    "injected create failure",
                ));
            }
            self.creates.fetch_add(1, Ordering::SeqCst);
            let session = HostSession {
                id: HostSessionId::new(),
                provider_id: request.provider_id,
                state: SessionState::Ready,
                created_at: chrono::Utc::now(),
                active_op_id: None,
                negotiated_capabilities: CapabilityDescriptor::native_cli_limited(),
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
            _session_id: HostSessionId,
            _op: HostOperation,
        ) -> nexus_agent_host::HostResult<HostEventStream> {
            Err(nexus_agent_host::HostError::internal("unused"))
        }

        async fn cancel(&self, _op_id: HostOperationId) -> nexus_agent_host::HostResult<()> {
            Ok(())
        }

        async fn health(&self) -> nexus_agent_host::HostResult<HostHealth> {
            Ok(HostHealth {
                running: true,
                active_sessions: self.sessions.lock().expect("sessions").len(),
                active_operations: 0,
            })
        }

        async fn shutdown(&self) -> nexus_agent_host::HostResult<()> {
            self.sessions.lock().expect("sessions").clear();
            Ok(())
        }

        async fn shutdown_session(
            &self,
            session_id: HostSessionId,
        ) -> nexus_agent_host::HostResult<()> {
            let delay = *self.shutdown_delay.lock().expect("shutdown delay");
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            let remaining = self.fail_shutdown_remaining.load(Ordering::SeqCst);
            if remaining > 0 {
                self.fail_shutdown_remaining
                    .store(remaining.saturating_sub(1), Ordering::SeqCst);
                return Err(nexus_agent_host::HostError::internal(
                    "injected shutdown failure",
                ));
            }
            self.sessions
                .lock()
                .expect("sessions")
                .remove(&session_id)
                .ok_or_else(|| nexus_agent_host::HostError::internal("session"))?;
            Ok(())
        }

        async fn list_sessions(&self) -> nexus_agent_host::HostResult<Vec<HostSession>> {
            let delay = *self.list_delay.lock().expect("list delay");
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if self.fail_list.swap(false, Ordering::SeqCst) {
                return Err(nexus_agent_host::HostError::internal(
                    "injected list failure",
                ));
            }
            Ok(self
                .sessions
                .lock()
                .expect("sessions")
                .values()
                .cloned()
                .collect())
        }

        async fn provider_catalog(&self) -> nexus_agent_host::HostResult<ProviderCatalog> {
            Ok(ProviderCatalog::new())
        }

        fn subscribe_events(&self, _session_id: HostSessionId) -> broadcast::Receiver<HostEvent> {
            self.events.subscribe()
        }
    }

    #[test]
    fn every_key_dimension_participates_in_equality() {
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let base = key_with(&ctx, "prov-a", cwd.path(), Some("m1"), Some("code"));

        let other_cwd = tempfile::tempdir().expect("cwd2");
        assert_ne!(
            base,
            key_with(&ctx, "prov-b", cwd.path(), Some("m1"), Some("code"))
        );
        assert_ne!(
            base,
            key_with(&ctx, "prov-a", other_cwd.path(), Some("m1"), Some("code"))
        );
        assert_ne!(
            base,
            key_with(&ctx, "prov-a", cwd.path(), Some("m2"), Some("code"))
        );
        assert_ne!(
            base,
            key_with(&ctx, "prov-a", cwd.path(), Some("m1"), Some("ask"))
        );

        let mut actor = ctx.clone();
        actor.actor = AdmittedActor::Character {
            character_id: "chr_dddddddddddddddddddddddddddddddd".into(),
        };
        assert_ne!(
            base,
            key_with(&actor, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );

        let mut world = ctx.clone();
        world.world_id = "wld_worldB".into();
        assert_ne!(
            base,
            key_with(&world, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );

        let mut binding = ctx.clone();
        binding.binding_id = Some("awb_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into());
        assert_ne!(
            base,
            key_with(&binding, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );

        let mut branch = ctx.clone();
        branch.branch_id = Some("fbk_branch1".into());
        assert_ne!(
            base,
            key_with(&branch, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );

        let mut event = ctx;
        event.event_id = Some("evt_anchor1".into());
        assert_ne!(
            base,
            key_with(&event, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );

        let creator = sample_ctx(
            AdmittedActor::Creator {
                creator_id: "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            },
            "wld_worldA",
            None,
        );
        assert_ne!(
            base,
            key_with(&creator, "prov-a", cwd.path(), Some("m1"), Some("code"))
        );
    }

    #[test]
    fn canonical_cwd_collapses_symlink_aliases() {
        let real = tempfile::tempdir().expect("real");
        let alias_root = tempfile::tempdir().expect("alias root");
        let link = alias_root.path().join("link");
        std::os::unix::fs::symlink(real.path(), &link).expect("symlink");
        let ctx = base_character_ctx();
        let a = key_with(&ctx, "prov", real.path(), None, None);
        let b = key_with(&ctx, "prov", &link, None, None);
        assert_eq!(a, b);
    }

    #[test]
    fn relative_cwd_is_rejected_by_host_helper() {
        let ctx = base_character_ctx();
        let err = ActorSessionRegistry::key_for("prov", Path::new("relative"), None, None, &ctx)
            .expect_err("relative");
        assert_eq!(err.error_code(), "forbidden");
    }

    #[tokio::test]
    async fn same_key_converges_and_concurrent_creates_mint_one_host_session() {
        let host = ScriptedHost::new();
        host.set_delay(Duration::from_millis(40));
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let host_a = host.clone();
        let host_b = host.clone();
        let reg_a = registry.clone();
        let reg_b = registry.clone();
        let key_a = key.clone();
        let key_b = key;
        let ctx_a = ctx.clone();
        let ctx_b = ctx;

        let (left, right) = tokio::join!(
            async move {
                let host_ref: &dyn HostFacade = host_a.as_ref();
                reg_a
                    .resolve_or_create(key_a, ctx_a, host_ref, || async {
                        host_ref
                            .create_session(host_req())
                            .await
                            .map_err(|e| map_host(&e))
                    })
                    .await
            },
            async move {
                let host_ref: &dyn HostFacade = host_b.as_ref();
                reg_b
                    .resolve_or_create(key_b, ctx_b, host_ref, || async {
                        host_ref
                            .create_session(host_req())
                            .await
                            .map_err(|e| map_host(&e))
                    })
                    .await
            }
        );

        let left = left.expect("left");
        let right = right.expect("right");
        assert_eq!(left.id, right.id);
        assert_eq!(host.creates.load(Ordering::SeqCst), 1);
        assert_eq!(registry.len(), 1);
    }

    #[tokio::test]
    async fn busy_exact_match_is_conflict_coded() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let created = registry
            .resolve_or_create(key.clone(), ctx.clone(), host.as_ref(), || async {
                host.create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
            .expect("create");
        host.set_state(&created.id, &SessionState::Busy(HostOperationId::new()));

        let err = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                panic!("must not create while busy");
            })
            .await
            .expect_err("busy");
        assert_eq!(err.error_code(), "actor_session_busy");
        assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
        assert_eq!(host.creates.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn missing_or_terminal_match_is_evicted_and_replaced() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = registry
            .resolve_or_create(key.clone(), ctx.clone(), host.as_ref(), || async {
                host.create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
            .expect("first");

        host.set_state(&first.id, &SessionState::Stopped);
        let replaced = registry
            .resolve_or_create(key.clone(), ctx.clone(), host.as_ref(), || async {
                host.create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
            .expect("replace terminal");
        assert_ne!(replaced.id, first.id);
        assert!(registry.context_for(&first.id).is_none());

        host.shutdown_session(replaced.id.clone())
            .await
            .expect("drop host row");
        let after_missing = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                host.create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
            .expect("replace missing");
        assert_ne!(after_missing.id, replaced.id);
        assert_eq!(host.creates.load(Ordering::SeqCst), 3);
        assert!(registry.is_actor_session(&first.id));
        assert!(host
            .sessions
            .lock()
            .expect("sessions")
            .get(&first.id)
            .is_none());
    }

    #[tokio::test]
    async fn close_drains_indexed_host_sessions() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let session = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                host.create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
            .expect("create");
        registry.close();
        assert!(registry.is_empty());
        assert!(registry.is_actor_session(&session.id));
        registry
            .drain_host_sessions(host.as_ref())
            .await
            .expect("drain");
        let live = host.list_sessions().await.expect("list");
        assert!(live.is_empty());
        let err = registry
            .resolve_or_create(
                key_with(&base_character_ctx(), "prov", cwd.path(), None, None),
                base_character_ctx(),
                host.as_ref(),
                || async {
                    panic!("must not mint after close");
                },
            )
            .await
            .expect_err("closed");
        assert_eq!(err.error_code(), "service_unavailable");
    }

    #[tokio::test]
    async fn session_and_daemon_shutdown_drop_indexes() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let session = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                host.create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
            .expect("create");
        assert_eq!(registry.len(), 1);
        registry
            .shutdown_session(session.id.clone(), host.as_ref())
            .await
            .expect("session shutdown");
        assert!(registry.is_empty());
        assert!(registry.context_for(&session.id).is_none());
        assert_eq!(registry.lock_entry_count(), 0);

        let ctx = base_character_ctx();
        let cwd = tempfile::tempdir().expect("cwd");
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let _ = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                host.create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
            .expect("second");
        registry.clear();
        assert!(registry.is_empty());
    }

    async fn mint(
        registry: &ActorSessionRegistry,
        host: &Arc<ScriptedHost>,
        key: ActorSessionKey,
        ctx: AdmittedActorContext,
    ) -> HostSession {
        registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                host.create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
            .expect("mint")
    }

    #[tokio::test]
    async fn shutdown_holds_key_lock_across_host_teardown() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = mint(&registry, &host, key.clone(), ctx.clone()).await;
        host.set_shutdown_delay(Duration::from_millis(40));

        let reg_shut = registry.clone();
        let host_shut = host.clone();
        let first_id = first.id.clone();
        let shutdown = tokio::spawn(async move {
            reg_shut
                .shutdown_session(first_id, host_shut.as_ref())
                .await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        let replaced = mint(&registry, &host, key, ctx).await;
        shutdown.await.expect("join").expect("shutdown");
        assert_ne!(replaced.id, first.id);
        assert!(registry.context_for(&first.id).is_none());
        assert!(registry.context_for(&replaced.id).is_some());
        assert_eq!(host.creates.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn reuse_completes_before_overlapping_shutdown() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = mint(&registry, &host, key.clone(), ctx.clone()).await;
        host.set_list_delay(Duration::from_millis(40));

        let reg_a = registry.clone();
        let host_a = host.clone();
        let key_a = key;
        let ctx_a = ctx;
        let reuse = tokio::spawn(async move { mint(&reg_a, &host_a, key_a, ctx_a).await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        registry
            .shutdown_session(first.id.clone(), host.as_ref())
            .await
            .expect("shutdown after reuse started");
        let reused = reuse.await.expect("join");
        assert_eq!(reused.id, first.id);
        assert!(registry.is_empty());
        assert_eq!(host.creates.load(Ordering::SeqCst), 1);
        assert_eq!(registry.lock_entry_count(), 0);
    }

    #[tokio::test]
    async fn close_aborts_in_flight_create_without_repopulation() {
        let host = ScriptedHost::new();
        host.set_delay(Duration::from_millis(40));
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let reg_a = registry.clone();
        let host_a = host.clone();
        let create = tokio::spawn(async move {
            reg_a
                .resolve_or_create(key, ctx, host_a.as_ref(), || async {
                    host_a
                        .create_session(host_req())
                        .await
                        .map_err(|e| map_host(&e))
                })
                .await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        registry.close();
        let err = create.await.expect("join").expect_err("closed");
        assert_eq!(err.error_code(), "service_unavailable");
        assert!(registry.is_empty());
        assert!(host.sessions.lock().expect("sessions").is_empty());
        let cwd = tempfile::tempdir().expect("cwd2");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let err = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                panic!("must not create after close");
            })
            .await
            .expect_err("still closed");
        assert_eq!(err.error_code(), "service_unavailable");
    }

    #[tokio::test]
    async fn lock_is_reclaimed_without_splitting_live_key() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = mint(&registry, &host, key.clone(), ctx.clone()).await;
        assert_eq!(registry.lock_entry_count(), 1);
        registry
            .shutdown_session(first.id.clone(), host.as_ref())
            .await
            .expect("shutdown");
        assert_eq!(registry.lock_entry_count(), 0);

        host.set_delay(Duration::from_millis(30));
        let host_a = host.clone();
        let host_b = host.clone();
        let reg_a = registry.clone();
        let reg_b = registry.clone();
        let key_a = key.clone();
        let key_b = key;
        let ctx_a = ctx.clone();
        let ctx_b = ctx;
        let (left, right) = tokio::join!(
            async move { mint(&reg_a, &host_a, key_a, ctx_a).await },
            async move { mint(&reg_b, &host_b, key_b, ctx_b).await }
        );
        assert_eq!(left.id, right.id);
        assert_eq!(host.creates.load(Ordering::SeqCst), 2);
        assert_eq!(registry.lock_entry_count(), 1);
    }

    #[tokio::test]
    async fn failed_create_reclaims_unindexed_key_lock() {
        let host = ScriptedHost::new();
        host.fail_next_create();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let err = registry
            .resolve_or_create(key.clone(), ctx.clone(), host.as_ref(), || async {
                host.create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
            .expect_err("create fail");
        assert_eq!(err.error_code(), "internal");
        assert!(registry.is_empty());
        assert_eq!(registry.lock_entry_count(), 0);
        assert!(host.sessions.lock().expect("sessions").is_empty());

        let minted = mint(&registry, &host, key, ctx).await;
        assert_eq!(host.creates.load(Ordering::SeqCst), 1);
        assert_eq!(registry.lock_entry_count(), 1);
        assert_eq!(registry.len(), 1);
        assert_eq!(minted.state, SessionState::Ready);
    }

    #[tokio::test]
    async fn failed_replacement_create_reclaims_lock_after_stale_eviction() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = mint(&registry, &host, key.clone(), ctx.clone()).await;
        host.sessions.lock().expect("sessions").remove(&first.id);
        host.fail_next_create();
        let err = registry
            .resolve_or_create(key.clone(), ctx.clone(), host.as_ref(), || async {
                host.create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
            .expect_err("replacement create fail");
        assert_eq!(err.error_code(), "internal");
        assert!(registry.is_empty());
        assert_eq!(registry.lock_entry_count(), 0);

        let replaced = mint(&registry, &host, key, ctx).await;
        assert_ne!(replaced.id, first.id);
        assert_eq!(registry.lock_entry_count(), 1);
    }

    #[tokio::test]
    async fn closed_minted_cleanup_retries_then_succeeds() {
        let host = ScriptedHost::new();
        host.set_delay(Duration::from_millis(40));
        host.fail_next_shutdowns(2);
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let host_a = host.clone();
        let reg = registry.clone();
        let create = tokio::spawn(async move {
            reg.resolve_or_create(key, ctx, host_a.as_ref(), || async {
                host_a
                    .create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        registry.close();
        let err = create.await.expect("join").expect_err("closed");
        assert_eq!(err.error_code(), "service_unavailable");
        assert!(registry.is_empty());
        assert_eq!(registry.lock_entry_count(), 0);
        assert!(host.sessions.lock().expect("sessions").is_empty());
    }

    #[tokio::test]
    async fn closed_minted_cleanup_failure_is_surfaced_and_host_row_remains() {
        let host = ScriptedHost::new();
        host.set_delay(Duration::from_millis(40));
        host.fail_next_shutdowns(8);
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let host_a = host.clone();
        let reg = registry.clone();
        let create = tokio::spawn(async move {
            reg.resolve_or_create(key, ctx, host_a.as_ref(), || async {
                host_a
                    .create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        registry.close();
        let err = create.await.expect("join").expect_err("cleanup");
        assert_eq!(err.error_code(), "internal");
        assert!(err.to_string().contains("injected shutdown failure"));
        assert!(registry.is_empty());
        assert_eq!(registry.lock_entry_count(), 0);
        assert_eq!(host.sessions.lock().expect("sessions").len(), 1);
        host.fail_next_shutdowns(0);
        let leftover = host
            .sessions
            .lock()
            .expect("sessions")
            .keys()
            .next()
            .cloned()
            .expect("leaked host row");
        host.shutdown_session(leftover)
            .await
            .expect("retry host teardown");
        assert!(host.sessions.lock().expect("sessions").is_empty());
    }

    #[tokio::test]
    async fn close_races_list_error_reclaims_unindexed_lock() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let _first = mint(&registry, &host, key.clone(), ctx.clone()).await;
        host.set_list_delay(Duration::from_millis(40));
        host.fail_next_list();
        let reg = registry.clone();
        let host_a = host.clone();
        let lookup = tokio::spawn(async move {
            reg.resolve_or_create(key, ctx, host_a.as_ref(), || async {
                panic!("must not create after list error");
            })
            .await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        registry.close();
        let err = lookup.await.expect("join").expect_err("list error");
        assert_eq!(err.error_code(), "internal");
        assert!(err.to_string().contains("injected list failure"));
        assert!(registry.is_empty());
        assert_eq!(registry.lock_entry_count(), 0);
    }

    #[tokio::test]
    async fn close_races_busy_list_reclaims_unindexed_lock() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = mint(&registry, &host, key.clone(), ctx.clone()).await;
        host.set_state(&first.id, &SessionState::Busy(HostOperationId::new()));
        host.set_list_delay(Duration::from_millis(40));
        let reg = registry.clone();
        let host_a = host.clone();
        let lookup = tokio::spawn(async move {
            reg.resolve_or_create(key, ctx, host_a.as_ref(), || async {
                panic!("must not create while listing busy");
            })
            .await
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        registry.close();
        let err = lookup.await.expect("join").expect_err("busy or closed");
        assert!(
            err.error_code() == "actor_session_busy" || err.error_code() == "service_unavailable",
            "unexpected {}",
            err.error_code()
        );
        assert!(registry.is_empty());
        assert_eq!(registry.lock_entry_count(), 0);
    }

    #[tokio::test]
    async fn busy_without_close_retains_indexed_lock() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = mint(&registry, &host, key.clone(), ctx.clone()).await;
        host.set_state(&first.id, &SessionState::Busy(HostOperationId::new()));
        let err = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                panic!("must not create while busy");
            })
            .await
            .expect_err("busy");
        assert_eq!(err.error_code(), "actor_session_busy");
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.lock_entry_count(), 1);
    }

    #[tokio::test]
    async fn close_races_indexed_shutdown_error_reclaims_lock() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = mint(&registry, &host, key, ctx).await;
        host.set_shutdown_delay(Duration::from_millis(40));
        host.fail_next_shutdowns(8);
        let reg = registry.clone();
        let host_a = host.clone();
        let sid = first.id.clone();
        let shutdown =
            tokio::spawn(async move { reg.shutdown_session(sid, host_a.as_ref()).await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        registry.close();
        let err = shutdown.await.expect("join").expect_err("shutdown error");
        assert_eq!(err.error_code(), "internal");
        assert!(err.to_string().contains("injected shutdown failure"));
        assert!(registry.is_empty());
        assert_eq!(registry.lock_entry_count(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn close_after_final_reclaim_before_owner_return_clears_key_locks() {
        let host = ScriptedHost::new();
        let registry = ActorSessionRegistry::new();
        let cwd = tempfile::tempdir().expect("cwd");
        let ctx = base_character_ctx();
        let key = key_with(&ctx, "prov", cwd.path(), None, None);
        let first = mint(&registry, &host, key.clone(), ctx.clone()).await;
        assert_eq!(registry.lock_entry_count(), 1);

        let during_owner_hold = Arc::new(AtomicUsize::new(usize::MAX));
        let during_owner_hold_hook = Arc::clone(&during_owner_hold);
        let closed_registry = registry.clone();
        registry.set_after_reclaim(move || {
            closed_registry.close();
            during_owner_hold_hook.store(closed_registry.lock_entry_count(), Ordering::SeqCst);
        });

        let reused = registry
            .resolve_or_create(key, ctx, host.as_ref(), || async {
                panic!("must reuse Ready before close hook");
            })
            .await
            .expect("reuse then close");
        assert_eq!(reused.id, first.id);
        assert_eq!(during_owner_hold.load(Ordering::SeqCst), 0);
        assert!(registry.is_empty());
        assert_eq!(registry.lock_entry_count(), 0);
        assert_eq!(host.creates.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn legacy_sessions_are_never_indexed() {
        let registry = ActorSessionRegistry::new();
        assert!(registry.context_for(&HostSessionId::new()).is_none());
        assert!(registry.is_empty());
    }

    // ── v1.185 P0 Task 2: per-Character activity fences + retired tombstones ──

    const DB_OWNER: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DB_WORLD: &str = "wld_worldA";

    /// Seed an owned active Character (with one active binding to an owned
    /// active World) behind a fresh registry; returns (registry, pool, tmp,
    /// `character_id`, `binding_id`).
    async fn seeded_character() -> (
        ActorSessionRegistry,
        sqlx::SqlitePool,
        crate::test_utils::TestTempRoot,
        String,
        String,
    ) {
        let (tmp, home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = crate::workspace::WorkspaceState::new_for_testing(home, db_path, None).await;
        let pool = state.pool().unwrap().clone();
        nexus_local_db::ensure_creator_row(&pool, DB_OWNER, "Owner")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(DB_WORLD)
        .bind(DB_OWNER)
        .bind(DB_WORLD)
        .bind(DB_WORLD)
        .execute(&pool)
        .await
        .unwrap();
        let created = nexus_local_db::create_character_with_initial_binding(
            &pool,
            nexus_local_db::CreateCharacterParams {
                owner_creator_id: DB_OWNER,
                display_name: "Ada",
                image_uri: None,
                persona_json: "{}",
                world_id: DB_WORLD,
                world_sheet_entry_id: None,
            },
        )
        .await
        .unwrap();
        (
            ActorSessionRegistry::new(),
            pool,
            tmp,
            created.character.character_id,
            created.binding.binding_id,
        )
    }

    /// An outstanding activity (admitted read guard) makes the lifecycle
    /// exclusive `try_character_transition` refuse `character_busy`; dropping
    /// the guard admits it again (durable §11.3.1/§11.3.2).
    #[tokio::test]
    async fn archive_try_refuses_while_activity_outstanding() {
        let (registry, pool, tmp, character_id, _binding) = seeded_character().await;
        let guard = registry
            .admit_character_activity(&pool, DB_OWNER, &character_id)
            .await
            .expect("admit active activity");

        let busy = registry
            .try_character_transition(&pool, DB_OWNER, &character_id)
            .await
            .expect_err("busy while activity in flight");
        assert_eq!(busy.error_code(), "character_busy");
        assert_eq!(busy.status_code(), axum::http::StatusCode::CONFLICT);
        assert_eq!(guard.epoch(), 0);

        drop(guard);
        let transition = registry
            .try_character_transition(&pool, DB_OWNER, &character_id)
            .await
            .expect("transition admits after activity drains");
        assert_eq!(transition.character_id(), character_id);
        assert_eq!(transition.epoch(), 0);

        drop(tmp);
    }

    /// `admit_character_activity` re-checks the lifecycle under the fence: an
    /// owned-but-archived Character refuses `character_inactive`, and a
    /// foreign/missing Character is 404 (existence hidden).
    #[tokio::test]
    async fn admit_activity_rejects_archived_and_foreign() {
        let (registry, pool, tmp, character_id, _binding) = seeded_character().await;

        sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
            .bind(&character_id)
            .execute(&pool)
            .await
            .unwrap();
        let err = registry
            .admit_character_activity(&pool, DB_OWNER, &character_id)
            .await
            .expect_err("archived");
        assert_eq!(err.error_code(), "character_inactive");
        assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);

        let foreign = registry
            .admit_character_activity(&pool, "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", &character_id)
            .await
            .expect_err("foreign owner");
        assert_eq!(foreign.error_code(), "not_found");

        let missing = registry
            .admit_character_activity(&pool, DB_OWNER, "chr_ffffffffffffffffffffffffffffffff")
            .await
            .expect_err("missing");
        assert_eq!(missing.error_code(), "not_found");

        drop(tmp);
    }

    /// A material transition retires the Character's indexed sessions into
    /// owner-retaining tombstones: reuse keys are gone, the retired id stays
    /// recognizably Actor-mode (never legacy) with its owner identity, and a
    /// fresh epoch key is distinct (no reuse of old history).
    #[tokio::test]
    async fn retire_character_sessions_tombstones_owner_and_fresh_epoch_reuses_nothing() {
        let (registry, _pool, tmp, character_id, binding_id) = seeded_character().await;
        let host = ScriptedHost::new();
        let cwd = tempfile::tempdir().expect("cwd");

        let mut ctx = sample_ctx(
            AdmittedActor::Character {
                character_id: character_id.clone(),
            },
            DB_WORLD,
            Some(&binding_id),
        );
        ctx.character_epoch = Some(0);
        ctx.owner_creator_id = DB_OWNER.to_string();

        let epoch0 = key_with(&ctx, "prov", cwd.path(), None, None);
        let old_session = mint(&registry, &host, epoch0.clone(), ctx.clone()).await;
        assert_eq!(registry.len(), 1);

        // A fresh (post-restore) epoch key must be distinct → resolve_or_create
        // mints a NEW host session rather than reusing the pre-transition one.
        ctx.character_epoch = Some(1);
        let epoch1 = key_with(&ctx, "prov", cwd.path(), None, None);
        assert_ne!(epoch0, epoch1);
        let new_session = mint(&registry, &host, epoch1.clone(), ctx.clone()).await;
        assert_ne!(new_session.id, old_session.id);
        assert_eq!(registry.len(), 2);

        // Retire the Character's sessions after a material transition: every
        // indexed key for this Character is removed and its ids tombstoned
        // (owner retained, Actor mode — never legacy). The fresh-epoch session
        // is also retired, because a committed transition invalidates all the
        // Character's reusable sessions; a later restore mints a brand-new one.
        let mut retired_ids = registry.retire_character_sessions(&character_id);
        retired_ids.sort_by_key(std::string::ToString::to_string);
        let mut expected = vec![old_session.id.clone(), new_session.id.clone()];
        expected.sort_by_key(std::string::ToString::to_string);
        assert_eq!(retired_ids, expected);
        assert_eq!(registry.len(), 0);
        for id in [&old_session.id, &new_session.id] {
            assert!(registry.is_actor_session(id));
            assert!(registry.context_for(id).is_none());
            let (owner, kind, retired) = registry.stored_session_owner(id).unwrap();
            assert_eq!(owner, DB_OWNER);
            assert_eq!(kind, ActorSessionKind::Character);
            assert!(retired);
        }

        // The old epoch key is gone: re-resolving the same ctx mints a NEW
        // host session rather than reusing a retired id.
        let after_retire = registry
            .resolve_or_create(epoch0.clone(), ctx.clone(), host.as_ref(), || async {
                host.create_session(host_req())
                    .await
                    .map_err(|e| map_host(&e))
            })
            .await
            .expect("mint after retire");
        assert_ne!(after_retire.id, old_session.id);
        assert_ne!(after_retire.id, new_session.id);
        assert_eq!(registry.len(), 1);

        drop(tmp);
    }
    /// Regression: lifecycle epoch used for material-vs-no-op decisions is
    /// re-read under the exclusive fence after any interleaved transition.
    #[tokio::test]
    async fn transition_guard_rereads_epoch_under_exclusive_fence() {
        let (registry, pool, tmp, character_id, _) = seeded_character().await;
        {
            let guard = registry
                .try_character_transition(&pool, DB_OWNER, &character_id)
                .await
                .expect("first material archive");
            assert_eq!(guard.epoch(), 0);
            nexus_local_db::transition_character(
                &pool,
                DB_OWNER,
                &character_id,
                0,
                nexus_local_db::CharacterStatus::Archived,
            )
            .await
            .expect("archive");
        }
        let guard = registry
            .try_character_transition(&pool, DB_OWNER, &character_id)
            .await
            .expect("same-state archive acquires fence");
        assert_eq!(
            guard.epoch(),
            1,
            "epoch must be re-read under fence, not a stale pre-fence snapshot"
        );
        let row = nexus_local_db::transition_character(
            &pool,
            DB_OWNER,
            &character_id,
            1,
            nexus_local_db::CharacterStatus::Archived,
        )
        .await
        .expect("same-state archive no-op");
        assert_eq!(guard.epoch(), row.lifecycle_epoch);
        drop(tmp);
    }

    /// Terminal operation FIFO evicts the oldest committed outcome after 1024 rows.
    #[test]
    fn terminal_operation_fifo_evicts_oldest() {
        let registry = ActorSessionRegistry::new();
        let ctx = base_character_ctx();
        let session_id = HostSessionId::new();
        let mut first_op = None;
        let mut last_op = None;
        for i in 0..1025 {
            let op_id = HostOperationId::new();
            if i == 0 {
                first_op = Some(op_id.clone());
            }
            if i == 1024 {
                last_op = Some(op_id.clone());
            }
            let snapshot = CharacterOperationSnapshot {
                owner_creator_id: DB_OWNER.to_string(),
                ctx: ctx.clone(),
                session_id: session_id.clone(),
                operation_id: op_id.clone(),
                remember: false,
                raw_prompt: "x".into(),
            };
            registry
                .reserve_character_operation(snapshot)
                .expect("reserve");
            registry.commit_operation_terminal(
                &op_id,
                CharacterOperationResult {
                    operation_id: op_id.to_string(),
                    session_id: session_id.to_string(),
                    run_status: CharacterOperationResultRunStatus::Succeeded,
                    finish_reason: None,
                    capture: NexusCharacterRunCaptureOutcome {
                        status: NexusCharacterRunCaptureOutcomeStatus::Disabled,
                        pending_id: None,
                        code: None,
                    },
                },
            );
        }
        let first = first_op.expect("first");
        assert!(registry
            .character_operation_result(DB_OWNER, &first)
            .is_err());
        let last = last_op.expect("last");
        assert!(registry.character_operation_result(DB_OWNER, &last).is_ok());
    }

    /// Archive refuses while a Character prompt activity guard is still held.
    #[tokio::test]
    async fn archive_refuses_while_prompt_activity_outstanding() {
        let (registry, pool, tmp, character_id, _binding) = seeded_character().await;
        let guard = registry
            .admit_character_activity(&pool, DB_OWNER, &character_id)
            .await
            .expect("admit prompt activity");
        let busy = registry
            .try_character_transition(&pool, DB_OWNER, &character_id)
            .await
            .expect_err("archive blocked during capture drain");
        assert_eq!(busy.error_code(), "character_busy");
        drop(guard);
        drop(tmp);
    }
}
