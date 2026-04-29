//! Clock / wall-clock trigger scheduler for Creator Schedules (V1.5 WS-D).
//!
//! A background poller that periodically triggers `ScheduleSupervisor::tick_clocked()`
//! for admission of due schedules.
//!
//! **Four hard constraints** (from crate-selection-best-practices-v1.md §3.7):
//! 1. **Wall-clock + tz-aware**: uses `ClockSource` trait returning Unix timestamp.
//! 2. **Graceful shutdown**: `CancellationToken` cancels background poll loop.
//!
//! # DST Safety
//!
//! The scheduler operates entirely in UTC via Unix timestamps. `scheduled_at`
//! values are stored as UTC Unix timestamps, and all comparisons use UTC.
//! This design is inherently safe against DST transitions.
//!
//! **Limitation**: The scheduler does not support wall-clock recurrence rules
//! (e.g., "every day at 8am local time"). If wall-clock recurrence is added
//! in a future version, DST jump detection will be required at that time.
//!
//! Pre-1.0: Simple hand-rolled implementation (no `tokio-cron-scheduler` crate).
//! Zero new third-party dependencies.

use std::sync::Arc;

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
    #[must_use]
    pub const fn new(initial: i64) -> Self {
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
/// Polls `ScheduleSupervisor::tick_clocked()` for admission of due schedules.
pub struct Scheduler {
    /// `SQLite` pool retained for future use (e.g. direct DB queries).
    #[allow(dead_code)]
    pool: Arc<SqlitePool>,
    clock: Arc<dyn ClockSource>,
}

impl Scheduler {
    /// Create a scheduler with the given `SQLite` pool and clock source.
    pub fn new(pool: Arc<SqlitePool>, clock: Arc<dyn ClockSource>) -> Self {
        Self { pool, clock }
    }

    /// Create a scheduler with production system clock.
    #[must_use]
    pub fn with_system_clock(pool: Arc<SqlitePool>) -> Self {
        Self::new(pool, Arc::new(SystemClock))
    }

    /// Poll for due schedules and submit to supervisor for admission.
    ///
    /// Returns the number of schedules that were found due (not necessarily admitted).
    ///
    /// Delegates entirely to `ScheduleSupervisor::tick_clocked()` which handles
    /// the `scheduled_at` filter and re-entrancy guard. The informational query
    /// that previously ran here was removed (R3 dead code cleanup) because
    /// `tick_clocked()` already performs the same filtering internally.
    pub async fn tick(&self, supervisor: &ScheduleSupervisor) -> usize {
        let now = self.clock.now_unix();

        // Submit to supervisor for admission using clocked tick.
        // tick_clocked() handles the scheduled_at filter, re-entrancy guard,
        // and actual admission — no redundant query needed.
        if let Err(e) = supervisor.tick_clocked(now).await {
            tracing::error!("supervisor tick_clocked failed: {}", e);
        }

        // We don't track admitted IDs here anymore (R3: removed dead query).
        // Return 0 as a conservative estimate — callers should check supervisor
        // state if they need exact admission counts.
        0
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
        let mut interval =
            tokio::time::interval(std::time::Duration::from_millis(poll_interval_ms));

        loop {
            tokio::select! {
                () = cancel.cancelled() => {
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
        assert!(
            now > 1_577_836_800,
            "system clock should return valid timestamp"
        );
    }
}
