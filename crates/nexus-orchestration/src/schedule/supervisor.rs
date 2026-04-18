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

        let result = self.tick_inner().await;

        // Always release the guard, even on error.
        self.tick_in_progress.store(false, Ordering::Release);

        result
    }

    /// Inner implementation of tick (no re-entrancy guard).
    async fn tick_inner(&self) -> Result<(), SupervisorError> {
        let now = chrono::Utc::now().timestamp();
        let pool = &*self.pool;

        // Load all schedules from DB
        let all_rows = sqlx::query_as::<_, ScheduleRow>(
            "SELECT schedule_id, creator_id, preset_id, preset_version,
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
                    pending.push(schedule);
                }
                _ => {}
            }
        }

        let completed_set = CompletedSet::from(completed_ids);

        // Load depends_on for each pending schedule from schedule_dependencies
        let pool_ref = &*self.pool;
        let mut pending_with_deps: Vec<Schedule> = Vec::with_capacity(pending.len());
        for schedule in pending {
            let dep_rows = sqlx::query_as::<_, (String,)>(
                "SELECT depends_on FROM schedule_dependencies WHERE schedule_id = ?1",
            )
            .bind(&schedule.id.0)
            .fetch_all(pool_ref)
            .await?;
            let deps: Vec<ScheduleId> = dep_rows.into_iter().map(|(d,)| ScheduleId(d)).collect();
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
            sqlx::query(
                "UPDATE creator_schedules SET status = 'running', updated_at = ?1
                 WHERE schedule_id = ?2",
            )
            .bind(now)
            .bind(sid)
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
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT creator_id FROM creator_schedules WHERE schedule_id = ?1",
        )
        .bind(schedule_id)
        .fetch_optional(&*self.pool)
        .await?;

        sqlx::query(
            "UPDATE creator_schedules
             SET status = ?1, terminated_at = ?2, updated_at = ?2
             WHERE schedule_id = ?3 AND status = 'running'",
        )
        .bind(status_str)
        .bind(now)
        .bind(schedule_id)
        .execute(&*self.pool)
        .await?;

        // Remove from running cache
        if let Some((creator_id,)) = row {
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
    pub async fn insert_pending(&self, schedule: Schedule) -> Result<(), SupervisorError> {
        let now = chrono::Utc::now().timestamp();

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

        sqlx::query(
            r#"INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, concurrency_whitelist,
                current_core_context_version, current_session_id,
                scheduled_at, label, created_at, updated_at, terminated_at)
               VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11, NULL)"#,
        )
        .bind(&schedule.id.0)
        .bind(&schedule.creator_id)
        .bind(&schedule.preset_id)
        .bind(schedule.preset_version as i64)
        .bind(&concurrency_kind)
        .bind(&concurrency_whitelist)
        .bind(schedule.current_core_context_version.0 as i64)
        .bind(&schedule.scheduled_at)
        .bind(&schedule.label)
        .bind(created_at)
        .bind(updated_at)
        .execute(&*self.pool)
        .await?;

        // Insert dependencies
        for dep in &schedule.depends_on {
            sqlx::query(
                "INSERT OR IGNORE INTO schedule_dependencies (schedule_id, depends_on)
                 VALUES (?1, ?2)",
            )
            .bind(&schedule.id.0)
            .bind(&dep.0)
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
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT status FROM creator_schedules WHERE schedule_id = ?1",
        )
        .bind(schedule_id)
        .fetch_optional(&*self.pool)
        .await?;

        match row {
            Some((status_str,)) => match status_str.as_str() {
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

        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT schedule_id FROM creator_schedules WHERE status = 'running'",
        )
        .fetch_all(&*self.pool)
        .await?;

        let count = rows.len();
        for (sid,) in rows {
            sqlx::query(
                "UPDATE creator_schedules SET status = 'paused', updated_at = ?1
                 WHERE schedule_id = ?2 AND status = 'running'",
            )
            .bind(now)
            .bind(&sid)
            .execute(&*self.pool)
            .await?;
            tracing::info!("paused schedule {} (reason: {})", sid, reason);
        }

        if count > 0 {
            tracing::info!("paused {} running schedule(s) (reason: {})", count, reason);
        }

        Ok(count)
    }
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
    scheduled_at: Option<String>,
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
            scheduled_at: self.scheduled_at.clone(),
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
        sqlx::query(
            r#"INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, current_core_context_version,
                created_at, updated_at)
               VALUES (?1, ?2, 'test-preset', 1, ?3,
               'serial', 0, ?4, ?4)"#,
        )
        .bind(id)
        .bind(creator_id)
        .bind(status)
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
        sqlx::query("INSERT INTO schedule_dependencies (schedule_id, depends_on) VALUES (?1, ?2)")
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

        sqlx::query("INSERT INTO schedule_dependencies (schedule_id, depends_on) VALUES (?1, ?2)")
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
}
