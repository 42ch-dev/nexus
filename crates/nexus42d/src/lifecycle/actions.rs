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
}

impl std::fmt::Debug for ActionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionContext")
            .field("shutdown_grace_ms", &self.shutdown_grace_ms)
            .field("subsystems_count", &self.subsystems.len())
            .finish_non_exhaustive()
    }
}

impl ActionContext {
    /// Create a new action context.
    pub fn new(
        lifecycle: Arc<StatigLifecycle>,
        subsystems: Vec<Arc<dyn SubsystemBootstrap>>,
        shutdown_grace_ms: u64,
    ) -> Self {
        Self {
            lifecycle,
            subsystems,
            shutdown_grace_ms,
        }
    }

    /// Create an action context for testing with mock subsystems.
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
    pub fn shutdown_grace_ms(&self) -> u64 {
        self.shutdown_grace_ms
    }
}

/// Entry action for `Starting` state.
///
/// Per spec §5.1:
/// - Bind HTTP listener (subsystem task)
/// - Open SQLite pool + run migrations (subsystem task)
/// - Initialize sync outbox reader (subsystem task)
/// - Instantiate OrchestrationEngine (stub, subsystem task)
/// - Start Worker Manager (stub, subsystem task)
///
/// Each subsystem is a tokio task that dispatches `SubsystemUp` or
/// `SubsystemFailed` on completion.
pub fn enter_starting(ctx: Arc<ActionContext>) {
    tracing::info!("entering Starting state — spawning subsystem tasks");

    let lc = ctx.lifecycle();

    for subsystem in &ctx.subsystems {
        let kind = subsystem.kind();
        let subsystem_clone = Arc::clone(subsystem);
        let lc_clone = Arc::clone(&lc);

        tokio::spawn(async move {
            let result = subsystem_clone.start().await;
            match result {
                Ok(()) => {
                    tracing::info!("subsystem {:?} started successfully", kind);
                    lc_clone.dispatch(Event::SubsystemUp(kind));
                }
                Err(e) => {
                    tracing::error!("subsystem {:?} failed to start: {}", kind, e);
                    lc_clone.dispatch(Event::SubsystemFailed {
                        kind,
                        err: e.to_string(),
                        retryable: false, // startup failures are non-retryable
                    });
                }
            }
        });
    }
}

/// Exit action for `Starting` state.
///
/// Per spec §5.6: Cancel any in-flight subsystem start tasks.
/// (For now, we rely on the tasks completing or the state transition
/// making their events irrelevant.)
pub fn exit_starting(_ctx: Arc<ActionContext>) {
    tracing::info!("exiting Starting state — in-flight starts will complete or be ignored");
    // In a full implementation, we would cancel the tokio tasks here.
    // For now, rely on state transition making their events irrelevant.
}

/// Entry action for `Running` state.
///
/// Per spec §5.2:
/// - `_system.maintenance` Session started in main.rs before lifecycle begins.
/// - Resume any paused sessions with `daemon_restart` reason (stub).
/// - Emit `tracing` event `daemon_lifecycle.running`.
pub fn enter_running(_ctx: Arc<ActionContext>) {
    tracing::info!("entering Running state — daemon fully operational");

    // Engine sessions (including _system.maintenance) are started in main.rs
    // before the lifecycle transitions to Running. This action is a hook
    // for future resume logic (e.g. re-activating paused sessions after
    // daemon restart — WS7+).
    tracing::info!(
        "Running.entry: orchestration engine already active (started in main.rs)"
    );

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
pub fn exit_running(_ctx: Arc<ActionContext>) {
    tracing::info!("exiting Running state");
    // No action: engine keeps running. Only Stopping.entry stops it.
}

/// Entry action for `Degraded` state.
///
/// Per spec §5.3:
/// - Record degraded subsystems in state-local storage (done in state.rs)
/// - Set HTTP endpoint lifecycle_state to "degraded"
/// - Keep orchestration engine running
pub fn enter_degraded(_ctx: Arc<ActionContext>) {
    tracing::info!("entering Degraded state — daemon partially operational");

    tracing::info!(
        target: "daemon_lifecycle",
        event = "degraded",
        "daemon lifecycle: degraded"
    );
}

/// Exit action for `Degraded` state.
///
/// Per spec §5.3: Clear degraded_subsystems tracking (done in state.rs).
pub fn exit_degraded(_ctx: Arc<ActionContext>) {
    tracing::info!("exiting Degraded state — all subsystems restored");
}

/// Entry action for `Stopping` state.
///
/// Per spec §5.4:
/// - Set HTTP endpoint lifecycle_state to "stopping"
/// - Stop accepting new sessions
/// - Call engine.shutdown(grace_ms)
/// - Send shutdown to workers
/// - Flush outbox, close DB pool, close HTTP listener
/// - Emit ShutdownDrained when complete
/// - Start watchdog for ShutdownTimeout
pub fn enter_stopping(ctx: Arc<ActionContext>) {
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
            lc_shutdown.dispatch(Event::ShutdownDrained);
        } else {
            tracing::warn!(
                "shutdown incomplete ({}/{}) — dispatching ShutdownDrained anyway",
                success_count,
                subsystems.len()
            );
            lc_shutdown.dispatch(Event::ShutdownDrained);
        }
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
/// - Set HTTP endpoint lifecycle_state to "failed"
/// - Log final tracing event
/// - Call `std::process::exit(exit_code)` after 100ms pause
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

// =============================================================================
// Legacy stubs (kept for backward compat with existing state.rs entry actions)
// =============================================================================

/// Stub for `Starting.entry` — replaced by `enter_starting(ctx)` above.
pub fn enter_starting_stub() {
    tracing::debug!("Starting.entry stub: would start HTTP, DB, Sync, Engine, WorkerMgr");
}

/// Stub for `Starting.exit` — replaced by `exit_starting(ctx)` above.
pub fn exit_starting_stub() {
    tracing::debug!("Starting.exit stub: would cancel in-flight subsystem starts");
}

/// Stub for `Running.entry` — replaced by `enter_running(ctx)` above.
pub fn enter_running_stub() {
    tracing::debug!("Running.entry stub: would start _system.maintenance session");
}

/// Stub for `Degraded.entry` — replaced by `enter_degraded(ctx)` above.
pub fn enter_degraded_stub() {
    tracing::debug!("Degraded.entry stub: would set HTTP lifecycle_state=degraded");
}

/// Stub for `Stopping.entry` — replaced by `enter_stopping(ctx)` above.
pub fn enter_stopping_stub() {
    tracing::debug!("Stopping.entry stub: would drain engine + workers, start watchdog");
}

/// Stub for `Failed.entry` — replaced by `enter_failed(exit_code, last_error)` above.
pub fn enter_failed_stub(exit_code: i32) {
    tracing::debug!(
        "Failed.entry stub: would log and exit with code {}",
        exit_code
    );
}
