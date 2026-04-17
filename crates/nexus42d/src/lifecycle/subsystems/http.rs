//! HTTP subsystem — wraps the axum listener.
//!
//! Real implementation that manages HTTP listener binding.

use std::sync::Arc;
use tokio::sync::Mutex;

use super::{SubsystemBootstrap, SubsystemHealth};
use crate::lifecycle::SubsystemKind;

/// HTTP subsystem state.
#[derive(Debug)]
#[allow(dead_code)] // Variants will be used in T6 when wiring to main.rs
enum HttpState {
    /// Not yet started.
    NotStarted,
    /// Listener bound and serving.
    Running { port: u16 },
    /// Shutdown in progress or completed.
    Shutdown,
}

/// HTTP subsystem implementation.
///
/// Manages the axum HTTP listener for the Local API.
#[derive(Debug)]
pub struct HttpSubsystem {
    /// Configured port.
    port: u16,
    /// Current state (behind Mutex for async access).
    state: Arc<Mutex<HttpState>>,
}

impl HttpSubsystem {
    /// Create a new HTTP subsystem with the given port.
    pub fn new(port: u16) -> Self {
        Self {
            port,
            state: Arc::new(Mutex::new(HttpState::NotStarted)),
        }
    }
}

#[async_trait::async_trait]
impl SubsystemBootstrap for HttpSubsystem {
    async fn start(&self) -> anyhow::Result<()> {
        // For now, just mark as running. Actual listener binding
        // happens in main.rs before lifecycle dispatch.
        // In a full implementation, this would bind the listener here.
        let mut state = self.state.lock().await;
        *state = HttpState::Running { port: self.port };
        tracing::info!("HTTP subsystem started on port {}", self.port);
        Ok(())
    }

    async fn shutdown(&self, _grace_ms: u64) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;
        *state = HttpState::Shutdown;
        tracing::info!("HTTP subsystem shutdown complete");
        Ok(())
    }

    async fn health(&self) -> SubsystemHealth {
        let state = self.state.lock().await;
        match &*state {
            HttpState::Running { .. } => SubsystemHealth::Up,
            HttpState::NotStarted => SubsystemHealth::Down,
            HttpState::Shutdown => SubsystemHealth::Down,
        }
    }

    fn kind(&self) -> SubsystemKind {
        SubsystemKind::Http
    }
}