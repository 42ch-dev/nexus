//! Event types for the daemon lifecycle HSM.
//!
//! Per spec §3, all events are handled by the HSM and trigger transitions.

use serde::{Deserialize, Serialize};

/// Subsystem identifier for health tracking and event payloads.
///
/// Note: `Http`, `Db`, `Sync`, `Engine`, `WorkerMgr` are mandatory;
/// `AcpRegistry` is optional (not required for `Running` transition).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubsystemKind {
    Http,
    Db,
    Sync,
    Engine,
    WorkerMgr,
    AcpRegistry,
}

impl SubsystemKind {
    /// Returns all mandatory subsystems (required for `Running` transition).
    pub fn mandatory() -> &'static [SubsystemKind] {
        &[
            SubsystemKind::Http,
            SubsystemKind::Db,
            SubsystemKind::Sync,
            SubsystemKind::Engine,
            SubsystemKind::WorkerMgr,
        ]
    }

    /// Returns true if this subsystem is mandatory.
    pub fn is_mandatory(&self) -> bool {
        Self::mandatory().contains(self)
    }
}

/// Events that drive the lifecycle HSM.
///
/// See spec §3 for source and payload details.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Process started after tokio runtime up.
    ProcessStarted,

    /// Subsystem reported successful startup.
    SubsystemUp(SubsystemKind),

    /// Subsystem startup failed.
    SubsystemFailed {
        kind: SubsystemKind,
        err: String,
        retryable: bool,
    },

    /// Health check detected degradation.
    HealthDegraded { kind: SubsystemKind, reason: String },

    /// Health check detected recovery.
    HealthRestored { kind: SubsystemKind },

    /// Shutdown requested (signal, admin, or supervisor).
    ShutdownRequested { source: String },

    /// Engine and workers finished draining.
    ShutdownDrained,

    /// Graceful shutdown timeout exceeded.
    ShutdownTimeout { grace_ms_exceeded: u64 },

    /// Unrecoverable error; process will exit.
    FatalError { kind: SubsystemKind, err: String },
}

impl Event {
    /// Returns true if this is a fatal event that should transition to `Failed`.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Event::FatalError { .. }
                | Event::SubsystemFailed {
                    retryable: false,
                    ..
                }
        )
    }
}
