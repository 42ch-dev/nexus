//! Workspace Management Module
//!
//! # Mutex Poisoning Policy
//!
//! This crate uses `unwrap_or_else` on mutex locks to recover from poisoned mutexes.
//! A poisoned mutex means a thread panicked while holding the lock. Rather than
//! crashing the entire daemon, we recover the lock and log a warning. The data
//! may be in an inconsistent state, but for a local development tool this is
//! preferable to a hard crash.

pub mod manager;
pub mod session;

use crate::db::pool::{DbPool, PoolConfig};
use crate::db::SqliteNarrativeGateway;
use crate::lifecycle::{Lifecycle, LifecycleState, StatigLifecycle};
use crate::workspace::session::WorkspaceSessionManager;
use nexus_contracts::local::domain::RuntimeMode;
use nexus_orchestration::{
    engine::OrchestrationEngine, schedule::supervisor::ScheduleSupervisor, CapabilityRegistry,
    WorkerManager,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::Notify;

/// Shared workspace state
#[derive(Clone)]
pub struct WorkspaceState {
    db: DbPool,
    nexus_home: PathBuf,
    db_path: PathBuf,
    started_at: std::time::Instant,
    /// Wall-clock timestamp of when the workspace state was created (daemon start).
    /// Used for reporting `started_at` in the daemon status API.
    started_at_wall: chrono::DateTime<chrono::Utc>,
    workspace_path: Arc<std::sync::Mutex<Option<String>>>,
    /// Runtime mode read from CLI config at startup.
    runtime_mode: RuntimeMode,
    /// Staleness: file modification time of the CLI config at daemon startup.
    /// Used to detect when CLI-side config changes may have occurred
    /// (e.g., runtime mode, degradation state).
    /// Lifecycle HSM for daemon state management.
    /// Set in T6 when main.rs wires up the lifecycle.
    lifecycle: Arc<Option<Arc<StatigLifecycle>>>,
    /// Orchestration engine (set at daemon startup when WS2 is wired).
    engine: Arc<Option<Arc<dyn OrchestrationEngine>>>,
    /// Worker manager (set at daemon startup when WS2 is wired).
    worker_manager: Arc<Option<Arc<WorkerManager>>>,
    /// Capability registry (set at daemon startup when WS2 is wired).
    capability_registry: Arc<Option<Arc<CapabilityRegistry>>>,
    /// Schedule supervisor for WS7 schedule management (set at daemon startup).
    schedule_supervisor: Arc<Option<Arc<ScheduleSupervisor>>>,
    /// Agent host facade (set at daemon startup when agent host subsystem is wired).
    agent_host: Arc<Option<Arc<dyn nexus_agent_host::HostFacade>>>,
    /// Narrative gateway — shared per workspace pool, constructed once at boot.
    narrative_gateway: Arc<SqliteNarrativeGateway>,
    /// Shutdown notification — fired when the daemon enters Stopping state.
    /// Consumers (HTTP server, engine drainer) await this to initiate graceful shutdown.
    shutdown_notify: Arc<Notify>,
    /// Daemon-side tool dispatch for nexus.* tools (DF-47, V1.42 P3).
    /// Set at daemon boot so schedule-executed `HostToolCallTask` can invoke tools.
    daemon_tool_dispatch: Arc<Option<Arc<dyn nexus_orchestration::capability::DaemonToolDispatch>>>,
    /// Workspace session manager (DF-31 skeleton).
    /// In-memory session store for workspace.open / workspace.commit.
    session_manager: Arc<WorkspaceSessionManager>,
    /// V1.80 REL-01: per-creator in-flight serialization guard for
    /// `POST /v1/local/memory/review`. Two overlapping review calls for the same
    /// creator fetch the same pending rows and would double-promote / mint
    /// duplicate fragments (the side effects are not idempotent at the DB).
    /// The outer `std::sync::Mutex` guards only the map lookup; each creator's
    /// lock is an independent `tokio::sync::Mutex` cloned out and awaited in the
    /// handler, so the map mutex is never held across `.await`.
    ///
    /// Lifecycle ceiling (R-V180P0-QC1-001): map entries are never evicted — the
    /// map grows with the number of distinct creators that have ever triggered a
    /// review on this daemon instance. This is bounded by the daemon lifetime +
    /// the single-active-creator local-only model (one entry per creator, an
    /// `Arc<AsyncMutex<()>>` is tiny). Revisit only if multi-creator session
    /// churn becomes real (e.g. a shared/rotating-creator deployment); an
    /// LRU/eviction policy would be the fix then.
    memory_review_locks: Arc<std::sync::Mutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
}

impl WorkspaceState {
    /// Create a `WorkspaceState` for testing purposes.
    /// Not intended for production use.
    ///
    /// Creates a connection pool with a single connection for test isolation.
    ///
    /// # Panics
    ///
    /// Panics if the database pool cannot be created.
    pub async fn new_for_testing(
        nexus_home: PathBuf,
        db_path: PathBuf,
        workspace_path: Option<String>,
    ) -> Self {
        let db = DbPool::new(&db_path, PoolConfig::default().with_max_connections(2))
            .await
            .expect("Failed to create test database pool");
        let narrative_gateway = Arc::new(SqliteNarrativeGateway::new(db.pool().clone()));
        let session_manager = Arc::new(WorkspaceSessionManager::new(Arc::new(db.pool().clone())));
        Self {
            db,
            nexus_home,
            db_path,
            started_at: std::time::Instant::now(),
            started_at_wall: chrono::Utc::now(),
            workspace_path: Arc::new(std::sync::Mutex::new(workspace_path)),
            runtime_mode: RuntimeMode::LocalOnly,
            lifecycle: Arc::new(None),
            engine: Arc::new(None),
            worker_manager: Arc::new(None),
            capability_registry: Arc::new(None),
            schedule_supervisor: Arc::new(None),
            agent_host: Arc::new(None),
            narrative_gateway,
            shutdown_notify: Arc::new(Notify::new()),
            daemon_tool_dispatch: Arc::new(None),
            session_manager,
            memory_review_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Initialize workspace state — create nexus home and `SQLite` database.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Home directory cannot be determined
    /// - Directory creation fails
    /// - CLI config cannot be read
    /// - Database schema initialization fails
    pub async fn initialize() -> anyhow::Result<Self> {
        let user_home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;

        let nexus_home = user_home.join(".nexus42");
        std::fs::create_dir_all(&nexus_home)?;

        // Read runtime mode from CLI config
        let cli_snapshot = crate::config::CliConfigSnapshot::load(&nexus_home)?;
        let runtime_mode = cli_snapshot.runtime_mode.unwrap_or(RuntimeMode::LocalOnly);

        let db_path = crate::config::resolve_state_db_path(&user_home, &nexus_home)?;

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Initialize schema and create connection pool via nexus_local_db
        crate::db::schema::Schema::init(&db_path).await?;
        let db = DbPool::new(&db_path, PoolConfig::from_env()).await?;

        let narrative_gateway = Arc::new(SqliteNarrativeGateway::new(db.pool().clone()));

        tracing::info!("Workspace state.db at {:?}", db_path);

        let session_manager = Arc::new(WorkspaceSessionManager::new(Arc::new(db.pool().clone())));
        Ok(Self {
            db,
            nexus_home,
            db_path,
            started_at: std::time::Instant::now(),
            started_at_wall: chrono::Utc::now(),
            workspace_path: Arc::new(std::sync::Mutex::new(None)),
            runtime_mode,
            lifecycle: Arc::new(None),
            engine: Arc::new(None),
            worker_manager: Arc::new(None),
            capability_registry: Arc::new(None),
            schedule_supervisor: Arc::new(None),
            agent_host: Arc::new(None),
            narrative_gateway,
            shutdown_notify: Arc::new(Notify::new()),
            daemon_tool_dispatch: Arc::new(None),
            session_manager,
            memory_review_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
        })
    }

    /// Set the lifecycle HSM for this workspace state.
    /// Called from main.rs after constructing the lifecycle.
    pub fn set_lifecycle(&mut self, lifecycle: Arc<StatigLifecycle>) {
        self.lifecycle = Arc::new(Some(lifecycle));
    }

    /// Set the orchestration engine.
    /// Called from main.rs after constructing the engine.
    pub fn set_engine(&mut self, engine: Arc<dyn OrchestrationEngine>) {
        self.engine = Arc::new(Some(engine));
    }

    /// Set the worker manager.
    pub fn set_worker_manager(&mut self, worker_manager: Arc<WorkerManager>) {
        self.worker_manager = Arc::new(Some(worker_manager));
    }

    /// Set the capability registry.
    pub fn set_capability_registry(&mut self, registry: Arc<CapabilityRegistry>) {
        self.capability_registry = Arc::new(Some(registry));
    }

    /// Set the schedule supervisor (WS7).
    pub fn set_schedule_supervisor(&mut self, supervisor: Arc<ScheduleSupervisor>) {
        self.schedule_supervisor = Arc::new(Some(supervisor));
    }

    /// Set the agent host facade.
    /// Called from boot.rs after constructing the agent host subsystem.
    pub fn set_agent_host(&mut self, host: Arc<dyn nexus_agent_host::HostFacade>) {
        self.agent_host = Arc::new(Some(host));
    }

    /// Set the daemon-side tool dispatch adapter (DF-47, V1.42 P3).
    pub fn set_daemon_tool_dispatch(
        &mut self,
        dispatch: Arc<dyn nexus_orchestration::capability::DaemonToolDispatch>,
    ) {
        self.daemon_tool_dispatch = Arc::new(Some(dispatch));
    }

    /// Get the daemon-side tool dispatch adapter, if set (DF-47, V1.42 P3).
    #[must_use]
    pub fn daemon_tool_dispatch(
        &self,
    ) -> Option<Arc<dyn nexus_orchestration::capability::DaemonToolDispatch>> {
        self.daemon_tool_dispatch.as_ref().clone()
    }

    /// Get the agent host facade, if set.
    #[must_use]
    pub fn agent_host(&self) -> Option<Arc<dyn nexus_agent_host::HostFacade>> {
        self.agent_host.as_ref().clone()
    }

    /// V1.80 REL-01: get (or lazily create) the per-creator review lock.
    ///
    /// The outer `std::sync::Mutex` guards only this map lookup — it is released
    /// as soon as the function returns. The caller then `.lock().await`s the
    /// returned `Arc<AsyncMutex<()>>` to serialize overlapping review calls for
    /// the same creator without blocking unrelated creators.
    #[must_use]
    pub fn memory_review_lock(&self, creator_id: &str) -> Arc<AsyncMutex<()>> {
        let mut map = self
            .memory_review_locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner); // poison-recovery (crate policy)
        map.entry(creator_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    /// Get the narrative gateway (shared per workspace pool).
    #[must_use]
    pub fn narrative_gateway(&self) -> Arc<SqliteNarrativeGateway> {
        Arc::clone(&self.narrative_gateway)
    }

    /// Get the orchestration engine, if set.
    #[must_use]
    pub fn engine(&self) -> Option<Arc<dyn OrchestrationEngine>> {
        self.engine.as_ref().clone()
    }

    /// Get the schedule supervisor, if set (WS7).
    #[must_use]
    pub fn schedule_supervisor(&self) -> Option<Arc<ScheduleSupervisor>> {
        self.schedule_supervisor.as_ref().clone()
    }

    /// Get the worker manager, if set.
    #[must_use]
    pub fn worker_manager(&self) -> Option<Arc<WorkerManager>> {
        self.worker_manager.as_ref().clone()
    }

    /// Get the capability registry, if set.
    #[must_use]
    pub fn capability_registry(&self) -> Option<Arc<CapabilityRegistry>> {
        self.capability_registry.as_ref().clone()
    }

    /// Get the shutdown notification handle.
    ///
    /// Callers await `.notified()` to block until the daemon enters Stopping state.
    #[must_use]
    pub fn shutdown_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.shutdown_notify)
    }

    /// Request graceful shutdown — fires the shutdown notification.
    ///
    /// Called from lifecycle `Stopping` entry or signal handlers.
    pub fn request_shutdown(&self) {
        self.shutdown_notify.notify_one();
    }

    /// Get the lifecycle, if set.
    #[must_use]
    pub fn lifecycle(&self) -> Option<Arc<StatigLifecycle>> {
        self.lifecycle.as_ref().clone()
    }

    /// Get the current lifecycle state.
    /// Returns a default state if no lifecycle is set.
    #[must_use]
    pub fn lifecycle_state(&self) -> LifecycleState {
        self.lifecycle
            .as_ref()
            .as_ref()
            .map_or(LifecycleState::Running, |lc| lc.current_state())
    }

    /// Get exit code from lifecycle, if set.
    #[must_use]
    pub fn lifecycle_exit_code(&self) -> Option<i32> {
        self.lifecycle
            .as_ref()
            .as_ref()
            .and_then(|lc| lc.exit_code())
    }

    /// Get a reference to the underlying sqlx pool.
    #[must_use]
    pub const fn pool(&self) -> &sqlx::SqlitePool {
        self.db.pool()
    }

    /// Check if workspace is initialized.
    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.workspace_path
            .lock()
            .unwrap_or_else(|poisoned| {
                tracing::warn!("workspace_path mutex poisoned, recovering");
                poisoned.into_inner()
            })
            .is_some()
    }

    /// Get workspace path.
    #[must_use]
    pub fn workspace_path(&self) -> Option<String> {
        self.workspace_path
            .lock()
            .unwrap_or_else(|poisoned| {
                tracing::warn!("workspace_path mutex poisoned, recovering");
                poisoned.into_inner()
            })
            .clone()
    }

    /// Get database path.
    #[must_use]
    pub fn database_path(&self) -> String {
        self.db_path.display().to_string()
    }

    /// Get nexus home directory.
    #[must_use]
    pub const fn nexus_home(&self) -> &PathBuf {
        &self.nexus_home
    }

    /// Get a clone of the database pool (for `TokenManager`, etc.)
    #[must_use]
    pub fn db_pool(&self) -> DbPool {
        self.db.clone()
    }

    /// Get uptime in seconds.
    #[must_use]
    pub fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Wall-clock timestamp when the daemon started (RFC 3339).
    #[must_use]
    pub const fn started_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.started_at_wall
    }

    /// Workspace session manager (DF-31 skeleton).
    #[must_use]
    pub fn session_manager(&self) -> Arc<WorkspaceSessionManager> {
        Arc::clone(&self.session_manager)
    }

    /// Current runtime mode (from CLI config at startup).
    #[must_use]
    pub const fn runtime_mode(&self) -> &RuntimeMode {
        &self.runtime_mode
    }

    /// Runtime mode as a string matching JSON Schema enum values.
    #[must_use]
    pub const fn runtime_mode_as_str(&self) -> &'static str {
        self.runtime_mode.as_str()
    }

    /// Initialize a workspace at the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Directory creation fails
    /// - Database write fails
    pub async fn init_workspace(&self, path: &str) -> anyhow::Result<()> {
        let workspace_dir = std::path::Path::new(path);
        let nexus_dir = workspace_dir.join(".nexus42");

        std::fs::create_dir_all(&nexus_dir)?;

        // Store workspace path in the database
        // SAFETY: single static INSERT into workspace_meta key-value table.
        // Uses unnamed ? for a single bind parameter.
        sqlx::query(
            "INSERT OR REPLACE INTO workspace_meta (key, value) VALUES ('workspace_path', ?)",
        )
        .bind(path)
        .execute(self.pool())
        .await
        .map_err(|e| anyhow::anyhow!("Database error: {e}"))?;

        // Update in-memory state so is_initialized() returns true
        *self.workspace_path.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("workspace_path mutex poisoned, recovering");
            poisoned.into_inner()
        }) = Some(path.to_string());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;

    #[tokio::test]
    async fn init_workspace_sets_is_initialized() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let workspace_dir = tmp.path().join("my-workspace");

        let state = WorkspaceState::new_for_testing(
            nexus_home, db_path, None, // no workspace path set initially
        )
        .await;

        // Before init: is_initialized should be false
        assert!(
            !state.is_initialized(),
            "is_initialized() should return false before init_workspace()"
        );

        // Initialize workspace
        let path_str = workspace_dir.display().to_string();
        state
            .init_workspace(&path_str)
            .await
            .expect("init_workspace should succeed");

        // After init: is_initialized should be true
        assert!(
            state.is_initialized(),
            "is_initialized() should return true after init_workspace()"
        );

        // workspace_path() should return the path
        assert_eq!(state.workspace_path(), Some(path_str));
    }
}
