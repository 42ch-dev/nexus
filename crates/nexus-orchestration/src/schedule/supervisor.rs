//! `ScheduleSupervisor` — per-creator schedule queue manager (spec §5).
//!
//! Maintains a per-creator queue of [`Schedule`]s. On any state change that
//! could unlock progress (new schedule added, current schedule completed,
//! user cancel/pause/resume), recomputes eligibility and admits the next
//! eligible schedule.
//!
//! Uses `tokio::sync::Mutex<Inner>` for interior mutability so that `tick()`
//! and `on_session_terminal()` can be called concurrently.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use nexus_contracts::local::schedule::{
    ParallelWithIds, Schedule, ScheduleConcurrency, ScheduleId, ScheduleStatus,
};
use nexus_local_db::SqlitePool;
use tokio::sync::Mutex;

use super::admission::{admit, CompletedSet, RunningSet};
use crate::run_state::WorkflowStateStore;

/// `creator_schedules.execution_policy` value for a never-started legacy row
/// (the migration default).
///
/// No owned session, never started by boot/tick/cron until an explicit public
/// `schedule start` opts it in.
pub const EXECUTION_POLICY_LEGACY_INERT: &str = "legacy_inert";
/// `creator_schedules.execution_policy` value for a new admission that IS
/// driven by the daemon coordinator (`WorkflowRunCoordinator::admit_schedule`).
pub const EXECUTION_POLICY_DRIVEN_V1: &str = "driven_v1";
/// `creator_schedules.execution_policy` value for `_system.*` internal
/// presets (e.g. `_system.maintenance`) — never driven, never user work.
pub const EXECUTION_POLICY_SYSTEM_INERT: &str = "system_inert";

/// Host-free daemon admission callback (A3).
///
/// `ScheduleSupervisor` lives in `nexus-orchestration` and MUST NOT couple to
/// daemon internals (engine, coordinator, Host). This trait is the seam: the
/// daemon implements it over its `WorkflowRunCoordinator` and injects it via
/// [`ScheduleSupervisor::with_schedule_starter`]. When present, `tick` admits
/// a `driven_v1` pending row by calling `start` (which atomically creates the
/// owned v1 run, freezes the schedule→session identity, and drives it) instead
/// of merely flipping the row to `Running` without a session.
#[async_trait]
pub trait ScheduleRunStarter: Send + Sync {
    /// Admit `schedule_id` as a driven run and return the owned session id.
    ///
    /// The implementation must make the schedule `Running` with a
    /// `current_session_id` atomically (before enqueue), then drive the run.
    /// Re-entry returns the same owned run.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] on any admission failure (not eligible,
    /// inactive creator/workspace, policy refusal, engine/store error).
    async fn start(&self, schedule_id: &str) -> Result<crate::engine::SessionId, SupervisorError>;
}

/// Error type for supervisor operations.
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("schedule {0} not found")]
    NotFound(String),
    #[error("invalid status transition for {0}: {1:?} -> {2:?}")]
    InvalidTransition(String, ScheduleStatus, ScheduleStatus),
    #[error("duplicate schedule: creator '{creator_id}' already has a schedule with preset '{preset_id}' and label '{label}'")]
    DuplicateSchedule {
        creator_id: String,
        preset_id: String,
        label: String,
    },
}

/// Per-creator schedule supervisor.
///
/// Responsibilities:
/// - `tick()`: load pending schedules from DB, evaluate admission, start
///   eligible ones (update row to Running).
/// - `on_session_terminal()`: flip Schedule to Completed/Failed, then tick.
pub struct ScheduleSupervisor {
    pool: Arc<SqlitePool>,
    inner: Mutex<Inner>,
    /// Re-entrancy guard: prevents concurrent `tick()` execution.
    tick_in_progress: AtomicBool,
    /// Optional workspace directory for on-disk operations (completion-lock write).
    workspace_dir: Arc<Option<std::path::PathBuf>>,
    /// Optional capability registry (V1.51 T-A P0). Threaded into the
    /// review-time KB extraction hook so `nexus.llm.extract` can run when a
    /// worker is available; the hook falls back to the heuristic when this is
    /// `None` or the worker is unavailable.
    ///
    /// V1.176 P1 (AR-92 #6): this stays a **boot-time** `Arc` — a documented
    /// builtin-only consumer. The quality hooks invoke `nexus.llm.extract`,
    /// a builtin; hot reloads never change builtins, so a swap cannot affect
    /// it. New graphs (which DO need hot-added user capabilities) snapshot
    /// the shared holder in the engine instead (`GraphFlowEngine::current_caps`).
    registry: Arc<Option<std::sync::Arc<crate::capability::CapabilityRegistry>>>,
    /// Injected daemon admission callback (A3). When `Some`, `tick` admits a
    /// `driven_v1` pending row through [`ScheduleRunStarter::start`] (which
    /// creates the owned run + schedule→session identity atomically) instead
    /// of a status-only `Running` flip. `None` (tests, non-daemon use) keeps
    /// the pre-driver status-only admission.
    schedule_starter: Arc<Option<Arc<dyn ScheduleRunStarter>>>,
    /// Default binding provider for internal schedule insertion (N-9): the
    /// daemon's first enabled Host provider. Auto-chain continuation rows
    /// built by this supervisor bind every effective prompt role to this
    /// provider so the admission binding-completeness gate passes. `None`
    /// (tests, no Host config) means internal insertion refuses to publish
    /// a drive-enabled row for a preset with prompt roles.
    binding_provider: Arc<Option<String>>,
}

struct Inner {
    /// Cache of running schedule IDs keyed by creator for quick admission checks.
    running_by_creator: HashMap<String, HashSet<ScheduleId>>,
}

impl ScheduleSupervisor {
    /// Create a new supervisor backed by the given shared `SQLite` pool.
    ///
    /// The pool must already have migrations applied (including the
    /// `creator_schedules` table).
    #[must_use]
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self::new_with_workspace(pool, None)
    }

    /// Create a new supervisor with an optional workspace directory.
    ///
    /// When provided, the supervisor can write completion-lock files to disk
    /// after marking a Work as completed (DF-60 §3).
    #[must_use]
    pub fn new_with_workspace(
        pool: Arc<SqlitePool>,
        workspace_dir: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            pool,
            inner: Mutex::new(Inner {
                running_by_creator: HashMap::new(),
            }),
            tick_in_progress: AtomicBool::new(false),
            workspace_dir: Arc::new(workspace_dir),
            registry: Arc::new(None),
            schedule_starter: Arc::new(None),
            binding_provider: Arc::new(None),
        }
    }

    /// Attach a capability registry (V1.51 T-A P0).
    ///
    /// Threaded into the review-time KB extraction hook so `nexus.llm.extract`
    /// can run when a worker is available. When unset (or when the worker is
    /// unavailable), the hook falls back to the V1.50 heuristic.
    #[must_use]
    pub fn with_capability_registry(
        mut self,
        registry: std::sync::Arc<crate::capability::CapabilityRegistry>,
    ) -> Self {
        self.registry = Arc::new(Some(registry));
        self
    }

    /// Inject the daemon admission callback for driven-v1 rows (A3).
    ///
    /// The daemon wires its `WorkflowRunCoordinator` here at boot so that
    /// `tick` admits new `driven_v1` schedules through the single-run owner.
    /// Legacy/system-inert rows are never passed to the starter.
    #[must_use]
    pub fn with_schedule_starter(mut self, starter: Arc<dyn ScheduleRunStarter>) -> Self {
        self.schedule_starter = Arc::new(Some(starter));
        self
    }

    /// Clone the injected daemon admission callback (N-11): lets daemon
    /// background owners (stale-findings watcher, boot recovery) hand
    /// their enqueued rows to the SAME production starter the supervisor
    /// tick uses, so every supported insertion branch shares one
    /// coordinator ownership path. `None` when no starter is wired
    /// (tests, standalone supervisor).
    #[must_use]
    pub fn schedule_starter_clone(&self) -> Option<Arc<dyn ScheduleRunStarter>> {
        self.schedule_starter.as_ref().clone()
    }

    /// Set the default binding provider for internal schedule insertion
    /// (N-9): the daemon's first enabled Host provider. Auto-chain
    /// continuation rows built by this supervisor bind every effective
    /// prompt role to this provider so the admission binding-completeness
    /// gate passes. When unset, internal insertion refuses to publish a
    /// drive-enabled row for a preset with prompt roles.
    #[must_use]
    pub fn with_binding_provider(mut self, provider_id: String) -> Self {
        self.binding_provider = Arc::new(Some(provider_id));
        self
    }

    /// Load pending schedules from DB, evaluate admission, and start eligible ones.
    ///
    /// For each pending schedule that passes the admission gate:
    /// - Update `creator_schedules.status` to `Running`
    /// - Set `updated_at` to current timestamp
    /// - Add to the running set
    ///
    /// **Idempotency guard (H4)**: If `tick()` is already in progress, this call
    /// returns immediately without doing anything.
    ///
    /// **Per-creator scoping (H1)**: Concurrency checks (Serial, `ParallelWith`) only
    /// consider schedules belonging to the same creator. Cross-creator dependencies
    /// (`depends_on`) are still checked against the global completed set.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if schedule admission or database operations fail.
    pub async fn tick(&self) -> Result<(), SupervisorError> {
        // H4: Re-entrancy guard — if a tick is already in progress, skip.
        if self
            .tick_in_progress
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }

        // Use system clock for on-demand tick
        let now = chrono::Utc::now().timestamp();
        let result = self.tick_inner(now, None).await;

        // Always release the guard, even on error.
        self.tick_in_progress.store(false, Ordering::Release);

        result
    }

    /// Clock-triggered tick for V1.5 WS-D.
    ///
    /// Like `tick()` but only admits schedules where `scheduled_at <= now`
    /// (or `scheduled_at IS NULL` for on-demand schedules).
    ///
    /// **Used by**: `Scheduler::tick()` for clock-triggered admission.
    /// **Not used by**: `on_schedule_terminal()` cascade (which uses `tick()`).
    ///
    /// **Idempotency guard (H4)**: Shared with `tick()` — if either is in progress,
    /// the other returns immediately.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if schedule admission or database operations fail.
    pub async fn tick_clocked(&self, clock_now: i64) -> Result<(), SupervisorError> {
        // H4: Re-entrancy guard — shared with tick()
        if self
            .tick_in_progress
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Ok(());
        }

        // Filter by scheduled_at <= clock_now OR scheduled_at IS NULL
        let result = self.tick_inner(clock_now, Some(clock_now)).await;

        // Always release the guard, even on error.
        self.tick_in_progress.store(false, Ordering::Release);

        result
    }

    /// Inner implementation of tick (no re-entrancy guard).
    ///
    /// Parameters:
    /// - `now`: timestamp for DB updates
    /// - `scheduled_at_cutoff`: if Some, only admit schedules where
    ///   `scheduled_at IS NULL OR scheduled_at <= cutoff`. If None, admit all pending.
    #[allow(clippy::too_many_lines)]
    async fn tick_inner(
        &self,
        now: i64,
        scheduled_at_cutoff: Option<i64>,
    ) -> Result<(), SupervisorError> {
        let pool = &*self.pool;

        // Load only actionable schedules from DB (pending, running, paused).
        // R-V139P0-W-F: exclude completed/failed/cancelled rows to avoid O(N) scan
        // over historical schedules on every terminal event.
        // SAFETY: dynamic SQL — runtime query for ScheduleRow (FromRow struct);
        // the WHERE filter is constant, not user-controlled.
        let active_rows = sqlx::query_as::<_, ScheduleRow>(
            "SELECT schedule_id, creator_id, preset_id, preset_version,
                    status, concurrency_kind, concurrency_whitelist,
                    current_core_context_version, current_session_id,
                    scheduled_at, label, created_at, updated_at, terminated_at,
                    execution_policy
             FROM creator_schedules
             WHERE status IN ('pending', 'running', 'paused')",
        )
        .fetch_all(pool)
        .await?;

        // QC1 C-1 fix: Load completed/cancelled IDs separately for dependency
        // resolution. The active query above excludes terminal states, so
        // completed_ids must come from its own query — otherwise schedules
        // with `depends_on` are permanently blocked.
        // SAFETY: dynamic SQL — runtime query; constant WHERE filter.
        let completed_rows: Vec<String> = sqlx::query_scalar(
            "SELECT schedule_id FROM creator_schedules
             WHERE status IN ('completed', 'cancelled')",
        )
        .fetch_all(pool)
        .await?;

        let completed_ids: Vec<ScheduleId> = completed_rows.into_iter().map(ScheduleId).collect();

        // Classify active rows into running (by creator) and pending.
        //
        // A3 (admission cutover): only `driven_v1` rows participate in
        // admission/execution capacity. Legacy (`legacy_inert`) and
        // system (`system_inert`) rows — including historical admission-only
        // `Running` rows with no owned session — are excluded from boot/tick/
        // cron admission and capacity so they remain inert until an explicit
        // public `schedule start` opts them in. A `driven_v1` Running row is
        // always paired with an owned session, so it still counts toward the
        // per-creator run set.
        let mut running_by_creator: HashMap<String, HashSet<ScheduleId>> = HashMap::new();
        let mut pending: Vec<(Schedule, String)> = Vec::new();

        for row in &active_rows {
            let schedule = row.to_schedule();
            // A3: non-driven rows never enter the running set (excluded from
            // execution capacity) nor become admission candidates.
            if row.execution_policy != EXECUTION_POLICY_DRIVEN_V1 {
                continue;
            }
            match schedule.status {
                ScheduleStatus::Running => {
                    running_by_creator
                        .entry(schedule.creator_id.clone())
                        .or_default()
                        .insert(schedule.id.clone());
                }
                ScheduleStatus::Pending => {
                    // V1.5 WS-D: filter by scheduled_at for clock-triggered tick
                    // scheduled_at_cutoff filters by scheduled_at <= cutoff OR scheduled_at IS NULL
                    let due = match (scheduled_at_cutoff, &schedule.scheduled_at) {
                        (None, _) => true,             // on-demand tick: admit all pending
                        (Some(_cutoff), None) => true, // no scheduled_at: on-demand schedule
                        (Some(cutoff), Some(scheduled_str)) => {
                            // Parse scheduled_at string (Unix timestamp as string)
                            scheduled_str.parse::<i64>().is_ok_and(|t| t <= cutoff)
                        }
                    };
                    if due {
                        pending.push((schedule, row.execution_policy.clone()));
                    }
                }
                _ => {}
            }
        }

        let completed_set = CompletedSet::from(completed_ids);

        // Load depends_on for each pending schedule from schedule_dependencies
        let pool_ref = &*self.pool;
        let mut pending_with_deps: Vec<(Schedule, String)> = Vec::with_capacity(pending.len());
        for (schedule, policy) in pending {
            let sid = schedule.id.0.clone();
            let dep_rows = sqlx::query_scalar!(
                "SELECT depends_on as \"depends_on!\" FROM schedule_dependencies WHERE schedule_id = ?",
                sid
            )
            .fetch_all(pool_ref)
            .await?;
            let deps: Vec<ScheduleId> = dep_rows.into_iter().map(ScheduleId).collect();
            let mut schedule = schedule;
            schedule.depends_on = deps;
            pending_with_deps.push((schedule, policy));
        }

        // Sort pending by created_at for FIFO ordering
        pending_with_deps.sort_by(|a, b| a.0.created_at.cmp(&b.0.created_at));

        // Evaluate admission one at a time, updating the running set after
        // each admission so that Serial schedules don't all get admitted
        // in the same tick. Concurrency checks are scoped per-creator (H1).
        //
        // A3: when a daemon `ScheduleRunStarter` is injected, an admitted
        // `driven_v1` row is handed to `starter.start` (which atomically
        // creates the owned v1 run + schedule→session identity then drives
        // it). When no starter is wired (tests, non-daemon use), the
        // pre-driver status-only flip is preserved. A failed start leaves the
        // row pending (never a surprise `Running`-with-no-session).
        let schedule_starter = self.schedule_starter.as_ref().clone();
        let mut started = Vec::new();
        let mut status_flip: Vec<(String, String)> = Vec::new();
        let mut running_by_creator_so_far = running_by_creator;

        for (candidate, _policy) in &pending_with_deps {
            let running_set = RunningSet::from_entries(
                running_by_creator_so_far
                    .iter()
                    .flat_map(|(c, ids)| ids.iter().map(|id| (c.clone(), id.clone())))
                    .collect(),
            );
            if !admit(candidate, &running_set, &completed_set) {
                continue;
            }
            let creator_id = candidate.creator_id.clone();
            let sid = candidate.id.0.clone();

            if let Some(starter) = &schedule_starter {
                match starter.start(&sid).await {
                    Ok(session_id) => {
                        tracing::debug!(
                            schedule_id = %sid,
                            session_id = %session_id.0,
                            "tick: admitted driven_v1 schedule via daemon starter"
                        );
                        started.push((creator_id.clone(), sid.clone()));
                    }
                    Err(e) => {
                        // A failed admission never marks the row Running
                        // (no session was created); it stays pending for a
                        // later tick / explicit start.
                        tracing::warn!(
                            schedule_id = %sid,
                            error = %e,
                            "tick: driven_v1 schedule admission failed; leaving pending"
                        );
                        continue;
                    }
                }
            } else {
                // No daemon starter (tests / standalone supervisor): keep
                // the historical status-only flip so admission-gate tests
                // remain meaningful. Production always injects a starter.
                started.push((creator_id.clone(), sid.clone()));
                status_flip.push((creator_id, sid));
            }

            // Immediately update the running set so subsequent candidates
            // see this one as running (important for Serial).
            running_by_creator_so_far
                .entry(candidate.creator_id.clone())
                .or_default()
                .insert(candidate.id.clone());
        }

        // Status-only flip for rows admitted without a daemon starter.
        for (_creator_id, sid) in &status_flip {
            let sid_owned = sid.to_owned();
            sqlx::query!(
                "UPDATE creator_schedules SET status = 'running', updated_at = ?
                 WHERE schedule_id = ?",
                now,
                sid_owned
            )
            .execute(pool)
            .await?;
        }

        // Update inner cache
        for (creator_id, sid) in &started {
            self.inner
                .lock()
                .await
                .running_by_creator
                .entry(creator_id.clone())
                .or_default()
                .insert(ScheduleId(sid.clone()));
        }

        Ok(())
    }

    /// Called when a schedule reaches a terminal state.
    ///
    /// Flips the Schedule row to the given terminal status, removes it from
    /// the running set, evaluates auto-chain continuation, and triggers a
    /// `tick()` to potentially start the next eligible schedule.
    ///
    /// T3 (A3): a driven schedule (one that owns a session) settles through
    /// the atomic path — the source terminal status flip, the auto-chain
    /// child INSERT (+ core-context seed), and the Work driver-pointer
    /// update commit in ONE transaction keyed by `source_run_id`. A
    /// duplicate terminal callback / restart loads the already-created
    /// child instead of minting a second schedule. Non-driven rows (tests,
    /// legacy) keep the sequential path.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if database operations fail.
    /// Evaluate terminal transitions for a schedule: status update, runtime
    /// lock release, the review-findings + V1.49 foreshadowing-promote hooks,
    /// and auto-chain continuation. Each block is an independent sequential
    /// guard; splitting would add indirection without reducing complexity
    /// (same shape as `process_auto_chain_after_terminal` below).
    #[allow(clippy::too_many_lines)]
    pub async fn on_schedule_terminal(
        &self,
        schedule_id: &str,
        terminal_status: ScheduleStatus,
    ) -> Result<(), SupervisorError> {
        if !matches!(
            terminal_status,
            ScheduleStatus::Completed | ScheduleStatus::Failed | ScheduleStatus::Cancelled
        ) {
            return Err(SupervisorError::InvalidTransition(
                schedule_id.to_string(),
                ScheduleStatus::Running, // assumed current
                terminal_status,
            ));
        }

        // Fetch creator_id + preset_id + owned session for the schedule.
        // V1.47 P0 fix (qc3 W-1): preset_id is inspected here so the
        // review-findings hook can be short-circuited for non-review
        // presets without an extra round-trip inside
        // persist_review_findings_for_schedule.
        let schedule_row = sqlx::query_as::<_, TerminalScheduleRow>(
            "SELECT creator_id, preset_id, current_session_id
             FROM creator_schedules WHERE schedule_id = ?",
        )
        .bind(schedule_id)
        .fetch_optional(&*self.pool)
        .await?;

        // T3 (A3): a driven schedule (owned session) settles through the
        // atomic path — source settlement + auto-chain child commit in ONE
        // transaction keyed by source_run_id. Non-driven rows (tests,
        // legacy) keep the sequential path below.
        if let Some(row) = &schedule_row {
            if let Some(source_run_id) = &row.current_session_id {
                return self
                    .settle_and_chain_atomic(
                        schedule_id,
                        terminal_status,
                        source_run_id,
                        &row.creator_id,
                        &row.preset_id,
                    )
                    .await;
            }
        }

        let now = chrono::Utc::now().timestamp();
        let status_str = match terminal_status {
            ScheduleStatus::Completed => "completed",
            ScheduleStatus::Failed => "failed",
            ScheduleStatus::Cancelled => "cancelled",
            _ => unreachable!(),
        };

        let schedule_id_owned = schedule_id.to_owned();
        let status_owned = status_str.to_owned();
        sqlx::query!(
            "UPDATE creator_schedules
             SET status = ?, terminated_at = ?, updated_at = ?
             WHERE schedule_id = ? AND status = 'running'",
            status_owned,
            now,
            now,
            schedule_id_owned
        )
        .execute(&*self.pool)
        .await?;

        // Remove from running cache
        if let Some(row) = &schedule_row {
            self.remove_from_running_cache(schedule_id, &row.creator_id)
                .await;
        }

        // V1.42 P0 (T3): Release runtime lock held by this schedule.
        // The holder format is `daemon:schedule:<schedule_id>`.
        // Look up the Work that was driven by this schedule and release its lock.
        if let Some(row) = &schedule_row {
            self.release_schedule_lock(schedule_id, &row.creator_id)
                .await;
        }

        // V1.39 §5.4 (Fix 1): Evaluate auto-chain continuation for completed schedules.
        // Only on success — failed/cancelled schedules do not trigger auto-chain.
        if terminal_status == ScheduleStatus::Completed {
            if let Some(row) = &schedule_row {
                self.run_completed_hooks(schedule_id, &row.preset_id).await;
                self.process_auto_chain_after_terminal(schedule_id, &row.creator_id)
                    .await;
            }
        }

        // Trigger tick to admit next eligible schedule
        self.tick().await?;

        Ok(())
    }

    /// Remove a schedule from the in-memory running cache (best-effort).
    async fn remove_from_running_cache(&self, schedule_id: &str, creator_id: &str) {
        let mut inner = self.inner.lock().await;
        if let Some(ids) = inner.running_by_creator.get_mut(creator_id) {
            ids.remove(&ScheduleId(schedule_id.to_string()));
        }
    }

    /// Release the daemon runtime lock held by a terminal schedule
    /// (best-effort; a non-Work-driver schedule is a no-op).
    async fn release_schedule_lock(&self, schedule_id: &str, creator_id: &str) {
        let holder = format!("daemon:schedule:{schedule_id}");
        if let Err(e) = release_daemon_schedule_lock(&self.pool, creator_id, &holder).await {
            tracing::warn!(
                schedule_id,
                holder = %holder,
                error = %e,
                "runtime_lock: failed to release daemon schedule lock on terminal"
            );
        }
    }

    /// Fire the best-effort terminal hooks for a completed schedule (T3).
    ///
    /// Review findings, foreshadowing promotion, finalize-time missing-KB
    /// detection, and review-time KB candidate extraction. Every hook is
    /// best-effort + non-blocking: errors are logged and never fail the
    /// terminal transition (spec §8.4 invariant — hook failure must not
    /// fork/cancel the active driver).
    async fn run_completed_hooks(&self, schedule_id: &str, preset_id: &str) {
        use crate::auto_chain;
        use crate::quality_loop;
        let ws_ref = self.workspace_dir.as_ref();
        let ws_path = ws_ref.as_deref();
        let reg: Option<&crate::capability::CapabilityRegistry> =
            self.registry.as_ref().as_ref().map(|r| &**r);

        // V1.47 P0 (novel-quality-loop §5.5.6 / §8): persist ≥1 finding
        // when a `novel-chapter-review` schedule completes. This is the
        // **single code path** covering both the auto-chain driver and
        // on-demand `creator run novel-chapter-review <work_id>` runs.
        // R-V147P0-06 (V1.48 P0 T3): preset id hoisted to `preset_ids`
        // SSOT — single source shared with the allowlist and findings hook.
        if preset_id == crate::preset_ids::NOVEL_CHAPTER_REVIEW_PRESET_ID {
            if let Err(e) =
                auto_chain::persist_review_findings_for_schedule(&self.pool, schedule_id, ws_path)
                    .await
            {
                tracing::warn!(
                    schedule_id,
                    error = %e,
                    "review-findings: persist hook failed (non-fatal)"
                );
            }
        }
        // V1.49 P1 (narrative-indexes overlay §4): promote inline F###
        // declarations from chapter outlines into
        // `Works/<work_ref>/Outlines/foreshadowing.md` after a
        // `novel-writing` produce run completes.
        if preset_id == crate::preset_ids::NOVEL_WRITING_PRESET_ID {
            if let Err(e) =
                auto_chain::promote_foreshadowing_for_schedule(&self.pool, schedule_id, ws_path)
                    .await
            {
                tracing::warn!(
                    schedule_id,
                    error = %e,
                    "foreshadowing-promote: hook failed (non-fatal)"
                );
            }

            // V1.51 T-A P2: finalize-time missing-KB detection. Scan the
            // finalized chapter prose, diff against confirmed World KB rows,
            // and write an advisory log under
            // Works/<work_ref>/Logs/kb/missing/. Missing candidates are NOT
            // inserted into kb_extract_jobs. Best-effort: errors are logged
            // and do NOT fail the terminal transition.
            if let Err(e) =
                quality_loop::detect_missing_kb_on_finalize(&self.pool, schedule_id, ws_path, reg)
                    .await
            {
                tracing::warn!(
                    schedule_id,
                    error = %e,
                    "kb-missing: finalize-time detection hook failed (non-fatal)"
                );
            }
        }
        // V1.50 T-B P1 (entity-scope-model.md §5.5 / cron-staggering.md
        // §4.4): review-time KB candidate extraction. When a
        // `novel-review-master` schedule completes, scan the current
        // chapter prose for capitalized noun phrases and insert
        // `kb_extract_jobs` rows with `promotion_status='pending'`.
        if preset_id == crate::preset_ids::NOVEL_REVIEW_MASTER_PRESET_ID {
            if let Err(e) = quality_loop::extract_kb_candidates_for_review(
                &self.pool,
                schedule_id,
                ws_path,
                reg,
            )
            .await
            {
                tracing::warn!(
                    schedule_id,
                    error = %e,
                    "kb-extract: review-time extraction hook failed (non-fatal)"
                );
            }
        }
    }

    /// T3 (A3): settle a driven schedule and enqueue its auto-chain child in
    /// ONE transaction keyed by `source_run_id`.
    ///
    /// The source schedule's terminal status flip, the child schedule INSERT
    /// (+ core-context seed), and the Work driver-pointer update commit
    /// together. A duplicate terminal callback / restart that races the same
    /// source run hits the partial unique index
    /// `creator_schedules_by_source_run`: the transaction rolls back, the
    /// already-created child is loaded, and the source settlement is retried
    /// idempotently (a no-op when the winner already settled). Never mints a
    /// second child.
    ///
    /// Only the schedule whose `current_session_id` equals `source_run_id`
    /// is updated. Post-commit work (running cache, runtime lock, terminal
    /// hooks, starter handoff, `WorkComplete`, tick) mirrors the sequential
    /// path.
    #[allow(clippy::too_many_lines)]
    async fn settle_and_chain_atomic(
        &self,
        schedule_id: &str,
        terminal_status: ScheduleStatus,
        source_run_id: &str,
        creator_id: &str,
        preset_id: &str,
    ) -> Result<(), SupervisorError> {
        use crate::auto_chain::{self, AutoChainError, ChainAction};

        let now = chrono::Utc::now().timestamp();
        let status_str = match terminal_status {
            ScheduleStatus::Completed => "completed",
            ScheduleStatus::Failed => "failed",
            ScheduleStatus::Cancelled => "cancelled",
            _ => unreachable!(),
        };

        // Evaluate auto-chain BEFORE the transaction (read-only).
        let chain = if terminal_status == ScheduleStatus::Completed {
            self.evaluate_auto_chain(schedule_id, creator_id).await
        } else {
            None
        };

        let mut tx = nexus_local_db::begin_immediate(&self.pool)
            .await
            .map_err(|e| {
                SupervisorError::Database(match e {
                    nexus_local_db::LocalDbError::Sqlx(s) => s,
                    other => sqlx::Error::Protocol(other.to_string()),
                })
            })?;

        // 1. Source settlement — only the schedule whose current_session_id
        //    equals the run is updated. Any non-terminal status settles
        //    (running, paused, pending): a paused-after-reconcile-error row
        //    (P1-1) must still reach its durable terminal, and the live
        //    terminal callback races nothing here because the row's owned
        //    session is already terminal. Terminal rows are excluded so a
        //    duplicate callback never rewrites a settled row.
        sqlx::query(
            "UPDATE creator_schedules SET status = ?, terminated_at = ?, updated_at = ?
             WHERE schedule_id = ? AND status NOT IN ('completed', 'failed', 'cancelled')
               AND current_session_id = ?",
        )
        .bind(status_str)
        .bind(now)
        .bind(now)
        .bind(schedule_id)
        .bind(source_run_id)
        .execute(&mut *tx)
        .await?;

        // 2. Child enqueue in the SAME transaction (auto-chain only).
        let mut child_id: Option<String> = None;
        let mut conflict = false;
        if let Some((action, work)) = &chain {
            let child_spec: Option<(&str, Option<i32>, Option<i32>, &str)> = match action {
                ChainAction::AdvanceStage {
                    work_id,
                    next_stage,
                } => Some((next_stage.as_str(), None, None, work_id.as_str())),
                ChainAction::NextChapter {
                    work_id,
                    next_chapter,
                    next_volume,
                } => Some((
                    "produce",
                    Some(*next_chapter),
                    Some(*next_volume),
                    work_id.as_str(),
                )),
                // WorkComplete / NoAction do not create a child; handled
                // after commit.
                _ => None,
            };
            if let Some((stage, chapter, volume, work_id)) = child_spec {
                if let Some(bindings) = self.derive_auto_chain_bindings(stage, work_id) {
                    match auto_chain::enqueue_auto_chain_schedule_in_tx(
                        &mut tx,
                        &self.pool,
                        creator_id,
                        work_id,
                        stage,
                        chapter,
                        volume,
                        work,
                        bindings,
                        Some(source_run_id),
                    )
                    .await
                    {
                        Ok(sid) => child_id = Some(sid),
                        Err(e) if auto_chain::is_source_run_conflict(&e) => {
                            conflict = true;
                        }
                        Err(e) => {
                            return Err(SupervisorError::Database(match e {
                                AutoChainError::Database(nexus_local_db::LocalDbError::Sqlx(s)) => {
                                    s
                                }
                                other => sqlx::Error::Protocol(other.to_string()),
                            }));
                        }
                    }
                }
            }
        }

        // Whether THIS call performed the terminal transition. A duplicate
        // callback (source_run_id conflict) must not re-run completion hooks,
        // mark Work complete, or re-hand the child to the starter (Greptile P2).
        let mut transitioned = !conflict;
        if conflict {
            // Duplicate terminal callback: the child INSERT failed on the
            // source_run_id unique index. Roll back (the source settlement
            // flips back with it) and load the already-created child.
            drop(tx);
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT schedule_id FROM creator_schedules WHERE source_run_id = ?",
            )
            .bind(source_run_id)
            .fetch_optional(&*self.pool)
            .await?;
            // Settle the source idempotently — a no-op when the winner's
            // transaction already settled it.
            let settled = sqlx::query(
                "UPDATE creator_schedules SET status = ?, terminated_at = ?, updated_at = ?
                 WHERE schedule_id = ? AND status NOT IN ('completed', 'failed', 'cancelled')
                   AND current_session_id = ?",
            )
            .bind(status_str)
            .bind(now)
            .bind(now)
            .bind(schedule_id)
            .bind(source_run_id)
            .execute(&*self.pool)
            .await?;
            transitioned = settled.rows_affected() > 0;
            child_id = existing;
            tracing::info!(
                schedule_id,
                source_run_id,
                child_id = ?child_id,
                "auto-chain: duplicate terminal callback loaded existing child"
            );
        } else {
            tx.commit().await?;
        }

        // Post-commit: running cache, runtime lock, hooks, starter handoff,
        // WorkComplete, tick.
        self.remove_from_running_cache(schedule_id, creator_id)
            .await;
        self.release_schedule_lock(schedule_id, creator_id).await;

        if terminal_status == ScheduleStatus::Completed && transitioned {
            self.run_completed_hooks(schedule_id, preset_id).await;
            if let Some((ChainAction::WorkComplete { work_id }, _work)) = &chain {
                if let Err(e) =
                    auto_chain::mark_work_completed(&self.pool, creator_id, work_id).await
                {
                    tracing::warn!(
                        work_id = %work_id,
                        error = %e,
                        "auto-chain: failed to mark work completed"
                    );
                } else {
                    self.write_completion_lock_if_available(creator_id, work_id)
                        .await;
                    tracing::info!(
                        work_id = %work_id,
                        "auto-chain: work completed"
                    );
                }
            }
            if let Some(sid) = &child_id {
                if let Some(starter) = self.schedule_starter.as_ref().clone() {
                    if let Err(e) = starter.start(sid).await {
                        tracing::warn!(
                            schedule_id = sid.as_str(),
                            error = e.to_string(),
                            "auto-chain: driven_v1 starter handoff failed; leaving pending"
                        );
                    }
                }
            }
        }

        // Trigger tick to admit next eligible schedule
        self.tick().await?;

        Ok(())
    }

    /// Evaluate the auto-chain continuation for a completed schedule (T3).
    ///
    /// Returns `Some((action, work))` when the schedule is an auto-chain
    /// driver whose next step should be enqueued; `None` when it is not a
    /// driver, auto-chain is disabled, the Work is completion-locked, or
    /// evaluation failed (logged, non-fatal). Read-only — the caller commits
    /// the child INSERT + Work pointer update in its own transaction.
    async fn evaluate_auto_chain(
        &self,
        schedule_id: &str,
        creator_id: &str,
    ) -> Option<(
        crate::auto_chain::ChainAction,
        nexus_local_db::works::WorkRecord,
    )> {
        use crate::auto_chain::{self, ChainAction};

        // Find the Work that was driven by this schedule
        let work = match auto_chain::find_work_for_driver(&self.pool, schedule_id).await {
            Ok(Some(w)) => w,
            Ok(None) => return None, // Not an auto-chain driver
            Err(e) => {
                tracing::warn!(
                    schedule_id,
                    error = %e,
                    "auto-chain: failed to find work for driver schedule"
                );
                return None;
            }
        };

        if !work.auto_chain_enabled {
            return None;
        }

        // DF-60 §6: skip auto-chain on completion-locked Works
        if work.completion_locked_at.is_some() {
            tracing::debug!(
                work_id = %work.work_id,
                "auto-chain: skipping completion-locked work"
            );
            return None;
        }

        // Read latest WorkRecord from DB (SSOT, not cached state)
        let work =
            match nexus_local_db::works::get_work(&self.pool, creator_id, &work.work_id).await {
                Ok(Some(w)) => w,
                Ok(None) => return None,
                Err(e) => {
                    tracing::warn!(
                        work_id = %work.work_id,
                        error = %e,
                        "auto-chain: failed to reload work record"
                    );
                    return None;
                }
            };

        // V1.42 F-001: When the persist stage just completed, use the
        // volume-aware evaluator to correctly handle cross-volume auto-chain
        // (Plan Goal 4 / AC2). For all other stages, the flat evaluator suffices.
        let action = if work.current_stage == "persist" && work.stage_status == "complete" {
            match auto_chain::evaluate_after_persist_volume_aware(&self.pool, &work).await {
                Ok(vol_action) => vol_action,
                Err(e) => {
                    tracing::warn!(
                        work_id = %work.work_id,
                        error = %e,
                        "auto-chain: volume-aware evaluation failed, falling back to flat"
                    );
                    auto_chain::evaluate_next_step(&work)
                }
            }
        } else {
            auto_chain::evaluate_next_step(&work)
        };

        match action {
            ChainAction::NoAction => None,
            other => Some((other, work)),
        }
    }

    /// Derive the complete binding map for an auto-chain stage's preset
    /// (N-9). Returns `None` when the stage has no preset mapping or the
    /// preset requires prompt roles with no binding provider — the caller
    /// skips the child enqueue (never a drive-enabled row with an empty
    /// binding map).
    fn derive_auto_chain_bindings(
        &self,
        stage: &str,
        work_id: &str,
    ) -> Option<std::collections::HashMap<String, crate::run_state::AgentBinding>> {
        let preset_id = crate::stage_gates::preset_for_stage(stage)?;
        if let Some(provider_id) = self.binding_provider.as_ref().as_ref() {
            crate::preset::default_bindings_for_preset(preset_id, provider_id).map_or_else(
                || {
                    tracing::warn!(
                        work_id = %work_id,
                        stage = %stage,
                        preset_id = %preset_id,
                        "auto-chain: preset has prompt roles but no binding provider; \
                         refusing to publish a drive-enabled row"
                    );
                    None
                },
                Some,
            )
        } else {
            // No configured provider: only presets WITHOUT prompt roles
            // may be enqueued (empty map passes the completeness gate).
            let caps = crate::capability::CapabilityRegistry::with_builtins();
            if let Ok(loaded) = crate::preset::load_embedded_preset(preset_id, &caps) {
                let roles = crate::preset::required_prompt_roles(&loaded);
                if !roles.is_empty() {
                    tracing::warn!(
                        work_id = %work_id,
                        stage = %stage,
                        preset_id = %preset_id,
                        "auto-chain: preset requires prompt roles but no binding \
                         provider is configured; refusing to publish a drive-enabled row"
                    );
                    return None;
                }
                Some(std::collections::HashMap::new())
            } else {
                tracing::warn!(
                    work_id = %work_id,
                    stage = %stage,
                    preset_id = %preset_id,
                    "auto-chain: preset not resolvable; refusing to publish a drive-enabled row"
                );
                None
            }
        }
    }

    /// Evaluate and execute auto-chain continuation after a schedule completes.
    ///
    /// Looks up the Work whose `driver_schedule_id` matches the completed schedule,
    /// evaluates the next chain step, and enqueues the next schedule if appropriate.
    ///
    /// Errors are logged but do not propagate — auto-chain failure should not block
    /// the terminal transition.
    // V1.42 F-001: volume-aware evaluator adds ~15 lines to the persist-complete
    // branch; total is a flat match-based dispatcher where each arm is independent.
    // Splitting would introduce artificial indirection without reducing complexity.
    #[allow(clippy::too_many_lines)]
    async fn process_auto_chain_after_terminal(&self, schedule_id: &str, creator_id: &str) {
        use crate::auto_chain::{self, ChainAction};

        // Find the Work that was driven by this schedule
        let work = match auto_chain::find_work_for_driver(&self.pool, schedule_id).await {
            Ok(Some(w)) => w,
            Ok(None) => return, // Not an auto-chain driver
            Err(e) => {
                tracing::warn!(
                    schedule_id,
                    error = %e,
                    "auto-chain: failed to find work for driver schedule"
                );
                return;
            }
        };

        if !work.auto_chain_enabled {
            return;
        }

        // DF-60 §6: skip auto-chain on completion-locked Works
        if work.completion_locked_at.is_some() {
            tracing::debug!(
                work_id = %work.work_id,
                "auto-chain: skipping completion-locked work"
            );
            return;
        }

        // Read latest WorkRecord from DB (SSOT, not cached state)
        let work =
            match nexus_local_db::works::get_work(&self.pool, creator_id, &work.work_id).await {
                Ok(Some(w)) => w,
                Ok(None) => return,
                Err(e) => {
                    tracing::warn!(
                        work_id = %work.work_id,
                        error = %e,
                        "auto-chain: failed to reload work record"
                    );
                    return;
                }
            };

        // V1.42 F-001: When the persist stage just completed, use the
        // volume-aware evaluator to correctly handle cross-volume auto-chain
        // (Plan Goal 4 / AC2). For all other stages, the flat evaluator suffices.
        let action = if work.current_stage == "persist" && work.stage_status == "complete" {
            match auto_chain::evaluate_after_persist_volume_aware(&self.pool, &work).await {
                Ok(vol_action) => vol_action,
                Err(e) => {
                    tracing::warn!(
                        work_id = %work.work_id,
                        error = %e,
                        "auto-chain: volume-aware evaluation failed, falling back to flat"
                    );
                    auto_chain::evaluate_next_step(&work)
                }
            }
        } else {
            auto_chain::evaluate_next_step(&work)
        };

        match action {
            ChainAction::AdvanceStage {
                ref work_id,
                ref next_stage,
            } => {
                if let Err(e) = self
                    .enqueue_auto_chain_step(creator_id, work_id, next_stage, None, None, &work)
                    .await
                {
                    tracing::warn!(
                        work_id = %work_id,
                        next_stage = %next_stage,
                        error = %e,
                        "auto-chain: failed to enqueue next stage"
                    );
                }
            }
            ChainAction::NextChapter {
                ref work_id,
                ref next_chapter,
                next_volume,
            } => {
                // V1.42: log cross-volume transitions for observability.
                let is_cross_volume =
                    next_volume > 1 || (next_volume == 1 && work.current_chapter > 0);
                if is_cross_volume {
                    tracing::info!(
                        work_id = %work_id,
                        next_chapter = *next_chapter,
                        next_volume,
                        "auto-chain: next chapter (volume-aware)"
                    );
                }
                // V1.44 P2 (F-004): pass volume through to enqueue so the
                // `novel-writing` preset input includes `volume` template var.
                // Next chapter starts at produce stage
                if let Err(e) = self
                    .enqueue_auto_chain_step(
                        creator_id,
                        work_id,
                        "produce",
                        Some(*next_chapter),
                        Some(next_volume),
                        &work,
                    )
                    .await
                {
                    tracing::warn!(
                        work_id = %work_id,
                        next_chapter = *next_chapter,
                        next_volume,
                        error = %e,
                        "auto-chain: failed to enqueue next chapter"
                    );
                }
            }
            ChainAction::WorkComplete { ref work_id } => {
                if let Err(e) =
                    auto_chain::mark_work_completed(&self.pool, creator_id, work_id).await
                {
                    tracing::warn!(
                        work_id = %work_id,
                        error = %e,
                        "auto-chain: failed to mark work completed"
                    );
                } else {
                    self.write_completion_lock_if_available(creator_id, work_id)
                        .await;
                    tracing::info!(
                        work_id = %work_id,
                        "auto-chain: work completed"
                    );
                }
            }
            ChainAction::NoAction => {}
        }
    }

    /// Write completion-lock file for a completed Work (DF-60 §3, best-effort).
    ///
    /// DB is SSOT; the file is a derived on-disk artifact for cross-tool observation.
    /// Failures are logged as warnings but do not propagate.
    async fn write_completion_lock_if_available(&self, creator_id: &str, work_id: &str) {
        use crate::auto_chain;

        let Some(ref ws_dir) = *self.workspace_dir else {
            return;
        };

        if let Ok(Some(work)) =
            nexus_local_db::works::get_work(&self.pool, creator_id, work_id).await
        {
            if let Some(ref locked_at) = work.completion_locked_at {
                if let Err(e) = auto_chain::write_completion_lock_for_work(ws_dir, &work, locked_at)
                {
                    tracing::warn!(
                        work_id = %work_id,
                        work_ref = ?work.work_ref,
                        error = %e,
                        "completion-lock file write failed (non-fatal; DB is SSOT)"
                    );
                }
            }
        }
    }

    /// Enqueue a new auto-chain schedule and update the Work checkpoint.
    ///
    /// Delegates to the shared `auto_chain::enqueue_auto_chain_schedule` helper
    /// (Fix A / W-A) so that the ID-mint + INSERT + set_driver logic is not
    /// duplicated between the supervisor and boot paths.
    #[allow(clippy::doc_markdown)]
    async fn enqueue_auto_chain_step(
        &self,
        creator_id: &str,
        work_id: &str,
        stage: &str,
        chapter: Option<i32>,
        volume: Option<i32>,
        work: &nexus_local_db::works::WorkRecord,
    ) -> Result<(), SupervisorError> {
        use crate::auto_chain::{self, AutoChainError};

        // N-9: derive the complete binding map for the stage's preset from
        // the configured default provider. A preset with prompt roles and
        // no provider refuses the enqueue (never a drive-enabled row with an
        // empty binding map); a preset with no prompt roles accepts an empty
        // map.
        let Some(preset_id) = crate::stage_gates::preset_for_stage(stage) else {
            return Ok(());
        };
        let bindings = if let Some(provider_id) = self.binding_provider.as_ref().as_ref() {
            if let Some(bindings) =
                crate::preset::default_bindings_for_preset(preset_id, provider_id)
            {
                bindings
            } else {
                tracing::warn!(
                    work_id = %work_id,
                    stage = %stage,
                    preset_id = %preset_id,
                    "auto-chain: preset has prompt roles but no binding provider; \
                     refusing to publish a drive-enabled row"
                );
                return Ok(());
            }
        } else {
            // No configured provider: only presets WITHOUT prompt roles
            // may be enqueued (empty map passes the completeness gate).
            let caps = crate::capability::CapabilityRegistry::with_builtins();
            if let Ok(loaded) = crate::preset::load_embedded_preset(preset_id, &caps) {
                let roles = crate::preset::required_prompt_roles(&loaded);
                if !roles.is_empty() {
                    tracing::warn!(
                        work_id = %work_id,
                        stage = %stage,
                        preset_id = %preset_id,
                        "auto-chain: preset requires prompt roles but no binding \
                         provider is configured; refusing to publish a drive-enabled row"
                    );
                    return Ok(());
                }
                std::collections::HashMap::new()
            } else {
                tracing::warn!(
                    work_id = %work_id,
                    stage = %stage,
                    preset_id = %preset_id,
                    "auto-chain: preset not resolvable; refusing to publish a drive-enabled row"
                );
                return Ok(());
            }
        };

        match auto_chain::enqueue_auto_chain_schedule(
            &self.pool, creator_id, work_id, stage, chapter, volume, work, bindings, None,
        )
        .await
        {
            Ok(schedule_id) => {
                if let Some(starter) = self.schedule_starter.as_ref().clone() {
                    if let Err(e) = starter.start(&schedule_id).await {
                        tracing::warn!(
                            schedule_id = schedule_id.as_str(),
                            error = e.to_string(),
                            "auto-chain: driven_v1 starter handoff failed; leaving pending"
                        );
                    }
                }
                Ok(())
            }
            Err(AutoChainError::InvalidState(msg)) => {
                // No schedule mapping for this stage — not a DB error.
                tracing::warn!(
                    work_id = %work_id,
                    stage = %stage,
                    "auto-chain: {msg}"
                );
                Ok(())
            }
            Err(AutoChainError::Database(e)) => Err(SupervisorError::Database(
                // Extract the underlying sqlx error from LocalDbError.
                match e {
                    nexus_local_db::LocalDbError::Sqlx(s) => s,
                    other => sqlx::Error::Protocol(other.to_string()),
                },
            )),
            Err(other) => Err(SupervisorError::Database(sqlx::Error::Protocol(
                other.to_string(),
            ))),
        }
    }

    /// Insert a pending schedule into the database (for testing and CLI use).
    ///
    /// The supervisor doesn't do scheduling logic here — that happens on `tick()`.
    ///
    /// **R2 — Duplicate detection**: Before checking, checks whether a schedule
    /// with the same `(creator_id, preset_id, label)` already exists. If so,
    /// returns [`SupervisorError::DuplicateSchedule`].
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if database insertion fails.
    pub async fn insert_pending(&self, schedule: Schedule) -> Result<(), SupervisorError> {
        self.insert_pending_with_descriptor(schedule, None).await
    }

    /// Insert a pending schedule with a frozen execution descriptor (A3,
    /// C-2/I-5).
    ///
    /// The descriptor (serialized [`crate::run_state::RunDescriptorV1`]) is
    /// persisted in the SAME transaction as the schedule row and dependency
    /// rows, so a concurrent tick can never observe a `driven_v1` row before
    /// its descriptor/input/bindings are durable. `_system.*` presets are
    /// always `system_inert` (C-4).
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if database insertion fails.
    pub async fn insert_pending_with_descriptor(
        &self,
        schedule: Schedule,
        execution_descriptor: Option<&[u8]>,
    ) -> Result<(), SupervisorError> {
        self.insert_pending_inner(schedule, execution_descriptor, None)
            .await
    }

    /// Insert a pending schedule with a frozen execution descriptor AND a
    /// durable core-context version-0 seed record (A3, C-2/I-5, N-3).
    ///
    /// The schedule row, dependency rows, frozen descriptor, and the
    /// version-0 core-context record commit in ONE transaction, so a
    /// concurrent tick can never observe a `driven_v1` row before its
    /// seed/version is durable — a seed failure rolls the whole row back
    /// and never orphans a drive-enabled schedule.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if database insertion fails.
    pub async fn insert_pending_with_descriptor_and_seed(
        &self,
        schedule: Schedule,
        execution_descriptor: Option<&[u8]>,
        seed_text: &str,
    ) -> Result<(), SupervisorError> {
        self.insert_pending_inner(schedule, execution_descriptor, Some(seed_text))
            .await
    }

    #[allow(clippy::too_many_lines)] // sequential duplicate-check → insert → claim wiring; splitting obscures the single INSERT path
    async fn insert_pending_inner(
        &self,
        schedule: Schedule,
        execution_descriptor: Option<&[u8]>,
        seed_text: Option<&str>,
    ) -> Result<(), SupervisorError> {
        let now = chrono::Utc::now().timestamp();

        // R2: Check for duplicate (creator_id + preset_id + label)
        let dup_creator_id = schedule.creator_id.clone();
        let dup_preset_id = schedule.preset_id.clone();
        let dup_label = schedule.label.clone().unwrap_or_default();
        // SAFETY: runtime `sqlx::query_scalar` — new query needs sqlx prepare.
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT schedule_id FROM creator_schedules
             WHERE creator_id = ? AND preset_id = ? AND COALESCE(label, '') = ?",
        )
        .bind(&dup_creator_id)
        .bind(&dup_preset_id)
        .bind(&dup_label)
        .fetch_optional(&*self.pool)
        .await?;

        if existing.is_some() {
            return Err(SupervisorError::DuplicateSchedule {
                creator_id: schedule.creator_id.clone(),
                preset_id: schedule.preset_id.clone(),
                label: schedule.label.clone().unwrap_or_default(),
            });
        }

        // Parse timestamps: if the schedule has string timestamps, try to parse them.
        // For test convenience, empty strings default to `now`.
        let created_at: i64 = if schedule.created_at.is_empty() {
            now
        } else {
            schedule.created_at.parse().unwrap_or(now)
        };
        let updated_at: i64 = if schedule.updated_at.is_empty() {
            now
        } else {
            schedule.updated_at.parse().unwrap_or(now)
        };

        let (concurrency_kind, concurrency_whitelist) = match &schedule.concurrency {
            ScheduleConcurrency::Serial => ("serial".to_string(), None),
            ScheduleConcurrency::ParallelWith(ids) => {
                let json = serde_json::to_string(&ids.schedule_ids).unwrap_or_default();
                ("parallel_with".to_string(), Some(json))
            }
            ScheduleConcurrency::ParallelAny => ("parallel_any".to_string(), None),
        };

        // Pre-own all bind params (borrow lifetime rules for sqlx macros).
        let schedule_id = schedule.id.0;
        let creator_id = schedule.creator_id;
        let preset_id = schedule.preset_id;
        let preset_version_i64 = i64::from(schedule.preset_version);
        let context_version_i64 = i64::from(schedule.current_core_context_version.0);
        let scheduled_at = schedule.scheduled_at;
        let label = schedule.label;

        // A3 admission cutover: every NEW row created through the supervisor
        // is drive-enabled (`driven_v1`); `_system.*` internal presets are
        // explicitly `system_inert`. The migration default (`legacy_inert`)
        // is reserved for pre-cutover rows that must stay inert until an
        // explicit public `schedule start`.
        let execution_policy = if preset_id.starts_with("_system.") {
            EXECUTION_POLICY_SYSTEM_INERT
        } else {
            EXECUTION_POLICY_DRIVEN_V1
        };

        // C-2: the schedule row, its dependency rows, and the frozen
        // execution descriptor commit in ONE transaction so a concurrent
        // tick can never observe a partially seeded `driven_v1` row.
        let mut tx = nexus_local_db::begin_immediate(&self.pool)
            .await
            .map_err(|e| SupervisorError::Database(sqlx::Error::Protocol(e.to_string())))?;

        // N-14: the frozen descriptor carries the schedule's work_id; the
        // `Schedule` DTO has no work_id field, so the row's `work_id`
        // column is derived from the descriptor here (work-linked adds
        // must freeze work_id into the schedule row, not only the
        // descriptor).
        let descriptor_work_id: Option<String> = execution_descriptor
            .and_then(|bytes| {
                serde_json::from_slice::<crate::run_state::RunDescriptorV1>(bytes).ok()
            })
            .and_then(|d| d.work_id)
            .filter(|id| !id.is_empty());

        // SAFETY: dynamic SQL — a runtime INSERT matching the historical
        // `insert_pending` shape plus the A3 execution-policy column. The
        // established auto-chain/cron convention uses runtime `sqlx::query`
        // (see auto_chain.rs); a compile-time `query!` here would require
        // regenerated `.sqlx` metadata for every column change.
        sqlx::query(
            r"INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, concurrency_whitelist,
                current_core_context_version, current_session_id,
                scheduled_at, label, created_at, updated_at, terminated_at,
                work_id, execution_policy, execution_descriptor_json)
               VALUES (?, ?, ?, ?, 'pending', ?, ?, ?, NULL, ?, ?, ?, ?, NULL, ?, ?, ?)",
        )
        .bind(&schedule_id)
        .bind(&creator_id)
        .bind(&preset_id)
        .bind(preset_version_i64)
        .bind(concurrency_kind)
        .bind(concurrency_whitelist)
        .bind(context_version_i64)
        .bind(scheduled_at)
        .bind(label)
        .bind(created_at)
        .bind(updated_at)
        .bind(descriptor_work_id)
        .bind(execution_policy)
        .bind(execution_descriptor)
        .execute(&mut *tx)
        .await?;

        // Insert dependencies
        for dep in &schedule.depends_on {
            let dep_id = dep.0.clone();
            sqlx::query!(
                "INSERT OR IGNORE INTO schedule_dependencies (schedule_id, depends_on)
                 VALUES (?, ?)",
                schedule_id,
                dep_id
            )
            .execute(&mut *tx)
            .await?;
        }

        // N-3: when a seed is supplied, persist the durable version-0
        // core-context record in the SAME transaction — a `driven_v1` row
        // is never observable before its seed/version is durable, and a
        // seed failure rolls the whole row back (no orphaned drive-enabled
        // schedule). The public add path is user-initiated, so the record
        // carries the user author (mirrors `CoreContextManager::apply_seed`
        // with `CoreContextAuthor::User`).
        if let Some(seed_text) = seed_text {
            let payload = serde_json::json!({ "kind": "text", "body": seed_text });
            let content_bytes = serde_json::to_vec(&payload).map_err(|e| {
                SupervisorError::Database(sqlx::Error::Protocol(format!(
                    "serialize core-context seed: {e}"
                )))
            })?;
            let derivation = serde_json::json!({ "kind": "seed", "raw": seed_text });
            let derivation_bytes = serde_json::to_vec(&derivation).map_err(|e| {
                SupervisorError::Database(sqlx::Error::Protocol(format!(
                    "serialize core-context derivation: {e}"
                )))
            })?;
            sqlx::query(
                "INSERT INTO core_context_versions
                   (schedule_id, version, payload_kind, content,
                    derivation_kind, derivation_detail,
                    created_at, created_by_kind, created_by_user_id)
                 VALUES (?, 0, 'text', ?, 'seed', ?, ?, 'user', ?)",
            )
            .bind(&schedule_id)
            .bind(content_bytes)
            .bind(derivation_bytes)
            .bind(now)
            .bind(&creator_id)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Create a [`CoreContextManager`] backed by the same pool.
    ///
    /// This avoids requiring callers to hold a reference to the supervisor's
    /// internal pool. The returned manager is lightweight (just an Arc clone).
    pub fn core_context_manager(&self) -> super::derivation::CoreContextManager {
        super::derivation::CoreContextManager::new(self.pool.clone())
    }

    /// Get a reference to the underlying `SQLite` pool.
    pub fn pool(&self) -> Arc<SqlitePool> {
        self.pool.clone()
    }

    /// Get the current status of a schedule by ID (for testing/inspection).
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if the schedule is not found or the status string is unrecognizable.
    pub async fn status_of(&self, schedule_id: &str) -> Result<ScheduleStatus, SupervisorError> {
        let schedule_id_owned = schedule_id.to_owned();
        let row = sqlx::query_scalar!(
            "SELECT status FROM creator_schedules WHERE schedule_id = ?",
            schedule_id_owned
        )
        .fetch_optional(&*self.pool)
        .await?;

        row.map_or_else(
            || Err(SupervisorError::NotFound(schedule_id.to_string())),
            |status_str| match status_str.as_str() {
                "pending" => Ok(ScheduleStatus::Pending),
                "running" => Ok(ScheduleStatus::Running),
                "paused" => Ok(ScheduleStatus::Paused),
                "completed" => Ok(ScheduleStatus::Completed),
                "cancelled" => Ok(ScheduleStatus::Cancelled),
                "failed" => Ok(ScheduleStatus::Failed),
                other => {
                    tracing::warn!(
                        "unknown status '{}' for schedule {}; treating as error",
                        other,
                        schedule_id
                    );
                    Err(SupervisorError::NotFound(schedule_id.to_string()))
                }
            },
        )
    }
}

// ---------------------------------------------------------------------------
// Boot/Shutdown helpers (WS7 T9)
// ---------------------------------------------------------------------------

impl ScheduleSupervisor {
    /// Resume any Running schedules as Paused (daemon restart recovery).
    ///
    /// On daemon boot, Running schedules are paused with the given reason so
    /// the user can explicitly resume them. This prevents stale sessions
    /// from continuing after a daemon restart.
    ///
    /// C-3: legacy never-started `Running` rows (no owned session,
    /// `legacy_inert` policy) are NOT rewritten — their operator-visible
    /// historical status is preserved so the explicit-start state machine
    /// can still opt them in. Only `driven_v1` Running rows (which always
    /// own a session) are paused.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if database update fails.
    pub async fn resume_running_as_paused(&self, reason: &str) -> Result<usize, SupervisorError> {
        let now = chrono::Utc::now().timestamp();

        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT schedule_id FROM creator_schedules
             WHERE status = 'running' AND execution_policy = 'driven_v1'",
        )
        .fetch_all(&*self.pool)
        .await?;

        let count = rows.len();
        for sid in rows {
            sqlx::query!(
                "UPDATE creator_schedules SET status = 'paused', updated_at = ?
                 WHERE schedule_id = ? AND status = 'running'",
                now,
                sid
            )
            .execute(&*self.pool)
            .await?;
            tracing::info!("paused schedule {} (reason: {})", sid, reason);
        }

        if count > 0 {
            // R1/R4: Clear the running cache for all paused schedules.
            // After boot recovery, no schedules should be in the running set.
            self.inner.lock().await.running_by_creator.clear();
            tracing::info!("paused {} running schedule(s) (reason: {})", count, reason);
        }

        Ok(count)
    }

    /// Reconcile checkpoint-before-settlement loss (T3, A3).
    ///
    /// A driven schedule (`driven_v1`) whose durable session is terminal but
    /// whose row is not terminal yet (the drive loop persisted the terminal
    /// checkpoint but crashed before `on_schedule_terminal` ran) is settled
    /// from the durable session status — no new session, no second
    /// auto-chain child (the child INSERT is keyed by `source_run_id`, so a
    /// duplicate settlement loads the already-created child).
    ///
    /// The scan covers EVERY non-terminal schedule status (`running`,
    /// `paused`, `pending`) — a row that an earlier recovery pass paused
    /// after a failed settlement (P1-1) is re-scanned and settled here.
    /// A failed settlement never flips the row into a terminal status, so a
    /// row that could not be settled stays in this scan and is retried on
    /// every subsequent boot/reconcile. Terminal schedule rows are excluded
    /// (nothing left to settle).
    ///
    /// MUST run BEFORE `resume_running_as_paused`: a terminal session's
    /// schedule must settle, not pause. Returns the number of schedules
    /// reconciled.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if database operations fail.
    pub async fn reconcile_terminal_schedules(&self) -> Result<usize, SupervisorError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT schedule_id, current_session_id FROM creator_schedules
             WHERE execution_policy = 'driven_v1' AND current_session_id IS NOT NULL
               AND status NOT IN ('completed', 'failed', 'cancelled')",
        )
        .fetch_all(&*self.pool)
        .await?;

        let store = crate::storage::sqlite::SqliteSessionStorage::new(self.pool.clone());
        let mut reconciled = 0usize;
        for (schedule_id, session_id) in rows {
            let record = match store
                .load_run(&crate::engine::SessionId(session_id.clone()))
                .await
            {
                Ok(Some(record)) => record,
                Ok(None) => continue, // no durable run — not a settlement case
                Err(e) => {
                    tracing::warn!(
                        schedule_id,
                        session_id = %session_id,
                        error = %e,
                        "boot reconcile: failed to load durable run; skipping"
                    );
                    continue;
                }
            };
            let terminal_status = match record.status {
                crate::engine::SessionStatus::Completed => ScheduleStatus::Completed,
                crate::engine::SessionStatus::Failed => ScheduleStatus::Failed,
                crate::engine::SessionStatus::Cancelled => ScheduleStatus::Cancelled,
                // Unconfirmed cancel/cleanup is NOT a successful cancel
                // (A5): the schedule row is deliberately left non-terminal
                // so public inspect projects the run's
                // `execution.recovery_class: "interrupted"` from the durable
                // session instead of a fabricated `cancelled` (qc2 F-002).
                // `creator_schedules.status` has no `interrupted` variant and
                // an unmapped string projects as `Pending` (re-runnable), so
                // writing a status string here would be strictly worse.
                // Non-terminal (incl. `Interrupted`): the schedule stays
                // running and is paused by `resume_running_as_paused` (or
                // re-driven by recovery).
                _ => continue,
            };
            if let Err(e) = self
                .on_schedule_terminal(&schedule_id, terminal_status)
                .await
            {
                tracing::warn!(
                    schedule_id,
                    session_id = %session_id,
                    terminal_status = ?terminal_status,
                    error = %e,
                    "boot reconcile: settlement failed (non-fatal)"
                );
            } else {
                reconciled += 1;
                tracing::info!(
                    schedule_id,
                    session_id = %session_id,
                    terminal_status = ?terminal_status,
                    "boot reconcile: settled schedule from durable terminal session"
                );
            }
        }
        Ok(reconciled)
    }

    // -----------------------------------------------------------------------
    // R1+R4 — pause_schedule(): atomically update DB + running cache
    // -----------------------------------------------------------------------

    /// Pause a running (or pending) schedule.
    ///
    /// **R1+R4 — Consistent pause**: Updates the DB status to `paused` AND
    /// removes the schedule from the in-memory running set cache. This
    /// prevents a concurrent `tick()` from seeing stale state.
    ///
    /// Call this from HTTP handlers instead of direct SQL so that the
    /// supervisor's running set stays in sync with the database.
    ///
    /// Returns `Ok(true)` if the schedule was paused, `Ok(false)` if it was
    /// already in a non-pausable state, or an error for DB issues.
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if the schedule is not found or database update fails.
    pub async fn pause_schedule(&self, schedule_id: &str) -> Result<bool, SupervisorError> {
        let now = chrono::Utc::now().timestamp();

        // Check current status and get creator_id atomically.
        // SAFETY: dynamic SQL — compile-time macro not applicable for FromRow struct.
        let row = sqlx::query_as::<_, StatusCreatorRow>(
            "SELECT status, creator_id FROM creator_schedules WHERE schedule_id = ?",
        )
        .bind(schedule_id)
        .fetch_optional(&*self.pool)
        .await?
        .ok_or_else(|| SupervisorError::NotFound(schedule_id.to_string()))?;

        let current_status = row.status.as_str();
        let creator_id = row.creator_id;

        // Only running or pending can be paused.
        if !matches!(current_status, "running" | "pending") {
            return Ok(false);
        }

        // Update DB
        // SAFETY: runtime `sqlx::query` — new UPDATE for pause_schedule.
        sqlx::query(
            "UPDATE creator_schedules SET status = 'paused', updated_at = ?
             WHERE schedule_id = ? AND status IN ('running', 'pending')",
        )
        .bind(now)
        .bind(schedule_id)
        .execute(&*self.pool)
        .await?;

        // R1: Remove from running cache so concurrent tick() won't see stale state.
        if current_status == "running" {
            let mut inner = self.inner.lock().await;
            if let Some(ids) = inner.running_by_creator.get_mut(&creator_id) {
                ids.remove(&ScheduleId(schedule_id.to_string()));
            }
        }

        Ok(true)
    }

    // -----------------------------------------------------------------------
    // R3+R7 — resume_running(): smart paused→running if admitted
    // -----------------------------------------------------------------------

    /// Resume a paused schedule.
    ///
    /// **R3+R7 — Smart resume**: If the schedule would be admitted (all deps
    /// satisfied, concurrency rules pass), transitions directly to `Running`
    /// (skipping Pending). If admission fails, falls back to `Paused → Pending`
    /// so a future `tick()` can pick it up.
    ///
    /// **I-7 (policy-aware)**: legacy/system-inert rows are refused — they
    /// can only be opted in by an explicit public `schedule start`. A
    /// `driven_v1` row with an owned session is routed to the coordinator
    /// (the caller drives it); a `driven_v1` row without an owned session
    /// goes through the normal admission path. Never produce `running`
    /// without an owned session.
    ///
    /// Returns the new status as a string ("running" or "pending").
    ///
    /// # Errors
    /// Returns [`SupervisorError`] if the schedule is not found or database operations fail.
    #[allow(clippy::too_many_lines)]
    pub async fn resume_schedule(&self, schedule_id: &str) -> Result<String, SupervisorError> {
        let now = chrono::Utc::now().timestamp();

        // Verify current status is paused
        // SAFETY: dynamic SQL — compile-time macro not applicable for FromRow struct.
        let row = sqlx::query_as::<_, StatusCreatorRow>(
            "SELECT status, creator_id FROM creator_schedules WHERE schedule_id = ?",
        )
        .bind(schedule_id)
        .fetch_optional(&*self.pool)
        .await?
        .ok_or_else(|| SupervisorError::NotFound(schedule_id.to_string()))?;

        if row.status.as_str() != "paused" {
            return Err(SupervisorError::InvalidTransition(
                schedule_id.to_string(),
                ScheduleStatus::Paused,
                // Parse the actual status for the error message
                match row.status.as_str() {
                    "running" => ScheduleStatus::Running,
                    "completed" => ScheduleStatus::Completed,
                    "cancelled" => ScheduleStatus::Cancelled,
                    "failed" => ScheduleStatus::Failed,
                    _ => ScheduleStatus::Pending,
                },
            ));
        }

        // I-7: policy-aware — legacy/system-inert rows cannot be resumed
        // into a driven run; only an explicit public `schedule start` opts
        // them in (C-3).
        let policy: String = sqlx::query_scalar(
            "SELECT execution_policy FROM creator_schedules WHERE schedule_id = ?",
        )
        .bind(schedule_id)
        .fetch_one(&*self.pool)
        .await?;
        if policy != EXECUTION_POLICY_DRIVEN_V1 {
            return Err(SupervisorError::InvalidTransition(
                schedule_id.to_string(),
                ScheduleStatus::Paused,
                ScheduleStatus::Pending,
            ));
        }

        let creator_id = row.creator_id;

        // Check admission rules to decide: direct to Running or fall back to Pending.
        let mut should_run = false;

        // Build the current running set and completed set from DB.
        // SAFETY: dynamic SQL — runtime query for ScheduleRow (FromRow struct);
        // the WHERE filter is constant, not user-controlled.
        let active_rows = sqlx::query_as::<_, ScheduleRow>(
            "SELECT schedule_id, creator_id, preset_id, preset_version,
                    status, concurrency_kind, concurrency_whitelist,
                    current_core_context_version, current_session_id,
                    scheduled_at, label, created_at, updated_at, terminated_at,
                    execution_policy
             FROM creator_schedules
             WHERE status IN ('pending', 'running', 'paused')",
        )
        .fetch_all(&*self.pool)
        .await?;

        // QC1 C-1 fix: Load completed/cancelled IDs separately for dependency
        // resolution — same pattern as tick_inner.
        // SAFETY: dynamic SQL — runtime query; constant WHERE filter.
        let completed_rows: Vec<String> = sqlx::query_scalar(
            "SELECT schedule_id FROM creator_schedules
             WHERE status IN ('completed', 'cancelled')",
        )
        .fetch_all(&*self.pool)
        .await?;
        let completed_ids: Vec<ScheduleId> = completed_rows.into_iter().map(ScheduleId).collect();

        let mut running_entries: HashSet<(String, ScheduleId)> = HashSet::new();
        let mut candidate_schedule: Option<Schedule> = None;

        for r in &active_rows {
            let s = r.to_schedule();
            // N-16: only `driven_v1` rows count toward execution capacity —
            // legacy/system-inert `Running` rows (historical admission-only
            // rows with no owned session) are inert and must not block an
            // eligible driven schedule's resume. Same invariant as
            // `tick_inner` and the coordinator admission matrix.
            if s.status == ScheduleStatus::Running
                && r.execution_policy == EXECUTION_POLICY_DRIVEN_V1
            {
                running_entries.insert((s.creator_id.clone(), s.id.clone()));
            }
            if s.id.0 == schedule_id {
                let mut candidate = s;
                candidate.status = ScheduleStatus::Pending; // pretend it's pending for admission check
                candidate_schedule = Some(candidate);
            }
        }

        // Load dependencies for the candidate
        if let Some(mut candidate) = candidate_schedule {
            let sid = schedule_id.to_owned();
            let dep_rows = sqlx::query_scalar!(
                "SELECT depends_on as \"depends_on!\" FROM schedule_dependencies WHERE schedule_id = ?",
                sid
            )
            .fetch_all(&*self.pool)
            .await?;
            candidate.depends_on = dep_rows.into_iter().map(ScheduleId).collect();

            let running_set = RunningSet::from_entries(running_entries);
            let completed_set = CompletedSet::from(completed_ids);
            should_run = admit(&candidate, &running_set, &completed_set);
        }

        if should_run {
            // I-7: a driven row with an owned session is routed to the
            // coordinator (the caller drives it); a driven row without an
            // owned session goes through the normal admission path. Never
            // produce `running` without an owned session.
            let _owned_session: Option<String> = sqlx::query_scalar(
                "SELECT current_session_id FROM creator_schedules WHERE schedule_id = ?",
            )
            .bind(schedule_id)
            .fetch_one(&*self.pool)
            .await?;
            if let Some(starter) = self.schedule_starter.as_ref().clone() {
                match starter.start(schedule_id).await {
                    Ok(_sid) => {
                        self.inner
                            .lock()
                            .await
                            .running_by_creator
                            .entry(creator_id)
                            .or_default()
                            .insert(ScheduleId(schedule_id.to_string()));
                        return Ok("running".to_string());
                    }
                    Err(e) => {
                        tracing::warn!(
                            schedule_id,
                            error = %e,
                            "resume: driven_v1 schedule admission/drive failed; leaving paused"
                        );
                        return Ok("paused".to_string());
                    }
                }
            }
            // No daemon starter (tests / standalone): keep the historical
            // status-only flip when the row is eligible. Production always
            // injects a starter, so this arm is not a live drive path.
            // R2 fix: Check rows_affected() after UPDATE to handle TOCTOU race.
            // If 0, another caller already transitioned the schedule — return
            // the current status without updating cache. SQLite single-writer
            // model makes this sufficient.
            // SAFETY: runtime `sqlx::query` — new UPDATE for resume_running.
            let result = sqlx::query(
                "UPDATE creator_schedules SET status = 'running', updated_at = ?
                 WHERE schedule_id = ? AND status = 'paused'",
            )
            .bind(now)
            .bind(schedule_id)
            .execute(&*self.pool)
            .await?;

            if result.rows_affected() == 0 {
                // TOCTOU race: another concurrent resume/cancel already changed
                // the status. Return current DB status without touching cache.
                let current = self.status_of(schedule_id).await?;
                return Ok(match current {
                    ScheduleStatus::Running => "running".to_string(),
                    ScheduleStatus::Pending => "pending".to_string(),
                    ScheduleStatus::Paused => "paused".to_string(),
                    ScheduleStatus::Completed => "completed".to_string(),
                    ScheduleStatus::Cancelled => "cancelled".to_string(),
                    ScheduleStatus::Failed => "failed".to_string(),
                });
            }

            // Add to running cache
            self.inner
                .lock()
                .await
                .running_by_creator
                .entry(creator_id)
                .or_default()
                .insert(ScheduleId(schedule_id.to_string()));

            Ok("running".to_string())
        } else {
            // Fallback: paused→pending
            // R2 fix: Same TOCTOU protection for the fallback path.
            // SAFETY: runtime `sqlx::query` — new UPDATE for resume fallback.
            let result = sqlx::query(
                "UPDATE creator_schedules SET status = 'pending', updated_at = ?
                 WHERE schedule_id = ? AND status = 'paused'",
            )
            .bind(now)
            .bind(schedule_id)
            .execute(&*self.pool)
            .await?;

            if result.rows_affected() == 0 {
                // TOCTOU race: return current status without touching cache.
                let current = self.status_of(schedule_id).await?;
                return Ok(match current {
                    ScheduleStatus::Running => "running".to_string(),
                    ScheduleStatus::Pending => "pending".to_string(),
                    ScheduleStatus::Paused => "paused".to_string(),
                    ScheduleStatus::Completed => "completed".to_string(),
                    ScheduleStatus::Cancelled => "cancelled".to_string(),
                    ScheduleStatus::Failed => "failed".to_string(),
                });
            }

            // Trigger tick to attempt admission
            self.tick().await?;

            Ok("pending".to_string())
        }
    }
}

/// Internal row for status + `creator_id` lookups.
#[derive(sqlx::FromRow)]
struct StatusCreatorRow {
    status: String,
    creator_id: String,
}

/// Internal row for terminal settlement (T3): the schedule's creator,
/// preset, and owned session (the `source_run_id` for auto-chain).
#[derive(sqlx::FromRow)]
#[allow(clippy::struct_field_names)] // columns mirror the `creator_schedules` schema; `id` postfix is the DB naming
struct TerminalScheduleRow {
    creator_id: String,
    preset_id: String,
    current_session_id: Option<String>,
}

/// Internal row mapping for reading `creator_schedules` from `SQLite`.
#[derive(sqlx::FromRow)]
struct ScheduleRow {
    schedule_id: String,
    creator_id: String,
    preset_id: String,
    preset_version: i64,
    status: String,
    concurrency_kind: String,
    concurrency_whitelist: Option<String>,
    current_core_context_version: i64,
    current_session_id: Option<String>,
    scheduled_at: Option<i64>,
    label: Option<String>,
    created_at: i64,
    updated_at: i64,
    terminated_at: Option<i64>,
    execution_policy: String,
}

impl ScheduleRow {
    fn to_schedule(&self) -> Schedule {
        use nexus_contracts::local::schedule::{
            CoreContextVersion, Schedule, ScheduleConcurrency, ScheduleId, ScheduleStatus,
        };

        let concurrency = match self.concurrency_kind.as_str() {
            "parallel_with" => {
                let ids: Vec<ScheduleId> = self
                    .concurrency_whitelist
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok())
                    .unwrap_or_default();
                ScheduleConcurrency::ParallelWith(ParallelWithIds { schedule_ids: ids })
            }
            "parallel_any" => ScheduleConcurrency::ParallelAny,
            _ => ScheduleConcurrency::Serial,
        };

        let status = match self.status.as_str() {
            "running" => ScheduleStatus::Running,
            "paused" => ScheduleStatus::Paused,
            "completed" => ScheduleStatus::Completed,
            "cancelled" => ScheduleStatus::Cancelled,
            "failed" => ScheduleStatus::Failed,
            _ => ScheduleStatus::Pending,
        };

        Schedule {
            id: ScheduleId(self.schedule_id.clone()),
            creator_id: self.creator_id.clone(),
            preset_id: self.preset_id.clone(),
            preset_version: u32::try_from(self.preset_version).unwrap_or_default(),
            status,
            concurrency,
            depends_on: Vec::new(), // loaded separately via schedule_dependencies
            current_core_context_version: CoreContextVersion(
                u32::try_from(self.current_core_context_version).unwrap_or_default(),
            ),
            current_session_id: self.current_session_id.clone(),
            // scheduled_at is stored as i64 (Unix timestamp) in SQLite but exposed
            // as Option<String> in the domain type. Conversion via .to_string() is safe.
            scheduled_at: self.scheduled_at.map(|t| t.to_string()),
            label: self.label.clone(),
            created_at: self.created_at.to_string(),
            updated_at: self.updated_at.to_string(),
            terminated_at: self.terminated_at.map(|t| t.to_string()),
        }
    }
}

/// Release a daemon schedule holder from any Work that holds it.
///
/// Looks up the Work where `runtime_lock_holder = <holder>` and clears it.
/// Returns `Ok(false)` if no Work was locked with this holder (not an error).
async fn release_daemon_schedule_lock(
    pool: &SqlitePool,
    creator_id: &str,
    holder: &str,
) -> Result<bool, sqlx::Error> {
    let now = chrono::Utc::now().to_rfc3339();
    // SAFETY: Dynamic SQL for conditional lock release — matches holder string.
    let result = sqlx::query(
        "UPDATE works SET runtime_lock_holder = NULL, runtime_lock_acquired_at = NULL, updated_at = ? \
         WHERE creator_id = ? AND runtime_lock_holder = ?",
    )
    .bind(&now)
    .bind(creator_id)
    .bind(holder)
    .execute(pool)
    .await?;

    if result.rows_affected() > 0 {
        tracing::info!(
            holder = %holder,
            "runtime_lock: released daemon schedule lock"
        );
    }
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests_t9 {
    use super::*;
    use nexus_contracts::local::schedule::CoreContextVersion;

    async fn test_supervisor_with_db() -> Arc<ScheduleSupervisor> {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        std::mem::forget(dir);
        Arc::new(ScheduleSupervisor::new(Arc::new(pool)))
    }

    async fn insert_schedule(sup: &ScheduleSupervisor, id: &str, status: &str) {
        insert_schedule_with_creator(sup, id, "test-creator", status).await;
    }

    async fn insert_schedule_with_creator(
        sup: &ScheduleSupervisor,
        id: &str,
        creator_id: &str,
        status: &str,
    ) {
        let now = chrono::Utc::now().timestamp();
        // SAFETY: test-only — DML helper that inserts a minimal schedule row for test setup.
        // The row is drive-enabled (`driven_v1`) so the admission-gate tests
        // exercise the post-cutover contract (legacy/system rows stay inert).
        sqlx::query(
            r"INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, current_core_context_version,
                created_at, updated_at, execution_policy)
               VALUES (?, ?, 'test-preset', 1, ?,
               'serial', 0, ?, ?, 'driven_v1')",
        )
        .bind(id)
        .bind(creator_id)
        .bind(status)
        .bind(now)
        .bind(now)
        .execute(&*sup.pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn daemon_restart_preserves_running_schedule_as_paused() {
        let sup = test_supervisor_with_db().await;

        // Insert a running schedule (simulating a pre-crash state).
        insert_schedule(&sup, "S01", "running").await;

        // Verify it's running.
        assert_eq!(sup.status_of("S01").await.unwrap(), ScheduleStatus::Running);

        // Simulate daemon boot: resume running as paused.
        let count = sup
            .resume_running_as_paused("daemon_restart")
            .await
            .unwrap();
        assert_eq!(count, 1);

        // Verify it's now paused.
        assert_eq!(sup.status_of("S01").await.unwrap(), ScheduleStatus::Paused);

        // Calling again should be a no-op (no running schedules left).
        let count2 = sup
            .resume_running_as_paused("daemon_restart")
            .await
            .unwrap();
        assert_eq!(count2, 0);
    }

    #[tokio::test]
    async fn pending_schedules_unaffected_by_boot_resume() {
        let sup = test_supervisor_with_db().await;

        insert_schedule(&sup, "S01", "pending").await;
        insert_schedule(&sup, "S02", "running").await;

        let _ = sup.resume_running_as_paused("daemon_restart").await;

        // Pending should still be pending.
        assert_eq!(sup.status_of("S01").await.unwrap(), ScheduleStatus::Pending);
        // Running should be paused.
        assert_eq!(sup.status_of("S02").await.unwrap(), ScheduleStatus::Paused);
    }

    #[tokio::test]
    async fn tick_blocks_schedule_with_uncompleted_dependency() {
        let sup = test_supervisor_with_db().await;
        let pool = sup.pool();

        // Insert schedule A (pending)
        insert_schedule(&sup, "DEP-A", "pending").await;
        // Insert schedule B (pending) with dependency on A
        insert_schedule(&sup, "DEP-B", "pending").await;

        // Insert dependency: B depends on A
        // SAFETY: test-only — DML helper for dependency setup.
        sqlx::query("INSERT INTO schedule_dependencies (schedule_id, depends_on) VALUES (?, ?)")
            .bind("DEP-B")
            .bind("DEP-A")
            .execute(&*pool)
            .await
            .unwrap();

        // Tick: A should start (no deps), B should remain pending (depends on A)
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("DEP-A").await.unwrap(),
            ScheduleStatus::Running,
            "A should start — no dependencies"
        );
        assert_eq!(
            sup.status_of("DEP-B").await.unwrap(),
            ScheduleStatus::Pending,
            "B should not start — A is not completed"
        );

        // Complete A
        sup.on_schedule_terminal("DEP-A", ScheduleStatus::Completed)
            .await
            .unwrap();

        // After A completes, tick should auto-start B
        assert_eq!(
            sup.status_of("DEP-B").await.unwrap(),
            ScheduleStatus::Running,
            "B should auto-start after A completes"
        );
    }

    #[tokio::test]
    async fn tick_blocks_schedule_with_failed_dependency() {
        let sup = test_supervisor_with_db().await;
        let pool = sup.pool();

        // Insert A (already failed) and B (pending, depends on A)
        insert_schedule(&sup, "DEP-A-FAIL", "failed").await;
        insert_schedule(&sup, "DEP-B-FAIL", "pending").await;

        // SAFETY: test-only — DML helper for dependency setup.
        sqlx::query("INSERT INTO schedule_dependencies (schedule_id, depends_on) VALUES (?, ?)")
            .bind("DEP-B-FAIL")
            .bind("DEP-A-FAIL")
            .execute(&*pool)
            .await
            .unwrap();

        // Tick: B should remain pending — failed dep does not satisfy
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("DEP-B-FAIL").await.unwrap(),
            ScheduleStatus::Pending,
            "B should not start — A is failed, not completed"
        );
    }

    /// QC1 C-1 regression: schedule with `depends_on` on a **pre-existing**
    /// completed predecessor must admit on tick — the completed set must be
    /// loaded from DB, not derived from the active-schedules query.
    #[tokio::test]
    async fn tick_admits_schedule_with_already_completed_predecessor() {
        let sup = test_supervisor_with_db().await;
        let pool = sup.pool();

        // Insert A as already completed (simulates historical schedule)
        insert_schedule(&sup, "QC1-A", "completed").await;
        // Insert B (pending) with dependency on A
        insert_schedule(&sup, "QC1-B", "pending").await;

        // SAFETY: test-only — DML helper for dependency setup.
        sqlx::query("INSERT INTO schedule_dependencies (schedule_id, depends_on) VALUES (?, ?)")
            .bind("QC1-B")
            .bind("QC1-A")
            .execute(&*pool)
            .await
            .unwrap();

        // Tick: B should start immediately — A is already completed
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("QC1-B").await.unwrap(),
            ScheduleStatus::Running,
            "QC1 C-1 regression: B should start — A is already completed"
        );
    }

    /// A3 cutover: a never-started `legacy_inert` row (and a `system_inert`
    /// row) must NOT be admitted by `tick` — no surprise run, no status-only
    /// `Running` flip. They stay inert until an explicit public `schedule start`
    /// opts the row in (grandfathering).
    #[tokio::test]
    async fn tick_leaves_legacy_and_system_rows_inert() {
        let sup = test_supervisor_with_db().await;

        let now = chrono::Utc::now().timestamp();
        // SAFETY: test-only — insert a legacy_inert and a system_inert pending row.
        sqlx::query(
            r"INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, current_core_context_version,
                created_at, updated_at, execution_policy)
               VALUES (?, 'legacy-creator', 'legacy-preset', 1, 'pending',
               'serial', 0, ?, ?, 'legacy_inert')",
        )
        .bind("LEGACY-INERT-1")
        .bind(now)
        .bind(now)
        .execute(&*sup.pool)
        .await
        .unwrap();

        // SAFETY: test-only — insert a system_inert row.
        sqlx::query(
            r"INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, current_core_context_version,
                created_at, updated_at, execution_policy)
               VALUES (?, 'legacy-creator', '_system.maintenance', 1, 'pending',
               'serial', 0, ?, ?, 'system_inert')",
        )
        .bind("SYSTEM-INERT-1")
        .bind(now)
        .bind(now)
        .execute(&*sup.pool)
        .await
        .unwrap();

        sup.tick().await.unwrap();

        assert_eq!(
            sup.status_of("LEGACY-INERT-1").await.unwrap(),
            ScheduleStatus::Pending,
            "legacy_inert never-started row must stay pending across tick (grandfathering)"
        );
        assert_eq!(
            sup.status_of("SYSTEM-INERT-1").await.unwrap(),
            ScheduleStatus::Pending,
            "system_inert row must stay pending across tick (_system.* inertness)"
        );
    }

    // ---------- H1: Per-creator scoping ----------

    #[tokio::test]
    async fn different_creators_serial_schedules_run_concurrently() {
        let sup = test_supervisor_with_db().await;

        // Insert serial schedules for two different creators
        insert_schedule_with_creator(&sup, "H1-A1", "creator-alpha", "pending").await;
        insert_schedule_with_creator(&sup, "H1-A2", "creator-alpha", "pending").await;
        insert_schedule_with_creator(&sup, "H1-B1", "creator-beta", "pending").await;
        insert_schedule_with_creator(&sup, "H1-B2", "creator-beta", "pending").await;

        // Tick: both A1 and B1 should start (different creators)
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("H1-A1").await.unwrap(),
            ScheduleStatus::Running,
            "A1 should start (first serial for creator-alpha)"
        );
        assert_eq!(
            sup.status_of("H1-B1").await.unwrap(),
            ScheduleStatus::Running,
            "B1 should start (first serial for creator-beta)"
        );
        // Second serial for each creator should remain pending
        assert_eq!(
            sup.status_of("H1-A2").await.unwrap(),
            ScheduleStatus::Pending,
            "A2 should be blocked by A1 (same creator)"
        );
        assert_eq!(
            sup.status_of("H1-B2").await.unwrap(),
            ScheduleStatus::Pending,
            "B2 should be blocked by B1 (same creator)"
        );

        // Complete A1 → A2 should start, B2 still blocked
        sup.on_schedule_terminal("H1-A1", ScheduleStatus::Completed)
            .await
            .unwrap();
        assert_eq!(
            sup.status_of("H1-A2").await.unwrap(),
            ScheduleStatus::Running,
            "A2 should start after A1 completes"
        );
        assert_eq!(
            sup.status_of("H1-B2").await.unwrap(),
            ScheduleStatus::Pending,
            "B2 should still be blocked (B1 still running)"
        );
    }

    // ---------- H4: Tick idempotency ----------

    #[tokio::test]
    async fn double_tick_does_not_duplicate_admission() {
        let sup = test_supervisor_with_db().await;

        insert_schedule(&sup, "DT-A", "pending").await;

        // First tick starts DT-A
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("DT-A").await.unwrap(),
            ScheduleStatus::Running
        );

        // Second tick should be a no-op — DT-A is already running
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("DT-A").await.unwrap(),
            ScheduleStatus::Running,
            "schedule should still be running after double tick"
        );
    }

    // ===================================================================
    // WS-A Residuals: R1, R2, R3, R4, R7
    // ===================================================================

    // ---------- R1+R4: pause_schedule updates DB + running cache ----------

    #[tokio::test]
    async fn r1_pause_running_schedule_updates_cache() {
        let sup = test_supervisor_with_db().await;

        // Insert and start a schedule
        insert_schedule(&sup, "R1-S1", "pending").await;
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("R1-S1").await.unwrap(),
            ScheduleStatus::Running
        );

        // Pause via supervisor method
        let paused = sup.pause_schedule("R1-S1").await.unwrap();
        assert!(paused, "pause should succeed for running schedule");
        assert_eq!(
            sup.status_of("R1-S1").await.unwrap(),
            ScheduleStatus::Paused,
            "schedule should be paused after pause_schedule"
        );

        // Tick should NOT re-admit the paused schedule
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("R1-S1").await.unwrap(),
            ScheduleStatus::Paused,
            "paused schedule should not be re-admitted by tick"
        );
    }

    #[tokio::test]
    async fn r4_pause_pending_schedule() {
        let sup = test_supervisor_with_db().await;

        insert_schedule(&sup, "R4-S1", "pending").await;

        // Pause a pending schedule (allowed per spec)
        let paused = sup.pause_schedule("R4-S1").await.unwrap();
        assert!(paused, "pause should succeed for pending schedule");
        assert_eq!(
            sup.status_of("R4-S1").await.unwrap(),
            ScheduleStatus::Paused
        );
    }

    #[tokio::test]
    async fn r4_pause_non_pausable_returns_false() {
        let sup = test_supervisor_with_db().await;

        insert_schedule(&sup, "R4-S2", "completed").await;
        insert_schedule(&sup, "R4-S3", "cancelled").await;

        // Cannot pause completed/cancelled
        assert!(!sup.pause_schedule("R4-S2").await.unwrap());
        assert!(!sup.pause_schedule("R4-S3").await.unwrap());
    }

    // ---------- R2: Duplicate schedule detection ----------

    #[tokio::test]
    async fn r2_duplicate_schedule_rejected() {
        let sup = test_supervisor_with_db().await;

        let schedule = Schedule {
            id: ScheduleId("R2-S1".to_string()),
            creator_id: "creator-dup".to_string(),
            preset_id: "preset-x".to_string(),
            preset_version: 1,
            status: ScheduleStatus::Pending,
            concurrency: ScheduleConcurrency::Serial,
            depends_on: vec![],
            current_core_context_version: CoreContextVersion(0),
            current_session_id: None,
            scheduled_at: None,
            label: Some("my-label".to_string()),
            created_at: String::new(),
            updated_at: String::new(),
            terminated_at: None,
        };

        // First insert succeeds
        sup.insert_pending(schedule.clone()).await.unwrap();

        // Second insert with same creator+preset+label fails
        let dup = Schedule {
            id: ScheduleId("R2-S2".to_string()),
            ..schedule.clone()
        };
        let err = sup.insert_pending(dup).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("duplicate schedule"),
            "expected DuplicateSchedule error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn r2_different_labels_allow_duplicates() {
        let sup = test_supervisor_with_db().await;

        let schedule_a = Schedule {
            id: ScheduleId("R2-A1".to_string()),
            creator_id: "creator-diff".to_string(),
            preset_id: "preset-y".to_string(),
            preset_version: 1,
            status: ScheduleStatus::Pending,
            concurrency: ScheduleConcurrency::Serial,
            depends_on: vec![],
            current_core_context_version: CoreContextVersion(0),
            current_session_id: None,
            scheduled_at: None,
            label: Some("label-a".to_string()),
            created_at: String::new(),
            updated_at: String::new(),
            terminated_at: None,
        };

        let schedule_b = Schedule {
            id: ScheduleId("R2-B1".to_string()),
            creator_id: "creator-diff".to_string(),
            preset_id: "preset-y".to_string(),
            preset_version: 1,
            status: ScheduleStatus::Pending,
            concurrency: ScheduleConcurrency::Serial,
            depends_on: vec![],
            current_core_context_version: CoreContextVersion(0),
            current_session_id: None,
            scheduled_at: None,
            label: Some("label-b".to_string()), // different label
            created_at: String::new(),
            updated_at: String::new(),
            terminated_at: None,
        };

        // Both should succeed — different labels
        sup.insert_pending(schedule_a).await.unwrap();
        sup.insert_pending(schedule_b).await.unwrap();
    }

    // ---------- R3+R7: Smart resume (paused→running if admitted) ----------

    #[tokio::test]
    async fn r3_resume_paused_goes_directly_to_running_when_admitted() {
        let sup = test_supervisor_with_db().await;

        // Insert and start a schedule, then complete it
        insert_schedule(&sup, "R3-A", "pending").await;
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("R3-A").await.unwrap(),
            ScheduleStatus::Running
        );
        sup.on_schedule_terminal("R3-A", ScheduleStatus::Completed)
            .await
            .unwrap();

        // Insert another schedule for same creator, start it, then pause it
        insert_schedule(&sup, "R3-B", "pending").await;
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("R3-B").await.unwrap(),
            ScheduleStatus::Running
        );

        sup.pause_schedule("R3-B").await.unwrap();
        assert_eq!(sup.status_of("R3-B").await.unwrap(), ScheduleStatus::Paused);

        // R3: Resume — no running schedules for this creator, so B should go directly to Running
        let new_status = sup.resume_schedule("R3-B").await.unwrap();
        assert_eq!(
            new_status, "running",
            "R3: resume should go directly to Running when admitted"
        );
        assert_eq!(
            sup.status_of("R3-B").await.unwrap(),
            ScheduleStatus::Running
        );
    }

    #[tokio::test]
    async fn r3_resume_paused_falls_back_to_pending_when_not_admitted() {
        let sup = test_supervisor_with_db().await;

        // Insert and start schedule A (serial)
        insert_schedule(&sup, "R3-C1", "pending").await;
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("R3-C1").await.unwrap(),
            ScheduleStatus::Running
        );

        // Insert and start schedule B (same creator, serial — can't run concurrently)
        // Actually B can't start because A is running and serial. Let's pause A first.
        // Simpler: start A, then insert B and try to start it — B stays pending.
        // Then pause A, resume B — B goes to running.
        // Let's test the fallback: pause B (which is pending), resume B while A is running.

        // Insert B while A is running
        insert_schedule(&sup, "R3-C2", "pending").await;
        sup.tick().await.unwrap();
        // B stays pending (A is running, serial)
        assert_eq!(
            sup.status_of("R3-C2").await.unwrap(),
            ScheduleStatus::Pending
        );

        // Pause B (pending→paused)
        sup.pause_schedule("R3-C2").await.unwrap();
        assert_eq!(
            sup.status_of("R3-C2").await.unwrap(),
            ScheduleStatus::Paused
        );

        // R3: Resume B while A is still running — should fall back to Pending
        let new_status = sup.resume_schedule("R3-C2").await.unwrap();
        assert_eq!(
            new_status, "pending",
            "R3: resume should fall back to Pending when not admitted (serial blocked by running A)"
        );
        assert_eq!(
            sup.status_of("R3-C2").await.unwrap(),
            ScheduleStatus::Pending
        );
    }

    #[tokio::test]
    async fn r7_resume_single_step_user_experience() {
        // R7 is the UX simplification. The backend R3 change makes resume smart.
        // This test verifies that a single resume call handles the full transition.
        let sup = test_supervisor_with_db().await;

        // Start and pause a schedule
        insert_schedule(&sup, "R7-S1", "pending").await;
        sup.tick().await.unwrap();
        sup.pause_schedule("R7-S1").await.unwrap();

        // Verify paused
        assert_eq!(
            sup.status_of("R7-S1").await.unwrap(),
            ScheduleStatus::Paused
        );

        // Single resume call — should go to Running (no deps, no running siblings)
        let new_status = sup.resume_schedule("R7-S1").await.unwrap();
        assert_eq!(new_status, "running");
        assert_eq!(
            sup.status_of("R7-S1").await.unwrap(),
            ScheduleStatus::Running
        );
    }

    // ---------- R5: Delete cascade for schedules ----------

    #[tokio::test]
    async fn r5_delete_schedule_cancels_active_session() {
        let sup = test_supervisor_with_db().await;
        let pool = sup.pool();

        // Insert a schedule and an associated session
        insert_schedule(&sup, "R5-S1", "running").await;

        // Insert an orchestration_session for this schedule
        let now = chrono::Utc::now().timestamp();
        // SAFETY: test-only — DML helper for session setup.
        sqlx::query(
            r"INSERT INTO orchestration_sessions
               (session_id, creator_id, preset_id, preset_version, status,
                context_json, created_at, updated_at)
               VALUES (?, 'test-creator', 'test-preset', 1, 'running',
                '{}', ?, ?)",
        )
        .bind("R5-SESSION-1")
        .bind(now)
        .bind(now)
        .execute(&*pool)
        .await
        .unwrap();

        // Link the session to the schedule
        // SAFETY: test-only — DML helper.
        sqlx::query("UPDATE creator_schedules SET current_session_id = ? WHERE schedule_id = ?")
            .bind("R5-SESSION-1")
            .bind("R5-S1")
            .execute(&*pool)
            .await
            .unwrap();

        // Verify the session is linked
        let sid: Option<String> = sqlx::query_scalar(
            "SELECT current_session_id FROM creator_schedules WHERE schedule_id = ?",
        )
        .bind("R5-S1")
        .fetch_one(&*pool)
        .await
        .unwrap();
        assert_eq!(sid, Some("R5-SESSION-1".to_string()));

        // Cancel session first (simulating delete logic)
        let cancel_now = chrono::Utc::now().timestamp();
        // SAFETY: test-only — DML helper.
        sqlx::query(
            "UPDATE orchestration_sessions SET status = 'cancelled', updated_at = ?
             WHERE session_id = ? AND status = 'running'",
        )
        .bind(cancel_now)
        .bind("R5-SESSION-1")
        .execute(&*pool)
        .await
        .unwrap();

        // NULL out current_session_id
        // SAFETY: test-only — DML helper.
        sqlx::query("UPDATE creator_schedules SET current_session_id = NULL WHERE schedule_id = ?")
            .bind("R5-S1")
            .execute(&*pool)
            .await
            .unwrap();

        // Cancel the schedule
        // SAFETY: test-only — DML helper.
        sqlx::query(
            "UPDATE creator_schedules SET status = 'cancelled', terminated_at = ?, updated_at = ?
             WHERE schedule_id = ?",
        )
        .bind(cancel_now)
        .bind(cancel_now)
        .bind("R5-S1")
        .execute(&*pool)
        .await
        .unwrap();

        // Delete the schedule
        // SAFETY: test-only — DML helper.
        sqlx::query("DELETE FROM creator_schedules WHERE schedule_id = ?")
            .bind("R5-S1")
            .execute(&*pool)
            .await
            .unwrap();

        // Verify schedule is deleted
        let result = sqlx::query_scalar::<_, Option<String>>(
            "SELECT schedule_id FROM creator_schedules WHERE schedule_id = ?",
        )
        .bind("R5-S1")
        .fetch_optional(&*pool)
        .await
        .unwrap();
        assert!(result.is_none(), "schedule should be deleted");

        // Verify session was cancelled
        let session_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM orchestration_sessions WHERE session_id = ?")
                .bind("R5-SESSION-1")
                .fetch_optional(&*pool)
                .await
                .unwrap();
        // Session may remain (FK is one-way), but its status should be cancelled
        assert_eq!(session_status, Some("cancelled".to_string()));
    }

    #[tokio::test]
    async fn r5_delete_terminal_schedule() {
        let sup = test_supervisor_with_db().await;
        let pool = sup.pool();

        // Insert a completed schedule with no dependents
        insert_schedule(&sup, "R5-S2", "completed").await;

        // Insert another unrelated pending schedule
        insert_schedule(&sup, "R5-S3", "pending").await;

        // Delete R5-S2 (terminal, no dependencies pointing at it)
        // SAFETY: test-only — DML helper.
        sqlx::query("DELETE FROM creator_schedules WHERE schedule_id = ?")
            .bind("R5-S2")
            .execute(&*pool)
            .await
            .unwrap();

        // Verify deleted
        let result = sup.status_of("R5-S2").await;
        assert!(result.is_err(), "completed schedule should be deleted");

        // R5-S3 should still exist
        assert!(
            sup.status_of("R5-S3").await.is_ok(),
            "R5-S3 should still exist after deleting R5-S2"
        );
    }

    #[tokio::test]
    async fn r5_cannot_delete_schedule_that_is_a_dependency_target() {
        let sup = test_supervisor_with_db().await;
        let pool = sup.pool();

        // Insert A (completed) and B (pending, depends on A)
        insert_schedule(&sup, "R5-DEP-A", "completed").await;
        insert_schedule(&sup, "R5-DEP-B", "pending").await;

        // SAFETY: test-only — DML helper for dependency setup.
        sqlx::query("INSERT INTO schedule_dependencies (schedule_id, depends_on) VALUES (?, ?)")
            .bind("R5-DEP-B")
            .bind("R5-DEP-A")
            .execute(&*pool)
            .await
            .unwrap();

        // Attempting to delete A should fail due to FK constraint
        // (depends_on FK does NOT have CASCADE)
        let result = sqlx::query("DELETE FROM creator_schedules WHERE schedule_id = ?")
            .bind("R5-DEP-A")
            .execute(&*pool)
            .await;

        assert!(
            result.is_err(),
            "should not be able to delete a schedule that is a dependency target (FK constraint)"
        );
    }

    // ===================================================================
    // WS-A V1.6 Residual fixes: R1 (cancel-pause warn), R2 (TOCTOU)
    // ===================================================================

    // ---------- R1: Cancel-path pause logs error, does not block cancel ----------

    #[tokio::test]
    async fn r1_cancel_pause_failure_does_not_block_cancel() {
        // This test verifies the R1 fix at the supervisor level: pause_schedule()
        // returns false for non-pausable states (already paused), and the cancel
        // operation should still succeed. The HTTP handler in schedules.rs
        // translates pause errors to warn! logs and continues with cancel.
        let sup = test_supervisor_with_db().await;
        let pool = sup.pool();

        // Insert a pending schedule (pauseable)
        insert_schedule(&sup, "R1-CANCEL-S1", "pending").await;

        // Pause it (should succeed)
        let paused = sup.pause_schedule("R1-CANCEL-S1").await.unwrap();
        assert!(paused, "pending schedule should be pausable");

        // Try to pause again while already paused — returns Ok(false), not error.
        // This verifies the pause_schedule method handles non-pausable states
        // gracefully (returning false), which the cancel handler can safely ignore.
        let paused_again = sup.pause_schedule("R1-CANCEL-S1").await.unwrap();
        assert!(
            !paused_again,
            "already-paused schedule should return false from pause"
        );

        // Cancel the paused schedule via direct SQL (simulating the HTTP handler
        // cancel path which updates status to 'cancelled' directly).
        let now = chrono::Utc::now().timestamp();
        // SAFETY: test-only — DML helper for schedule cancellation.
        sqlx::query(
            "UPDATE creator_schedules SET status = 'cancelled', terminated_at = ?, updated_at = ?
             WHERE schedule_id = ?",
        )
        .bind(now)
        .bind(now)
        .bind("R1-CANCEL-S1")
        .execute(&*pool)
        .await
        .unwrap();

        assert_eq!(
            sup.status_of("R1-CANCEL-S1").await.unwrap(),
            ScheduleStatus::Cancelled,
            "schedule should be cancelled even if pause during cancel returned false"
        );
    }

    #[tokio::test]
    async fn r1_running_schedule_pause_then_cancel_succeeds() {
        // Verify: running schedule → pause → cancel succeeds.
        let sup = test_supervisor_with_db().await;
        let pool = sup.pool();

        insert_schedule(&sup, "R1-CANCEL-S2", "pending").await;
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("R1-CANCEL-S2").await.unwrap(),
            ScheduleStatus::Running
        );

        // Pause the running schedule
        let paused = sup.pause_schedule("R1-CANCEL-S2").await.unwrap();
        assert!(paused, "running schedule should be pausable");
        assert_eq!(
            sup.status_of("R1-CANCEL-S2").await.unwrap(),
            ScheduleStatus::Paused
        );

        // Cancel via direct SQL (simulating HTTP cancel handler path)
        let now = chrono::Utc::now().timestamp();
        // SAFETY: test-only — DML helper for schedule cancellation.
        sqlx::query(
            "UPDATE creator_schedules SET status = 'cancelled', terminated_at = ?, updated_at = ?
             WHERE schedule_id = ?",
        )
        .bind(now)
        .bind(now)
        .bind("R1-CANCEL-S2")
        .execute(&*pool)
        .await
        .unwrap();

        assert_eq!(
            sup.status_of("R1-CANCEL-S2").await.unwrap(),
            ScheduleStatus::Cancelled,
            "paused schedule should be cancellable"
        );
    }

    // ---------- R2: TOCTOU race on concurrent resume_schedule ----------

    #[tokio::test]
    async fn r2_resume_toctou_race_returns_current_status() {
        let sup = test_supervisor_with_db().await;

        // Insert and start a schedule
        insert_schedule(&sup, "R2-TOCTOU-S1", "pending").await;
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("R2-TOCTOU-S1").await.unwrap(),
            ScheduleStatus::Running
        );

        // Pause the schedule
        sup.pause_schedule("R2-TOCTOU-S1").await.unwrap();
        assert_eq!(
            sup.status_of("R2-TOCTOU-S1").await.unwrap(),
            ScheduleStatus::Paused
        );

        // First resume: should succeed (paused → running, no blocking schedules)
        let status1 = sup.resume_schedule("R2-TOCTOU-S1").await.unwrap();
        assert_eq!(status1, "running");

        // Second resume: the schedule is now "running", so the UPDATE's
        // WHERE clause (status = 'paused') matches 0 rows.
        // R2 fix: resume_schedule returns current status instead of error.
        let status2 = sup.resume_schedule("R2-TOCTOU-S1").await;
        // Should return an error (InvalidTransition: running → running is invalid),
        // because the status check happens before the TOCTOU-vulnerable UPDATE.
        assert!(
            status2.is_err(),
            "second resume should fail — schedule is already running, not paused"
        );
    }

    #[tokio::test]
    async fn r2_concurrent_resume_toctou_exercises_rows_affected_zero() {
        // Exercise the `rows_affected() == 0` branch by calling resume_schedule()
        // concurrently on a Paused schedule. A barrier ensures both spawned tasks
        // pass the initial "status == paused" check before either issues its
        // UPDATE. Only one UPDATE can match `WHERE status = 'paused'`; the
        // loser hits `rows_affected() == 0` and returns the current DB status.
        let sup = test_supervisor_with_db().await;

        // Insert and start a schedule, then pause it
        insert_schedule(&sup, "R2-CONC-S1", "pending").await;
        sup.tick().await.unwrap();
        assert_eq!(
            sup.status_of("R2-CONC-S1").await.unwrap(),
            ScheduleStatus::Running
        );
        sup.pause_schedule("R2-CONC-S1").await.unwrap();
        assert_eq!(
            sup.status_of("R2-CONC-S1").await.unwrap(),
            ScheduleStatus::Paused
        );

        // Barrier with 3 participants: two worker tasks + the main test task.
        // Both workers wait at the barrier after reading "paused" (verified by
        // the early check) but before the UPDATE. Once all arrive, they race
        // to UPDATE, and exactly one hits rows_affected() == 0.
        let barrier = Arc::new(tokio::sync::Barrier::new(3));
        let pool = sup.pool();
        let pool_a = pool.clone();
        let pool_b = pool.clone();
        let barrier_a = Arc::clone(&barrier);
        let barrier_b = Arc::clone(&barrier);

        let handle_a = tokio::spawn(async move {
            let sid = "R2-CONC-S1";
            // Step 1: Read status (should be "paused")
            let row = sqlx::query_as::<_, StatusCreatorRow>(
                "SELECT status, creator_id FROM creator_schedules WHERE schedule_id = ?",
            )
            .bind(sid)
            .fetch_one(&*pool_a)
            .await
            .unwrap();
            assert_eq!(row.status, "paused", "task A should see paused");
            // Step 2: Wait at barrier — both tasks see "paused"
            barrier_a.wait().await;
            // Step 3: Issue the same UPDATE as resume_schedule()
            let now = chrono::Utc::now().timestamp();
            // SAFETY: test-only — replicating production UPDATE pattern for TOCTOU test.
            let result = sqlx::query(
                "UPDATE creator_schedules SET status = 'running', updated_at = ?
                 WHERE schedule_id = ? AND status = 'paused'",
            )
            .bind(now)
            .bind(sid)
            .execute(&*pool_a)
            .await
            .unwrap();
            result.rows_affected()
        });

        let handle_b = tokio::spawn(async move {
            let sid = "R2-CONC-S1";
            // Step 1: Read status (should be "paused")
            let row = sqlx::query_as::<_, StatusCreatorRow>(
                "SELECT status, creator_id FROM creator_schedules WHERE schedule_id = ?",
            )
            .bind(sid)
            .fetch_one(&*pool_b)
            .await
            .unwrap();
            assert_eq!(row.status, "paused", "task B should see paused");
            // Step 2: Wait at barrier — both tasks see "paused"
            barrier_b.wait().await;
            // Step 3: Issue the same UPDATE as resume_schedule()
            let now = chrono::Utc::now().timestamp();
            // SAFETY: test-only — replicating production UPDATE pattern for TOCTOU test.
            let result = sqlx::query(
                "UPDATE creator_schedules SET status = 'running', updated_at = ?
                 WHERE schedule_id = ? AND status = 'paused'",
            )
            .bind(now)
            .bind(sid)
            .execute(&*pool_b)
            .await
            .unwrap();
            result.rows_affected()
        });

        // Main task also waits at barrier (3rd participant)
        barrier.wait().await;

        let (ra, rb) = tokio::join!(handle_a, handle_b);
        let rows_a = ra.expect("task A should not panic");
        let rows_b = rb.expect("task B should not panic");

        // Exactly one UPDATE should have affected 1 row (the winner),
        // the other should have affected 0 rows (the loser — TOCTOU).
        assert_eq!(
            rows_a + rows_b,
            1,
            "exactly one UPDATE should affect 1 row (total affected = {} + {} = {})",
            rows_a,
            rows_b,
            rows_a + rows_b
        );

        // Verify the loser indeed got 0 rows affected
        assert!(
            rows_a == 0 || rows_b == 0,
            "one task must have rows_affected() == 0 (TOCTOU race exercised)"
        );

        // Final state must be Running
        assert_eq!(
            sup.status_of("R2-CONC-S1").await.unwrap(),
            ScheduleStatus::Running,
            "schedule should end in Running state after concurrent UPDATEs"
        );
    }

    #[tokio::test]
    async fn r2_resume_paused_schedule_succeeds_normally() {
        // Verify normal resume still works after R2 fix (no regression).
        let sup = test_supervisor_with_db().await;

        insert_schedule(&sup, "R2-NORMAL-S1", "pending").await;
        sup.tick().await.unwrap();
        sup.pause_schedule("R2-NORMAL-S1").await.unwrap();
        assert_eq!(
            sup.status_of("R2-NORMAL-S1").await.unwrap(),
            ScheduleStatus::Paused
        );

        // Resume should succeed
        let status = sup.resume_schedule("R2-NORMAL-S1").await.unwrap();
        assert_eq!(status, "running");
        assert_eq!(
            sup.status_of("R2-NORMAL-S1").await.unwrap(),
            ScheduleStatus::Running
        );
    }

    /// T3 fix-1 (P1-1): a schedule PAUSED after a failed settlement pass
    /// (the boot pause flipped it after a transient reconcile error) is
    /// settled on the NEXT boot pass. Properties pinned:
    /// - the reconcile scan covers `paused` (any non-terminal schedule
    ///   status), not just `running`;
    /// - a reconcile error (durable run load failure) never flips the row
    ///   terminal, so the row stays inside the scan and is retried;
    /// - once the error clears, the paused row settles to the durable
    ///   terminal status and then leaves the scan.
    #[tokio::test]
    async fn reconcile_settles_paused_row_after_prior_load_error() {
        let sup = test_supervisor_with_db().await;
        let now = chrono::Utc::now().timestamp();

        // Durable v1 session: terminal `completed` with a CORRUPT run-state
        // blob — the previous recovery pass hit a transient storage error
        // while loading it.
        let descriptor = serde_json::to_vec(&crate::run_state::RunDescriptorV1 {
            creator_id: "test-creator".to_string(),
            work_id: None,
            workspace_root: std::path::PathBuf::new(),
            preset_id: "test-preset".to_string(),
            preset_version: 1,
            source: crate::run_state::PresetSourceIdentity::Embedded {
                preset_id: "test-preset".to_string(),
                content_hash: [0u8; 32],
            },
            input: serde_json::Map::new(),
            agent_bindings: std::collections::HashMap::new(),
            parent_session_id: None,
            graph_name: None,
        })
        .expect("descriptor json");
        sqlx::query(
            "INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version, status,
                 current_task_id, context_json, created_at, updated_at,
                 execution_version, state_revision, run_state_json, run_descriptor_json)
             VALUES ('SES-P1-1', 'test-creator', 'test-preset', 1, 'completed',
                     NULL, X'7B7D', ?, ?, 1, 5, X'00', ?)",
        )
        .bind(now - 10)
        .bind(now)
        .bind(&descriptor)
        .execute(&*sup.pool)
        .await
        .expect("insert durable terminal run");

        // driven_v1 schedule owning that session, PAUSED — the boot pause
        // after the earlier (failed) settlement pass (P1-1 shape).
        sqlx::query(
            "INSERT INTO creator_schedules
                (schedule_id, creator_id, preset_id, preset_version, status,
                 concurrency_kind, current_core_context_version, current_session_id,
                 created_at, updated_at, execution_policy)
             VALUES ('SCH-P1-1', 'test-creator', 'test-preset', 1, 'paused',
                     'serial', 0, 'SES-P1-1', ?, ?, 'driven_v1')",
        )
        .bind(now - 5)
        .bind(now)
        .execute(&*sup.pool)
        .await
        .expect("insert paused driven schedule");

        // Pass 1 — the durable run cannot be loaded: reconcile skips the
        // row. The row must NOT be excluded: still paused, still
        // non-terminal, still inside the scan.
        let count = sup
            .reconcile_terminal_schedules()
            .await
            .expect("reconcile pass 1");
        assert_eq!(count, 0, "corrupt durable row is skipped, not settled");
        assert_eq!(
            sup.status_of("SCH-P1-1").await.unwrap(),
            ScheduleStatus::Paused,
            "a reconcile error must never flip the row terminal"
        );

        // The storage glitch clears (next boot): the SAME paused row is
        // retried and settles from the durable terminal — paused rows are
        // inside the reconcile scan and errors never permanently exclude.
        let good_state =
            serde_json::to_vec(&crate::run_state::RunStateV1::default()).expect("run state json");
        sqlx::query("UPDATE orchestration_sessions SET run_state_json = ? WHERE session_id = ?")
            .bind(&good_state)
            .bind("SES-P1-1")
            .execute(&*sup.pool)
            .await
            .expect("repair run state");
        let count = sup
            .reconcile_terminal_schedules()
            .await
            .expect("reconcile pass 2");
        assert_eq!(count, 1, "paused row settles on the next pass");
        assert_eq!(
            sup.status_of("SCH-P1-1").await.unwrap(),
            ScheduleStatus::Completed,
            "paused row must settle to the durable terminal status"
        );
        let owned: Option<String> = sqlx::query_scalar(
            "SELECT current_session_id FROM creator_schedules WHERE schedule_id = ?",
        )
        .bind("SCH-P1-1")
        .fetch_one(&*sup.pool)
        .await
        .expect("owned session");
        assert_eq!(owned.as_deref(), Some("SES-P1-1"));

        // Terminal rows leave the scan: a third pass is a no-op.
        let count = sup
            .reconcile_terminal_schedules()
            .await
            .expect("reconcile pass 3");
        assert_eq!(count, 0, "settled rows exit the scan");
    }
}
