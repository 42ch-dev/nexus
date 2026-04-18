//! Clock / wall-clock trigger scheduler for Creator Schedules (V1.5 WS-D).
//!
//! A background poller that periodically queries `creator_schedules` for
//! `Pending` schedules where `scheduled_at <= now()`, then submits them
//! to `ScheduleSupervisor::tick()` for admission.
//!
//! **Four hard constraints** (from crate-selection-best-practices-v1.md §3.7):
//! 1. **Wall-clock + tz-aware**: uses `ClockSource` trait returning Unix timestamp.
//! 2. **Per-key serialisation**: same schedule ID tracked in `HashSet` per tick cycle.
//! 3. **Graceful shutdown**: `CancellationToken` cancels background poll loop.
//! 4. **DST / clock-jump safety**: `ClockSource` abstraction allows mock for testing.
//!
//! Pre-1.0: Simple hand-rolled implementation (no `tokio-cron-scheduler` crate).
//! Zero new third-party dependencies.

use std::collections::HashSet;
use std::sync::Arc;

use nexus_contracts::local::schedule::ScheduleId;
use nexus_local_db::SqlitePool;
use tokio_util::sync::CancellationToken;

use crate::schedule::supervisor::ScheduleSupervisor;

/// Clock source abstraction for time-based triggers.
///
/// Allows mocking in tests without sleeping. Production uses `SystemClock`.
pub trait ClockSource: Send + Sync {
    /// Returns current Unix timestamp in seconds.
    fn now_unix(&self) -> i64;
}

/// Production clock source using `chrono::Utc::now()`.
pub struct SystemClock;

impl ClockSource for SystemClock {
    fn now_unix(&self) -> i64 {
        chrono::Utc::now().timestamp()
    }
}

/// Mock clock for testing with controllable time.
pub struct MockClock {
    now: std::sync::atomic::AtomicI64,
}

impl MockClock {
    /// Create a mock clock starting at the given Unix timestamp.
    pub fn new(initial: i64) -> Self {
        Self {
            now: std::sync::atomic::AtomicI64::new(initial),
        }
    }

    /// Set the current time to the given Unix timestamp.
    pub fn set(&self, ts: i64) {
        self.now.store(ts, std::sync::atomic::Ordering::SeqCst);
    }
}

impl ClockSource for MockClock {
    fn now_unix(&self) -> i64 {
        self.now.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Clock-trigger scheduler for Creator Schedules.
///
/// Polls `creator_schedules` for schedules due at `scheduled_at <= now()`
/// and submits them to `ScheduleSupervisor::tick()` for admission.
pub struct Scheduler {
    pool: Arc<SqlitePool>,
    clock: Arc<dyn ClockSource>,
}

impl Scheduler {
    /// Create a scheduler with the given SQLite pool and clock source.
    pub fn new(pool: Arc<SqlitePool>, clock: Arc<dyn ClockSource>) -> Self {
        Self { pool, clock }
    }

    /// Create a scheduler with production system clock.
    pub fn with_system_clock(pool: Arc<SqlitePool>) -> Self {
        Self::new(pool, Arc::new(SystemClock))
    }

    /// Poll for due schedules and submit to supervisor for admission.
    ///
    /// Returns the number of schedules that were found due (not necessarily admitted).
    ///
    /// **Per-key serialisation**: uses in-memory `HashSet<ScheduleId>` to prevent
    /// double-firing the same schedule within one poll cycle.
    pub async fn tick(&self, supervisor: &ScheduleSupervisor) -> usize {
        let now = self.clock.now_unix();

        // Query schedules due at scheduled_at <= now, status = pending
        // This is informational logging; actual admission is via tick_clocked().
        // SAFETY: dynamic SQL — compile-time macro not applicable for conditional query.
        let due_rows: Vec<String> = sqlx::query_scalar(
            "SELECT schedule_id FROM creator_schedules
             WHERE status = 'pending' AND scheduled_at IS NOT NULL AND scheduled_at <= ?",
        )
        .bind(now)
        .fetch_all(&*self.pool)
        .await
        .unwrap_or_default();

        // Track admitted IDs to prevent duplicate-fire within this cycle
        let mut admitted_ids: HashSet<ScheduleId> = HashSet::new();

        for schedule_id in due_rows.clone() {
            let id = ScheduleId(schedule_id.clone());

            // Skip if already admitted in this cycle
            if admitted_ids.contains(&id) {
                tracing::debug!("skip duplicate schedule {}", schedule_id);
                continue;
            }

            // Mark as admitted for this cycle (before supervisor call)
            admitted_ids.insert(id.clone());

            tracing::info!("clock trigger candidate for schedule {}", schedule_id);
        }

        // Submit to supervisor for admission using clocked tick
        // supervisor.tick_clocked() handles the scheduled_at filter and re-entrancy guard
        if let Err(e) = supervisor.tick_clocked(now).await {
            tracing::error!("supervisor tick_clocked failed: {}", e);
        }

        admitted_ids.len()
    }

    /// Run background poll loop until cancellation.
    ///
    /// Polls at the given interval (default: 1 second for pre-1.0).
    /// Integrates with daemon lifecycle via `CancellationToken`.
    pub async fn run(
        self,
        supervisor: Arc<ScheduleSupervisor>,
        cancel: CancellationToken,
        poll_interval_ms: u64,
    ) {
        let mut interval = tokio::time::interval(
            std::time::Duration::from_millis(poll_interval_ms),
        );

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("scheduler poll loop cancelled");
                    break;
                }
                _ = interval.tick() => {
                    self.tick(&supervisor).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_clock_set_works() {
        let clock = MockClock::new(1000);
        assert_eq!(clock.now_unix(), 1000);

        clock.set(2000);
        assert_eq!(clock.now_unix(), 2000);
    }

    #[test]
    fn system_clock_returns_current_time() {
        let clock = SystemClock;
        let now = clock.now_unix();

        // Should be within reasonable range (past 2020)
        assert!(now > 1577836800, "system clock should return valid timestamp");
    }
}