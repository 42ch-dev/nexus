//! Master-decision timeout watcher (V1.39 P4 T1).
//!
//! Periodically scans `findings` for open rows older than a threshold
//! (default 96h) and emits a structured `tracing::warn!` per stale row so
//! operators and the CLI status banner have a workspace-wide signal that a
//! finding has missed its master-decision SLA.
//!
//! Design notes:
//! - Non-blocking: the watcher is a detached `tokio::spawn` task. All
//!   database errors are logged at `error!` level and never bubbled out.
//! - Default cadence is 24h (one sweep per day) and the default threshold
//!   is 96h, matching `.mstar/specs/novel-quality-loop.md`. Both are
//!   overridable via env vars so the hermetic integration test in
//!   `crates/nexus-daemon-runtime/tests/master_decision_timeout.rs` can
//!   drive a sub-second cadence with a tiny threshold.
//! - The watcher only **reports** stale findings. Auto-scheduling
//!   `review-master` is gated by the per-Work opt-in flag introduced in
//!   T4 and is performed elsewhere (see T1 follow-up in P4).
//!
//! This module deliberately depends only on `nexus_local_db::findings`
//! T2 DAOs (`list_all_stale_open_findings`) — it does **not** add any new
//! SQL queries, so no `.sqlx/` regeneration is needed for this task.

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Default sweep cadence: 24 hours.
pub const DEFAULT_SWEEP_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// Default stale threshold: 96 hours (V1.39 master-decision SLA).
pub const DEFAULT_STALE_THRESHOLD_SECS: i64 = 96 * 60 * 60;

/// Env var overriding the sweep cadence (seconds). Test-only knob.
pub const ENV_SWEEP_INTERVAL_SECS: &str = "NEXUS_DAEMON_STALE_FINDINGS_INTERVAL_SECS";

/// Env var overriding the stale threshold (seconds). Test-only knob.
pub const ENV_STALE_THRESHOLD_SECS: &str = "NEXUS_DAEMON_STALE_FINDINGS_THRESHOLD_SECS";

/// Resolved configuration for the stale-findings watcher.
#[derive(Debug, Clone, Copy)]
pub struct StaleFindingsWatcherConfig {
    /// How often to scan the `findings` table.
    pub interval: Duration,
    /// How old (in seconds) an open finding must be to count as stale.
    pub threshold_seconds: i64,
}

impl StaleFindingsWatcherConfig {
    /// Build the config from defaults, allowing env overrides for tests.
    ///
    /// Invalid env values silently fall back to defaults — the watcher
    /// must be best-effort and must never refuse to start.
    #[must_use]
    pub fn from_env() -> Self {
        let interval_secs = std::env::var(ENV_SWEEP_INTERVAL_SECS)
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_SWEEP_INTERVAL_SECS);

        let threshold_seconds = std::env::var(ENV_STALE_THRESHOLD_SECS)
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_STALE_THRESHOLD_SECS);

        Self {
            interval: Duration::from_secs(interval_secs),
            threshold_seconds,
        }
    }
}

/// Spawn the master-decision timeout watcher.
///
/// Returns a `JoinHandle` so callers (the boot path, integration tests)
/// can observe completion; in production the handle is dropped and the
/// task lives for the lifetime of the process or until `shutdown_notify`
/// fires.
///
/// The first sweep runs immediately on spawn so a freshly booted daemon
/// surfaces existing stale findings without waiting a full interval.
///
/// # Behavior
///
/// On every tick:
/// 1. Read the current UNIX epoch.
/// 2. Call [`nexus_local_db::findings::list_all_stale_open_findings`].
/// 3. Emit one `tracing::warn!` per stale row with
///    `finding_id`, `work_id`, `creator_id`, `severity`, `age_seconds`.
/// 4. Errors emit `tracing::error!` and the loop continues.
///
/// The task exits cleanly when `shutdown_notify.notified()` fires.
pub fn spawn_stale_findings_watcher(
    pool: SqlitePool,
    shutdown_notify: Arc<Notify>,
    config: StaleFindingsWatcherConfig,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!(
            interval_secs = config.interval.as_secs(),
            threshold_seconds = config.threshold_seconds,
            "stale-findings watcher started"
        );

        let mut ticker = tokio::time::interval(config.interval);
        // Default `MissedTickBehavior::Burst` would cause back-to-back
        // sweeps after a long pause (e.g. laptop sleep). `Delay` keeps
        // sweeps spaced and avoids piling work on the DB.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    run_one_sweep(&pool, config.threshold_seconds).await;
                }
                () = shutdown_notify.notified() => {
                    tracing::info!("stale-findings watcher: shutdown received, exiting");
                    break;
                }
            }
        }
    })
}

/// Perform a single sweep. Public for the hermetic integration test
/// (V1.39 P4 T5) which drives the sweep deterministically without
/// running the spawned interval loop.
pub async fn run_one_sweep(pool: &SqlitePool, threshold_seconds: i64) {
    let now_epoch = current_epoch_seconds();

    match nexus_local_db::findings::list_all_stale_open_findings(pool, now_epoch, threshold_seconds)
        .await
    {
        Ok(stale) => {
            if stale.is_empty() {
                tracing::debug!(
                    threshold_seconds,
                    "stale-findings sweep: no findings past master-decision timeout"
                );
                return;
            }
            tracing::warn!(
                count = stale.len(),
                threshold_seconds,
                "stale-findings sweep: open findings past master-decision timeout"
            );
            for row in &stale {
                tracing::warn!(
                    finding_id = %row.finding_id,
                    work_id = %row.work_id,
                    creator_id = %row.creator_id,
                    severity = %row.severity,
                    age_seconds = row.age_seconds,
                    "stale open finding past master-decision timeout"
                );
            }
        }
        Err(e) => {
            // Non-blocking: log and continue. The watcher must never
            // bubble errors — that would crash the spawned task and
            // silently stop the SLA signal.
            tracing::error!(
                error = %e,
                threshold_seconds,
                "stale-findings sweep: database query failed"
            );
        }
    }
}

/// Current UNIX epoch in seconds.
///
/// Falls back to `0` if the system clock is before the UNIX epoch — that
/// is impossible on a sane host but the watcher must never panic.
fn current_epoch_seconds() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}
