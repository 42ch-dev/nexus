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
//! Design: `docs/specs/orchestration/orchestration-engine.md` §4.2.

use async_trait::async_trait;
use graph_flow::{ExecutionStatus, FlowRunner, Graph, SessionStorage};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

#[cfg(test)]
use nexus_capability::CapabilityError;
use nexus_capability::CapabilityRegistry;
use nexus_contracts::local::schedule::CoreContextPayload;
use nexus_preset::source_identity::PresetSourceIdentity;
use nexus_session_storage::run_state::{
    ChildCheckpoint, RunCheckpoint, RunDescriptorV1, RunFailure, RunRecord, RunStateV1,
    SettlementResult, TerminalSettlementTarget, WorkflowStateStore,
};
use nexus_session_storage::StorageError;

/// Context key that effectful tasks set when they perform an external effect
/// (capability call, ACP/Host prompt dispatch, `HostTool` call, child spawn).
///
/// `run_step_internal` inspects the post-step root context for this marker to
/// decide the disposition of a failed post-effect `commit_transition`: if an
/// external effect may have executed, the run is **interrupted** (never a
/// blind rewind to a seemingly safe boundary — A2/A7); if the step was
/// deterministic (no marker), a CAS-fenced restore is acceptable (Important 3).
pub const EXTERNAL_EFFECT_MARKER: &str = "__external_effect_ran";

/// Context key carrying the core-context version a run's snapshot was rendered
/// from.
///
/// This is the `{{core_context.version}}` binding the creator-schedule master
/// §6.4 names among the variables bound "from the current `core_context`
/// snapshot at the start of the state". Admission seeds it with the schedule
/// pointer the run was frozen against; the outer-state boundary refresh
/// (v1.195 P0-T6, §3.3/§6.4) advances it only when the schedule's committed
/// pointer has genuinely moved past it, so an unchanged pointer re-renders
/// nothing.
pub const CORE_CONTEXT_VERSION_KEY: &str = "core_context.version";

/// Context key carrying the FLAT core-context body.
///
/// Bound only while the run's committed payload kind is
/// [`CoreContextPayload::Text`] (creator-schedule master §6.4:
/// `{{core_context.text}}` — "the text body (if payload kind is `Text`)").
pub const CORE_CONTEXT_TEXT_KEY: &str = "core_context.text";

/// Context key carrying the STRUCTURED core-context body.
///
/// Bound only while the run's committed payload kind is
/// [`CoreContextPayload::Struct`] (§6.4: `{{core_context.struct.<path>}}` —
/// "dotted-path lookup (if payload kind is `Struct`)").
pub const CORE_CONTEXT_STRUCT_KEY: &str = "core_context.struct";

/// Upper bound on cancel-intent fence reload/retry attempts (Finding 1).
///
/// A cancel that loses the phase-1 revision CAS to a concurrent transition
/// (e.g. `mark_step_in_flight`) reloads the authoritative row and re-fences
/// against the new revision while the run is still cancellable. The bound
/// keeps the retry bounded: after it is exhausted the CAS loss is surfaced
/// so the caller projects the exact public conflict after reloading.
pub const CANCEL_FENCE_RETRY_BOUND: u32 = 8;

/// The core-context version a run's context was last rendered from.
///
/// Admission writes [`CORE_CONTEXT_VERSION_KEY`] with the schedule pointer the
/// run was frozen against; the boundary refresh advances it. A run that
/// predates the version binding has NO marker and reads as version 0 — its
/// schedule pointer then decides whether a refresh is due, which is exactly
/// the pre-binding admission semantics.
///
/// A marker that is PRESENT but is not the decimal string every writer emits
/// is corrupt metadata, not a legacy run: reinterpreting it as "no marker"
/// would silently compare `0` against the committed pointer and skip a
/// refresh the pointer demands. The caller therefore fails the boundary
/// closed instead.
///
/// # Errors
/// Returns the refusal reason when the marker exists with an unexpected shape.
fn applied_core_context_version(context: &graph_flow::Context) -> Result<u32, String> {
    let Some(marker) = context.get::<serde_json::Value>(CORE_CONTEXT_VERSION_KEY) else {
        return Ok(0);
    };
    match marker {
        serde_json::Value::String(raw) => raw
            .parse::<u32>()
            .map_err(|_| format!("core-context version marker '{raw}' is not a decimal version")),
        other => Err(format!(
            "core-context version marker has unexpected JSON type ({other}); \
             expected a decimal version string"
        )),
    }
}

/// Bind one committed `core_context` payload into a run's context under the
/// namespace its KIND owns (§6.4), dropping the other kind's binding.
///
/// The two bindings are mutually exclusive by contract: `{{core_context.text}}`
/// resolves only while the payload is [`CoreContextPayload::Text`], and
/// `{{core_context.struct.<path>}}` only while it is
/// [`CoreContextPayload::Struct`]. Leaving the previous kind bound would let a
/// payload the run no longer owns satisfy a template — and serializing a
/// structured body into the text namespace (or the reverse) would name a value
/// no contract promises. Both admission and the boundary refresh bind through
/// this one function so the two paths cannot drift (v1.195 P0-T6).
///
/// # Errors
/// Returns the context write error (an unserializable value) unchanged.
fn bind_core_context_payload(
    context: &graph_flow::Context,
    payload: &CoreContextPayload,
) -> Result<(), graph_flow::GraphError> {
    match payload {
        CoreContextPayload::Text { body } => {
            context.set(CORE_CONTEXT_TEXT_KEY, body.clone())?;
            context.remove(CORE_CONTEXT_STRUCT_KEY);
        }
        CoreContextPayload::Struct { body } => {
            context.set(CORE_CONTEXT_STRUCT_KEY, body.clone())?;
            context.remove(CORE_CONTEXT_TEXT_KEY);
        }
    }
    Ok(())
}

/// Whether the post-step root context carries the [`EXTERNAL_EFFECT_MARKER`],
/// indicating that one or more external effects executed during the step.
fn step_had_external_effect(root: &graph_flow::Session) -> bool {
    root.context
        .get::<bool>(EXTERNAL_EFFECT_MARKER)
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
fn clear_step_effect_marker(root: &graph_flow::Session) {
    root.context.remove(EXTERNAL_EFFECT_MARKER);
}

// engine internals import them privately and consumers name the owner directly.
use nexus_session_storage::{SessionId, SessionStatus, SessionSummary};

/// Composite key that uniquely identifies a session within the engine.
#[derive(Debug, Clone)]
pub struct SessionKey {
    pub creator_id: String,
    pub preset_id: String,
    pub instance_id: String,
}

/// Optional filters for [`OrchestrationEngine::list_active`].
#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    pub creator_id: Option<String>,
    pub preset_id: Option<String>,
}

/// Successful stopping point of a single engine step.
///
/// Failures use [`EngineError`], retaining any owned step witness.
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
    /// Failed-step cleanup: same bounded Host teardown as cancel, but the
    /// confirmed terminal durable status is `Failed` (dependency admission
    /// must not treat a prerequisite failure as a satisfied cancel).
    FailAnchored {
        expected_revision: u64,
        expected_graph_version: u64,
    },
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

/// Durable failure writes must CAS against
/// [`owned_revision`](Self::owned_revision) and the pre-step checkpoint —
/// never a post-error reload of the latest revision.
#[derive(Debug, Clone)]
pub struct FailedStepWitness {
    /// Revision owned by this step attempt after `mark_step_in_flight`
    /// (or the pre-step revision when no in-flight marker was written).
    pub owned_revision: u64,
    /// Graph clock owned at the witness anchor (`None` only for legacy
    /// paths that predate the graph column — never fabricated).
    pub owned_graph_version: Option<u64>,
    /// Durable status observed before the step began.
    pub pre_step_status: SessionStatus,
    /// Pre-step root checkpoint — the position/context before `runner.run`.
    pub pre_step_root: graph_flow::Session,
}

/// Structured outcome of an authoritative failure persistence attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailurePersistenceDisposition {
    /// Authoritative `RunFailure` committed at the witnessed revision.
    Committed,
    /// A concurrent writer already holds a terminal row legitimately.
    ExternalTerminalWinner,
    /// The witnessed revision no longer matches — the drive lost ownership.
    OwnershipLost,
    /// Persistence could not commit; the owner must be fenced.
    PersistenceFailed,
    /// No valid ownership anchor was available (fail-closed).
    NoAuthority,
    /// Legacy v0 context-only failure record: the durable row predates v1
    /// authority, so the only honest failure evidence is the persisted
    /// `_run_status`/`_run_error` context pair plus the A7 recovery skip it
    /// drives. No v1 workflow transition is attempted (a v0 row cannot be
    /// promoted or terminalized by a v1 writer) and no successful v1 cancel
    /// is claimed.
    LegacyContextOnly,
}

impl FailurePersistenceDisposition {
    /// Whether the drive may proceed with terminal cleanup (`Cancel`).
    #[must_use]
    pub const fn permits_terminal_cleanup(self) -> bool {
        matches!(self, Self::Committed)
    }

    /// Whether the coordinator should fence re-entry.
    #[must_use]
    pub const fn requires_owner_fence(self) -> bool {
        matches!(
            self,
            Self::PersistenceFailed | Self::NoAuthority | Self::LegacyContextOnly
        )
    }
}

/// Settlement context carried from the drive loop to the coordinator owner.
#[derive(Debug, Clone)]
pub struct FailureSettlement {
    pub witness: Option<FailedStepWitness>,
    pub authoritative: FailurePersistenceDisposition,
    pub context: FailurePersistenceDisposition,
    pub cleanup: FailurePersistenceDisposition,
}

/// Serializable settlement summary for drive outcomes (no `Session` clone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureSettlementSummary {
    pub authoritative: FailurePersistenceDisposition,
    pub context: FailurePersistenceDisposition,
    pub cleanup: FailurePersistenceDisposition,
}

impl FailureSettlementSummary {
    #[must_use]
    pub const fn from_settlement(settlement: &FailureSettlement) -> Self {
        Self {
            authoritative: settlement.authoritative,
            context: settlement.context,
            cleanup: settlement.cleanup,
        }
    }
}

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
    /// A step failed after the engine captured an ownership witness. The
    /// witness must be propagated to failure persistence — never discarded.
    #[error("step failed for session {session_id}: {source}")]
    StepFailed {
        /// Session id.
        session_id: String,
        /// Underlying failure.
        source: Box<Self>,
        /// Pre-attempt ownership anchor.
        witness: Box<FailedStepWitness>,
    },
}

impl EngineError {
    /// Return the failed-step witness when this error carries one.
    #[must_use]
    pub const fn step_witness(&self) -> Option<&FailedStepWitness> {
        match self {
            Self::StepFailed { witness, .. } => Some(witness),
            _ => None,
        }
    }

    /// Wrap `source` with a failed-step ownership witness.
    #[must_use]
    pub fn step_failed(session_id: &str, source: Self, witness: FailedStepWitness) -> Self {
        Self::StepFailed {
            session_id: session_id.to_string(),
            source: Box::new(source),
            witness: Box::new(witness),
        }
    }
}

impl From<StorageError> for EngineError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::GraphFlow(source) => Self::GraphFlow(source),
            StorageError::SessionNotFound(session_id) => Self::SessionNotFound(session_id),
            StorageError::RevisionMismatch {
                session_id,
                expected,
                found,
            } => Self::RevisionMismatch {
                session_id,
                expected,
                found,
            },
            StorageError::TerminalState(session_id) => Self::TerminalState(session_id),
            StorageError::RunAlreadyExists {
                session_id,
                execution_version,
            } => Self::RunAlreadyExists {
                session_id,
                execution_version,
            },
            StorageError::WaitConflict {
                session_id,
                status,
                current_wait_id,
            } => Self::WaitConflict {
                session_id,
                status,
                current_wait_id,
            },
        }
    }
}

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

    /// Live workspace state provider for wired preset graphs (v1.188 P3).
    ///
    /// Default `None`: in-memory/test engines expose no provider, so wired
    /// graphs fall back to the configured fixed state (if any).
    fn workspace_state_provider(
        &self,
    ) -> Option<std::sync::Arc<dyn nexus_capability::WorkspaceStateProvider>> {
        None
    }

    /// Start a session using a loaded preset (outer graph + inner graphs wired).
    async fn start_session_with_preset(
        &self,
        loaded: &nexus_preset::LoadedPreset,
    ) -> Result<SessionId, EngineError>;

    /// Start a session using a loaded preset and trusted creator identity.
    async fn start_session_with_preset_for_creator(
        &self,
        loaded: &nexus_preset::LoadedPreset,
        creator_id: &str,
    ) -> Result<SessionId, EngineError>;
}

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
    pub prompt_executor: Option<std::sync::Arc<dyn nexus_capability::PromptExecutor>>,
    /// Live workspace state provider (v1.188 P3) — shared by the engine and
    /// every `EngineProxy` so wired preset graphs resolve
    /// `_context.workspace.*` against the daemon's workspace authority.
    pub workspace_state_provider:
        Option<std::sync::Arc<dyn nexus_capability::WorkspaceStateProvider>>,
    /// Creator-DB core-context store (v1.195 P0-T6) — the durable
    /// `creator_schedules` pointer + immutable `core_context_versions` rows.
    ///
    /// When present, every outer state boundary of a schedule-owned run
    /// re-reads the committed pointer and renders the version it names
    /// ([`EngineSharedState::refresh_core_context_at_boundary`]). `None` for
    /// engines with no creator-DB core-context store (in-memory/test engines,
    /// session-only callers): no boundary refresh is attempted.
    pub core_context_store: Option<Arc<crate::schedule::derivation::CoreContextManager>>,
    /// Applied-identity source (P4) — recovery resolves a run's current apply
    /// through the SAME route admission uses, so a restart mid-run reconstructs
    /// the applied graph instead of the descriptor's. `None` on engines that
    /// never wire it (in-memory/test engines): recovery keeps the
    /// descriptor-only behaviour.
    pub applied_document_source: Option<Arc<dyn crate::applied_document::AppliedDocumentSource>>,
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
            workspace_state_provider: None,
            core_context_store: None,
            applied_document_source: None,
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
            workspace_state_provider: None,
            core_context_store: None,
            applied_document_source: None,
        }
    }

    /// Set the live workspace state provider (v1.188 P3).
    ///
    /// Stored on the SHARED state (not the engine) so `EngineProxy` — the
    /// handle the preset loader receives at graph-build time — resolves the
    /// same provider the daemon wired.
    pub fn set_workspace_state_provider(
        &mut self,
        provider: std::sync::Arc<dyn nexus_capability::WorkspaceStateProvider>,
    ) {
        self.workspace_state_provider = Some(provider);
    }

    /// Wire the creator-DB core-context store used by the outer-state boundary
    /// refresh (v1.195 P0-T6).
    ///
    /// Called once at daemon boot with the same Creator DB pool the schedule
    /// supervisor reads its pointers through. Engines that never wire it keep
    /// the pre-existing behavior (no boundary refresh).
    pub fn set_core_context_store(
        &mut self,
        store: Arc<crate::schedule::derivation::CoreContextManager>,
    ) {
        self.core_context_store = Some(store);
    }

    /// Wire the applied-identity source recovery reads (P4).
    ///
    /// Kept on the SHARED state so every handle recovery runs through (boot
    /// re-drive and the on-demand attach for a resumed human wait) resolves the
    /// same source.
    pub fn set_applied_document_source(
        &mut self,
        source: Arc<dyn crate::applied_document::AppliedDocumentSource>,
    ) {
        self.applied_document_source = Some(source);
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
        let children = match store.load_children(parent_session_id).await {
            Ok(children) => children,
            Err(e) => {
                self.children.write().await.remove(&parent_session_id.0);
                return Err(e.into());
            }
        };
        let parent = self
            .storage
            .get(&parent_session_id.0)
            .await
            .map_err(EngineError::GraphFlow)?
            .ok_or_else(|| EngineError::SessionNotFound(parent_session_id.0.clone()))?;
        let parent_record = match store.load_run(parent_session_id).await {
            Ok(record) => record,
            Err(e) => {
                self.children.write().await.remove(&parent_session_id.0);
                return Err(e.into());
            }
        };
        if let Some(parent_record) = parent_record {
            for child in &children {
                if let Err(e) = validate_child_descriptor_identity(
                    parent_session_id,
                    parent_record.descriptor.as_ref(),
                    child,
                ) {
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
            crate::resume_rules::context_data(&parent_context).map_or_default(|data| {
                data.iter()
                    .filter(|(key, _)| key.starts_with("_inner_child_session_"))
                    .filter_map(|(_, value)| value.as_str().map(str::to_owned))
                    .collect()
            });
        let mut checkpoints = Vec::with_capacity(children.len());
        for child in &children {
            if child.status.is_terminal()
                && !active_terminal_children
                    .iter()
                    .any(|id| id == &child.session_id.0)
            {
                continue;
            }
            let Ok(Some(child_session)) = self
                .storage
                .get(&child.session_id.0)
                .await
                .map_err(EngineError::GraphFlow)
            else {
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
        let cleanup_terminal = match signal {
            EngineSignal::Cancel => Some(SessionStatus::Cancelled),
            EngineSignal::FailAnchored { .. } => Some(SessionStatus::Failed),
            _ => None,
        };
        let cancel_allow_rebase = !matches!(
            signal,
            EngineSignal::FailAnchored {
                expected_revision: _,
                expected_graph_version: _,
            }
        );
        let target_status = match signal {
            EngineSignal::Pause => SessionStatus::Paused,
            EngineSignal::Cancel => SessionStatus::Cancelled,
            EngineSignal::FailAnchored { .. } => SessionStatus::Failed,
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

        let cancel_anchor = if let EngineSignal::FailAnchored {
            expected_revision: anchor_revision,
            expected_graph_version: anchor_graph,
        } = *signal
        {
            if record.state_revision != anchor_revision || record.graph_version != anchor_graph {
                return Err(EngineError::RevisionMismatch {
                    session_id: session_id.0.clone(),
                    expected: anchor_revision,
                    found: record.state_revision,
                });
            }
            Some((anchor_revision, anchor_graph))
        } else {
            None
        };

        if record.status.is_terminal() {
            let interrupted_cancel_retry = matches!(
                signal,
                EngineSignal::Cancel | EngineSignal::FailAnchored { .. }
            ) && record.status == SessionStatus::Interrupted
                && record.state.as_ref().is_some_and(|s| s.cancel_requested);
            if !interrupted_cancel_retry {
                return Err(EngineError::TerminalState(session_id.0.clone()));
            }
        }
        let mut next_state = record.state.unwrap_or_default();
        if next_state.cancel_requested
            && !matches!(
                signal,
                EngineSignal::Cancel | EngineSignal::FailAnchored { .. }
            )
        {
            return Err(EngineError::TerminalState(session_id.0.clone()));
        }
        if !matches!(
            signal,
            EngineSignal::Cancel
                | EngineSignal::FailAnchored { .. }
                | EngineSignal::Continue { .. }
        ) && (matches!(record.status, SessionStatus::WaitingForInput)
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

        if let EngineSignal::Continue { wait_id } = signal {
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

            next_state.wait = None;
            let checkpoint = RunCheckpoint {
                root: &root,
                children: &children,
            };
            store
                .commit_transition_with_graph_fence(
                    session_id,
                    expected_revision,
                    Some(root.version),
                    checkpoint,
                    SessionStatus::Running,
                    &next_state,
                )
                .await?;

            for child in &children {
                self.sync_child_checkpoint_after_step(&SessionId(child.session.id.clone()))
                    .await?;
            }
            return Ok(SessionStatus::Running);
        }

        let checkpoint = RunCheckpoint {
            root: &root,
            children: &[],
        };

        if let Some(mut cleanup_terminal) = cleanup_terminal {
            let interrupted_retry = record.status == SessionStatus::Interrupted;

            let mut anchored_clocks: Option<(u64, u64)> = cancel_anchor;

            if !interrupted_retry {
                let mut fence_revision = expected_revision;
                let mut fence_status = record.status.clone();
                let mut fence_state = next_state.clone();
                fence_state.cancel_requested = true;
                let mut attempts: u32 = 0;
                loop {
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
                    let fence_graph = cancel_anchor.map_or(fence_root.version, |(_, graph)| graph);
                    let fence_result = store
                        .commit_transition_with_graph_fence(
                            session_id,
                            fence_revision,
                            Some(fence_graph),
                            fence_checkpoint,
                            fence_status.clone(),
                            &fence_state,
                        )
                        .await;
                    match fence_result {
                        Ok(record) => {
                            if cancel_anchor.is_some() {
                                anchored_clocks =
                                    Some((record.state_revision, record.graph_version));
                            }
                            break;
                        }
                        Err(StorageError::RevisionMismatch { .. }) => {
                            if !cancel_allow_rebase {
                                return Err(EngineError::RevisionMismatch {
                                    session_id: session_id.0.clone(),
                                    expected: fence_revision,
                                    found: fence_revision,
                                });
                            }
                            attempts += 1;
                            if attempts >= CANCEL_FENCE_RETRY_BOUND {
                                return Err(EngineError::RevisionMismatch {
                                    session_id: session_id.0.clone(),
                                    expected: fence_revision,
                                    found: fence_revision,
                                });
                            }
                            let Some(current) = store.load_run(session_id).await? else {
                                return Err(EngineError::GraphFlow(
                                    graph_flow::GraphError::StorageError(format!(
                                        "run '{}' disappeared during cancel fence",
                                        session_id.0
                                    )),
                                ));
                            };
                            if current.status.is_terminal() {
                                return Err(EngineError::TerminalState(session_id.0.clone()));
                            }
                            fence_revision = current.state_revision;
                            fence_status = current.status.clone();
                            fence_state = current.state.unwrap_or_default();
                            fence_state.cancel_requested = true;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
            }

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
                    if let Some(store) = &self.workflow_store {
                        let persisted = store.load_children(&SessionId(parent.clone())).await?;
                        for child in persisted {
                            let child_id = child.session_id.0;
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

            if let Some(executor) = &self.prompt_executor {
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
                    let reason = if cleanup_terminal == SessionStatus::Failed {
                        format!(
                            "failure cleanup unconfirmed for run '{}': {e}",
                            session_id.0
                        )
                    } else {
                        format!("cancel cleanup unconfirmed for run '{}': {e}", session_id.0)
                    };
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
                        if interrupted_state
                            .failure
                            .as_ref()
                            .is_none_or(|f| f.code != "driver_failed")
                        {
                            interrupted_state.failure = Some(RunFailure {
                                code: if cleanup_terminal == SessionStatus::Failed {
                                    "failure_cleanup_unconfirmed".to_string()
                                } else {
                                    "cancel_cleanup_unconfirmed".to_string()
                                },
                                message: reason.clone(),
                            });
                        }
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
                        let (interrupted_revision, interrupted_graph) =
                            anchored_clocks.unwrap_or((current_revision, interrupted_root.version));
                        let interrupted_result = store
                            .commit_transition_with_graph_fence(
                                session_id,
                                interrupted_revision,
                                Some(interrupted_graph),
                                interrupted_checkpoint,
                                SessionStatus::Interrupted,
                                &interrupted_state,
                            )
                            .await;
                        if let Ok(record) = &interrupted_result {
                            anchored_clocks = Some((record.state_revision, record.graph_version));
                        }
                        match interrupted_result {
                            Ok(_) => break,
                            Err(StorageError::RevisionMismatch { .. }) => {
                                if !cancel_allow_rebase {
                                    return Err(EngineError::RevisionMismatch {
                                        session_id: session_id.0.clone(),
                                        expected: current_revision,
                                        found: current_revision,
                                    });
                                }
                                interrupted_attempts += 1;
                                if interrupted_attempts >= CANCEL_FENCE_RETRY_BOUND {
                                    return Err(EngineError::RevisionMismatch {
                                        session_id: session_id.0.clone(),
                                        expected: current_revision,
                                        found: current_revision,
                                    });
                                }
                            }
                            Err(err) => return Err(err.into()),
                        }
                    }
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
                if current
                    .state
                    .as_ref()
                    .and_then(|state| state.failure.as_ref())
                    .is_some_and(|failure| failure.code == "driver_failed")
                {
                    cleanup_terminal = SessionStatus::Failed;
                }
                if current.status == cleanup_terminal {
                    if let Some((owned_revision, owned_graph)) = anchored_clocks {
                        if current.state_revision != owned_revision
                            || current.graph_version != owned_graph
                        {
                            return Err(EngineError::RevisionMismatch {
                                session_id: session_id.0.clone(),
                                expected: owned_revision,
                                found: current.state_revision,
                            });
                        }
                    }
                    return Ok(cleanup_terminal);
                }
                let interrupted_retry_shape = current.status == SessionStatus::Interrupted
                    && current.state.as_ref().is_some_and(|s| s.cancel_requested);
                if current.status.is_terminal() && !interrupted_retry_shape {
                    return Err(EngineError::TerminalState(session_id.0.clone()));
                }
                let current_revision = current.state_revision;
                let mut cancelled_state = current.state.unwrap_or_default();
                cancelled_state.cancel_requested = true;
                cancelled_state.in_flight = None;
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
                let (settle_revision, settle_graph) =
                    anchored_clocks.unwrap_or((current_revision, settle_root.version));
                let terminal_target = if cleanup_terminal == SessionStatus::Cancelled {
                    TerminalSettlementTarget::Cancelled
                } else {
                    TerminalSettlementTarget::Failed
                };
                let settle_result = store
                    .settle_run(
                        session_id,
                        settle_revision,
                        Some(settle_graph),
                        settle_checkpoint,
                        &cancelled_state,
                        terminal_target,
                    )
                    .await;
                match settle_result {
                    Ok(SettlementResult::Applied(record)) => {
                        cleanup_terminal = record.status;
                        break;
                    }
                    Ok(SettlementResult::Observed(record)) => {
                        return Ok(record.status);
                    }
                    Err(StorageError::RevisionMismatch { .. }) => {
                        if !cancel_allow_rebase {
                            return Err(EngineError::RevisionMismatch {
                                session_id: session_id.0.clone(),
                                expected: current_revision,
                                found: current_revision,
                            });
                        }
                        settle_attempts += 1;
                        if settle_attempts >= CANCEL_FENCE_RETRY_BOUND {
                            return Err(EngineError::RevisionMismatch {
                                session_id: session_id.0.clone(),
                                expected: current_revision,
                                found: current_revision,
                            });
                        }
                    }
                    Err(e) => return Err(e.into()),
                }
            }
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
            return Ok(cleanup_terminal);
        }

        store
            .commit_transition_with_graph_fence(
                session_id,
                expected_revision,
                Some(root.version),
                checkpoint,
                target_status.clone(),
                &next_state,
            )
            .await?;
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
        let parent_id: Option<String> = match self.storage.get(&session_id.0).await {
            Ok(Some(session)) => session.context.get("_parent_session_id"),
            Ok(None) => None,
            Err(e) => return Err(EngineError::GraphFlow(e)),
        };
        let Some(parent_id) = parent_id else {
            return Ok(());
        };
        let Some(store) = &self.workflow_store else {
            return Ok(());
        };
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

    /// Refresh a run's `core_context` snapshot at the OUTER STATE BOUNDARY it
    /// is about to enter (v1.195 P0-T6; creator-schedule master §3.3/§6.4).
    ///
    /// A schedule edit made while its run is mid-execution commits a new
    /// immutable version and advances the schedule pointer WITHOUT interrupting
    /// the in-flight state: "the current state finishes on the old snapshot;
    /// the next state's enter evaluates template bindings against the new
    /// version". This is that read — it runs immediately BEFORE the boundary's
    /// step, so the step's `StateCompositeTask` renders whatever the committed
    /// pointer names.
    ///
    /// Scope and failure semantics:
    ///
    /// - No wired store, or no schedule owns this run (`session-POST` runs and
    ///   nested children) ⇒ no-op: those runs carry no core-context contract.
    /// - The snapshot is the EXACT version the schedule pointer names, read
    ///   through [`CoreContextManager::head_for_run`] — never the newest row,
    ///   never an in-memory or test substitute.
    /// - A store failure, an unreadable/missing named version, a corrupt
    ///   payload, a corrupt applied-version marker, or a run marker ahead of
    ///   the committed pointer FAILS CLOSED: the boundary is refused rather
    ///   than rendered from a stale or guessed snapshot.
    /// - Exactly one payload namespace is (re)bound — `core_context.text` for
    ///   a `Text` version, `core_context.struct` for a `Struct` version — plus
    ///   the applied-version marker, and only while the run sits at a boundary
    ///   (no step in flight). The other kind's binding is dropped so a template
    ///   can never read a payload the run no longer owns. An unchanged pointer
    ///   writes nothing at all.
    ///
    /// # Errors
    /// Returns [`EngineError`] when the committed snapshot cannot be read or
    /// the refreshed context cannot be persisted.
    async fn refresh_core_context_at_boundary(
        &self,
        session_id: &SessionId,
    ) -> Result<(), EngineError> {
        let Some(store) = &self.core_context_store else {
            return Ok(());
        };
        let Some(committed) = store.head_for_run(&session_id.0).await.map_err(|e| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "run '{}': core-context boundary read failed: {e}",
                session_id.0
            )))
        })?
        else {
            return Ok(());
        };

        let session = self
            .storage
            .get(&session_id.0)
            .await
            .map_err(EngineError::GraphFlow)?
            .ok_or_else(|| EngineError::SessionNotFound(session_id.0.clone()))?;
        let applied = applied_core_context_version(&session.context).map_err(|reason| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "run '{}': {reason}; refusing the boundary (corrupt version metadata)",
                session_id.0
            )))
        })?;
        let committed_version = committed.version.0;
        if applied > committed_version {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "run '{}': applied core-context version {applied} is ahead of the \
                     committed pointer {committed_version}; refusing the boundary \
                     (ownership mismatch)",
                    session_id.0
                )),
            ));
        }
        if applied == committed_version {
            return Ok(());
        }

        bind_core_context_payload(&session.context, &committed.content)
            .map_err(EngineError::GraphFlow)?;
        session
            .context
            .set(CORE_CONTEXT_VERSION_KEY, committed_version.to_string())
            .map_err(EngineError::GraphFlow)?;
        self.storage
            .save(session)
            .await
            .map_err(EngineError::GraphFlow)?;
        tracing::info!(
            session_id = %session_id.0,
            schedule_id = %committed.schedule_id,
            from_version = applied,
            to_version = committed_version,
            payload_kind = match &committed.content {
                CoreContextPayload::Text { .. } => "text",
                CoreContextPayload::Struct { .. } => "struct",
            },
            "outer state boundary refreshed core context from the committed pointer"
        );
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
        let runner = {
            let runners = self.runners.read().await;
            let found = runners.get(&session_id.0).cloned();
            drop(runners);
            if let Some(runner) = found {
                runner
            } else {
                if let Some(store) = &self.workflow_store {
                    if let Ok(Some(record)) = store.load_run(session_id).await {
                        if record.execution_version < 1 {
                            tracing::warn!(
                                session_id = %session_id.0,
                                "run_step: legacy v0 row is not steppable under v1 authority (non-replayable)"
                            );
                            return Err(EngineError::GraphFlow(
                                graph_flow::GraphError::StorageError(format!(
                                    "run '{}': legacy execution_version 0 is not steppable under v1 authority (non-replayable)",
                                    session_id.0
                                )),
                            ));
                        }
                    }
                }
                return Err(EngineError::NoGraphLoaded);
            }
        };

        let mut step_witness: Option<FailedStepWitness> = None;

        self.refresh_core_context_at_boundary(session_id).await?;

        let (expected_revision, pre_step_status, pre_execution_version) =
            if let Some(store) = &self.workflow_store {
                match store.load_run(session_id).await {
                    Ok(Some(r)) => (r.state_revision, r.status, Some(r.execution_version)),
                    Ok(None) => (0, SessionStatus::Running, None),
                    Err(e) => return Err(e.into()),
                }
            } else {
                let status = self
                    .sessions
                    .read()
                    .await
                    .iter()
                    .find(|s| s.session_id == *session_id)
                    .map_or(SessionStatus::Running, |s| s.status.clone());
                (0, status, None)
            };
        let mut transition_revision = expected_revision;

        let pre_step_root = match self.storage.get(&session_id.0).await {
            Ok(session) => session,
            Err(e) => return Err(EngineError::GraphFlow(e)),
        };

        //
        // - A workflow-store run whose root session cannot be read has no
        //   durable pre-step checkpoint and can therefore never be marked,
        //   witnessed, or replayed. Refuse with the typed non-replayable
        //   error instead of dispatching a step that would leave the effect
        //   boundary ambiguous.
        // - A legacy v0 row (`execution_version < 1`) carries NO v1 authority:
        //   a v1 marker/transition may neither mutate nor promote it, so it is
        //   never stepped under a workflow store. Its failure contract is
        //   context-only.
        //
        // No root is synthesized and no witness is fabricated.
        if self.workflow_store.is_some() {
            if matches!(pre_execution_version, Some(v) if v < 1) {
                tracing::warn!(
                    session_id = %session_id.0,
                    "run_step: legacy v0 row is not steppable under v1 authority; refusing dispatch (non-replayable)"
                );
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "run '{}': legacy execution_version 0 is not steppable under v1 authority (non-replayable)",
                        session_id.0
                    )),
                ));
            }
            if pre_step_root.is_none() {
                tracing::warn!(
                    session_id = %session_id.0,
                    "run_step: no pre-step root session for a workflow-store run; refusing dispatch (non-replayable)"
                );
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "run '{}': no pre-step root session to mark in flight (non-replayable)",
                        session_id.0
                    )),
                ));
            }
        }

        if let (Some(store), Some(pre)) = (&self.workflow_store, pre_step_root.as_ref()) {
            expected_revision.checked_add(1).ok_or_else(|| {
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
            match store
                .mark_step_in_flight(
                    session_id,
                    expected_revision,
                    Some(pre.version),
                    in_flight_checkpoint,
                    &in_flight_state,
                )
                .await
            {
                Err(
                    e @ (StorageError::RevisionMismatch { .. }
                    | StorageError::TerminalState(_)
                    | StorageError::GraphFlow(graph_flow::GraphError::SessionConflict(_))),
                ) => {
                    return Err(e.into());
                }
                Err(e) => {
                    let witness = FailedStepWitness {
                        owned_revision: expected_revision,
                        owned_graph_version: Some(pre.version),
                        pre_step_status: pre_step_status.clone(),
                        pre_step_root: pre.clone(),
                    };
                    return Err(EngineError::step_failed(&session_id.0, e.into(), witness));
                }
                Ok(marker_record) => {
                    transition_revision = marker_record.state_revision;
                    step_witness = Some(FailedStepWitness {
                        owned_revision: marker_record.state_revision,
                        owned_graph_version: Some(marker_record.graph_version),
                        pre_step_status: pre_step_status.clone(),
                        pre_step_root: pre.clone(),
                    });
                }
            }
        }

        let wrap_step_boundary_err = |err: EngineError| -> EngineError {
            if matches!(
                err,
                EngineError::RevisionMismatch { .. } | EngineError::TerminalState(_)
            ) {
                return err;
            }
            if let Some(witness) = step_witness.clone() {
                return EngineError::step_failed(&session_id.0, err, witness);
            }
            if let Some(pre) = pre_step_root.as_ref() {
                return EngineError::step_failed(
                    &session_id.0,
                    err,
                    FailedStepWitness {
                        owned_revision: transition_revision,
                        owned_graph_version: Some(pre.version),
                        pre_step_status: pre_step_status.clone(),
                        pre_step_root: pre.clone(),
                    },
                );
            }
            err
        };

        let result = match runner.run(&session_id.0).await {
            Ok(result) => result,
            Err(e) => {
                let witness = step_witness.clone().map_or_else(
                    || {
                        pre_step_root.as_ref().map(|pre| FailedStepWitness {
                            owned_revision: transition_revision,
                            owned_graph_version: Some(pre.version),
                            pre_step_status: pre_step_status.clone(),
                            pre_step_root: pre.clone(),
                        })
                    },
                    Some,
                );
                return if let Some(witness) = witness {
                    Err(EngineError::step_failed(
                        &session_id.0,
                        EngineError::GraphFlow(e),
                        witness,
                    ))
                } else {
                    tracing::warn!(
                        session_id = %session_id.0,
                        "run_step: step failed with no pre-step root checkpoint; no ownership anchor exists (non-replayable)"
                    );
                    Err(EngineError::GraphFlow(e))
                };
            }
        };

        let mut status = match &result.status {
            ExecutionStatus::Completed => SessionStatus::Completed,
            ExecutionStatus::WaitingForInput => SessionStatus::WaitingForInput,
            ExecutionStatus::Paused { .. } => SessionStatus::Paused,
        };

        if let Some(store) = &self.workflow_store {
            if status.is_terminal()
                || matches!(
                    status,
                    SessionStatus::WaitingForInput | SessionStatus::Paused
                )
            {
                let root = self
                    .storage
                    .get(&session_id.0)
                    .await
                    .map_err(|e| wrap_step_boundary_err(EngineError::GraphFlow(e)))?
                    .ok_or_else(|| {
                        wrap_step_boundary_err(EngineError::SessionNotFound(session_id.0.clone()))
                    })?;

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

                let next_state = build_step_state(&status, &root.current_task_id, &root.context);
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
                            .map_err(|e| wrap_step_boundary_err(EngineError::GraphFlow(e)))?;
                    }
                }
                let checkpoint = RunCheckpoint {
                    root: &root,
                    children: &children,
                };
                let had_effect = step_had_external_effect(&root);
                if had_effect {
                    clear_step_effect_marker(&root);
                }
                let commit_result: Result<(bool, SessionStatus), EngineError> =
                    if status == SessionStatus::Completed {
                        match store
                            .settle_run(
                                session_id,
                                transition_revision,
                                Some(root.version),
                                checkpoint,
                                &next_state,
                                TerminalSettlementTarget::Completed,
                            )
                            .await
                        {
                            Ok(SettlementResult::Applied(record)) => Ok((true, record.status)),
                            Ok(SettlementResult::Observed(record)) => Ok((false, record.status)),
                            Err(e) => Err(e.into()),
                        }
                    } else {
                        store
                            .commit_transition_with_graph_fence(
                                session_id,
                                transition_revision,
                                Some(root.version),
                                checkpoint,
                                status.clone(),
                                &next_state,
                            )
                            .await
                            .map(|_| (true, status.clone()))
                            .map_err(EngineError::from)
                    };
                match commit_result {
                    Ok((owned_commit, durable_status)) => {
                        status = durable_status;
                        if owned_commit {
                            for child in &children {
                                if let Err(e) = self
                                    .sync_child_checkpoint_after_step(&SessionId(
                                        child.session.id.clone(),
                                    ))
                                    .await
                                {
                                    return Err(wrap_step_boundary_err(e));
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
                        } else {
                            // cleanup to the owner that won the CAS.
                        }
                    }
                    Err(commit_err) => {
                        if had_effect {
                            let interrupted_state = RunStateV1 {
                                step_in_flight: Some(root.current_task_id.clone()),
                                ..RunStateV1::default()
                            };
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
                                .commit_transition_with_graph_fence(
                                    session_id,
                                    transition_revision,
                                    Some(root.version),
                                    interrupted_checkpoint,
                                    SessionStatus::Interrupted,
                                    &interrupted_state,
                                )
                                .await;

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
                            return Err(wrap_step_boundary_err(commit_err));
                        }

                        let mut restored_witness: Option<FailedStepWitness> = None;
                        if let Some(pre) = &pre_step_root {
                            match store
                                .restore_pre_step(session_id, transition_revision, pre)
                                .await
                            {
                                Ok(restored) => {
                                    restored_witness = Some(FailedStepWitness {
                                        owned_revision: restored.state_revision,
                                        owned_graph_version: Some(restored.graph_version),
                                        pre_step_status: restored.status,
                                        pre_step_root: pre.clone(),
                                    });
                                }
                                // a newer control/terminal state legitimately
                                // holds the row. Preserve the original commit
                                // error as the truthful outcome; the winner is
                                // untouched.
                                Err(
                                    StorageError::RevisionMismatch { .. }
                                    | StorageError::TerminalState(_),
                                ) => {}
                                Err(restore_err) => {
                                    return Err(wrap_step_boundary_err(EngineError::GraphFlow(
                                        graph_flow::GraphError::StorageError(format!(
                                            "step commit failed ({commit_err}) and the \
                                             pre-step restore also failed: {restore_err}"
                                        )),
                                    )));
                                }
                            }
                        }
                        if matches!(
                            commit_err,
                            EngineError::RevisionMismatch { .. } | EngineError::TerminalState(_)
                        ) {
                            return Err(commit_err);
                        }
                        return Err(match restored_witness {
                            Some(witness) => {
                                EngineError::step_failed(&session_id.0, commit_err, witness)
                            }
                            None => wrap_step_boundary_err(commit_err),
                        });
                    }
                }
            }
        }

        if let Err(e) = self.sync_child_checkpoint_after_step(session_id).await {
            return Err(wrap_step_boundary_err(e));
        }

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
        let child_session_id = format!(
            "{}:child:{}",
            params.parent_session_id,
            uuid::Uuid::new_v4()
        );
        let start_task_id = params
            .inner_graph
            .start_task_id()
            .unwrap_or_default()
            .to_string();
        let mut session_mut =
            graph_flow::Session::new_from_task(child_session_id.clone(), &start_task_id);
        session_mut.context = params.initial_context;
        session_mut
            .context
            .set("_session_id", child_session_id.clone())?;
        session_mut
            .context
            .set("_parent_session_id", params.parent_session_id.clone())?;

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
                applied: root_descriptor.applied.clone(),
                intake_mode: root_descriptor.intake_mode,
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

        self.register_cancellation(&SessionId(child_session_id.clone()));

        let child_revision = u64::from(self.workflow_store.is_some());
        let persisted_child = self
            .storage
            .get(&child_session_id)
            .await
            .map_err(EngineError::GraphFlow)?
            .ok_or_else(|| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "spawn_child_session '{child_session_id}': persisted child session \
                     missing immediately after admission (non-replayable)"
                )))
            })?;
        let child_checkpoint = ChildCheckpoint {
            session: persisted_child,
            status: SessionStatus::Running,
            state: RunStateV1::default(),
            state_revision: child_revision,
            graph_name: Some(params.inner_graph.id.clone()),
        };

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
                    {
                        let cancels = self
                            .session_cancels
                            .read()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        if let Some(token) = cancels.get(&child_session_id).cloned() {
                            token.cancel();
                        }
                    }
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
                            break;
                        }
                        let interrupted_retry_shape = current.status == SessionStatus::Interrupted
                            && current.state.as_ref().is_some_and(|s| s.cancel_requested);
                        if current.status.is_terminal() && !interrupted_retry_shape {
                            return Err(EngineError::TerminalState(child_session_id.clone()));
                        }
                        let current_revision = current.state_revision;
                        let mut cancelled_state = current.state.unwrap_or_default();
                        cancelled_state.cancel_requested = true;
                        cancelled_state.in_flight = None;
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
                            .settle_run(
                                &child_sid,
                                current_revision,
                                None,
                                settle_checkpoint,
                                &cancelled_state,
                                TerminalSettlementTarget::Cancelled,
                            )
                            .await
                        {
                            Ok(SettlementResult::Applied(_) | SettlementResult::Observed(_)) => {
                                break;
                            }
                            Err(StorageError::RevisionMismatch { .. }) => {
                                settle_attempts += 1;
                                if settle_attempts >= CANCEL_FENCE_RETRY_BOUND {
                                    return Err(EngineError::RevisionMismatch {
                                        session_id: child_session_id.clone(),
                                        expected: current_revision,
                                        found: current_revision,
                                    });
                                }
                            }
                            Err(e) => return Err(e.into()),
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
        if !self.runners.read().await.contains_key(&child_id) {
            let runner = Arc::new(FlowRunner::new(inner_graph, self.storage.clone()));
            self.runners.write().await.insert(child_id.clone(), runner);
        }
        self.register_cancellation(&SessionId(child_id.clone()));
        let (parent_creator_id, parent_preset_id) = self
            .sessions
            .read()
            .await
            .iter()
            .find(|summary| summary.session_id.0 == parent_session_id)
            .map_or_default(|summary| (summary.creator_id.clone(), summary.preset_id.clone()));
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
    manifest: &'a nexus_preset::manifest::PresetManifest,
    state_id: &str,
) -> Option<&'a str> {
    manifest
        .states
        .iter()
        .find(|state| state.id == state_id)
        .and_then(|state| {
            state.enter.iter().find_map(|action| match action {
                nexus_preset::manifest::EnterAction::InnerGraph { name } => Some(name.as_str()),
                _ => None,
            })
        })
}

/// The applied document an embedded preset executes: the document its own
/// compiled-in bundle constructs, under the compass D10 identity the applied
/// route freezes.
///
/// The bytes are compiled into this binary, so an engine with no configured
/// [`AppliedDocumentSource`](crate::applied_document::AppliedDocumentSource) can
/// still resolve the document the preset runs. A bundle whose document cannot
/// be constructed is the refusal below, named by preset — its YAML manifest is
/// never an execution input.
fn embedded_applied_document(
    loaded: &nexus_preset::LoadedPreset,
) -> Result<crate::applied_document::AppliedDocument, String> {
    let document = nexus_preset::embedded_workflow_document(loaded)
        .map_err(|error| format!("embedded_document_unavailable: {error}"))?;
    let (apply_id, content_hash) = nexus_preset::embedded_applied_identity(&loaded.id, &document)
        .map_err(|error| {
        format!(
            "embedded_document_unavailable: document of `{}` cannot be hashed: {error}",
            loaded.id
        )
    })?;
    Ok(crate::applied_document::AppliedDocument {
        identity: nexus_session_storage::run_state::AppliedRunIdentity {
            apply_id,
            content_hash,
        },
        document,
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
    status: &SessionStatus,
    current_task_id: &str,
    root_context: &graph_flow::Context,
) -> RunStateV1 {
    let failure: Option<nexus_session_storage::run_state::RunFailure> = None;
    let (child_session_id, child_task_id) = if matches!(status, SessionStatus::WaitingForInput) {
        (
            root_context.get("_child_wait_session"),
            root_context.get("_child_wait_task"),
        )
    } else {
        (None, None)
    };
    let wait = if matches!(status, SessionStatus::WaitingForInput) {
        Some(nexus_session_storage::run_state::WaitRecord {
            wait_id: uuid::Uuid::new_v4().to_string(),
            task_id: current_task_id.to_string(),
            child_session_id,
            child_task_id,
            kind: nexus_session_storage::run_state::WaitKind::Manual,
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

///
/// Used by `start_session_with_preset` when we need `Arc<dyn OrchestrationEngine>`
/// to pass to preset loader. The proxy delegates all operations to the shared
/// state, eliminating code duplication.
struct EngineProxy {
    state: Arc<EngineSharedState>,
}

#[async_trait]
impl OrchestrationEngine for EngineProxy {
    fn workspace_state_provider(
        &self,
    ) -> Option<std::sync::Arc<dyn nexus_capability::WorkspaceStateProvider>> {
        self.state.workspace_state_provider.clone()
    }

    async fn run_step(&self, session_id: &SessionId) -> Result<StepOutcome, EngineError> {
        let result = self.state.run_step_internal(session_id).await?;

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
        let start_task_id = graph.start_task_id().unwrap_or_default().to_string();
        let session = graph_flow::Session::new_from_task(session_id.clone(), &start_task_id);
        session.context.set("_session_id", session_id.clone())?;
        if self.state.workflow_store.is_some() {
            return Err(EngineError::GraphFlow(graph_flow::GraphError::StorageError(
                "start_session_with_graph cannot create a recoverable run without a preset descriptor"
                    .to_string(),
            )));
        }
        self.state.storage.save(session).await?;
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
        self.state.recover_sessions(summaries).await;
    }

    async fn ensure_recovered_runner(&self, session_id: &SessionId) -> Result<(), EngineError> {
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
        _loaded: &nexus_preset::LoadedPreset,
    ) -> Result<SessionId, EngineError> {
        Err(EngineError::NoGraphLoaded)
    }

    async fn start_session_with_preset_for_creator(
        &self,
        _loaded: &nexus_preset::LoadedPreset,
        _creator_id: &str,
    ) -> Result<SessionId, EngineError> {
        Err(EngineError::NoGraphLoaded)
    }
}

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
    caps: nexus_capability::CapabilityRegistryHolder,
    /// Daemon-side tool dispatch for `nexus.*` host tool actions (DF-47, V1.42 P3).
    daemon_tool_dispatch: Option<std::sync::Arc<dyn nexus_capability::DaemonToolDispatch>>,
    /// Production prompt executor (A1) — wired into every inner graph
    /// `acp_prompt` node at wired-graph build time. `None` for
    /// in-memory/test engines (inner prompt nodes refuse).
    prompt_executor: Option<std::sync::Arc<dyn nexus_capability::PromptExecutor>>,
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
        caps: nexus_capability::CapabilityRegistryHolder,
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
        caps: nexus_capability::CapabilityRegistryHolder,
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
        caps: nexus_capability::CapabilityRegistryHolder,
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
        dispatch: Arc<dyn nexus_capability::DaemonToolDispatch>,
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
        executor: Arc<dyn nexus_capability::PromptExecutor>,
        session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        >,
    ) {
        self.prompt_executor = Some(executor.clone());
        self.session_cancels = session_cancels.clone();
        if let Some(state) = std::sync::Arc::get_mut(&mut self.state) {
            state.session_cancels = session_cancels;
            state.prompt_executor = Some(executor);
        }
    }

    /// Wire the live workspace state provider (v1.188 P3).
    ///
    /// Must be called during daemon boot so wired preset graphs resolve
    /// `_context.workspace.*` from the shared workspace authority. Mirrors
    /// `set_prompt_executor`'s unified-shared-state contract: the daemon
    /// calls this on a freshly constructed engine, so the provider lands on
    /// the ONE `EngineSharedState` that every `EngineProxy` clone shares.
    pub fn set_workspace_state_provider(
        &mut self,
        provider: std::sync::Arc<dyn nexus_capability::WorkspaceStateProvider>,
    ) {
        match std::sync::Arc::get_mut(&mut self.state) {
            Some(state) => state.set_workspace_state_provider(provider),
            None => {
                tracing::warn!(
                    "workspace state provider not wired: engine shared state is aliased"
                );
            }
        }
    }

    /// Wire the applied-identity source recovery reads (P4).
    ///
    /// Called once at daemon boot with the same Creator DB the schedule intake
    /// resolves applies through, so a restart re-enters the applied identity
    /// admission used instead of the descriptor bundle.
    pub fn set_applied_document_source(
        &mut self,
        source: std::sync::Arc<dyn crate::applied_document::AppliedDocumentSource>,
    ) {
        match std::sync::Arc::get_mut(&mut self.state) {
            Some(state) => state.set_applied_document_source(source),
            None => {
                tracing::warn!("applied document source not wired: engine shared state is aliased");
            }
        }
    }

    /// Wire the creator-DB core-context store for the outer-state boundary
    /// refresh (v1.195 P0-T6).
    ///
    /// Called during daemon boot with the same Creator DB pool the schedule
    /// supervisor reads `creator_schedules.current_core_context_version`
    /// through, so a run whose schedule was edited mid-execution renders the
    /// committed version at its next state transition. Mirrors
    /// `set_workspace_state_provider`: a freshly constructed engine lands the
    /// store on the ONE shared state every `EngineProxy` clone shares.
    pub fn set_core_context_store(
        &mut self,
        store: std::sync::Arc<crate::schedule::derivation::CoreContextManager>,
    ) {
        match std::sync::Arc::get_mut(&mut self.state) {
            Some(state) => state.set_core_context_store(store),
            None => {
                tracing::warn!(
                    "core-context store not wired: engine shared state is aliased, so \
                     outer-state boundaries will not refresh from the schedule pointer"
                );
            }
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
            if summary.status.is_terminal() {
                continue;
            }

            self.register_cancellation(&summary.session_id);

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
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        hydrate_queue.push_back(root_session_id.clone());
        visited.insert(root_session_id.0.clone());
        while let Some(parent) = hydrate_queue.pop_front() {
            self.state.hydrate_children(&parent).await?;
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

    /// Reload a run's bundle from the FROZEN source identity and enforce the
    /// frozen-source guards (A7 / Important 5).
    ///
    /// A directory source is reloaded from its persisted root, an embedded
    /// source from the compiled-in preset id — never the current
    /// higher-precedence bytes. The reloaded identity must equal the frozen one
    /// and the frozen version must still be the bundle's, so a changed or moved
    /// bundle refuses reconstruction instead of running different bytes.
    ///
    /// This proves the source of a run that executes its bundle: the no-apply
    /// path. An applied run executes the applied document and never calls this
    /// — its bundle is an admission input, and its frozen proof is the applied
    /// identity on the descriptor.
    fn reload_frozen_bundle(
        &self,
        descriptor: &RunDescriptorV1,
        session_id: &SessionId,
    ) -> Result<nexus_preset::LoadedPreset, EngineError> {
        let caps = self.current_caps();
        let loaded = match &descriptor.source {
            PresetSourceIdentity::Embedded { preset_id, .. } => {
                match nexus_preset::load_embedded_preset(preset_id, &caps) {
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
                match nexus_preset::load_preset(root, &caps) {
                    Ok(loaded) => loaded,
                    Err(e) => {
                        return Err(EngineError::GraphFlow(
                            graph_flow::GraphError::StorageError(format!(
                                "R6: directory preset at '{}' is missing/invalid for session {}: {} \
                                 (reconstruction_unavailable, non-replayable)",
                                root.display(),
                                session_id.0,
                                e
                            )),
                        ));
                    }
                }
            }
        };

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

        if descriptor.preset_version != loaded.version {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "R6: preset version mismatch for session {} (frozen {} != current {}) \
                     — reconstruction_unavailable (non-replayable)",
                    session_id.0, descriptor.preset_version, loaded.version
                )),
            ));
        }
        Ok(loaded)
    }

    /// The applied document a run of `preset_id` executes.
    ///
    /// The configured [`AppliedDocumentSource`](crate::applied_document::AppliedDocumentSource)
    /// owns the route when one is installed (the daemon's core wiring). Without
    /// one, an embedded preset still resolves its own compiled-in document —
    /// [`embedded_applied_document`] — because those bytes are in this binary.
    ///
    /// `Ok(None)` therefore means "no applied identity", which only a
    /// non-embedded preset (`_system.`, or a system directory bundle) can
    /// answer: it keeps executing its manifest. An embedded preset resolves a
    /// document or refuses through the `Err`; it never falls back to YAML — so
    /// a configured source answering `None` for one is stale wiring, and every
    /// caller's `None` arm is provably a non-embedded preset.
    async fn resolve_applied(
        &self,
        preset_id: &str,
        loaded: Option<&nexus_preset::LoadedPreset>,
    ) -> Result<Option<crate::applied_document::AppliedDocument>, String> {
        let resolved = match &self.state.applied_document_source {
            Some(source) => source.applied_document(preset_id, loaded).await?,
            None => loaded
                .filter(|loaded| {
                    matches!(
                        loaded.source_identity,
                        Some(PresetSourceIdentity::Embedded { .. })
                    )
                })
                .map_or(Ok(None), |loaded| {
                    embedded_applied_document(loaded).map(Some)
                })?,
        };
        if resolved.is_none()
            && loaded.is_some_and(|loaded| {
                matches!(
                    loaded.source_identity,
                    Some(PresetSourceIdentity::Embedded { .. })
                )
            })
        {
            return Err(format!(
                "embedded_document_unavailable: embedded preset '{preset_id}' resolved no applied \
                 document; refusing to execute its YAML manifest"
            ));
        }
        Ok(resolved)
    }

    #[allow(clippy::too_many_lines, clippy::significant_drop_tightening)] // sequential descriptor→validate→rebuild path; guard spans await loop
    async fn reconstruct_runner(&self, session_id: &SessionId) -> Result<(), EngineError> {
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

        // The reload below is the no-apply path's frozen-source proof (A7), and
        // for an EMBEDDED applied run it is also the applied route's key: the
        // embedded arm of the resolution below is keyed off the reloaded source
        // identity, so the compiled-in bundle is reloaded and its identity
        // re-compared — a changed embedded byte stays non-replayable here rather
        // than being rebuilt from the YAML descriptor. An applied USER run
        // executes its document instead and never reads the bundle: one edited,
        // moved or deleted between admission and this restart must not make such
        // a run non-replayable, and the frozen applied identity already encodes
        // the user-preset classification a reload would only re-establish.
        let bundle = match (&descriptor.applied, &descriptor.source) {
            (Some(_), nexus_preset::source_identity::PresetSourceIdentity::Directory { .. }) => {
                None
            }
            _ => Some(self.reload_frozen_bundle(&descriptor, session_id)?),
        };

        let applied = {
            if descriptor.intake_mode
                == Some(nexus_home_layout::strategy_intake::StrategyIntakeMode::IrOnly)
                && descriptor.applied.is_none()
            {
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "R6: session {} records [strategy_intake] mode 'ir-only' but no applied \
                         identity (yaml_intake_disabled, non-replayable) — the strategy had no \
                         current apply at admission, which ir-only refuses",
                        session_id.0
                    )),
                ));
            }
            self.resolve_applied(&descriptor.preset_id, bundle.as_ref())
                .await
                .map_err(|error| {
                    EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                        "R6: applied identity of session {} does not replay: {error} \
                         (reconstruction_unavailable, non-replayable)",
                        session_id.0
                    )))
                })?
        };
        let applied = match (descriptor.applied.as_ref(), applied) {
            (None, current) => current.map(|applied| applied.document),
            (Some(recorded), Some(current)) if current.identity == *recorded => {
                Some(current.document)
            }
            (Some(recorded), current) => {
                let current = current.map_or_else(
                    || "the strategy no longer has a current apply".to_string(),
                    |current| {
                        format!(
                            "its current apply is `{}` (content hash `{}`)",
                            current.identity.apply_id, current.identity.content_hash
                        )
                    },
                );
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "R6: session {} was admitted under apply `{}` (content hash `{}`), but \
                         {current} — the applied identity changed since admission, so this run's \
                         graph cannot be rebuilt from the current document \
                         (reconstruction_unavailable, non-replayable)",
                        session_id.0, recorded.apply_id, recorded.content_hash
                    )),
                ));
            }
        };

        let applied_manifest = match &applied {
            Some(document) => Some(nexus_preset::export_preset(document).map_err(|error| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "R6: applied document of session {} does not project to a preset manifest: \
                     {error} (reconstruction_unavailable, non-replayable)",
                    session_id.0
                )))
            })?),
            None => None,
        };

        let caps = self.current_caps();
        let loaded = bundle;

        {
            let run_manifest = match (&applied_manifest, &loaded) {
                (Some(manifest), _) => manifest,
                (None, Some(loaded)) => &loaded.manifest,
                (None, None) => {
                    return Err(EngineError::GraphFlow(
                        graph_flow::GraphError::StorageError(format!(
                            "R6: session {} has neither an applied document nor a reloadable \
                             bundle; its descendants cannot be validated (non-replayable)",
                            session_id.0
                        )),
                    ));
                }
            };
            let valid_inner: std::collections::HashSet<&str> = run_manifest
                .inner_graphs
                .iter()
                .flat_map(|graphs| graphs.keys())
                .map(String::as_str)
                .collect();
            let snapshot: Vec<(
                SessionId,
                Vec<nexus_session_storage::run_state::ChildCheckpoint>,
            )> = {
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
                    .map_or_default(|session| session.current_task_id.clone());
                let parent_is_root = parent.0 == session_id.0;
                let expected_inner = if parent_is_root {
                    inner_graph_entered_by_state(run_manifest, &parent_task)
                } else {
                    None
                };
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
                                inner_graph_entered_by_state(run_manifest, state)
                                    .map(str::to_owned),
                            );
                        }
                    }
                    map
                } else {
                    std::collections::HashMap::new()
                };
                for child in children {
                    let recorded_graph: Option<&str> = recorded
                        .get(&child.session.id)
                        .filter(|set| set.len() == 1)
                        .and_then(|set| set.iter().next())
                        .and_then(|entry| entry.as_deref());
                    validate_descendant_position(
                        &descriptor.preset_id,
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

        let proxy = Arc::new(EngineProxy {
            state: self.state.clone(),
        });
        let engine_proxy: Arc<dyn OrchestrationEngine> = proxy;
        let wired = match (&applied, &loaded) {
            (Some(document), _) => crate::preset_runtime::build_wired_outer_graph_from_ir(
                document,
                crate::preset_runtime::ir_template_source(loaded.as_ref()),
                &engine_proxy,
                &caps,
                self.daemon_tool_dispatch.clone(),
                self.prompt_executor.clone(),
                self.session_cancels.clone(),
            )
            .map_err(|error| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "R6: applied document of session {} is not buildable: {error} \
                     (reconstruction_unavailable, non-replayable)",
                    session_id.0
                )))
            })?,
            (None, Some(loaded)) => crate::preset_runtime::build_wired_outer_graph(
                loaded,
                &engine_proxy,
                &caps,
                self.daemon_tool_dispatch.clone(),
                self.prompt_executor.clone(),
                self.session_cancels.clone(),
            )?,
            (None, None) => {
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "R6: session {} has neither an applied document nor a reloadable bundle; \
                         its graph cannot be rebuilt (non-replayable)",
                        session_id.0
                    )),
                ));
            }
        };

        let runner = Arc::new(FlowRunner::new(Arc::new(wired), self.state.storage.clone()));

        self.state
            .runners
            .write()
            .await
            .insert(session_id.0.clone(), runner);

        tracing::info!(
            "R6: reconstructed runner for session {} (preset: {}, bundle version: {}, applied: {})",
            session_id.0,
            descriptor.preset_id,
            descriptor.preset_version,
            descriptor.applied.is_some()
        );

        Ok(())
    }

    /// Legacy reconstruction against the current embedded preset (no workflow
    /// store). Used only by in-memory/test engines that never persisted a v1
    /// descriptor, so there is no frozen identity to compare against: the run
    /// is rebuilt from the applied document the same route answers for a
    /// persisted run — an embedded preset's own compiled-in document — and
    /// never from its YAML manifest.
    async fn reconstruct_runner_legacy(&self, summary: &SessionSummary) -> Result<(), EngineError> {
        let caps = self.current_caps();
        let loaded =
            nexus_preset::load_embedded_preset(&summary.preset_id, &caps).map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "R6: failed to load embedded preset '{}' for session {}: {}",
                    summary.preset_id, summary.session_id.0, e
                )))
            })?;
        let applied = self
            .resolve_applied(&summary.preset_id, Some(&loaded))
            .await
            .map_err(|error| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "R6: applied identity of session {} does not replay: {error} \
                     (reconstruction_unavailable, non-replayable)",
                    summary.session_id.0
                )))
            })?
            .ok_or_else(|| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "R6: session {} resolved no applied document for preset '{}' \
                     (reconstruction_unavailable, non-replayable)",
                    summary.session_id.0, summary.preset_id
                )))
            })?;

        let proxy = Arc::new(EngineProxy {
            state: self.state.clone(),
        });
        let engine_proxy: Arc<dyn OrchestrationEngine> = proxy;
        let wired = crate::preset_runtime::build_wired_outer_graph_from_ir(
            &applied.document,
            crate::preset_runtime::ir_template_source(Some(&loaded)),
            &engine_proxy,
            &caps,
            self.daemon_tool_dispatch.clone(),
            self.prompt_executor.clone(),
            self.session_cancels.clone(),
        )
        .map_err(|error| {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                "R6: applied document of session {} is not buildable: {error} \
                 (reconstruction_unavailable, non-replayable)",
                summary.session_id.0
            )))
        })?;

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
    /// `intake_mode` is the mode arm the admission layer selected; the
    /// descriptor records it so recovery follows the recorded arm.
    /// `applied` is the identity the caller resolved this run under (I-000443):
    /// frozen here, at construction, so no seam can persist a descriptor whose
    /// bundle claim and applied identity were assembled at different times.
    fn build_descriptor(
        &self,
        loaded: &nexus_preset::LoadedPreset,
        creator_id: &str,
        applied: Option<nexus_session_storage::run_state::AppliedRunIdentity>,
        intake_mode: Option<nexus_home_layout::strategy_intake::StrategyIntakeMode>,
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
            applied,
            intake_mode,
        })
    }

    /// Start a v1 run for a loaded preset when a workflow store is present
    /// (Critical 2). Falls back to the raw-graph path when no store exists.
    async fn start_preset_run(
        &self,
        loaded: &nexus_preset::LoadedPreset,
        creator_id: &str,
        applied: Option<nexus_session_storage::run_state::AppliedRunIdentity>,
        intake_mode: Option<nexus_home_layout::strategy_intake::StrategyIntakeMode>,
        graph: Arc<Graph>,
    ) -> Result<SessionId, EngineError> {
        if let Some(store) = &self.state.workflow_store {
            let descriptor = self.build_descriptor(loaded, creator_id, applied, intake_mode)?;
            let session_id = format!("{}:{}", loaded.id, uuid::Uuid::new_v4());
            let start_task_id = graph.start_task_id().unwrap_or_default().to_string();
            let session = graph_flow::Session::new_from_task(session_id.clone(), &start_task_id);
            session.context.set("_session_id", session_id.clone())?;
            if !creator_id.is_empty() {
                session.context.set("_creator_id", creator_id.to_string())?;
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

    /// Admit a trait-entry start under the configured intake mode: the wired
    /// APPLIED graph when the strategy has a current apply, and — for a user
    /// bundle with none — a typed `yaml_intake_disabled` refusal.
    ///
    /// Both preset trait entries come through here, so neither can admit a graph
    /// and freeze a descriptor that disagree — recovery rebuilds from the
    /// recorded identity, and a run admitted on the YAML graph because this seam
    /// skipped the resolution would recover into a document it never ran.
    async fn start_preset_entry(
        &self,
        loaded: &nexus_preset::LoadedPreset,
        creator_id: &str,
    ) -> Result<SessionId, EngineError> {
        let proxy: Arc<dyn OrchestrationEngine> = Arc::new(EngineProxy {
            state: self.state.clone(),
        });
        let caps = self.current_caps();
        let user_bundle = nexus_home_layout::strategy_intake::is_user_strategy_bundle(
            &loaded.id,
            matches!(
                loaded.source_identity,
                Some(nexus_preset::source_identity::PresetSourceIdentity::Directory { .. })
            ),
        );
        let intake_mode = if user_bundle {
            Some(
                self.nexus_home
                    .as_deref()
                    .map(nexus_home_layout::strategy_intake::strategy_intake_mode)
                    .transpose()
                    .map_err(|reason| {
                        EngineError::GraphFlow(graph_flow::GraphError::StorageError(reason))
                    })?
                    .unwrap_or(nexus_home_layout::strategy_intake::StrategyIntakeMode::IrOnly),
            )
        } else {
            None
        };
        let applied = self
            .resolve_applied(&loaded.id, Some(loaded))
            .await
            .map_err(|error| EngineError::GraphFlow(graph_flow::GraphError::StorageError(error)))?;
        if user_bundle && applied.is_none() {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(format!(
                    "yaml_intake_disabled: user preset '{}' has no current apply under \
                     [strategy_intake] mode 'ir-only'; apply a strategy before running it",
                    loaded.id
                )),
            ));
        }
        let wired = match &applied {
            Some(applied) => crate::preset_runtime::build_wired_outer_graph_from_ir(
                &applied.document,
                crate::preset_runtime::ir_template_source(Some(loaded)),
                &proxy,
                &caps,
                self.daemon_tool_dispatch.clone(),
                self.prompt_executor.clone(),
                self.session_cancels.clone(),
            )
            .map_err(|error| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(error.to_string()))
            })?,
            None => crate::preset_runtime::build_wired_outer_graph(
                loaded,
                &proxy,
                &caps,
                self.daemon_tool_dispatch.clone(),
                self.prompt_executor.clone(),
                self.session_cancels.clone(),
            )?,
        };
        self.start_preset_run(
            loaded,
            creator_id,
            applied.map(|applied| applied.identity),
            intake_mode,
            Arc::new(wired),
        )
        .await
    }

    /// Start a v1 run for a loaded preset with frozen admission input (A3).
    ///
    /// Mirrors [`start_preset_run`] but freezes the schedule admission's
    /// input map, agent bindings, work id and resolved applied identity into
    /// the descriptor AND seeds the session context (`preset.input.*`, the
    /// core-context payload under the namespace its KIND owns —
    /// `core_context.text` for `Text`, `core_context.struct` for `Struct`)
    /// before the initial checkpoint is persisted — so the first step
    /// renders real input and the descriptor/core seed are durable before
    /// eligibility is published (A3: persist descriptor/core seed before
    /// enqueue).
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
        loaded: &nexus_preset::LoadedPreset,
        creator_id: &str,
        work_id: Option<String>,
        input: serde_json::Map<String, serde_json::Value>,
        core_context: Option<&CoreContextPayload>,
        agent_bindings: std::collections::HashMap<
            String,
            nexus_session_storage::run_state::AgentBinding,
        >,
        applied: Option<nexus_session_storage::run_state::AppliedRunIdentity>,
        intake_mode: Option<nexus_home_layout::strategy_intake::StrategyIntakeMode>,
        graph: Arc<Graph>,
    ) -> Result<SessionId, EngineError> {
        if let Some(store) = &self.state.workflow_store {
            let mut descriptor = self.build_descriptor(loaded, creator_id, applied, intake_mode)?;
            descriptor.work_id = work_id.filter(|id| !id.is_empty());
            descriptor.input = input.clone();
            descriptor.agent_bindings = agent_bindings;
            let start_task_id = graph.start_task_id().unwrap_or_default().to_string();
            let session =
                graph_flow::Session::new_from_task(session_id.to_string(), &start_task_id);
            session.context.set("_session_id", session_id.to_string())?;
            if !creator_id.is_empty() {
                session.context.set("_creator_id", creator_id.to_string())?;
            }
            for (key, value) in &input {
                session
                    .context
                    .set(format!("preset.input.{key}"), value.clone())?;
            }
            if let Some(payload) = core_context {
                bind_core_context_payload(&session.context, payload)?;
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
        loaded: &nexus_preset::LoadedPreset,
        creator_id: &str,
        work_id: Option<String>,
        input: serde_json::Map<String, serde_json::Value>,
        core_context: Option<&CoreContextPayload>,
        core_context_version: u32,
        expected_core_context_version: u32,
        agent_bindings: std::collections::HashMap<
            String,
            nexus_session_storage::run_state::AgentBinding,
        >,
        frozen_source: Option<PresetSourceIdentity>,
        applied: Option<nexus_session_storage::run_state::AppliedRunIdentity>,
        intake_mode: Option<nexus_home_layout::strategy_intake::StrategyIntakeMode>,
        admission_gate: Option<nexus_session_storage::run_state::ScheduleAdmissionGate>,
        graph: Arc<Graph>,
    ) -> Result<SessionId, EngineError> {
        let Some(store) = &self.state.workflow_store else {
            return Err(EngineError::GraphFlow(
                graph_flow::GraphError::StorageError(
                    "admit_schedule_run_with_input requires a durable workflow store".to_string(),
                ),
            ));
        };
        let mut descriptor = self.build_descriptor(loaded, creator_id, applied, intake_mode)?;
        descriptor.work_id = work_id;
        descriptor.input = input.clone();
        descriptor.agent_bindings = agent_bindings;
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
        let start_task_id = graph.start_task_id().unwrap_or_default().to_string();
        let session = graph_flow::Session::new_from_task(session_id.to_string(), &start_task_id);
        session.context.set("_session_id", session_id.to_string())?;
        if !creator_id.is_empty() {
            session.context.set("_creator_id", creator_id.to_string())?;
        }
        for (key, value) in &input {
            session
                .context
                .set(format!("preset.input.{key}"), value.clone())?;
        }
        if let Some(payload) = core_context {
            bind_core_context_payload(&session.context, payload)?;
        }
        session
            .context
            .set(CORE_CONTEXT_VERSION_KEY, core_context_version.to_string())?;
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

        let start_task_id = graph.start_task_id().unwrap_or_default().to_string();

        let session = graph_flow::Session::new_from_task(session_id.clone(), &start_task_id);
        session.context.set("_session_id", session_id.clone())?;
        if let Some(creator_id) = creator_id {
            session.context.set("_creator_id", creator_id.to_string())?;
        }

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
                applied: None,
                intake_mode: None,
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

        let runner = Arc::new(FlowRunner::new(graph, self.state.storage.clone()));
        self.state
            .runners
            .write()
            .await
            .insert(session_id.clone(), runner);

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
        if let Some(home) = &self.nexus_home {
            if let Ok(loaded) = nexus_preset::resolve_preset(preset_id, home, &caps) {
                if let Some(identity) = loaded.source_identity {
                    return Ok((identity, loaded.version));
                }
            }
        }
        if let Ok(loaded) = nexus_preset::load_embedded_preset(preset_id, &caps) {
            if let Some(identity) = loaded.source_identity {
                return Ok((identity, loaded.version));
            }
        }
        Err(EngineError::GraphFlow(
            graph_flow::GraphError::StorageError(format!(
                "cannot resolve source identity for preset '{preset_id}' (not embedded and no directory bundle)"
            )),
        ))
    }
}

#[async_trait]
impl OrchestrationEngine for GraphFlowEngine {
    fn workspace_state_provider(
        &self,
    ) -> Option<std::sync::Arc<dyn nexus_capability::WorkspaceStateProvider>> {
        self.state.workspace_state_provider.clone()
    }

    async fn run_step(&self, session_id: &SessionId) -> Result<StepOutcome, EngineError> {
        let result = self.state.run_step_internal(session_id).await?;

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
        };

        Ok(outcome)
    }

    async fn new_session(&self, key: SessionKey, _ctx: Context) -> Result<SessionId, EngineError> {
        let session_id = format!("{}:{}", key.preset_id, key.instance_id);

        let session = graph_flow::Session::new_from_task(session_id.clone(), "");
        session.context.set("_session_id", session_id.clone())?;
        session.context.set("_creator_id", key.creator_id.clone())?;
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
                applied: None,
                intake_mode: None,
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
        loaded: &nexus_preset::LoadedPreset,
    ) -> Result<SessionId, EngineError> {
        self.start_preset_entry(loaded, "").await
    }

    async fn start_session_with_preset_for_creator(
        &self,
        loaded: &nexus_preset::LoadedPreset,
        creator_id: &str,
    ) -> Result<SessionId, EngineError> {
        self.start_preset_entry(loaded, creator_id).await
    }
}

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
impl nexus_capability::PromptExecutor for ScriptedFinalizeExecutor {
    async fn execute(
        &self,
        _request: nexus_capability::PromptRequest,
    ) -> Result<nexus_capability::PromptResult, CapabilityError> {
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
    ) -> Result<Option<nexus_session_storage::run_state::RunRecord>, StorageError> {
        self.inner.load_run(session_id).await
    }

    async fn start_run(
        &self,
        session_id: &SessionId,
        descriptor: &nexus_session_storage::run_state::RunDescriptorV1,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        self.inner
            .start_run(session_id, descriptor, checkpoint, next_state)
            .await
    }

    async fn admit_schedule_run(
        &self,
        schedule_id: &str,
        session_id: &SessionId,
        descriptor: &nexus_session_storage::run_state::RunDescriptorV1,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_state: &nexus_session_storage::run_state::RunStateV1,
        core_context_version: u32,
        expected_core_context_version: u32,
        admission_gate: Option<&nexus_session_storage::run_state::ScheduleAdmissionGate>,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
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
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        if next_status == SessionStatus::Interrupted
            && !self
                .injected
                .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
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
                    nexus_session_storage::run_state::RunCheckpoint {
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

    async fn commit_transition_with_graph_fence(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        let _ = expected_graph_version;
        self.commit_transition(
            session_id,
            expected_revision,
            checkpoint,
            next_status,
            next_state,
        )
        .await
    }

    async fn settle_run(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_state: &nexus_session_storage::run_state::RunStateV1,
        terminal_target: TerminalSettlementTarget,
    ) -> Result<SettlementResult, StorageError> {
        self.inner
            .settle_run(
                session_id,
                expected_revision,
                expected_graph_version,
                checkpoint,
                next_state,
                terminal_target,
            )
            .await
    }

    async fn restore_pre_step(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        pre_step: &graph_flow::Session,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        self.inner
            .restore_pre_step(session_id, expected_revision, pre_step)
            .await
    }

    async fn mark_step_in_flight(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        step_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        self.inner
            .mark_step_in_flight(
                session_id,
                expected_revision,
                expected_graph_version,
                checkpoint,
                step_state,
            )
            .await
    }

    async fn persist_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        expected_attempt_id: Option<&str>,
        attempt: &nexus_session_storage::run_state::PromptAttempt,
    ) -> Result<(), StorageError> {
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
    ) -> Result<(), StorageError> {
        self.inner
            .clear_prompt_attempt(session_id, expected_revision, expected_step, attempt_id)
            .await
    }

    async fn load_children(
        &self,
        parent_session_id: &SessionId,
    ) -> Result<Vec<nexus_session_storage::run_state::RunRecord>, StorageError> {
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
    ) -> Result<Option<nexus_session_storage::run_state::RunRecord>, StorageError> {
        self.inner.load_run(session_id).await
    }

    async fn start_run(
        &self,
        session_id: &SessionId,
        descriptor: &nexus_session_storage::run_state::RunDescriptorV1,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        self.inner
            .start_run(session_id, descriptor, checkpoint, next_state)
            .await
    }

    async fn admit_schedule_run(
        &self,
        schedule_id: &str,
        session_id: &SessionId,
        descriptor: &nexus_session_storage::run_state::RunDescriptorV1,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_state: &nexus_session_storage::run_state::RunStateV1,
        core_context_version: u32,
        expected_core_context_version: u32,
        admission_gate: Option<&nexus_session_storage::run_state::ScheduleAdmissionGate>,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
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
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
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
                .unwrap();
            self.inner
                .commit_transition(
                    session_id,
                    record.state_revision,
                    nexus_session_storage::run_state::RunCheckpoint {
                        root: &winner_root,
                        children: &[],
                    },
                    SessionStatus::Running,
                    &nexus_session_storage::run_state::RunStateV1::default(),
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

    async fn commit_transition_with_graph_fence(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        let _ = expected_graph_version;
        self.commit_transition(
            session_id,
            expected_revision,
            checkpoint,
            next_status,
            next_state,
        )
        .await
    }

    async fn settle_run(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_state: &nexus_session_storage::run_state::RunStateV1,
        terminal_target: TerminalSettlementTarget,
    ) -> Result<SettlementResult, StorageError> {
        self.inner
            .settle_run(
                session_id,
                expected_revision,
                expected_graph_version,
                checkpoint,
                next_state,
                terminal_target,
            )
            .await
    }

    async fn restore_pre_step(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        pre_step: &graph_flow::Session,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        self.inner
            .restore_pre_step(session_id, expected_revision, pre_step)
            .await
    }

    async fn mark_step_in_flight(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        step_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        self.inner
            .mark_step_in_flight(
                session_id,
                expected_revision,
                expected_graph_version,
                checkpoint,
                step_state,
            )
            .await
    }

    async fn persist_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        expected_attempt_id: Option<&str>,
        attempt: &nexus_session_storage::run_state::PromptAttempt,
    ) -> Result<(), StorageError> {
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
    ) -> Result<(), StorageError> {
        self.inner
            .clear_prompt_attempt(session_id, expected_revision, expected_step, attempt_id)
            .await
    }

    async fn load_children(
        &self,
        parent_session_id: &SessionId,
    ) -> Result<Vec<nexus_session_storage::run_state::RunRecord>, StorageError> {
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
    ) -> Result<Option<nexus_session_storage::run_state::RunRecord>, StorageError> {
        self.inner.load_run(session_id).await
    }

    async fn start_run(
        &self,
        session_id: &SessionId,
        descriptor: &nexus_session_storage::run_state::RunDescriptorV1,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        self.inner
            .start_run(session_id, descriptor, checkpoint, next_state)
            .await
    }

    async fn admit_schedule_run(
        &self,
        schedule_id: &str,
        session_id: &SessionId,
        descriptor: &nexus_session_storage::run_state::RunDescriptorV1,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_state: &nexus_session_storage::run_state::RunStateV1,
        core_context_version: u32,
        expected_core_context_version: u32,
        admission_gate: Option<&nexus_session_storage::run_state::ScheduleAdmissionGate>,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
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
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
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

    async fn commit_transition_with_graph_fence(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        let _ = expected_graph_version;
        self.commit_transition(
            session_id,
            expected_revision,
            checkpoint,
            next_status,
            next_state,
        )
        .await
    }

    async fn settle_run(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        next_state: &nexus_session_storage::run_state::RunStateV1,
        terminal_target: TerminalSettlementTarget,
    ) -> Result<SettlementResult, StorageError> {
        if terminal_target == TerminalSettlementTarget::Cancelled
            && !self
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
                .unwrap();
            self.inner
                .commit_transition(
                    session_id,
                    record.state_revision,
                    nexus_session_storage::run_state::RunCheckpoint {
                        root: &winner_root,
                        children: &[],
                    },
                    SessionStatus::Running,
                    &nexus_session_storage::run_state::RunStateV1::default(),
                )
                .await
                .expect("competing transition wins the CAS before the child settle");
        }
        self.inner
            .settle_run(
                session_id,
                expected_revision,
                expected_graph_version,
                checkpoint,
                next_state,
                terminal_target,
            )
            .await
    }

    async fn restore_pre_step(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        pre_step: &graph_flow::Session,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        self.inner
            .restore_pre_step(session_id, expected_revision, pre_step)
            .await
    }

    async fn mark_step_in_flight(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: nexus_session_storage::run_state::RunCheckpoint<'_>,
        step_state: &nexus_session_storage::run_state::RunStateV1,
    ) -> Result<nexus_session_storage::run_state::RunRecord, StorageError> {
        self.inner
            .mark_step_in_flight(
                session_id,
                expected_revision,
                expected_graph_version,
                checkpoint,
                step_state,
            )
            .await
    }

    async fn persist_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        expected_attempt_id: Option<&str>,
        attempt: &nexus_session_storage::run_state::PromptAttempt,
    ) -> Result<(), StorageError> {
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
    ) -> Result<(), StorageError> {
        self.inner
            .clear_prompt_attempt(session_id, expected_revision, expected_step, attempt_id)
            .await
    }

    async fn load_children(
        &self,
        parent_session_id: &SessionId,
    ) -> Result<Vec<nexus_session_storage::run_state::RunRecord>, StorageError> {
        self.inner.load_children(parent_session_id).await
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::too_many_lines)] // deterministic race/settlement scenarios keep setup+assertions linear
    #![allow(clippy::await_holding_lock)] // test fixtures hold sync mutex guards across awaits to freeze state
    #![allow(clippy::significant_drop_tightening)] // tests read state snapshots; early-drop noise without contention value

    use super::*;
    use graph_flow::InMemorySessionStorage;
    use nexus_session_storage::storage::SqliteSessionStorage;

    /// Helper: create a test engine with in-memory storage and built-in caps.
    fn test_engine() -> GraphFlowEngine {
        let storage: Arc<dyn SessionStorage> = Arc::new(InMemorySessionStorage::new());
        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        GraphFlowEngine::new_with_storage(storage, caps)
    }

    /// v1.195 P0-T6: the applied-version marker distinguishes a LEGACY run
    /// (no marker → the pre-binding version 0, where the schedule pointer
    /// decides whether a refresh is due) from CORRUPT metadata, which is
    /// refused instead of silently read as version 0.
    ///
    /// The corrupt arm is the dangerous one: with a committed pointer at 0 the
    /// equality path would return without refreshing AND without refusing, so
    /// a malformed marker would silently keep a stale snapshot.
    #[test]
    fn applied_version_marker_is_fail_closed_when_present_but_malformed() {
        let context = graph_flow::Context::new();
        assert_eq!(
            applied_core_context_version(&context),
            Ok(0),
            "an absent marker is the legacy pre-binding run"
        );

        context.set(CORE_CONTEXT_VERSION_KEY, "7").unwrap();
        assert_eq!(applied_core_context_version(&context), Ok(7));

        context
            .set(CORE_CONTEXT_VERSION_KEY, "not-a-version")
            .unwrap();
        let err = applied_core_context_version(&context)
            .expect_err("a non-decimal marker must refuse, never read as version 0");
        assert!(err.contains("not-a-version"), "got {err}");

        context.set(CORE_CONTEXT_VERSION_KEY, 7u32).unwrap();
        let err = applied_core_context_version(&context)
            .expect_err("a wrong-typed marker must refuse, never read as version 0");
        assert!(err.contains("unexpected JSON type"), "got {err}");
    }

    /// v1.195 P0-T6: one committed payload binds exactly ONE namespace — the
    /// one its kind owns — and drops the other kind's binding, so a template
    /// can never resolve a payload the run no longer carries.
    #[test]
    fn binding_a_payload_owns_exactly_one_namespace() {
        let context = graph_flow::Context::new();

        bind_core_context_payload(
            &context,
            &CoreContextPayload::Text {
                body: "flat body".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            context.get::<String>(CORE_CONTEXT_TEXT_KEY).as_deref(),
            Some("flat body")
        );
        assert!(context
            .get::<serde_json::Value>(CORE_CONTEXT_STRUCT_KEY)
            .is_none());

        bind_core_context_payload(
            &context,
            &CoreContextPayload::Struct {
                body: serde_json::json!({ "idea": "structured body" }),
            },
        )
        .unwrap();
        assert_eq!(
            context
                .get::<serde_json::Value>(CORE_CONTEXT_STRUCT_KEY)
                .and_then(|value| value
                    .get("idea")
                    .and_then(|field| field.as_str())
                    .map(str::to_string))
                .as_deref(),
            Some("structured body"),
            "a struct payload resolves under the dotted struct namespace"
        );
        assert!(
            context
                .get::<serde_json::Value>(CORE_CONTEXT_TEXT_KEY)
                .is_none(),
            "the stale text binding must be dropped when the run moves to a struct version"
        );

        bind_core_context_payload(
            &context,
            &CoreContextPayload::Text {
                body: "back to text".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            context.get::<String>(CORE_CONTEXT_TEXT_KEY).as_deref(),
            Some("back to text")
        );
        assert!(
            context
                .get::<serde_json::Value>(CORE_CONTEXT_STRUCT_KEY)
                .is_none(),
            "the stale struct binding must be dropped when the run moves back to text"
        );
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
        let graph = Arc::new(
            graph_flow::GraphBuilder::new("creator-context")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );

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
            .expect("trusted creator id should be seeded");
        let seeded_session_id: String = ctx
            .get("_session_id")
            .expect("trusted session id should be seeded");

        assert_eq!(creator_id, "creator_alice");
        assert_eq!(seeded_session_id, session_id.0);
    }

    #[tokio::test]
    async fn r6_recovered_session_has_runner_no_graph_loaded_fix() {
        // With the R6 fix, recover_sessions() loads the embedded preset and
        // reconstructs the FlowRunner. However, this only works for sessions
        // started with known embedded presets (e.g., "novel-writing").
        //
        // For sessions with unknown presets, reconstruct_runner logs a warning
        // and the runner is not created. The session remains in the tracker
        // but run_step will fail — this is expected behavior for unknown presets.

        let engine = test_engine();

        let summary = SessionSummary {
            session_id: SessionId("test:unknown-preset-session".to_string()),
            creator_id: "test-creator".to_string(),
            preset_id: "nonexistent-preset".to_string(),
            status: SessionStatus::Paused,
            current_task_id: Some("gathering".to_string()),
        };

        engine.recover_sessions(vec![summary.clone()]).await;

        let active = engine.list_active(SessionFilter::default()).await.unwrap();
        assert!(
            active.iter().any(|s| s.session_id == summary.session_id),
            "recovered session should be in active list"
        );

        let result = engine.run_step(&summary.session_id).await;
        assert!(
            result.is_err(),
            "run_step should fail for unknown preset recovery (NoGraphLoaded)"
        );
    }

    #[tokio::test]
    async fn r6_recovered_session_with_known_preset_has_runner() {
        let storage: Arc<dyn SessionStorage> = Arc::new(InMemorySessionStorage::new());
        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let engine = GraphFlowEngine::new_with_storage(storage.clone(), caps);

        let summary = SessionSummary {
            session_id: SessionId("novel-writing:1234567890".to_string()),
            creator_id: "test-creator".to_string(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Paused,
            current_task_id: Some("gathering".to_string()),
        };

        let session = graph_flow::Session::new_from_task(
            summary.session_id.0.clone(),
            summary.current_task_id.as_deref().unwrap_or(""),
        );
        storage.save(session).await.unwrap();

        engine.recover_sessions(vec![summary.clone()]).await;

        let active = engine.list_active(SessionFilter::default()).await.unwrap();
        assert!(
            active.iter().any(|s| s.session_id == summary.session_id),
            "recovered session should be in active list"
        );

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
        let storage: Arc<dyn SessionStorage> = Arc::new(InMemorySessionStorage::new());
        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let engine = GraphFlowEngine::new_with_storage(storage.clone(), caps);

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

        engine
            .recover_sessions(vec![terminal_summary.clone()])
            .await;

        let active = engine.list_active(SessionFilter::default()).await.unwrap();
        assert!(
            active.is_empty(),
            "terminal sessions should not appear in active list"
        );

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

    #[tokio::test]
    async fn cancel_fence_reloads_after_mark_step_in_flight_wins() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(())]));
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("cancel-race")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

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
                None,
                RunCheckpoint {
                    root: &pre_step_root,
                    children: &[],
                },
                &in_flight_state,
            )
            .await
            .expect("mark_step_in_flight wins the CAS");

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

    /// Deterministic cancel-intent-v-step race (v1.195 P0-T6 Critical): the
    /// cancel winner's durable linearization point — the phase-1 intent fence
    /// committed while the status stays non-terminal — is EXACTLY the
    /// revision a later step's pre-step load anchors to, so the marker's
    /// revision CAS cannot catch it. The marker rewrites `run_state_json`
    /// wholesale from the step's fresh state, so without a durable fence it
    /// erases the committed intent and the external effect dispatches after
    /// the cancel linearization point.
    ///
    /// The step must lose to the cancel winner, the effect must never start,
    /// and the committed intent must survive untouched.
    #[tokio::test]
    async fn step_marker_cannot_erase_committed_cancel_intent() {
        use graph_flow::{NextAction, Task, TaskResult};
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct CountingTask {
            dispatches: Arc<AtomicUsize>,
        }
        #[async_trait]
        impl Task for CountingTask {
            fn id(&self) -> &'static str {
                "count"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<TaskResult, graph_flow::GraphError> {
                self.dispatches.fetch_add(1, Ordering::SeqCst);
                Ok(TaskResult::new(Some("done".into()), NextAction::Continue))
            }
        }

        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );

        let dispatches = Arc::new(AtomicUsize::new(0));
        let graph = Arc::new(
            graph_flow::GraphBuilder::new("cancel-intent-step")
                .add_task(Arc::new(CountingTask {
                    dispatches: dispatches.clone(),
                }))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert!(
            !record.status.is_terminal(),
            "the intent fence must keep the status non-terminal"
        );
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let mut fenced_state = record.state.clone().unwrap_or_default();
        fenced_state.cancel_requested = true;
        store
            .commit_transition_with_graph_fence(
                &session_id,
                record.state_revision,
                Some(root.version),
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                record.status.clone(),
                &fenced_state,
            )
            .await
            .expect("the cancel-intent fence commits");
        let fenced = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert!(
            fenced.state.as_ref().is_some_and(|s| s.cancel_requested),
            "precondition: the durable cancel intent is committed"
        );

        let step_result = engine.run_step(&session_id).await;
        if let Err(err) = &step_result {
            let conflict = match err {
                EngineError::StepFailed { source, .. } => source.as_ref(),
                other => other,
            };
            assert!(
                matches!(
                    conflict,
                    EngineError::GraphFlow(graph_flow::GraphError::SessionConflict(_))
                ),
                "the cancel winner owns the row, so the step must lose to it, got {err:?}"
            );
        }
        let after = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let observed_dispatches = dispatches.load(Ordering::SeqCst);
        assert!(
            after.state.as_ref().is_some_and(|s| s.cancel_requested),
            "the step marker must not erase the committed cancel intent \
             (observed external-effect dispatches = {observed_dispatches})"
        );
        assert_eq!(
            observed_dispatches, 0,
            "no external effect may start after the cancel linearization point"
        );
        assert_eq!(
            after.state_revision, fenced.state_revision,
            "the refused step must not advance the durable revision"
        );
        assert_eq!(
            after.graph_version, fenced.graph_version,
            "the refused step must not advance the durable graph clock"
        );
    }

    /// Deterministic terminal-v-cancel race (Finding 1): the run reaches a
    /// terminal state while the cancel fence is in flight. The cancel must
    /// surface a deliberate conflict, never silent success.
    #[tokio::test]
    async fn cancel_fence_terminal_observation_is_conflict() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("cancel-terminal")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

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
                None,
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

    #[tokio::test]
    async fn interrupted_cleanup_retry_preserves_terminal_cause() {
        for failed in [false, true] {
            let expected_status = if failed {
                SessionStatus::Failed
            } else {
                SessionStatus::Cancelled
            };
            let db = tempfile::NamedTempFile::new().unwrap();
            let guarded = nexus_local_db::init_engine_pool(db.path())
                .await
                .expect("open pool");
            let pool = guarded.clone_pool();
            let pool = Arc::new(pool);
            let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
            let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
            let storage: Arc<dyn SessionStorage> = sqlite.clone();

            let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![
                Err(CapabilityError::Internal("shutdown timeout".to_string())),
                Ok(()),
            ]));

            let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
                CapabilityRegistry::with_builtins(),
            ));
            let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                store.clone(),
                caps,
            );
            engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

            let graph = Arc::new(
                graph_flow::GraphBuilder::new("cancel-retry")
                    .add_task(Arc::new(crate::tasks::ManualWaitTask))
                    .build()
                    .expect("test graph"),
            );
            let session_id = engine
                .start_session("novel-writing", graph)
                .await
                .expect("start session");
            if failed {
                let record = store.load_run(&session_id).await.unwrap().unwrap();
                let root = storage.get(&session_id.0).await.unwrap().unwrap();
                let mut state = record.state.unwrap();
                state.failure = Some(RunFailure {
                    code: "driver_failed".to_string(),
                    message: "original execution error".to_string(),
                });
                store
                    .commit_transition_with_graph_fence(
                        &session_id,
                        record.state_revision,
                        Some(root.version),
                        RunCheckpoint {
                            root: &root,
                            children: &[],
                        },
                        record.status,
                        &state,
                    )
                    .await
                    .unwrap();
            }

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
            let failure = interrupted
                .state
                .as_ref()
                .unwrap()
                .failure
                .as_ref()
                .unwrap();
            assert_eq!(
                failure.code,
                if failed {
                    "driver_failed"
                } else {
                    "cancel_cleanup_unconfirmed"
                }
            );
            if failed {
                assert_eq!(failure.message, "original execution error");
            }
            assert!(
                interrupted
                    .state
                    .as_ref()
                    .is_some_and(|s| s.cancel_requested),
                "interrupted run must carry the durable cancel intent"
            );
            assert_eq!(executor.calls(), 1, "first finalize attempted");

            let err = engine
                .state
                .persist_signal_transition(&session_id, &EngineSignal::Resume)
                .await
                .expect_err("resume on interrupted must refuse");
            assert!(matches!(err, EngineError::TerminalState(_)));

            let status = engine
                .state
                .persist_signal_transition(&session_id, &EngineSignal::Cancel)
                .await
                .expect("retry confirms cleanup");
            assert_eq!(status, expected_status);
            assert_eq!(executor.calls(), 2, "retry re-invokes finalize_run");
            let final_record = store
                .load_run(&session_id)
                .await
                .expect("load run")
                .expect("run exists");
            assert_eq!(final_record.status, expected_status);
            assert!(
                final_record
                    .state
                    .as_ref()
                    .is_some_and(|s| s.cancel_requested),
                "settled cancelled run must carry cancel_requested"
            );
        }
    }

    /// Deterministic repeated-failure proof (Finding 3): a retry that still
    /// cannot confirm cleanup keeps the run `Interrupted` and retains the
    /// owned Host session for a later retry.
    #[tokio::test]
    async fn interrupted_cancel_retry_retains_on_repeated_failure() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![
            Err(CapabilityError::Internal("shutdown timeout".to_string())),
            Err(CapabilityError::Internal("shutdown timeout".to_string())),
        ]));

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("cancel-retry-fail")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
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

    #[tokio::test]
    async fn continue_after_cancel_fence_is_state_conflict() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("continue-fence")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

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
            wait: Some(nexus_session_storage::run_state::WaitRecord {
                wait_id: "wait-tok-1".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: nexus_session_storage::run_state::WaitKind::Manual,
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

    /// the run is waiting at revision R; Continue consumes the wait and
    /// commits a NEWER root position/context at R+1; Cancel's phase-1 fence
    /// loses the CAS, reloads the R+1 row/root, re-fences at R+2, and the
    /// terminal Cancelled settlement must carry the NEWEST root — the
    /// pre-Continue stale root must never clobber the winner's position.
    #[tokio::test]
    async fn cancel_settlement_keeps_continue_winner_root() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(())]));

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("continue-wins")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

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
            wait: Some(nexus_session_storage::run_state::WaitRecord {
                wait_id: "wait-tok-1".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: nexus_session_storage::run_state::WaitKind::Manual,
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
            .unwrap();
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

        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("cancel must reload and retry the fence");
        assert_eq!(status, SessionStatus::Cancelled);

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
            final_root.context.get::<String>("winner.marker").as_deref(),
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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
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

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("continue-wins-race")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

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
            wait: Some(nexus_session_storage::run_state::WaitRecord {
                wait_id: "wait-tok-1".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: nexus_session_storage::run_state::WaitKind::Manual,
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
            final_root.context.get::<String>("winner.marker").as_deref(),
            Some("continue-won"),
            "the terminal checkpoint must carry the Continue winner's context"
        );
    }

    #[tokio::test]
    async fn cancel_finalizes_recursive_descendant_closure() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(()), Ok(()), Ok(())]));

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("nested-cancel")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let inner = Arc::new(
            graph_flow::GraphBuilder::new("inner-a")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let child_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        let inner2 = Arc::new(
            graph_flow::GraphBuilder::new("inner-b")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let grandchild_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: child_id.0.clone(),
                inner_graph: inner2,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn grandchild");

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
            wait: Some(nexus_session_storage::run_state::WaitRecord {
                wait_id: "grandchild-wait-tok".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: nexus_session_storage::run_state::WaitKind::Manual,
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

        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("cancel must settle cancelled");
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
            "root, child, and grandchild each finalized by their own run id"
        );

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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(()), Ok(()), Ok(())]));

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("late-child-cancel")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let inner = Arc::new(
            graph_flow::GraphBuilder::new("inner-a")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let child_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        let inner2 = Arc::new(
            graph_flow::GraphBuilder::new("inner-late")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
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

        let status = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect("cancel must settle cancelled");
        assert_eq!(status, SessionStatus::Cancelled);

        assert!(late_token.is_cancelled(), "late child token must fire");

        assert_eq!(
            executor.finalized_runs(),
            vec![session_id.0.clone(), child_id.0.clone(), late_id.0.clone(),],
            "root, mapped child, and persisted late child each finalized by their own run id"
        );

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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(())]));

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("child-after-fence")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

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

        let inner = Arc::new(
            graph_flow::GraphBuilder::new("inner-late")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let race_store = Arc::new(ChildSettleCasLossStore::new(store, storage.clone()));
        let store: Arc<dyn WorkflowStateStore> = race_store.clone();

        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Ok(()), Ok(())]));

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("child-settle-cas-loss")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

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

        let inner = Arc::new(
            graph_flow::GraphBuilder::new("inner-late")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![
            Ok(()),
            Ok(()),
            Err(CapabilityError::Internal("shutdown timeout".to_string())),
            Ok(()),
            Ok(()),
            Ok(()),
        ]));

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("nested-retry")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let inner = Arc::new(
            graph_flow::GraphBuilder::new("inner-a")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let child_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        let inner2 = Arc::new(
            graph_flow::GraphBuilder::new("inner-b")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let grandchild_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: child_id.0.clone(),
                inner_graph: inner2,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn grandchild");

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

    #[tokio::test]
    async fn recovered_root_cancel_reaches_persisted_grandchild() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let live = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        let graph = Arc::new(
            graph_flow::GraphBuilder::new("restart-nested")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = live
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let inner = Arc::new(
            graph_flow::GraphBuilder::new("inner-a")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let child_id = live
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        let inner2 = Arc::new(
            graph_flow::GraphBuilder::new("inner-b")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let grandchild_id = live
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: child_id.0.clone(),
                inner_graph: inner2,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn grandchild");

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
            wait: Some(nexus_session_storage::run_state::WaitRecord {
                wait_id: "restart-grandchild-wait".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: nexus_session_storage::run_state::WaitKind::Manual,
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

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
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
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let live = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        let graph = Arc::new(
            graph_flow::GraphBuilder::new("restart-retry")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = live
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let inner = Arc::new(
            graph_flow::GraphBuilder::new("inner-a")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let child_id = live
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        let inner2 = Arc::new(
            graph_flow::GraphBuilder::new("inner-b")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let grandchild_id = live
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: child_id.0.clone(),
                inner_graph: inner2,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn grandchild");

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
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

    #[tokio::test]
    async fn interrupted_write_cas_loss_retries_and_persists() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = Arc::new(InterruptedCasInjectingStore::new(
            real_store.clone(),
            storage.clone(),
        ));

        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![Err(
            CapabilityError::Internal("shutdown timeout".to_string()),
        )]));

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("interrupted-cas")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let err = engine
            .state
            .persist_signal_transition(&session_id, &EngineSignal::Cancel)
            .await
            .expect_err("unconfirmed cleanup surfaces the error");
        assert!(
            matches!(err, EngineError::GraphFlow(_)),
            "unconfirmed cleanup surfaces the error, got {err:?}"
        );

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

    #[tokio::test]
    async fn cancel_finalizes_child_host_sessions() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        // root finalize succeeds, then the child's OWN finalize succeeds.
        let executor = Arc::new(ScriptedFinalizeExecutor::new(vec![
            Err(CapabilityError::Internal("shutdown timeout".to_string())),
            Ok(()),
            Ok(()),
        ]));

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        engine.set_prompt_executor(executor.clone(), engine.state.session_cancels.clone());

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("child-cancel")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let inner = Arc::new(
            graph_flow::GraphBuilder::new("inner")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let child_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

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
            wait: Some(nexus_session_storage::run_state::WaitRecord {
                wait_id: "child-wait-tok".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: nexus_session_storage::run_state::WaitKind::Manual,
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

    /// T1 review gap 4: the cached child checkpoint must carry the
    /// AUTHORITATIVE persisted child `Session.version` (graph OCC clock) —
    /// at admission (seeded from the persisted row, not the version-0
    /// pre-save snapshot), after a parent commit that carries the child, and
    /// after a nested continuation commit. Both clocks (`state_revision` and
    /// `graph_version`) are asserted.
    #[tokio::test]
    async fn cached_child_checkpoint_tracks_persisted_graph_version() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        let graph = Arc::new(
            graph_flow::GraphBuilder::new("parent-graph")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("test graph"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let inner = Arc::new(
            graph_flow::GraphBuilder::new("inner-g")
                .add_task(Arc::new(crate::tasks::ManualWaitTask))
                .build()
                .expect("inner graph"),
        );
        let child_id = engine
            .state
            .spawn_child_session_internal(ChildSessionParams {
                parent_session_id: session_id.0.clone(),
                inner_graph: inner,
                initial_context: graph_flow::Context::new(),
            })
            .await
            .expect("spawn child");

        let persisted_child_gv = async |pool: &sqlx::SqlitePool, id: &str| -> i64 {
            sqlx::query_scalar(
                "SELECT graph_version FROM orchestration_sessions WHERE session_id = ?",
            )
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("child graph_version")
        };
        let cached_child = async |engine: &GraphFlowEngine, parent: &str, child: &str| {
            let map = engine.state.children.read().await;
            map.get(parent)
                .and_then(|entries| entries.iter().find(|c| c.session.id == child))
                .map(|c| (c.session.version, c.state_revision))
        };

        let (cached_v, cached_rev) = cached_child(&engine, &session_id.0, &child_id.0)
            .await
            .expect("child cached");
        assert_eq!(persisted_child_gv(&pool, &child_id.0).await, 1);
        assert_eq!(
            cached_v, 1,
            "admission cache must carry the persisted version"
        );
        assert_eq!(cached_rev, 1);

        let outcome = engine.run_step(&session_id).await.expect("parent step");
        assert!(
            matches!(
                outcome,
                StepOutcome::WaitingForInput { .. } | StepOutcome::Paused { .. }
            ),
            "manual wait outcome, got {outcome:?}"
        );
        let (cached_v, cached_rev) = cached_child(&engine, &session_id.0, &child_id.0)
            .await
            .expect("child cached after parent commit");
        let persisted_v = persisted_child_gv(&pool, &child_id.0).await;
        assert_eq!(
            persisted_v, 2,
            "carried child clock advanced by the parent commit"
        );
        assert_eq!(
            cached_v,
            u64::try_from(persisted_v).unwrap(),
            "cached child version must equal the persisted clock"
        );
        assert_eq!(
            cached_rev, 2,
            "cached child revision must equal the persisted revision"
        );

        let parent_record = store
            .load_run(&session_id)
            .await
            .expect("load parent")
            .expect("parent record");
        let parent_rev = parent_record.state_revision;
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root row");
        let wait_state = RunStateV1 {
            wait: Some(nexus_session_storage::run_state::WaitRecord {
                wait_id: "wait-nested-1".to_string(),
                task_id: root.current_task_id.clone(),
                child_session_id: Some(child_id.0.clone()),
                child_task_id: None,
                kind: nexus_session_storage::run_state::WaitKind::Manual,
            }),
            ..RunStateV1::default()
        };
        store
            .commit_transition(
                &session_id,
                parent_rev,
                RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                SessionStatus::WaitingForInput,
                &wait_state,
            )
            .await
            .expect("commit nested wait");

        let status = engine
            .state
            .persist_signal_transition(
                &session_id,
                &EngineSignal::Continue {
                    wait_id: "wait-nested-1".to_string(),
                },
            )
            .await
            .expect("continuation commits");
        assert_eq!(status, SessionStatus::Running);

        let (cached_v, cached_rev) = cached_child(&engine, &session_id.0, &child_id.0)
            .await
            .expect("child cached after continuation");
        let persisted_v = persisted_child_gv(&pool, &child_id.0).await;
        assert_eq!(
            persisted_v, 3,
            "continuation commit advanced the child clock"
        );
        assert_eq!(
            cached_v,
            u64::try_from(persisted_v).unwrap(),
            "cached child version must track the persisted clock"
        );
        assert_eq!(
            cached_rev, 3,
            "cached child revision must track the persisted revision"
        );
    }

    /// T1 review gap 5: a real SQLite winner/loser race through the engine
    /// drive path. A competing authoritative writer commits DURING the losing
    /// step's task execution; the losing runner's graph save then conflicts.
    /// The loser surfaces `EngineError::GraphFlow(SessionConflict)`, the task
    /// is never re-invoked, and the winner's row/revision/context are
    /// untouched (no cancel signal, no failure-context mutation — the daemon
    /// arm behavior is covered by
    /// `nexus_execution::workflow::tests::session_conflict_stops_losing_drive_without_winner_mutation`).
    #[tokio::test]
    async fn losing_drive_conflict_leaves_sqlite_winner_untouched() {
        use graph_flow::{NextAction, Task, TaskResult};

        struct CompetingWinnerTask {
            store: Arc<SqliteSessionStorage>,
            calls: Arc<std::sync::atomic::AtomicU64>,
        }

        #[async_trait]
        impl Task for CompetingWinnerTask {
            fn id(&self) -> &'static str {
                "competing_winner_task"
            }

            async fn run(
                &self,
                context: graph_flow::Context,
            ) -> Result<TaskResult, graph_flow::GraphError> {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let session_id: String = context
                    .get("_session_id")
                    .expect("engine seeds _session_id");
                let winner = graph_flow::Session::new_from_task(session_id.clone(), "winner-task");
                winner
                    .context
                    .set("_session_id", session_id.clone())
                    .expect("session id is a JSON-infallible string scalar");
                winner
                    .context
                    .set("winner.marker", "committed")
                    .expect("marker is a JSON-infallible string scalar");
                self.store
                    .commit_transition(
                        &SessionId(session_id),
                        2,
                        RunCheckpoint {
                            root: &winner,
                            children: &[],
                        },
                        SessionStatus::Running,
                        &RunStateV1::default(),
                    )
                    .await
                    .map_err(|e| {
                        graph_flow::GraphError::StorageError(format!(
                            "fixture winner commit failed: {e}"
                        ))
                    })?;
                Ok(TaskResult::new(
                    Some("done".to_string()),
                    NextAction::Continue,
                ))
            }
        }

        let db = tempfile::NamedTempFile::new().unwrap();
        let guarded = nexus_local_db::init_engine_pool(db.path())
            .await
            .expect("open pool");
        let pool = guarded.clone_pool();
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let storage: Arc<dyn SessionStorage> = sqlite.clone();

        let caps = nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(
            CapabilityRegistry::with_builtins(),
        ));
        let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );

        let calls = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let race_graph = Arc::new(
            graph_flow::GraphBuilder::new("race-graph")
                .add_task(Arc::new(CompetingWinnerTask {
                    store: sqlite.clone(),
                    calls: calls.clone(),
                }))
                .build()
                .expect("race graph"),
        );
        let session_id = engine
            .start_session("novel-writing", race_graph)
            .await
            .expect("start session");

        let err = engine
            .run_step(&session_id)
            .await
            .expect_err("the losing drive must surface the graph-save conflict");
        let conflict = match &err {
            EngineError::StepFailed {
                source, witness, ..
            } => {
                assert!(
                    witness.owned_graph_version.is_some(),
                    "graph-save conflict must carry a graph-clock witness"
                );
                source.as_ref()
            }
            other => other,
        };
        assert!(
            matches!(
                conflict,
                EngineError::GraphFlow(graph_flow::GraphError::SessionConflict(_))
            ),
            "expected GraphFlow(SessionConflict), got {err:?}"
        );

        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        let row = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root row");
        assert_eq!(row.current_task_id, "winner-task");
        assert_eq!(
            row.context.get::<String>("winner.marker").as_deref(),
            Some("committed")
        );
        assert!(
            row.context.get::<String>("_run_status").is_none()
                && row.context.get::<String>("_run_error").is_none(),
            "a losing drive must never write failure context"
        );
        let record = store
            .load_run(&session_id)
            .await
            .expect("load_run")
            .expect("record");
        assert_eq!(
            record.state_revision, 3,
            "winner revision retained (1 start + 1 mark + 1 win)"
        );
        assert_eq!(record.status, SessionStatus::Running);
    }

    fn recovery_root_descriptor() -> RunDescriptorV1 {
        RunDescriptorV1 {
            creator_id: "creator-1".to_string(),
            work_id: None,
            workspace_root: std::path::PathBuf::from("/tmp/ws"),
            preset_id: "novel-writing".to_string(),
            preset_version: 3,
            source: PresetSourceIdentity::Embedded {
                preset_id: "novel-writing".to_string(),
                content_hash: [7u8; 32],
            },
            input: serde_json::Map::new(),
            agent_bindings: HashMap::new(),
            parent_session_id: None,
            graph_name: None,
            applied: None,
            intake_mode: None,
        }
    }

    /// Persisted child row carrying `descriptor` at `execution_version`.
    fn persisted_child_record(
        session_id: &str,
        execution_version: u32,
        descriptor: Option<RunDescriptorV1>,
    ) -> RunRecord {
        RunRecord {
            session_id: SessionId(session_id.to_string()),
            status: SessionStatus::Running,
            state_revision: 1,
            execution_version,
            descriptor,
            state: Some(RunStateV1::default()),
            graph_version: 1,
        }
    }

    /// The one `validate_descendant_position` rejection branch the shipped
    /// v1.186 matrix
    /// (`descendant_position_validation_rejects_valid_but_wrong_inner_graph`)
    /// does not already pin: a child row with no `graph_name` at all. That
    /// existing test already covers the unknown inner graph, the nested
    /// non-root parent, the disagreeing entered graph, the unrecorded
    /// past-position child, and the tolerated recorded/terminal/reattach
    /// cases, so they are not restated here. The caller-level marker
    /// projection that drives the past-position branch end to end is covered
    /// by the `workflow_run_state` integration scenarios.
    #[test]
    fn recovery_child_identity_child_without_graph_name_is_non_replayable() {
        let valid_inner: std::collections::HashSet<&str> =
            ["inner_graph", "other_inner"].into_iter().collect();

        let err = validate_descendant_position(
            "novel-writing",
            &valid_inner,
            Some("inner_graph"),
            true,
            None,
            "root",
            "state_s",
            "child",
            None,
            false,
        )
        .expect_err("a child without a graph name is non-replayable");
        assert!(err.to_string().contains("has no graph_name"), "got {err}");
    }

    /// `validate_child_descriptor_identity` rejects every tampered branch of
    /// the frozen child identity: a v1 child under a descriptor-less parent,
    /// a v1 child without its own descriptor, a descriptor naming a different
    /// root, a descriptor with no graph name, and a descriptor forking the
    /// trusted creator/preset/version/source. A matching descriptor and a
    /// legacy v0 child are accepted.
    #[test]
    fn recovery_child_identity_descriptor_identity_rejects_tampered_branches() {
        let root = SessionId("root".to_string());
        let parent = recovery_root_descriptor();
        let matching = RunDescriptorV1 {
            parent_session_id: Some(root.clone()),
            graph_name: Some("inner_graph".to_string()),
            ..parent.clone()
        };

        let err = validate_child_descriptor_identity(
            &root,
            None,
            &persisted_child_record("child", 1, Some(matching.clone())),
        )
        .expect_err("a v1 child under a descriptor-less parent is non-replayable");
        assert!(
            err.to_string().contains("has no frozen v1 descriptor"),
            "got {err}"
        );

        let err = validate_child_descriptor_identity(
            &root,
            Some(&parent),
            &persisted_child_record("child", 1, None),
        )
        .expect_err("a v1 child without a descriptor is non-replayable");
        assert!(
            err.to_string()
                .contains("child has no frozen v1 descriptor"),
            "got {err}"
        );

        let err = validate_child_descriptor_identity(
            &root,
            Some(&parent),
            &persisted_child_record(
                "child",
                1,
                Some(RunDescriptorV1 {
                    parent_session_id: Some(SessionId("other-root".to_string())),
                    ..matching.clone()
                }),
            ),
        )
        .expect_err("a descriptor naming another root is non-replayable");
        assert!(
            err.to_string()
                .contains("does not name the recovering root"),
            "got {err}"
        );

        let err = validate_child_descriptor_identity(
            &root,
            Some(&parent),
            &persisted_child_record(
                "child",
                1,
                Some(RunDescriptorV1 {
                    graph_name: None,
                    ..matching.clone()
                }),
            ),
        )
        .expect_err("a descriptor without a graph name is non-replayable");
        assert!(
            err.to_string().contains("graph_name is absent"),
            "got {err}"
        );

        let forked = [
            (
                "creator_id",
                RunDescriptorV1 {
                    creator_id: "other-creator".to_string(),
                    ..matching.clone()
                },
            ),
            (
                "preset_id",
                RunDescriptorV1 {
                    preset_id: "other-preset".to_string(),
                    ..matching.clone()
                },
            ),
            (
                "preset_version",
                RunDescriptorV1 {
                    preset_version: 999,
                    ..matching.clone()
                },
            ),
            (
                "source",
                RunDescriptorV1 {
                    source: PresetSourceIdentity::Embedded {
                        preset_id: "novel-writing".to_string(),
                        content_hash: [9u8; 32],
                    },
                    ..matching.clone()
                },
            ),
        ];
        for (field, descriptor) in forked {
            let err = validate_child_descriptor_identity(
                &root,
                Some(&parent),
                &persisted_child_record("child", 1, Some(descriptor)),
            )
            .expect_err("a forked frozen identity is non-replayable");
            assert!(
                err.to_string().contains("do not match the trusted root"),
                "{field}: got {err}"
            );
        }

        validate_child_descriptor_identity(
            &root,
            Some(&parent),
            &persisted_child_record("child", 1, Some(matching)),
        )
        .expect("a matching frozen identity is replayable");

        validate_child_descriptor_identity(&root, None, &persisted_child_record("child", 0, None))
            .expect("a legacy v0 child is not identity-checked");
    }
}
