//! `OrchestrationEngine` trait + `GraphFlowEngine` adapter over `graph-flow`.
//!
//! ## WS2 R3: Arc<FlowRunner> per session
//!
//! The engine stores `Arc<FlowRunner>` instead of cloning `FlowRunner` on every
//! step, avoiding unnecessary clone overhead while ensuring internal state is
//! shared correctly.
//!
//! ## WS3 R1: `EngineSharedState` extraction
//!
//! Shared state (`storage`, `runners`, `sessions`) is extracted into
//! `EngineSharedState`, eliminating duplication between `GraphFlowEngine` and
//! `EngineProxy`. Both hold an `Arc<EngineSharedState>`.
//!
//! Design: `.mstar/specs/orchestration-engine.md` §4.2.

use async_trait::async_trait;
use graph_flow::{ExecutionStatus, FlowRunner, Graph, SessionStorage};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

// Re-export for internal use.
#[cfg(test)]
use crate::capability::CapabilityError;
use crate::capability::CapabilityRegistry;
use crate::run_state::{
    ChildCheckpoint, PresetSourceIdentity, RunCheckpoint, RunDescriptorV1, RunFailure, RunRecord,
    RunStateV1, WorkflowStateStore,
};

/// Context key that effectful tasks set when they perform an external effect
/// (capability call, ACP/Host prompt dispatch, `HostTool` call, child spawn).
///
/// `run_step_internal` inspects the post-step root context for this marker to
/// decide the disposition of a failed post-effect `commit_transition`: if an
/// external effect may have executed, the run is **interrupted** (never a
/// blind rewind to a seemingly safe boundary — A2/A7); if the step was
/// deterministic (no marker), a CAS-fenced restore is acceptable (Important 3).
pub const EXTERNAL_EFFECT_MARKER: &str = "__external_effect_ran";

/// Upper bound on cancel-intent fence reload/retry attempts (Finding 1).
///
/// A cancel that loses the phase-1 revision CAS to a concurrent transition
/// (e.g. `mark_step_in_flight`) reloads the authoritative row and re-fences
/// against the new revision while the run is still cancellable. The bound
/// keeps the retry bounded: after it is exhausted the CAS loss is surfaced
/// so the caller projects the exact public conflict after reloading.
pub const CANCEL_FENCE_RETRY_BOUND: u32 = 8;

/// Whether the post-step root context carries the [`EXTERNAL_EFFECT_MARKER`],
/// indicating that one or more external effects executed during the step.
async fn step_had_external_effect(root: &graph_flow::Session) -> bool {
    root.context
        .get::<bool>(EXTERNAL_EFFECT_MARKER)
        .await
        .unwrap_or(false)
}

/// Clear the [`EXTERNAL_EFFECT_MARKER`] from a root context (Minor 2): the
/// marker must reflect current-step intent only, not history. A prior step's
/// effect must not make a later unrelated deterministic commit failure
/// over-classified Interrupted.
///
/// Called before `commit_transition` persists the step's context, so the
/// durable context written by the next checkpoint stops carrying the stale
/// marker. The marker is captured into a local `had_effect` binding first so a
/// failed post-effect commit can still classify as Interrupted (Important 3).
async fn clear_step_effect_marker(root: &graph_flow::Session) {
    root.context.remove(EXTERNAL_EFFECT_MARKER).await;
}

// ---------------------------------------------------------------------------
// Helper types
// ---------------------------------------------------------------------------

/// Opaque session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SessionId(pub String);

/// Composite key that uniquely identifies a session within the engine.
#[derive(Debug, Clone)]
pub struct SessionKey {
    pub creator_id: String,
    pub preset_id: String,
    pub instance_id: String,
}

impl SessionKey {
    /// Deterministic key for tests (and integration tests).
    #[must_use]
    pub fn test_fixture() -> Self {
        Self {
            creator_id: "test-creator".into(),
            preset_id: "test-preset".into(),
            instance_id: "test-instance".into(),
        }
    }
}

/// Optional filters for [`OrchestrationEngine::list_active`].
#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    pub creator_id: Option<String>,
    pub preset_id: Option<String>,
}

/// Lightweight summary returned by [`OrchestrationEngine::list_active`].
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub creator_id: String,
    pub preset_id: String,
    pub status: SessionStatus,
    pub current_task_id: Option<String>,
}

/// Runtime status of a session.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SessionStatus {
    Running,
    Paused,
    WaitingForInput,
    Completed,
    Failed,
    /// Cancelled by an operator; terminal, never auto-driven.
    Cancelled,
    /// Interrupted — uncertain in-flight work; stopped, never safe retry.
    Interrupted,
}

impl SessionStatus {
    /// Returns `true` if the session is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }

    /// Returns `true` if the session has completed successfully.
    #[must_use]
    pub const fn is_completed(&self) -> bool {
        matches!(self, Self::Completed)
    }

    /// Map to the authoritative DB `status` string (A2).
    #[must_use]
    pub const fn as_db_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::WaitingForInput => "waiting_for_input",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    /// Parse a DB `status` string back into a [`SessionStatus`].
    ///
    /// Returns `None` for unknown/unsupported values (A7): a corrupt or
    /// ambiguous status must never be silently reinterpreted as `Running`.
    /// Callers that need strict parsing should treat `None` as
    /// unknown/non-replayable.
    #[must_use]
    pub fn from_db_str(status: &str) -> Option<Self> {
        match status {
            "running" => Some(Self::Running),
            "paused" => Some(Self::Paused),
            "waiting_for_input" => Some(Self::WaitingForInput),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }
}

/// Outcome of a single engine step.
#[derive(Debug, Clone)]
pub enum StepOutcome {
    Completed {
        response: Option<String>,
    },
    Paused {
        next_task_id: String,
        reason: String,
    },
    WaitingForInput {
        response: Option<String>,
    },
    Error(String),
}

impl StepOutcome {
    /// Returns `true` if the outcome requires user input.
    #[must_use]
    pub const fn is_waiting_for_input(&self) -> bool {
        matches!(self, Self::WaitingForInput { .. })
    }
}

/// Signals that external callers (HTTP, CLI) can send to the engine.
#[derive(Debug, Clone)]
pub enum EngineSignal {
    Pause,
    Resume,
    Cancel,
    Advance,
    /// Authorized continuation of a human wait (A4). Carries the exact
    /// durable wait token; a stale/consumed/wrong token loses the CAS and
    /// is refused with [`EngineError::WaitConflict`] — never a second
    /// driver and never an auto-approval.
    Continue {
        wait_id: String,
    },
}

/// Parameters for spawning a child session (inner graph).
pub struct ChildSessionParams {
    /// ID of the parent session.
    pub parent_session_id: String,
    /// The inner graph to execute.
    pub inner_graph: Arc<Graph>,
    /// Initial context for the child (inherits `core_context.*` + `preset.input.*`).
    pub initial_context: graph_flow::Context,
}

/// Thin wrapper around [`graph_flow::Context`].
///
/// In future tasks this will carry engine-specific metadata alongside the
/// graph-flow context (e.g. creator memory keys, preset input bindings).
#[derive(Debug, Clone)]
pub struct Context {
    #[allow(dead_code)]
    inner: graph_flow::Context,
}

impl Context {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: graph_flow::Context::new(),
        }
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced by the orchestration engine.
#[derive(Error, Debug)]
pub enum EngineError {
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("graph-flow error: {0}")]
    GraphFlow(#[from] graph_flow::GraphError),
    #[error(
        "no graph loaded — run_step requires a graph (set via start_session or system preset)"
    )]
    NoGraphLoaded,
    /// A revision compare-and-swap failed: the persisted `state_revision` no
    /// longer equals the expected value (a concurrent transition won).
    #[error(
        "state revision mismatch for session {session_id}: expected {expected}, found {found}"
    )]
    RevisionMismatch {
        /// Session id.
        session_id: String,
        /// Expected revision.
        expected: u64,
        /// Persisted revision.
        found: u64,
    },
    /// A transition was attempted on a row that is already terminal or
    /// cancelled — late graph saves must not overwrite newer checkpoint state.
    #[error("session {0} is already terminal/cancelled; refusing transition")]
    TerminalState(String),
    /// A new v1 run was requested for a session id that already has a row.
    ///
    /// A real explicit new start creates a **new** v1 run (new session id);
    /// it never launders an existing uncertain legacy row in place (A2).
    #[error(
        "session {session_id} already exists (execution_version={execution_version}); \
         a new run must use a new session id"
    )]
    RunAlreadyExists {
        /// Session id.
        session_id: String,
        /// Persisted execution version of the existing row.
        execution_version: u32,
    },
    /// A human-wait continuation token was stale, consumed, or wrong (A4).
    ///
    /// The caller must surface the current persisted status and current
    /// wait id (null when none) so the operator can re-issue the exact
    /// token. No mutation occurred and no second driver was started.
    #[error("wait conflict for session {session_id}: token does not match the current wait")]
    WaitConflict {
        /// Session id.
        session_id: String,
        /// Current persisted status.
        status: SessionStatus,
        /// Current durable wait id (null when the run is not waiting).
        current_wait_id: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Adapter layer over any graph-flow-like execution backend.
///
/// Daemon code depends on **this trait**, not on `graph_flow` directly.
/// If the upstream crate ships breaking changes, swap the impl — callers are
/// insulated.
#[async_trait]
pub trait OrchestrationEngine: Send + Sync {
    /// Execute exactly one step for the given session.
    async fn run_step(&self, session_id: &SessionId) -> Result<StepOutcome, EngineError>;

    /// Create a new session identified by `key`, seeded with `ctx`.
    async fn new_session(&self, key: SessionKey, ctx: Context) -> Result<SessionId, EngineError>;

    /// Start a session on a specific graph (for preset-driven execution).
    async fn start_session_with_graph(
        &self,
        id_prefix: &str,
        graph: Arc<Graph>,
    ) -> Result<SessionId, EngineError>;

    /// Query the current status of a session.
    async fn get_status(&self, session_id: &SessionId) -> Result<SessionStatus, EngineError>;

    /// Send a control signal (pause / resume / cancel / advance) to a session.
    async fn signal(&self, session_id: &SessionId, signal: EngineSignal)
        -> Result<(), EngineError>;

    /// List sessions that are still active (running / paused / waiting).
    async fn list_active(&self, filter: SessionFilter) -> Result<Vec<SessionSummary>, EngineError>;

    /// Spawn a child session for inner graph execution (§3.4 graph-of-graphs).
    ///
    /// The child session runs on `inner_graph` with `initial_context`.
    /// Returns the child session ID.
    async fn spawn_child_session(
        &self,
        params: ChildSessionParams,
    ) -> Result<SessionId, EngineError>;

    /// Return a persisted child session id for the given parent and inner
    /// graph when one already exists (recovery attach, Important 4).
    ///
    /// After a parent restarts with its cursor still at an inner-graph task,
    /// recovery hydrates the persisted child rows into the engine's children
    /// map. `InnerGraphTask` must reattach that child (resume its remaining
    /// steps, preserving cursor/context) instead of spawning a fresh one,
    /// which would create a duplicate child row and potentially replay
    /// completed child work. The engine also reconstructs a `FlowRunner` for
    /// the reattached child. Returns `None` when no matching non-terminal
    /// child row exists — the caller should then `spawn_child_session`.
    async fn attach_existing_child_session(
        &self,
        parent_session_id: &str,
        inner_graph: Arc<Graph>,
    ) -> Result<Option<SessionId>, EngineError>;

    /// Retrieve the context for a session.
    async fn get_context(&self, session_id: &SessionId)
        -> Result<graph_flow::Context, EngineError>;

    /// Retrieve the current task id for a session (authoritative persisted
    /// cursor). `Ok(None)` when no session snapshot exists or the call is
    /// unsupported (in-memory/test engines). Used by `InnerGraphTask` to
    /// name the exact waiting child in the propagated root `WaitRecord`
    /// (round-4 Critical 2 / A4 child cursor).
    async fn get_current_task_id(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<String>, EngineError>;

    /// Returns `true` when a `FlowRunner` exists for the session (started
    /// in-process or reconstructed at boot via `recover_sessions`).
    ///
    /// A recovered session whose runner failed reconstruction (e.g. a user
    /// preset that is not embedded) returns `false` — it stays
    /// tracked-but-not-driven and must not be stepped.
    async fn has_runner(&self, session_id: &SessionId) -> bool;

    /// Recover persisted non-terminal sessions into the in-memory tracker
    /// (WS2 R1 + R6 + A7).
    ///
    /// Called by daemon boot and the daemon-level restart path with the
    /// persisted non-terminal summaries: registers coordinator tokens,
    /// hydrates the complete owned-descendant closure, and reconstructs a
    /// `FlowRunner` per session from the FROZEN source identity (descriptor
    /// hash + referenced templates verified; changed/missing source or
    /// corrupt child identity is non-replayable — the session stays
    /// tracked-but-not-driven). Terminal sessions are skipped. No new IDs
    /// are minted; existing session/child IDs and checkpoints are attached.
    async fn recover_sessions(&self, summaries: Vec<SessionSummary>);

    /// Attach a runner for `session_id` ON DEMAND (A7 rule 4).
    ///
    /// A human-wait run whose runner was not attached at boot gets its
    /// runner rebuilt here on a matching continue — the complete
    /// owned-descendant closure first, then the root runner, using the
    /// exact same frozen-source verification as boot recovery. Returns the
    /// concrete [`EngineError`] reason when reconstruction is impossible
    /// (source missing, hash mismatch, corrupt child identity), so the
    /// caller can surface `reconstruction_unavailable` while the durable
    /// human wait stays preserved.
    ///
    /// # Errors
    /// Returns [`EngineError`] when the run has no durable v1 descriptor,
    /// the frozen source no longer matches, or a persisted child is
    /// corrupt/unsupported — all non-replayable.
    async fn ensure_recovered_runner(&self, session_id: &SessionId) -> Result<(), EngineError>;

    /// Start a session using a loaded preset (outer graph + inner graphs wired).
    async fn start_session_with_preset(
        &self,
        loaded: &crate::preset::LoadedPreset,
    ) -> Result<SessionId, EngineError>;

    /// Start a session using a loaded preset and trusted creator identity.
    async fn start_session_with_preset_for_creator(
        &self,
        loaded: &crate::preset::LoadedPreset,
        creator_id: &str,
    ) -> Result<SessionId, EngineError>;
}

// ---------------------------------------------------------------------------
// EngineSharedState — extracted shared state (WS3 R1)
// ---------------------------------------------------------------------------

/// Shared state extracted from `GraphFlowEngine` for reuse by `EngineProxy` (WS3 R1).
///
/// Eliminates duplication between `GraphFlowEngine` and `EngineProxy` by
/// placing storage, runners, and sessions in a single Arc-wrapped struct.
pub struct EngineSharedState {
    /// Session persistence backend.
    pub storage: Arc<dyn SessionStorage>,
    /// Per-session `FlowRunners` wrapped in Arc (WS2 R3: avoids clone overhead).
    pub runners: Arc<tokio::sync::RwLock<std::collections::HashMap<String, Arc<FlowRunner>>>>,
    /// In-memory bookkeeping of active sessions.
    pub sessions: Arc<tokio::sync::RwLock<Vec<SessionSummary>>>,
    /// Transactional workflow state store (A2) — present when the storage
    /// backend also implements [`WorkflowStateStore`] (SQLite production;
    /// `None` for in-memory test storage). When present, terminal/wait
    /// transitions are persisted authoritatively via `commit_transition`.
    pub workflow_store: Option<Arc<dyn WorkflowStateStore>>,
    /// Child checkpoints for each root session (A2/A4).
    ///
    /// `spawn_child_session` records the child's durable checkpoint here so
    /// `run_step_internal` can pass real `ChildCheckpoint` data to
    /// `commit_transition` — making nested child rows reconstructible after
    /// restart (Important 2). Keyed by root session id.
    pub children: Arc<tokio::sync::RwLock<std::collections::HashMap<String, Vec<ChildCheckpoint>>>>,
    /// Per-run coordinator cancellation tokens (A1) — the production prompt
    /// consumers resolve their run token from this shared map and fail
    /// closed (`CancellationUnavailable`) when none is registered. The
    /// daemon boot constructs the map, wires it via
    /// `GraphFlowEngine::set_prompt_executor`, and every run the engine
    /// admits (start/spawn/recovery) registers a token here.
    pub session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
    /// Production prompt executor (A1) — used by the cancel path to await
    /// bounded owned-Host teardown before persisting terminal `Cancelled`
    /// (C-001). `None` for in-memory/test engines.
    pub prompt_executor: Option<std::sync::Arc<dyn crate::capability::PromptExecutor>>,
}

impl EngineSharedState {
    /// Create empty shared state with the given storage.
    pub fn new(storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            storage,
            runners: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            sessions: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            workflow_store: None,
            children: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            prompt_executor: None,
        }
    }

    /// Create shared state with a transactional workflow store (A2).
    pub fn with_workflow_store(
        storage: Arc<dyn SessionStorage>,
        workflow_store: Arc<dyn WorkflowStateStore>,
    ) -> Self {
        Self {
            storage,
            runners: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            sessions: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            workflow_store: Some(workflow_store),
            children: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            prompt_executor: None,
        }
    }

    /// Register a run's coordinator cancellation token (A1, fail-closed
    /// contract). Idempotent — every admission path (start, spawn, recovery)
    /// calls this so the shared map carries one token per run; a prompt for
    /// an unregistered run fails closed with `CancellationUnavailable`
    /// rather than minting an uncancellable token.
    pub fn register_cancellation(&self, session_id: &SessionId) {
        self.session_cancels
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(session_id.0.clone())
            .or_default();
    }

    /// Recover persisted non-terminal sessions into in-memory tracker (WS2 R1).
    ///
    /// Called on daemon restart to repopulate the session tracker from
    /// persisted sessions with status `running`, `paused`, or `waiting_for_input`.
    /// The recovered sessions are added to the in-memory sessions map but
    /// **not** to the runners map (runners are created lazily when `run_step`
    /// is called on a recovered session).
    pub async fn recover_sessions(&self, summaries: Vec<SessionSummary>) {
        let mut sessions = self.sessions.write().await;
        for summary in summaries {
            // Only add if not already present (idempotent).
            if !sessions.iter().any(|s| s.session_id == summary.session_id) {
                sessions.push(summary);
            }
        }
    }

    /// Hydrate the in-memory children map from persisted child rows for a
    /// recovered parent session (Important 1).
    ///
    /// A restarted parent must know its child checkpoints (with their current
    /// persisted revisions) so its next `commit_transition` submits the
    /// correct child revisions — otherwise the child CAS fails with
    /// `RevisionMismatch`. This reads the persisted child rows via
    /// `WorkflowStateStore::load_children` and rebuilds the `ChildCheckpoint`
    /// entries keyed by the parent session id.
    ///
    /// Any persisted child that is corrupt/unsupported (a failed child row
    /// load, a missing/invalid child session snapshot) is **non-replayable**:
    /// this method propagates the error (A7) so the caller marks the parent
    /// non-replayable rather than reconstructing a runner over an incomplete
    /// children map. An empty result **replaces/clears** any prior map entry
    /// so a stale checkpoint never lingers (Important 1).
    ///
    /// # Errors
    ///
    /// Returns `EngineError` when a child row load fails, a child's stored
    /// snapshot is unreadable, its descriptor identity is tampered, or the
    /// parent's own snapshot cannot be loaded or serialized.
    pub async fn hydrate_children(&self, parent_session_id: &SessionId) -> Result<(), EngineError> {
        let Some(store) = &self.workflow_store else {
            return Ok(());
        };
        // Propagate a load failure (corrupt/unsupported child rows) instead
        // of silently continuing with an empty children map (Important 1).
        let children = match store.load_children(parent_session_id).await {
            Ok(children) => children,
            Err(e) => {
                // Clear any prior (stale) children-map entry for this parent
                // so a checkpoint from before the error never lingers
                // (Minor 1: hydrate_children must not leave a stale entry on
                // a load/child-snapshot error).
                self.children.write().await.remove(&parent_session_id.0);
                return Err(e);
            }
        };
        let parent = self
            .storage
            .get(&parent_session_id.0)
            .await
            .map_err(EngineError::GraphFlow)?
            .ok_or_else(|| EngineError::SessionNotFound(parent_session_id.0.clone()))?;
        // A7 rule 2 (P3 T1 rereview-2/3 P1): every persisted child must still
        // carry the trusted frozen identity. A parseable but tampered child
        // descriptor makes the whole owned closure non-replayable — the root
        // must not be reconstructed over it (and a later reattachment must
        // never fall back to minting a fresh child). Legacy v0 children are
        // left to the shipped conservative path by the validator itself, so a
        // v1 child under a v0 parent is still refused.
        if let Ok(Some(parent_record)) = store.load_run(parent_session_id).await {
            for child in &children {
                if let Err(e) = validate_child_descriptor_identity(
                    parent_session_id,
                    parent_record.descriptor.as_ref(),
                    child,
                ) {
                    // Mirror the load-error discipline: never leave a stale
                    // children-map entry behind a failed hydration (Minor 1).
                    self.children.write().await.remove(&parent_session_id.0);
                    return Err(e);
                }
            }
        }
        let parent_context = match serde_json::to_value(&parent.context) {
            Ok(context) => context,
            Err(e) => {
                self.children.write().await.remove(&parent_session_id.0);
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "serialize recovered parent context '{}': {e}",
                        parent_session_id.0
                    )),
                ));
            }
        };
        let active_terminal_children: Vec<String> =
            crate::resume_rules::context_data(&parent_context)
                .map(|data| {
                    data.iter()
                        .filter(|(key, _)| key.starts_with("_inner_child_session_"))
                        .filter_map(|(_, value)| value.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
        let mut checkpoints = Vec::with_capacity(children.len());
        for child in &children {
            if child.status.is_terminal()
                && !active_terminal_children
                    .iter()
                    .any(|id| id == &child.session_id.0)
            {
                continue;
            }
            // A child whose session snapshot cannot be read from storage is
            // corrupt: propagate rather than silently skip it (Important 1).
            let Ok(Some(child_session)) = self
                .storage
                .get(&child.session_id.0)
                .await
                .map_err(EngineError::GraphFlow)
            else {
                // On child-snapshot error, clear the parent's stale
                // children-map entry before propagating (Minor 1).
                self.children.write().await.remove(&parent_session_id.0);
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "hydrate_children '{}': persisted child '{}' has no readable \
                         session snapshot (non-replayable)",
                        parent_session_id.0, child.session_id.0
                    )),
                ));
            };
            let graph_name = child.descriptor.as_ref().and_then(|d| d.graph_name.clone());
            checkpoints.push(ChildCheckpoint {
                session: child_session,
                status: child.status.clone(),
                state: child.state.clone().unwrap_or_default(),
                state_revision: child.state_revision,
                graph_name,
            });
        }
        // Replace any prior map entry — an empty result must also clear a
        // stale entry so it never lingers as if hydrated (Important 1).
        let mut map = self.children.write().await;
        if checkpoints.is_empty() {
            map.remove(&parent_session_id.0);
        } else {
            map.insert(parent_session_id.0.clone(), checkpoints);
        }
        drop(map); // release children write guard before returning Ok(())
        Ok(())
    }

    /// Authoritative persisted cursor for a session (round-4 Critical 2).
    ///
    /// Reads the current task id from the storage snapshot — the same
    /// source `commit_transition` persists — rather than the in-memory
    /// tracker (which may lag after recovery). `Ok(None)` when no session
    /// row exists yet (or storage errors are treated as absent by callers).
    async fn current_task_id(
        state: &Self,
        session_id: &SessionId,
    ) -> Result<Option<String>, EngineError> {
        match state.storage.get(&session_id.0).await {
            Ok(Some(session)) => Ok(Some(session.current_task_id)),
            Ok(None) => Ok(None),
            Err(e) => Err(EngineError::GraphFlow(e)),
        }
    }

    /// Persist a control-signal transition authoritatively (A2/A5) when a
    /// workflow store is present.
    ///
    /// - `Cancel` → `Cancelled` with `cancel_requested = true`.
    /// - `Pause` → `Paused`.
    /// - `Resume`/`Advance` → `Running`, **fenced against terminal states**
    ///   (Critical 3): a terminal session must never be flipped back to
    ///   `Running` by a resume signal.
    /// - `Continue { wait_id }` → `Running`, **fenced against the exact
    ///   durable wait token** (A4): the run must be waiting with a
    ///   `WaitRecord` whose `wait_id` matches. A stale/consumed/wrong token
    ///   loses the CAS and returns [`EngineError::WaitConflict`] — no
    ///   mutation, no second driver. A nested child wait clears the child's
    ///   durable wait record in the SAME root `commit_transition` (one
    ///   atomic CAS), so a duplicate consumer can never clear the child
    ///   without winning the root.
    ///
    /// Returns the target status on success. When the store is absent
    /// (in-memory test storage) this is a no-op returning the target status.
    ///
    /// **Cancel ordering (C-001):** the durable cancel-intent fence is
    /// persisted FIRST (revision-fenced `commit_transition` with
    /// `cancel_requested = true`, status kept non-terminal), then the run's
    /// coordinator cancellation token is fired, then the prompt executor's
    /// bounded owned-Host teardown is awaited. Terminal `Cancelled` is
    /// persisted only after owned Host work is stopped/reaped;
    /// cleanup-unconfirmed persists `Interrupted` (actionable, never false
    /// successful cancellation).
    #[allow(clippy::too_many_lines, clippy::significant_drop_tightening)] // sequential linear signal→persist→teardown path; closure read-guard spans the BFS await loop
    async fn persist_signal_transition(
        &self,
        session_id: &SessionId,
        signal: &EngineSignal,
    ) -> Result<SessionStatus, EngineError> {
        let target_status = match signal {
            EngineSignal::Pause => SessionStatus::Paused,
            EngineSignal::Cancel => SessionStatus::Cancelled,
            EngineSignal::Resume | EngineSignal::Advance | EngineSignal::Continue { .. } => {
                SessionStatus::Running
            }
        };

        let Some(store) = &self.workflow_store else {
            return Ok(target_status);
        };

        let record = store
            .load_run(session_id)
            .await?
            .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))?;
        let expected_revision = record.state_revision;

        if record.status.is_terminal() {
            // A5 cleanup retry: an `Interrupted` run with a durable cancel
            // intent accepts a further `Cancel` as the bounded, owner-scoped
            // cleanup retry — the run is terminal and never re-driven, but
            // the owned Host sessions may still be unconfirmed and must
            // remain reapeable. All other signals on a terminal run are
            // refused.
            let interrupted_cancel_retry = matches!(signal, EngineSignal::Cancel)
                && record.status == SessionStatus::Interrupted
                && record.state.as_ref().is_some_and(|s| s.cancel_requested);
            if !interrupted_cancel_retry {
                return Err(EngineError::TerminalState(session_id.0.clone()));
            }
        }
        let mut next_state = record.state.unwrap_or_default();
        // A5 cancel-intent fence (Finding 1): once the durable cancel
        // intent is committed, the cancel winner is the ONLY durable
        // control winner. Every other signal — Continue included — is
        // refused with a state conflict and the wait is never consumed:
        // no new work is created while cancellation is in flight.
        if next_state.cancel_requested && !matches!(signal, EngineSignal::Cancel) {
            return Err(EngineError::TerminalState(session_id.0.clone()));
        }
        // Continue is the ONLY non-cancel signal that may act on a waiting
        // run; all other non-cancel signals are fenced against waits and
        // in-flight markers (A4: advance/resume/force-transition cannot
        // bypass a human wait).
        if !matches!(signal, EngineSignal::Cancel | EngineSignal::Continue { .. })
            && (matches!(record.status, SessionStatus::WaitingForInput)
                || next_state.wait.is_some()
                || next_state.step_in_flight.is_some()
                || next_state.in_flight.is_some())
        {
            return Err(EngineError::TerminalState(session_id.0.clone()));
        }

        let root = self
            .storage
            .get(&session_id.0)
            .await
            .map_err(EngineError::GraphFlow)?
            .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))?;

        // A4: authorized continuation of a human wait.
        if let EngineSignal::Continue { wait_id } = signal {
            // The run must be waiting with a durable wait record.
            let Some(wait) = &next_state.wait else {
                return Err(EngineError::WaitConflict {
                    session_id: session_id.0.clone(),
                    status: record.status.clone(),
                    current_wait_id: None,
                });
            };
            if wait.wait_id != *wait_id {
                return Err(EngineError::WaitConflict {
                    session_id: session_id.0.clone(),
                    status: record.status.clone(),
                    current_wait_id: Some(wait.wait_id.clone()),
                });
            }

            // Nested child wait: the child's durable wait record must also
            // be cleared so the child can be re-stepped (A4 "propagate one
            // authorized token to the exact waiting descendant"). The child
            // checkpoint is carried in the SAME root `commit_transition`
            // so the root+child wait clear is one atomic CAS — a duplicate
            // consumer loses the root CAS and never clears the child.
            let mut children: Vec<ChildCheckpoint> = Vec::new();
            if let Some(child_session_id) = &wait.child_session_id {
                let child_sid = SessionId(child_session_id.clone());
                let child_record = store
                    .load_run(&child_sid)
                    .await?
                    .ok_or_else(|| EngineError::SessionNotFound(child_session_id.clone()))?;
                let child_session = self
                    .storage
                    .get(child_session_id)
                    .await
                    .map_err(EngineError::GraphFlow)?
                    .ok_or_else(|| EngineError::SessionNotFound(child_session_id.clone()))?;
                let mut child_state = child_record.state.unwrap_or_default();
                child_state.wait = None;
                let graph_name = child_record
                    .descriptor
                    .as_ref()
                    .and_then(|d| d.graph_name.clone());
                children.push(ChildCheckpoint {
                    session: child_session,
                    status: SessionStatus::Running,
                    state: child_state,
                    state_revision: child_record.state_revision,
                    graph_name,
                });
            }

            // Clear the root wait and make the run runnable. The child
            // checkpoint (when nested) is committed atomically with the
            // root under the root revision CAS.
            next_state.wait = None;
            let checkpoint = RunCheckpoint {
                root: &root,
                children: &children,
            };
            store
                .commit_transition(
                    session_id,
                    expected_revision,
                    checkpoint,
                    SessionStatus::Running,
                    &next_state,
                )
                .await?;

            // The root commit advanced each carried child's persisted
            // revision by one; synchronize the in-memory children map so
            // the parent's next `commit_transition` submits the correct
            // child CAS anchor (N-7).
            if !children.is_empty() {
                let mut child_map = self.children.write().await;
                if let Some(entries) = child_map.get_mut(&session_id.0) {
                    for child in entries.iter_mut() {
                        if children.iter().any(|c| c.session.id == child.session.id) {
                            child.state_revision = child.state_revision.saturating_add(1);
                        }
                    }
                }
            }
            return Ok(SessionStatus::Running);
        }

        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };

        if matches!(target_status, SessionStatus::Cancelled) {
            // A5 cleanup retry (Finding 3): an `Interrupted` run with a
            // durable cancel intent re-enters the cancel path as the
            // bounded, owner-scoped cleanup retry. The cancel-intent fence
            // is already durable — skip phase 1 and go straight to token
            // fire + bounded Host teardown. The run is terminal, so no step
            // is ever re-driven (the drive loop and `ensure_driving` refuse
            // `Interrupted`).
            let interrupted_retry = record.status == SessionStatus::Interrupted;

            // C-001 phase 1: durable cancel-intent fence. Persist
            // `cancel_requested = true` while keeping the status
            // non-terminal — the run is not yet confirmed cancelled. The
            // fence is a revision CAS: a concurrent `mark_step_in_flight`
            // (or any other transition) that wins the CAS is reloaded and
            // the fence is retried against the new revision while the run
            // is still cancellable (Finding 1) — the cancel intent is
            // persisted BEFORE the token fires, so active Host work is
            // always reached. A terminal observation is a deliberate
            // conflict, never silent success.
            if !interrupted_retry {
                let mut fence_revision = expected_revision;
                let mut fence_status = record.status.clone();
                let mut fence_state = next_state.clone();
                fence_state.cancel_requested = true;
                let mut attempts: u32 = 0;
                loop {
                    // Re-read the root snapshot each attempt so a retry
                    // never clobbers the winner's position/context with a
                    // stale checkpoint.
                    let fence_root = self
                        .storage
                        .get(&session_id.0)
                        .await
                        .map_err(EngineError::GraphFlow)?
                        .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))?;
                    let fence_checkpoint = RunCheckpoint {
                        root: &fence_root,
                        children: &[],
                    };
                    match store
                        .commit_transition(
                            session_id,
                            fence_revision,
                            fence_checkpoint,
                            fence_status.clone(),
                            &fence_state,
                        )
                        .await
                    {
                        Ok(_) => break,
                        Err(EngineError::RevisionMismatch { .. }) => {
                            attempts += 1;
                            if attempts >= CANCEL_FENCE_RETRY_BOUND {
                                // The run is still cancellable but the fence
                                // could not win within the bound — surface
                                // the CAS loss so the caller projects the
                                // exact public conflict after reloading.
                                return Err(EngineError::RevisionMismatch {
                                    session_id: session_id.0.clone(),
                                    expected: fence_revision,
                                    found: fence_revision,
                                });
                            }
                            // Reload the authoritative row and re-fence
                            // against the new revision while the run is
                            // still cancellable.
                            let Some(current) = store.load_run(session_id).await? else {
                                return Err(EngineError::GraphFlow(
                                    graph_flow::GraphError::StorageError(format!(
                                        "run '{}' disappeared during cancel fence",
                                        session_id.0
                                    )),
                                ));
                            };
                            if current.status.is_terminal() {
                                // The run reached a terminal state while the
                                // cancel fence was in flight — a deliberate
                                // conflict, never silent success.
                                return Err(EngineError::TerminalState(session_id.0.clone()));
                            }
                            fence_revision = current.state_revision;
                            fence_status = current.status.clone();
                            fence_state = current.state.unwrap_or_default();
                            fence_state.cancel_requested = true;
                        }
                        Err(e) => return Err(e),
                    }
                }
            }

            // Fire the run's coordinator cancellation token so the
            // in-flight prompt operation observes cancellation and begins
            // its bounded Host cleanup. The root token covers root-level
            // prompt nodes; nested inner-graph prompt nodes run in child
            // sessions with their OWN registered tokens (A4 nested child
            // propagation) — fire those too so a cancel reaches the exact
            // waiting descendant.
            //
            // Finding 2 (round 3): the owned-descendant closure is built
            // RECURSIVELY from the in-memory parent relationship — a child
            // can itself own nested children (`InnerGraphTask` spawns
            // through the same engine path), so the closure walks the
            // children map breadth-first from the root and covers
            // children, grandchildren, and deeper. Every descendant token
            // is fired and every descendant run is finalized under the
            // per-run ownership lock; failed entries are retained for the
            // owner-scoped retry, and descendant tokens are reclaimed only
            // at their confirmed cleanup boundary (after the whole closure
            // is confirmed).
            //
            // P1 (rereview-4): the closure is reconciled against the
            // AUTHORITATIVE persisted parent relation. The in-memory
            // children map is the admission-time snapshot; a child admitted
            // by an in-flight `InnerGraphTask` after the fence (or a child
            // whose map entry was dropped) is still a durable owned
            // descendant that must be fired and finalized. `load_children`
            // reads the persisted `parent_session_id` rows, so the closure
            // covers every durable descendant — mapped or not — and no
            // child can be admitted after the fence and escape the closure.
            let descendant_ids: Vec<String> = {
                let children = self.children.read().await;
                let mut ids: Vec<String> = Vec::new();
                let mut queue: std::collections::VecDeque<String> =
                    std::collections::VecDeque::new();
                queue.push_back(session_id.0.clone());
                while let Some(parent) = queue.pop_front() {
                    if let Some(entries) = children.get(&parent) {
                        for child in entries {
                            let child_id = child.session.id.clone();
                            if child_id != session_id.0 {
                                ids.push(child_id.clone());
                                queue.push_back(child_id);
                            }
                        }
                    }
                    // Reconcile the persisted parent relation: every durable
                    // child row of this parent joins the closure (and is
                    // enqueued for its own persisted descendants), even when
                    // the in-memory map entry is absent or stale. A
                    // load failure is non-replayable (A7) — propagate rather
                    // than settle a cancellation over an incomplete closure.
                    if let Some(store) = &self.workflow_store {
                        let persisted = store.load_children(&SessionId(parent.clone())).await?;
                        for child in persisted {
                            let child_id = child.session_id.0.clone();
                            if child_id != session_id.0 && !ids.contains(&child_id) {
                                ids.push(child_id.clone());
                                queue.push_back(child_id);
                            }
                        }
                    }
                }
                ids
            };
            {
                let cancels = self
                    .session_cancels
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(token) = cancels.get(&session_id.0).cloned() {
                    token.cancel();
                }
                for child_id in &descendant_ids {
                    if let Some(token) = cancels.get(child_id).cloned() {
                        token.cancel();
                    }
                }
            }

            // Await the prompt executor's bounded owned-Host teardown for
            // the root AND every owned descendant run (Finding 3): Host
            // sessions are keyed by the request run id, and nested prompt
            // tasks pass their own child session id — a child parked at a
            // human wait has a cached Host session with no active operation
            // to observe the fired token, so the root finalize alone never
            // visits it. All owned cleanup must be confirmed before the run
            // settles `Cancelled`; failed entries are retained for the
            // owner-scoped retry.
            if let Some(executor) = &self.prompt_executor {
                // Prefer the root cleanup error; if the root confirms, keep
                // the first descendant-finalize error (if any).
                let root_cleanup_err = executor.finalize_run(&session_id.0).await.err();
                let cleanup_error = if let Some(e) = root_cleanup_err {
                    Some(e)
                } else {
                    let mut child_err = None;
                    for child_id in &descendant_ids {
                        if let Err(e) = executor.finalize_run(child_id).await {
                            child_err = Some(e);
                            break;
                        }
                    }
                    child_err
                };
                if let Some(e) = cleanup_error {
                    // Cleanup unconfirmed: persist Interrupted (actionable,
                    // never false successful cancellation) and surface the
                    // error. The run stays visibly interrupted. The
                    // transition is revision-fenced and bounded like the
                    // other cancellation transitions (Finding 2): a
                    // concurrent transition that wins the CAS is reloaded
                    // and re-committed against the new revision; the write
                    // result is never discarded, and the in-memory summary
                    // is updated only after the durable write succeeds.
                    let reason =
                        format!("cancel cleanup unconfirmed for run '{}': {e}", session_id.0);
                    let mut interrupted_attempts: u32 = 0;
                    loop {
                        let Some(current) = store.load_run(session_id).await? else {
                            return Err(EngineError::GraphFlow(
                                graph_flow::GraphError::StorageError(format!(
                                    "run '{}' disappeared during cancel cleanup",
                                    session_id.0
                                )),
                            ));
                        };
                        let current_revision = current.state_revision;
                        let mut interrupted_state = current.state.unwrap_or_default();
                        interrupted_state.cancel_requested = true;
                        interrupted_state.in_flight = None;
                        interrupted_state.failure = Some(RunFailure {
                            code: "cancel_cleanup_unconfirmed".to_string(),
                            message: reason.clone(),
                        });
                        // Re-read the root snapshot so the Interrupted
                        // checkpoint never clobbers a position/context a
                        // concurrent transition advanced.
                        let interrupted_root = self
                            .storage
                            .get(&session_id.0)
                            .await
                            .map_err(EngineError::GraphFlow)?
                            .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))?;
                        let interrupted_checkpoint = RunCheckpoint {
                            root: &interrupted_root,
                            children: &[],
                        };
                        match store
                            .commit_transition(
                                session_id,
                                current_revision,
                                interrupted_checkpoint,
                                SessionStatus::Interrupted,
                                &interrupted_state,
                            )
                            .await
                        {
                            Ok(_) => break,
                            Err(EngineError::RevisionMismatch { .. }) => {
                                interrupted_attempts += 1;
                                if interrupted_attempts >= CANCEL_FENCE_RETRY_BOUND {
                                    return Err(EngineError::RevisionMismatch {
                                        session_id: session_id.0.clone(),
                                        expected: current_revision,
                                        found: current_revision,
                                    });
                                }
                                // Reload and re-commit against the new
                                // revision.
                            }
                            Err(err) => return Err(err),
                        }
                    }
                    // Durable Interrupted confirmed: only now update the
                    // in-memory summary (Finding 2 — never before durable
                    // success).
                    if let Some(s) = self
                        .sessions
                        .write()
                        .await
                        .iter_mut()
                        .find(|s| s.session_id == *session_id)
                    {
                        s.status = SessionStatus::Interrupted;
                    }
                    return Err(EngineError::GraphFlow(
                        graph_flow::GraphError::TaskExecutionFailed(reason),
                    ));
                }
            }

            // C-001 phase 2: confirmed cleanup — persist terminal Cancelled.
            // `settle_cancelled` is the A5 settlement fence: it admits the
            // `Interrupted` cleanup-retry shape (and non-terminal rows) but
            // refuses every other terminal status, so a retry can never
            // overwrite a newer terminal outcome. The settlement is itself
            // revision-fenced and retried on a CAS loss while the run is
            // still cancellable — a concurrent cancel (e.g. the drive loop's
            // best-effort failure flip) that wins the fence between our
            // reload and commit must not turn this confirmed cleanup into a
            // spurious conflict; both cancels settle to `cancelled`.
            let mut settle_attempts: u32 = 0;
            loop {
                let Some(current) = store.load_run(session_id).await? else {
                    return Err(EngineError::GraphFlow(
                        graph_flow::GraphError::StorageError(format!(
                            "run '{}' disappeared before cancelled settlement",
                            session_id.0
                        )),
                    ));
                };
                if current.status == SessionStatus::Cancelled {
                    // A concurrent cancel already settled the run — the
                    // confirmed cleanup outcome is identical.
                    return Ok(SessionStatus::Cancelled);
                }
                // The `Interrupted` cleanup-retry shape (durable cancel
                // intent) is the A5 settlement target — it must pass through
                // to `settle_cancelled`. Every other terminal status is a
                // deliberate conflict.
                let interrupted_retry_shape = current.status == SessionStatus::Interrupted
                    && current.state.as_ref().is_some_and(|s| s.cancel_requested);
                if current.status.is_terminal() && !interrupted_retry_shape {
                    return Err(EngineError::TerminalState(session_id.0.clone()));
                }
                let current_revision = current.state_revision;
                let mut cancelled_state = current.state.unwrap_or_default();
                cancelled_state.cancel_requested = true;
                cancelled_state.in_flight = None;
                // Finding 1 (round 3): every settlement attempt reloads the
                // authoritative root snapshot together with the current run
                // record. The `root` loaded before the phase-1 fence may
                // predate a concurrent control winner (e.g. a Continue that
                // consumed the wait and advanced the position/context); the
                // terminal checkpoint must carry the NEWEST durable root so
                // the Cancelled row never clobbers the winner's position.
                let settle_root = self
                    .storage
                    .get(&session_id.0)
                    .await
                    .map_err(EngineError::GraphFlow)?
                    .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))?;
                let settle_checkpoint = RunCheckpoint {
                    root: &settle_root,
                    children: &[],
                };
                match store
                    .settle_cancelled(
                        session_id,
                        current_revision,
                        settle_checkpoint,
                        &cancelled_state,
                    )
                    .await
                {
                    Ok(_) => break,
                    Err(EngineError::RevisionMismatch { .. }) => {
                        settle_attempts += 1;
                        if settle_attempts >= CANCEL_FENCE_RETRY_BOUND {
                            return Err(EngineError::RevisionMismatch {
                                session_id: session_id.0.clone(),
                                expected: current_revision,
                                found: current_revision,
                            });
                        }
                        // Reload and re-settle against the new revision.
                    }
                    Err(e) => return Err(e),
                }
            }
            // M-004: reclaim the run's coordinator cancellation token at
            // the same confirmed run-terminal ownership boundary. Finding 2
            // (round 3): every owned descendant token is reclaimed at the
            // same confirmed-cleanup boundary — the whole closure was
            // finalized successfully above, so no descendant can still be
            // driving work.
            {
                let mut cancels = self
                    .session_cancels
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                cancels.remove(&session_id.0);
                for child_id in &descendant_ids {
                    cancels.remove(child_id);
                }
            }
            return Ok(SessionStatus::Cancelled);
        }

        // Non-cancel signals: single commit as before.
        store
            .commit_transition(
                session_id,
                expected_revision,
                checkpoint,
                target_status.clone(),
                &next_state,
            )
            .await?;
        // A signal that advanced a CHILD's revision must synchronize the
        // parent's children map (N-7): the inner poller resumes a paused
        // child via `EngineSignal::Resume`, which commits +1 revision here;
        // without the sync the parent's next `commit_transition` submits a
        // stale child CAS anchor and the run is misclassified Interrupted.
        self.sync_child_checkpoint_after_step(session_id).await?;
        Ok(target_status)
    }

    /// Synchronize a child's persisted checkpoint back into the parent's
    /// children map after the child has been stepped (Important 1).
    ///
    /// `InnerGraphTask` polls a child via `engine.run_step(&child_sid)`, which
    /// advances the child's persisted `state_revision`/status/state. Without
    /// this sync, the parent's children map would keep the original revision
    /// and the parent's later `commit_transition` would fail the child CAS
    /// with `RevisionMismatch`. This method reads the child's current
    /// persisted record and updates the parent's map entry so the parent
    /// submits the correct child revision.
    ///
    /// Any storage/load failure here is propagated (A7): a child whose
    /// persisted record cannot be read or whose session snapshot is missing
    /// is corrupt, and the caller must surface it rather than let the parent
    /// continue with a stale/missing child checkpoint.
    async fn sync_child_checkpoint_after_step(
        &self,
        session_id: &SessionId,
    ) -> Result<(), EngineError> {
        // Only children carry a `_parent_session_id` in context.
        let parent_id: Option<String> = match self.storage.get(&session_id.0).await {
            Ok(Some(session)) => session.context.get("_parent_session_id").await,
            Ok(None) => None,
            Err(e) => return Err(EngineError::GraphFlow(e)),
        };
        let Some(parent_id) = parent_id else {
            return Ok(());
        };
        let Some(store) = &self.workflow_store else {
            return Ok(());
        };
        // Load the child's current persisted record (revision/status/state).
        // A missing/erroneous record is non-replayable — propagate (A7).
        let child_record = store.load_run(session_id).await?;
        let Some(child_record) = child_record else {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "sync_child_checkpoint_after_step '{}': child has no persisted run record \
                     (non-replayable)",
                    session_id.0
                )),
            ));
        };
        // Rebuild the child checkpoint from the persisted record.
        let child_session = self
            .storage
            .get(&session_id.0)
            .await
            .map_err(EngineError::GraphFlow)?
            .ok_or_else(|| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "sync_child_checkpoint_after_step '{}': child has no session row \
                     (non-replayable)",
                    session_id.0
                )))
            })?;
        let graph_name = child_record
            .descriptor
            .as_ref()
            .and_then(|d| d.graph_name.clone());
        let child_checkpoint = ChildCheckpoint {
            session: child_session,
            status: child_record.status.clone(),
            state: child_record.state.clone().unwrap_or_default(),
            state_revision: child_record.state_revision,
            graph_name,
        };
        // Update the parent's children map entry for this child.
        let mut children = self.children.write().await;
        let entry = children.entry(parent_id).or_default();
        if let Some(existing) = entry.iter_mut().find(|c| c.session.id == session_id.0) {
            *existing = child_checkpoint;
        } else {
            entry.push(child_checkpoint);
        }
        drop(children); // release children write guard before returning
        Ok(())
    }

    /// Run a single step for a session, updating status after execution.
    ///
    /// Common logic shared between `GraphFlowEngine` and `EngineProxy`.
    ///
    /// When a transactional [`WorkflowStateStore`] is present (SQLite
    /// production), terminal/wait transitions are persisted authoritatively
    /// via `commit_transition` under a revision CAS — the DB `status` column
    /// becomes the authoritative value, never a second context-key SSOT.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the engine has no graph loaded, the step cannot be resolved,
    /// or capability execution fails.
    #[allow(clippy::too_many_lines)] // sequential step→persist→wait pipeline; splitting obscures the single execution path
    pub async fn run_step_internal(
        &self,
        session_id: &SessionId,
    ) -> Result<graph_flow::ExecutionResult, EngineError> {
        // Get Arc<FlowRunner> without cloning (WS2 R3).
        let runner = {
            let runners = self.runners.read().await;
            runners
                .get(&session_id.0)
                .cloned()
                .ok_or(EngineError::NoGraphLoaded)?
        };

        // Capture the current revision before the step so the transition CAS
        // is anchored to the pre-step state (A2).
        let expected_revision = if let Some(store) = &self.workflow_store {
            store
                .load_run(session_id)
                .await?
                .map_or(0, |r| r.state_revision)
        } else {
            0
        };
        let mut transition_revision = expected_revision;

        // Capture the pre-step root session (position/context) so a failed
        // `commit_transition` (e.g. stale-child CAS) can restore the root to
        // its pre-step position — `FlowRunner.run` saves position/context
        // before `commit_transition`, so without this the root would be left
        // partially advanced on failure (Important 2).
        let pre_step_root = self
            .storage
            .get(&session_id.0)
            .await
            .map_err(EngineError::GraphFlow)?;

        // Persist the current-position safety intent BEFORE any external
        // effect in the step. The marker write is fenced to step-able status
        // plus the pre-step revision and atomically advances the revision.
        // A signal loaded before the marker then loses its CAS; a signal
        // loaded afterward sees the in-flight state and is refused unless it
        // is cancellation. The subsequent transition anchors to the marker's
        // revision. A FlowRunner/storage save failure after this point cannot
        // leave an unmarked running row.
        if let Some(store) = &self.workflow_store {
            if let Some(pre) = &pre_step_root {
                transition_revision = expected_revision.checked_add(1).ok_or_else(|| {
                    EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                        "run '{}': state revision overflow",
                        session_id.0
                    )))
                })?;
                let in_flight_state = RunStateV1 {
                    step_in_flight: Some(pre.current_task_id.clone()),
                    ..RunStateV1::default()
                };
                let in_flight_checkpoint = RunCheckpoint {
                    root: pre,
                    children: &[],
                };
                store
                    .mark_step_in_flight(
                        session_id,
                        expected_revision,
                        in_flight_checkpoint,
                        &in_flight_state,
                    )
                    .await?;
            }
        }

        // Execute one step using the Arc<FlowRunner>.
        let result = runner.run(&session_id.0).await?;

        // Map graph-flow ExecutionStatus → SessionStatus. A `WaitingForInput`
        // outcome is EITHER a scheduler converge/merge gate park (live join
        // keys in the post-step context — persisted `paused`, tokenless per
        // A2) OR a genuine manual/nested human wait (no live join keys —
        // persisted `waiting_for_input` with a fresh A4 token). Both classes
        // surface to the caller as `StepOutcome::WaitingForInput` (the drive
        // loop stops at the same boundary); only the durable shape differs.
        let mut status = match &result.status {
            ExecutionStatus::Completed => SessionStatus::Completed,
            ExecutionStatus::Error(_) => SessionStatus::Failed,
            ExecutionStatus::WaitingForInput => SessionStatus::WaitingForInput,
            ExecutionStatus::Paused { .. } => SessionStatus::Paused,
        };

        // Persist the authoritative status transition (A2) when a workflow
        // store is present. Terminal/wait/paused statuses must survive
        // restart and not be disguised as `running` (Critical 1: paused is
        // authoritative; Critical 4: a human wait carries a durable token).
        if let Some(store) = &self.workflow_store {
            if status.is_terminal()
                || matches!(
                    status,
                    SessionStatus::WaitingForInput | SessionStatus::Paused
                )
            {
                // Re-fetch the root session (the step already saved position).
                let root = self
                    .storage
                    .get(&session_id.0)
                    .await
                    .map_err(EngineError::GraphFlow)?
                    .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))?;

                // A `WaitingForInput` outcome at a scheduler converge/merge
                // join persists as `paused` with NO human-wait token (A2:
                // "Distinguish persisted scheduler joins (paused + existing
                // join keys) from human waits (waiting_for_input + A4 wait
                // record)"). Classification is authoritative to the CURRENT
                // gate: the gated `StateCompositeTask` writes a state-scoped
                // marker `_gate_park_{state_id}` at the exact merge/converge
                // park and nulls it on gate success/leave/timeout (fix round
                // 4, Critical 1). Historical/broad keys (`_merge_*`,
                // `_converge_arrivals_*`, `_join_wait_start_*`) are written
                // by labeled/conditional ROUTING for the routed target —
                // including plain manual-wait states — so they are not
                // authoritative park evidence and must never demote a
                // genuine manual/nested wait to `paused`. Re-classifying
                // only on the current task's live gate marker keeps
                // scheduler joins tokenless (rule 4 cannot later
                // misclassify a park as a human wait) while manual waits
                // reached through labeled/conditional paths keep their
                // fresh retained A4 token.
                if matches!(status, SessionStatus::WaitingForInput)
                    && serde_json::to_value(&root.context)
                        .ok()
                        .as_ref()
                        .and_then(crate::resume_rules::context_data)
                        .is_some_and(|data| {
                            crate::resume_rules::gate_park_live(data, &root.current_task_id)
                        })
                {
                    status = SessionStatus::Paused;
                }

                let next_state =
                    build_step_state(&result, &status, &root.current_task_id, &root.context);
                // Pass the real child checkpoints recorded by
                // `spawn_child_session` so nested child rows are persisted
                // atomically and reconstructible after restart (Important 2).
                let children: Vec<ChildCheckpoint> = self
                    .children
                    .read()
                    .await
                    .get(&session_id.0)
                    .cloned()
                    .unwrap_or_default();
                let parent_advanced = pre_step_root
                    .as_ref()
                    .is_some_and(|pre| pre.current_task_id != root.current_task_id);
                if parent_advanced && children.iter().any(|child| child.status.is_terminal()) {
                    if let Some(pre) = &pre_step_root {
                        root.context
                            .set(
                                format!("_inner_child_session_{}", pre.current_task_id),
                                serde_json::Value::Null,
                            )
                            .await;
                    }
                }
                let checkpoint = RunCheckpoint {
                    root: &root,
                    children: &children,
                };
                // Capture whether the step performed an external effect BEFORE
                // clearing the marker, so a failed commit can still classify as
                // Interrupted (Important 3). The marker is then removed from the
                // context that `commit_transition` persists so it reflects
                // current-step intent only, not history (Minor 2).
                let had_effect = step_had_external_effect(&root).await;
                if had_effect {
                    clear_step_effect_marker(&root).await;
                }
                let commit_result = store
                    .commit_transition(
                        session_id,
                        transition_revision,
                        checkpoint,
                        status.clone(),
                        &next_state,
                    )
                    .await;
                match commit_result {
                    Ok(_) => {
                        // The parent's `commit_transition` persisted each
                        // child checkpoint with `state_revision + 1` (the
                        // store's ON CONFLICT bump). Synchronize the
                        // in-memory children map to the persisted revisions
                        // for exactly the children THIS commit carried —
                        // otherwise the stale pre-commit revision fails the
                        // child CAS on the parent's next step and the run is
                        // misclassified Interrupted (N-7: multi-step inner
                        // graphs after a child step).
                        {
                            let mut child_map = self.children.write().await;
                            if let Some(entries) = child_map.get_mut(&session_id.0) {
                                for child in entries.iter_mut() {
                                    if children.iter().any(|c| c.session.id == child.session.id) {
                                        child.state_revision =
                                            child.state_revision.saturating_add(1);
                                    }
                                }
                            }
                        }
                        if parent_advanced
                            && children.iter().any(|child| child.status.is_terminal())
                        {
                            let mut child_map = self.children.write().await;
                            if let Some(entries) = child_map.get_mut(&session_id.0) {
                                entries.retain(|child| !child.status.is_terminal());
                                if entries.is_empty() {
                                    child_map.remove(&session_id.0);
                                }
                            }
                        }

                        // I-001 / M-004: a confirmed run-terminal transition
                        // (Completed/Failed) finalizes every `(run, role)`
                        // Host session and reclaims the run's coordinator
                        // cancellation token. The cancel path
                        // (`persist_signal_transition`) performs the same
                        // finalization before persisting terminal
                        // `Cancelled`; cleanup-unconfirmed stays
                        // non-terminal/actionable (never finalized here).
                        //
                        // I-002: the token is reclaimed ONLY when
                        // finalization is confirmed. On unconfirmed cleanup
                        // the token and the executor's session entries are
                        // kept so a later retry can still reap the owned
                        // sessions; the run is already terminal, so no new
                        // admission can occur, but the coordinator token
                        // must remain cancellable for the retry path.
                        if status.is_terminal() {
                            let finalized = match &self.prompt_executor {
                                Some(executor) => {
                                    match executor.finalize_run(&session_id.0).await {
                                        Ok(()) => true,
                                        Err(e) => {
                                            tracing::warn!(
                                                session_id = %session_id.0,
                                                error = %e,
                                                "run-terminal Host session finalization unconfirmed; keeping cancellation token for retry"
                                            );
                                            false
                                        }
                                    }
                                }
                                None => true,
                            };
                            if finalized {
                                self.session_cancels
                                    .write()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                                    .remove(&session_id.0);
                            }
                        }
                    }
                    Err(commit_err) => {
                        if had_effect {
                            // A failed post-effect commit must NOT blindly rewind
                            // to a seemingly safe boundary — a retry could execute
                            // the external effect again. Persist an
                            // interrupted/uncertain disposition instead (A2/A7):
                            // the run becomes terminal `interrupted`, never
                            // auto-replayed. The interrupted write is itself
                            // revision-fenced (F2): if a concurrent transition
                            // already won, it is left as the truthful state and we
                            // surface the original error.
                            let interrupted_state = RunStateV1 {
                                step_in_flight: Some(root.current_task_id.clone()),
                                ..RunStateV1::default()
                            };
                            // Persist the root interruption WITHOUT child checkpoints:
                            // the child rows remain as-is and the interrupted root is
                            // terminal/non-replayable. Requiring a child CAS here would
                            // block the interruption write on the same stale child that
                            // caused the original failure, leaving the run non-terminal
                            // and unknowingly replayable — exactly what must not happen
                            // (Important 3).
                            let interrupted_checkpoint = RunCheckpoint {
                                root: &root,
                                children: &[],
                            };
                            let interrupted = store
                                .commit_transition(
                                    session_id,
                                    transition_revision,
                                    interrupted_checkpoint,
                                    SessionStatus::Interrupted,
                                    &interrupted_state,
                                )
                                .await;

                            // Update in-memory status to reflect the interrupted
                            // disposition when the interrupted CAS succeeded.
                            if interrupted.is_ok() {
                                if let Some(s) = self
                                    .sessions
                                    .write()
                                    .await
                                    .iter_mut()
                                    .find(|s| s.session_id == *session_id)
                                {
                                    s.status = SessionStatus::Interrupted;
                                }
                            }
                            return Err(commit_err);
                        }

                        // Deterministic step (no external effect): restore the
                        // pre-step root via ONE atomic revision-fenced storage
                        // operation (Important 3) — never a separate check-then-
                        // save. A concurrent successful root transition (revision
                        // advanced) is never overwritten by the older pre-step
                        // position, and the restore clears the in-flight markers
                        // the pre-step mark wrote (a safe boundary).
                        if let Some(pre) = &pre_step_root {
                            let _ = store
                                .restore_pre_step(session_id, transition_revision, pre)
                                .await;
                        }
                        return Err(commit_err);
                    }
                }
            }
        }

        // If this session is a child, synchronize its persisted checkpoint
        // back into the parent's children map so the parent's next
        // `commit_transition` submits the correct child revision (Important 1).
        // This runs AFTER the child's own `commit_transition` (above) so the
        // parent's map reflects the child's post-transition revision.
        // A storage/load failure here is propagated (A7): a child whose
        // persisted record cannot be read is non-replayable and must not
        // let the parent continue with a stale/missing checkpoint.
        self.sync_child_checkpoint_after_step(session_id).await?;

        // Update in-memory status.
        if let Some(s) = self
            .sessions
            .write()
            .await
            .iter_mut()
            .find(|s| s.session_id == *session_id)
        {
            s.status = status;
        }

        Ok(result)
    }

    /// Spawn a child session (inner graph) with durable v1 identity (A2/A4).
    ///
    /// When a workflow store is present, the child is created as a v1 run
    /// that inherits the trusted root descriptor (creator, preset, source,
    /// workspace, bindings) and names its parent session and inner graph —
    /// never a bare `SessionStorage::save` with empty identity (Important 2).
    /// The child checkpoint is recorded so `run_step_internal` can persist
    /// nested child rows atomically under the root transition.
    ///
    /// # Errors
    ///
    /// Returns `EngineError` if the child session cannot be persisted, the
    /// inner graph cannot be resolved, or a storage write fails.
    #[allow(clippy::too_many_lines)] // sequential admit→persist→wire path; splitting obscures the single admission flow
    pub async fn spawn_child_session_internal(
        &self,
        params: ChildSessionParams,
    ) -> Result<SessionId, EngineError> {
        // Collision-resistant child id (Minor 1): two child admissions in
        // the same millisecond must not collide with `start_run`'s
        // existing-row rejection. A UUID is unique per admission.
        let child_session_id = format!(
            "{}:child:{}",
            params.parent_session_id,
            uuid::Uuid::new_v4()
        );
        let start_task_id = params.inner_graph.start_task_id().unwrap_or_default();
        let mut session_mut =
            graph_flow::Session::new_from_task(child_session_id.clone(), &start_task_id);
        session_mut.context = params.initial_context;
        // Seed the trusted child run identity into the child context BEFORE
        // any inner node executes: `InnerGraphNodeTask::resolve_session_id`
        // reads `_session_id` to route the prompt through the durable child
        // descriptor. Without this, un-bound inner `acp_prompt` nodes fell
        // back to `"default"` and the Host executor refused/looked up the
        // wrong run (Important: nested graph prompts lose the child durable
        // identity).
        session_mut
            .context
            .set("_session_id", child_session_id.clone())
            .await;
        // Record the parent so `run_step_internal` can sync this child's
        // checkpoint back to the parent's children map after each step
        // (Important 1: child revisions must be synchronized).
        session_mut
            .context
            .set("_parent_session_id", params.parent_session_id.clone())
            .await;

        // When a workflow store is present, create a v1 child run that
        // inherits the trusted root descriptor (A2/A4). The child's
        // `graph_name` is the inner graph's id.
        if let Some(store) = &self.workflow_store {
            let root_record = store
                .load_run(&SessionId(params.parent_session_id.clone()))
                .await?
                .ok_or_else(|| EngineError::SessionNotFound(params.parent_session_id.clone()))?;
            let root_descriptor = root_record.descriptor.ok_or_else(|| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "spawn_child_session '{}': parent has no v1 descriptor; \
                     cannot inherit trusted child identity",
                    params.parent_session_id
                )))
            })?;
            let child_descriptor = RunDescriptorV1 {
                creator_id: root_descriptor.creator_id.clone(),
                work_id: root_descriptor.work_id.clone(),
                workspace_root: root_descriptor.workspace_root.clone(),
                preset_id: root_descriptor.preset_id.clone(),
                preset_version: root_descriptor.preset_version,
                source: root_descriptor.source.clone(),
                input: root_descriptor.input.clone(),
                agent_bindings: root_descriptor.agent_bindings.clone(),
                parent_session_id: Some(SessionId(params.parent_session_id.clone())),
                graph_name: Some(params.inner_graph.id.clone()),
            };
            let checkpoint = RunCheckpoint {
                root: &session_mut,
                children: &[],
            };
            store
                .start_run(
                    &SessionId(child_session_id.clone()),
                    &child_descriptor,
                    checkpoint,
                    &RunStateV1::default(),
                )
                .await?;
        } else {
            self.storage.save(session_mut.clone()).await?;
        }

        // Register the child run's coordinator cancellation token (A1): the
        // child's `acp_prompt` nodes resolve their token from the shared
        // per-run map and fail closed when none is registered.
        self.register_cancellation(&SessionId(child_session_id.clone()));

        // Record the child checkpoint so the root transition persists the
        // nested child row atomically (Important 2). The CAS anchor is the
        // child's actual persisted revision: a v1 child created via
        // `start_run` is at revision 1; a bare-save child (no store) is at 0.
        let child_revision = u64::from(self.workflow_store.is_some());
        let child_checkpoint = ChildCheckpoint {
            session: session_mut.clone(),
            status: SessionStatus::Running,
            state: RunStateV1::default(),
            state_revision: child_revision,
            graph_name: Some(params.inner_graph.id.clone()),
        };

        // P1 (rereview-4): child admission is cancellation-aware and
        // revision-linearized with the parent. The child row is now durable,
        // but an in-flight `InnerGraphTask` can race the root cancel fence:
        // the fence may commit (durable `cancel_requested`) or the root may
        // reach a terminal state while this admission was in flight. Re-check
        // the AUTHORITATIVE parent record AFTER the child row is persisted,
        // atomically with the children-map push (the cancel closure snapshots
        // the map under the read lock, so a fence that commits after this
        // recheck is guaranteed to observe the child in the closure). A parent
        // that is terminal or carries durable `cancel_requested` refuses the
        // admission: the child is rolled back — its coordinator token is
        // fired, its row is settled durably `cancelled` (A5 settlement fence;
        // never runnable, never replayed), and every in-memory entry is
        // dropped — so no descendant can escape a root cancellation closure
        // (A5: no downstream effects after the fence, complete owned-work
        // cleanup).
        {
            let mut child_map = self.children.write().await;
            if let Some(store) = &self.workflow_store {
                let parent_record = store
                    .load_run(&SessionId(params.parent_session_id.clone()))
                    .await?
                    .ok_or_else(|| {
                        EngineError::SessionNotFound(params.parent_session_id.clone())
                    })?;
                if parent_record.status.is_terminal()
                    || parent_record
                        .state
                        .as_ref()
                        .is_some_and(|s| s.cancel_requested)
                {
                    // Roll back the just-persisted child: fire its token so
                    // no prompt can ever admit under it, settle the child row
                    // terminal `cancelled` (the row is `running` at revision
                    // 1 — an admissible A5 settlement source), and drop every
                    // in-memory entry so no late child remains mapped,
                    // driven, or awaiting a closure reap.
                    {
                        let cancels = self
                            .session_cancels
                            .read()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        if let Some(token) = cancels.get(&child_session_id).cloned() {
                            token.cancel();
                        }
                    }
                    // P1 (fix round 6): the rollback must confirm the durable
                    // Cancelled settlement BEFORE any in-memory ownership is
                    // dropped. The settlement is revision-fenced and retried
                    // on a CAS loss against the current child record (the
                    // same cancel-settlement retry shape as the confirmed
                    // cleanup path): a concurrent transition that wins the
                    // fence between our reload and commit is reloaded and
                    // re-settled against the new revision (or confirmed
                    // already-Cancelled — the rollback outcome is identical).
                    // Other storage/settlement errors propagate: the token
                    // stays registered so the parent-cancel persisted-closure
                    // reconciliation remains the owner-scoped fallback, and
                    // no `TerminalState` is returned as if the rollback
                    // completed.
                    let child_sid = SessionId(child_session_id.clone());
                    let mut settle_attempts: u32 = 0;
                    loop {
                        let Some(current) = store.load_run(&child_sid).await? else {
                            return Err(EngineError::GraphFlow(
                                graph_flow::GraphError::StorageError(format!(
                                    "child '{child_session_id}' disappeared before cancelled settlement"
                                )),
                            ));
                        };
                        if current.status == SessionStatus::Cancelled {
                            // A concurrent cancel already settled the child —
                            // the rollback outcome is identical.
                            break;
                        }
                        // The `Interrupted` cleanup-retry shape (durable
                        // cancel intent) is an A5 settlement target; every
                        // other terminal status is a deliberate conflict.
                        let interrupted_retry_shape = current.status == SessionStatus::Interrupted
                            && current.state.as_ref().is_some_and(|s| s.cancel_requested);
                        if current.status.is_terminal() && !interrupted_retry_shape {
                            return Err(EngineError::TerminalState(child_session_id.clone()));
                        }
                        let current_revision = current.state_revision;
                        let mut cancelled_state = current.state.unwrap_or_default();
                        cancelled_state.cancel_requested = true;
                        cancelled_state.in_flight = None;
                        // Reload the authoritative root snapshot together
                        // with the current run record so the terminal
                        // checkpoint never clobbers a position/context a
                        // concurrent transition advanced.
                        let settle_root = self
                            .storage
                            .get(&child_session_id)
                            .await
                            .map_err(EngineError::GraphFlow)?
                            .ok_or_else(|| {
                                EngineError::SessionNotFound(child_session_id.clone())
                            })?;
                        let settle_checkpoint = RunCheckpoint {
                            root: &settle_root,
                            children: &[],
                        };
                        match store
                            .settle_cancelled(
                                &child_sid,
                                current_revision,
                                settle_checkpoint,
                                &cancelled_state,
                            )
                            .await
                        {
                            Ok(_) => break,
                            Err(EngineError::RevisionMismatch { .. }) => {
                                settle_attempts += 1;
                                if settle_attempts >= CANCEL_FENCE_RETRY_BOUND {
                                    return Err(EngineError::RevisionMismatch {
                                        session_id: child_session_id.clone(),
                                        expected: current_revision,
                                        found: current_revision,
                                    });
                                }
                                // Reload and re-settle against the new
                                // revision.
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    child_map
                        .entry(params.parent_session_id.clone())
                        .or_default()
                        .retain(|c| c.session.id != child_session_id);
                    if child_map
                        .get(&params.parent_session_id)
                        .is_some_and(Vec::is_empty)
                    {
                        child_map.remove(&params.parent_session_id);
                    }
                    self.runners.write().await.remove(&child_session_id);
                    self.sessions
                        .write()
                        .await
                        .retain(|s| s.session_id.0 != child_session_id);
                    self.session_cancels
                        .write()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .remove(&child_session_id);
                    return Err(EngineError::TerminalState(params.parent_session_id.clone()));
                }
            }
            child_map
                .entry(params.parent_session_id.clone())
                .or_default()
                .push(child_checkpoint);
        }

        // WS2 R3: Store Arc<FlowRunner> instead of FlowRunner.
        let runner = Arc::new(FlowRunner::new(params.inner_graph, self.storage.clone()));
        self.runners
            .write()
            .await
            .insert(child_session_id.clone(), runner);

        self.sessions.write().await.push(SessionSummary {
            session_id: SessionId(child_session_id.clone()),
            creator_id: String::new(),
            preset_id: String::new(),
            status: SessionStatus::Running,
            current_task_id: Some(start_task_id),
        });

        Ok(SessionId(child_session_id))
    }

    /// Return a persisted child session id for the given parent + inner graph
    /// when one already exists (recovery attach, Important 4).
    ///
    /// The children map is keyed by parent id and each child carries its
    /// `graph_name`. We return the first matching child whose `graph_name`
    /// matches — **terminal children included** — and reconstruct a
    /// `FlowRunner` for it so the caller (`InnerGraphTask`) can consume it
    /// rather than spawning a duplicate. When a durable child row exists for
    /// the current inner-graph position, recovery must consume/reattach it
    /// (terminal included) and only spawn when NO persisted child exists; a
    /// terminal child's completed prompt/effect work must never be replayed
    /// (A2/A7 no-replay guarantee, Round-5 Important 1). The reconstructed
    /// runner points at the same storage, resuming from the persisted
    /// position. When the store is absent or no matching child exists,
    /// returns `None`.
    ///
    /// # Errors
    ///
    /// Returns `EngineError` when a durable child row must be reattached but
    /// cannot be loaded or its descriptor identity is tampered.
    pub async fn attach_existing_child_session_internal(
        &self,
        parent_session_id: &str,
        inner_graph: Arc<Graph>,
    ) -> Result<Option<SessionId>, EngineError> {
        let target = {
            let children = self.children.read().await;
            let Some(parent_children) = children.get(parent_session_id) else {
                return Ok(None);
            };
            let found = parent_children
                .iter()
                .find(|c| c.graph_name.as_deref() == Some(inner_graph.id.as_str()))
                .map(|c| {
                    (
                        c.session.id.clone(),
                        c.session.current_task_id.clone(),
                        c.status.clone(),
                    )
                });
            drop(children); // release read guard before the find result is used downstream
            found
        };
        let Some((child_id, child_task, child_status)) = target else {
            return Ok(None);
        };
        // P3 T1 rereview-2 P1 (A7 rule 2): a persisted child row may only be
        // attached when its frozen descriptor still matches the trusted root
        // identity. A parseable but tampered child (different parent, graph,
        // preset, version or source) is non-replayable — failing closed here
        // also prevents the caller from falling back to `spawn_child_session`
        // and replaying work under a fresh child id.
        if let Some(store) = &self.workflow_store {
            let parent_id = SessionId(parent_session_id.to_string());
            let parent_record = store
                .load_run(&parent_id)
                .await?
                .ok_or_else(|| EngineError::SessionNotFound(parent_session_id.to_string()))?;
            if parent_record.execution_version >= 1 {
                let child_session = SessionId(child_id.clone());
                let child_record = store.load_run(&child_session).await?.ok_or_else(|| {
                    EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                        "attach_existing_child_session '{child_id}': child row missing (non-replayable)"
                    )))
                })?;
                validate_child_descriptor_identity(
                    &parent_id,
                    parent_record.descriptor.as_ref(),
                    &child_record,
                )?;
            }
        }
        // Reconstruct a FlowRunner for the reattached child so it can be
        // stepped from its persisted position (and so the caller can query
        // its durable status via `get_status`).
        if !self.runners.read().await.contains_key(&child_id) {
            let runner = Arc::new(FlowRunner::new(inner_graph, self.storage.clone()));
            self.runners.write().await.insert(child_id.clone(), runner);
        }
        // Register the reattached child's coordinator cancellation token
        // (A1): a resumed child's `acp_prompt` nodes must find their token
        // in the shared map (fail-closed otherwise).
        self.register_cancellation(&SessionId(child_id.clone()));
        let (parent_creator_id, parent_preset_id) = self
            .sessions
            .read()
            .await
            .iter()
            .find(|summary| summary.session_id.0 == parent_session_id)
            .map(|summary| (summary.creator_id.clone(), summary.preset_id.clone()))
            .unwrap_or_default();
        // Register the reattached child in the in-memory session tracker so
        // `get_status`/`InnerGraphTask` can observe its durable status
        // (terminal children are attached and consumed, never re-stepped —
        // Round-5 Important 1).
        {
            let mut sessions = self.sessions.write().await;
            if !sessions.iter().any(|s| s.session_id.0 == child_id) {
                sessions.push(SessionSummary {
                    session_id: SessionId(child_id.clone()),
                    creator_id: parent_creator_id,
                    preset_id: parent_preset_id,
                    status: child_status,
                    current_task_id: Some(child_task),
                });
            }
        }
        Ok(Some(SessionId(child_id)))
    }
}

/// A7 rule 2 (P3 T1 rereview-2 P1): a persisted v1 child descriptor must
/// match the trusted root identity exactly. Parseable but tampered metadata
/// is non-replayable — never silently reinterpreted by attaching the row to
/// a root-built graph (which would also let a graph-name mismatch fall back
/// to spawning a fresh child and replaying work).
fn validate_child_descriptor_identity(
    parent_session_id: &SessionId,
    parent: Option<&RunDescriptorV1>,
    child: &RunRecord,
) -> Result<(), EngineError> {
    let non_replayable = |reason: String| {
        EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
            "child '{}' descriptor identity mismatch (non-replayable): {reason}",
            child.session_id.0
        )))
    };
    // Legacy v0 children keep the shipped conservative classifier; only v1
    // children carry the frozen identity this validator enforces. A v1 child
    // under a v0 parent reaches the `parent` check below and is refused.
    if child.execution_version < 1 {
        return Ok(());
    }
    let Some(parent) = parent else {
        return Err(non_replayable(format!(
            "parent '{}' has no frozen v1 descriptor; child identity unverifiable",
            parent_session_id.0
        )));
    };
    let Some(child_descriptor) = child.descriptor.as_ref() else {
        return Err(non_replayable(
            "child has no frozen v1 descriptor".to_string(),
        ));
    };
    if child_descriptor.parent_session_id.as_ref() != Some(parent_session_id) {
        return Err(non_replayable(format!(
            "parent_session_id {:?} does not name the recovering root",
            child_descriptor.parent_session_id
        )));
    }
    if child_descriptor.graph_name.is_none() {
        return Err(non_replayable("graph_name is absent".to_string()));
    }
    if child_descriptor.creator_id != parent.creator_id
        || child_descriptor.preset_id != parent.preset_id
        || child_descriptor.preset_version != parent.preset_version
        || child_descriptor.source != parent.source
    {
        return Err(non_replayable(format!(
            "creator/preset/version/source ({} v{}) do not match the trusted root ({} v{})",
            child_descriptor.preset_id,
            child_descriptor.preset_version,
            parent.preset_id,
            parent.preset_version
        )));
    }
    Ok(())
}

/// A7 rule 2 (P3 T1 rereview-4 P1): decide whether a persisted descendant's
/// `graph_name` is replayable for its parent's current position.
///
/// - unknown inner graph name → non-replayable;
/// - NON-TERMINAL child whose name differs from the inner graph entered by
///   the parent's current task position → non-replayable (a valid-but-wrong
///   name would miss reattachment and mint a replacement child);
/// - terminal children are historical and only need membership.
#[allow(clippy::too_many_arguments)] // pure predicate over the candidate child's fields; a struct would flatten the caller
fn validate_descendant_position(
    preset_id: &str,
    valid_inner: &std::collections::HashSet<&str>,
    expected_inner: Option<&str>,
    parent_is_root: bool,
    recorded_graph: Option<&str>,
    parent_id: &str,
    parent_task: &str,
    child_id: &str,
    graph_name: Option<&str>,
    terminal: bool,
) -> Result<(), EngineError> {
    let name = graph_name.ok_or_else(|| {
        EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
            "R6: child '{child_id}' has no graph_name (non-replayable)"
        )))
    })?;
    if !valid_inner.contains(name) {
        return Err(EngineError::GraphFlow(
            graph_flow::GraphError::StorageError(format!(
                "R6: child '{child_id}' names unknown inner graph '{name}' for preset \
                 '{preset_id}' (tampered/unsupported metadata, non-replayable)"
            )),
        ));
    }
    if terminal {
        return Ok(());
    }
    // Inner graph nodes are `acp_prompt` only (no nested inner graphs), so a
    // child session can never own a live descendant (P3 T1 rereview-5 P1).
    if !parent_is_root {
        return Err(EngineError::GraphFlow(
            graph_flow::GraphError::StorageError(format!(
                "R6: non-terminal child '{child_id}' under inner-graph session '{parent_id}' \
                 (position '{parent_task}') — nested inner graphs do not exist \
                 (tampered/unsupported metadata, non-replayable)"
            )),
        ));
    }
    match expected_inner {
        Some(expected) if expected == name => Ok(()),
        Some(expected) => Err(EngineError::GraphFlow(
            graph_flow::GraphError::StorageError(format!(
                "R6: non-terminal child '{child_id}' names inner graph '{name}' but parent \
                 '{parent_id}' is at position '{parent_task}' which enters '{expected}' \
                 (tampered/unsupported metadata, non-replayable)"
            )),
        )),
        // The root is past the inner-graph position (e.g. the bounded poller
        // abandoned the child). Tolerated ONLY when the root context records
        // this exact child under a state that enters THIS graph — otherwise a
        // loop-back re-entry would miss attachment and mint a replacement.
        None if recorded_graph == Some(name) => Ok(()),
        None => Err(EngineError::GraphFlow(
            graph_flow::GraphError::StorageError(format!(
                "R6: non-terminal child '{child_id}' under root '{parent_id}' (position \
                 '{parent_task}') is not recorded by an inner-graph state entering '{name}' \
                 (tampered/unsupported metadata, non-replayable)"
            )),
        )),
    }
}

/// The inner graph a manifest state enters, if any.
fn inner_graph_entered_by_state<'a>(
    loaded: &'a crate::preset::LoadedPreset,
    state_id: &str,
) -> Option<&'a str> {
    loaded
        .manifest
        .states
        .iter()
        .find(|state| state.id == state_id)
        .and_then(|state| {
            state.enter.iter().find_map(|action| match action {
                crate::preset::manifest::EnterAction::InnerGraph { name } => Some(name.as_str()),
                _ => None,
            })
        })
}

/// Build the durable [`RunStateV1`] for a step outcome (A2/A4).
///
/// On a terminal/wait/paused transition the step is no longer in flight; the
/// failure record is populated from a graph-flow error when the step failed.
/// A **human** wait (`WaitingForInput` that is NOT a scheduler converge/merge
/// park — the caller has already re-mapped join parks to `Paused`) produces
/// a **fresh** durable [`WaitRecord`] with a new UUID token per arrival (A4),
/// retained through restart until a successful CAS consumes it. When the
/// wait belongs to a nested inner-graph child, `root_context` carries the
/// `_child_wait_session` / `_child_wait_task` markers written by
/// `InnerGraphTask` and the root `WaitRecord` names the exact waiting
/// descendant (`child_session_id` / `child_task_id`, A4 nested path).
/// Scheduler converge/merge parks arrive here as `SessionStatus::Paused` and
/// persist tokenless (A2) — the join keys in the context are the wait
/// evidence. Manual waits reached through labeled/conditional routing with
/// stale/broad join keys are not re-mapped (no current-gate park marker) and
/// keep the `waiting_for_input` + fresh token shape.
fn build_step_state(
    result: &graph_flow::ExecutionResult,
    status: &SessionStatus,
    current_task_id: &str,
    root_context: &graph_flow::Context,
) -> RunStateV1 {
    let failure = match (&result.status, status) {
        (ExecutionStatus::Error(msg), SessionStatus::Failed) => {
            Some(crate::run_state::RunFailure {
                code: "graph_step_error".to_string(),
                message: msg.clone(),
            })
        }
        _ => None,
    };
    // Round-4 Critical 2 (A4): when THIS step parked because a nested child
    // is waiting for human input, the root WaitRecord names the exact
    // waiting descendant (`child_session_id` / `child_task_id`) so restart
    // reconstruction and explicit continue target the right child cursor.
    // `InnerGraphTask` writes `_child_wait_session` / `_child_wait_task` on
    // the parent context in exactly that case.
    let (child_session_id, child_task_id) = if matches!(status, SessionStatus::WaitingForInput) {
        (
            root_context.get_sync("_child_wait_session"),
            root_context.get_sync("_child_wait_task"),
        )
    } else {
        (None, None)
    };
    let wait = if matches!(status, SessionStatus::WaitingForInput) {
        Some(crate::run_state::WaitRecord {
            wait_id: uuid::Uuid::new_v4().to_string(),
            task_id: current_task_id.to_string(),
            child_session_id,
            child_task_id,
            kind: crate::run_state::WaitKind::Manual,
        })
    } else {
        None
    };
    RunStateV1 {
        wait,
        step_in_flight: None,
        in_flight: None,
        failure,
        cancel_requested: false,
    }
}

// ---------------------------------------------------------------------------
// EngineProxy — lightweight wrapper over EngineSharedState (WS3 R1)
// ---------------------------------------------------------------------------

/// Lightweight proxy engine that wraps `EngineSharedState`.
///
/// Used by `start_session_with_preset` when we need `Arc<dyn OrchestrationEngine>`
/// to pass to preset loader. The proxy delegates all operations to the shared
/// state, eliminating code duplication.
struct EngineProxy {
    state: Arc<EngineSharedState>,
}

#[async_trait]
impl OrchestrationEngine for EngineProxy {
    async fn run_step(&self, session_id: &SessionId) -> Result<StepOutcome, EngineError> {
        // Delegate to shared state (WS3 R1: eliminates duplication).
        let result = self.state.run_step_internal(session_id).await?;

        // Translate graph-flow ExecutionResult to our StepOutcome.
        let outcome = match &result.status {
            ExecutionStatus::Completed => StepOutcome::Completed {
                response: result.response,
            },
            ExecutionStatus::Paused {
                next_task_id,
                reason,
            } => StepOutcome::Paused {
                next_task_id: next_task_id.clone(),
                reason: reason.clone(),
            },
            ExecutionStatus::WaitingForInput => StepOutcome::WaitingForInput {
                response: result.response,
            },
            ExecutionStatus::Error(msg) => StepOutcome::Error(msg.clone()),
        };

        Ok(outcome)
    }

    async fn new_session(&self, _key: SessionKey, _ctx: Context) -> Result<SessionId, EngineError> {
        Err(EngineError::NoGraphLoaded)
    }

    async fn start_session_with_graph(
        &self,
        id_prefix: &str,
        graph: Arc<Graph>,
    ) -> Result<SessionId, EngineError> {
        let session_id = format!("{}:{}", id_prefix, uuid::Uuid::new_v4());
        let start_task_id = graph.start_task_id().unwrap_or_default();
        let session = graph_flow::Session::new_from_task(session_id.clone(), &start_task_id);
        session.context.set("_session_id", session_id.clone()).await;
        if self.state.workflow_store.is_some() {
            return Err(EngineError::GraphFlow(graph_flow::GraphError::StorageError(
                "start_session_with_graph cannot create a recoverable run without a preset descriptor"
                    .to_string(),
            )));
        }
        self.state.storage.save(session).await?;
        // WS2 R3: Store Arc<FlowRunner> instead of FlowRunner.
        let runner = Arc::new(graph_flow::FlowRunner::new(
            graph,
            self.state.storage.clone(),
        ));
        self.state
            .runners
            .write()
            .await
            .insert(session_id.clone(), runner);
        self.state.sessions.write().await.push(SessionSummary {
            session_id: SessionId(session_id.clone()),
            creator_id: String::new(),
            preset_id: id_prefix.to_string(),
            status: SessionStatus::Running,
            current_task_id: Some(start_task_id),
        });
        Ok(SessionId(session_id))
    }

    async fn get_status(&self, session_id: &SessionId) -> Result<SessionStatus, EngineError> {
        let sessions = self.state.sessions.read().await;
        sessions
            .iter()
            .find(|s| s.session_id == *session_id)
            .map(|s| s.status.clone())
            .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))
    }

    async fn signal(
        &self,
        session_id: &SessionId,
        signal: EngineSignal,
    ) -> Result<(), EngineError> {
        // Persist the transition authoritatively (A2/A5) when a workflow
        // store is present: Cancel → Cancelled, Pause → Paused, Resume →
        // Running (fenced against terminal states). Memory-only status
        // flips are no longer the SSOT (Critical 3).
        let target_status = self
            .state
            .persist_signal_transition(session_id, &signal)
            .await?;

        let mut sessions = self.state.sessions.write().await;
        if let Some(s) = sessions.iter_mut().find(|s| s.session_id == *session_id) {
            s.status = target_status;
            Ok(())
        } else {
            Err(EngineError::SessionNotFound(session_id.0.clone()))
        }
    }

    async fn list_active(&self, filter: SessionFilter) -> Result<Vec<SessionSummary>, EngineError> {
        let sessions = self.state.sessions.read().await;
        Ok(sessions
            .iter()
            .filter(|s| {
                let status_ok = matches!(
                    s.status,
                    SessionStatus::Running | SessionStatus::Paused | SessionStatus::WaitingForInput
                );
                let creator_ok = filter
                    .creator_id
                    .as_ref()
                    .is_none_or(|c| c == &s.creator_id);
                let preset_ok = filter.preset_id.as_ref().is_none_or(|p| p == &s.preset_id);
                status_ok && creator_ok && preset_ok
            })
            .cloned()
            .collect())
    }

    async fn spawn_child_session(
        &self,
        params: ChildSessionParams,
    ) -> Result<SessionId, EngineError> {
        self.state.spawn_child_session_internal(params).await
    }

    async fn attach_existing_child_session(
        &self,
        parent_session_id: &str,
        inner_graph: Arc<Graph>,
    ) -> Result<Option<SessionId>, EngineError> {
        self.state
            .attach_existing_child_session_internal(parent_session_id, inner_graph)
            .await
    }

    async fn get_current_task_id(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<String>, EngineError> {
        EngineSharedState::current_task_id(&self.state, session_id).await
    }

    async fn get_context(
        &self,
        session_id: &SessionId,
    ) -> Result<graph_flow::Context, EngineError> {
        let session = self
            .state
            .storage
            .get(&session_id.0)
            .await
            .map_err(EngineError::GraphFlow)?
            .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))?;
        Ok(session.context)
    }

    async fn has_runner(&self, session_id: &SessionId) -> bool {
        self.state.runners.read().await.contains_key(&session_id.0)
    }

    async fn recover_sessions(&self, summaries: Vec<SessionSummary>) {
        // EngineProxy is a lightweight constructor-time facade (used by the
        // preset loader to wire inner graphs); full A7 reconstruction lives
        // on GraphFlowEngine. The in-memory tracker half is still honored so
        // a proxy-backed consumer observes recovered sessions.
        self.state.recover_sessions(summaries).await;
    }

    async fn ensure_recovered_runner(&self, session_id: &SessionId) -> Result<(), EngineError> {
        // EngineProxy carries no capability holder/executor wiring, so the
        // frozen-source reconstruction cannot run through it. GraphFlowEngine
        // (the daemon's concrete engine) is the only supported path; this
        // proxy never serves daemon recovery.
        Err(EngineError::GraphFlow(
            graph_flow::GraphError::StorageError(format!(
                "ensure_recovered_runner: EngineProxy cannot reconstruct session '{}' \
                 (no capability/executor wiring); use the concrete GraphFlowEngine \
                 (reconstruction_unavailable, non-replayable)",
                session_id.0
            )),
        ))
    }

    async fn start_session_with_preset(
        &self,
        _loaded: &crate::preset::LoadedPreset,
    ) -> Result<SessionId, EngineError> {
        Err(EngineError::NoGraphLoaded)
    }

    async fn start_session_with_preset_for_creator(
        &self,
        _loaded: &crate::preset::LoadedPreset,
        _creator_id: &str,
    ) -> Result<SessionId, EngineError> {
        Err(EngineError::NoGraphLoaded)
    }
}

// ---------------------------------------------------------------------------
// GraphFlowEngine — adapter over graph-flow
// ---------------------------------------------------------------------------

/// Concrete [`OrchestrationEngine`] backed by [`graph_flow::FlowRunner`].
///
/// The engine stores an `Arc<FlowRunner>` per session (WS2 R3), avoiding clone
/// overhead. Sessions are persisted via the provided [`SessionStorage`].
///
/// WS3 R1: Uses `EngineSharedState` for shared state, eliminating duplication
/// with `EngineProxy`.
pub struct GraphFlowEngine {
    /// Shared state (storage, runners, sessions) — WS3 R1 extraction.
    state: Arc<EngineSharedState>,
    /// Shared capability registry holder (V1.176 P1, AR-92 #6): graph
    /// builds snapshot `handle.get()` at graph-build time, so NEW graphs see
    /// hot-reloaded user capabilities while an in-flight session's graph
    /// keeps its session-start snapshot (in-flight honesty).
    caps: crate::capability::CapabilityRegistryHolder,
    /// Daemon-side tool dispatch for `nexus.*` host tool actions (DF-47, V1.42 P3).
    daemon_tool_dispatch: Option<std::sync::Arc<dyn crate::capability::DaemonToolDispatch>>,
    /// Production prompt executor (A1) — wired into every inner graph
    /// `acp_prompt` node at wired-graph build time. `None` for
    /// in-memory/test engines (inner prompt nodes refuse).
    prompt_executor: Option<std::sync::Arc<dyn crate::capability::PromptExecutor>>,
    /// Per-run coordinator cancellation tokens (A1) — the executor listens to
    /// the token for the current run concurrently with the Host stream.
    session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
    /// Resolved workspace root the engine's runs execute in (A2). `None`
    /// for in-memory/test engines that do not persist v1 descriptors.
    workspace_root: Option<std::path::PathBuf>,
    /// Nexus home (`~/.nexus42`) used to resolve directory presets for
    /// source identity (A2/A7). `None` for in-memory/test engines.
    nexus_home: Option<std::path::PathBuf>,
}

impl Clone for GraphFlowEngine {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            caps: self.caps.clone(),
            daemon_tool_dispatch: self.daemon_tool_dispatch.clone(),
            prompt_executor: self.prompt_executor.clone(),
            session_cancels: self.session_cancels.clone(),
            workspace_root: self.workspace_root.clone(),
            nexus_home: self.nexus_home.clone(),
        }
    }
}

impl GraphFlowEngine {
    /// Create a new engine that persists sessions into `storage`.
    ///
    /// The `storage` parameter accepts **any** [`SessionStorage`] implementation
    /// — `InMemorySessionStorage` for tests, `SqliteSessionStorage` for
    /// production.
    ///
    /// This constructor does **not** attach a transactional
    /// [`WorkflowStateStore`]; use [`Self::new_with_storage_and_workflow_store`]
    /// when the storage backend also implements it (SQLite production) so
    /// terminal/wait transitions persist authoritatively.
    pub fn new_with_storage(
        storage: Arc<dyn SessionStorage>,
        caps: crate::capability::CapabilityRegistryHolder,
    ) -> Self {
        Self {
            state: Arc::new(EngineSharedState::new(storage)),
            caps,
            daemon_tool_dispatch: None,
            prompt_executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            workspace_root: None,
            nexus_home: None,
        }
    }

    /// Create a new engine with a transactional workflow store (A2).
    ///
    /// `storage` is the graph-flow session backend; `workflow_store` is the
    /// authoritative run-state store (the SQLite adapter implements both).
    /// When present, terminal/wait transitions are persisted via
    /// `commit_transition` under a revision CAS.
    pub fn new_with_storage_and_workflow_store(
        storage: Arc<dyn SessionStorage>,
        workflow_store: Arc<dyn WorkflowStateStore>,
        caps: crate::capability::CapabilityRegistryHolder,
    ) -> Self {
        Self {
            state: Arc::new(EngineSharedState::with_workflow_store(
                storage,
                workflow_store,
            )),
            caps,
            daemon_tool_dispatch: None,
            prompt_executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            workspace_root: None,
            nexus_home: None,
        }
    }

    /// Create a new engine with a transactional workflow store and a resolved
    /// workspace root (A2). The workspace root is frozen into every v1 run
    /// descriptor the engine starts.
    pub fn new_with_storage_and_workflow_store_and_workspace(
        storage: Arc<dyn SessionStorage>,
        workflow_store: Arc<dyn WorkflowStateStore>,
        caps: crate::capability::CapabilityRegistryHolder,
        workspace_root: std::path::PathBuf,
    ) -> Self {
        Self {
            state: Arc::new(EngineSharedState::with_workflow_store(
                storage,
                workflow_store,
            )),
            caps,
            daemon_tool_dispatch: None,
            prompt_executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            workspace_root: Some(workspace_root),
            nexus_home: None,
        }
    }

    /// Set the nexus home (`~/.nexus42`) used to resolve directory presets
    /// for source identity (A2/A7). Called by the daemon boot.
    pub fn set_nexus_home(&mut self, nexus_home: std::path::PathBuf) {
        self.nexus_home = Some(nexus_home);
    }

    /// Register a run's coordinator cancellation token in the shared
    /// per-run map (A1, fail-closed contract).
    ///
    /// Every run the engine admits — preset start, session start, nested
    /// child spawn, and boot recovery — registers a fresh cancellation
    /// token so the production prompt consumers (capabilities + graph
    /// `acp_prompt`) find a cancellable token at dispatch. A prompt invoked
    /// for a run with NO registered token fails closed with
    /// `CancellationUnavailable` (never a fresh uncancellable token). The
    /// future P2 coordinator shares this same map and the executor's
    /// per-run operation locks.
    fn register_cancellation(&self, session_id: &SessionId) {
        self.session_cancels
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(session_id.0.clone())
            .or_default();
    }

    /// Set the daemon-side tool dispatch adapter (DF-47, V1.42 P3).
    ///
    /// Must be called before any session starts if preset graphs contain
    /// `host_tool` enter actions. Typically called once during daemon boot.
    pub fn set_daemon_tool_dispatch(
        &mut self,
        dispatch: Arc<dyn crate::capability::DaemonToolDispatch>,
    ) {
        self.daemon_tool_dispatch = Some(dispatch);
    }

    /// Set the production prompt executor and per-run cancellation tokens
    /// (A1).
    ///
    /// Must be called before any session starts so inner graph `acp_prompt`
    /// nodes execute through the Host plane. Typically called once during
    /// daemon boot.
    pub fn set_prompt_executor(
        &mut self,
        executor: Arc<dyn crate::capability::PromptExecutor>,
        session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        >,
    ) {
        self.prompt_executor = Some(executor.clone());
        self.session_cancels = session_cancels.clone();
        // Unify the shared per-run cancellation map: run-admission
        // registrations (spawn/attach on `EngineSharedState`) and the
        // prompt consumers (graph tasks wired with this engine's map,
        // capabilities wired with the boot map) must read the SAME Arc.
        // The production boot calls this on a freshly constructed engine
        // (state refcount 1), so `get_mut` succeeds; engines whose shared
        // state is aliased keep their own map (test wiring passes the
        // engine's shared map to the prompt consumers explicitly).
        if let Some(state) = std::sync::Arc::get_mut(&mut self.state) {
            state.session_cancels = session_cancels;
            state.prompt_executor = Some(executor);
        }
    }

    /// Recover persisted sessions into the in-memory tracker (WS2 R1 + R6).
    ///
    /// Called after engine construction on daemon restart. Queries
    /// `SqliteSessionStorage.list_non_terminal_sessions()` and repopulates
    /// the in-memory tracker.
    ///
    /// **R6 fix**: For each non-terminal session summary, reconstructs the
    /// `FlowRunner` so that `run_step` succeeds after recovery. The trait
    /// entry point (`OrchestrationEngine::recover_sessions`) delegates here.
    pub async fn recover_sessions_inner(&self, summaries: Vec<SessionSummary>) {
        for summary in &summaries {
            // Skip terminal sessions — they don't need runners.
            if summary.status.is_terminal() {
                continue;
            }

            // Register the recovered run's coordinator cancellation token
            // (A1): a resumed run's prompt consumers resolve their token
            // from the shared per-run map and fail closed when none is
            // registered.
            self.register_cancellation(&summary.session_id);

            // Store-backed runs: reconstruct the COMPLETE owned-descendant
            // closure (children hydrated first, then the root runner from
            // the frozen source). A corrupt/unsupported child propagates as
            // non-replayable (A7). No-store (in-memory/test) engines keep
            // the legacy embedded-preset reconstruction directly.
            let result = if self.state.workflow_store.is_some() {
                self.attach_runner_with_owned_closure(&summary.session_id)
                    .await
            } else {
                self.reconstruct_runner_legacy(summary).await
            };
            if let Err(e) = result {
                tracing::warn!(
                    "R6: failed to reconstruct runner for session {}: {}; \
                     session will remain in tracker but run_step will fail until \
                     manually re-started (reconstruction_unavailable, non-replayable)",
                    summary.session_id.0,
                    e
                );
            }
        }

        // Add all summaries to the in-memory tracker (idempotent).
        self.state.recover_sessions(summaries).await;
    }

    /// Reconstruct the COMPLETE owned-descendant closure for a recovered
    /// root — hydrated children map first, then the root runner (A7).
    ///
    /// `hydrate_children` installs only the DIRECT children of the requested
    /// parent; a child can itself own nested children (`InnerGraphTask`
    /// spawns through the same engine path), so a restarted root must
    /// recursively hydrate the persisted parent relation breadth-first. A
    /// root Cancel after this recovery can then fire/finalize every owned
    /// descendant.
    ///
    /// A corrupt/unsupported child (`load_children` failure, missing child
    /// session snapshot) must propagate: the parent cannot be reconstructed
    /// over an incomplete children map (A7). Surface it as non-replayable.
    async fn attach_runner_with_owned_closure(
        &self,
        root_session_id: &SessionId,
    ) -> Result<(), EngineError> {
        let mut hydrate_queue: std::collections::VecDeque<SessionId> =
            std::collections::VecDeque::new();
        // A cyclic persisted parent relation (corrupt rows) must never hang
        // recovery: visit each session at most once (tri-QC P1-D).
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        hydrate_queue.push_back(root_session_id.clone());
        visited.insert(root_session_id.0.clone());
        while let Some(parent) = hydrate_queue.pop_front() {
            self.state.hydrate_children(&parent).await?;
            // Every recovered descendant registers its coordinator
            // cancellation token (A1) and is enqueued for its own
            // children — the closure is complete only when the whole
            // persisted tree is hydrated.
            let children = self
                .state
                .children
                .read()
                .await
                .get(&parent.0)
                .cloned()
                .unwrap_or_default();
            for child in &children {
                self.register_cancellation(&SessionId(child.session.id.clone()));
                if visited.insert(child.session.id.clone()) {
                    hydrate_queue.push_back(SessionId(child.session.id.clone()));
                }
            }
        }
        // R6: Try to reconstruct the root FlowRunner from the frozen source
        // descriptor. The persisted session data in storage preserves the
        // execution position.
        self.reconstruct_runner(root_session_id).await
    }

    /// Attach a runner for `session_id` ON DEMAND (A7 rule 4).
    ///
    /// A human-wait run whose runner was not attached at boot (recovery
    /// skipped it, or this daemon instance never ran recovery for it) gets
    /// its runner rebuilt here on a matching continue — the complete
    /// owned-descendant closure first, then the root runner, using the
    /// exact same frozen-source verification as boot recovery.
    ///
    /// Returns the concrete [`EngineError`] reason (source missing, hash
    /// mismatch, corrupt child identity) when reconstruction is impossible,
    /// so the caller can surface `reconstruction_unavailable` while the
    /// durable human wait stays preserved.
    ///
    /// # Errors
    /// Returns [`EngineError`] when the run has no durable v1 descriptor,
    /// the frozen source no longer matches, or a persisted child is
    /// corrupt/unsupported — all non-replayable.
    pub async fn ensure_recovered_runner_inner(
        &self,
        session_id: &SessionId,
    ) -> Result<(), EngineError> {
        self.register_cancellation(session_id);
        if self.has_runner(session_id).await {
            return Ok(());
        }
        self.attach_runner_with_owned_closure(session_id).await
    }

    /// Reconstruct a `FlowRunner` for a recovered session (R6).
    ///
    /// Loads the embedded preset by `preset_id`, builds the wired outer graph,
    /// and creates a `FlowRunner` with the engine's storage. The persisted
    /// session data in `SqliteSessionStorage` preserves the execution position.
    /// Snapshot the current registry from the shared holder (AR-92 #6).
    ///
    /// Returns an empty registry when the holder was never populated — an
    /// unreachable state in production (boot swaps the registry in before
    /// the engine exists), kept fail-closed so a never-populated holder
    /// validates nothing rather than resolving capabilities that do not
    /// exist.
    fn current_caps(&self) -> Arc<CapabilityRegistry> {
        self.caps
            .get()
            .unwrap_or_else(|| Arc::new(CapabilityRegistry::empty()))
    }

    #[allow(clippy::too_many_lines, clippy::significant_drop_tightening)] // sequential descriptor→validate→rebuild path; guard spans await loop
    async fn reconstruct_runner(&self, session_id: &SessionId) -> Result<(), EngineError> {
        // Load the durable descriptor (frozen at admission) so we honour the
        // persisted source identity and preset version (Important 5). A run
        // that has no v1 descriptor is a legacy/unverified row — its runner
        // cannot be reconstructed over a frozen source (A2/A7).
        let descriptor = if let Some(store) = &self.state.workflow_store {
            let record = store.load_run(session_id).await?.ok_or_else(|| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "R6: session '{}' has no persisted run record; cannot reconstruct runner \
                         (non-replayable)",
                    session_id.0
                )))
            })?;
            record.descriptor.ok_or_else(|| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "R6: session '{}' has no v1 descriptor; cannot reconstruct runner over a \
                     frozen source (non-replayable)",
                    session_id.0
                )))
            })?
        } else {
            // No workflow store (in-memory/test engine): no durable
            // descriptor exists, so a frozen-source reconstruction is
            // impossible. The legacy embedded path is used by
            // `recover_sessions` directly; an on-demand attach without
            // a store (no durable run) is non-replayable.
            let summary = self
                .state
                .sessions
                .read()
                .await
                .iter()
                .find(|s| s.session_id == *session_id)
                .cloned();
            match summary.as_ref() {
                Some(summary) => return self.reconstruct_runner_legacy(summary).await,
                None => {
                    return Err(EngineError::GraphFlow(
                        graph_flow::GraphError::StorageError(format!(
                            "R6: session '{}' has no durable run record and is not tracked; \
                             cannot reconstruct runner (non-replayable)",
                            session_id.0
                        )),
                    ));
                }
            }
        };

        // Resolve and reload the preset from the FROZEN source identity —
        // never the current higher-precedence embedded/current bytes (A7).
        // A directory source is reloaded from its persisted root; an
        // embedded source from the compiled-in preset id.
        let caps = self.current_caps();
        let loaded = match &descriptor.source {
            PresetSourceIdentity::Embedded { preset_id, .. } => {
                match crate::preset::load_embedded_preset(preset_id, &caps) {
                    Ok(loaded) => loaded,
                    Err(e) => {
                        return Err(EngineError::GraphFlow(
                            graph_flow::GraphError::StorageError(format!(
                                "R6: embedded preset '{preset_id}' is missing for session {}: {} \
                                 (reconstruction_unavailable, non-replayable)",
                                session_id.0, e
                            )),
                        ));
                    }
                }
            }
            PresetSourceIdentity::Directory { root, .. } => {
                match crate::preset::load_preset(root, &caps) {
                    Ok(loaded) => loaded,
                    Err(e) => {
                        return Err(EngineError::GraphFlow(
                            graph_flow::GraphError::StorageError(format!(
                                "R6: directory preset at '{}' is missing/invalid for session {}: {} \
                                 (reconstruction_unavailable, non-replayable)",
                                root.display(), session_id.0, e
                            )),
                        ));
                    }
                }
            }
        };

        // Verify the frozen manifest/template content hash matches the
        // reloaded preset's source identity — a changed template or manifest
        // must refuse reconstruction rather than fall back to current bytes
        // (A7). `loaded.source_identity` reflects the actual resolved bytes.
        let reloaded_identity = loaded.source_identity.as_ref().ok_or_else(|| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "R6: reloaded preset '{}' for session {} has no source identity \
                 (reconstruction_unavailable, non-replayable)",
                loaded.id, session_id.0
            )))
        })?;
        if reloaded_identity != &descriptor.source {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "R6: preset source identity mismatch for session {} (frozen {:?} != reloaded {:?}) \
                     — source changed or moved; reconstruction_unavailable (non-replayable)",
                    session_id.0, descriptor.source, reloaded_identity
                )),
            ));
        }

        // Take the preset version from the persisted descriptor (the frozen
        // version, not the current embed) — Important 5.
        let version = descriptor.preset_version;
        if version != loaded.version {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "R6: preset version mismatch for session {} (frozen {} != current {}) \
                     — reconstruction_unavailable (non-replayable)",
                    session_id.0, version, loaded.version
                )),
            ));
        }

        // P3 T1 rereview-3/4 P1 (A7 rule 2): every persisted descendant's
        // `graph_name` must name an inner graph the frozen preset actually
        // defines, and every NON-TERMINAL child must additionally name the
        // inner graph entered by its parent's CURRENT task position. A
        // parseable but tampered graph_name (unknown, or valid-but-wrong
        // position) would otherwise miss the reattachment lookup and make
        // `InnerGraphTask` spawn a fresh child and replay the work.
        {
            let valid_inner: std::collections::HashSet<&str> =
                loaded.inner_graphs.keys().map(String::as_str).collect();
            // Snapshot the closure first so no children-map lock is held
            // across the storage awaits below.
            let snapshot: Vec<(SessionId, Vec<crate::run_state::ChildCheckpoint>)> = {
                let children_map = self.state.children.read().await;
                let mut queue: std::collections::VecDeque<SessionId> =
                    std::collections::VecDeque::new();
                let mut visited: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                queue.push_back(session_id.clone());
                visited.insert(session_id.0.clone());
                let mut collected = Vec::new();
                while let Some(parent) = queue.pop_front() {
                    let Some(children) = children_map.get(&parent.0) else {
                        continue;
                    };
                    for child in children {
                        if visited.insert(child.session.id.clone()) {
                            queue.push_back(SessionId(child.session.id.clone()));
                        }
                    }
                    collected.push((parent, children.clone()));
                }
                collected
            };
            for (parent, children) in snapshot {
                let parent_session = self
                    .state
                    .storage
                    .get(&parent.0)
                    .await
                    .map_err(EngineError::GraphFlow)?;
                let parent_task = parent_session
                    .as_ref()
                    .map(|session| session.current_task_id.clone())
                    .unwrap_or_default();
                let parent_is_root = parent.0 == session_id.0;
                let expected_inner = if parent_is_root {
                    inner_graph_entered_by_state(&loaded, &parent_task)
                } else {
                    // Inner graph nodes cannot enter inner graphs.
                    None
                };
                // Root context records the child session id per inner-graph
                // state that owns it (`_inner_child_session_<state>`); a
                // non-terminal child past the inner-graph position is only
                // replayable when that record names it consistently.
                let recorded: std::collections::HashMap<
                    String,
                    std::collections::HashSet<Option<String>>,
                > = if parent_is_root {
                    let data = parent_session
                        .as_ref()
                        .and_then(|session| serde_json::to_value(&session.context).ok())
                        .and_then(|value| crate::resume_rules::context_data(&value).cloned());
                    let mut map: std::collections::HashMap<
                        String,
                        std::collections::HashSet<Option<String>>,
                    > = std::collections::HashMap::new();
                    if let Some(data) = data {
                        for (key, value) in data {
                            let Some(state) = key.strip_prefix("_inner_child_session_") else {
                                continue;
                            };
                            let Some(child_id) = value.as_str() else {
                                continue;
                            };
                            map.entry(child_id.to_string()).or_default().insert(
                                inner_graph_entered_by_state(&loaded, state).map(str::to_owned),
                            );
                        }
                    }
                    map
                } else {
                    std::collections::HashMap::new()
                };
                for child in children {
                    // All markers naming this child must agree on one inner
                    // graph (duplicate/conflicting markers are tampering).
                    let recorded_graph: Option<&str> = recorded
                        .get(&child.session.id)
                        .filter(|set| set.len() == 1)
                        .and_then(|set| set.iter().next())
                        .and_then(|entry| entry.as_deref());
                    validate_descendant_position(
                        &loaded.id,
                        &valid_inner,
                        expected_inner,
                        parent_is_root,
                        recorded_graph,
                        &parent.0,
                        &parent_task,
                        &child.session.id,
                        child.graph_name.as_deref(),
                        child.status.is_terminal(),
                    )?;
                }
            }
        }

        // Build the wired outer graph using EngineProxy + capabilities.
        let proxy = Arc::new(EngineProxy {
            state: self.state.clone(),
        });
        let engine_proxy: Arc<dyn OrchestrationEngine> = proxy;
        let wired = crate::preset::loader::build_wired_outer_graph(
            &loaded,
            &engine_proxy,
            &caps,
            self.daemon_tool_dispatch.clone(),
            self.prompt_executor.clone(),
            self.session_cancels.clone(),
        );

        // Create FlowRunner with the wired graph and existing storage.
        // The storage already contains the persisted session data, so the
        // runner will resume from the correct execution position.
        let runner = Arc::new(FlowRunner::new(Arc::new(wired), self.state.storage.clone()));

        // Store the runner in the shared state.
        self.state
            .runners
            .write()
            .await
            .insert(session_id.0.clone(), runner);

        tracing::info!(
            "R6: reconstructed runner for session {} (preset: {}, version: {})",
            session_id.0,
            loaded.id,
            version
        );

        Ok(())
    }

    /// Legacy reconstruction against the current embedded preset (no workflow
    /// store). Used only by in-memory/test engines that never persisted a v1
    /// descriptor. This path is intentionally NOT frozen-source aware.
    async fn reconstruct_runner_legacy(&self, summary: &SessionSummary) -> Result<(), EngineError> {
        let caps = self.current_caps();
        let loaded =
            crate::preset::load_embedded_preset(&summary.preset_id, &caps).map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "R6: failed to load embedded preset '{}' for session {}: {}",
                    summary.preset_id, summary.session_id.0, e
                )))
            })?;

        let proxy = Arc::new(EngineProxy {
            state: self.state.clone(),
        });
        let engine_proxy: Arc<dyn OrchestrationEngine> = proxy;
        let wired = crate::preset::loader::build_wired_outer_graph(
            &loaded,
            &engine_proxy,
            &caps,
            self.daemon_tool_dispatch.clone(),
            self.prompt_executor.clone(),
            self.session_cancels.clone(),
        );

        let runner = Arc::new(FlowRunner::new(Arc::new(wired), self.state.storage.clone()));
        self.state
            .runners
            .write()
            .await
            .insert(summary.session_id.0.clone(), runner);

        tracing::info!(
            "R6: reconstructed runner (legacy) for session {} (preset: {})",
            summary.session_id.0,
            summary.preset_id
        );

        Ok(())
    }

    /// Get a reference to the shared state for use in preset loader (WS3 R1).
    #[must_use]
    pub fn shared_state(&self) -> Arc<EngineSharedState> {
        self.state.clone()
    }

    /// Build a frozen [`RunDescriptorV1`] from a loaded preset (A2).
    ///
    /// The source identity comes from the loaded preset (embedded or
    /// directory bundle); the workspace root from the engine. `input` and
    /// `agent_bindings` are empty at this seam — the schedule/creator
    /// admission layer freezes them before driving (P2 owns that).
    fn build_descriptor(
        &self,
        loaded: &crate::preset::LoadedPreset,
        creator_id: &str,
    ) -> Result<RunDescriptorV1, EngineError> {
        let source = loaded.source_identity.clone().ok_or_else(|| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "preset '{}' has no source identity; cannot start a v1 run",
                loaded.id
            )))
        })?;
        let workspace_root = self.workspace_root.clone().unwrap_or_default();
        Ok(RunDescriptorV1 {
            creator_id: creator_id.to_string(),
            work_id: None,
            workspace_root,
            preset_id: loaded.id.clone(),
            preset_version: loaded.version,
            source,
            input: serde_json::Map::new(),
            agent_bindings: HashMap::new(),
            parent_session_id: None,
            graph_name: None,
        })
    }

    /// Start a v1 run for a loaded preset when a workflow store is present
    /// (Critical 2). Falls back to the raw-graph path when no store exists.
    async fn start_preset_run(
        &self,
        loaded: &crate::preset::LoadedPreset,
        creator_id: &str,
        graph: Arc<Graph>,
    ) -> Result<SessionId, EngineError> {
        if let Some(store) = &self.state.workflow_store {
            let descriptor = self.build_descriptor(loaded, creator_id)?;
            let session_id = format!("{}:{}", loaded.id, uuid::Uuid::new_v4());
            let start_task_id = graph.start_task_id().unwrap_or_default();
            let session = graph_flow::Session::new_from_task(session_id.clone(), &start_task_id);
            session.context.set("_session_id", session_id.clone()).await;
            if !creator_id.is_empty() {
                session
                    .context
                    .set("_creator_id", creator_id.to_string())
                    .await;
            }
            let checkpoint = RunCheckpoint {
                root: &session,
                children: &[],
            };
            store
                .start_run(
                    &SessionId(session_id.clone()),
                    &descriptor,
                    checkpoint,
                    &RunStateV1::default(),
                )
                .await?;
            let sid = SessionId(session_id.clone());
            // Register the run's coordinator cancellation token (A1): the
            // run's prompt consumers resolve their token from the shared
            // per-run map and fail closed when none is registered.
            self.register_cancellation(&sid);
            let runner = Arc::new(FlowRunner::new(graph, self.state.storage.clone()));
            self.state
                .runners
                .write()
                .await
                .insert(session_id.clone(), runner);
            self.state.sessions.write().await.push(SessionSummary {
                session_id: SessionId(session_id.clone()),
                creator_id: creator_id.to_string(),
                preset_id: loaded.id.clone(),
                status: SessionStatus::Running,
                current_task_id: Some(start_task_id),
            });
            Ok(SessionId(session_id))
        } else {
            self.start_session_with_creator(&loaded.id, graph, Some(creator_id))
                .await
        }
    }

    /// Start a v1 run for a loaded preset with frozen admission input (A3).
    ///
    /// Mirrors [`start_preset_run`] but freezes the schedule admission's
    /// input map, agent bindings and work id into the descriptor AND seeds
    /// the session context (`preset.input.*`, `core_context.text`) before
    /// the initial checkpoint is persisted — so the first step renders real
    /// input and the descriptor/core seed are durable before eligibility is
    /// published (A3: persist descriptor/core seed before enqueue).
    ///
    /// `session_id` is caller-minted: the schedule admission claims the
    /// schedule row with this exact id BEFORE the run is created, so a
    /// concurrent admission can never mint a second session for the same
    /// schedule (A3 single-session admission).
    ///
    /// # Errors
    /// Returns [`EngineError`] if session creation, preset loading, or the
    /// initial checkpoint write fails.
    #[allow(clippy::too_many_arguments, clippy::implicit_hasher)] // admit-time parameter bundle; a config struct would obscure the graph/frozen-source coupling
    pub async fn start_preset_run_with_input(
        &self,
        session_id: &str,
        loaded: &crate::preset::LoadedPreset,
        creator_id: &str,
        work_id: Option<String>,
        input: serde_json::Map<String, serde_json::Value>,
        core_context: Option<&str>,
        agent_bindings: std::collections::HashMap<String, crate::run_state::AgentBinding>,
        graph: Arc<Graph>,
    ) -> Result<SessionId, EngineError> {
        if let Some(store) = &self.state.workflow_store {
            let mut descriptor = self.build_descriptor(loaded, creator_id)?;
            descriptor.work_id = work_id.filter(|id| !id.is_empty());
            descriptor.input = input.clone();
            descriptor.agent_bindings = agent_bindings;
            let start_task_id = graph.start_task_id().unwrap_or_default();
            let session =
                graph_flow::Session::new_from_task(session_id.to_string(), &start_task_id);
            session
                .context
                .set("_session_id", session_id.to_string())
                .await;
            if !creator_id.is_empty() {
                session
                    .context
                    .set("_creator_id", creator_id.to_string())
                    .await;
            }
            // Seed the frozen admission input + core-context seed into the
            // session context BEFORE the checkpoint is persisted (A3).
            for (key, value) in &input {
                session
                    .context
                    .set(format!("preset.input.{key}"), value.clone())
                    .await;
            }
            if let Some(cc) = core_context {
                session
                    .context
                    .set("core_context.text", cc.to_string())
                    .await;
            }
            let checkpoint = RunCheckpoint {
                root: &session,
                children: &[],
            };
            store
                .start_run(
                    &SessionId(session_id.to_string()),
                    &descriptor,
                    checkpoint,
                    &RunStateV1::default(),
                )
                .await?;
            let sid = SessionId(session_id.to_string());
            // Register the run's coordinator cancellation token (A1): the
            // run's prompt consumers resolve their token from the shared
            // per-run map and fail closed when none is registered.
            self.register_cancellation(&sid);
            let runner = Arc::new(FlowRunner::new(graph, self.state.storage.clone()));
            self.state
                .runners
                .write()
                .await
                .insert(session_id.to_string(), runner);
            self.state.sessions.write().await.push(SessionSummary {
                session_id: SessionId(session_id.to_string()),
                creator_id: creator_id.to_string(),
                preset_id: loaded.id.clone(),
                status: SessionStatus::Running,
                current_task_id: Some(start_task_id),
            });
            Ok(SessionId(session_id.to_string()))
        } else {
            // No workflow store (Tier-0/test): fall back to the raw-graph
            // path. Input seeding is a v1-run concern; the raw path has no
            // durable descriptor to freeze.
            self.start_session_with_creator(&loaded.id, graph, Some(creator_id))
                .await
        }
    }

    /// Atomically admit a schedule as a driven v1 run (A3, C-1/C-2).
    ///
    /// One Creator-DB transaction linearizes the schedule claim
    /// (`status='running'`, `current_session_id`, core-context version), the
    /// v1 session row (initial checkpoint + frozen descriptor + seeded
    /// context), and the schedule→session identity. A concurrent admission
    /// for the same schedule loses the claim and returns the winner's
    /// already-owned run (exactly one `SessionId` per schedule).
    ///
    /// The schedule row must carry `execution_policy = 'driven_v1'` and be
    /// in `pending`/`paused` with no owned session. A `_system.*` preset is
    /// refused regardless of stored policy (A8 fail-closed, C-4). A stale
    /// claim (schedule `Running` with a `current_session_id` whose run row
    /// does not exist) is NOT cleared — a crash between claim and run
    /// creation is recovered by boot recovery, never by a second admission
    /// minting another session (C-1).
    ///
    /// `core_context_version` is the actual committed core-context version
    /// (I-10) — the caller seeds core context first and passes the returned
    /// version so the schedule pointer and the version agree.
    ///
    /// # Errors
    /// Returns [`EngineError`] on storage failure, policy refusal, or when
    /// the schedule is not in an admissible state.
    #[allow(clippy::too_many_arguments, clippy::implicit_hasher)] // admit-time parameter bundle; a config struct would obscure the CAS gate coupling
    pub async fn admit_schedule_run_with_input(
        &self,
        schedule_id: &str,
        session_id: &str,
        loaded: &crate::preset::LoadedPreset,
        creator_id: &str,
        work_id: Option<String>,
        input: serde_json::Map<String, serde_json::Value>,
        core_context: Option<&str>,
        core_context_version: u32,
        expected_core_context_version: u32,
        agent_bindings: std::collections::HashMap<String, crate::run_state::AgentBinding>,
        frozen_source: Option<crate::run_state::PresetSourceIdentity>,
        admission_gate: Option<crate::run_state::ScheduleAdmissionGate>,
        graph: Arc<Graph>,
    ) -> Result<SessionId, EngineError> {
        let Some(store) = &self.state.workflow_store else {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(
                    "admit_schedule_run_with_input requires a durable workflow store".to_string(),
                ),
            ));
        };
        let mut descriptor = self.build_descriptor(loaded, creator_id)?;
        descriptor.work_id = work_id;
        descriptor.input = input.clone();
        descriptor.agent_bindings = agent_bindings;
        // N-5/N-5b: the runner is built from the FROZEN source identity
        // persisted with the schedule, never by resolving current registry
        // precedence and discarding the stored identity. The stored identity
        // must match the current load's content hash — a changed/shadowed
        // preset refuses admission instead of running under stale bytes.
        if let Some(frozen) = frozen_source {
            let current = loaded.source_identity.as_ref().ok_or_else(|| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "admit_schedule_run_with_input: preset '{}' has no current source identity",
                    loaded.id
                )))
            })?;
            if current != &frozen {
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "admit_schedule_run_with_input: schedule {schedule_id} frozen \
                         source identity does not match the current preset load \
                         (content changed or shadowed); refusing admission"
                    )),
                ));
            }
            descriptor.source = frozen;
        }
        let start_task_id = graph.start_task_id().unwrap_or_default();
        let session = graph_flow::Session::new_from_task(session_id.to_string(), &start_task_id);
        session
            .context
            .set("_session_id", session_id.to_string())
            .await;
        if !creator_id.is_empty() {
            session
                .context
                .set("_creator_id", creator_id.to_string())
                .await;
        }
        // Seed the frozen admission input + core-context seed into the
        // session context BEFORE the checkpoint is persisted (A3).
        for (key, value) in &input {
            session
                .context
                .set(format!("preset.input.{key}"), value.clone())
                .await;
        }
        if let Some(cc) = core_context {
            session
                .context
                .set("core_context.text", cc.to_string())
                .await;
        }
        let checkpoint = RunCheckpoint {
            root: &session,
            children: &[],
        };
        store
            .admit_schedule_run(
                schedule_id,
                &SessionId(session_id.to_string()),
                &descriptor,
                checkpoint,
                &RunStateV1::default(),
                core_context_version,
                expected_core_context_version,
                admission_gate.as_ref(),
            )
            .await?;
        let sid = SessionId(session_id.to_string());
        // Register the run's coordinator cancellation token (A1): the
        // run's prompt consumers resolve their token from the shared
        // per-run map and fail closed when none is registered.
        self.register_cancellation(&sid);
        let runner = Arc::new(FlowRunner::new(graph, self.state.storage.clone()));
        self.state
            .runners
            .write()
            .await
            .insert(session_id.to_string(), runner);
        self.state.sessions.write().await.push(SessionSummary {
            session_id: SessionId(session_id.to_string()),
            creator_id: creator_id.to_string(),
            preset_id: loaded.id.clone(),
            status: SessionStatus::Running,
            current_task_id: Some(start_task_id),
        });
        Ok(SessionId(session_id.to_string()))
    }

    /// Start a session on a specific graph.
    ///
    /// Creates a [`graph_flow::Session`] seeded at the graph's start task,
    /// stores it, and registers an `Arc<FlowRunner>` for future `run_step` calls.
    ///
    /// Returns the session ID.
    ///
    /// # Errors
    /// Returns [`EngineError`] if session creation, preset loading, or initial step execution fails.
    pub async fn start_session(
        &self,
        preset_id: &str,
        graph: Arc<Graph>,
    ) -> Result<SessionId, EngineError> {
        self.start_session_with_creator(preset_id, graph, None)
            .await
    }

    async fn start_session_with_creator(
        &self,
        preset_id: &str,
        graph: Arc<Graph>,
        creator_id: Option<&str>,
    ) -> Result<SessionId, EngineError> {
        let session_id = format!("{}:{}", preset_id, uuid::Uuid::new_v4());

        // Determine the start task from the graph.
        let start_task_id = graph.start_task_id().unwrap_or_default();

        // Create and persist the session.
        let session = graph_flow::Session::new_from_task(session_id.clone(), &start_task_id);
        // Store session ID in context so InnerGraphTask can find it.
        session.context.set("_session_id", session_id.clone()).await;
        if let Some(creator_id) = creator_id {
            session
                .context
                .set("_creator_id", creator_id.to_string())
                .await;
        }

        // Critical 2: when a workflow store is present, a normal start creates
        // a v1 run (authoritative descriptor + state) via start_run — never a
        // bare SessionStorage::save. The source identity is resolved from the
        // preset (embedded or directory bundle).
        if let Some(store) = &self.state.workflow_store {
            let (source, preset_version) = self.resolve_source_identity(preset_id)?;
            let descriptor = RunDescriptorV1 {
                creator_id: creator_id.unwrap_or_default().to_string(),
                work_id: None,
                workspace_root: self.workspace_root.clone().unwrap_or_default(),
                preset_id: preset_id.to_string(),
                preset_version,
                source,
                input: serde_json::Map::new(),
                agent_bindings: HashMap::new(),
                parent_session_id: None,
                graph_name: None,
            };
            let checkpoint = RunCheckpoint {
                root: &session,
                children: &[],
            };
            store
                .start_run(
                    &SessionId(session_id.clone()),
                    &descriptor,
                    checkpoint,
                    &RunStateV1::default(),
                )
                .await?;
            self.register_cancellation(&SessionId(session_id.clone()));
        } else {
            self.state.storage.save(session).await?;
        }

        // WS2 R3: Create and store Arc<FlowRunner>.
        let runner = Arc::new(FlowRunner::new(graph, self.state.storage.clone()));
        self.state
            .runners
            .write()
            .await
            .insert(session_id.clone(), runner);

        // Track in memory.
        let summary = SessionSummary {
            session_id: SessionId(session_id.clone()),
            creator_id: creator_id.unwrap_or_default().to_string(),
            preset_id: preset_id.to_string(),
            status: SessionStatus::Running,
            current_task_id: Some(start_task_id),
        };

        self.state.sessions.write().await.push(summary);

        Ok(SessionId(session_id))
    }

    /// Resolve the content-addressed source identity and version for a preset
    /// (A2/A7).
    ///
    /// Follows the A7 initial-admission precedence: user directory → system
    /// directory → embedded. When `nexus_home` is set, `resolve_preset` is
    /// tried first (it already applies user → system → embedded order), so a
    /// directory preset that shadows an embedded preset with the same id is
    /// resolved to the directory bundle — never the embedded source. Only
    /// when no `nexus_home` is configured (in-memory/test engines) does this
    /// fall back to the embedded preset directly.
    ///
    /// Returns the source identity **and** the resolved preset's version so
    /// the frozen descriptor carries the real version, not a hard-coded 0
    /// (Important 3).
    /// Recovery requires this exact frozen identity. Replacing an embedded
    /// preset's bytes or changing a directory bundle after admission changes
    /// its content hash and intentionally makes the stored run
    /// reconstruction-unavailable; recovery never substitutes current preset
    /// content for the admitted source.
    fn resolve_source_identity(
        &self,
        preset_id: &str,
    ) -> Result<(PresetSourceIdentity, u32), EngineError> {
        let caps = self.current_caps();
        // Directory bundle under nexus_home (user → system → embedded via
        // resolve_preset's own precedence). A directory preset shadows an
        // embedded preset with the same id (A7).
        if let Some(home) = &self.nexus_home {
            if let Ok(loaded) = crate::preset::resolve_preset(preset_id, home, &caps) {
                if let Some(identity) = loaded.source_identity {
                    return Ok((identity, loaded.version));
                }
            }
        }
        // No nexus_home (or resolve_preset failed): fall back to embedded.
        if let Ok(loaded) = crate::preset::load_embedded_preset(preset_id, &caps) {
            if let Some(identity) = loaded.source_identity {
                return Ok((identity, loaded.version));
            }
        }
        Err(EngineError::GraphFlow(graph_flow::GraphError::StorageError(
            format!(
                "cannot resolve source identity for preset '{preset_id}' (not embedded and no directory bundle)"
            ),
        )))
    }
}

#[async_trait]
impl OrchestrationEngine for GraphFlowEngine {
    async fn run_step(&self, session_id: &SessionId) -> Result<StepOutcome, EngineError> {
        // Delegate to shared state (WS3 R1: uses Arc<FlowRunner> internally).
        let result = self.state.run_step_internal(session_id).await?;

        // Translate graph-flow ExecutionResult to our StepOutcome.
        let outcome = match &result.status {
            ExecutionStatus::Completed => StepOutcome::Completed {
                response: result.response,
            },
            ExecutionStatus::Paused {
                next_task_id,
                reason,
            } => StepOutcome::Paused {
                next_task_id: next_task_id.clone(),
                reason: reason.clone(),
            },
            ExecutionStatus::WaitingForInput => StepOutcome::WaitingForInput {
                response: result.response,
            },
            ExecutionStatus::Error(msg) => StepOutcome::Error(msg.clone()),
        };

        Ok(outcome)
    }

    async fn new_session(&self, key: SessionKey, _ctx: Context) -> Result<SessionId, EngineError> {
        let session_id = format!("{}:{}", key.preset_id, key.instance_id);

        // Persist a session stub into the graph-flow storage.
        let session = graph_flow::Session::new_from_task(session_id.clone(), "");
        session.context.set("_session_id", session_id.clone()).await;
        session
            .context
            .set("_creator_id", key.creator_id.clone())
            .await;
        if let Some(store) = &self.state.workflow_store {
            let (source, preset_version) = self.resolve_source_identity(&key.preset_id)?;
            let descriptor = RunDescriptorV1 {
                creator_id: key.creator_id.clone(),
                work_id: None,
                workspace_root: self.workspace_root.clone().unwrap_or_default(),
                preset_id: key.preset_id.clone(),
                preset_version,
                source,
                input: serde_json::Map::new(),
                agent_bindings: HashMap::new(),
                parent_session_id: None,
                graph_name: None,
            };
            store
                .start_run(
                    &SessionId(session_id.clone()),
                    &descriptor,
                    RunCheckpoint {
                        root: &session,
                        children: &[],
                    },
                    &RunStateV1::default(),
                )
                .await?;
            self.register_cancellation(&SessionId(session_id.clone()));
        } else {
            self.state.storage.save(session).await?;
        }

        let summary = SessionSummary {
            session_id: SessionId(session_id.clone()),
            creator_id: key.creator_id,
            preset_id: key.preset_id,
            status: SessionStatus::Running,
            current_task_id: None,
        };

        self.state.sessions.write().await.push(summary);

        Ok(SessionId(session_id))
    }

    async fn start_session_with_graph(
        &self,
        id_prefix: &str,
        graph: Arc<Graph>,
    ) -> Result<SessionId, EngineError> {
        self.start_session(id_prefix, graph).await
    }

    async fn get_status(&self, session_id: &SessionId) -> Result<SessionStatus, EngineError> {
        let sessions = self.state.sessions.read().await;
        sessions
            .iter()
            .find(|s| s.session_id == *session_id)
            .map(|s| s.status.clone())
            .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))
    }

    async fn signal(
        &self,
        session_id: &SessionId,
        signal: EngineSignal,
    ) -> Result<(), EngineError> {
        // Persist the transition authoritatively (A2/A5) when a workflow
        // store is present: Cancel → Cancelled, Pause → Paused, Resume →
        // Running (fenced against terminal states). Memory-only status
        // flips are no longer the SSOT (Critical 3).
        let target_status = self
            .state
            .persist_signal_transition(session_id, &signal)
            .await?;

        let mut sessions = self.state.sessions.write().await;
        if let Some(s) = sessions.iter_mut().find(|s| s.session_id == *session_id) {
            s.status = target_status;
            Ok(())
        } else {
            Err(EngineError::SessionNotFound(session_id.0.clone()))
        }
    }

    async fn list_active(&self, filter: SessionFilter) -> Result<Vec<SessionSummary>, EngineError> {
        let sessions = self.state.sessions.read().await;
        Ok(sessions
            .iter()
            .filter(|s| {
                let status_ok = matches!(
                    s.status,
                    SessionStatus::Running | SessionStatus::Paused | SessionStatus::WaitingForInput
                );
                let creator_ok = filter
                    .creator_id
                    .as_ref()
                    .is_none_or(|c| c == &s.creator_id);
                let preset_ok = filter.preset_id.as_ref().is_none_or(|p| p == &s.preset_id);
                status_ok && creator_ok && preset_ok
            })
            .cloned()
            .collect())
    }

    async fn spawn_child_session(
        &self,
        params: ChildSessionParams,
    ) -> Result<SessionId, EngineError> {
        self.state.spawn_child_session_internal(params).await
    }

    async fn attach_existing_child_session(
        &self,
        parent_session_id: &str,
        inner_graph: Arc<Graph>,
    ) -> Result<Option<SessionId>, EngineError> {
        self.state
            .attach_existing_child_session_internal(parent_session_id, inner_graph)
            .await
    }

    async fn get_context(
        &self,
        session_id: &SessionId,
    ) -> Result<graph_flow::Context, EngineError> {
        let session = self
            .state
            .storage
            .get(&session_id.0)
            .await
            .map_err(EngineError::GraphFlow)?
            .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))?;
        Ok(session.context)
    }

    async fn get_current_task_id(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<String>, EngineError> {
        EngineSharedState::current_task_id(&self.state, session_id).await
    }

    async fn has_runner(&self, session_id: &SessionId) -> bool {
        self.state.runners.read().await.contains_key(&session_id.0)
    }

    async fn recover_sessions(&self, summaries: Vec<SessionSummary>) {
        self.recover_sessions_inner(summaries).await;
    }

    async fn ensure_recovered_runner(&self, session_id: &SessionId) -> Result<(), EngineError> {
        self.ensure_recovered_runner_inner(session_id).await
    }

    async fn start_session_with_preset(
        &self,
        loaded: &crate::preset::LoadedPreset,
    ) -> Result<SessionId, EngineError> {
        // WS3 R1: Use EngineProxy wrapping EngineSharedState.
        let proxy: Arc<dyn OrchestrationEngine> = Arc::new(EngineProxy {
            state: self.state.clone(),
        });
        let caps = self.current_caps();
        let wired = crate::preset::loader::build_wired_outer_graph(
            loaded,
            &proxy,
            &caps,
            self.daemon_tool_dispatch.clone(),
            self.prompt_executor.clone(),
            self.session_cancels.clone(),
        );
        self.start_preset_run(loaded, "", Arc::new(wired)).await
    }

    async fn start_session_with_preset_for_creator(
        &self,
        loaded: &crate::preset::LoadedPreset,
        creator_id: &str,
    ) -> Result<SessionId, EngineError> {
        let proxy: Arc<dyn OrchestrationEngine> = Arc::new(EngineProxy {
            state: self.state.clone(),
        });
        let caps = self.current_caps();
        let wired = crate::preset::loader::build_wired_outer_graph(
            loaded,
            &proxy,
            &caps,
            self.daemon_tool_dispatch.clone(),
            self.prompt_executor.clone(),
            self.session_cancels.clone(),
        );
        self.start_preset_run(loaded, creator_id, Arc::new(wired))
            .await
    }
}

// Re-export EngineSharedState for consumers (e.g., preset loader).
pub use EngineSharedState as SharedState;

/// Scripted [`PromptExecutor`] for the A5 cleanup-retry tests: `finalize_run`
/// pops from a scripted queue of outcomes and records every call's run id.
#[cfg(test)]
struct ScriptedFinalizeExecutor {
    outcomes: std::sync::Mutex<std::collections::VecDeque<Result<(), CapabilityError>>>,
    calls: std::sync::atomic::AtomicU64,
    finalized_runs: std::sync::Mutex<Vec<String>>,
}

#[cfg(test)]
impl ScriptedFinalizeExecutor {
    fn new(outcomes: Vec<Result<(), CapabilityError>>) -> Self {
        Self {
            outcomes: std::sync::Mutex::new(outcomes.into()),
            calls: std::sync::atomic::AtomicU64::new(0),
            finalized_runs: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> u64 {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Run ids passed to `finalize_run`, in call order.
    fn finalized_runs(&self) -> Vec<String> {
        self.finalized_runs.lock().expect("finalized_runs").clone()
    }
}

#[cfg(test)]
#[async_trait]
impl crate::capability::PromptExecutor for ScriptedFinalizeExecutor {
    async fn execute(
        &self,
        _request: crate::capability::PromptRequest,
    ) -> Result<crate::capability::PromptResult, CapabilityError> {
        unreachable!("not used by cancel-path tests")
    }

    async fn finalize_run(&self, run_id: &str) -> Result<(), CapabilityError> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.finalized_runs
            .lock()
            .expect("finalized_runs")
            .push(run_id.to_string());
        self.outcomes
            .lock()
            .expect("outcomes")
            .pop_front()
            .unwrap_or(Ok(()))
    }
}

/// Store wrapper that makes the FIRST `Interrupted` commit lose its revision
/// CAS to a competing transition (Finding 2): the competing transition wins
/// the same revision first, so the engine's bounded retry must reload and
/// re-commit against the new revision. Every later commit passes through.
#[cfg(test)]
struct InterruptedCasInjectingStore {
    inner: Arc<dyn WorkflowStateStore>,
    storage: Arc<dyn SessionStorage>,
    injected: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl InterruptedCasInjectingStore {
    fn new(inner: Arc<dyn WorkflowStateStore>, storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            inner,
            storage,
            injected: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl WorkflowStateStore for InterruptedCasInjectingStore {
    async fn load_run(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<crate::run_state::RunRecord>, EngineError> {
        self.inner.load_run(session_id).await
    }

    async fn start_run(
        &self,
        session_id: &SessionId,
        descriptor: &crate::run_state::RunDescriptorV1,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_state: &crate::run_state::RunStateV1,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        self.inner
            .start_run(session_id, descriptor, checkpoint, next_state)
            .await
    }

    async fn admit_schedule_run(
        &self,
        schedule_id: &str,
        session_id: &SessionId,
        descriptor: &crate::run_state::RunDescriptorV1,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_state: &crate::run_state::RunStateV1,
        core_context_version: u32,
        expected_core_context_version: u32,
        admission_gate: Option<&crate::run_state::ScheduleAdmissionGate>,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        self.inner
            .admit_schedule_run(
                schedule_id,
                session_id,
                descriptor,
                checkpoint,
                next_state,
                core_context_version,
                expected_core_context_version,
                admission_gate,
            )
            .await
    }

    async fn commit_transition(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &crate::run_state::RunStateV1,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        if next_status == SessionStatus::Interrupted
            && !self
                .injected
                .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            // The competing transition wins the CAS first: a concurrent
            // transition commits at the SAME revision the Interrupted write
            // is about to lose.
            let record = self
                .inner
                .load_run(session_id)
                .await?
                .expect("run exists at interrupted race gate");
            let root = self
                .storage
                .get(&session_id.0)
                .await
                .expect("root session at interrupted race gate")
                .expect("root exists at interrupted race gate");
            let mut competing_state = record.state.unwrap_or_default();
            competing_state.cancel_requested = true;
            self.inner
                .commit_transition(
                    session_id,
                    record.state_revision,
                    crate::run_state::RunCheckpoint {
                        root: &root,
                        children: &[],
                    },
                    record.status.clone(),
                    &competing_state,
                )
                .await
                .expect("competing transition wins the CAS");
        }
        self.inner
            .commit_transition(
                session_id,
                expected_revision,
                checkpoint,
                next_status,
                next_state,
            )
            .await
    }

    async fn settle_cancelled(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_state: &crate::run_state::RunStateV1,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        self.inner
            .settle_cancelled(session_id, expected_revision, checkpoint, next_state)
            .await
    }

    async fn restore_pre_step(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        pre_step: &graph_flow::Session,
    ) -> Result<(), EngineError> {
        self.inner
            .restore_pre_step(session_id, expected_revision, pre_step)
            .await
    }

    async fn mark_step_in_flight(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        step_state: &crate::run_state::RunStateV1,
    ) -> Result<(), EngineError> {
        self.inner
            .mark_step_in_flight(session_id, expected_revision, checkpoint, step_state)
            .await
    }

    async fn persist_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        expected_attempt_id: Option<&str>,
        attempt: &crate::run_state::PromptAttempt,
    ) -> Result<(), EngineError> {
        self.inner
            .persist_prompt_attempt(
                session_id,
                expected_revision,
                expected_step,
                expected_attempt_id,
                attempt,
            )
            .await
    }

    async fn clear_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        attempt_id: &str,
    ) -> Result<(), EngineError> {
        self.inner
            .clear_prompt_attempt(session_id, expected_revision, expected_step, attempt_id)
            .await
    }

    async fn load_children(
        &self,
        parent_session_id: &SessionId,
    ) -> Result<Vec<crate::run_state::RunRecord>, EngineError> {
        self.inner.load_children(parent_session_id).await
    }
}

/// Store wrapper that injects a Continue winner BETWEEN Cancel's initial
/// read and its phase-1 fence commit (Finding 3, round 4): the FIRST
/// `commit_transition` carrying the durable cancel intent (the phase-1
/// fence) is pre-empted by a competing Continue that consumes the wait and
/// commits a NEWER root position/context at the SAME revision. Cancel's
/// fence then loses the CAS, reloads the winner row/root, and re-fences
/// against the new revision. Every later commit passes through.
///
/// This is the deterministic Continue-v-cancel interleaving the round-3
/// regression did not create: the Continue genuinely wins BEFORE the cancel
/// fence, so the settlement must reload the authoritative root — the stale
/// pre-fence root would clobber the winner's position/context.
#[cfg(test)]
struct ContinueWinsBeforeCancelFenceStore {
    inner: Arc<dyn WorkflowStateStore>,
    storage: Arc<dyn SessionStorage>,
    injected: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl ContinueWinsBeforeCancelFenceStore {
    fn new(inner: Arc<dyn WorkflowStateStore>, storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            inner,
            storage,
            injected: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Whether the Continue winner was injected (the interleaving actually
    /// happened — the cancel fence genuinely lost the CAS).
    fn injected(&self) -> bool {
        self.injected.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
#[async_trait]
impl WorkflowStateStore for ContinueWinsBeforeCancelFenceStore {
    async fn load_run(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<crate::run_state::RunRecord>, EngineError> {
        self.inner.load_run(session_id).await
    }

    async fn start_run(
        &self,
        session_id: &SessionId,
        descriptor: &crate::run_state::RunDescriptorV1,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_state: &crate::run_state::RunStateV1,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        self.inner
            .start_run(session_id, descriptor, checkpoint, next_state)
            .await
    }

    async fn admit_schedule_run(
        &self,
        schedule_id: &str,
        session_id: &SessionId,
        descriptor: &crate::run_state::RunDescriptorV1,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_state: &crate::run_state::RunStateV1,
        core_context_version: u32,
        expected_core_context_version: u32,
        admission_gate: Option<&crate::run_state::ScheduleAdmissionGate>,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        self.inner
            .admit_schedule_run(
                schedule_id,
                session_id,
                descriptor,
                checkpoint,
                next_state,
                core_context_version,
                expected_core_context_version,
                admission_gate,
            )
            .await
    }

    async fn commit_transition(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &crate::run_state::RunStateV1,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        // The phase-1 cancel fence is the first commit carrying the durable
        // cancel intent on a non-terminal status. Inject the Continue winner
        // at the SAME revision BEFORE the fence commits, so the fence loses
        // the CAS and must reload/re-fence against the winner.
        if next_state.cancel_requested
            && next_status != SessionStatus::Interrupted
            && !self
                .injected
                .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            let record = self
                .inner
                .load_run(session_id)
                .await?
                .expect("run exists at continue-v-cancel gate");
            let mut winner_root = self
                .storage
                .get(&session_id.0)
                .await
                .expect("root session at continue-v-cancel gate")
                .expect("root exists at continue-v-cancel gate");
            winner_root.current_task_id = "winner-task".to_string();
            winner_root
                .context
                .set("winner.marker", "continue-won")
                .await;
            self.inner
                .commit_transition(
                    session_id,
                    record.state_revision,
                    crate::run_state::RunCheckpoint {
                        root: &winner_root,
                        children: &[],
                    },
                    SessionStatus::Running,
                    &crate::run_state::RunStateV1::default(),
                )
                .await
                .expect("Continue wins the CAS before the cancel fence");
        }
        self.inner
            .commit_transition(
                session_id,
                expected_revision,
                checkpoint,
                next_status,
                next_state,
            )
            .await
    }

    async fn settle_cancelled(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_state: &crate::run_state::RunStateV1,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        self.inner
            .settle_cancelled(session_id, expected_revision, checkpoint, next_state)
            .await
    }

    async fn restore_pre_step(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        pre_step: &graph_flow::Session,
    ) -> Result<(), EngineError> {
        self.inner
            .restore_pre_step(session_id, expected_revision, pre_step)
            .await
    }

    async fn mark_step_in_flight(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        step_state: &crate::run_state::RunStateV1,
    ) -> Result<(), EngineError> {
        self.inner
            .mark_step_in_flight(session_id, expected_revision, checkpoint, step_state)
            .await
    }

    async fn persist_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        expected_attempt_id: Option<&str>,
        attempt: &crate::run_state::PromptAttempt,
    ) -> Result<(), EngineError> {
        self.inner
            .persist_prompt_attempt(
                session_id,
                expected_revision,
                expected_step,
                expected_attempt_id,
                attempt,
            )
            .await
    }

    async fn clear_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        attempt_id: &str,
    ) -> Result<(), EngineError> {
        self.inner
            .clear_prompt_attempt(session_id, expected_revision, expected_step, attempt_id)
            .await
    }

    async fn load_children(
        &self,
        parent_session_id: &SessionId,
    ) -> Result<Vec<crate::run_state::RunRecord>, EngineError> {
        self.inner.load_children(parent_session_id).await
    }
}

/// Store wrapper that makes the FIRST child-rollback `settle_cancelled`
/// lose its revision CAS to a competing transition (P1, fix round 6): the
/// competing transition advances the child row at the SAME revision BEFORE
/// the rollback settle commits, so the settle returns `RevisionMismatch`
/// and the engine's rollback retry must reload the child record and
/// re-settle against the CURRENT revision. Every later call passes through.
#[cfg(test)]
struct ChildSettleCasLossStore {
    inner: Arc<dyn WorkflowStateStore>,
    storage: Arc<dyn SessionStorage>,
    injected: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl ChildSettleCasLossStore {
    fn new(inner: Arc<dyn WorkflowStateStore>, storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            inner,
            storage,
            injected: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Whether the competing transition was injected (the rollback settle
    /// genuinely lost the CAS).
    fn injected(&self) -> bool {
        self.injected.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
#[async_trait]
impl WorkflowStateStore for ChildSettleCasLossStore {
    async fn load_run(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<crate::run_state::RunRecord>, EngineError> {
        self.inner.load_run(session_id).await
    }

    async fn start_run(
        &self,
        session_id: &SessionId,
        descriptor: &crate::run_state::RunDescriptorV1,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_state: &crate::run_state::RunStateV1,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        self.inner
            .start_run(session_id, descriptor, checkpoint, next_state)
            .await
    }

    async fn admit_schedule_run(
        &self,
        schedule_id: &str,
        session_id: &SessionId,
        descriptor: &crate::run_state::RunDescriptorV1,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_state: &crate::run_state::RunStateV1,
        core_context_version: u32,
        expected_core_context_version: u32,
        admission_gate: Option<&crate::run_state::ScheduleAdmissionGate>,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        self.inner
            .admit_schedule_run(
                schedule_id,
                session_id,
                descriptor,
                checkpoint,
                next_state,
                core_context_version,
                expected_core_context_version,
                admission_gate,
            )
            .await
    }

    async fn commit_transition(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &crate::run_state::RunStateV1,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        self.inner
            .commit_transition(
                session_id,
                expected_revision,
                checkpoint,
                next_status,
                next_state,
            )
            .await
    }

    async fn settle_cancelled(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        next_state: &crate::run_state::RunStateV1,
    ) -> Result<crate::run_state::RunRecord, EngineError> {
        // The child-rollback settle is the first `settle_cancelled` on the
        // freshly started child row. Inject a competing transition at the
        // SAME revision BEFORE the settle commits, so the settle loses the
        // CAS and the engine's rollback retry must reload and re-settle
        // against the new revision.
        if !self
            .injected
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            let record = self
                .inner
                .load_run(session_id)
                .await?
                .expect("child exists at rollback settle gate");
            let mut winner_root = self
                .storage
                .get(&session_id.0)
                .await
                .expect("child session at rollback settle gate")
                .expect("child session exists at rollback settle gate");
            winner_root.current_task_id = "competing-winner".to_string();
            winner_root
                .context
                .set("competing.marker", "advanced")
                .await;
            self.inner
                .commit_transition(
                    session_id,
                    record.state_revision,
                    crate::run_state::RunCheckpoint {
                        root: &winner_root,
                        children: &[],
                    },
                    SessionStatus::Running,
                    &crate::run_state::RunStateV1::default(),
                )
                .await
                .expect("competing transition wins the CAS before the child settle");
        }
        self.inner
            .settle_cancelled(session_id, expected_revision, checkpoint, next_state)
            .await
    }

    async fn restore_pre_step(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        pre_step: &graph_flow::Session,
    ) -> Result<(), EngineError> {
        self.inner
            .restore_pre_step(session_id, expected_revision, pre_step)
            .await
    }

    async fn mark_step_in_flight(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: crate::run_state::RunCheckpoint<'_>,
        step_state: &crate::run_state::RunStateV1,
    ) -> Result<(), EngineError> {
        self.inner
            .mark_step_in_flight(session_id, expected_revision, checkpoint, step_state)
            .await
    }

    async fn persist_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        expected_attempt_id: Option<&str>,
        attempt: &crate::run_state::PromptAttempt,
    ) -> Result<(), EngineError> {
        self.inner
            .persist_prompt_attempt(
                session_id,
                expected_revision,
                expected_step,
                expected_attempt_id,
                attempt,
            )
            .await
    }

    async fn clear_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        attempt_id: &str,
    ) -> Result<(), EngineError> {
        self.inner
            .clear_prompt_attempt(session_id, expected_revision, expected_step, attempt_id)
            .await
    }

    async fn load_children(
        &self,
        parent_session_id: &SessionId,
    ) -> Result<Vec<crate::run_state::RunRecord>, EngineError> {
        self.inner.load_children(parent_session_id).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::too_many_lines)] // deterministic race/settlement scenarios keep setup+assertions linear
    #![allow(clippy::await_holding_lock)] // test fixtures hold sync mutex guards across awaits to freeze state
    #![allow(clippy::significant_drop_tightening)] // tests read state snapshots; early-drop noise without contention value

    use super::*;
    use crate::storage::SqliteSessionStorage;
    use graph_flow::InMemorySessionStorage;

    /// Helper: create a test engine with in-memory storage and built-in caps.
    fn test_engine() -> GraphFlowEngine {
        let storage: Arc<dyn SessionStorage> = Arc::new(InMemorySessionStorage::new());
        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        GraphFlowEngine::new_with_storage(storage, caps)
    }

    /// P3 T1 rereview-4 P1 (A7 rule 2): a NON-TERMINAL descendant must name
    /// the inner graph entered by its parent's current position — a valid but
    /// wrong inner-graph name is non-replayable (it would miss reattachment
    /// and mint a replacement child).
    #[test]
    fn descendant_position_validation_rejects_valid_but_wrong_inner_graph() {
        use std::collections::HashSet;
        let valid: HashSet<&str> = ["graph_a", "graph_b"].into_iter().collect();

        validate_descendant_position(
            "preset",
            &valid,
            Some("graph_a"),
            true,
            None,
            "parent",
            "s1",
            "child",
            Some("graph_a"),
            false,
        )
        .expect("matching non-terminal child is replayable");

        let err = validate_descendant_position(
            "preset",
            &valid,
            Some("graph_a"),
            true,
            None,
            "parent",
            "s1",
            "child",
            Some("graph_b"),
            false,
        )
        .expect_err("valid-but-wrong-position must refuse");
        assert!(err.to_string().contains("non-replayable"), "got {err}");

        let err = validate_descendant_position(
            "preset",
            &valid,
            None,
            true,
            None,
            "parent",
            "s1",
            "child",
            Some("graph_c"),
            true,
        )
        .expect_err("unknown inner graph must refuse even when terminal");
        assert!(err.to_string().contains("unknown inner graph"), "got {err}");

        validate_descendant_position(
            "preset",
            &valid,
            Some("graph_a"),
            true,
            None,
            "parent",
            "s1",
            "child",
            Some("graph_b"),
            true,
        )
        .expect("terminal child is historical: membership suffices");

        validate_descendant_position(
            "preset",
            &valid,
            None,
            true,
            Some("graph_a"),
            "parent",
            "s2",
            "child",
            Some("graph_a"),
            false,
        )
        .expect("a child recorded under a state entering its own graph is tolerated");

        let err = validate_descendant_position(
            "preset",
            &valid,
            None,
            true,
            Some("graph_a"),
            "parent",
            "s2",
            "child",
            Some("graph_b"),
            false,
        )
        .expect_err("a recorded state entering a different graph must refuse");
        assert!(
            err.to_string()
                .contains("not recorded by an inner-graph state entering"),
            "got {err}"
        );

        let err = validate_descendant_position(
            "preset",
            &valid,
            None,
            true,
            None,
            "parent",
            "s2",
            "child",
            Some("graph_a"),
            false,
        )
        .expect_err("an unrecorded past-position child must refuse (loop-back replay)");
        assert!(
            err.to_string()
                .contains("not recorded by an inner-graph state entering"),
            "got {err}"
        );

        // Inner graph nodes cannot enter inner graphs: a live descendant
        // under a non-root parent is non-replayable.
        let err = validate_descendant_position(
            "preset",
            &valid,
            Some("graph_a"),
            false,
            None,
            "child-parent",
            "n1",
            "grandchild",
            Some("graph_a"),
            false,
        )
        .expect_err("live descendant under an inner-graph session must refuse");
        assert!(
            err.to_string().contains("nested inner graphs do not exist"),
            "got {err}"
        );
    }

    #[tokio::test]
    async fn sec_v131_01_creator_start_seeds_trusted_creator_context() {
        let engine = test_engine();
        let graph = Arc::new(Graph::new("creator-context"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));

        let session_id = engine
            .start_session_with_creator("creator-context", graph, Some("creator_alice"))
            .await
            .expect("creator-aware session start should succeed");

        let ctx = engine
            .get_context(&session_id)
            .await
            .expect("session context should be persisted");
        let creator_id: String = ctx
            .get("_creator_id")
            .await
            .expect("trusted creator id should be seeded");
        let seeded_session_id: String = ctx
            .get("_session_id")
            .await
            .expect("trusted session id should be seeded");

        assert_eq!(creator_id, "creator_alice");
        assert_eq!(seeded_session_id, session_id.0);
    }

    // ---------- R6: Session recovery reconstructs FlowRunner ----------

    #[tokio::test]
    async fn r6_recovered_session_has_runner_no_graph_loaded_fix() {
        // Before the R6 fix, recover_sessions() only added summaries to the
        // in-memory tracker but did NOT reconstruct FlowRunners. Calling
        // run_step() on a recovered session would fail with NoGraphLoaded.
        //
        // With the R6 fix, recover_sessions() loads the embedded preset and
        // reconstructs the FlowRunner. However, this only works for sessions
        // started with known embedded presets (e.g., "novel-writing").
        //
        // For sessions with unknown presets, reconstruct_runner logs a warning
        // and the runner is not created. The session remains in the tracker
        // but run_step will fail — this is expected behavior for unknown presets.

        let engine = test_engine();

        // Create a session summary with an unknown preset (simulating a session
        // that was started with a preset not available as embedded).
        let summary = SessionSummary {
            session_id: SessionId("test:unknown-preset-session".to_string()),
            creator_id: "test-creator".to_string(),
            preset_id: "nonexistent-preset".to_string(),
            status: SessionStatus::Paused,
            current_task_id: Some("gathering".to_string()),
        };

        // Recover should not panic even with unknown preset
        engine.recover_sessions(vec![summary.clone()]).await;

        // Session should be in the tracker
        let active = engine.list_active(SessionFilter::default()).await.unwrap();
        assert!(
            active.iter().any(|s| s.session_id == summary.session_id),
            "recovered session should be in active list"
        );

        // run_step should fail because no runner was reconstructed for
        // unknown preset (this is the expected degraded behavior).
        let result = engine.run_step(&summary.session_id).await;
        assert!(
            result.is_err(),
            "run_step should fail for unknown preset recovery (NoGraphLoaded)"
        );
    }

    #[tokio::test]
    async fn r6_recovered_session_with_known_preset_has_runner() {
        // Test that recovery works for sessions started with known embedded presets.
        let storage: Arc<dyn SessionStorage> = Arc::new(InMemorySessionStorage::new());
        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let engine = GraphFlowEngine::new_with_storage(storage.clone(), caps);

        // Create a session summary that matches an embedded preset
        let summary = SessionSummary {
            session_id: SessionId("novel-writing:1234567890".to_string()),
            creator_id: "test-creator".to_string(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Paused,
            current_task_id: Some("gathering".to_string()),
        };

        // Simulate a persisted session in storage (even though it's minimal,
        // the FlowRunner reconstruction should succeed)
        let session = graph_flow::Session::new_from_task(
            summary.session_id.0.clone(),
            summary.current_task_id.as_deref().unwrap_or(""),
        );
        storage.save(session).await.unwrap();

        // Recover — should reconstruct runner from embedded "novel-writing" preset
        engine.recover_sessions(vec![summary.clone()]).await;

        // Session should be in the tracker
        let active = engine.list_active(SessionFilter::default()).await.unwrap();
        assert!(
            active.iter().any(|s| s.session_id == summary.session_id),
            "recovered session should be in active list"
        );

        // The runner should exist now — run_step should NOT fail with NoGraphLoaded.
        // (It may fail for other reasons if the session state is minimal,
        // but it should not be NoGraphLoaded.)
        let result = engine.run_step(&summary.session_id).await;
        assert!(
            !matches!(result, Err(EngineError::NoGraphLoaded)),
            "R6 regression: run_step returned NoGraphLoaded for recovered session with known preset"
        );
    }

    #[tokio::test]
    async fn has_runner_reflects_reconstruction_success() {
        // BL-04 slice (T2): the resume re-drive must not step a session
        // whose runner failed reconstruction (user presets stay
        // tracked-but-not-driven). `has_runner` is the seam.
        let storage: Arc<dyn SessionStorage> = Arc::new(InMemorySessionStorage::new());
        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let engine = GraphFlowEngine::new_with_storage(storage.clone(), caps);

        // Unknown preset: reconstruction fails -> no runner.
        let unknown = SessionSummary {
            session_id: SessionId("test:unknown".to_string()),
            creator_id: "test-creator".to_string(),
            preset_id: "nonexistent-preset".to_string(),
            status: SessionStatus::Paused,
            current_task_id: Some("gathering".to_string()),
        };
        engine.recover_sessions(vec![unknown.clone()]).await;
        assert!(
            !engine.has_runner(&unknown.session_id).await,
            "a session whose runner failed reconstruction has no runner"
        );

        // Known embedded preset: reconstruction succeeds -> runner present.
        let known = SessionSummary {
            session_id: SessionId("novel-writing:1234567890".to_string()),
            creator_id: "test-creator".to_string(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Paused,
            current_task_id: Some("gathering".to_string()),
        };
        let session = graph_flow::Session::new_from_task(
            known.session_id.0.clone(),
            known.current_task_id.as_deref().unwrap_or(""),
        );
        storage.save(session).await.unwrap();
        engine.recover_sessions(vec![known.clone()]).await;
        assert!(
            engine.has_runner(&known.session_id).await,
            "a session whose runner was reconstructed has a runner"
        );
    }

    #[tokio::test]
    async fn r6_terminal_sessions_skipped_during_recovery() {
        let engine = test_engine();

        let terminal_summary = SessionSummary {
            session_id: SessionId("test:completed-session".to_string()),
            creator_id: "test-creator".to_string(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Completed,
            current_task_id: None,
        };

        // Recovery should skip terminal sessions for runner reconstruction
        engine
            .recover_sessions(vec![terminal_summary.clone()])
            .await;

        // Terminal session should NOT be in active list
        let active = engine.list_active(SessionFilter::default()).await.unwrap();
        assert!(
            active.is_empty(),
            "terminal sessions should not appear in active list"
        );

        // No runner should have been reconstructed for the terminal session.
        // The runners map should not contain the terminal session ID.
        assert!(
            !engine
                .state
                .runners
                .read()
                .await
                .contains_key(&terminal_summary.session_id.0),
            "terminal session should not have a reconstructed runner"
        );
    }

    // ------------------------------------------------------------------
    // T2 fix round 1 — Finding 1: cancel fence reload/retry
    // ------------------------------------------------------------------

    /// Deterministic mark-step/cancel race (Finding 1): the step loop wins
    /// `mark_step_in_flight` immediately before the cancel fence commits.
    /// The cancel must reload the authoritative row, re-fence against the
    /// new revision, fire the token, and settle `cancelled` — the active
    /// Host work is always reached.
    #[tokio::test]
    async fn cancel_fence_reloads_after_mark_step_in_flight_wins() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        // Production-shaped Host executor (Finding 4): the cancel path must
        // invoke `finalize_run` (bounded owned-Host teardown) after the
        // re-fenced cancel intent — the token-only check below is the
        // lower-level fence assertion, and the executor call is the
        // load-bearing external side effect.
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(())]));
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        // Start a v1 run through the engine (registers the run token in the
        // shared map the cancel path reads).
        let graph = Arc::new(Graph::new("cancel-race"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // The step loop captures revision R and wins `mark_step_in_flight`
        // (advancing to R+1) BEFORE the cancel fence commits against R.
        // Simulate the winner's write directly through the store.
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
        let in_flight_state = RunStateV1 {
            step_in_flight: Some(pre_step_root.current_task_id.clone()),
            ..RunStateV1::default()
        };
        store
            .mark_step_in_flight(
                &session_id,
                record.state_revision,
                RunCheckpoint {
                    root: &pre_step_root,
                    children: &[],
                },
                &in_flight_state,
            )
            .await
            .expect("mark_step_in_flight wins the CAS");

        // Register a token in the shared map so the cancel path can fire it.
        engine
            .state
            .session_cancels
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(session_id.0.clone())
            .or_default();
        let token = engine
            .state
            .session_cancels
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&session_id.0)
            .cloned()
            .expect("token registered");

        // Cancel: the phase-1 fence loses the CAS (revision moved), reloads
        // the authoritative row, re-fences against the new revision, fires
        // the token, and settles cancelled.
        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("cancel must reload and retry the fence");
        assert_eq!(status, SessionStatus::Cancelled);
        assert!(
            token.is_cancelled(),
            "the coordinator token must fire after the re-fenced cancel intent"
        );
        assert_eq!(
            executor.finalized_runs(),
            vec![session_id.0.clone()],
            "the production-shaped Host executor must reap the run after the re-fenced cancel"
        );

        // The run is durably cancelled with the cancel intent persisted.
        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(final_record.status, SessionStatus::Cancelled);
        assert!(
            final_record
                .state
                .as_ref()
                .is_some_and(|s| s.cancel_requested),
            "cancelled run must carry cancel_requested"
        );
    }

    /// Deterministic terminal-v-cancel race (Finding 1): the run reaches a
    /// terminal state while the cancel fence is in flight. The cancel must
    /// surface a deliberate conflict, never silent success.
    #[tokio::test]
    async fn cancel_fence_terminal_observation_is_conflict() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );

        let graph = Arc::new(Graph::new("cancel-terminal"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // The step loop wins `mark_step_in_flight` (revision R → R+1) and
        // then completes the run (R+1 → R+2, terminal `completed`) before
        // the cancel fence commits against R.
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
        store
            .mark_step_in_flight(
                &session_id,
                record.state_revision,
                RunCheckpoint {
                    root: &pre_step_root,
                    children: &[],
                },
                &RunStateV1 {
                    step_in_flight: Some(pre_step_root.current_task_id.clone()),
                    ..RunStateV1::default()
                },
            )
            .await
            .expect("mark_step_in_flight wins the CAS");
        let stepped = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let completed_root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        store
            .commit_transition(
                &session_id,
                stepped.state_revision,
                RunCheckpoint {
                    root: &completed_root,
                    children: &[],
                },
                SessionStatus::Completed,
                &RunStateV1::default(),
            )
            .await
            .expect("run completes");

        // Cancel: the phase-1 fence loses the CAS, reloads, observes the
        // terminal state, and returns a deliberate conflict.
        let err = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect_err("cancel must refuse a terminal observation");
        assert!(
            matches!(err, EngineError::TerminalState(_)),
            "terminal observation must be a deliberate conflict, got {err:?}"
        );
        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            final_record.status,
            SessionStatus::Completed,
            "completed remains completed"
        );
    }

    // ------------------------------------------------------------------
    // T2 fix round 1 — Finding 3: interrupted-cancel cleanup retry
    // ------------------------------------------------------------------

    /// Deterministic faulted-shutdown proof (Finding 3): the first cancel
    /// yields `Interrupted` (cleanup unconfirmed), a subsequent cancel
    /// re-enters the cancel path as the owner-scoped cleanup retry, reaps
    /// the owned Host session, and settles `Cancelled` — no step is
    /// re-driven.
    #[tokio::test]
    async fn interrupted_cancel_retry_reaps_and_settles_cancelled() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        // Scripted executor: the first finalize fails (cleanup unconfirmed),
        // the retry succeeds (reaps the owned Host session).
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![
            Err(CapabilityError::Internal("shutdown timeout".to_string())),
            Ok(()),
        ]));

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(Graph::new("cancel-retry"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // First cancel: cleanup unconfirmed → Interrupted (actionable, never
        // false successful cancellation).
        let err = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect_err("first cancel must surface the unconfirmed cleanup");
        assert!(
            matches!(err, EngineError::GraphFlow(_)),
            "unconfirmed cleanup surfaces the error, got {err:?}"
        );
        let interrupted = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(interrupted.status, SessionStatus::Interrupted);
        assert!(
            interrupted
                .state
                .as_ref()
                .is_some_and(|s| s.cancel_requested),
            "interrupted run must carry the durable cancel intent"
        );
        assert_eq!(executor.calls(), 1, "first finalize attempted");

        // A non-cancel signal on the interrupted run is refused (terminal).
        let err = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Resume)
            .await
            .expect_err("resume on interrupted must refuse");
        assert!(matches!(err, EngineError::TerminalState(_)));

        // Retry cancel: the terminal fence admits the owner-scoped cleanup
        // retry, finalize_run reaps the session, and the run settles
        // Cancelled — no step is re-driven.
        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("retry cancel must settle cancelled");
        assert_eq!(status, SessionStatus::Cancelled);
        assert_eq!(executor.calls(), 2, "retry re-invokes finalize_run");
        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(final_record.status, SessionStatus::Cancelled);
        assert!(
            final_record
                .state
                .as_ref()
                .is_some_and(|s| s.cancel_requested),
            "settled cancelled run must carry cancel_requested"
        );
    }

    /// Deterministic repeated-failure proof (Finding 3): a retry that still
    /// cannot confirm cleanup keeps the run `Interrupted` and retains the
    /// owned Host session for a later retry.
    #[tokio::test]
    async fn interrupted_cancel_retry_retains_on_repeated_failure() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        // Both finalize attempts fail: the run stays interrupted and the
        // owned session is retained.
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![
            Err(CapabilityError::Internal("shutdown timeout".to_string())),
            Err(CapabilityError::Internal("shutdown timeout".to_string())),
        ]));

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(Graph::new("cancel-retry-fail"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let _ = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect_err("first cancel unconfirmed");
        let _ = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect_err("retry cancel still unconfirmed");

        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            record.status,
            SessionStatus::Interrupted,
            "repeated failure keeps the run interrupted (actionable)"
        );
        assert_eq!(executor.calls(), 2, "both retries invoked finalize_run");
    }

    // ------------------------------------------------------------------
    // T2 fix round 2 — Finding 1: Continue is fenced after the
    // cancel-intent winner
    // ------------------------------------------------------------------

    /// Deterministic continue-after-cancel-fence proof (Finding 1): once the
    /// durable cancel intent is committed, a matching Continue must be
    /// refused with a state conflict — the wait is never consumed, the
    /// cancel fence stays intact, and no new work is created while
    /// cancellation is in flight.
    #[tokio::test]
    async fn continue_after_cancel_fence_is_state_conflict() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );

        let graph = Arc::new(Graph::new("continue-fence"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Park the run at a durable human wait.
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let waiting_state = RunStateV1 {
            wait: Some(crate::run_state::WaitRecord {
                wait_id: "wait-tok-1".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: crate::run_state::WaitKind::Manual,
            }),
            ..RunStateV1::default()
        };
        store
            .commit_transition(
                &session_id,
                record.state_revision,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                SessionStatus::WaitingForInput,
                &waiting_state,
            )
            .await
            .expect("park at human wait");

        // The cancel winner commits the durable cancel intent (phase 1):
        // `cancel_requested = true` while the status stays non-terminal.
        let fenced = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let fenced_root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let mut fenced_state = fenced.state.unwrap_or_default();
        fenced_state.cancel_requested = true;
        store
            .commit_transition(
                &session_id,
                fenced.state_revision,
                RunCheckpoint {
                    root: &fenced_root,
                    children: &[],
                },
                SessionStatus::WaitingForInput,
                &fenced_state,
            )
            .await
            .expect("cancel fence wins the CAS");

        // A matching Continue after the fence is refused: the cancel winner
        // is the ONLY durable control winner. The wait is never consumed.
        let err = engine
            .state
            .persist_signal_transition(
                &session_id,
                &EngineSignal::Continue {
                    wait_id: "wait-tok-1".to_string(),
                },
            )
            .await
            .expect_err("continue after the cancel fence must be refused");
        assert!(
            matches!(err, EngineError::TerminalState(_)),
            "continue after the cancel fence is a state conflict, got {err:?}"
        );

        // The wait and the cancel fence are both intact.
        let after = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(after.status, SessionStatus::WaitingForInput);
        assert!(
            after.state.as_ref().is_some_and(|s| s.cancel_requested),
            "the cancel fence must remain durable"
        );
        assert_eq!(
            after
                .state
                .as_ref()
                .and_then(|s| s.wait.as_ref())
                .map(|w| w.wait_id.as_str()),
            Some("wait-tok-1"),
            "the wait must not be consumed by the refused continue"
        );
    }

    // ------------------------------------------------------------------
    // T2 fix round 3 — Finding 1: settlement reloads the authoritative
    // root; a concurrent Continue winner's position/context survives
    // ------------------------------------------------------------------

    /// Deterministic Continue-wins-before-cancel proof (Finding 1, round 3):
    /// the run is waiting at revision R; Continue consumes the wait and
    /// commits a NEWER root position/context at R+1; Cancel's phase-1 fence
    /// loses the CAS, reloads the R+1 row/root, re-fences at R+2, and the
    /// terminal Cancelled settlement must carry the NEWEST root — the
    /// pre-Continue stale root must never clobber the winner's position.
    #[tokio::test]
    async fn cancel_settlement_keeps_continue_winner_root() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(())]));

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(Graph::new("continue-wins"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Park the run at a durable human wait (revision R).
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let waiting_state = RunStateV1 {
            wait: Some(crate::run_state::WaitRecord {
                wait_id: "wait-tok-1".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: crate::run_state::WaitKind::Manual,
            }),
            ..RunStateV1::default()
        };
        store
            .commit_transition(
                &session_id,
                record.state_revision,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                SessionStatus::WaitingForInput,
                &waiting_state,
            )
            .await
            .expect("park at human wait");

        // Continue wins BEFORE the cancel fence: it consumes the wait and
        // commits a NEWER root position/context at R+1 (the durable winner).
        let continued = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let mut winner_root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        winner_root.current_task_id = "winner-task".to_string();
        winner_root
            .context
            .set("winner.marker", "continue-won")
            .await;
        let winner_state = RunStateV1::default();
        store
            .commit_transition(
                &session_id,
                continued.state_revision,
                RunCheckpoint {
                    root: &winner_root,
                    children: &[],
                },
                SessionStatus::Running,
                &winner_state,
            )
            .await
            .expect("continue wins the CAS");

        // Cancel: the phase-1 fence loses the CAS (revision moved), reloads
        // the authoritative row AND root, re-fences against the new
        // revision, and settles Cancelled. The terminal checkpoint must
        // carry the winner's root — never the stale pre-Continue root.
        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("cancel must reload and retry the fence");
        assert_eq!(status, SessionStatus::Cancelled);

        // The final Cancelled row retains the newest root position/context.
        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(final_record.status, SessionStatus::Cancelled);
        assert!(
            final_record
                .state
                .as_ref()
                .is_some_and(|s| s.cancel_requested),
            "cancelled run must carry cancel_requested"
        );
        let final_root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        assert_eq!(
            final_root.current_task_id, "winner-task",
            "the terminal checkpoint must carry the Continue winner's position"
        );
        assert_eq!(
            final_root
                .context
                .get::<String>("winner.marker")
                .await
                .as_deref(),
            Some("continue-won"),
            "the terminal checkpoint must carry the Continue winner's context"
        );
    }

    /// Deterministic Continue-v-cancel interleaving proof (Finding 3,
    /// round 4): the run is waiting at revision R; Cancel loads the record
    /// and root at R; a competing Continue consumes the wait and commits a
    /// NEWER root position/context at R (the SAME revision) BEFORE Cancel's
    /// phase-1 fence commits. The fence loses the CAS, reloads the winner
    /// row/root, re-fences at R+1, and the terminal Cancelled settlement
    /// must carry the Continue winner's task/context — the stale pre-fence
    /// root must never clobber the winner. This is the exact interleaving
    /// the round-3 regression did not create (it committed the Continue
    /// before invoking Cancel, so Cancel's initial read already saw the
    /// winner).
    #[tokio::test]
    async fn cancel_fence_loses_to_continue_and_settlement_keeps_winner() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let race_store = Arc::new(ContinueWinsBeforeCancelFenceStore::new(
            store,
            storage.clone(),
        ));
        let store: Arc<dyn WorkflowStateStore> = race_store.clone();

        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(())]));

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(Graph::new("continue-wins-race"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Park the run at a durable human wait (revision R).
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let waiting_state = RunStateV1 {
            wait: Some(crate::run_state::WaitRecord {
                wait_id: "wait-tok-1".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: crate::run_state::WaitKind::Manual,
            }),
            ..RunStateV1::default()
        };
        store
            .commit_transition(
                &session_id,
                record.state_revision,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                SessionStatus::WaitingForInput,
                &waiting_state,
            )
            .await
            .expect("park at human wait");

        // Cancel: the phase-1 fence loses the CAS to the injected Continue
        // winner (same revision), reloads the winner row/root, re-fences
        // against the new revision, and settles Cancelled. The terminal
        // checkpoint must carry the winner's root — never the stale
        // pre-fence root.
        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("cancel must reload and retry the fence after the Continue winner");
        assert_eq!(status, SessionStatus::Cancelled);
        assert!(
            race_store.injected(),
            "the Continue winner must have been injected between Cancel's read and its fence"
        );

        // The final Cancelled row retains the newest root position/context.
        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(final_record.status, SessionStatus::Cancelled);
        assert!(
            final_record
                .state
                .as_ref()
                .is_some_and(|s| s.cancel_requested),
            "cancelled run must carry cancel_requested"
        );
        let final_root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        assert_eq!(
            final_root.current_task_id, "winner-task",
            "the terminal checkpoint must carry the Continue winner's position"
        );
        assert_eq!(
            final_root
                .context
                .get::<String>("winner.marker")
                .await
                .as_deref(),
            Some("continue-won"),
            "the terminal checkpoint must carry the Continue winner's context"
        );
    }

    // ------------------------------------------------------------------
    // T2 fix round 3 — Finding 2: recursive owned-descendant closure
    // ------------------------------------------------------------------

    /// Deterministic nested child/grandchild cancellation proof (Finding 2,
    /// round 3): a root with child A and grandchild A:child:* — the cancel
    /// fires EVERY descendant token, finalizes EVERY descendant run (root,
    /// child, grandchild), and reclaims every descendant token only after
    /// the whole closure is confirmed. A parked grandchild with no active
    /// operation is still reaped by its own run id.
    #[tokio::test]
    async fn cancel_finalizes_recursive_descendant_closure() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        // All finalizes succeed: root, child, grandchild.
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(()), Ok(()), Ok(())]));

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(Graph::new("nested-cancel"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Spawn child A (inner graph).
        let inner = Arc::new(Graph::new("inner-a"));
        inner.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let child_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        // Spawn grandchild A:child:* (nested inner graph under the child).
        let inner2 = Arc::new(Graph::new("inner-b"));
        inner2.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let grandchild_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: child_id.0.clone(),
                inner_graph: inner2,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn grandchild");

        // Park the grandchild at a durable human wait: its Host session is
        // cached but no grandchild operation is active to observe a fired
        // token — the recursive finalize must still reach it by run id.
        let grandchild_record = store
            .load_run(&grandchild_id)
            .await
            .expect("load grandchild")
            .expect("grandchild exists");
        let grandchild_root = storage
            .get(&grandchild_id.0)
            .await
            .expect("get grandchild root")
            .expect("grandchild root exists");
        let grandchild_waiting = RunStateV1 {
            wait: Some(crate::run_state::WaitRecord {
                wait_id: "grandchild-wait-tok".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: crate::run_state::WaitKind::Manual,
            }),
            ..RunStateV1::default()
        };
        store
            .commit_transition(
                &grandchild_id,
                grandchild_record.state_revision,
                RunCheckpoint {
                    root: &grandchild_root,
                    children: &[],
                },
                SessionStatus::WaitingForInput,
                &grandchild_waiting,
            )
            .await
            .expect("park grandchild at human wait");

        // Register tokens for the child and grandchild (spawn registers
        // them; assert they exist and are not yet cancelled).
        let cancels = engine
            .state
            .session_cancels
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let child_token = cancels
            .get(&child_id.0)
            .cloned()
            .expect("child token registered");
        let grandchild_token = cancels
            .get(&grandchild_id.0)
            .cloned()
            .expect("grandchild token registered");
        drop(cancels);
        assert!(!child_token.is_cancelled());
        assert!(!grandchild_token.is_cancelled());

        // Cancel: fires root + child + grandchild tokens, finalizes root +
        // child + grandchild, and settles Cancelled.
        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("cancel must settle cancelled");
        assert_eq!(status, SessionStatus::Cancelled);

        // Every descendant token fired.
        assert!(child_token.is_cancelled(), "child token must fire");
        assert!(
            grandchild_token.is_cancelled(),
            "grandchild token must fire"
        );

        // Every descendant run finalized by its own run id.
        assert_eq!(
            executor.finalized_runs(),
            vec![
                session_id.0.clone(),
                child_id.0.clone(),
                grandchild_id.0.clone(),
            ],
            "root, child, and grandchild each finalized by their own run id"
        );

        // Every descendant token reclaimed at the confirmed boundary.
        let cancels = engine
            .state
            .session_cancels
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(!cancels.contains_key(&session_id.0), "root token reclaimed");
        assert!(!cancels.contains_key(&child_id.0), "child token reclaimed");
        assert!(
            !cancels.contains_key(&grandchild_id.0),
            "grandchild token reclaimed"
        );
        drop(cancels);

        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(final_record.status, SessionStatus::Cancelled);
    }

    /// Deterministic late-child admission race proof (P1, rereview-4): a
    /// child is admitted (durable row + coordinator token) but its in-memory
    /// children-map entry is dropped BEFORE the root cancel — the exact
    /// shape of an in-flight `InnerGraphTask` racing the cancel fence. The
    /// cancel closure reconciles the AUTHORITATIVE persisted parent
    /// relation, so the late child's token is fired and its run is finalized
    /// by its own run id — no descendant can be admitted after the fence and
    /// escape the closure (A5: no downstream effects after the fence,
    /// complete owned-work cleanup).
    #[tokio::test]
    async fn cancel_reconciles_persisted_late_child_into_closure() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        // Root + mapped child + late (persisted, unmapped) child all
        // finalize successfully.
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(()), Ok(()), Ok(())]));

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(Graph::new("late-child-cancel"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Spawn child A (mapped).
        let inner = Arc::new(Graph::new("inner-a"));
        inner.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let child_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        // Spawn the LATE child (durable row + token), then drop its
        // in-memory entries — the shape of an admission whose map entry was
        // lost before the cancel closure snapshot.
        let inner2 = Arc::new(Graph::new("inner-late"));
        inner2.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let late_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner2,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn late child");
        {
            let mut child_map = engine.state.children.write().await;
            if let Some(entries) = child_map.get_mut(&session_id.0) {
                entries.retain(|c| c.session.id != late_id.0);
            }
            engine.state.runners.write().await.remove(&late_id.0);
            engine
                .state
                .sessions
                .write()
                .await
                .retain(|s| s.session_id.0 != late_id.0);
        }

        // The late child's token is registered but not yet fired.
        let late_token = {
            let cancels = engine
                .state
                .session_cancels
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cancels
                .get(&late_id.0)
                .cloned()
                .expect("late child token registered")
        };
        assert!(!late_token.is_cancelled());

        // Cancel: the closure reconciles the persisted parent relation,
        // fires the late child's token, finalizes root + child + late child,
        // and settles Cancelled.
        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("cancel must settle cancelled");
        assert_eq!(status, SessionStatus::Cancelled);

        // The late child's token fired — no Host admission can occur under
        // it after the fence.
        assert!(late_token.is_cancelled(), "late child token must fire");

        // Every owned run finalized by its own run id — the late child is
        // reaped even though its map entry was gone.
        assert_eq!(
            executor.finalized_runs(),
            vec![session_id.0.clone(), child_id.0.clone(), late_id.0.clone(),],
            "root, mapped child, and persisted late child each finalized by their own run id"
        );

        // No late child remains mapped, tracked, driven, or token-registered.
        {
            let child_map = engine.state.children.read().await;
            assert!(
                !child_map
                    .get(&session_id.0)
                    .is_some_and(|entries| { entries.iter().any(|c| c.session.id == late_id.0) }),
                "late child must not remain in the children map"
            );
            let sessions = engine.state.sessions.read().await;
            assert!(
                !sessions.iter().any(|s| s.session_id.0 == late_id.0),
                "late child must not remain in the session tracker"
            );
            let runners = engine.state.runners.read().await;
            assert!(
                !runners.contains_key(&late_id.0),
                "late child runner must be dropped"
            );
            let cancels = engine
                .state
                .session_cancels
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            assert!(
                !cancels.contains_key(&late_id.0),
                "late child token reclaimed at the confirmed boundary"
            );
        }

        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(final_record.status, SessionStatus::Cancelled);
    }

    /// Deterministic child-admission-vs-cancel-fence proof (P1, rereview-4):
    /// the parent's durable `cancel_requested` fence is committed BEFORE the
    /// child admission's post-persistence recheck. The admission must refuse
    /// (revision-linearized with the parent), roll the child back — token
    /// fired, durable row settled `cancelled`, every in-memory entry dropped
    /// — and no child Host admission can occur after the root fence.
    #[tokio::test]
    async fn child_admission_after_parent_cancel_fence_is_rolled_back() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(())]));

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(Graph::new("child-after-fence"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Commit the durable cancel-intent fence on the parent (the phase-1
        // fence of the cancel path) BEFORE the child admission recheck.
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let mut fenced_state = record.state.unwrap_or_default();
        fenced_state.cancel_requested = true;
        store
            .commit_transition(
                &session_id,
                record.state_revision,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                SessionStatus::Running,
                &fenced_state,
            )
            .await
            .expect("cancel-intent fence commits");

        // The child admission must refuse: the parent carries durable
        // `cancel_requested`, so no child may be created/registered after the
        // fence.
        let inner = Arc::new(Graph::new("inner-late"));
        inner.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let err = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect_err("child admission after the cancel fence must refuse");
        assert!(
            matches!(err, EngineError::TerminalState(_)),
            "refusal must be the terminal-state conflict, got {err:?}"
        );

        // The rolled-back child row is durably terminal `Cancelled` — never
        // runnable, never replayed (A5 settlement fence).
        let children = store
            .load_children(&session_id)
            .await
            .expect("load children");
        assert_eq!(children.len(), 1, "exactly one child row remains");
        assert_eq!(
            children[0].status,
            SessionStatus::Cancelled,
            "the rolled-back child must be durably cancelled, got {:?}",
            children[0].status
        );
        assert!(
            children[0]
                .state
                .as_ref()
                .is_some_and(|s| s.cancel_requested),
            "the rolled-back child must carry the durable cancel intent"
        );

        // No child remains in any in-memory structure.
        {
            let child_map = engine.state.children.read().await;
            assert!(
                !child_map.contains_key(&session_id.0),
                "no child may remain in the children map"
            );
            let sessions = engine.state.sessions.read().await;
            assert_eq!(
                sessions.len(),
                1,
                "only the root may remain in the session tracker"
            );
            let runners = engine.state.runners.read().await;
            assert_eq!(runners.len(), 1, "only the root runner may remain");
            let cancels = engine
                .state
                .session_cancels
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            assert_eq!(
                cancels.len(),
                1,
                "only the root coordinator token may remain registered"
            );
        }

        // The root cancel settles Cancelled. The closure reconciles the
        // persisted parent relation, so the rolled-back child's durable row
        // (terminal `cancelled`) is also finalized — idempotent, and any
        // Host session it might have created is reaped.
        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("cancel must settle cancelled");
        assert_eq!(status, SessionStatus::Cancelled);
        let finalized = executor.finalized_runs();
        assert_eq!(finalized.len(), 2, "root and the rolled-back child row");
        assert_eq!(finalized[0], session_id.0, "root finalized first");
        assert!(
            finalized[1].starts_with(&format!("{}:child:", session_id.0)),
            "the rolled-back child's durable row is finalized, got {:?}",
            finalized[1]
        );
    }

    /// Deterministic child-rollback settlement CAS-loss proof (P1, fix
    /// round 6): the child admission's rollback settle loses its revision
    /// CAS to a competing transition (the child row advanced between the
    /// rollback's reload and its settle). The rollback must reload the child
    /// record and re-settle against the CURRENT revision — never discard the
    /// settlement result and drop ownership over an unconfirmed row. After
    /// the parent cancel fence, no running child row and no unregistered
    /// late child may remain.
    #[tokio::test]
    async fn child_rollback_settle_cas_loss_reloads_and_retries() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let race_store = Arc::new(ChildSettleCasLossStore::new(store, storage.clone()));
        let store: Arc<dyn WorkflowStateStore> = race_store.clone();

        // Root + rolled-back child both finalize successfully.
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(()), Ok(())]));

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(Graph::new("child-settle-cas-loss"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Commit the durable cancel-intent fence on the parent BEFORE the
        // child admission recheck.
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let mut fenced_state = record.state.unwrap_or_default();
        fenced_state.cancel_requested = true;
        store
            .commit_transition(
                &session_id,
                record.state_revision,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                SessionStatus::Running,
                &fenced_state,
            )
            .await
            .expect("cancel-intent fence commits");

        // The child admission must refuse; the rollback's settle loses its
        // CAS to the injected competing transition, reloads the child
        // record, and re-settles against the current revision.
        let inner = Arc::new(Graph::new("inner-late"));
        inner.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let err = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect_err("child admission after the cancel fence must refuse");
        assert!(
            matches!(err, EngineError::TerminalState(_)),
            "refusal must be the terminal-state conflict, got {err:?}"
        );
        assert!(
            race_store.injected(),
            "the competing transition must have been injected before the child settle"
        );

        // The rolled-back child row is durably terminal `Cancelled` — the
        // reload/retry settled against the CURRENT revision, never a stale
        // one, and no running child row remains.
        let children = store
            .load_children(&session_id)
            .await
            .expect("load children");
        assert_eq!(children.len(), 1, "exactly one child row remains");
        assert_eq!(
            children[0].status,
            SessionStatus::Cancelled,
            "the rolled-back child must be durably cancelled, got {:?}",
            children[0].status
        );
        assert!(
            children[0]
                .state
                .as_ref()
                .is_some_and(|s| s.cancel_requested),
            "the rolled-back child must carry the durable cancel intent"
        );

        // No child remains in any in-memory structure — ownership was
        // dropped only after the confirmed Cancelled settlement, so no
        // unregistered late child remains.
        {
            let child_map = engine.state.children.read().await;
            assert!(
                !child_map.contains_key(&session_id.0),
                "no child may remain in the children map"
            );
            let sessions = engine.state.sessions.read().await;
            assert_eq!(
                sessions.len(),
                1,
                "only the root may remain in the session tracker"
            );
            let runners = engine.state.runners.read().await;
            assert_eq!(runners.len(), 1, "only the root runner may remain");
            let cancels = engine
                .state
                .session_cancels
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            assert_eq!(
                cancels.len(),
                1,
                "only the root coordinator token may remain registered"
            );
        }

        // The root cancel settles Cancelled; the closure reconciles the
        // persisted parent relation and finalizes the rolled-back child's
        // durable row (idempotent).
        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("cancel must settle cancelled");
        assert_eq!(status, SessionStatus::Cancelled);
        let finalized = executor.finalized_runs();
        assert_eq!(finalized.len(), 2, "root and the rolled-back child row");
        assert_eq!(finalized[0], session_id.0, "root finalized first");
        assert!(
            finalized[1].starts_with(&format!("{}:child:", session_id.0)),
            "the rolled-back child's durable row is finalized, got {:?}",
            finalized[1]
        );
    }

    /// Deterministic descendant retry-retention proof (Finding 2, round 3):
    /// the grandchild's finalize fails on the first cancel → the run is
    /// `Interrupted` and the grandchild's token is retained (not reclaimed);
    /// the retry finalizes root + child + grandchild and settles Cancelled,
    /// reclaiming every token only at the confirmed boundary.
    #[tokio::test]
    async fn cancel_descendant_retry_retains_failed_entries() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        // First cancel: root OK, child OK, grandchild FAILS (cleanup
        // unconfirmed). Retry: root OK, child OK, grandchild OK.
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![
            Ok(()),
            Ok(()),
            Err(CapabilityError::Internal("shutdown timeout".to_string())),
            Ok(()),
            Ok(()),
            Ok(()),
        ]));

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(Graph::new("nested-retry"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let inner = Arc::new(Graph::new("inner-a"));
        inner.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let child_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        let inner2 = Arc::new(Graph::new("inner-b"));
        inner2.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let grandchild_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: child_id.0.clone(),
                inner_graph: inner2,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn grandchild");

        // First cancel: grandchild finalize fails → Interrupted. The
        // grandchild token must be RETAINED (not reclaimed) because its
        // cleanup is unconfirmed.
        let err = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect_err("first cancel must surface the unconfirmed cleanup");
        assert!(matches!(err, EngineError::GraphFlow(_)));
        let interrupted = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(interrupted.status, SessionStatus::Interrupted);
        assert_eq!(
            executor.finalized_runs(),
            vec![
                session_id.0.clone(),
                child_id.0.clone(),
                grandchild_id.0.clone(),
            ],
            "root and child finalized before the grandchild failure"
        );
        let cancels = engine
            .state
            .session_cancels
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(
            cancels.contains_key(&grandchild_id.0),
            "grandchild token retained for retry after unconfirmed cleanup"
        );
        assert!(
            cancels.contains_key(&child_id.0),
            "child token retained (closure unconfirmed)"
        );
        assert!(
            cancels.contains_key(&session_id.0),
            "root token retained (closure unconfirmed)"
        );
        drop(cancels);

        // Retry cancel: root + child + grandchild all finalize, the run
        // settles Cancelled, and every token is reclaimed.
        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("retry cancel must settle cancelled");
        assert_eq!(status, SessionStatus::Cancelled);
        assert_eq!(
            executor.finalized_runs(),
            vec![
                session_id.0.clone(),
                child_id.0.clone(),
                grandchild_id.0.clone(),
                session_id.0.clone(),
                child_id.0.clone(),
                grandchild_id.0.clone(),
            ],
            "retry finalizes the whole closure again"
        );
        let cancels = engine
            .state
            .session_cancels
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(
            !cancels.contains_key(&session_id.0),
            "root token reclaimed after confirmed closure"
        );
        assert!(
            !cancels.contains_key(&child_id.0),
            "child token reclaimed after confirmed closure"
        );
        assert!(
            !cancels.contains_key(&grandchild_id.0),
            "grandchild token reclaimed after confirmed closure"
        );
        drop(cancels);
    }

    // ------------------------------------------------------------------
    // T2 fix round 4 — Important 1: restarted roots reconstruct the
    // COMPLETE owned-descendant closure from the persisted parent relation
    // ------------------------------------------------------------------

    /// Restart-shaped nested root/child/grandchild cancellation proof
    /// (Important 1, round 4): a root with child A and grandchild A:child:*
    /// is persisted, then a FRESH engine (restart) recovers ONLY the root
    /// summary (the boot shape — `list_non_terminal_sessions` returns root
    /// rows only). Recovery must recursively hydrate the persisted parent
    /// relation so the root Cancel fires/finalizes every owned descendant
    /// — the grandchild is never left outside the closure. A failed
    /// descendant cleanup on the retry remains reachable.
    #[tokio::test]
    async fn recovered_root_cancel_reaches_persisted_grandchild() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        // Phase 1: a live engine spawns the nested tree and parks the
        // grandchild at a durable human wait (non-terminal, recoverable).
        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let live = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        let graph = Arc::new(Graph::new("restart-nested"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = live
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let inner = Arc::new(Graph::new("inner-a"));
        inner.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let child_id = live
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        let inner2 = Arc::new(Graph::new("inner-b"));
        inner2.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let grandchild_id = live
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: child_id.0.clone(),
                inner_graph: inner2,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn grandchild");

        // Park the grandchild at a durable human wait so it is a
        // non-terminal persisted descendant after "restart".
        let grandchild_record = store
            .load_run(&grandchild_id)
            .await
            .expect("load grandchild")
            .expect("grandchild exists");
        let grandchild_root = storage
            .get(&grandchild_id.0)
            .await
            .expect("get grandchild root")
            .expect("grandchild root exists");
        let grandchild_waiting = RunStateV1 {
            wait: Some(crate::run_state::WaitRecord {
                wait_id: "restart-grandchild-wait".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: crate::run_state::WaitKind::Manual,
            }),
            ..RunStateV1::default()
        };
        store
            .commit_transition(
                &grandchild_id,
                grandchild_record.state_revision,
                RunCheckpoint {
                    root: &grandchild_root,
                    children: &[],
                },
                SessionStatus::WaitingForInput,
                &grandchild_waiting,
            )
            .await
            .expect("park grandchild at human wait");

        // Phase 2: a FRESH engine (restart) recovers ONLY the root summary
        // — the boot shape (`list_non_terminal_sessions` returns root rows
        // only). The child/grandchild rows are never supplied as summaries.
        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut restarted = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(()), Ok(()), Ok(())]));
        restarted.set_prompt_executor(executor.clone(), restarted.state.session_cancels.clone());

        let root_summary = SessionSummary {
            session_id: session_id.clone(),
            creator_id: String::new(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("persist".to_string()),
        };
        restarted.recover_sessions(vec![root_summary]).await;

        // The complete owned-descendant closure is reconstructed: the
        // children map carries root→child AND child→grandchild, and every
        // descendant has a registered coordinator token.
        {
            let children = restarted.state.children.read().await;
            let root_children = children
                .get(&session_id.0)
                .expect("root children hydrated after restart");
            assert_eq!(
                root_children
                    .iter()
                    .map(|c| c.session.id.as_str())
                    .collect::<Vec<_>>(),
                vec![child_id.0.as_str()],
                "root's direct child hydrated"
            );
            let child_children = children
                .get(&child_id.0)
                .expect("child children hydrated after restart");
            assert_eq!(
                child_children
                    .iter()
                    .map(|c| c.session.id.as_str())
                    .collect::<Vec<_>>(),
                vec![grandchild_id.0.as_str()],
                "grandchild hydrated under the child (recursive closure)"
            );
        }
        let cancels = restarted
            .state
            .session_cancels
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let child_token = cancels
            .get(&child_id.0)
            .cloned()
            .expect("child token registered by recovery");
        let grandchild_token = cancels
            .get(&grandchild_id.0)
            .cloned()
            .expect("grandchild token registered by recovery");
        drop(cancels);
        assert!(!child_token.is_cancelled());
        assert!(!grandchild_token.is_cancelled());

        // Root Cancel after restart: fires root + child + grandchild
        // tokens, finalizes every owned descendant by its own run id, and
        // settles Cancelled — the grandchild is never left outside the
        // closure.
        let status = restarted
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("restarted root cancel must settle cancelled");
        assert_eq!(status, SessionStatus::Cancelled);
        assert!(child_token.is_cancelled(), "child token must fire");
        assert!(
            grandchild_token.is_cancelled(),
            "grandchild token must fire"
        );
        assert_eq!(
            executor.finalized_runs(),
            vec![
                session_id.0.clone(),
                child_id.0.clone(),
                grandchild_id.0.clone(),
            ],
            "restarted root cancel finalizes the whole persisted closure"
        );
        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(final_record.status, SessionStatus::Cancelled);
    }

    /// Restart-shaped failed-descendant retry proof (Important 1, round 4):
    /// after recovery, the grandchild's finalize fails on the first cancel
    /// → the run is `Interrupted` and the grandchild's token is retained;
    /// the retry finalizes the whole closure and settles Cancelled. This
    /// proves failed descendant cleanup remains retryable for a restarted
    /// root, not only for an in-process engine.
    #[tokio::test]
    async fn recovered_root_cancel_retry_retains_failed_grandchild() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let live = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        let graph = Arc::new(Graph::new("restart-retry"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = live
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let inner = Arc::new(Graph::new("inner-a"));
        inner.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let child_id = live
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        let inner2 = Arc::new(Graph::new("inner-b"));
        inner2.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let grandchild_id = live
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: child_id.0.clone(),
                inner_graph: inner2,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn grandchild");

        // Restart: fresh engine recovers only the root summary. First
        // cancel: root OK, child OK, grandchild FAILS. Retry: all OK.
        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut restarted = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![
            Ok(()),
            Ok(()),
            Err(CapabilityError::Internal("shutdown timeout".to_string())),
            Ok(()),
            Ok(()),
            Ok(()),
        ]));
        restarted.set_prompt_executor(executor.clone(), restarted.state.session_cancels.clone());

        let root_summary = SessionSummary {
            session_id: session_id.clone(),
            creator_id: String::new(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("persist".to_string()),
        };
        restarted.recover_sessions(vec![root_summary]).await;

        // First cancel: the grandchild's finalize fails → Interrupted; the
        // grandchild token is retained for the owner-scoped retry.
        let err = restarted
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect_err("first cancel must surface the unconfirmed cleanup");
        assert!(matches!(err, EngineError::GraphFlow(_)));
        let interrupted = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(interrupted.status, SessionStatus::Interrupted);
        assert_eq!(
            executor.finalized_runs(),
            vec![
                session_id.0.clone(),
                child_id.0.clone(),
                grandchild_id.0.clone(),
            ],
            "root and child finalized before the grandchild failure"
        );
        let cancels = restarted
            .state
            .session_cancels
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(
            cancels.contains_key(&grandchild_id.0),
            "grandchild token retained for retry after unconfirmed cleanup"
        );
        assert!(
            cancels.contains_key(&child_id.0),
            "child token retained (closure unconfirmed)"
        );
        assert!(
            cancels.contains_key(&session_id.0),
            "root token retained (closure unconfirmed)"
        );
        drop(cancels);

        // Retry cancel: the whole closure finalizes, the run settles
        // Cancelled, and every token is reclaimed.
        let status = restarted
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("retry cancel must settle cancelled");
        assert_eq!(status, SessionStatus::Cancelled);
        assert_eq!(
            executor.finalized_runs(),
            vec![
                session_id.0.clone(),
                child_id.0.clone(),
                grandchild_id.0.clone(),
                session_id.0.clone(),
                child_id.0.clone(),
                grandchild_id.0.clone(),
            ],
            "retry finalizes the whole closure again"
        );
        let cancels = restarted
            .state
            .session_cancels
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(
            !cancels.contains_key(&session_id.0),
            "root token reclaimed after confirmed closure"
        );
        assert!(
            !cancels.contains_key(&child_id.0),
            "child token reclaimed after confirmed closure"
        );
        assert!(
            !cancels.contains_key(&grandchild_id.0),
            "grandchild token reclaimed after confirmed closure"
        );
        drop(cancels);
    }

    // ------------------------------------------------------------------
    // T2 fix round 2 — Finding 2: Interrupted persistence is
    // revision-fenced, bounded, and never discarded
    // ------------------------------------------------------------------

    /// Deterministic interruption-write CAS failure proof (Finding 2): the
    /// `Interrupted` commit loses a revision CAS to a competing transition;
    /// the bounded retry reloads and re-commits against the new revision.
    /// The durable row AND the in-memory summary both land on
    /// `Interrupted`, and the actionable failure record is persisted.
    #[tokio::test]
    async fn interrupted_write_cas_loss_retries_and_persists() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = Arc::new(InterruptedCasInjectingStore::new(
            real_store.clone(),
            storage.clone(),
        ));

        // The first finalize fails (cleanup unconfirmed) — the Interrupted
        // write then loses its CAS once and must retry.
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Err(
            CapabilityError::Internal("shutdown timeout".to_string()),
        )]));

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(Graph::new("interrupted-cas"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // First cancel: cleanup unconfirmed → the Interrupted write loses
        // the CAS once, the bounded retry re-commits, and the error is
        // surfaced with the actionable reason.
        let err = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect_err("unconfirmed cleanup surfaces the error");
        assert!(
            matches!(err, EngineError::GraphFlow(_)),
            "unconfirmed cleanup surfaces the error, got {err:?}"
        );

        // Durable row: Interrupted with the cancel intent and the failure
        // record (the write result was never discarded).
        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(record.status, SessionStatus::Interrupted);
        assert!(
            record.state.as_ref().is_some_and(|s| s.cancel_requested),
            "interrupted run must carry the durable cancel intent"
        );
        let failure = record
            .state
            .as_ref()
            .and_then(|s| s.failure.as_ref())
            .expect("interrupted run must carry the actionable failure record");
        assert_eq!(failure.code, "cancel_cleanup_unconfirmed");
        assert!(
            failure.message.contains("cancel cleanup unconfirmed"),
            "failure message must be actionable, got: {}",
            failure.message
        );

        // In-memory summary: updated ONLY after the durable write succeeded
        // (Finding 2 — never before).
        let sessions = engine.state.sessions.read().await;
        let in_memory = sessions
            .iter()
            .find(|s| s.session_id == session_id)
            .expect("session tracked in memory");
        assert_eq!(
            in_memory.status,
            SessionStatus::Interrupted,
            "in-memory summary must reflect the durable interrupted outcome"
        );
    }

    // ------------------------------------------------------------------
    // T2 fix round 2 — Finding 3: child Host sessions are finalized
    // ------------------------------------------------------------------

    /// Deterministic child-wait cancellation proof (Finding 3): a child
    /// parked at a human wait (cached Host session, no active operation)
    /// is reaped via its OWN `finalize_run` call — the root finalize alone
    /// never visits the child-keyed cache entry. On root cleanup failure the
    /// run is `Interrupted`; the retry finalizes root AND child and settles
    /// `Cancelled`.
    #[tokio::test]
    async fn cancel_finalizes_child_host_sessions() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let pool = nexus_local_db::open_pool(db.path())
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        // First cancel: root finalize fails (cleanup unconfirmed). Retry:
        // root finalize succeeds, then the child's OWN finalize succeeds.
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![
            Err(CapabilityError::Internal("shutdown timeout".to_string())),
            Ok(()),
            Ok(()),
        ]));

        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(Graph::new("child-cancel"));
        graph.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // Spawn a child (inner graph) — registered in the children map with
        // its own cancellation token and v1 run row.
        let inner = Arc::new(Graph::new("inner"));
        inner.add_task(Arc::new(crate::tasks::ManualWaitTask));
        let child_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        // Park the child at a durable human wait: its Host session is cached
        // but no child operation is active to observe a fired token.
        let child_record = store
            .load_run(&child_id)
            .await
            .expect("load child")
            .expect("child exists");
        let child_root = storage
            .get(&child_id.0)
            .await
            .expect("get child root")
            .expect("child root exists");
        let child_waiting = RunStateV1 {
            wait: Some(crate::run_state::WaitRecord {
                wait_id: "child-wait-tok".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: crate::run_state::WaitKind::Manual,
            }),
            ..RunStateV1::default()
        };
        store
            .commit_transition(
                &child_id,
                child_record.state_revision,
                RunCheckpoint {
                    root: &child_root,
                    children: &[],
                },
                SessionStatus::WaitingForInput,
                &child_waiting,
            )
            .await
            .expect("park child at human wait");

        // First cancel: root finalize fails → Interrupted. The child was
        // NOT finalized (root failure short-circuits before descendants).
        let err = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect_err("first cancel must surface the unconfirmed cleanup");
        assert!(matches!(err, EngineError::GraphFlow(_)));
        assert_eq!(
            executor.finalized_runs(),
            vec![session_id.0.clone()],
            "root finalize attempted first; child not reached on root failure"
        );
        let interrupted = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(interrupted.status, SessionStatus::Interrupted);

        // Retry cancel: root finalize succeeds, then the child's OWN
        // finalize reaps the child-keyed Host session, and the run settles
        // Cancelled.
        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("retry cancel must settle cancelled");
        assert_eq!(status, SessionStatus::Cancelled);
        assert_eq!(
            executor.finalized_runs(),
            vec![
                session_id.0.clone(),
                session_id.0.clone(),
                child_id.0.clone(),
            ],
            "retry finalizes the root then the child by its own run id"
        );
        let final_record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(final_record.status, SessionStatus::Cancelled);
    }
}
