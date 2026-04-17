//! OrchestrationEngine trait + GraphFlowEngine adapter over `graph-flow`.
//!
//! Design: `.agents/plans/knowledge/orchestration-engine-v1.md` §4.2.

use async_trait::async_trait;
use graph_flow::SessionStorage;
use std::sync::Arc;
use thiserror::Error;

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

/// Outcome of a single engine step.
#[derive(Debug, Clone)]
pub enum StepOutcome {
    Completed { response: Option<String> },
    Paused { next_task_id: String, reason: String },
    WaitingForInput { response: Option<String> },
    Error(String),
}

/// Signals that external callers (HTTP, CLI) can send to the engine.
#[derive(Debug, Clone)]
pub enum EngineSignal {
    Pause,
    Resume,
    Cancel,
    Advance,
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
    #[error("no graph loaded — run_step requires a graph (set via start_session or system preset)")]
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
    async fn new_session(
        &self,
        key: SessionKey,
        ctx: Context,
    ) -> Result<SessionId, EngineError>;

    /// Query the current status of a session.
    async fn get_status(
        &self,
        session_id: &SessionId,
    ) -> Result<SessionStatus, EngineError>;

    /// Send a control signal (pause / resume / cancel / advance) to a session.
    async fn signal(
        &self,
        session_id: &SessionId,
        signal: EngineSignal,
    ) -> Result<(), EngineError>;

    /// List sessions that are still active (running / paused / waiting).
    async fn list_active(
        &self,
        filter: SessionFilter,
    ) -> Result<Vec<SessionSummary>, EngineError>;
}

// ---------------------------------------------------------------------------
// GraphFlowEngine — adapter over graph-flow
// ---------------------------------------------------------------------------

/// Concrete [`OrchestrationEngine`] backed by [`graph_flow::FlowRunner`].
///
/// T2 provides `new_with_storage` and the core trait methods.
/// Later tasks wire a real [`graph_flow::FlowRunner`] (once a [`Graph`] is
/// available via system preset or preset loader).
pub struct GraphFlowEngine {
    storage: Arc<dyn SessionStorage>,
    /// In-memory bookkeeping of active sessions.
    /// (graph-flow's `SessionStorage` has no `list` method, so we track here.)
    sessions: tokio::sync::RwLock<Vec<SessionSummary>>,
}

impl GraphFlowEngine {
    /// Create a new engine that persists sessions into `storage`.
    ///
    /// The `storage` parameter accepts **any** [`SessionStorage`] implementation
    /// — `InMemorySessionStorage` for tests, `SqliteSessionStorage` for
    /// production (T3).
    pub fn new_with_storage(storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            storage,
            sessions: tokio::sync::RwLock::new(Vec::new()),
        }
    }
}

#[async_trait]
impl OrchestrationEngine for GraphFlowEngine {
    async fn run_step(&self, _session_id: &SessionId) -> Result<StepOutcome, EngineError> {
        // Placeholder — needs a FlowRunner + Graph.
        // T6 wires `_system.maintenance` and makes this functional.
        Err(EngineError::NoGraphLoaded)
    }

    async fn new_session(
        &self,
        key: SessionKey,
        _ctx: Context,
    ) -> Result<SessionId, EngineError> {
        let session_id = format!("{}:{}", key.preset_id, key.instance_id);

        // Persist a session stub into the graph-flow storage so that
        // future `run_step` calls can load it.
        let session = graph_flow::Session::new_from_task(session_id.clone(), "");
        self.storage.save(session).await?;

        let summary = SessionSummary {
            session_id: SessionId(session_id.clone()),
            creator_id: key.creator_id,
            preset_id: key.preset_id,
            status: SessionStatus::Running,
            current_task_id: None,
        };

        self.sessions.write().await.push(summary);

        Ok(SessionId(session_id))
    }

    async fn get_status(
        &self,
        session_id: &SessionId,
    ) -> Result<SessionStatus, EngineError> {
        let sessions = self.sessions.read().await;
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
        let mut sessions = self.sessions.write().await;
        if let Some(s) = sessions.iter_mut().find(|s| s.session_id == *session_id) {
            match signal {
                EngineSignal::Pause => s.status = SessionStatus::Paused,
                EngineSignal::Resume => s.status = SessionStatus::Running,
                EngineSignal::Cancel => s.status = SessionStatus::Failed,
                EngineSignal::Advance => s.status = SessionStatus::Running,
            }
            Ok(())
        } else {
            Err(EngineError::SessionNotFound(session_id.0.clone()))
        }
    }

    async fn list_active(
        &self,
        filter: SessionFilter,
    ) -> Result<Vec<SessionSummary>, EngineError> {
        let sessions = self.sessions.read().await;
        Ok(sessions
            .iter()
            .filter(|s| {
                let status_ok = matches!(
                    s.status,
                    SessionStatus::Running
                        | SessionStatus::Paused
                        | SessionStatus::WaitingForInput
                );
                let creator_ok = filter
                    .creator_id
                    .as_ref()
                    .is_none_or(|c| c == &s.creator_id);
                let preset_ok = filter
                    .preset_id
                    .as_ref()
                    .is_none_or(|p| p == &s.preset_id);
                status_ok && creator_ok && preset_ok
            })
            .cloned()
            .collect())
    }
}
