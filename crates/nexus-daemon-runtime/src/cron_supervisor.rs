//! `cron_supervisor` — daemon background task for novel-writing cron staggering
//! (V1.50 T-A P1).
//!
//! Spec: `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §4.
//!
//! ## Role
//!
//! Spawns a periodic task (default 1-min interval) that, on each tick:
//! 1. Calls [`nexus_orchestration::schedule::cron_supervisor::evaluate_cron_fires`]
//!    to enqueue pending `Schedule`s for any per-Work role cron that matches the
//!    current minute (brainstorm + write in T-A P1).
//! 2. Calls [`nexus_orchestration::schedule::supervisor::ScheduleSupervisor::tick_clocked`]
//!    to admit due pending schedules (including the ones just enqueued).
//!
//! The task is detached (`tokio::spawn`) and lives for the daemon's lifetime.
//! All errors are logged and the loop continues — a single failed tick must
//! never crash the daemon. The task exits cleanly when `shutdown_notify` fires.
//!
//! This mirrors the [`crate::stale_findings_watcher`] spawn pattern (V1.39 P4).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use nexus_orchestration::schedule::cron_supervisor as cron_eval;
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;

/// Default cron-evaluation cadence: 60 seconds (spec §4.1: "1-min interval").
pub const DEFAULT_CRON_INTERVAL_SECS: u64 = 60;

/// Env var overriding the cadence (seconds). Test-only knob.
pub const ENV_CRON_INTERVAL_SECS: &str = "NEXUS_DAEMON_CRON_INTERVAL_SECS";

/// Resolved configuration for the cron supervisor task.
#[derive(Debug, Clone, Copy)]
pub struct CronSupervisorConfig {
    /// How often to evaluate per-Work crons + admit pending schedules.
    pub interval: Duration,
}

impl CronSupervisorConfig {
    /// Build the config from defaults, allowing env overrides for tests.
    ///
    /// Invalid env values silently fall back to the default — the task must
    /// be best-effort and must never refuse to start.
    #[must_use]
    pub fn from_env() -> Self {
        let interval_secs = std::env::var(ENV_CRON_INTERVAL_SECS)
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_CRON_INTERVAL_SECS);
        Self {
            interval: Duration::from_secs(interval_secs),
        }
    }
}

/// Spawn the cron supervisor background task.
///
/// Returns a `JoinHandle` so callers (boot, integration tests) can observe
/// completion; in production the handle is dropped and the task lives for the
/// daemon's lifetime or until `shutdown_notify` fires.
///
/// The first tick runs immediately on spawn so a freshly booted daemon
/// surfaces any due cron fires without waiting a full interval.
///
/// `workspace_dir` is cloned into the spawned task for file-lock path
/// construction (V1.51 T-B P0).
pub fn spawn_cron_supervisor(
    pool: SqlitePool,
    workspace_dir: std::path::PathBuf,
    supervisor: Arc<ScheduleSupervisor>,
    shutdown_notify: Arc<Notify>,
    config: CronSupervisorConfig,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!(
            interval_secs = config.interval.as_secs(),
            "cron-supervisor task started"
        );

        let mut ticker = tokio::time::interval(config.interval);
        // `Delay` keeps ticks spaced after a long pause (e.g. laptop sleep);
        // avoids piling cron evaluations onto the DB.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    run_one_tick(&pool, &workspace_dir, &supervisor).await;
                }
                () = shutdown_notify.notified() => {
                    tracing::info!("cron-supervisor task: shutdown received, exiting");
                    break;
                }
            }
        }
    })
}

/// Perform a single cron-evaluation + admission tick. Public for hermetic
/// integration tests which drive the tick deterministically without running
/// the spawned interval loop.
///
/// `workspace_dir` is the operational workspace directory
/// (e.g. `~/.nexus42/creators/<id>/workspaces/<slug>`) — passed through to
/// the cron evaluator for file-lock path construction (V1.51 T-B P0).
pub async fn run_one_tick(
    pool: &SqlitePool,
    workspace_dir: &Path,
    supervisor: &ScheduleSupervisor,
) {
    let now = chrono::Utc::now();
    // Step 1: evaluate per-Work crons → enqueue pending schedules.
    let summary = cron_eval::evaluate_cron_fires(pool, Some(workspace_dir), now).await;
    // Step 2: admit due pending schedules (including any just enqueued).
    // `tick_clocked` filters by `scheduled_at <= now` (cron schedules have no
    // `scheduled_at`, so they are on-demand-admissible).
    if summary.fired > 0 {
        // Only trigger admission when we actually enqueued something — avoids a
        // redundant full-table scan on idle ticks (the supervisor's tick_inner
        // loads all active schedules every call).
        if let Err(e) = supervisor.tick_clocked(now.timestamp()).await {
            tracing::warn!(
                error = %e,
                fired = summary.fired,
                "cron-supervisor task: admission tick failed (non-fatal)"
            );
        }
    }
}
