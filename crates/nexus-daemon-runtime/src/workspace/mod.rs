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

use crate::api::errors::NexusApiError;
use crate::db::pool::{DbPool, PoolConfig};
use crate::db::SqliteNarrativeGateway;
use crate::lifecycle::{Lifecycle, LifecycleState, StatigLifecycle};
use crate::workspace::session::WorkspaceSessionManager;
use nexus_agent_host::config::AgentHostConfig;
use nexus_contracts::local::domain::RuntimeMode;
use nexus_contracts::CertFingerprintResponse;
use nexus_orchestration::{
    engine::OrchestrationEngine, schedule::supervisor::ScheduleSupervisor, CapabilityRegistry,
    WorkerManager,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::Notify;

/// Shared creator DB slot — interior mutability so lazy-open propagates across
/// Axum `State<WorkspaceState>` clones (V1.118 P0 T2 / architect Option A).
#[derive(Clone, Default)]
struct CreatorDbSlot {
    db: Option<DbPool>,
    db_path: Option<PathBuf>,
    narrative_gateway: Option<Arc<SqliteNarrativeGateway>>,
    session_manager: Option<Arc<WorkspaceSessionManager>>,
}

/// Shared workspace state
#[derive(Clone)]
pub struct WorkspaceState {
    creator_db: Arc<RwLock<CreatorDbSlot>>,
    /// Stable pool handle for `pool()` borrows across lazy-open (V1.118 T2).
    shared_pool: Arc<OnceLock<Arc<sqlx::SqlitePool>>>,
    nexus_home: PathBuf,
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
    /// Agent host configuration loaded at boot from `agent-host/config.toml`.
    agent_host_config: Arc<AgentHostConfig>,
    /// Shutdown notification — fired when the daemon enters Stopping state.
    /// Consumers (HTTP server, engine drainer) await this to initiate graceful shutdown.
    shutdown_notify: Arc<Notify>,
    /// Daemon-side tool dispatch for nexus.* tools (DF-47, V1.42 P3).
    /// Set at daemon boot so schedule-executed `HostToolCallTask` can invoke tools.
    daemon_tool_dispatch: Arc<Option<Arc<dyn nexus_orchestration::capability::DaemonToolDispatch>>>,
    /// V1.80 REL-01: per-creator in-flight serialization guard for
    /// `POST /v1/daemon/memory/review`. Two overlapping review calls for the same
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
    /// V1.92: optional TLS certificate fingerprint for remote (non-loopback)
    /// binds. Loopback-only daemons leave this as `None`.
    tls_fingerprint: Arc<Option<CertFingerprintResponse>>,
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
        let creator_db = Arc::new(RwLock::new(CreatorDbSlot {
            db: Some(db),
            db_path: Some(db_path.clone()),
            narrative_gateway: Some(narrative_gateway),
            session_manager: Some(session_manager),
        }));
        let shared_pool = Arc::new(OnceLock::new());
        let _ = shared_pool.set(Arc::new(
            creator_db
                .read()
                .expect("creator_db lock")
                .db
                .as_ref()
                .expect("test db")
                .pool()
                .clone(),
        ));
        Self {
            creator_db,
            shared_pool,
            nexus_home,
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
            agent_host_config: Arc::new(AgentHostConfig::default()),
            shutdown_notify: Arc::new(Notify::new()),
            daemon_tool_dispatch: Arc::new(None),
            memory_review_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            tls_fingerprint: Arc::new(None),
        }
    }

    /// Initialize workspace state — create nexus home and optionally open `SQLite` database.
    ///
    /// Creates the `~/.nexus42/` system layout and config skeleton on every boot.
    /// The creator `state.db` is opened lazily **only when** `active_creator_id` is
    /// present in config, via [`ensure_creator_pool`]. This allows the daemon to
    /// boot without a Profile selected (AC-P0-1, AC-P0-6).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Home directory cannot be determined
    /// - System directory creation fails
    /// - CLI config cannot be read
    pub async fn initialize() -> anyhow::Result<Self> {
        let user_home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;

        let nexus_home = user_home.join(".nexus42");

        // Create system layout and config skeleton (AC-P0-6).
        nexus_home_layout::ensure_system_layout(&nexus_home)?;

        // Read runtime mode from CLI config
        let cli_snapshot = crate::config::CliConfigSnapshot::load(&nexus_home)?;
        let runtime_mode = cli_snapshot.runtime_mode.unwrap_or(RuntimeMode::LocalOnly);

        // Apply the same default workspace root as the CLI and desktop shell.
        let workspace_path = cli_snapshot
            .workspace_path
            .clone()
            .unwrap_or_else(crate::config::resolve_default_workspace_path);
        if let Err(e) = std::fs::create_dir_all(&workspace_path) {
            tracing::warn!(
                path = %workspace_path.display(),
                error = %e,
                "failed to create default workspace root"
            );
        }

        // Try to open creator DB — non-fatal if no active creator (AC-P0-1).
        let (db, db_path, narrative_gateway, session_manager) =
            Self::try_open_creator_db(&user_home, &nexus_home).await;

        let agent_host_config =
            nexus_agent_host::config::load_config(&nexus_home).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "failed to load agent host config; using defaults");
                AgentHostConfig::default()
            });

        if db.is_some() {
            tracing::info!("Workspace state.db at {:?}", db_path);
        } else {
            tracing::info!(
                "No active creator — creator state.db deferred (lazy-open on Profile attach)"
            );
        }

        let shared_pool = Self::new_shared_pool_handle(db.as_ref());
        Ok(Self {
            creator_db: Self::new_creator_db_slot(db, db_path, narrative_gateway, session_manager),
            shared_pool,
            nexus_home,
            started_at: std::time::Instant::now(),
            started_at_wall: chrono::Utc::now(),
            workspace_path: Arc::new(std::sync::Mutex::new(Some(
                workspace_path.to_string_lossy().to_string(),
            ))),
            runtime_mode,
            lifecycle: Arc::new(None),
            engine: Arc::new(None),
            worker_manager: Arc::new(None),
            capability_registry: Arc::new(None),
            schedule_supervisor: Arc::new(None),
            agent_host: Arc::new(None),
            agent_host_config: Arc::new(agent_host_config),
            shutdown_notify: Arc::new(Notify::new()),
            daemon_tool_dispatch: Arc::new(None),
            memory_review_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            tls_fingerprint: Arc::new(None),
        })
    }

    fn new_creator_db_slot(
        db: Option<DbPool>,
        db_path: Option<PathBuf>,
        narrative_gateway: Option<Arc<SqliteNarrativeGateway>>,
        session_manager: Option<Arc<WorkspaceSessionManager>>,
    ) -> Arc<RwLock<CreatorDbSlot>> {
        Arc::new(RwLock::new(CreatorDbSlot {
            db,
            db_path,
            narrative_gateway,
            session_manager,
        }))
    }

    fn new_shared_pool_handle(db: Option<&DbPool>) -> Arc<OnceLock<Arc<sqlx::SqlitePool>>> {
        let handle = Arc::new(OnceLock::new());
        if let Some(db) = db {
            let _ = handle.set(Arc::new(db.pool().clone()));
        }
        handle
    }

    fn publish_shared_pool(&self, db: &DbPool) {
        if self.shared_pool.get().is_none() {
            let _ = self.shared_pool.set(Arc::new(db.pool().clone()));
        }
    }

    fn creator_db_read(&self) -> std::sync::RwLockReadGuard<'_, CreatorDbSlot> {
        self.creator_db.read().unwrap_or_else(|poisoned| {
            tracing::warn!("creator_db mutex poisoned, recovering");
            poisoned.into_inner()
        })
    }

    fn creator_db_write(&self) -> std::sync::RwLockWriteGuard<'_, CreatorDbSlot> {
        self.creator_db.write().unwrap_or_else(|poisoned| {
            tracing::warn!("creator_db mutex poisoned, recovering");
            poisoned.into_inner()
        })
    }

    /// Try to open the creator DB if `active_creator_id` is present in config.
    ///
    /// Returns `(None, None, None, None)` when no creator is active — the
    /// daemon can boot without a creator DB (T0/T1 tier).
    async fn try_open_creator_db(
        user_home: &Path,
        nexus_home: &Path,
    ) -> (
        Option<DbPool>,
        Option<PathBuf>,
        Option<Arc<SqliteNarrativeGateway>>,
        Option<Arc<WorkspaceSessionManager>>,
    ) {
        let Some(db_path) = crate::config::try_resolve_state_db_path(user_home, nexus_home) else {
            return (None, None, None, None);
        };

        if let Some(parent) = db_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!(
                    path = %parent.display(),
                    error = %e,
                    "failed to create creator DB parent directory"
                );
                return (None, None, None, None);
            }
        }

        // Initialize schema and create connection pool (same pattern as original initialize)
        if let Err(e) = crate::db::schema::Schema::init(&db_path).await {
            tracing::warn!(error = %e, "failed to init creator schema; deferring DB open");
            return (None, None, None, None);
        }
        let db = match DbPool::new(&db_path, PoolConfig::from_env()).await {
            Ok(pool) => pool,
            Err(e) => {
                tracing::warn!(error = %e, "failed to create DbPool; deferring DB open");
                return (None, None, None, None);
            }
        };

        let narrative_gateway = Arc::new(SqliteNarrativeGateway::new(db.pool().clone()));
        let session_manager = Arc::new(WorkspaceSessionManager::new(Arc::new(db.pool().clone())));
        (
            Some(db),
            Some(db_path),
            Some(narrative_gateway),
            Some(session_manager),
        )
    }

    /// Lazily open the creator DB pool if not already open.
    ///
    /// Called on Profile attach (`set_active_creator`) or when a Tier-2 handler
    /// finds `active_creator_id` in config. Idempotent: no-ops if pool already
    /// open for the same creator.
    ///
    /// # Errors
    ///
    /// Returns an error if the creator DB path cannot be resolved, schema init
    /// fails, or pool creation fails.
    pub async fn ensure_creator_pool(&self) -> anyhow::Result<()> {
        if self.pool().is_some() {
            return Ok(()); // already open
        }

        let user_home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        let (db, db_path, narrative_gateway, session_manager) =
            Self::try_open_creator_db(&user_home, &self.nexus_home).await;

        match (db, db_path, narrative_gateway, session_manager) {
            (Some(db), Some(db_path), Some(narrative_gateway), Some(session_manager)) => {
                self.publish_shared_pool(&db);
                let mut slot = self.creator_db_write();
                if slot.db.is_some() {
                    return Ok(()); // concurrent attach won the race
                }
                slot.db = Some(db);
                slot.db_path = Some(db_path);
                slot.narrative_gateway = Some(narrative_gateway);
                slot.session_manager = Some(session_manager);
                drop(slot);
                Ok(())
            }
            _ => Err(anyhow::anyhow!(
                "Failed to open creator database — no active creator or path resolution failed"
            )),
        }
    }

    /// Set the TLS certificate fingerprint for remote binds.
    pub fn set_tls_fingerprint(&mut self, fingerprint: Option<CertFingerprintResponse>) {
        self.tls_fingerprint = Arc::new(fingerprint);
    }

    /// Get the TLS certificate fingerprint, if any.
    #[must_use]
    pub fn tls_fingerprint(&self) -> Option<CertFingerprintResponse> {
        self.tls_fingerprint.as_ref().clone()
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

    /// Set the agent host configuration.
    /// Called from boot.rs after loading the config from disk.
    pub fn set_agent_host_config(&mut self, config: AgentHostConfig) {
        self.agent_host_config = Arc::new(config);
    }

    /// Get the agent host configuration.
    #[must_use]
    pub fn agent_host_config(&self) -> Arc<AgentHostConfig> {
        Arc::clone(&self.agent_host_config)
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
    /// Returns `None` when no creator DB is open (boot before Profile attach).
    #[must_use]
    pub fn narrative_gateway(&self) -> Option<Arc<SqliteNarrativeGateway>> {
        self.creator_db_read().narrative_gateway.clone()
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

    /// Get a reference to the underlying sqlx pool, if open.
    /// Returns `None` when no creator DB is open (boot before Profile attach).
    #[must_use]
    pub fn pool(&self) -> Option<&sqlx::SqlitePool> {
        self.shared_pool.get().map(std::convert::AsRef::as_ref)
    }

    /// Get the pool or return `Uninitialized` error.
    /// Convenience for Tier-2 handlers that require an active creator.
    ///
    /// # Errors
    ///
    /// Returns [`NexusApiError::Uninitialized`] when no creator DB pool is open
    /// (daemon booted without an active creator and no attach has occurred).
    pub fn pool_or_uninit(&self) -> Result<&sqlx::SqlitePool, NexusApiError> {
        self.pool().ok_or(NexusApiError::Uninitialized)
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
    /// Returns `None` when no creator DB is open (boot before Profile attach).
    #[must_use]
    pub fn database_path(&self) -> Option<String> {
        self.creator_db_read()
            .db_path
            .as_ref()
            .map(|p| p.display().to_string())
    }

    /// Get nexus home directory.
    #[must_use]
    pub const fn nexus_home(&self) -> &PathBuf {
        &self.nexus_home
    }

    /// Get a clone of the database pool (for `TokenManager`, etc.)
    /// Returns `None` when no creator DB is open (boot before Profile attach).
    #[must_use]
    pub fn db_pool(&self) -> Option<DbPool> {
        self.creator_db_read().db.clone()
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
    /// Returns `None` when no creator DB is open (boot before Profile attach).
    #[must_use]
    pub fn session_manager(&self) -> Option<Arc<WorkspaceSessionManager>> {
        self.creator_db_read().session_manager.clone()
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
    /// - No creator DB is open (`init_workspace` requires an active creator)
    pub async fn init_workspace(&self, path: &str) -> anyhow::Result<()> {
        let pool = self.pool().ok_or_else(|| {
            anyhow::anyhow!("Cannot initialize workspace: no active creator database")
        })?;
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
        .execute(pool)
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
    use serial_test::serial;

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

    /// AC-P0-1: boot initializes without `active_creator_id` and no pool.
    #[tokio::test]
    #[serial]
    async fn initialize_without_active_creator_has_no_pool() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let user_home = tmp.path();
        let nexus_home = user_home.join(".nexus42");
        nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", user_home);

        let state = WorkspaceState::initialize().await.expect("initialize");
        assert!(
            state.pool().is_none(),
            "no active creator should leave pool closed at boot"
        );

        match original_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }

    /// AC-P0-5: after Profile attach config is written, `ensure_creator_pool` opens the DB.
    #[tokio::test]
    #[serial]
    async fn ensure_creator_pool_opens_after_active_creator_attach() {
        const CREATOR_ID: &str = "crt_attach_test";

        let tmp = tempfile::TempDir::new().expect("temp dir");
        let user_home = tmp.path();
        let nexus_home = user_home.join(".nexus42");
        nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

        let cache = serde_json::json!({
            "creators": {
                CREATOR_ID: { "handle": "attach-test" }
            }
        });
        std::fs::write(
            nexus_home.join("creator_identity_cache.json"),
            serde_json::to_string_pretty(&cache).expect("cache json"),
        )
        .expect("write cache");

        let op_dir = nexus_home_layout::operational_workspace_dir(user_home, CREATOR_ID, "default");
        std::fs::create_dir_all(&op_dir).expect("operational dir");
        let meta = serde_json::json!({
            "schema_version": 1,
            "creator_id": CREATOR_ID,
            "workspace_slug": "default",
            "local_root": user_home.join("creative"),
            "created_at": "2020-01-01T00:00:00Z"
        });
        std::fs::write(
            op_dir.join("meta.json"),
            serde_json::to_string(&meta).expect("meta json"),
        )
        .expect("meta.json");

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", user_home);

        let state = WorkspaceState::initialize().await.expect("initialize");
        assert!(state.pool().is_none());

        let config_toml = format!("active_creator_id = \"{CREATOR_ID}\"\n");
        std::fs::write(nexus_home.join("config.toml"), config_toml).expect("config.toml");

        state
            .ensure_creator_pool()
            .await
            .expect("ensure_creator_pool after attach");
        assert!(
            state.pool().is_some(),
            "pool should be open after Profile attach"
        );

        match original_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }
}
