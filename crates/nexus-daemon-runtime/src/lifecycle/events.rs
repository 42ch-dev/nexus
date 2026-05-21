//! Event types for the daemon lifecycle HSM.
//!
//! Per spec §3, all events are handled by the HSM and trigger transitions.

use serde::{Deserialize, Serialize};

/// Subsystem identifier for health tracking and event payloads.
///
/// Note: `Http`, `Db`, `Engine`, `WorkerMgr` are mandatory;
/// `Sync`, `AcpRegistry` are optional (not required for `Running` transition).
/// `Sync` retained for backward-compatible health reporting (always Down on local daemon).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubsystemKind {
    Http,
    Db,
    Sync,
    Engine,
    WorkerMgr,
    AcpRegistry,
    AgentHost,
}

impl SubsystemKind {
    /// Returns all mandatory subsystems (required for `Running` transition).
    #[must_use]
    pub const fn mandatory() -> &'static [Self] {
        &[Self::Http, Self::Db, Self::Engine, Self::WorkerMgr]
    }

    /// Returns true if this subsystem is mandatory.
    #[must_use]
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
    #[must_use]
    pub const fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::FatalError { .. }
                | Self::SubsystemFailed {
                    retryable: false,
                    ..
                }
        )
    }
}
