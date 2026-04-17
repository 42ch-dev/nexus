//! Worker Manager subsystem — mock stub (WS2 not complete).
//!
//! This is a mock implementation because `nexus-orchestration` crate
//! does not exist yet (WS2 will create it). The real implementation
//! will wrap `nexus-orchestration::WorkerManager`.

use std::sync::Arc;
use tokio::sync::Mutex;

use super::{SubsystemBootstrap, SubsystemHealth};
use crate::lifecycle::SubsystemKind;

/// Worker Manager subsystem state (mock).
#[derive(Debug)]
enum WorkerMgrState {
    /// Not yet started.
    NotStarted,
    /// Worker manager initialized (mock - always healthy).
    Running,
    /// Shutdown.
    Shutdown,
}

/// Worker Manager subsystem mock implementation.
///
/// WS2 will replace this with real `WorkerManager` integration.
/// For now, start/shutdown are no-ops and health always returns `Up`.
#[derive(Debug)]
pub struct WorkerMgrSubsystem {
    /// Current state (behind Mutex for async access).
    state: Arc<Mutex<WorkerMgrState>>,
}

impl WorkerMgrSubsystem {
    /// Create a new Worker Manager subsystem mock.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(WorkerMgrState::NotStarted)),
        }
    }
}

impl Default for WorkerMgrSubsystem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SubsystemBootstrap for WorkerMgrSubsystem {
    async fn start(&self) -> anyhow::Result<()> {
        // Mock: no real worker manager exists yet.
        // WS2 will implement actual WorkerManager initialization here.
        let mut state = self.state.lock().await;
        *state = WorkerMgrState::Running;
        tracing::info!("WorkerMgr subsystem started (mock)");
        Ok(())
    }

    async fn shutdown(&self, _grace_ms: u64) -> anyhow::Result<()> {
        // Mock: no real workers to shutdown.
        let mut state = self.state.lock().await;
        *state = WorkerMgrState::Shutdown;
        tracing::info!("WorkerMgr subsystem shutdown complete (mock)");
        Ok(())
    }

    async fn health(&self) -> SubsystemHealth {
        let state = self.state.lock().await;
        match &*state {
            WorkerMgrState::Running => SubsystemHealth::Up, // Mock: always healthy
            WorkerMgrState::NotStarted | WorkerMgrState::Shutdown => SubsystemHealth::Down,
        }
    }

    fn kind(&self) -> SubsystemKind {
        SubsystemKind::WorkerMgr
    }
}
