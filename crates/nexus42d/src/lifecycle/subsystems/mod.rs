//! Subsystem bootstrap implementations.
//!
//! Per spec §5, each subsystem implements `SubsystemBootstrap` for
//! lifecycle-controlled startup/shutdown. Real implementations where possible,
//! mock stubs for WS2 components (Engine, WorkerMgr) not yet available.

mod db;
mod engine;
mod http;
mod mock_all;
mod sync;
mod worker_mgr;

pub use db::DbSubsystem;
pub use engine::EngineSubsystem;
pub use http::HttpSubsystem;
pub use mock_all::MockAllSubsystems;
pub use sync::SyncSubsystem;
pub use worker_mgr::WorkerMgrSubsystem;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::SubsystemKind;

/// Health status of a subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubsystemHealth {
    /// Subsystem is operational.
    Up,
    /// Subsystem is degraded but partially functional.
    Degraded,
    /// Subsystem is down and non-functional.
    Down,
}

/// Trait for subsystem lifecycle management.
///
/// Each managed subsystem (HTTP, DB, Sync, Engine, WorkerMgr) implements this trait.
/// The lifecycle HSM calls these methods during state transitions.
#[async_trait]
pub trait SubsystemBootstrap: Send + Sync {
    /// Start the subsystem.
    ///
    /// Called from `Starting.entry`. On success, the subsystem should dispatch
    /// `SubsystemUp(kind)` to the lifecycle. On failure, dispatch `SubsystemFailed`.
    async fn start(&self) -> anyhow::Result<()>;

    /// Gracefully shutdown the subsystem.
    ///
    /// Called from `Stopping.entry`. The `grace_ms` parameter specifies the
    /// maximum time to wait for graceful drain before forcing termination.
    async fn shutdown(&self, grace_ms: u64) -> anyhow::Result<()>;

    /// Query current health status.
    ///
    /// Called periodically from health check tasks to determine
    /// `HealthDegraded` / `HealthRestored` events.
    async fn health(&self) -> SubsystemHealth;

    /// Return the subsystem kind for event dispatch.
    fn kind(&self) -> SubsystemKind;
}
