//! nexus-session-storage — session identity/status types and the storage
//! error contract for the orchestration session store.
//!
// why: extracted from `nexus-orchestration::engine` so session-store code can
// depend on these types without depending on the engine; the engine module
// re-exports them so existing consumer paths stay unchanged.

pub mod run_state;
pub mod storage;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SessionId(pub String);

/// note: returned by `OrchestrationEngine::list_active`.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub creator_id: String,
    pub preset_id: String,
    pub status: SessionStatus,
    pub current_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SessionStatus {
    Running,
    Paused,
    WaitingForInput,
    Completed,
    Failed,
    /// note: cancelled by an operator; terminal, never auto-driven.
    Cancelled,
    /// note: interrupted — uncertain in-flight work; stopped, never safe retry.
    Interrupted,
}

impl SessionStatus {
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }

    #[must_use]
    pub const fn is_completed(&self) -> bool {
        matches!(self, Self::Completed)
    }

    /// note: maps to the authoritative DB `status` string (A2).
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

    /// why: unknown/unsupported values return `None` (A7) — a corrupt or
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

/// invariant: variant shapes and `Display` strings mirror the session-persistence
/// variants of `nexus-orchestration::engine::EngineError` byte-for-byte, so
/// `From<StorageError> for EngineError` preserves the exact rendering.
#[derive(Error, Debug)]
pub enum StorageError {
    /// note: underlying graph-flow storage failure (serialization, CAS bookkeeping).
    #[error("graph-flow error: {0}")]
    GraphFlow(#[from] graph_flow::GraphError),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    /// invariant: a revision compare-and-swap failed because the persisted
    /// `state_revision` no longer equals the expected value (a concurrent
    /// transition won).
    #[error(
        "state revision mismatch for session {session_id}: expected {expected}, found {found}"
    )]
    RevisionMismatch {
        session_id: String,
        expected: u64,
        found: u64,
    },
    /// invariant: a transition was attempted on a row that is already terminal or
    /// cancelled — late graph saves must not overwrite newer checkpoint state.
    #[error("session {0} is already terminal/cancelled; refusing transition")]
    TerminalState(String),
    /// invariant: a real explicit new start creates a **new** v1 run (new session
    /// id); it never launders an existing uncertain legacy row in place (A2).
    #[error(
        "session {session_id} already exists (execution_version={execution_version}); \
         a new run must use a new session id"
    )]
    RunAlreadyExists {
        session_id: String,
        execution_version: u32,
    },
    /// note: a human-wait continuation token was stale, consumed, or wrong (A4).
    ///
    /// The caller must surface the current persisted status and current
    /// wait id (null when none) so the operator can re-issue the exact
    /// token. No mutation occurred and no second driver was started.
    #[error("wait conflict for session {session_id}: token does not match the current wait")]
    WaitConflict {
        session_id: String,
        status: SessionStatus,
        /// note: current durable wait id (null when the run is not waiting).
        current_wait_id: Option<String>,
    },
}
