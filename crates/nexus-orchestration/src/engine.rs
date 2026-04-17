//! OrchestrationEngine trait + GraphFlowEngine adapter over `graph-flow`.
//!
//! Design: `.agents/plans/knowledge/orchestration-engine-v1.md` §4.2.

use async_trait::async_trait;
use graph_flow::{ExecutionStatus, FlowRunner, Graph, SessionStorage};
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

impl SessionStatus {
    /// Returns `true` if the session is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, SessionStatus::Completed | SessionStatus::Failed)
    }

    /// Returns `true` if the session has completed successfully.
    pub fn is_completed(&self) -> bool {
        matches!(self, SessionStatus::Completed)
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

    /// Query the current status of a session.
    async fn get_status(&self, session_id: &SessionId) -> Result<SessionStatus, EngineError>;

    /// Send a control signal (pause / resume / cancel / advance) to a session.
    async fn signal(&self, session_id: &SessionId, signal: EngineSignal)
        -> Result<(), EngineError>;

    /// List sessions that are still active (running / paused / waiting).
    async fn list_active(&self, filter: SessionFilter) -> Result<Vec<SessionSummary>, EngineError>;
}

// ---------------------------------------------------------------------------
// GraphFlowEngine — adapter over graph-flow
// ---------------------------------------------------------------------------

/// Concrete [`OrchestrationEngine`] backed by [`graph_flow::FlowRunner`].
///
/// The engine stores a `FlowRunner` per session (each session may use a
/// different graph, e.g. `_system.maintenance` vs user presets). Sessions
/// are persisted via the provided [`SessionStorage`].
pub struct GraphFlowEngine {
    storage: Arc<dyn SessionStorage>,
    /// Per-session FlowRunners (graph + storage combo for `run()` calls).
    runners: tokio::sync::RwLock<std::collections::HashMap<String, FlowRunner>>,
    /// In-memory bookkeeping of active sessions.
    /// (graph-flow's `SessionStorage` has no `list` method, so we track here.)
    sessions: tokio::sync::RwLock<Vec<SessionSummary>>,
}

impl GraphFlowEngine {
    /// Create a new engine that persists sessions into `storage`.
    ///
    /// The `storage` parameter accepts **any** [`SessionStorage`] implementation
    /// — `InMemorySessionStorage` for tests, `SqliteSessionStorage` for
    /// production.
    pub fn new_with_storage(storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            storage,
            runners: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            sessions: tokio::sync::RwLock::new(Vec::new()),
        }
    }

    /// Start a session on a specific graph.
    ///
    /// Creates a [`graph_flow::Session`] seeded at the graph's start task,
    /// stores it, and registers a [`FlowRunner`] for future `run_step` calls.
    ///
    /// Returns the session ID.
    pub async fn start_session(
        &self,
        preset_id: &str,
        graph: Arc<Graph>,
    ) -> Result<SessionId, EngineError> {
        let session_id = format!("{}:{}", preset_id, chrono::Utc::now().timestamp_millis());

        // Determine the start task from the graph.
        let start_task_id = graph.start_task_id().unwrap_or_default();

        // Create and persist the session.
        let session = graph_flow::Session::new_from_task(session_id.clone(), &start_task_id);
        self.storage.save(session).await?;

        // Create a FlowRunner for this session.
        let runner = FlowRunner::new(graph, self.storage.clone());
        self.runners
            .write()
            .await
            .insert(session_id.clone(), runner);

        // Track in memory.
        let summary = SessionSummary {
            session_id: SessionId(session_id.clone()),
            creator_id: String::new(),
            preset_id: preset_id.to_string(),
            status: SessionStatus::Running,
            current_task_id: Some(start_task_id),
        };

        self.sessions.write().await.push(summary);

        Ok(SessionId(session_id))
    }
}

#[async_trait]
impl OrchestrationEngine for GraphFlowEngine {
    async fn run_step(&self, session_id: &SessionId) -> Result<StepOutcome, EngineError> {
        // Look up the FlowRunner for this session.
        let runner = {
            let runners = self.runners.read().await;
            runners
                .get(&session_id.0)
                .cloned()
                .ok_or(EngineError::NoGraphLoaded)?
        };

        // Execute one step.
        let result = runner.run(&session_id.0).await?;

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

        // Update our in-memory bookkeeping.
        self.update_session_status(&session_id.0, &result.status)
            .await;

        Ok(outcome)
    }

    async fn new_session(&self, key: SessionKey, _ctx: Context) -> Result<SessionId, EngineError> {
        let session_id = format!("{}:{}", key.preset_id, key.instance_id);

        // Persist a session stub into the graph-flow storage.
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

    async fn get_status(&self, session_id: &SessionId) -> Result<SessionStatus, EngineError> {
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

    async fn list_active(&self, filter: SessionFilter) -> Result<Vec<SessionSummary>, EngineError> {
        let sessions = self.sessions.read().await;
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
}

impl GraphFlowEngine {
    /// Update in-memory session status after a step.
    async fn update_session_status(&self, session_id: &str, exec_status: &ExecutionStatus) {
        let status = match exec_status {
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
            .find(|s| s.session_id.0 == session_id)
        {
            s.status = status;
        }
    }
}
