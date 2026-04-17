//! Entry/exit action bodies for the daemon lifecycle HSM.
//!
//! Per spec §5, these actions drive subsystem lifecycle.
//! For T1-T3, these are stubs that just log. Full implementation comes in T4.

/// Stub for `Starting.entry` — full impl starts subsystems in T4.
pub fn enter_starting_stub() {
    tracing::debug!("Starting.entry stub: would start HTTP, DB, Sync, Engine, WorkerMgr");
}

/// Stub for `Starting.exit` — full impl cancels in-flight starts in T4.
pub fn exit_starting_stub() {
    tracing::debug!("Starting.exit stub: would cancel in-flight subsystem starts");
}

/// Stub for `Running.entry` — full impl starts system preset session in T4.
pub fn enter_running_stub() {
    tracing::debug!("Running.entry stub: would start _system.maintenance session");
}

/// Stub for `Degraded.entry` — full impl updates HTTP endpoint status in T4.
pub fn enter_degraded_stub() {
    tracing::debug!("Degraded.entry stub: would set HTTP lifecycle_state=degraded");
}

/// Stub for `Stopping.entry` — full impl drains engine/workers in T4.
pub fn enter_stopping_stub() {
    tracing::debug!("Stopping.entry stub: would drain engine + workers, start watchdog");
}

/// Stub for `Failed.entry` — full impl logs and exits in T4.
pub fn enter_failed_stub(exit_code: i32) {
    tracing::debug!(
        "Failed.entry stub: would log and exit with code {}",
        exit_code
    );
}
