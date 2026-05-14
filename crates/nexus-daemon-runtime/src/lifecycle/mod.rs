//! Daemon lifecycle HSM (statig-based).
//!
//! Implements the 6-state hierarchical state machine for the daemon runtime
//! per `knowledge/daemon-lifecycle-api-v2.md`.
//!
//! States: `Stopped → Starting → Running ⇄ Degraded → Stopping → Failed`.
//! The `Alive` superstate groups `Running` and `Degraded`.

mod actions;
mod events;
mod state;
pub mod subsystems;

pub use actions::*;
pub use events::{Event, SubsystemKind};
pub use state::{DaemonHsm, StatigLifecycle};
pub use subsystems::{
    DbSubsystem, HttpSubsystem, MockAllSubsystems, SubsystemBootstrap, SubsystemHealth,
    SyncSubsystem, WorkerMgrSubsystem,
};

use tokio::sync::broadcast;

/// External state label for HTTP endpoint and tests.
///
/// Note: `Stopped` is the initial pseudo-state and is never externally visible
/// (invariant §2.3 in spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleState {
    Starting,
    Running,
    Degraded,
    Stopping,
    Failed,
}

impl std::fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Starting => write!(f, "starting"),
            Self::Running => write!(f, "running"),
            Self::Degraded => write!(f, "degraded"),
            Self::Stopping => write!(f, "stopping"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl LifecycleState {
    /// Returns true if this state is terminal (no further transitions).
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Failed)
    }
}

/// Recorded when a state transition occurs (for broadcast subscribers).
#[derive(Debug, Clone)]
pub struct LifecycleTransition {
    pub from: LifecycleState,
    pub to: LifecycleState,
    pub event: Event,
}

/// Adapter trait for the HSM.
///
/// HTTP handlers and orchestration engine interact with this trait,
/// not with `statig` types directly.
pub trait Lifecycle: Send + Sync {
    /// Returns the current lifecycle state.
    fn current_state(&self) -> LifecycleState;

    /// Dispatches an event to the HSM.
    fn dispatch(&self, event: Event);

    /// Subscribe to state transitions.
    fn subscribe(&self) -> broadcast::Receiver<LifecycleTransition>;

    /// Returns exit code if the machine is in `Failed` state.
    fn exit_code(&self) -> Option<i32>;
}
