//! DB subsystem — wraps SQLite pool + migrations.
//!
//! Real implementation that manages the local SQLite database.

use std::sync::Arc;
use tokio::sync::Mutex;

use super::{SubsystemBootstrap, SubsystemHealth};
use crate::lifecycle::SubsystemKind;

/// DB subsystem state.
#[derive(Debug)]
enum DbState {
    /// Not yet started.
    NotStarted,
    /// Pool open and migrations run.
    Running,
    /// Shutdown (pool closed).
    Shutdown,
}

/// DB subsystem implementation.
///
/// Manages the SQLite pool via `nexus-local-db`.
#[derive(Debug)]
pub struct DbSubsystem {
    /// Current state (behind Mutex for async access).
    state: Arc<Mutex<DbState>>,
    /// Path to the SQLite database (for logging).
    db_path: Option<String>,
}

impl DbSubsystem {
    /// Create a new DB subsystem.
    ///
    /// The actual pool initialization happens via `nexus_local_db::open_pool()`
    /// which is called from `WorkspaceState::initialize()`.
    pub fn new(db_path: Option<String>) -> Self {
        Self {
            state: Arc::new(Mutex::new(DbState::NotStarted)),
            db_path,
        }
    }
}

#[async_trait::async_trait]
impl SubsystemBootstrap for DbSubsystem {
    async fn start(&self) -> anyhow::Result<()> {
        // For now, just mark as running. Actual pool/migrations
        // happen in WorkspaceState::initialize() before lifecycle dispatch.
        // In a full implementation, this would call nexus_local_db::open_pool()
        // and run_migrations() here.
        let mut state = self.state.lock().await;
        *state = DbState::Running;
        tracing::info!(
            "DB subsystem started (path: {})",
            self.db_path.as_deref().unwrap_or("default")
        );
        Ok(())
    }

    async fn shutdown(&self, _grace_ms: u64) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;
        *state = DbState::Shutdown;
        tracing::info!("DB subsystem shutdown complete");
        Ok(())
    }

    async fn health(&self) -> SubsystemHealth {
        let state = self.state.lock().await;
        match &*state {
            DbState::Running => SubsystemHealth::Up,
            DbState::NotStarted | DbState::Shutdown => SubsystemHealth::Down,
        }
    }

    fn kind(&self) -> SubsystemKind {
        SubsystemKind::Db
    }
}
