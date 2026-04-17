//! `ScheduleSupervisor` — per-creator schedule queue manager (spec §5).
//!
//! Maintains a per-creator queue of [`Schedule`]s. On any state change that
//! could unlock progress (new schedule added, current schedule completed,
//! user cancel/pause/resume), recomputes eligibility and admits the next
//! eligible schedule.
//!
//! Uses `tokio::sync::Mutex<Inner>` for interior mutability so that `tick()`
//! and `on_session_terminal()` can be called concurrently.

use std::collections::HashSet;
use std::sync::Arc;

use nexus_contracts::local::schedule::{ParallelWithIds, Schedule, ScheduleConcurrency, ScheduleId, ScheduleStatus};
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
}

struct Inner {
    /// Cache of running schedule IDs for quick admission checks.
    running_ids: HashSet<ScheduleId>,
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
                running_ids: HashSet::new(),
            }),
        }
    }

    /// Load pending schedules from DB, evaluate admission, and start eligible ones.
    ///
    /// For each pending schedule that passes the admission gate:
    /// - Update `creator_schedules.status` to `Running`
    /// - Set `updated_at` to current timestamp
    /// - Add to the running set
    pub async fn tick(&self) -> Result<(), SupervisorError> {
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

        // Classify into running, completed/cancelled, and pending
        let mut running_ids: HashSet<ScheduleId> = HashSet::new();
        let mut completed_ids: Vec<ScheduleId> = Vec::new();
        let mut pending: Vec<Schedule> = Vec::new();

        for row in &all_rows {
            let schedule = row.to_schedule();
            match schedule.status {
                ScheduleStatus::Running => {
                    running_ids.insert(schedule.id.clone());
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

        // Sort pending by created_at for FIFO ordering
        pending.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        // Evaluate admission one at a time, updating the running set after
        // each admission so that Serial schedules don't all get admitted
        // in the same tick.
        let mut started = Vec::new();
        let mut running_ids_so_far: HashSet<ScheduleId> = running_ids;

        for candidate in &pending {
            let running_set = RunningSet::from_ids(running_ids_so_far.clone());
            if admit(candidate, &running_set, &completed_set) {
                started.push(candidate.id.0.clone());
                // Immediately update the running set so subsequent candidates
                // see this one as running (important for Serial).
                running_ids_so_far.insert(candidate.id.clone());
            }
        }

        // Update admitted schedules to Running in DB
        for sid in &started {
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
        for sid in &started {
            inner.running_ids.insert(ScheduleId(sid.clone()));
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
        {
            let mut inner = self.inner.lock().await;
            inner.running_ids.remove(&ScheduleId(schedule_id.to_string()));
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
            schedule
                .created_at
                .parse()
                .unwrap_or(now)
        };
        let updated_at: i64 = if schedule.updated_at.is_empty() {
            now
        } else {
            schedule
                .updated_at
                .parse()
                .unwrap_or(now)
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
    pub async fn status_of(&self, schedule_id: &str) -> ScheduleStatus {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT status FROM creator_schedules WHERE schedule_id = ?1",
        )
        .bind(schedule_id)
        .fetch_optional(&*self.pool)
        .await;

        match row {
            Ok(Some((status_str,))) => match status_str.as_str() {
                "pending" => ScheduleStatus::Pending,
                "running" => ScheduleStatus::Running,
                "paused" => ScheduleStatus::Paused,
                "completed" => ScheduleStatus::Completed,
                "cancelled" => ScheduleStatus::Cancelled,
                "failed" => ScheduleStatus::Failed,
                _ => ScheduleStatus::Pending,
            },
            _ => ScheduleStatus::Pending,
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
                ScheduleConcurrency::ParallelWith(ParallelWithIds {
                    schedule_ids: ids,
                })
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
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, current_core_context_version,
                created_at, updated_at)
               VALUES (?1, 'test-creator', 'test-preset', 1, ?2,
               'serial', 0, ?3, ?3)"#,
        )
        .bind(id)
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
        assert_eq!(sup.status_of("S01").await, ScheduleStatus::Running);

        // Simulate daemon boot: resume running as paused.
        let count = sup
            .resume_running_as_paused("daemon_restart")
            .await
            .unwrap();
        assert_eq!(count, 1);

        // Verify it's now paused.
        assert_eq!(sup.status_of("S01").await, ScheduleStatus::Paused);

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
        assert_eq!(sup.status_of("S01").await, ScheduleStatus::Pending);
        // Running should be paused.
        assert_eq!(sup.status_of("S02").await, ScheduleStatus::Paused);
    }
}
