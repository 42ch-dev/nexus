//! Hosted scheduler wake/clock task (v1.195 P0-T2).
//!
//! The hosted production owner composes exactly ONE retained task for schedule
//! admission: on every clock tick it drives the injected
//! [`ScheduleSupervisor`], whose tick admits eligible durable pending
//! `driven_v1` rows through the coordinator-backed
//! [`nexus_orchestration::schedule::supervisor::ScheduleRunStarter`] — i.e.
//! through the atomic schedule→run claim, never a status-only flip.
//!
//! Why a task rather than a caller: admission is asynchronous by contract
//! (current-host contracts §3.3 — "asynchronous admission may still be
//! pending"). The row is durable before `add_schedule` answers; this clock is
//! what later admits it, whether the row was inserted by a public call or by a
//! `scheduled_at`/dependency gate that only became satisfiable later.
//!
//! Ordering, both load-bearing:
//!
//! - The owner installs the task and the supervisor **BEFORE** recovery runs
//!   (so a recovered terminal run settles through the attached supervisor and
//!   the starter is present for any re-drive), and signals
//!   [`spawn`]'s `start` handle only AFTER recovery completed. Until that
//!   signal the clock does not tick at all, so an admission sweep can never
//!   race the recovery pass.
//! - The owner's `close` fires the shutdown handle and JOINS the returned
//!   handle, so no tick outlives the owner's drain.
//!
//! The task is strictly bounded work per tick: the supervisor's own
//! re-entrancy guard and single-flight admission make a slow tick skip
//! overlapping ticks rather than stack them. Unrelated historical jobs (cron
//! staggering, refresh, SOUL) are NOT started here — this slice owns schedule
//! admission only.

use std::sync::Arc;
use std::time::Duration;

use nexus_orchestration::schedule::supervisor::{ScheduleSupervisor, SupervisorError};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Default cadence for the hosted supervisor clock.
///
/// One second: this is the admission clock a just-inserted durable pending
/// schedule waits for, so it must be short enough to read as immediate to a
/// user, and the tick itself is bounded (two indexed reads plus one admission
/// decision per eligible row).
pub const DEFAULT_HOSTED_SCHEDULER_INTERVAL_SECS: u64 = 1;

/// Env var overriding the cadence (seconds). Test/boot knob.
pub const ENV_HOSTED_SCHEDULER_INTERVAL_SECS: &str = "NEXUS_HOSTED_SCHEDULER_INTERVAL_SECS";

/// Resolved cadence for the owned supervisor clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostedSchedulerConfig {
    /// How often to admit eligible durable pending schedules.
    pub interval: Duration,
}

impl Default for HostedSchedulerConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(DEFAULT_HOSTED_SCHEDULER_INTERVAL_SECS),
        }
    }
}

impl HostedSchedulerConfig {
    /// Build a config with an explicit cadence.
    #[must_use]
    pub const fn new(interval: Duration) -> Self {
        Self { interval }
    }

    /// Build the config from defaults, allowing an env override.
    ///
    /// An invalid or zero value silently falls back to the default — the
    /// scheduler must never refuse to start because of a bad knob.
    #[must_use]
    pub fn from_env() -> Self {
        let interval_secs = std::env::var(ENV_HOSTED_SCHEDULER_INTERVAL_SECS)
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_HOSTED_SCHEDULER_INTERVAL_SECS);
        Self::new(Duration::from_secs(interval_secs))
    }
}

/// Drive ONE supervisor clock tick.
///
/// This is the exact work the spawned loop performs per tick, exposed so the
/// tick can be driven deterministically (tests, and any future bounded caller)
/// without waiting on the interval.
///
/// Uses `tick_clocked`, not the on-demand `tick`: a row whose `scheduled_at`
/// is still in the future must stay pending, and the supervisor's own store
/// re-verifies the same gate inside the claim transaction.
///
/// # Errors
/// Returns the supervisor's error when the durable admission sweep fails; the
/// spawned loop logs it and keeps ticking (a failed tick must never end the
/// owner's clock).
pub async fn run_one_tick(supervisor: &ScheduleSupervisor) -> Result<(), SupervisorError> {
    supervisor
        .tick_clocked(chrono::Utc::now().timestamp())
        .await
}

/// Spawn the owner's ONE supervisor wake/clock task.
///
/// Returns the task handle the owner must join on close. `start` is signalled
/// by the owner once recovery has completed; `shutdown` is fired by the
/// owner's close so the loop exits promptly instead of waiting out an
/// interval.
#[must_use]
pub fn spawn(
    supervisor: Arc<ScheduleSupervisor>,
    start: Arc<Notify>,
    shutdown: Arc<Notify>,
    config: HostedSchedulerConfig,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!(
            interval_secs = config.interval.as_secs(),
            "hosted scheduler task installed; waiting for the owner's post-recovery start"
        );
        // The owner installs this task before recovery: no admission sweep may
        // run until recovery has classified the durable runs.
        tokio::select! {
            biased;
            () = shutdown.notified() => return,
            () = start.notified() => {}
        }

        let mut ticker = tokio::time::interval(config.interval);
        // `Delay` keeps ticks spaced after a long pause (laptop sleep) instead
        // of firing a burst of sweeps at the DB.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // `interval`'s first tick is immediate; the clock is measured from the
        // owner's start signal, so consume it and wait a full cadence.
        ticker.tick().await;

        loop {
            tokio::select! {
                biased;
                () = shutdown.notified() => {
                    tracing::info!("hosted scheduler task: shutdown received, exiting");
                    break;
                }
                _ = ticker.tick() => {
                    if let Err(e) = run_one_tick(&supervisor).await {
                        tracing::warn!(
                            error = %e,
                            "hosted scheduler tick failed (non-fatal; the next tick retries)"
                        );
                    }
                }
            }
        }
    })
}
