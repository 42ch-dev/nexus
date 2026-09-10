//! Engine subsystem — signals that the orchestration engine is wired.
//!
//! The engine itself is constructed by `WorkspaceState` before the lifecycle
//! dispatch; this bootstrap's contract is to report readiness so the HSM can
//! reach `Running` (Engine is a mandatory subsystem per spec §5).

use std::sync::Arc;
use tokio::sync::Mutex;

use super::{SubsystemBootstrap, SubsystemHealth};
use crate::lifecycle::SubsystemKind;

/// Engine subsystem state.
#[derive(Debug)]
enum EngineState {
    NotStarted,
    Running,
    Shutdown,
}

/// Engine subsystem implementation.
#[derive(Debug)]
pub struct EngineSubsystem {
    state: Arc<Mutex<EngineState>>,
}

impl EngineSubsystem {
    /// Create a new engine subsystem.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(EngineState::NotStarted)),
        }
    }
}

impl Default for EngineSubsystem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SubsystemBootstrap for EngineSubsystem {
    async fn start(&self) -> anyhow::Result<()> {
        // The orchestration engine is constructed and wired by
        // `WorkspaceState` before the lifecycle starts; nothing to spawn here.
        let mut state = self.state.lock().await;
        *state = EngineState::Running;
        drop(state);
        tracing::info!("Engine subsystem started");
        Ok(())
    }

    async fn shutdown(&self, _grace_ms: u64) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;
        *state = EngineState::Shutdown;
        drop(state);
        tracing::info!("Engine subsystem shutdown complete");
        Ok(())
    }

    async fn health(&self) -> SubsystemHealth {
        let state = self.state.lock().await;
        match &*state {
            EngineState::Running => SubsystemHealth::Up,
            EngineState::NotStarted | EngineState::Shutdown => SubsystemHealth::Down,
        }
    }

    fn kind(&self) -> SubsystemKind {
        SubsystemKind::Engine
    }
}
