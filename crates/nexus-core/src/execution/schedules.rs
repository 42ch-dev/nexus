//! Scheduler and background-effect ownership (v1.190 P3-T2).
//!
//! These four tasks moved out of the daemon unchanged in behavior. They own
//! the periodic domain effects the transport used to drive — no TypeScript
//! timer performs a domain effect any more.
//!
//! - [`cron`] — per-Work cron staggering and due-schedule admission
//!   ([`cron::spawn_cron_supervisor`]). Admission routes through the injected
//!   [`nexus_orchestration::schedule::supervisor::ScheduleSupervisor`], which
//!   gates on STORED Actor admission rather than a global "active" flag.
//! - [`stale_findings`] — the master-decision timeout watcher
//!   ([`stale_findings::spawn_stale_findings_watcher`]) and its opt-in
//!   `novel-review-master` enqueue.
//! - [`refresh`] — the stale reference-source refresh sweep
//!   ([`refresh::spawn_refresh_scheduler`]).
//! - [`chronology`] — per-Work volume auto-advance
//!   ([`chronology::spawn_auto_chronology_tick`]).
//! - [`hosted_scheduler`] — the v1.195 P0-T2 hosted owner's ONE supervisor
//!   wake/clock task (and its `HostedSchedulerConfig`), which admits eligible
//!   durable pending schedules through the coordinator-backed starter. Unlike
//!   the four tasks above it is not daemon-era behavior moved here: it is the
//!   clock the hosted production factory installs over its own supervisor.
//!
//! Every task is a detached `tokio::spawn` owned by its caller (the daemon
//! boot composition), is best-effort (a failed tick logs and continues), and
//! exits cleanly on the supplied shutdown [`tokio::sync::Notify`]. Each also
//! exposes a `run_one_*` entry point so tests drive a deterministic tick
//! without the interval loop.
//!
//! `cron::run_one_tick` and `chronology::run_one_tick` deliberately keep their
//! original names in their own modules and are NOT re-exported here — both
//! lived at distinct daemon module paths before the move and callers keep
//! naming them by module.

pub mod chronology;
pub mod cron;
pub mod hosted_scheduler;
pub mod refresh;
pub mod stale_findings;

pub use chronology::{AutoChronologyConfig, DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS};
pub use cron::{CronSupervisorConfig, DEFAULT_CRON_INTERVAL_SECS};
pub use hosted_scheduler::{
    HostedSchedulerConfig, DEFAULT_HOSTED_SCHEDULER_INTERVAL_SECS,
    ENV_HOSTED_SCHEDULER_INTERVAL_SECS,
};
pub use refresh::{RefreshSchedulerConfig, DEFAULT_REFRESH_INTERVAL_SECS};
pub use stale_findings::{StaleFindingsWatcherConfig, DEFAULT_SWEEP_INTERVAL_SECS};
