//! Engine subsystem — mock stub (WS2 not complete).
//!
//! This is a mock implementation because `nexus-orchestration` crate
//! does not exist yet (WS2 will create it). The real implementation
//! will wrap `nexus-orchestration::GraphFlowEngine`.

use std::sync::Arc;
use tokio::sync::Mutex;

use super::{SubsystemBootstrap, SubsystemHealth};
use crate::lifecycle::SubsystemKind;

/// Engine subsystem state (mock).
#[derive(Debug)]
enum EngineState {
    /// Not yet started.
    NotStarted,
    /// Engine initialized (mock - always healthy).
    Running,
    /// Shutdown.
    Shutdown,
}

/// Engine subsystem mock implementation.
///
/// WS2 will replace this with real `GraphFlowEngine` integration.
/// For now, start/shutdown are no-ops and health always returns `Up`.
#[derive(Debug)]
pub struct EngineSubsystem {
    /// Current state (behind Mutex for async access).
    state: Arc<Mutex<EngineState>>,
}

impl EngineSubsystem {
    /// Create a new Engine subsystem mock.
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
        // Mock: no real engine exists yet.
        // WS2 will implement actual GraphFlowEngine initialization here.
        let mut state = self.state.lock().await;
        *state = EngineState::Running;
        tracing::info!("Engine subsystem started (mock)");
        Ok(())
    }

    async fn shutdown(&self, _grace_ms: u64) -> anyhow::Result<()> {
        // Mock: no real engine to shutdown.
        let mut state = self.state.lock().await;
        *state = EngineState::Shutdown;
        tracing::info!("Engine subsystem shutdown complete (mock)");
        Ok(())
    }

    async fn health(&self) -> SubsystemHealth {
        let state = self.state.lock().await;
        match &*state {
            EngineState::Running => SubsystemHealth::Up, // Mock: always healthy
            EngineState::NotStarted | EngineState::Shutdown => SubsystemHealth::Down,
        }
    }

    fn kind(&self) -> SubsystemKind {
        SubsystemKind::Engine
    }
}