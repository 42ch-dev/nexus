//! `auto_chronology` — daemon background task for per-Work volume auto-advance
//! on finish (V1.50 T-A P3).
//!
//! Spec: `.mstar/knowledge/specs/novel-writing/auto-chronology.md` §3 / §4.
//!
//! ## Role
//!
//! Spawns a periodic task (default 5-min interval) that, on each tick, calls
//! [`nexus_orchestration::auto_chronology::run_one_tick`] to scan Works with
//! `auto_chronology = true`, run finish detection, and auto-create the next
//! volume outline + seed + chronology log for each eligible Work.
//!
//! The task is detached (`tokio::spawn`) and lives for the daemon's lifetime.
//! All errors are logged and the loop continues — a single failed tick must
//! never crash the daemon. The task exits cleanly when `shutdown_notify` fires.
//!
//! This mirrors the [`crate::cron_supervisor`] / [`crate::stale_findings_watcher`]
//! spawn pattern (V1.39 P4 / V1.50 T-A P1).
//!
//! ## Workspace path
//!
//! The advance writes the next-volume outline under the daemon's workspace
//! directory (`Works/<work_ref>/Outlines/...`). When `workspace_dir` is `None`
//! (hermetic DB-only tests) the orchestration tick still scans the DB but
//! outline/log writes are skipped inside the advance — the advance returns an
//! error per Work, which is logged and non-fatal. Production always passes a
//! real workspace path.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Default auto-chronology tick cadence: 5 minutes (spec §3).
pub const DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS: u64 = 5 * 60;

/// Env var overriding the cadence (minutes). Test-only knob.
pub const ENV_AUTO_CHRONOLOGY_INTERVAL_MIN: &str = "NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN";

/// Resolved configuration for the auto-chronology task.
#[derive(Debug, Clone, Copy)]
pub struct AutoChronologyConfig {
    /// How often to scan opted-in Works for finish detection.
    pub interval: Duration,
}

impl AutoChronologyConfig {
    /// Build the config from defaults, allowing env overrides for tests.
    ///
    /// Invalid env values silently fall back to the default — the task must
    /// be best-effort and must never refuse to start.
    #[must_use]
    pub fn from_env() -> Self {
        // Spec §3 names the env in minutes (`NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN`).
        let env_value = std::env::var(ENV_AUTO_CHRONOLOGY_INTERVAL_MIN)
            .ok()
            .map(|s| s.as_str().to_string());
        let interval_secs = parse_interval_secs(env_value.as_deref());
        Self {
            interval: Duration::from_secs(interval_secs),
        }
    }
}

/// Pure parsing of the `NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN` env override into
/// an interval in seconds (V1.50 §3).
///
/// Extracted from [`AutoChronologyConfig::from_env`] so tests can exercise the
/// parsing logic hermetically — passing the env value as a parameter instead of
/// mutating the process-global env var (which is unsafe under parallel test
/// execution; V1.49 R-V149P1-02 flake pattern).
///
/// Rules (match `from_env`'s historical behaviour):
/// - `None` → default (5 min).
/// - `Some(s)` where `s` parses to a positive `u64` → `s * 60` seconds.
/// - Unparseable / non-positive → default (5 min).
#[must_use]
pub fn parse_interval_secs(env_value: Option<&str>) -> u64 {
    env_value
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .map_or(DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS, |minutes| {
            minutes * 60
        })
}

/// Spawn the auto-chronology background task.
///
/// Returns a `JoinHandle` so callers (boot, integration tests) can observe
/// completion; in production the handle is dropped and the task lives for the
/// daemon's lifetime or until `shutdown_notify` fires.
///
/// The first tick runs immediately on spawn so a freshly booted daemon
/// surfaces any due advances without waiting a full interval.
pub fn spawn_auto_chronology_tick(
    pool: SqlitePool,
    workspace_dir: Option<PathBuf>,
    shutdown_notify: Arc<Notify>,
    config: AutoChronologyConfig,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!(
            interval_secs = config.interval.as_secs(),
            has_workspace = workspace_dir.is_some(),
            "auto-chronology task started"
        );

        let mut ticker = tokio::time::interval(config.interval);
        // `Delay` keeps ticks spaced after a long pause (e.g. laptop sleep);
        // avoids piling advance evaluations onto the DB.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    run_one_tick(&pool, workspace_dir.as_deref()).await;
                }
                () = shutdown_notify.notified() => {
                    tracing::info!("auto-chronology task: shutdown received, exiting");
                    break;
                }
            }
        }
    })
}

/// Perform a single auto-chronology tick. Public for hermetic integration tests
/// which drive the tick deterministically without running the spawned loop.
///
/// Thin wrapper over [`nexus_orchestration::auto_chronology::run_one_tick`];
/// the orchestration layer owns scan + finish detection + advance.
pub async fn run_one_tick(pool: &SqlitePool, workspace_dir: Option<&std::path::Path>) {
    // Use an empty path sentinel when the caller has no workspace (hermetic
    // DB-only tests). The orchestration advance will fail its outline write
    // per-Work and log the error non-fatally; production always passes a real
    // workspace path.
    let ws: std::path::PathBuf = workspace_dir.map_or_else(
        || std::path::PathBuf::from("/__nexus_no_workspace__"),
        std::path::PathBuf::from,
    );
    nexus_orchestration::auto_chronology::run_one_tick(pool, &ws).await;
}
