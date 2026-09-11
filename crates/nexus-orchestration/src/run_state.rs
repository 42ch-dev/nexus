//! Durable run state types (v1.186 P0, A2).
//!
//! Authoritative contracts for the durable workflow-state slice. These types
//! freeze the server-side run descriptor, the durable wait/in-flight/cancel/
//! failure state, and the transactional store that persists root+child
//! checkpoints under a root revision compare-and-swap.
//!
//! Design: `.mstar/iterations/v1.186/guides/architecture-decisions.md` A2/A4/A5.

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::engine::{EngineError, SessionId, SessionStatus};

/// Frozen server-side descriptor for a v1 run (A2).
///
/// Validated and frozen at admission. Contains **no** env values, credentials
/// or live handles — only the identity needed to reconstruct a supported
/// preset session after restart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunDescriptorV1 {
    /// Owning creator id.
    pub creator_id: String,
    /// Optional work id (schedule/chain origin).
    pub work_id: Option<String>,
    /// Resolved workspace root the run executes in.
    pub workspace_root: PathBuf,
    /// Preset identifier.
    pub preset_id: String,
    /// Preset schema version.
    pub preset_version: u32,
    /// Content-addressed preset source identity (A2/A7).
    pub source: PresetSourceIdentity,
    /// Frozen preset input map.
    pub input: serde_json::Map<String, serde_json::Value>,
    /// Role → provider binding map (A1).
    pub agent_bindings: HashMap<String, AgentBinding>,
    /// Parent session id for inner-graph child runs.
    pub parent_session_id: Option<SessionId>,
    /// Named inner graph this run executes (child runs).
    pub graph_name: Option<String>,
}

/// Role → provider binding (A1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentBinding {
    /// Provider id.
    pub provider_id: String,
    /// Optional model id.
    pub model: Option<String>,
}

/// Content-addressed preset source identity (A2/A7).
///
/// The content hash is over the manifest **and** referenced prompt/template
/// bytes — not the YAML hash alone — so a changed template invalidates the
/// identity and recovery refuses to fall back to current bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PresetSourceIdentity {
    /// Compiled-in embedded preset.
    Embedded {
        /// Preset id.
        preset_id: String,
        /// blake3 over manifest + referenced template bytes.
        content_hash: [u8; 32],
    },
    /// On-disk preset bundle (user/system directory).
    Directory {
        /// Resolved bundle root.
        root: PathBuf,
        /// blake3 over manifest + referenced template bytes.
        content_hash: [u8; 32],
    },
}

/// Admission matrix snapshot for a schedule (N-1).
///
/// The store re-checks due-time, dependency satisfaction, and the
/// per-creator concurrency rule INSIDE the same `BEGIN IMMEDIATE` claim
/// transaction as the schedule claim + session insert, so two distinct
/// serial schedules for one creator can never both pass a preflight and
/// both claim. The coordinator snapshots these values from the schedule
/// row/dependency table before admission; the store re-verifies them under
/// the write lock where the running set is authoritative.
#[derive(Debug, Clone)]
pub struct ScheduleAdmissionGate {
    /// Owning creator id (per-creator concurrency scope).
    pub creator_id: String,
    /// `scheduled_at` (Unix seconds) — `None` = on-demand, always due.
    pub scheduled_at: Option<i64>,
    /// `depends_on` entries; each must be `completed` or `cancelled`.
    pub depends_on: Vec<String>,
    /// `serial` | `parallel_with` | `parallel_any` (unknown → fail closed).
    pub concurrency_kind: String,
    /// JSON array of whitelisted schedule ids for `parallel_with`.
    pub concurrency_whitelist: Option<String>,
}

/// Durable human-wait record (A4).
///
/// A fresh token is generated on **each** arrival at a human wait, even if
/// the same task loops; it is retained through restart until a successful
/// CAS consumes it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WaitRecord {
    /// Fresh UUID token per arrival.
    pub wait_id: String,
    /// Task id at the wait point.
    pub task_id: String,
    /// Child session id when the wait is on a nested child.
    pub child_session_id: Option<String>,
    /// Child task id when the wait is on a nested child.
    pub child_task_id: Option<String>,
    /// Wait kind — always `manual` for human waits.
    pub kind: WaitKind,
}

/// Wait kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WaitKind {
    /// Human approval wait.
    Manual,
}

/// In-flight prompt attempt (A2).
///
/// Persisted in `dispatching` phase **before** any external Host effect, so a
/// crash after the effect but before the result checkpoint leaves the run
/// interrupted (never auto-replayed).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptAttempt {
    /// Attempt id (UUID).
    pub attempt_id: String,
    /// Task id the prompt belongs to.
    pub task_id: String,
    /// Dispatch phase.
    pub phase: PromptPhase,
    /// Host session id once allocated.
    pub host_session_id: Option<String>,
    /// Host operation id once allocated.
    pub operation_id: Option<String>,
    /// Owned process identity once launched (A5).
    pub process_identity: Option<OwnedProcessIdentity>,
}

/// Prompt dispatch phase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptPhase {
    /// Persisted before the external Host effect.
    Dispatching,
    /// Host effect in flight.
    Active,
}

/// Opaque owned process identity (A5) — PID plus platform process-birth and
/// owned group identity; never PID alone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnedProcessIdentity {
    /// Process id.
    pub pid: u32,
    /// Platform process-birth identity (e.g. start time) for reuse detection.
    pub process_birth: Option<String>,
    /// Owned process-group identity for tree cleanup.
    pub group_id: Option<String>,
}

/// Stable machine failure code (A2) — not provider-prose parsing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunFailure {
    /// Stable machine code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

/// Durable run state (A2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RunStateV1 {
    /// Active human wait (None when not waiting).
    pub wait: Option<WaitRecord>,
    /// Current task id set before stepping, cleared only with a completed
    /// checkpoint (a current-position safety marker, not an activity journal).
    pub step_in_flight: Option<String>,
    /// In-flight prompt attempt (None when no prompt is dispatching/active).
    pub in_flight: Option<PromptAttempt>,
    /// Durable failure record (None when not failed).
    pub failure: Option<RunFailure>,
    /// Cancellation requested but not yet confirmed.
    pub cancel_requested: bool,
}

/// Authoritative run record (A2).
///
/// `execution_version = 0` marks a **legacy/unverified** row: the status is
/// not authoritative and the descriptor/state are absent. v1 rows carry the
/// authoritative status plus descriptor and state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRecord {
    /// Run id.
    pub session_id: SessionId,
    /// Authoritative status (v1 rows) or legacy diagnostic (v0 rows).
    pub status: SessionStatus,
    /// Current state revision (CAS anchor).
    pub state_revision: u64,
    /// Execution version — `0` = legacy/unverified, `>=1` = v1 authoritative.
    pub execution_version: u32,
    /// Frozen descriptor (v1 rows only).
    pub descriptor: Option<RunDescriptorV1>,
    /// Durable run state (v1 rows only).
    pub state: Option<RunStateV1>,
    /// Durable graph-flow session version (graph clock; distinct from the
    /// workflow control CAS `state_revision`).
    pub graph_version: u64,
}

/// Child checkpoint (A2) — carries the child session, status and run state.
#[derive(Debug, Clone)]
pub struct ChildCheckpoint {
    /// Child session snapshot.
    pub session: graph_flow::Session,
    /// Child status.
    pub status: SessionStatus,
    /// Child durable run state.
    pub state: RunStateV1,
    /// Child state revision (CAS anchor for the child row).
    pub state_revision: u64,
    /// Named inner graph this child executes (A2 child identity).
    pub graph_name: Option<String>,
}

/// Batched root/child checkpoint (A2).
///
/// Carries existing root/child snapshots without requiring a graph/context
/// clone at the trait boundary.
#[derive(Debug, Clone)]
pub struct RunCheckpoint<'a> {
    /// Root session snapshot.
    pub root: &'a graph_flow::Session,
    /// Child session snapshots.
    pub children: &'a [ChildCheckpoint],
}

/// Transactional workflow state store (A2).
///
/// The SQLite adapter commits root+child state atomically under the root
/// revision. In-memory parity exists only for tests / Tier-0 non-executing use.
#[async_trait]
pub trait WorkflowStateStore: Send + Sync {
    /// Load the authoritative run record for a session.
    ///
    /// Returns `None` when the session does not exist. v0 rows return a
    /// legacy/unverified record (`execution_version = 0`, no descriptor/state).
    ///
    /// # Errors
    /// Returns [`EngineError`] on storage failure.
    async fn load_run(&self, session_id: &SessionId) -> Result<Option<RunRecord>, EngineError>;

    /// Start a new v1 run with a frozen descriptor.
    ///
    /// Creates the authoritative v1 record (`execution_version = 1`,
    /// `state_revision = 1`, status `Running`) and writes the descriptor plus
    /// the initial run state. CASes on the row being fresh (not yet v1) so a
    /// real explicit new start never launders an uncertain old run.
    ///
    /// # Errors
    /// Returns [`EngineError`] on storage failure or if the row is already v1.
    async fn start_run(
        &self,
        session_id: &SessionId,
        descriptor: &RunDescriptorV1,
        checkpoint: RunCheckpoint<'_>,
        next_state: &RunStateV1,
    ) -> Result<RunRecord, EngineError>;

    /// Atomically admit a schedule as a driven v1 run (A3).
    ///
    /// One Creator-DB transaction linearizes: the schedule claim
    /// (`status='running'`, `current_session_id=<session_id>`,
    /// `current_core_context_version=<core_context_version>`), the v1
    /// session row (initial checkpoint + frozen descriptor + seeded
    /// context), and the schedule→session identity. A concurrent admission
    /// for the same schedule loses the claim and returns the winner's
    /// already-owned run (exactly one `SessionId` per schedule).
    ///
    /// The schedule row must carry `execution_policy = 'driven_v1'` and be
    /// in `pending`/`paused` with no owned session. A `_system.*` preset is
    /// refused regardless of stored policy (A8 fail-closed). A stale claim
    /// (schedule `Running` with a `current_session_id` whose run row does
    /// not exist) is NOT cleared here — a crash between claim and run
    /// creation is recovered by boot recovery, never by a second admission
    /// minting another session (C-1).
    ///
    /// `expected_core_context_version` fences the claim on the EXACT frozen
    /// version the caller read (N-3): a concurrent context edit that
    /// advanced the row after the read fails the fence — the claim never
    /// overwrites a newer pointer with a stale payload.
    ///
    /// `admission_gate` re-verifies the due-time/dependency/concurrency
    /// matrix INSIDE the claim transaction (N-1): the running set is only
    /// authoritative under the `BEGIN IMMEDIATE` write lock, so two
    /// distinct serial schedules for one creator can never both claim.
    ///
    /// # Errors
    /// Returns [`EngineError`] on storage failure, policy refusal, or when
    /// the schedule is not in an admissible state.
    #[allow(clippy::too_many_arguments)] // atomic claim payload; grouping into a struct would leak the CAS-fence coupling
    async fn admit_schedule_run(
        &self,
        schedule_id: &str,
        session_id: &SessionId,
        descriptor: &RunDescriptorV1,
        checkpoint: RunCheckpoint<'_>,
        next_state: &RunStateV1,
        core_context_version: u32,
        expected_core_context_version: u32,
        admission_gate: Option<&ScheduleAdmissionGate>,
    ) -> Result<RunRecord, EngineError>;

    /// Atomically persist a transition under a root revision compare-and-swap.
    ///
    /// Writes the root checkpoint position/context, status and execution
    /// metadata, plus child checkpoints, in one transaction. Fails (CAS) when
    /// the persisted `state_revision` no longer equals `expected_revision`.
    ///
    /// # Errors
    /// Returns [`EngineError`] on storage failure or revision mismatch.
    async fn commit_transition(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        checkpoint: RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &RunStateV1,
    ) -> Result<RunRecord, EngineError> {
        self.commit_transition_with_graph_fence(
            session_id,
            expected_revision,
            None,
            checkpoint,
            next_status,
            next_state,
        )
        .await
    }

    /// [`commit_transition`](Self::commit_transition) with an additional
    /// graph-clock fence: when `expected_graph_version` is `Some`, the
    /// commit CAS additionally requires the persisted `graph_version` to
    /// equal it, so a graph-only writer that advanced the session clock
    /// without the workflow revision cannot be overwritten by a stale
    /// owner. Control signals fence their loaded snapshot; ordinary cancel
    /// may reload and retry, while failed-step anchored cleanup never rebases.
    ///
    /// # Errors
    /// Returns [`EngineError`] on storage failure or revision/graph
    /// mismatch (both surface as ownership loss).
    async fn commit_transition_with_graph_fence(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: RunCheckpoint<'_>,
        next_status: SessionStatus,
        next_state: &RunStateV1,
    ) -> Result<RunRecord, EngineError>;

    /// Atomically settle a run to terminal `Cancelled` (A5).
    ///
    /// The A5 cancel settlement is the ONLY transition that may move an
    /// `Interrupted` run (a durable cancel intent whose owned-Host cleanup
    /// was previously unconfirmed) to `Cancelled` — the bounded, owner-scoped
    /// cleanup retry. The write is revision-fenced and refuses every other
    /// terminal status (`completed`/`failed`/`cancelled`), so a retry can
    /// never overwrite a newer terminal outcome. The run is terminal and
    /// never re-driven; this method is used only by the cancel path after
    /// `finalize_run` confirms cleanup.
    ///
    /// When `expected_graph_version` is `Some`, the settlement CAS additionally
    /// requires the persisted `graph_version` to equal it, protecting the
    /// checkpoint from a graph-only writer. Ordinary user cancellation passes
    /// the loaded snapshot's version and may reload and retry after a conflict;
    /// anchored cleanup passes its commit-owned clock and never rebases.
    /// `None` provides an explicitly revision-only authoritative write.
    ///
    /// # Errors
    /// Returns [`EngineError`] on storage failure, revision/graph mismatch,
    /// or a terminal status other than `Interrupted`.
    async fn settle_cancelled(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: RunCheckpoint<'_>,
        next_state: &RunStateV1,
    ) -> Result<RunRecord, EngineError>;

    /// Atomically restore a pre-step root snapshot via ONE revision-fenced
    /// storage operation (Important 3).
    ///
    /// Unlike a separate check-then-save (which races concurrent transitions),
    /// this restores `pre_step` (position/context) only when the persisted
    /// `state_revision` still equals `expected_revision` AND the row is still
    /// non-terminal (running or paused). A concurrent transition that wins
    /// between a check and a save is never overwritten — this leaves the row
    /// untouched and returns [`EngineError::RevisionMismatch`].
    ///
    /// Returns the restored row's [`RunRecord`] — the commit-owned clocks of
    /// THIS operation, which a failed step's witness must adopt (the restore
    /// advances only the graph clock; the workflow revision is unchanged).
    ///
    /// # Errors
    /// Returns [`EngineError::RevisionMismatch`] when the persisted revision no
    /// longer equals `expected_revision`; [`EngineError::TerminalState`] when
    /// the row is no longer non-terminal.
    async fn restore_pre_step(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        pre_step: &graph_flow::Session,
    ) -> Result<RunRecord, EngineError>;

    /// Persist the in-flight (current-position safety) intent BEFORE an
    /// external effect dispatch (A2/A7 Important 5), so a crash after an
    /// effect but before the result checkpoint leaves an interrupted (never
    /// replayable) run.
    ///
    /// Writes `step_state` (with `step_in_flight` set) plus the current step
    /// position/context, fenced to `status IN ('running', 'paused')`,
    /// `state_revision = expected_revision`, and (when
    /// `expected_graph_version` is `Some`) the persisted `graph_version`.
    /// The marker atomically advances the revision AND the graph clock by
    /// one, so a concurrently loaded control signal cannot erase it and a
    /// graph-only writer that advanced the session clock without the
    /// workflow revision cannot be overwritten by the stale pre-step
    /// checkpoint. The subsequent transition CAS anchors to the RETURNED
    /// record's clocks.
    ///
    /// Fails when the pre-step row is no longer step-able, its revision
    /// moved, or (with a graph fence) a graph-only winner owns the clock —
    /// the external effect must not run on an unmarked or clobbered row.
    ///
    /// # Errors
    /// Returns [`EngineError`] on storage failure, revision/graph mismatch,
    /// or a non-step-able status.
    async fn mark_step_in_flight(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_graph_version: Option<u64>,
        checkpoint: RunCheckpoint<'_>,
        step_state: &RunStateV1,
    ) -> Result<RunRecord, EngineError>;

    /// Persist the durable `PromptAttempt` dispatch intent (A2/A5) BEFORE
    /// the external Host effect, and update it to `Active` once the Host
    /// session/operation IDs are known.
    ///
    /// The write is **revision-linearized with the engine's control
    /// transitions**: it compare-and-sets on `state_revision =
    /// expected_revision` AND the step marker `step_in_flight =
    /// expected_step` (when `Some`) and fenced to step-able status
    /// (`running`/`paused`), while leaving the revision itself unchanged so
    /// the engine's step transition CAS anchor is preserved. A stale prompt
    /// invocation that loaded the run before a control transition
    /// (e.g. `Cancel`) fails the CAS and is rejected — it can never
    /// overwrite newer state. A crash after the effect but before the result
    /// checkpoint leaves the attempt persisted and the run recovers as
    /// `Interrupted` (never auto-replayed). The engine's terminal/wait
    /// `commit_transition` clears `in_flight` on success.
    ///
    /// **Durable attempt ownership (Important)**: `expected_attempt_id`
    /// names this request's own admission. When `Some`, the row must either
    /// have no `in_flight` attempt yet (the Dispatching claim) or already
    /// carry exactly this operation's attempt id (the Active update of the
    /// SAME operation) — a concurrent same-anchor prompt can never
    /// overwrite or replace the durable in-flight operation with another
    /// attempt's ids. Callers pass `None` only when they intentionally
    /// bypass ownership CAS (storage-level tests).
    ///
    /// # Errors
    /// Returns [`EngineError::RevisionMismatch`] when the persisted revision
    /// no longer equals `expected_revision` (or the step marker moved, or an
    /// unrelated operation owns `in_flight`), [`EngineError::TerminalState`]
    /// when the row is no longer step-able — the external effect must not
    /// run on an unmatching row.
    async fn persist_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        expected_attempt_id: Option<&str>,
        attempt: &PromptAttempt,
    ) -> Result<(), EngineError>;

    /// Clear the durable `in_flight` prompt-attempt marker after the owning
    /// operation completed successfully.
    ///
    /// Fenced on the same linearization anchor (`state_revision`,
    /// `step_in_flight`, and the owning `attempt_id`): only the operation
    /// whose marker is present may clear it, and only while the revision it
    /// admitted on is still current. A zero-row result (marker already
    /// cleared by the engine's `commit_transition`, or the state advanced)
    /// is a benign no-op — the newer state owns the row. The engine's
    /// terminal/wait `commit_transition` also clears `in_flight` on success;
    /// this method lets the executor clear it for standalone/sequential
    /// capability callers so the next operation can claim an empty slot.
    ///
    /// # Errors
    /// Returns [`EngineError`] only on storage failure; fence misses are
    /// returned as `Ok(())` (idempotent).
    async fn clear_prompt_attempt(
        &self,
        session_id: &SessionId,
        expected_revision: u64,
        expected_step: Option<&str>,
        attempt_id: &str,
    ) -> Result<(), EngineError>;

    /// Load all persisted child runs whose `parent_session_id` matches.
    ///
    /// Used to hydrate the engine's in-memory children map during recovery
    /// (Important 1): a restarted parent must know its child checkpoints so
    /// its next `commit_transition` submits the correct child revisions.
    ///
    /// # Errors
    /// Returns [`EngineError`] on storage failure.
    async fn load_children(
        &self,
        parent_session_id: &SessionId,
    ) -> Result<Vec<RunRecord>, EngineError>;
}
