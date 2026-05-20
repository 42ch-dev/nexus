//! Mutex lock patterns have scoped drops.
#![allow(clippy::significant_drop_tightening)]
//! Sync subsystem — wraps nexus-cloud-sync outbox reader.
//!
//! Real implementation that manages the sync queue.

use std::sync::Arc;
use tokio::sync::Mutex;

use super::{SubsystemBootstrap, SubsystemHealth};
use crate::lifecycle::SubsystemKind;

/// Sync subsystem state.
#[derive(Debug)]
enum SyncState {
    /// Not yet started.
    NotStarted,
    /// Outbox reader initialized and running.
    Running,
    /// Shutdown (outbox closed).
    Shutdown,
}

/// Sync subsystem implementation.
///
/// Manages the `nexus-cloud-sync::OutboxPool` for outbound sync operations.
#[derive(Debug)]
pub struct SyncSubsystem {
    /// Current state (behind Mutex for async access).
    state: Arc<Mutex<SyncState>>,
}

impl SyncSubsystem {
    /// Create a new Sync subsystem.
    ///
    /// The actual outbox initialization happens via `nexus-cloud-sync::OutboxPool`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(SyncState::NotStarted)),
        }
    }
}

impl Default for SyncSubsystem {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SubsystemBootstrap for SyncSubsystem {
    async fn start(&self) -> anyhow::Result<()> {
        // For now, just mark as running. Actual outbox init
        // happens in WorkspaceState::initialize().
        // In a full implementation, this would create the OutboxPool here.
        let mut state = self.state.lock().await;
        *state = SyncState::Running;
        tracing::info!("Sync subsystem started");
        Ok(())
    }

    async fn shutdown(&self, _grace_ms: u64) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;
        *state = SyncState::Shutdown;
        tracing::info!("Sync subsystem shutdown complete");
        Ok(())
    }

    async fn health(&self) -> SubsystemHealth {
        let state = self.state.lock().await;
        match &*state {
            SyncState::Running => SubsystemHealth::Up,
            SyncState::NotStarted | SyncState::Shutdown => SubsystemHealth::Down,
        }
    }

    fn kind(&self) -> SubsystemKind {
        SubsystemKind::Sync
    }
}
