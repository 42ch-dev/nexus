//! Refresh-scheduler hook (V1.58 P1 — DF-44).
//!
//! Periodically scans `reference_sources` for stale rows (`on_change` or
//! overdue scheduled) and dispatches `nexus.reference.refresh` for each
//! candidate through the orchestration `CapabilityRegistry`.
//!
//! # Design
//!
//! - Non-blocking: the scheduler is a detached `tokio::spawn` task. All
//!   errors are logged at `warn!` level and never bubbled out.
//! - Default cadence: 3600s (1 hour), configurable via env var.
//! - First refresh cycle fires after 60s initial delay (avoids blocking
//!   daemon boot).
//! - Idempotent: sources with `refresh_status = 'refreshing'` are excluded
//!   to prevent concurrent refresh of the same source.
//! - tracing spans at each refresh attempt; metrics counters for
//!   total/success/failure.
//!
//! This mirrors the [`crate::stale_findings_watcher`] and
//! [`crate::cron_supervisor`] spawn patterns.

use std::sync::Arc;
use std::time::Duration;

use nexus_orchestration::capability::builtins::ReferenceRefresh;
use nexus_orchestration::capability::Capability;
use sqlx::SqlitePool;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Default refresh sweep cadence: 3600 seconds (1 hour).
pub const DEFAULT_REFRESH_INTERVAL_SECS: u64 = 3600;

/// Default stale threshold for scheduled sources: 86400 seconds (24 hours).
pub const DEFAULT_STALE_THRESHOLD_SECS: i64 = 86400;

/// Initial delay before the first refresh cycle: 60 seconds.
pub const INITIAL_DELAY_SECS: u64 = 60;

/// Env var overriding the refresh sweep cadence (seconds). Test-only knob.
pub const ENV_REFRESH_INTERVAL_SECS: &str = "NEXUS_DAEMON_REFRESH_SCHEDULER_INTERVAL_SECS";

/// Env var overriding the stale threshold (seconds). Test-only knob.
pub const ENV_REFRESH_STALE_THRESHOLD_SECS: &str =
    "NEXUS_DAEMON_REFRESH_SCHEDULER_STALE_THRESHOLD_SECS";

/// Resolved configuration for the refresh scheduler.
#[derive(Debug, Clone, Copy)]
pub struct RefreshSchedulerConfig {
    /// How often to scan for stale reference sources.
    pub interval: Duration,
    /// How old (in seconds since last refresh) a scheduled source must be
    /// to count as stale. Not used for `on_change` sources (always evaluated).
    pub stale_threshold_seconds: i64,
}

impl Default for RefreshSchedulerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(DEFAULT_REFRESH_INTERVAL_SECS),
            stale_threshold_seconds: DEFAULT_STALE_THRESHOLD_SECS,
        }
    }
}

impl RefreshSchedulerConfig {
    /// Build the config from defaults, allowing env overrides for tests.
    ///
    /// Invalid env values silently fall back to defaults — the scheduler
    /// must be best-effort and must never refuse to start.
    #[must_use]
    pub fn from_env() -> Self {
        let interval_secs = std::env::var(ENV_REFRESH_INTERVAL_SECS)
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_REFRESH_INTERVAL_SECS);
        let stale_threshold_seconds = std::env::var(ENV_REFRESH_STALE_THRESHOLD_SECS)
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_STALE_THRESHOLD_SECS);

        Self {
            interval: Duration::from_secs(interval_secs),
            stale_threshold_seconds,
        }
    }
}

/// Spawn the refresh scheduler background task.
///
/// Returns a `JoinHandle` so callers (boot, integration tests) can observe
/// completion; in production the handle is dropped and the task lives for the
/// daemon's lifetime or until `shutdown_notify` fires.
///
/// The first refresh cycle fires after a 60s initial delay to avoid
/// blocking daemon boot.
pub fn spawn_refresh_scheduler(
    pool: SqlitePool,
    shutdown_notify: Arc<Notify>,
    config: RefreshSchedulerConfig,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!(
            interval_secs = config.interval.as_secs(),
            stale_threshold_secs = config.stale_threshold_seconds,
            "refresh-scheduler task: waiting initial delay before first cycle"
        );

        // Initial delay — don't block daemon boot.
        tokio::time::sleep(Duration::from_secs(INITIAL_DELAY_SECS)).await;

        tracing::info!(
            interval_secs = config.interval.as_secs(),
            "refresh-scheduler task started"
        );

        let refresh_cap = ReferenceRefresh::with_pool(pool.clone());

        let mut ticker = tokio::time::interval(config.interval);
        // `Delay` keeps ticks spaced after a long pause (e.g. laptop sleep).
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    run_one_refresh_tick(&pool, &refresh_cap, config.stale_threshold_seconds).await;
                }
                () = shutdown_notify.notified() => {
                    tracing::info!("refresh-scheduler task: shutdown received, exiting");
                    break;
                }
            }
        }
    })
}

/// Perform a single refresh sweep tick. Public for hermetic integration tests
/// which drive the tick deterministically without running the spawned interval
/// loop.
pub async fn run_one_refresh_tick(
    pool: &SqlitePool,
    refresh_cap: &ReferenceRefresh,
    stale_threshold_seconds: i64,
) {
    // Find stale sources.
    let stale_sources = match nexus_local_db::reference_source::find_stale_sources(
        pool,
        Some(50), // max per tick
        stale_threshold_seconds,
    )
    .await
    {
        Ok(sources) => sources,
        Err(e) => {
            tracing::warn!(error = %e, "refresh-scheduler: find_stale_sources failed");
            return;
        }
    };

    if stale_sources.is_empty() {
        tracing::debug!("refresh-scheduler: no stale sources found");
        return;
    }

    tracing::info!(
        count = stale_sources.len(),
        "refresh-scheduler: dispatching refresh for stale sources"
    );

    let mut success_count = 0u64;
    let mut failure_count = 0u64;

    for source in &stale_sources {
        let input = serde_json::json!({
            "reference_source_id": source.reference_source_id,
        });

        match refresh_cap.run(input).await {
            Ok(result) => {
                let status = result
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                tracing::info!(
                    reference_source_id = %source.reference_source_id,
                    status = %status,
                    "refresh-scheduler: source refreshed"
                );
                success_count += 1;
            }
            Err(e) => {
                tracing::warn!(
                    reference_source_id = %source.reference_source_id,
                    error = %e,
                    "refresh-scheduler: source refresh failed"
                );
                failure_count += 1;
            }
        }
    }

    tracing::info!(
        total = stale_sources.len(),
        success = success_count,
        failure = failure_count,
        "refresh-scheduler: tick complete"
    );
}
