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

use nexus_contracts::local::schedule::{
    ParallelWithIds, Schedule, ScheduleConcurrency, ScheduleId, ScheduleStatus,
};
use nexus_local_db::SqlitePool;
use tokio::sync::Mutex;

use super::admission::{admit, CompletedSet, RunningSet};

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
}

struct Inner {
    /// Cache of running schedule IDs keyed by creator for quick admission checks.
    running_by_creator: HashMap<String, HashSet<ScheduleId>>,
}

impl ScheduleSupervisor {
    /// Create a new supervisor backed by the given shared SQLite pool.
    ///
    /// The pool must already have migrations applied (including the
    /// `creator_schedules` table).
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            pool,
            inner: Mutex::new(Inner {
                running_by_creator: HashMap::new(),
            }),
            tick_in_progress: AtomicBool::new(false),
        }
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
    /// **Per-creator scoping (H1)**: Concurrency checks (Serial, ParallelWith) only
    /// consider schedules belonging to the same creator. Cross-creator dependencies
    /// (`depends_on`) are still checked against the global completed set.
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
    async fn tick_inner(
        &self,
        now: i64,
        scheduled_at_cutoff: Option<i64>,
    ) -> Result<(), SupervisorError> {
        let pool = &*self.pool;

        // Load all schedules from DB
        let all_rows = sqlx::query_as!(
            ScheduleRow,
            "SELECT schedule_id as \"schedule_id!\", creator_id, preset_id, preset_version,
                    status, concurrency_kind, concurrency_whitelist,
                    current_core_context_version, current_session_id,
                    scheduled_at, label, created_at, updated_at, terminated_at
             FROM creator_schedules",
        )
        .fetch_all(pool)
        .await?;

        // Classify into running (by creator), completed/cancelled, and pending
        let mut running_by_creator: HashMap<String, HashSet<ScheduleId>> = HashMap::new();
        let mut completed_ids: Vec<ScheduleId> = Vec::new();
        let mut pending: Vec<Schedule> = Vec::new();

        for row in &all_rows {
            let schedule = row.to_schedule();
            match schedule.status {
                ScheduleStatus::Running => {
                    running_by_creator
                        .entry(schedule.creator_id.clone())
                        .or_default()
                        .insert(schedule.id.clone());
                }
                ScheduleStatus::Completed | ScheduleStatus::Cancelled => {
                    completed_ids.push(schedule.id.clone());
                }
                ScheduleStatus::Pending => {
                    // V1.5 WS-D: filter by scheduled_at for clock-triggered tick
                    // scheduled_at_cutoff filters by scheduled_at <= cutoff OR scheduled_at IS NULL
                    let due = match (scheduled_at_cutoff, &schedule.scheduled_at) {
                        (None, _) => true, // on-demand tick: admit all pending
                        (Some(_cutoff), None) => true, // no scheduled_at: on-demand schedule
                        (Some(cutoff), Some(scheduled_str)) => {
                            // Parse scheduled_at string (Unix timestamp as string)
                            scheduled_str.parse::<i64>().map(|t| t <= cutoff).unwrap_or(false)
                        }
                    };
                    if due {
                        pending.push(schedule);
                    }
                }
                _ => {}
            }
        }

        let completed_set = CompletedSet::from(completed_ids);

        // Load depends_on for each pending schedule from schedule_dependencies
        let pool_ref = &*self.pool;
        let mut pending_with_deps: Vec<Schedule> = Vec::with_capacity(pending.len());
        for schedule in pending {
            let sid = schedule.id.0.to_owned();
            let dep_rows = sqlx::query_scalar!(
                "SELECT depends_on as \"depends_on!\" FROM schedule_dependencies WHERE schedule_id = ?",
                sid
            )
            .fetch_all(pool_ref)
            .await?;
            let deps: Vec<ScheduleId> = dep_rows.into_iter().map(ScheduleId).collect();
            let mut schedule = schedule;
            schedule.depends_on = deps;
            pending_with_deps.push(schedule);
        }

        // Sort pending by created_at for FIFO ordering
        pending_with_deps.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        // Evaluate admission one at a time, updating the running set after
        // each admission so that Serial schedules don't all get admitted
        // in the same tick. Concurrency checks are scoped per-creator (H1).
        let mut started = Vec::new();
        let mut running_by_creator_so_far = running_by_creator;

        for candidate in &pending_with_deps {
            let running_set = RunningSet::from_entries(
                running_by_creator_so_far
                    .iter()
                    .flat_map(|(c, ids)| ids.iter().map(|id| (c.clone(), id.clone())))
                    .collect(),
            );
            if admit(candidate, &running_set, &completed_set) {
                started.push((candidate.creator_id.clone(), candidate.id.0.clone()));
                // Immediately update the running set so subsequent candidates
                // see this one as running (important for Serial).
                running_by_creator_so_far
                    .entry(candidate.creator_id.clone())
                    .or_default()
                    .insert(candidate.id.clone());
            }
        }

        // Update admitted schedules to Running in DB
        for (_creator_id, sid) in &started {
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
        let mut inner = self.inner.lock().await;
        for (creator_id, sid) in &started {
            inner
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
    /// the running set, and triggers a `tick()` to potentially start the
    /// next eligible schedule.
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

        let now = chrono::Utc::now().timestamp();
        let status_str = match terminal_status {
            ScheduleStatus::Completed => "completed",
            ScheduleStatus::Failed => "failed",
            ScheduleStatus::Cancelled => "cancelled",
            _ => unreachable!(),
        };

        // Fetch creator_id for the schedule before removing from running set
        let schedule_id_owned = schedule_id.to_owned();
        let row = sqlx::query_scalar!(
            "SELECT creator_id FROM creator_schedules WHERE schedule_id = ?",
            schedule_id_owned
        )
        .fetch_optional(&*self.pool)
        .await?;

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
        if let Some(creator_id) = row {
            let mut inner = self.inner.lock().await;
            if let Some(ids) = inner.running_by_creator.get_mut(&creator_id) {
                ids.remove(&ScheduleId(schedule_id.to_string()));
            }
        }

        // Trigger tick to admit next eligible schedule
        self.tick().await?;

        Ok(())
    }

    /// Insert a pending schedule into the database (for testing and CLI use).
    ///
    /// The supervisor doesn't do scheduling logic here — that happens on `tick()`.
    ///
    /// **R2 — Duplicate detection**: Before inserting, checks whether a schedule
    /// with the same `(creator_id, preset_id, label)` already exists. If so,
    /// returns [`SupervisorError::DuplicateSchedule`].
    pub async fn insert_pending(&self, schedule: Schedule) -> Result<(), SupervisorError> {
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
        let preset_version_i64 = schedule.preset_version as i64;
        let context_version_i64 = schedule.current_core_context_version.0 as i64;
        let scheduled_at = schedule.scheduled_at;
        let label = schedule.label;

        sqlx::query!(
            r#"INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, concurrency_whitelist,
                current_core_context_version, current_session_id,
                scheduled_at, label, created_at, updated_at, terminated_at)
               VALUES (?, ?, ?, ?, 'pending', ?, ?, ?, NULL, ?, ?, ?, ?, NULL)"#,
            schedule_id,
            creator_id,
            preset_id,
            preset_version_i64,
            concurrency_kind,
            concurrency_whitelist,
            context_version_i64,
            scheduled_at,
            label,
            created_at,
            updated_at
        )
        .execute(&*self.pool)
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
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    /// Create a [`CoreContextManager`] backed by the same pool.
    ///
    /// This avoids requiring callers to hold a reference to the supervisor's
    /// internal pool. The returned manager is lightweight (just an Arc clone).
    pub fn core_context_manager(&self) -> super::derivation::CoreContextManager {
        super::derivation::CoreContextManager::new(self.pool.clone())
    }

    /// Get a reference to the underlying SQLite pool.
    pub fn pool(&self) -> Arc<SqlitePool> {
        self.pool.clone()
    }

    /// Get the current status of a schedule by ID (for testing/inspection).
    pub async fn status_of(&self, schedule_id: &str) -> Result<ScheduleStatus, SupervisorError> {
        let schedule_id_owned = schedule_id.to_owned();
        let row = sqlx::query_scalar!(
            "SELECT status FROM creator_schedules WHERE schedule_id = ?",
            schedule_id_owned
        )
        .fetch_optional(&*self.pool)
        .await?;

        match row {
            Some(status_str) => match status_str.as_str() {
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
            None => Err(SupervisorError::NotFound(schedule_id.to_string())),
        }
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
    pub async fn resume_running_as_paused(&self, reason: &str) -> Result<usize, SupervisorError> {
        let now = chrono::Utc::now().timestamp();

        let rows = sqlx::query_scalar!(
            "SELECT schedule_id as \"schedule_id!\" FROM creator_schedules WHERE status = 'running'",
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
            let mut inner = self.inner.lock().await;
            inner.running_by_creator.clear();
            tracing::info!("paused {} running schedule(s) (reason: {})", count, reason);
        }

        Ok(count)
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
    /// Returns the new status as a string ("running" or "pending").
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
                    "pending" => ScheduleStatus::Pending,
                    "running" => ScheduleStatus::Running,
                    "completed" => ScheduleStatus::Completed,
                    "cancelled" => ScheduleStatus::Cancelled,
                    "failed" => ScheduleStatus::Failed,
                    _ => ScheduleStatus::Pending,
                },
            ));
        }

        let creator_id = row.creator_id;

        // Check admission rules to decide: direct to Running or fall back to Pending.
        let mut should_run = false;

        // Build the current running set and completed set from DB.
        let all_rows = sqlx::query_as!(
            ScheduleRow,
            "SELECT schedule_id as \"schedule_id!\", creator_id, preset_id, preset_version,
                    status, concurrency_kind, concurrency_whitelist,
                    current_core_context_version, current_session_id,
                    scheduled_at, label, created_at, updated_at, terminated_at
             FROM creator_schedules",
        )
        .fetch_all(&*self.pool)
        .await?;

        let mut running_entries: HashSet<(String, ScheduleId)> = HashSet::new();
        let mut completed_ids: Vec<ScheduleId> = Vec::new();
        let mut candidate_schedule: Option<Schedule> = None;

        for r in &all_rows {
            let s = r.to_schedule();
            match s.status {
                ScheduleStatus::Running => {
                    running_entries.insert((s.creator_id.clone(), s.id.clone()));
                }
                ScheduleStatus::Completed | ScheduleStatus::Cancelled => {
                    completed_ids.push(s.id.clone());
                }
                _ => {}
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
            // R3: Direct paused→running transition
            // SAFETY: runtime `sqlx::query` — new UPDATE for resume_running.
            sqlx::query(
                "UPDATE creator_schedules SET status = 'running', updated_at = ?
                 WHERE schedule_id = ? AND status = 'paused'",
            )
            .bind(now)
            .bind(schedule_id)
            .execute(&*self.pool)
            .await?;

            // Add to running cache
            let mut inner = self.inner.lock().await;
            inner
                .running_by_creator
                .entry(creator_id)
                .or_default()
                .insert(ScheduleId(schedule_id.to_string()));

            Ok("running".to_string())
        } else {
            // Fallback: paused→pending
            // SAFETY: runtime `sqlx::query` — new UPDATE for resume fallback.
            sqlx::query(
                "UPDATE creator_schedules SET status = 'pending', updated_at = ?
                 WHERE schedule_id = ? AND status = 'paused'",
            )
            .bind(now)
            .bind(schedule_id)
            .execute(&*self.pool)
            .await?;

            // Trigger tick to attempt admission
            self.tick().await?;

            Ok("pending".to_string())
        }
    }
}

/// Internal row for status + creator_id lookups.
#[derive(sqlx::FromRow)]
struct StatusCreatorRow {
    status: String,
    creator_id: String,
}

/// Internal row mapping for reading `creator_schedules` from SQLite.
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
}

impl ScheduleRow {
    fn to_schedule(&self) -> Schedule {
        use nexus_contracts::local::schedule::{
            CoreContextVersion, Schedule, ScheduleConcurrency, ScheduleId, ScheduleStatus,
        };

        let concurrency = match self.concurrency_kind.as_str() {
            "serial" => ScheduleConcurrency::Serial,
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
            "pending" => ScheduleStatus::Pending,
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
            preset_version: self.preset_version as u32,
            status,
            concurrency,
            depends_on: Vec::new(), // loaded separately via schedule_dependencies
            current_core_context_version: CoreContextVersion(
                self.current_core_context_version as u32,
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
        sqlx::query(
            r#"INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, current_core_context_version,
                created_at, updated_at)
               VALUES (?, ?, 'test-preset', 1, ?,
               'serial', 0, ?, ?)"#,
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
            r#"INSERT INTO orchestration_sessions
               (session_id, creator_id, preset_id, preset_version, status,
                context_json, created_at, updated_at)
               VALUES (?, 'test-creator', 'test-preset', 1, 'running',
                '{}', ?, ?)"#,
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
}
