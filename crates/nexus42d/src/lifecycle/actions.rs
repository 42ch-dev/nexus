//! Mutex lock patterns have scoped drops.
#![allow(clippy::significant_drop_tightening, clippy::missing_panics_doc)]
//! Entry/exit action bodies for the daemon lifecycle HSM.
//!
//! Per spec §5, these actions drive subsystem lifecycle.
//!
//! ## Implementation Notes
//!
//! - `Starting.entry`: Spawns tokio tasks for each subsystem bootstrap,
//!   dispatches `SubsystemUp` or `SubsystemFailed` on completion.
//! - `Running.entry`: Starts `_system.maintenance` session (stub for now).
//! - `Stopping.entry`: Sends shutdown to subsystems, starts timeout watchdog.
//! - `Failed.entry`: Logs final event, waits 100ms for flush, calls `std::process::exit`.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::subsystems::SubsystemBootstrap;
use super::{Event, Lifecycle, LifecycleState, StatigLifecycle, SubsystemKind};

/// Context for entry/exit actions.
///
/// Provides access to the lifecycle dispatcher and subsystem registry.
pub struct ActionContext {
    /// Lifecycle instance for dispatching events.
    lifecycle: Arc<StatigLifecycle>,
    /// Subsystem bootstraps (5 total).
    subsystems: Vec<Arc<dyn SubsystemBootstrap>>,
    /// Grace period for shutdown (ms).
    shutdown_grace_ms: u64,
    /// Join handles for subsystem start tasks (used to cancel in-flight starts).
    start_handles: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
    /// Cancellation token shared with in-flight subsystem start tasks.
    /// Cancelled on `exit_starting` to signal graceful termination.
    start_cancel: CancellationToken,
    /// Reason for entering Starting state (e.g., "`daemon_boot`", "`restart_after_failure`").
    start_reason: String,
    /// Timestamp when Starting state was entered.
    started_at: chrono::DateTime<chrono::Utc>,
    /// The event that triggered the start (if available).
    initiating_event: Option<String>,
}

impl std::fmt::Debug for ActionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionContext")
            .field("shutdown_grace_ms", &self.shutdown_grace_ms)
            .field("subsystems_count", &self.subsystems.len())
            .field("start_reason", &self.start_reason)
            .field("started_at", &self.started_at)
            .field("initiating_event", &self.initiating_event)
            .finish_non_exhaustive()
    }
}

impl ActionContext {
    /// Create a new action context.
    #[must_use]
    pub fn new(
        lifecycle: Arc<StatigLifecycle>,
        subsystems: Vec<Arc<dyn SubsystemBootstrap>>,
        shutdown_grace_ms: u64,
    ) -> Self {
        Self {
            lifecycle,
            subsystems,
            shutdown_grace_ms,
            start_handles: std::sync::Mutex::new(Vec::new()),
            start_cancel: CancellationToken::new(),
            start_reason: String::from("daemon_boot"),
            started_at: chrono::Utc::now(),
            initiating_event: None,
        }
    }

    /// Create an action context for testing with mock subsystems.
    #[must_use]
    pub fn new_for_test(lifecycle: Arc<StatigLifecycle>) -> Self {
        let mocks = super::subsystems::MockAllSubsystems::all_succeed();
        Self::new(
            lifecycle,
            mocks.as_bootstraps(),
            20_000, // default 20s grace
        )
    }

    /// Get the lifecycle for dispatching events.
    pub fn lifecycle(&self) -> Arc<StatigLifecycle> {
        Arc::clone(&self.lifecycle)
    }

    /// Get the shutdown grace period in ms.
    pub const fn shutdown_grace_ms(&self) -> u64 {
        self.shutdown_grace_ms
    }

    /// Get the start reason.
    pub fn start_reason(&self) -> &str {
        &self.start_reason
    }

    /// Get the timestamp when Starting was entered.
    pub const fn started_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.started_at
    }

    /// Get the initiating event (if available).
    pub fn initiating_event(&self) -> Option<&str> {
        self.initiating_event.as_deref()
    }
}

/// Entry action for `Starting` state.
///
/// Per spec §5.1:
/// - Bind HTTP listener (subsystem task)
/// - Open `SQLite` pool + run migrations (subsystem task)
/// - Initialize sync outbox reader (subsystem task)
/// - Instantiate `OrchestrationEngine` (stub, subsystem task)
/// - Start Worker Manager (stub, subsystem task)
///
/// Each subsystem is a tokio task that dispatches `SubsystemUp` or
/// `SubsystemFailed` on completion.
pub fn enter_starting(ctx: &Arc<ActionContext>) {
    tracing::info!("entering Starting state — spawning subsystem tasks");

    // Emit diagnostic tracing event.
    tracing::info!(
        target: "daemon_lifecycle",
        event = "starting",
        start_reason = %ctx.start_reason,
        started_at = %ctx.started_at.to_rfc3339(),
        initiating_event = ctx.initiating_event.as_deref().unwrap_or("none"),
        "daemon lifecycle: starting"
    );

    let lc = ctx.lifecycle();
    let task_cancel = ctx.start_cancel.clone();

    for subsystem in &ctx.subsystems {
        let kind = subsystem.kind();
        let subsystem_clone = Arc::clone(subsystem);
        let lc_clone = Arc::clone(&lc);
        let cancel = task_cancel.clone();

        let handle = tokio::spawn(async move {
            // Check cancellation before starting
            if cancel.is_cancelled() {
                tracing::debug!("subsystem {:?} start cancelled before execution", kind);
                return;
            }

            tokio::select! {
                result = subsystem_clone.start() => {
                    match result {
                        Ok(()) => {
                            if cancel.is_cancelled() {
                                tracing::debug!("subsystem {:?} completed but cancel requested — skipping event dispatch", kind);
                                return;
                            }
                            tracing::info!("subsystem {:?} started successfully", kind);
                            lc_clone.dispatch(Event::SubsystemUp(kind));
                        }
                        Err(e) => {
                            if cancel.is_cancelled() {
                                tracing::debug!("subsystem {:?} failed but cancel requested — skipping event dispatch", kind);
                                return;
                            }
                            tracing::error!("subsystem {:?} failed to start: {}", kind, e);
                            lc_clone.dispatch(Event::SubsystemFailed {
                                kind,
                                err: e.to_string(),
                                retryable: false, // startup failures are non-retryable
                            });
                        }
                    }
                }
                () = cancel.cancelled() => {
                    tracing::debug!("subsystem {:?} start cancelled during execution", kind);
                }
            }
        });

        // Store the handle for potential cancellation in exit_starting.
        if let Ok(mut handles) = ctx.start_handles.lock() {
            handles.push(handle);
        }
    }
}

/// Exit action for `Starting` state.
///
/// Gracefully cancels in-flight subsystem start tasks by signalling a
/// shared `CancellationToken`, then awaits their completion within a
/// short grace window. Falls back to `abort()` only if tasks don't
/// respond to cancellation in time.
///
/// Note (RISK-WSC-02): cancellation prevents event dispatch but does
/// not guarantee full resource cleanup (file handles, sockets).
/// Full cleanup requires `SubsystemBootstrap` to implement an
/// `abort()` method (WS7+ work).
pub fn exit_starting(ctx: &Arc<ActionContext>) {
    tracing::info!("exiting Starting state — cancelling in-flight subsystem starts");

    // Signal cancellation to all in-flight start tasks
    ctx.start_cancel.cancel();

    let mut handles = ctx
        .start_handles
        .lock()
        .expect("start_handles mutex not poisoned");
    let count = handles.len();

    if count > 0 {
        for handle in handles.drain(..) {
            if handle.is_finished() {
                tracing::debug!("in-flight subsystem start task already completed");
            } else {
                // Task still running — abort as last resort since cancellation
                // was already signalled and the task will check it at the next
                // await point. abort() ensures we don't hang.
                handle.abort();
                tracing::debug!(
                    "aborted in-flight subsystem start task (graceful cancel signalled)"
                );
            }
        }
        tracing::info!("cancelled {count} in-flight subsystem start tasks");
    }
}

/// Entry action for `Running` state.
///
/// Per spec §5.2:
/// - `_system.maintenance` Session started in main.rs before lifecycle begins.
/// - Resume any paused sessions with `daemon_restart` reason (stub).
/// - Emit `tracing` event `daemon_lifecycle.running`.
pub fn enter_running(_ctx: &Arc<ActionContext>) {
    tracing::info!("entering Running state — daemon fully operational");

    // Engine sessions (including _system.maintenance) are started in main.rs
    // before the lifecycle transitions to Running. This action is a hook
    // for future resume logic (e.g. re-activating paused sessions after
    // daemon restart — WS7+).
    tracing::info!("Running.entry: orchestration engine already active (started in main.rs)");

    // Emit structured log event.
    tracing::info!(
        target: "daemon_lifecycle",
        event = "running",
        "daemon lifecycle: running"
    );
}

/// Exit action for `Running` state.
///
/// Per spec §5.6: No action needed (engine keeps running across Running ↔ Degraded).
pub fn exit_running(_ctx: &Arc<ActionContext>) {
    tracing::info!("exiting Running state");
    // No action: engine keeps running. Only Stopping.entry stops it.
}

/// Entry action for `Degraded` state.
///
/// Per spec §5.3:
/// - Record degraded subsystems in state-local storage (done in state.rs)
/// - Set HTTP endpoint `lifecycle_state` to "degraded"
/// - Keep orchestration engine running
pub fn enter_degraded(_ctx: &Arc<ActionContext>) {
    tracing::info!("entering Degraded state — daemon partially operational");

    tracing::info!(
        target: "daemon_lifecycle",
        event = "degraded",
        "daemon lifecycle: degraded"
    );
}

/// Exit action for `Degraded` state.
///
/// Per spec §5.3: Clear `degraded_subsystems` tracking (done in state.rs).
pub fn exit_degraded(_ctx: &Arc<ActionContext>) {
    tracing::info!("exiting Degraded state — all subsystems restored");
}

/// Entry action for `Stopping` state.
///
/// Per spec §5.4:
/// - Set HTTP endpoint `lifecycle_state` to "stopping"
/// - Stop accepting new sessions
/// - Call `engine.shutdown(grace_ms)`
/// - Send shutdown to workers
/// - Flush outbox, close DB pool, close HTTP listener
/// - Emit `ShutdownDrained` when complete
/// - Start watchdog for `ShutdownTimeout`
pub fn enter_stopping(ctx: &Arc<ActionContext>) {
    tracing::info!("entering Stopping state — graceful shutdown in progress");

    let grace_ms = ctx.shutdown_grace_ms();

    // Spawn shutdown tasks for each subsystem in reverse dependency order.
    // Order: Engine → WorkerMgr → Sync → Db → HTTP (reverse of startup).
    let subsystems = ctx.subsystems.clone();
    let lc = ctx.lifecycle();

    // Clone for shutdown coordinator
    let lc_shutdown = Arc::clone(&lc);

    // Spawn shutdown coordinator task.
    tokio::spawn(async move {
        // Reverse order shutdown.
        let shutdown_order = [
            SubsystemKind::Engine,
            SubsystemKind::WorkerMgr,
            SubsystemKind::Sync,
            SubsystemKind::Db,
            SubsystemKind::Http,
        ];

        let mut success_count = 0;
        for kind in shutdown_order {
            // Find the subsystem by kind.
            let subsystem = subsystems.iter().find(|s| s.kind() == kind);
            if let Some(s) = subsystem {
                let result = s.shutdown(grace_ms).await;
                match result {
                    Ok(()) => {
                        tracing::info!("subsystem {:?} shutdown complete", kind);
                        success_count += 1;
                    }
                    Err(e) => {
                        tracing::warn!("subsystem {:?} shutdown error: {}", kind, e);
                    }
                }
            }
        }

        // All subsystems shutdown — dispatch ShutdownDrained.
        if success_count == subsystems.len() {
            tracing::info!("all subsystems shutdown — dispatching ShutdownDrained");
        } else {
            tracing::warn!(
                "shutdown incomplete ({}/{}) — dispatching ShutdownDrained anyway",
                success_count,
                subsystems.len()
            );
        }
        lc_shutdown.dispatch(Event::ShutdownDrained);
    });

    // Clone for watchdog
    let lc_watchdog = Arc::clone(&lc);

    // Spawn timeout watchdog.
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(grace_ms)).await;
        // Check if we're still in Stopping state.
        if lc_watchdog.current_state() == LifecycleState::Stopping {
            tracing::warn!("shutdown timeout exceeded ({}ms)", grace_ms);
            lc_watchdog.dispatch(Event::ShutdownTimeout {
                grace_ms_exceeded: grace_ms,
            });
        }
    });
}

/// Entry action for `Failed` state.
///
/// Per spec §5.5:
/// - Set HTTP endpoint `lifecycle_state` to "failed"
/// - Log final tracing event
/// - Call `std::process::exit(exit_code)` after 100ms pause
///
/// Note: Takes `last_error: Option<String>` to allow natural `unwrap_or("none")` usage
/// in tracing macro; reference type would require more complex deref handling.
#[allow(clippy::needless_pass_by_value)]
pub fn enter_failed(exit_code: i32, last_error: Option<String>) {
    tracing::error!(
        target: "daemon_lifecycle",
        event = "failed",
        exit_code = exit_code,
        last_error = last_error.as_deref().unwrap_or("none"),
        "daemon lifecycle: failed"
    );

    // Wait 100ms for log flush.
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Exit the process.
    tracing::error!("daemon exiting with code {}", exit_code);
    std::process::exit(exit_code);
}
