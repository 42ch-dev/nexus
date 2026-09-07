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
use crate::capability::CapabilityRegistry;
use crate::run_state::{
    ChildCheckpoint, PresetSourceIdentity, RunCheckpoint, RunDescriptorV1, RunStateV1,
    WorkflowStateStore,
};

/// Context key that effectful tasks set when they perform an external effect
/// (capability call, ACP/Host prompt dispatch, HostTool call, child spawn).
///
/// `run_step_internal` inspects the post-step root context for this marker to
/// decide the disposition of a failed post-effect `commit_transition`: if an
/// external effect may have executed, the run is **interrupted** (never a
/// blind rewind to a seemingly safe boundary — A2/A7); if the step was
/// deterministic (no marker), a CAS-fenced restore is acceptable (Important 3).
pub const EXTERNAL_EFFECT_MARKER: &str = "__external_effect_ran";

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
    #[error("state revision mismatch for session {session_id}: expected {expected}, found {found}")]
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
    ) -> Option<SessionId>;

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
        }
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
    pub async fn hydrate_children(
        &self,
        parent_session_id: &SessionId,
    ) -> Result<(), EngineError> {
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
        let active_terminal_children: Vec<String> = serde_json::to_value(&parent.context)
            .ok()
            .as_ref()
            .and_then(crate::resume_rules::context_data)
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
            let child_session = match self
                .storage
                .get(&child.session_id.0)
                .await
                .map_err(EngineError::GraphFlow)
            {
                Ok(Some(s)) => s,
                Ok(None) | Err(_) => {
                    // On child-snapshot error, clear the parent's stale
                    // children-map entry before propagating (Minor 1).
                    self.children.write().await.remove(&parent_session_id.0);
                    return Err(EngineError::GraphFlow(graph_flow::GraphError::StorageError(
                        format!(
                            "hydrate_children '{}': persisted child '{}' has no readable \
                             session snapshot (non-replayable)",
                            parent_session_id.0, child.session_id.0
                        ),
                    )));
                }
            };
            let graph_name = child
                .descriptor
                .as_ref()
                .and_then(|d| d.graph_name.clone());
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
        Ok(())
    }

    /// Authoritative persisted cursor for a session (round-4 Critical 2).
    ///
    /// Reads the current task id from the storage snapshot — the same
    /// source `commit_transition` persists — rather than the in-memory
    /// tracker (which may lag after recovery). `Ok(None)` when no session
    /// row exists yet (or storage errors are treated as absent by callers).
    async fn current_task_id(
        state: &EngineSharedState,
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
    ///
    /// Returns the target status on success. When the store is absent
    /// (in-memory test storage) this is a no-op returning the target status.
    async fn persist_signal_transition(
        &self,
        session_id: &SessionId,
        signal: &EngineSignal,
    ) -> Result<SessionStatus, EngineError> {
        let target_status = match signal {
            EngineSignal::Pause => SessionStatus::Paused,
            EngineSignal::Resume | EngineSignal::Advance => SessionStatus::Running,
            EngineSignal::Cancel => SessionStatus::Cancelled,
        };

        let Some(store) = &self.workflow_store else {
            return Ok(target_status);
        };

        let record = store
            .load_run(session_id)
            .await?
            .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))?;
        let expected_revision = record.state_revision;
        let mut next_state = record.state.unwrap_or_default();

        if record.status.is_terminal() {
            return Err(EngineError::TerminalState(session_id.0.clone()));
        }
        if !matches!(signal, EngineSignal::Cancel)
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

        if matches!(target_status, SessionStatus::Cancelled) {
            next_state.cancel_requested = true;
        }
        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };
        store
            .commit_transition(
                session_id,
                expected_revision,
                checkpoint,
                target_status.clone(),
                &next_state,
            )
            .await?;
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
                .map(|r| r.state_revision)
                .unwrap_or(0)
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
                if let Err(e) = store
                    .mark_step_in_flight(
                        session_id,
                        expected_revision,
                        in_flight_checkpoint,
                        &in_flight_state,
                    )
                    .await
                {
                    return Err(e);
                }
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

                let next_state = build_step_state(&result, &status, &root.current_task_id, &root.context);
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
                let parent_advanced = pre_step_root.as_ref().is_some_and(|pre| {
                    pre.current_task_id != root.current_task_id
                });
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
                    .commit_transition(session_id, transition_revision, checkpoint, status.clone(), &next_state)
                    .await;
                match commit_result {
                    Ok(_) => {
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

        // Record the child checkpoint so the root transition persists the
        // nested child row atomically (Important 2). The CAS anchor is the
        // child's actual persisted revision: a v1 child created via
        // `start_run` is at revision 1; a bare-save child (no store) is at 0.
        let child_revision = if self.workflow_store.is_some() { 1 } else { 0 };
        let child_checkpoint = ChildCheckpoint {
            session: session_mut.clone(),
            status: SessionStatus::Running,
            state: RunStateV1::default(),
            state_revision: child_revision,
            graph_name: Some(params.inner_graph.id.clone()),
        };
        self.children
            .write()
            .await
            .entry(params.parent_session_id.clone())
            .or_default()
            .push(child_checkpoint);

        // WS2 R3: Store Arc<FlowRunner> instead of FlowRunner.
        let runner = Arc::new(FlowRunner::new(
            params.inner_graph,
            self.storage.clone(),
        ));
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
    pub async fn attach_existing_child_session_internal(
        &self,
        parent_session_id: &str,
        inner_graph: Arc<Graph>,
    ) -> Option<SessionId> {
        let target = {
            let children = self.children.read().await;
            let parent_children = children.get(parent_session_id)?;
            parent_children
                .iter()
                .find(|c| c.graph_name.as_deref() == Some(inner_graph.id.as_str()))
                .map(|c| (c.session.id.clone(), c.session.current_task_id.clone(), c.status.clone()))
        };
        let (child_id, child_task, child_status) = target?;
        // Reconstruct a FlowRunner for the reattached child so it can be
        // stepped from its persisted position (and so the caller can query
        // its durable status via `get_status`).
        if !self
            .runners
            .read()
            .await
            .contains_key(&child_id)
        {
            let runner = Arc::new(FlowRunner::new(inner_graph, self.storage.clone()));
            self.runners.write().await.insert(child_id.clone(), runner);
        }
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
        Some(SessionId(child_id))
    }
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
/// `InnerGraphTask` and the root WaitRecord names the exact waiting
/// descendant (`child_session_id` / `child_task_id`, A4 nested path).
/// Scheduler converge/merge parks arrive here as `SessionStatus::Paused` and
/// persist tokenless (A2) — the join keys in the context are the wait
/// evidence. Manual waits reached through labeled/conditional routing with
/// stale/broad join keys are not re-mapped (no current-gate park marker) and
/// keep the waiting_for_input + fresh token shape.
fn build_step_state(
    result: &graph_flow::ExecutionResult,
    status: &SessionStatus,
    current_task_id: &str,
    root_context: &graph_flow::Context,
) -> RunStateV1 {
    let failure = match (&result.status, status) {
        (ExecutionStatus::Error(msg), SessionStatus::Failed) => Some(crate::run_state::RunFailure {
            code: "graph_step_error".to_string(),
            message: msg.clone(),
        }),
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
    ) -> Option<SessionId> {
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
            state: Arc::new(EngineSharedState::with_workflow_store(storage, workflow_store)),
            caps,
            daemon_tool_dispatch: None,
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
            state: Arc::new(EngineSharedState::with_workflow_store(storage, workflow_store)),
            caps,
            daemon_tool_dispatch: None,
            workspace_root: Some(workspace_root),
            nexus_home: None,
        }
    }

    /// Set the nexus home (`~/.nexus42`) used to resolve directory presets
    /// for source identity (A2/A7). Called by the daemon boot.
    pub fn set_nexus_home(&mut self, nexus_home: std::path::PathBuf) {
        self.nexus_home = Some(nexus_home);
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

    /// Recover persisted sessions into the in-memory tracker (WS2 R1 + R6).
    ///
    /// Called after engine construction on daemon restart. Queries
    /// `SqliteSessionStorage.list_non_terminal_sessions()` and repopulates
    /// the in-memory tracker.
    ///
    /// **R6 fix**: For each non-terminal session summary, reconstructs the
    /// `FlowRunner` from the embedded preset so that `run_step` succeeds
    /// after recovery (previously returned `NoGraphLoaded`).
    pub async fn recover_sessions(&self, summaries: Vec<SessionSummary>) {
        for summary in &summaries {
            // Skip terminal sessions — they don't need runners.
            if summary.status.is_terminal() {
                continue;
            }

            // Important 1: hydrate the in-memory children map from persisted
            // child rows so a restarted parent knows its child checkpoints
            // (with current revisions) for its next commit_transition.
            //
            // A corrupt/unsupported child (load_children failure, missing
            // child session snapshot) must propagate: the parent cannot be
            // reconstructed over an incomplete children map (A7). We surface
            // it as a warning and skip runner reconstruction — the session
            // stays tracked but not driven (non-replayable).
            if let Err(e) = self.state.hydrate_children(&summary.session_id).await {
                tracing::warn!(
                    "recovery: failed to hydrate children for session {}: {}; \
                     session marked non-replayable (no runner)",
                    summary.session_id.0,
                    e
                );
                continue;
            }

            // R6: Try to reconstruct the FlowRunner from the embedded preset.
            // The preset_id in the summary corresponds to the preset that was
            // used to start the session. We load it, wire the outer graph,
            // and create a FlowRunner pointing at the same storage (which
            // already has the persisted session data).
            if let Err(e) = self.reconstruct_runner(summary).await {
                tracing::warn!(
                    "R6: failed to reconstruct runner for session {}: {}; \
                     session will remain in tracker but run_step will fail until \
                     manually re-started",
                    summary.session_id.0,
                    e
                );
            }
        }

        // Add all summaries to the in-memory tracker (idempotent).
        self.state.recover_sessions(summaries).await;
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

    async fn reconstruct_runner(&self, summary: &SessionSummary) -> Result<(), EngineError> {
        // Load the durable descriptor (frozen at admission) so we honour the
        // persisted source identity and preset version (Important 5). A run
        // that has no v1 descriptor is a legacy/unverified row — its runner
        // cannot be reconstructed over a frozen source (A2/A7).
        let descriptor = match &self.state.workflow_store {
            Some(store) => {
                let record = store
                    .load_run(&summary.session_id)
                    .await?
                    .ok_or_else(|| {
                        EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                            "R6: session '{}' has no persisted run record; cannot reconstruct runner \
                             (non-replayable)",
                            summary.session_id.0
                        )))
                    })?;
                record.descriptor.ok_or_else(|| {
                    EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                        "R6: session '{}' has no v1 descriptor; cannot reconstruct runner over a \
                         frozen source (non-replayable)",
                        summary.session_id.0
                    )))
                })?
            }
            None => {
                // No workflow store (in-memory/test engine): fall back to the
                // legacy embedded-preset path (no durable descriptor).
                return self
                    .reconstruct_runner_legacy(summary)
                    .await;
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
                                summary.session_id.0, e
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
                                "R6: directory preset at '{:?}' is missing/invalid for session {}: {} \
                                 (reconstruction_unavailable, non-replayable)",
                                root, summary.session_id.0, e
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
                loaded.id, summary.session_id.0
            )))
        })?;
        if reloaded_identity != &descriptor.source {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "R6: preset source identity mismatch for session {} (frozen {:?} != reloaded {:?}) \
                     — source changed or moved; reconstruction_unavailable (non-replayable)",
                    summary.session_id.0, descriptor.source, reloaded_identity
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
                    summary.session_id.0, version, loaded.version
                )),
            ));
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
            .insert(summary.session_id.0.clone(), runner);

        tracing::info!(
            "R6: reconstructed runner for session {} (preset: {}, version: {})",
            summary.session_id.0,
            summary.preset_id,
            version
        );

        Ok(())
    }

    /// Legacy reconstruction against the current embedded preset (no workflow
    /// store). Used only by in-memory/test engines that never persisted a v1
    /// descriptor. This path is intentionally NOT frozen-source aware.
    async fn reconstruct_runner_legacy(
        &self,
        summary: &SessionSummary,
    ) -> Result<(), EngineError> {
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
                session.context.set("_creator_id", creator_id.to_string()).await;
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
    ) -> Option<SessionId> {
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
        );
        self.start_preset_run(loaded, creator_id, Arc::new(wired))
            .await
    }
}

// Re-export EngineSharedState for consumers (e.g., preset loader).
pub use EngineSharedState as SharedState;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graph_flow::InMemorySessionStorage;

    /// Helper: create a test engine with in-memory storage and built-in caps.
    fn test_engine() -> GraphFlowEngine {
        let storage: Arc<dyn SessionStorage> = Arc::new(InMemorySessionStorage::new());
        let caps = crate::capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        GraphFlowEngine::new_with_storage(storage, caps)
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
}
