//! nexus-session-storage — session identity/status types and the storage
//! error contract for the orchestration session store.
//!
//! Extracted from `nexus-orchestration::engine` so session-store code can
//! depend on these types without depending on the engine; the engine module
//! re-exports them so existing consumer paths stay unchanged.
// why: engine re-exports preserve the existing consumer paths (see the doc block above).

pub mod run_state;
pub mod storage;

use thiserror::Error;

/// note: Opaque session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SessionId(pub String);

/// note: Lightweight summary returned by `OrchestrationEngine::list_active`.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub creator_id: String,
    pub preset_id: String,
    pub status: SessionStatus,
    pub current_task_id: Option<String>,
}

/// note: Runtime status of a session.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SessionStatus {
    Running,
    Paused,
    WaitingForInput,
    Completed,
    Failed,
    /// note: Cancelled by an operator; terminal, never auto-driven.
    Cancelled,
    /// note: Interrupted — uncertain in-flight work; stopped, never safe retry.
    Interrupted,
}

impl SessionStatus {
    /// note: Returns `true` if the session is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }

    /// note: Returns `true` if the session has completed successfully.
    #[must_use]
    pub const fn is_completed(&self) -> bool {
        matches!(self, Self::Completed)
    }

    /// note: Map to the authoritative DB `status` string (A2).
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

    /// note: Parse a DB `status` string back into a [`SessionStatus`].
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

/// note: Errors produced by the session storage layer.
///
/// Variant shapes and `Display` strings mirror the session-persistence
/// variants of `nexus-orchestration::engine::EngineError` byte-for-byte, so
/// `From<StorageError> for EngineError` preserves the exact rendering.
#[derive(Error, Debug)]
pub enum StorageError {
    /// note: Underlying graph-flow storage failure (serialization, CAS bookkeeping).
    #[error("graph-flow error: {0}")]
    GraphFlow(#[from] graph_flow::GraphError),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    /// invariant: A revision compare-and-swap failed: the persisted `state_revision` no
    /// longer equals the expected value (a concurrent transition won).
    #[error(
        "state revision mismatch for session {session_id}: expected {expected}, found {found}"
    )]
    RevisionMismatch {
        /// note: Session id.
        session_id: String,
        /// note: Expected revision.
        expected: u64,
        /// note: Persisted revision.
        found: u64,
    },
    /// invariant: A transition was attempted on a row that is already terminal or
    /// cancelled — late graph saves must not overwrite newer checkpoint state.
    #[error("session {0} is already terminal/cancelled; refusing transition")]
    TerminalState(String),
    /// invariant: A new v1 run was requested for a session id that already has a row.
    ///
    /// A real explicit new start creates a **new** v1 run (new session id);
    /// it never launders an existing uncertain legacy row in place (A2).
    #[error(
        "session {session_id} already exists (execution_version={execution_version}); \
         a new run must use a new session id"
    )]
    RunAlreadyExists {
        /// note: Session id.
        session_id: String,
        /// note: Persisted execution version of the existing row.
        execution_version: u32,
    },
    /// note: A human-wait continuation token was stale, consumed, or wrong (A4).
    ///
    /// The caller must surface the current persisted status and current
    /// wait id (null when none) so the operator can re-issue the exact
    /// token. No mutation occurred and no second driver was started.
    #[error("wait conflict for session {session_id}: token does not match the current wait")]
    WaitConflict {
        /// note: Session id.
        session_id: String,
        /// note: Current persisted status.
        status: SessionStatus,
        /// note: Current durable wait id (null when the run is not waiting).
        current_wait_id: Option<String>,
    },
}
