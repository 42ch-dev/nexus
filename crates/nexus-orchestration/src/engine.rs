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
//! Design: `.mstar/knowledge/specs/orchestration-engine.md` §4.2.

use async_trait::async_trait;
use graph_flow::{ExecutionStatus, FlowRunner, Graph, SessionStorage};
use std::sync::Arc;
use thiserror::Error;

// Re-export for internal use.
use crate::capability::CapabilityRegistry;

// ---------------------------------------------------------------------------
// Helper types
// ---------------------------------------------------------------------------

/// Opaque session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    Paused,
    WaitingForInput,
    Completed,
    Failed,
}

impl SessionStatus {
    /// Returns `true` if the session is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }

    /// Returns `true` if the session has completed successfully.
    #[must_use]
    pub const fn is_completed(&self) -> bool {
        matches!(self, Self::Completed)
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

    /// Retrieve the context for a session.
    async fn get_context(&self, session_id: &SessionId)
        -> Result<graph_flow::Context, EngineError>;

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
}

impl EngineSharedState {
    /// Create empty shared state with the given storage.
    pub fn new(storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            storage,
            runners: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            sessions: Arc::new(tokio::sync::RwLock::new(Vec::new())),
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

    /// Run a single step for a session, updating status after execution.
    ///
    /// Common logic shared between `GraphFlowEngine` and `EngineProxy`.
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

        // Execute one step using the Arc<FlowRunner>.
        let result = runner.run(&session_id.0).await?;

        // Update in-memory status.
        let status = match &result.status {
            ExecutionStatus::Completed => SessionStatus::Completed,
            ExecutionStatus::Error(_) => SessionStatus::Failed,
            ExecutionStatus::WaitingForInput => SessionStatus::WaitingForInput,
            ExecutionStatus::Paused { .. } => SessionStatus::Paused,
        };
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
        let session_id = format!("{}:{}", id_prefix, chrono::Utc::now().timestamp_millis());
        let start_task_id = graph.start_task_id().unwrap_or_default();
        let session = graph_flow::Session::new_from_task(session_id.clone(), &start_task_id);
        session.context.set("_session_id", session_id.clone()).await;
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
        let mut sessions = self.state.sessions.write().await;
        if let Some(s) = sessions.iter_mut().find(|s| s.session_id == *session_id) {
            match signal {
                EngineSignal::Pause => s.status = SessionStatus::Paused,
                EngineSignal::Resume | EngineSignal::Advance => s.status = SessionStatus::Running,
                EngineSignal::Cancel => s.status = SessionStatus::Failed,
            }
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
        let child_session_id = format!(
            "{}:child:{}",
            params.parent_session_id,
            chrono::Utc::now().timestamp_millis()
        );
        let start_task_id = params.inner_graph.start_task_id().unwrap_or_default();
        let mut session_mut =
            graph_flow::Session::new_from_task(child_session_id.clone(), &start_task_id);
        session_mut.context = params.initial_context;
        self.state.storage.save(session_mut).await?;
        // WS2 R3: Store Arc<FlowRunner> instead of FlowRunner.
        let runner = Arc::new(graph_flow::FlowRunner::new(
            params.inner_graph,
            self.state.storage.clone(),
        ));
        self.state
            .runners
            .write()
            .await
            .insert(child_session_id.clone(), runner);
        self.state.sessions.write().await.push(SessionSummary {
            session_id: SessionId(child_session_id.clone()),
            creator_id: String::new(),
            preset_id: String::new(),
            status: SessionStatus::Running,
            current_task_id: Some(start_task_id),
        });
        Ok(SessionId(child_session_id))
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
    /// Shared capability registry (propagated to composite tasks at runtime).
    caps: Arc<CapabilityRegistry>,
}

impl Clone for GraphFlowEngine {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            caps: self.caps.clone(),
        }
    }
}

impl GraphFlowEngine {
    /// Create a new engine that persists sessions into `storage`.
    ///
    /// The `storage` parameter accepts **any** [`SessionStorage`] implementation
    /// — `InMemorySessionStorage` for tests, `SqliteSessionStorage` for
    /// production.
    pub fn new_with_storage(
        storage: Arc<dyn SessionStorage>,
        caps: Arc<CapabilityRegistry>,
    ) -> Self {
        Self {
            state: Arc::new(EngineSharedState::new(storage)),
            caps,
        }
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
    async fn reconstruct_runner(&self, summary: &SessionSummary) -> Result<(), EngineError> {
        // Step 1: Load the embedded preset by preset_id.
        let loaded =
            crate::preset::load_embedded_preset(&summary.preset_id, &self.caps).map_err(|e| {
                EngineError::GraphFlow(graph_flow::GraphError::StorageError(format!(
                    "R6: failed to load embedded preset '{}' for session {}: {}",
                    summary.preset_id, summary.session_id.0, e
                )))
            })?;

        // Step 2: Build the wired outer graph using EngineProxy + capabilities.
        let proxy = Arc::new(EngineProxy {
            state: self.state.clone(),
        });
        let engine_proxy: Arc<dyn OrchestrationEngine> = proxy;
        let wired =
            crate::preset::loader::build_wired_outer_graph(&loaded, &engine_proxy, &self.caps);

        // Step 3: Create FlowRunner with the wired graph and existing storage.
        // The storage already contains the persisted session data, so the
        // runner will resume from the correct execution position.
        let runner = Arc::new(FlowRunner::new(Arc::new(wired), self.state.storage.clone()));

        // Step 4: Store the runner in the shared state.
        self.state
            .runners
            .write()
            .await
            .insert(summary.session_id.0.clone(), runner);

        tracing::info!(
            "R6: reconstructed runner for session {} (preset: {})",
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
        let session_id = format!("{}:{}", preset_id, chrono::Utc::now().timestamp_millis());

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
        self.state.storage.save(session).await?;

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
        self.state.storage.save(session).await?;

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
        let mut sessions = self.state.sessions.write().await;
        if let Some(s) = sessions.iter_mut().find(|s| s.session_id == *session_id) {
            match signal {
                EngineSignal::Pause => s.status = SessionStatus::Paused,
                EngineSignal::Resume | EngineSignal::Advance => s.status = SessionStatus::Running,
                EngineSignal::Cancel => s.status = SessionStatus::Failed,
            }
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
        let child_session_id = format!(
            "{}:child:{}",
            params.parent_session_id,
            chrono::Utc::now().timestamp_millis()
        );

        let start_task_id = params.inner_graph.start_task_id().unwrap_or_default();

        // Create a child session with the provided initial context.
        let mut session_mut =
            graph_flow::Session::new_from_task(child_session_id.clone(), &start_task_id);
        session_mut.context = params.initial_context;
        self.state.storage.save(session_mut).await?;

        // WS2 R3: Store Arc<FlowRunner> instead of FlowRunner.
        let runner = Arc::new(FlowRunner::new(
            params.inner_graph,
            self.state.storage.clone(),
        ));
        self.state
            .runners
            .write()
            .await
            .insert(child_session_id.clone(), runner);

        let summary = SessionSummary {
            session_id: SessionId(child_session_id.clone()),
            creator_id: String::new(),
            preset_id: String::new(),
            status: SessionStatus::Running,
            current_task_id: Some(start_task_id),
        };

        self.state.sessions.write().await.push(summary);

        Ok(SessionId(child_session_id))
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

    async fn start_session_with_preset(
        &self,
        loaded: &crate::preset::LoadedPreset,
    ) -> Result<SessionId, EngineError> {
        // WS3 R1: Use EngineProxy wrapping EngineSharedState.
        let proxy: Arc<dyn OrchestrationEngine> = Arc::new(EngineProxy {
            state: self.state.clone(),
        });
        let wired = crate::preset::loader::build_wired_outer_graph(loaded, &proxy, &self.caps);
        self.start_session(&loaded.id, Arc::new(wired)).await
    }

    async fn start_session_with_preset_for_creator(
        &self,
        loaded: &crate::preset::LoadedPreset,
        creator_id: &str,
    ) -> Result<SessionId, EngineError> {
        let proxy: Arc<dyn OrchestrationEngine> = Arc::new(EngineProxy {
            state: self.state.clone(),
        });
        let wired = crate::preset::loader::build_wired_outer_graph(loaded, &proxy, &self.caps);
        self.start_session_with_creator(&loaded.id, Arc::new(wired), Some(creator_id))
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
        let caps = Arc::new(CapabilityRegistry::with_builtins());
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
        let caps = Arc::new(CapabilityRegistry::with_builtins());
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
