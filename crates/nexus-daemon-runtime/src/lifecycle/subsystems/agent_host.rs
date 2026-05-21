//! Agent Host subsystem — wraps the `nexus-agent-host` facade.
//!
//! Integrates the agent host into the daemon lifecycle HSM.
//! Manages startup/shutdown of the agent host provider adapters.
//! Optional subsystem — not required for the `Running` lifecycle transition.

use std::sync::Arc;
use tokio::sync::Mutex;

use nexus_agent_host::HostFacade;

use super::{SubsystemBootstrap, SubsystemHealth};
use crate::lifecycle::SubsystemKind;

/// Agent Host subsystem state.
#[derive(Debug)]
enum AgentHostState {
    /// Not yet started.
    NotStarted,
    /// Host is running and accepting sessions.
    Running,
    /// Shutdown in progress or completed.
    Shutdown,
}

/// Agent Host subsystem implementation.
///
/// Wraps `Arc<dyn HostFacade>` from `nexus-agent-host` and integrates it
/// into the daemon lifecycle HSM as an optional subsystem.
pub struct AgentHostSubsystem {
    /// The host facade (from `nexus-agent-host`).
    host: Arc<dyn HostFacade>,
    /// Path to the agent-host config file.
    config_path: std::path::PathBuf,
    /// Workspace root for the agent host.
    workspace_root: std::path::PathBuf,
    /// Current state (behind Mutex for async access).
    state: Arc<Mutex<AgentHostState>>,
}

impl AgentHostSubsystem {
    /// Create a new Agent Host subsystem.
    ///
    /// The `host` should be a fully-constructed `HostManager` (or other `HostFacade` impl)
    /// with providers already registered.
    pub fn new(
        host: Arc<dyn HostFacade>,
        config_path: std::path::PathBuf,
        workspace_root: std::path::PathBuf,
    ) -> Self {
        Self {
            host,
            config_path,
            workspace_root,
            state: Arc::new(Mutex::new(AgentHostState::NotStarted)),
        }
    }

    /// Get a reference to the underlying host facade.
    #[must_use]
    pub fn host(&self) -> Arc<dyn HostFacade> {
        Arc::clone(&self.host)
    }
}

impl std::fmt::Debug for AgentHostSubsystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentHostSubsystem")
            .field("config_path", &self.config_path)
            .field("workspace_root", &self.workspace_root)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl SubsystemBootstrap for AgentHostSubsystem {
    async fn start(&self) -> anyhow::Result<()> {
        let start_config = nexus_agent_host::capability::HostStartConfig {
            config_path: self.config_path.clone(),
            workspace_root: self.workspace_root.clone(),
            max_sessions: nexus_agent_host::config::AgentHostConfig::default().max_sessions,
            max_ops_per_session: nexus_agent_host::config::AgentHostConfig::default()
                .max_ops_per_session,
            timeouts: nexus_agent_host::config::TimeoutConfig::default(),
        };

        self.host
            .start(start_config)
            .await
            .map_err(|e| anyhow::anyhow!("agent host start failed: {e}"))?;

        {
            *self.state.lock().await = AgentHostState::Running;
        }
        tracing::info!("Agent Host subsystem started");
        Ok(())
    }

    async fn shutdown(&self, _grace_ms: u64) -> anyhow::Result<()> {
        self.host
            .shutdown()
            .await
            .map_err(|e| anyhow::anyhow!("agent host shutdown failed: {e}"))?;

        {
            *self.state.lock().await = AgentHostState::Shutdown;
        }
        tracing::info!("Agent Host subsystem shutdown complete");
        Ok(())
    }

    async fn health(&self) -> SubsystemHealth {
        let state = self.state.lock().await;
        match &*state {
            AgentHostState::Running => {
                // Check host health
                match self.host.health().await {
                    Ok(h) if h.running => SubsystemHealth::Up,
                    Ok(_) | Err(_) => SubsystemHealth::Degraded,
                }
            }
            AgentHostState::NotStarted | AgentHostState::Shutdown => SubsystemHealth::Down,
        }
    }

    fn kind(&self) -> SubsystemKind {
        SubsystemKind::AgentHost
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_agent_host::core::manager::HostManager;

    #[test]
    fn kind_returns_agent_host() {
        let host = Arc::new(HostManager::new());
        let subsystem = AgentHostSubsystem::new(
            host,
            std::path::PathBuf::from("/tmp/test-config"),
            std::path::PathBuf::from("/tmp/workspace"),
        );
        assert_eq!(subsystem.kind(), SubsystemKind::AgentHost);
    }

    #[tokio::test]
    async fn health_is_down_before_start() {
        let host = Arc::new(HostManager::new());
        let subsystem = AgentHostSubsystem::new(
            host,
            std::path::PathBuf::from("/tmp/test-config"),
            std::path::PathBuf::from("/tmp/workspace"),
        );
        assert_eq!(subsystem.health().await, SubsystemHealth::Down);
    }
}
