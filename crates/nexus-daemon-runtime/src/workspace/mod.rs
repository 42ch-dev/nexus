//! Workspace Management Module
//!
//! # Mutex Poisoning Policy
//!
//! This crate uses `unwrap_or_else` on mutex locks to recover from poisoned mutexes.
//! A poisoned mutex means a thread panicked while holding the lock. Rather than
//! crashing the entire daemon, we recover the lock and log a warning. The data
//! may be in an inconsistent state, but for a local development tool this is
//! preferable to a hard crash.

pub mod actor_sessions;
pub mod authority;
pub mod bounds;
pub mod commit_fs;
pub mod executor;
pub mod manager;
pub mod scope;
pub mod session;
pub mod session_commit;

use crate::api::errors::NexusApiError;
use crate::db::pool::{DbPool, PoolConfig};
use crate::db::SqliteNarrativeGateway;
use crate::lifecycle::{Lifecycle, LifecycleState, StatigLifecycle};
use crate::workspace::actor_sessions::ActorSessionRegistry;
use crate::workspace::session::WorkspaceSessionManager;
use graph_flow::SessionStorage as GraphFlowSessionStorage;
use nexus_agent_host::config::AgentHostConfig;
use nexus_contracts::local::domain::RuntimeMode;
use nexus_contracts::CertFingerprintResponse;
use nexus_orchestration::{
    engine::OrchestrationEngine, run_state::WorkflowStateStore,
    schedule::supervisor::ScheduleSupervisor, storage::sqlite::SqliteSessionStorage,
    CapabilityRegistry, CapabilityRegistryHolder, GraphFlowEngine,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Outcome of attempting to open the creator DB pool (V1.119 QC2-C-001).
///
/// `open_error` captures the diagnostic when the open was attempted but failed
/// (e.g. schema migration error). It is `None` when no creator was active (the
/// normal lazy-open-deferred case at boot) or when the open succeeded. Carrying
/// this detail lets [`WorkspaceState::ensure_creator_pool`] return a descriptive
/// error whose message the web classifier can match (AC-P0-3).
struct CreatorDbOutcome {
    db: Option<DbPool>,
    db_path: Option<PathBuf>,
    narrative_gateway: Option<Arc<SqliteNarrativeGateway>>,
    session_manager: Option<Arc<WorkspaceSessionManager>>,
    open_error: Option<String>,
}

/// One immutable aggregate runtime bundle (N-2).
///
/// All components — engine, capability registry holder, prompt executor,
/// coordinator, supervisor, and the shared pool — are constructed off to
/// the side and published together in a SINGLE readiness cell. Accessors
/// read the aggregate, so a reader can never observe a mixed/partial bundle
/// (e.g. a coordinator from one publication phase with a supervisor from
/// another). The individual slots remain for the boot path (which wires
/// components incrementally before serving) and are only consulted as a
/// fallback when no aggregate has been published.
pub struct RuntimeBundle {
    /// The orchestration engine over the durable store.
    engine: Arc<dyn OrchestrationEngine>,
    /// The capability registry holder (pool-backed + Host prompt executor
    /// for the lazy path; boot holder for the boot path).
    capability_holder: CapabilityRegistryHolder,
    /// The production prompt executor (A1), when a Host facade is wired.
    prompt_executor: Option<Arc<dyn nexus_orchestration::capability::PromptExecutor>>,
    /// The single public run coordinator (A3).
    coordinator: Arc<crate::preset_run::WorkflowRunCoordinator>,
    /// The schedule supervisor with the daemon admission starter.
    supervisor: Arc<ScheduleSupervisor>,
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
    /// `RwLock`-backed so the lazy Profile-attach bundle (I-3) can publish
    /// the durable engine from `&self` (`ensure_creator_pool`).
    engine: Arc<RwLock<Option<Arc<dyn OrchestrationEngine>>>>,
    /// Capability registry holder (set at daemon startup when WS2 is wired;
    /// V1.176 P1, AR-92 #2). The holder is shared with the engine and the
    /// hot-reload watcher; every `capability_registry()` read clones the
    /// current registry under the holder's read lock.
    capability_registry: Arc<RwLock<Option<CapabilityRegistryHolder>>>,
    /// Schedule supervisor for WS7 schedule management (set at daemon startup).
    schedule_supervisor: Arc<RwLock<Option<Arc<ScheduleSupervisor>>>>,
    /// Single public run coordinator (v1.186 P2 T1, A3) — one cancellation/
    /// join owner per (Creator DB, session) around the bounded
    /// `drive_preset_run` loop. Set at daemon boot when a creator DB is
    /// present; `None` on Tier-0 boot (deferred until Profile attach).
    run_coordinator: Arc<RwLock<Option<Arc<crate::preset_run::WorkflowRunCoordinator>>>>,
    /// Production prompt executor (A1, v1.186 P1 T2) — the daemon-owned
    /// `HostPromptExecutor` over the Host facade. Set at daemon boot when a
    /// creator DB is present; `None` on Tier-0 boot. Schedule admission
    /// resolves it here to wire the same executor into driven graphs.
    prompt_executor: Arc<RwLock<Option<Arc<dyn nexus_orchestration::capability::PromptExecutor>>>>,
    /// Shared per-run cancellation tokens (A1, v1.186 P1 T2) — the same map
    /// the engine registers run tokens in and the prompt executor resolves.
    /// Created at daemon boot; the lazy-attach bundle shares it.
    session_cancels: Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
    /// One async initialization gate for the lazy-attach runtime bundle
    /// (N-2): concurrent `ensure_creator_pool` callers serialize here and
    /// re-check; only the winner opens the pool and publishes the bundle.
    /// Readers can never observe a duplicate or mixed bundle — there is
    /// exactly one builder and the readiness slot (`run_coordinator`) is
    /// published after its dependencies (engine, prompt executor).
    bundle_gate: Arc<tokio::sync::Mutex<()>>,
    /// Immutable aggregate runtime bundle (N-2): published as ONE cell after
    /// every component is wired. Accessors read this aggregate; the
    /// individual slots are the boot path's incremental wiring and a
    /// fallback when no aggregate has been published.
    runtime_bundle: Arc<RwLock<Option<Arc<RuntimeBundle>>>>,
    /// Agent host facade (set at daemon startup when agent host subsystem is wired).
    agent_host: Arc<Option<Arc<dyn nexus_agent_host::HostFacade>>>,
    /// Process-lifetime Actor session indexes over `HostFacade` (v1.184 P2).
    actor_sessions: ActorSessionRegistry,
    /// Agent host configuration loaded at boot from `agent-host/config.toml`.
    agent_host_config: Arc<AgentHostConfig>,
    /// Shutdown notification — fired when the daemon enters Stopping state.
    /// Consumers (HTTP server, engine drainer) await this to initiate graceful shutdown.
    shutdown_notify: Arc<Notify>,
    /// Shutdown gate (Bugbot PR #234): raised by [`WorkspaceState::request_shutdown`]
    /// BEFORE the `notify_waiters` broadcast. `notify_waiters` wakes only
    /// waiters registered at broadcast time and stores no permit, so a
    /// consumer that would register AFTER the broadcast (an embedded MCP
    /// session established post-shutdown) must consult this flag instead
    /// of awaiting [`WorkspaceState::shutdown_notify`] forever.
    shutdown_requested: Arc<AtomicBool>,
    /// Daemon-side tool dispatch for nexus.* tools (DF-47, V1.42 P3).
    /// Set at daemon boot so schedule-executed `HostToolCallTask` can invoke tools.
    daemon_tool_dispatch:
        Arc<RwLock<Option<Arc<dyn nexus_orchestration::capability::DaemonToolDispatch>>>>,
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
    /// V1.147 P0: daemon-wide WASM compute engine singleton (boot.rs wires
    /// the same engine into `narrative.compute` via the capability registry).
    wasm_engine: Arc<Option<Arc<nexus_wasm_host::WasmEngine>>>,
    /// V1.147 P0: daemon-wide compiled module cache (pre-warmed at boot with
    /// embedded + user-installed modules).
    module_cache: Arc<Option<Arc<nexus_wasm_host::ModuleCache>>>,
    /// V1.147 P0 fix wave (W-2): serializes `engine.compute` invocations on
    /// the shared daemon engine. The wasmtime epoch counter is engine-global
    /// (`increment_epoch` is a single atomic on the engine) — two concurrent
    /// runs would share the shortest wall-time budget (the first watchdog to
    /// fire traps every running invocation). A `Semaphore(1)` around compute
    /// makes each invocation's watchdog observe only its own budget; it also
    /// caps W-1's worst case (one CPU-bound compute at a time).
    compute_serializer: Arc<tokio::sync::Semaphore>,
    /// V1.179 P0 T1 (DF-88): boot-scoped embedded MCP server (Model B).
    /// Stored at daemon boot (`boot.rs` §8.5) so in-daemon consumers can
    /// `establish()` on the ONE boot instance (GC #9 enablement gate: the
    /// `PeerToolsConfig.embedded_mcp` key OR the `--embedded-mcp` CLI flag;
    /// the cargo `embedded-mcp` feature is the hard gate). The session
    /// budget is process-global — every `EmbeddedMcpServer` handle shares
    /// the same registry, so the boot instance and any consumer-constructed
    /// handle count against the same `EMBEDDED_MCP_MAX_SESSIONS` cap.
    #[cfg(feature = "embedded-mcp")]
    embedded_mcp_server: Arc<Option<Arc<crate::connect::mcp_embedded::EmbeddedMcpServer>>>,
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
        let session_manager = Arc::new(
            WorkspaceSessionManager::new_recoverable(
                Arc::new(db.pool().clone()),
                db_path.clone(),
            )
            .expect("workspace authority lease"),
        );
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
            engine: Arc::new(RwLock::new(None)),
            capability_registry: Arc::new(RwLock::new(None)),
            schedule_supervisor: Arc::new(RwLock::new(None)),
            run_coordinator: Arc::new(RwLock::new(None)),
            prompt_executor: Arc::new(RwLock::new(None)),
            session_cancels: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            bundle_gate: Arc::new(tokio::sync::Mutex::new(())),
            runtime_bundle: Arc::new(RwLock::new(None)),
            agent_host: Arc::new(None),
            actor_sessions: ActorSessionRegistry::new(),
            agent_host_config: Arc::new(AgentHostConfig::default()),
            shutdown_notify: Arc::new(Notify::new()),
            shutdown_requested: Arc::new(AtomicBool::new(false)),
            daemon_tool_dispatch: Arc::new(RwLock::new(None)),
            memory_review_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            tls_fingerprint: Arc::new(None),
            wasm_engine: Arc::new(None),
            module_cache: Arc::new(None),
            compute_serializer: Arc::new(tokio::sync::Semaphore::new(1)),
            #[cfg(feature = "embedded-mcp")]
            embedded_mcp_server: Arc::new(None),
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
        let outcome = Self::try_open_creator_db(&user_home, &nexus_home).await;
        let (db, db_path, narrative_gateway, session_manager) = (
            outcome.db,
            outcome.db_path,
            outcome.narrative_gateway,
            outcome.session_manager,
        );
        // `open_error` is intentionally ignored at boot — lazy-open defers when
        // there is no active creator; the error resurfaces on the first Tier-2
        // request via `ensure_creator_pool`.

        // `load_config` takes the USER home and resolves
        // `$HOME/.nexus42/agent-host/config.toml` itself; passing the already
        // resolved `nexus_home` double-nested the path and left the canonical
        // file unread (tri-QC P1-A).
        let agent_host_config =
            nexus_agent_host::config::load_config(&user_home).unwrap_or_else(|e| {
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
            engine: Arc::new(RwLock::new(None)),
            capability_registry: Arc::new(RwLock::new(None)),
            schedule_supervisor: Arc::new(RwLock::new(None)),
            run_coordinator: Arc::new(RwLock::new(None)),
            prompt_executor: Arc::new(RwLock::new(None)),
            session_cancels: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            bundle_gate: Arc::new(tokio::sync::Mutex::new(())),
            runtime_bundle: Arc::new(RwLock::new(None)),
            agent_host: Arc::new(None),
            actor_sessions: ActorSessionRegistry::new(),
            agent_host_config: Arc::new(agent_host_config),
            shutdown_notify: Arc::new(Notify::new()),
            shutdown_requested: Arc::new(AtomicBool::new(false)),
            daemon_tool_dispatch: Arc::new(RwLock::new(None)),
            memory_review_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            tls_fingerprint: Arc::new(None),
            wasm_engine: Arc::new(None),
            module_cache: Arc::new(None),
            compute_serializer: Arc::new(tokio::sync::Semaphore::new(1)),
            #[cfg(feature = "embedded-mcp")]
            embedded_mcp_server: Arc::new(None),
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

    /// True when the creator DB slot is fully populated (pool + gateways).
    ///
    /// Used instead of `pool().is_some()` for lazy-open readiness so concurrent
    /// Tier-2 requests never observe a published pool before `narrative_gateway`
    /// / `session_manager` are wired (V1.118 P0 T2 fix F1).
    fn creator_pool_ready(&self) -> bool {
        let slot = self.creator_db_read();
        slot.db.is_some() && slot.narrative_gateway.is_some() && slot.session_manager.is_some()
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
    /// Returns a [`CreatorDbOutcome`] whose components are all `None` when no
    /// creator is active — the daemon can boot without a creator DB (T0/T1
    /// tier). When the open is attempted but fails (schema migration, pool
    /// creation), `open_error` captures a diagnostic so callers can surface a
    /// meaningful error instead of a generic "no active creator" message
    /// (V1.119 QC2-C-001 / AC-P0-3).
    async fn try_open_creator_db(user_home: &Path, nexus_home: &Path) -> CreatorDbOutcome {
        let Some(db_path) = crate::config::try_resolve_state_db_path(user_home, nexus_home) else {
            return CreatorDbOutcome {
                db: None,
                db_path: None,
                narrative_gateway: None,
                session_manager: None,
                open_error: None,
            };
        };

        if let Some(parent) = db_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!(
                    path = %parent.display(),
                    error = %e,
                    "failed to create creator DB parent directory"
                );
                return CreatorDbOutcome {
                    db: None,
                    db_path: None,
                    narrative_gateway: None,
                    session_manager: None,
                    open_error: Some(format!("Failed to create database directory: {e}")),
                };
            }
        }

        // Initialize schema and create connection pool (same pattern as original initialize)
        if let Err(e) = crate::db::schema::Schema::init(&db_path).await {
            tracing::warn!(error = %e, "failed to init creator schema; deferring DB open");
            return CreatorDbOutcome {
                db: None,
                db_path: None,
                narrative_gateway: None,
                session_manager: None,
                // Message must contain "migration" so the web classifier's
                // `/migration/i` regex matches (AC-P0-3).
                open_error: Some(format!("Failed to run database migrations: {e}")),
            };
        }
        let db = match DbPool::new(&db_path, PoolConfig::from_env()).await {
            Ok(pool) => pool,
            Err(e) => {
                tracing::warn!(error = %e, "failed to create DbPool; deferring DB open");
                return CreatorDbOutcome {
                    db: None,
                    db_path: None,
                    narrative_gateway: None,
                    session_manager: None,
                    open_error: Some(format!("Failed to create database connection pool: {e}")),
                };
            }
        };

        let narrative_gateway = Arc::new(SqliteNarrativeGateway::new(db.pool().clone()));
        let session_manager = Arc::new(
            WorkspaceSessionManager::new_recoverable(
                Arc::new(db.pool().clone()),
                db_path.clone(),
            )
            .expect("workspace authority lease"),
        );
        CreatorDbOutcome {
            db: Some(db),
            db_path: Some(db_path),
            narrative_gateway: Some(narrative_gateway),
            session_manager: Some(session_manager),
            open_error: None,
        }
    }

    /// Lazily open the creator DB pool if not already open.
    ///
    /// Called on Profile attach (`set_active_creator`) or when a Tier-2 handler
    /// finds `active_creator_id` in config. Idempotent: no-ops if pool already
    /// open for the same creator.
    ///
    /// I-3/N-2: when the pool is opened lazily (Tier-0 boot had no creator DB),
    /// the matching runtime bundle is published BEFORE the attach is
    /// reported ready: durable `SqliteSessionStorage`/`WorkflowStateStore`,
    /// a `GraphFlowEngine` configured with that store, a
    /// `HostPromptExecutor`, a `WorkflowRunCoordinator`, and a
    /// `ScheduleSupervisor` with `ScheduleRunStarter`. The whole
    /// open-and-publish sequence runs under ONE async initialization gate:
    /// concurrent callers serialize, re-check, and observe the winner's
    /// complete bundle — never a mixed engine/coordinator/supervisor state
    /// and never a duplicate bundle (N-2).
    ///
    /// # Errors
    ///
    /// Returns an error if the creator DB path cannot be resolved, schema init
    /// fails, or pool creation fails.
    pub async fn ensure_creator_pool(&self) -> anyhow::Result<()> {
        // Fast path: the pool AND its matching bundle are already published.
        if self.creator_pool_ready() && self.run_coordinator().is_some() {
            return Ok(());
        }

        // N-2: one async initialization gate for the open-and-publish
        // sequence. The first caller opens the pool and publishes the
        // bundle; every concurrent caller waits here, re-checks, and returns
        // without building a second bundle.
        let _gate = self.bundle_gate.lock().await;

        // Re-check under the gate: the winner may have completed while we
        // waited.
        if self.creator_pool_ready() && self.run_coordinator().is_some() {
            return Ok(());
        }

        if !self.creator_pool_ready() {
            let user_home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
            let CreatorDbOutcome {
                db,
                db_path,
                narrative_gateway,
                session_manager,
                open_error,
            } = Self::try_open_creator_db(&user_home, &self.nexus_home).await;

            if let (Some(db), Some(db_path), Some(narrative_gateway), Some(session_manager)) =
                (db, db_path, narrative_gateway, session_manager)
            {
                session_manager
                    .startup_recovery()
                    .await
                    .map_err(|e| anyhow::anyhow!("workspace startup recovery failed: {e}"))?;
                {
                    let mut slot = self.creator_db_write();
                    if slot.db.is_none() {
                        slot.db = Some(db);
                        slot.db_path = Some(db_path);
                        slot.narrative_gateway = Some(narrative_gateway);
                        slot.session_manager = Some(session_manager);
                        if let Some(db_ref) = slot.db.as_ref() {
                            self.publish_shared_pool(db_ref);
                        }
                    }
                }
            } else {
                // Propagate the captured diagnostic so the web classifier can
                // detect migration-class failures (AC-P0-3). Falls back to a
                // generic message only when no creator was active.
                let detail = open_error
                    .unwrap_or_else(|| "no active creator or path resolution failed".to_string());
                return Err(anyhow::anyhow!("Failed to open creator database: {detail}"));
            }
        }

        // Publish the matching runtime bundle before this attach is treated
        // as ready by callers that only saw the pool (I-3). Under the gate
        // exactly one caller reaches this point; a second caller re-checks
        // `run_coordinator().is_some()` above and returns.
        if self.run_coordinator().is_none() {
            self.publish_lazy_attach_bundle().await?;
        }

        Ok(())
    }

    /// Publish the immutable aggregate runtime bundle from the boot-wired
    /// slots (N-2): called at the END of daemon boot, after the engine,
    /// capability holder, prompt executor, coordinator, and schedule
    /// supervisor are all wired, and BEFORE route readiness is exposed.
    ///
    /// Production accessors read this single readiness cell; a route can
    /// never observe a mixed/partial bundle (e.g. a coordinator from one
    /// wiring phase with a supervisor from another). The individual slots
    /// remain for the boot path's incremental wiring and as a fallback when
    /// no aggregate has been published (Tier-0 boot before Profile attach).
    ///
    /// N-12: routes through the SAME async once/initialization gate as
    /// [`Self::publish_creator_runtime_bundle`] — concurrent callers
    /// serialize, re-check, and observe the winner's aggregate; the
    /// aggregate is written exactly once and no synchronous ungated
    /// publication bypass remains.
    ///
    /// # Errors
    /// Returns an error when a required component is missing (the caller
    /// must wire every component before publishing).
    pub async fn publish_boot_runtime_bundle(&self) -> anyhow::Result<()> {
        if self.runtime_bundle().is_some() {
            return Ok(());
        }
        // N-12: one async initialization gate for the publish sequence.
        let _gate = self.bundle_gate.lock().await;
        // Re-check under the gate: the winner may have completed while we
        // waited.
        if self.runtime_bundle().is_some() {
            return Ok(());
        }
        self.publish_boot_runtime_bundle_inner()
    }

    /// The gated boot-bundle publication body (N-12): reads the boot-wired
    /// slots and writes the aggregate exactly once. Private — the only
    /// public entry is the async gated [`Self::publish_boot_runtime_bundle`].
    fn publish_boot_runtime_bundle_inner(&self) -> anyhow::Result<()> {
        let engine = self
            .engine
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| anyhow::anyhow!("engine not wired before boot bundle publish"))?;
        let capability_holder = self
            .capability_registry
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| {
                anyhow::anyhow!("capability registry not wired before boot bundle publish")
            })?;
        let coordinator = self
            .run_coordinator
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| {
                anyhow::anyhow!("run coordinator not wired before boot bundle publish")
            })?;
        let supervisor = self
            .schedule_supervisor
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| {
                anyhow::anyhow!("schedule supervisor not wired before boot bundle publish")
            })?;
        let prompt_executor = self
            .prompt_executor
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let bundle = Arc::new(RuntimeBundle {
            engine,
            capability_holder,
            prompt_executor,
            coordinator,
            supervisor,
        });
        *self
            .runtime_bundle
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(bundle);
        tracing::info!("boot: published immutable aggregate runtime bundle");
        Ok(())
    }

    /// Publish the durable Creator-DB runtime bundle (engine, coordinator,
    /// supervisor, prompt executor) over the currently attached pool.
    ///
    /// Used by lazy Profile attach and by hermetic production-daemon tests.
    ///
    /// N-2b: routes through the SAME async once/initialization gate as
    /// [`Self::ensure_creator_pool`] — concurrent callers serialize,
    /// re-check, and observe the winner's complete bundle; no caller can
    /// publish a second aggregate over an existing creator runtime.
    ///
    /// # Errors
    /// Returns an error if the creator pool is missing or engine construction fails.
    pub async fn publish_creator_runtime_bundle(&self) -> anyhow::Result<()> {
        // Fast path: the matching bundle is already published.
        if self.runtime_bundle().is_some() {
            return Ok(());
        }
        // N-2b: one async initialization gate for the publish sequence.
        let _gate = self.bundle_gate.lock().await;
        // Re-check under the gate: the winner may have completed while we
        // waited.
        if self.runtime_bundle().is_some() {
            return Ok(());
        }
        self.publish_lazy_attach_bundle().await
    }

    #[allow(clippy::too_many_lines)] // single atomic aggregate publish; extracting stages adds cross-struct invariants
    async fn publish_lazy_attach_bundle(&self) -> anyhow::Result<()> {
        let pool = self
            .pool()
            .ok_or_else(|| anyhow::anyhow!("creator pool not published after attach"))?;
        let pool_arc = Arc::new(pool.clone());

        // Durable storage + workflow store (the SQLite adapter implements both).
        let sqlite_storage = Arc::new(SqliteSessionStorage::new(pool_arc.clone()));
        let storage: Arc<dyn GraphFlowSessionStorage> = sqlite_storage.clone();
        let workflow_store: Arc<dyn WorkflowStateStore> = sqlite_storage.clone();

        // Prompt executor: the daemon-owned HostPromptExecutor over the
        // Host facade (A1). When no Host facade is wired (tests), the
        // executor is absent and LLM-backed capabilities fail closed.
        let prompt_executor: Option<Arc<dyn nexus_orchestration::capability::PromptExecutor>> =
            self.agent_host().map(|host| {
                let host_config = self.agent_host_config();
                let executor: Arc<dyn nexus_orchestration::capability::PromptExecutor> =
                    Arc::new(crate::prompt_executor::HostPromptExecutor::new(
                        host,
                        workflow_store.clone(),
                        host_config.timeouts.clone(),
                    ));
                executor
            });

        // N-2b: REBUILD the capability registry with the lazy-attach runtime
        // dependencies (Creator DB pool + production Host prompt executor)
        // while preserving the existing user-capability/wasm configuration.
        // A real Tier-0 boot builds the holder with `pool: None` and no
        // executor; reusing that holder would leave pool-backed and
        // LLM-backed capabilities without the newly opened pool/executor.
        // The rebuild runs the SAME scan/admission path as boot
        // (`with_runtime_deps_and_user_caps`), so user capabilities and the
        // daemon-wide WASM singleton are preserved.
        let holder = {
            let workspace_executor: Option<
                Arc<dyn nexus_orchestration::capability::WorkspaceExecutor>,
            > = self.session_manager().and_then(|mgr| {
                self.workspace_path().map(|root| {
                    Arc::new(crate::workspace::executor::DaemonWorkspaceExecutor::new(
                        mgr,
                        root,
                    )) as Arc<dyn nexus_orchestration::capability::WorkspaceExecutor>
                })
            });
            let deps = nexus_orchestration::capability::CapabilityRuntimeDeps {
                pool: Some(pool_arc.as_ref().clone()),
                prompt_executor: prompt_executor.clone(),
                session_cancels: self.session_cancels(),
                daemon_tool_dispatch: self.daemon_tool_dispatch(),
                cdn_config: None,
                workspace_executor,
            };
            let scan_dir = crate::boot::user_capabilities_scan_dir(self);
            let (registry, _outcome) = match (self.wasm_engine(), self.module_cache()) {
                (Some(engine), Some(cache)) => {
                    nexus_orchestration::capability::CapabilityRegistry::with_runtime_deps_and_wasm_and_user_caps(
                        &deps,
                        engine,
                        cache,
                        &scan_dir,
                    )
                }
                _ => {
                    nexus_orchestration::capability::CapabilityRegistry::with_runtime_deps_and_user_caps(
                        &deps,
                        &scan_dir,
                    )
                }
            };
            CapabilityRegistryHolder::with_registry(Arc::new(registry))
        };

        // Engine over the durable store.
        let workspace_root = self
            .workspace_path()
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store_and_workspace(
            storage.clone(),
            workflow_store.clone(),
            holder.clone(),
            workspace_root,
        );
        if let Some(dispatch) = self.daemon_tool_dispatch() {
            engine.set_daemon_tool_dispatch(dispatch);
        }
        if let Some(executor) = &prompt_executor {
            engine.set_prompt_executor(executor.clone(), self.session_cancels());
        }
        engine.set_nexus_home(self.nexus_home().clone());

        let engine_arc = Arc::new(engine);

        // Coordinator over the same engine/storage/pool. When a Host facade
        // is wired, attach it so admission validates provider references in
        // agent bindings before enqueue (N-4).
        let mut coordinator_builder = crate::preset_run::WorkflowRunCoordinator::new(
            engine_arc.clone(),
            storage.clone(),
            pool_arc.clone(),
            self.session_cancels(),
        );
        if let Some(host) = self.agent_host() {
            coordinator_builder = coordinator_builder.with_agent_host(host);
        }
        // C-3: the sanctioned default binding provider for explicit legacy
        // starts — the same config source the supervisor's internal
        // insertion paths use.
        if let Some(provider_id) = self
            .agent_host_config()
            .providers
            .iter()
            .find(|p| p.enabled)
            .map(|p| p.id.clone())
        {
            coordinator_builder = coordinator_builder.with_binding_provider(provider_id);
        }
        let coordinator = Arc::new(coordinator_builder);

        // Schedule supervisor with the daemon admission callback.
        let mut supervisor_builder = ScheduleSupervisor::new_with_workspace(
            pool_arc.clone(),
            self.workspace_path().map(std::path::PathBuf::from),
        );
        if let Some(reg) = holder.get() {
            supervisor_builder = supervisor_builder.with_capability_registry(reg);
        }
        // N-9: the default binding provider for internal schedule insertion
        // is the first enabled Host provider (config order).
        if let Some(provider_id) = self
            .agent_host_config()
            .providers
            .iter()
            .find(|p| p.enabled)
            .map(|p| p.id.clone())
        {
            supervisor_builder = supervisor_builder.with_binding_provider(provider_id);
        }
        let starter = crate::boot::DaemonScheduleRunStarter {
            coordinator: coordinator.clone(),
            pool: pool_arc.clone(),
            nexus_home: self.nexus_home().clone(),
            caps: Some(holder.clone()),
            daemon_tool_dispatch: self.daemon_tool_dispatch(),
            prompt_executor: prompt_executor.clone(),
        };
        supervisor_builder = supervisor_builder.with_schedule_starter(Arc::new(starter));
        let supervisor = Arc::new(supervisor_builder);
        // T3 (A3): the coordinator settles terminal runs through the
        // supervisor. The supervisor is constructed AFTER the coordinator
        // (it needs the coordinator's starter), so the handle is attached
        // here once both exist.
        coordinator.set_schedule_supervisor(supervisor.clone());

        // T3 (A3) fix-1 (P1-2): a lazy-attached Profile opens an EXISTING
        // Creator DB whose schedules may hold checkpoint-before-settlement
        // rows (a `driven_v1` schedule with a durable terminal
        // `current_session_id` left by a prior process). Settle them from
        // the durable session status BEFORE the bundle is published so the
        // runtime never exposes an unsettled row — same reconciliation and
        // warn-not-fatal failure handling as the normal boot path.
        if let Err(e) = supervisor.reconcile_terminal_schedules().await {
            tracing::warn!("lazy attach: failed to reconcile terminal schedules: {}", e);
        }

        // N-2: publish ONE immutable aggregate after every component is
        // wired. Accessors read this cell; a reader can never observe a
        // mixed/partial bundle.
        let bundle = Arc::new(RuntimeBundle {
            engine: engine_arc.clone() as Arc<dyn OrchestrationEngine>,
            capability_holder: holder.clone(),
            prompt_executor: prompt_executor.clone(),
            coordinator: coordinator.clone(),
            supervisor: supervisor.clone(),
        });
        *self
            .runtime_bundle
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(bundle);

        // Mirror the aggregate into the individual slots so existing
        // callers (boot-shaped code, health checks) keep working.
        self.set_engine(engine_arc as Arc<dyn OrchestrationEngine>);
        self.set_capability_registry(holder);
        if let Some(executor) = &prompt_executor {
            self.set_prompt_executor(executor.clone());
        }
        self.set_run_coordinator(coordinator);
        self.set_schedule_supervisor(supervisor);

        tracing::info!(
            "lazy Profile attach: published durable Creator-DB runtime bundle \
             (engine/coordinator/supervisor)"
        );
        Ok(())
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

    /// Set the daemon-wide WASM compute engine singleton (V1.147 P0).
    pub fn set_wasm_engine(&mut self, engine: Arc<nexus_wasm_host::WasmEngine>) {
        self.wasm_engine = Arc::new(Some(engine));
    }

    /// Get the daemon-wide WASM compute engine, if set.
    #[must_use]
    pub fn wasm_engine(&self) -> Option<Arc<nexus_wasm_host::WasmEngine>> {
        self.wasm_engine.as_ref().clone()
    }

    /// Set the daemon-wide compiled module cache (V1.147 P0).
    pub fn set_module_cache(&mut self, cache: Arc<nexus_wasm_host::ModuleCache>) {
        self.module_cache = Arc::new(Some(cache));
    }

    /// Get the daemon-wide compiled module cache, if set.
    #[must_use]
    pub fn module_cache(&self) -> Option<Arc<nexus_wasm_host::ModuleCache>> {
        self.module_cache.as_ref().clone()
    }

    /// Compute serialization permit (W-2): acquire before calling
    /// `engine.compute` so concurrent runs never share the engine-global
    /// epoch watchdog budget.
    #[must_use]
    pub fn compute_serializer(&self) -> Arc<tokio::sync::Semaphore> {
        Arc::clone(&self.compute_serializer)
    }

    /// Set the lifecycle HSM for this workspace state.
    /// Called from main.rs after constructing the lifecycle.
    pub fn set_lifecycle(&mut self, lifecycle: Arc<StatigLifecycle>) {
        self.lifecycle = Arc::new(Some(lifecycle));
    }

    /// Set the orchestration engine.
    /// Called from main.rs after constructing the engine.
    pub fn set_engine(&self, engine: Arc<dyn OrchestrationEngine>) {
        *self
            .engine
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(engine);
    }

    /// Set the capability registry holder (shared with the engine and the
    /// hot-reload watcher — AR-92 #2). The registry itself is swapped into
    /// the holder by the watcher; readers clone per call.
    pub fn set_capability_registry(&self, holder: CapabilityRegistryHolder) {
        *self
            .capability_registry
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(holder);
    }

    /// Set the schedule supervisor (WS7).
    pub fn set_schedule_supervisor(&self, supervisor: Arc<ScheduleSupervisor>) {
        *self
            .schedule_supervisor
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(supervisor);
    }

    /// Set the single public run coordinator (v1.186 P2 T1, A3).
    ///
    /// Called from daemon boot when a creator DB is present. The coordinator
    /// owns one cancellation/join handle per (Creator DB, session) around
    /// the bounded `drive_preset_run` loop; schedule admission and session
    /// POST route through it.
    pub fn set_run_coordinator(&self, coordinator: Arc<crate::preset_run::WorkflowRunCoordinator>) {
        *self
            .run_coordinator
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(coordinator);
    }

    /// Set the production prompt executor (A1, v1.186 P1 T2).
    ///
    /// Called from daemon boot when a creator DB is present so schedule
    /// admission can wire the same executor into driven graphs.
    pub fn set_prompt_executor(
        &self,
        executor: Arc<dyn nexus_orchestration::capability::PromptExecutor>,
    ) {
        *self
            .prompt_executor
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(executor);
    }

    /// Set the shared per-run cancellation tokens (A1, v1.186 P1 T2).
    ///
    /// Called from daemon boot so the lazy-attach bundle shares the SAME
    /// map the boot engine and prompt executor use.
    pub fn set_session_cancels(
        &mut self,
        cancels: Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        >,
    ) {
        self.session_cancels = cancels;
    }

    /// Get the single public run coordinator, if wired.
    ///
    /// Returns `None` on Tier-0 boot (no creator DB) or before Profile
    /// attach publishes the matching bundle.
    #[must_use]
    pub fn run_coordinator(&self) -> Option<Arc<crate::preset_run::WorkflowRunCoordinator>> {
        if let Some(bundle) = self.runtime_bundle() {
            return Some(bundle.coordinator.clone());
        }
        self.run_coordinator
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Get the immutable aggregate runtime bundle (N-2), if published.
    ///
    /// `None` on Tier-0 boot (no creator DB) or before Profile attach
    /// publishes the matching bundle. All components in the bundle were
    /// wired together before the single readiness cell was published — a
    /// reader can never observe a mixed/partial bundle.
    #[must_use]
    pub fn runtime_bundle(&self) -> Option<Arc<RuntimeBundle>> {
        self.runtime_bundle
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Set the agent host facade.
    /// Called from boot.rs after constructing the agent host subsystem.
    pub fn set_agent_host(&mut self, host: Arc<dyn nexus_agent_host::HostFacade>) {
        self.agent_host = Arc::new(Some(host));
    }

    /// Reset the published runtime aggregate (daemon-level restart seam, A7).
    ///
    /// A daemon restart republishes a FRESH engine/coordinator/supervisor
    /// bundle over the SAME Creator DB and HOME. The prior bundle's drives
    /// must be quiescent first (callers abort them via
    /// [`WorkflowRunCoordinator::abort_all_drives`]); this cell is then
    /// cleared so the gated publishers
    /// ([`Self::publish_boot_runtime_bundle`] /
    /// [`Self::publish_creator_runtime_bundle`]) construct a new engine —
    /// whose `recover_sessions` reattaches existing session/child IDs and
    /// checkpoints from the frozen descriptors (never re-running recovery
    /// against a half-wired aggregate).
    pub fn reset_runtime_bundle(&self) {
        *self
            .runtime_bundle
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
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
        &self,
        dispatch: Arc<dyn nexus_orchestration::capability::DaemonToolDispatch>,
    ) {
        *self
            .daemon_tool_dispatch
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(dispatch);
    }

    /// Get the daemon-side tool dispatch adapter, if set (DF-47, V1.42 P3).
    #[must_use]
    pub fn daemon_tool_dispatch(
        &self,
    ) -> Option<Arc<dyn nexus_orchestration::capability::DaemonToolDispatch>> {
        self.daemon_tool_dispatch
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Get the production prompt executor (A1), if wired.
    ///
    /// `None` on Tier-0 boot (no creator DB) — LLM-backed capabilities
    /// return `WorkerUnavailable` then.
    #[must_use]
    pub fn prompt_executor(
        &self,
    ) -> Option<Arc<dyn nexus_orchestration::capability::PromptExecutor>> {
        if let Some(bundle) = self.runtime_bundle() {
            return bundle.prompt_executor.clone();
        }
        self.prompt_executor
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Get the shared per-run cancellation tokens (A1).
    #[must_use]
    pub fn session_cancels(
        &self,
    ) -> Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    > {
        self.session_cancels.clone()
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
        if let Some(bundle) = self.runtime_bundle() {
            return Some(bundle.engine.clone());
        }
        self.engine
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Get the schedule supervisor, if set (WS7).
    #[must_use]
    pub fn schedule_supervisor(&self) -> Option<Arc<ScheduleSupervisor>> {
        if let Some(bundle) = self.runtime_bundle() {
            return Some(bundle.supervisor.clone());
        }
        self.schedule_supervisor
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Get the current capability registry, if set (V1.176 P1, AR-92).
    ///
    /// Reads through the shared holder: each call clones the current
    /// registry under the read lock and releases immediately, so callers see
    /// hot reloads without holding the lock (AR-92 #7).
    #[must_use]
    pub fn capability_registry(&self) -> Option<Arc<CapabilityRegistry>> {
        if let Some(bundle) = self.runtime_bundle() {
            return bundle.capability_holder.get();
        }
        self.capability_registry
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .and_then(|holder| holder.get())
    }

    /// Get the shared capability registry holder itself, if set (V1.176 P1,
    /// AR-92).
    ///
    /// The peer-tools lane (AR-68 #2(ii)) derives its reserved-name set
    /// LIVE from this holder at each admission, so a user capability
    /// hot-added after the lane spawned stays reserved against peer
    /// admission (V1.176 P1 QC fix, W-A).
    #[must_use]
    pub fn capability_registry_holder(&self) -> Option<CapabilityRegistryHolder> {
        if let Some(bundle) = self.runtime_bundle() {
            return Some(bundle.capability_holder.clone());
        }
        self.capability_registry
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    /// Get the shutdown notification handle.
    ///
    /// Callers await `.notified()` to block until the daemon enters Stopping
    /// state. This Notify is MULTI-CONSUMER: boot's engine drainer and HTTP
    /// accept loop, the cron supervisor, refresh scheduler, auto-chronology
    /// tick, connect relay/watcher, and — since the v1.179 P0 fix wave —
    /// every live embedded MCP session all wait on it. A single-permit
    /// `notify_one` would let one arbitrary waiter steal the permit and
    /// hang the rest, so [`WorkspaceState::request_shutdown`] broadcasts
    /// with `notify_waiters`.
    ///
    /// `notify_waiters` wakes only waiters registered at call time and
    /// stores no permit: a consumer that registers AFTER the broadcast
    /// must consult [`WorkspaceState::shutdown_requested`] instead of
    /// awaiting blindly (embedded MCP `establish()` refuses on that gate).
    #[must_use]
    pub fn shutdown_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.shutdown_notify)
    }

    /// Whether [`WorkspaceState::request_shutdown`] has fired on this state
    /// (or any `WorkspaceState` clone sharing the same gate).
    ///
    /// Late-arriving shutdown consumers — anything that would register a
    /// `shutdown_notify` waiter after the broadcast — poll this gate
    /// instead of awaiting a notification that can never reach them.
    #[must_use]
    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::SeqCst)
    }
    /// Get the boot-scoped embedded MCP server (DF-88 Model B), if one was
    /// started at boot. `None` when the `embedded-mcp` feature is compiled
    /// off, or when GC #9 enablement (the `PeerToolsConfig.embedded_mcp`
    /// key OR the `--embedded-mcp` CLI flag) was not requested.
    #[cfg(feature = "embedded-mcp")]
    #[must_use]
    pub fn embedded_mcp_server(
        &self,
    ) -> Option<Arc<crate::connect::mcp_embedded::EmbeddedMcpServer>> {
        self.embedded_mcp_server.as_ref().clone()
    }

    /// Store the boot-scoped embedded MCP server (called from `boot.rs`
    /// §8.5; the ONE instance in-daemon consumers `establish()` on).
    #[cfg(feature = "embedded-mcp")]
    pub fn set_embedded_mcp_server(
        &mut self,
        server: Arc<crate::connect::mcp_embedded::EmbeddedMcpServer>,
    ) {
        self.embedded_mcp_server = Arc::new(Some(server));
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
    pub fn workspace_path_handle(&self) -> Arc<std::sync::Mutex<Option<String>>> {
        Arc::clone(&self.workspace_path)
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

    /// Request graceful shutdown — raise the shutdown gate, then broadcast
    /// the shutdown notification to ALL current waiters.
    ///
    /// Called from lifecycle `Stopping` entry or signal handlers.
    ///
    /// # Why `notify_waiters` (Bugbot PR #234)
    ///
    /// Many independent subsystems wait on the ONE `shutdown_notify` —
    /// boot.rs: engine drainer, HTTP accept loop, user-capability watcher,
    /// stale-findings watcher, cron supervisor, auto-chronology tick,
    /// refresh scheduler, connect relay/watcher — and, since the v1.179 P0
    /// fix wave, every live embedded MCP session (`mcp_embedded.rs`
    /// selects on it per session). `notify_one` stores a SINGLE permit, so
    /// with N > 1 waiters one arbitrary waiter steals it and every other
    /// waiter hangs forever — the daemon shutdown deadlocks.
    /// `notify_waiters` wakes every currently-registered waiter and stores
    /// no permit (same multi-consumer pattern as the peer accept lane's
    /// `closed` latch, `connect/accept.rs`).
    ///
    /// # Pre-registration window (pre-existing, unchanged)
    ///
    /// A waiter that has not yet registered `.notified()` when this fires
    /// is not woken by the broadcast — the same window every subsystem on
    /// this Notify already has; signal handlers run post-boot, after the
    /// accept loop and subsystems have registered. To keep NEW arrivals
    /// honest, the gate is raised BEFORE the broadcast and embedded MCP
    /// `establish()` refuses with `embedded_mcp_shutdown` once it is up:
    /// a session established after `request_shutdown` can never receive
    /// the broadcast, so it must never be created.
    pub fn request_shutdown(&self) {
        // Gate FIRST: a consumer loading the flag around the broadcast
        // observes shutdown one way or the other (refusal or wake-up).
        self.shutdown_requested.store(true, Ordering::SeqCst);
        self.actor_sessions.close();
        if let Some(host) = self.agent_host() {
            let registry = self.actor_sessions.clone();
            tokio::spawn(async move {
                if let Err(err) = registry.drain_host_sessions(host.as_ref()).await {
                    tracing::warn!("actor host drain on shutdown failed: {err}");
                }
            });
        }
        self.shutdown_notify.notify_waiters();
    }

    /// Process-lifetime Actor session registry.
    #[must_use]
    pub const fn actor_sessions(&self) -> &ActorSessionRegistry {
        &self.actor_sessions
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

    /// P0 QC fix wave (qc1 S-2): boot's user-module warm-up derives the RAW
    /// user home from `state.nexus_home().parent()` (deviation #1, boot.rs)
    /// to feed `nexus-home-layout::user_modules_dir` (which joins `.nexus42`
    /// internally). Pin the invariant that `nexus_home` is ALWAYS
    /// `<raw user home>/.nexus42`, so the `.parent()` derivation stays
    /// correct and matches the `nexus42 compute install` store path.
    #[tokio::test]
    #[serial]
    async fn nexus_home_is_raw_user_home_joined_nexus42() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let user_home = tmp.path();
        let nexus_home = user_home.join(".nexus42");
        nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", user_home);

        let state = WorkspaceState::initialize().await.expect("initialize");
        assert_eq!(
            state.nexus_home().parent(),
            Some(user_home),
            "nexus_home must be `<raw user home>/.nexus42` — boot.rs derives \
             the raw home via .parent() for nexus-home-layout helpers"
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
        assert!(
            state.narrative_gateway().is_some(),
            "narrative_gateway must be ready when pool is visible"
        );
        assert!(
            state.session_manager().is_some(),
            "session_manager must be ready when pool is visible"
        );

        match original_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }

    /// F1 regression: published pool must not outpace slot gateway/session wiring.
    #[tokio::test]
    #[serial]
    async fn ensure_creator_pool_pool_visible_implies_slot_ready() {
        const CREATOR_ID: &str = "crt_ready_gate_test";

        let tmp = tempfile::TempDir::new().expect("temp dir");
        let user_home = tmp.path();
        let nexus_home = user_home.join(".nexus42");
        nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

        let cache = serde_json::json!({
            "creators": {
                CREATOR_ID: { "handle": "ready-gate" }
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
        let config_toml = format!("active_creator_id = \"{CREATOR_ID}\"\n");
        std::fs::write(nexus_home.join("config.toml"), config_toml).expect("config.toml");

        state
            .ensure_creator_pool()
            .await
            .expect("ensure_creator_pool");

        // Invariant: pool handle is only published after slot is fully wired.
        if state.pool().is_some() {
            assert!(
                state.narrative_gateway().is_some(),
                "pool visible but narrative_gateway still None"
            );
            assert!(
                state.session_manager().is_some(),
                "pool visible but session_manager still None"
            );
        }

        match original_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }

    /// T3 fix-1 (P1-2): lazy Profile attach publishes the runtime bundle
    /// through the SAME terminal reconciliation as the normal boot path.
    ///
    /// A Tier-0 daemon starts with no active creator; an EXISTING Creator DB
    /// with a checkpoint-before-settlement row (`paused` driven schedule
    /// whose durable v1 session is terminal) is opened on Profile attach.
    /// `ensure_creator_pool` must settle that row from the durable session
    /// status BEFORE reporting the attached runtime ready — the pre-existing
    /// unsettled row must never stay visible through the published bundle.
    #[tokio::test]
    #[serial]
    async fn lazy_attach_reconciles_pre_existing_unsettled_row() {
        const CREATOR_ID: &str = "crt_lazy_reconcile_test";

        let tmp = tempfile::TempDir::new().expect("temp dir");
        let user_home = tmp.path();
        let nexus_home = user_home.join(".nexus42");
        nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", user_home);

        let state = WorkspaceState::initialize().await.expect("initialize");
        assert!(state.pool().is_none(), "Tier-0 boot has no creator DB");

        // Profile attach: write the config the middleware would, then open
        // and seed the EXISTING Creator DB exactly where the daemon will
        // open it (ADR-014 state.db under the operational workspace).
        let config_toml = format!(
            "active_creator_id = \"{CREATOR_ID}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR_ID}\" = \"default\"\n"
        );
        std::fs::write(nexus_home.join("config.toml"), config_toml).expect("config.toml");
        let db_path = nexus_home_layout::workspace_state_db_path(user_home, CREATOR_ID, "default");
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).expect("db parent");
        }
        let pool = nexus_local_db::init_pool(&db_path)
            .await
            .expect("init existing creator DB");

        // Seed the checkpoint-before-settlement row: a `driven_v1` schedule
        // PAUSED by an earlier recovery boot whose durable v1 session is
        // terminal `completed` (the settlement crashed before the flip).
        let now = chrono::Utc::now().timestamp();
        let descriptor = serde_json::to_vec(&serde_json::json!({
            "creator_id": CREATOR_ID,
            "work_id": null,
            "workspace_root": "",
            "preset_id": "memory-augmented",
            "preset_version": 1,
            "source": { "Embedded": { "preset_id": "memory-augmented", "content_hash": vec![0u8; 32] } },
            "input": {},
            "agent_bindings": {},
            "parent_session_id": null,
            "graph_name": null
        }))
        .expect("descriptor json");
        sqlx::query(
            "INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version, status,
                 current_task_id, context_json, created_at, updated_at,
                 execution_version, state_revision, run_state_json, run_descriptor_json)
             VALUES ('SES-LAZY-RECON', ?, 'memory-augmented', 1, 'completed',
                     NULL, X'7B7D', ?, ?, 1, 5, ?, ?)",
        )
        .bind(CREATOR_ID)
        .bind(now - 10)
        .bind(now)
        .bind(
            serde_json::to_vec(&serde_json::json!({
                "wait": null,
                "step_in_flight": null,
                "in_flight": null,
                "failure": null,
                "cancel_requested": false
            }))
            .expect("run state"),
        )
        .bind(&descriptor)
        .execute(&pool)
        .await
        .expect("seed durable terminal session");
        sqlx::query(
            "INSERT INTO creator_schedules
                (schedule_id, creator_id, preset_id, preset_version, status,
                 concurrency_kind, current_core_context_version, current_session_id,
                 created_at, updated_at, execution_policy)
             VALUES ('SCH-LAZY-RECON', ?, 'memory-augmented', 1, 'paused',
                     'serial', 0, 'SES-LAZY-RECON', ?, ?, 'driven_v1')",
        )
        .bind(CREATOR_ID)
        .bind(now - 5)
        .bind(now)
        .execute(&pool)
        .await
        .expect("seed unsettled driven schedule");

        // The lazy-attach publish runs the terminal reconciliation. The
        // pre-existing paused row settles from the durable session status
        // before the bundle is reported ready.
        state
            .ensure_creator_pool()
            .await
            .expect("ensure_creator_pool after attach");
        assert!(
            state.runtime_bundle().is_some(),
            "lazy attach must publish the runtime bundle"
        );
        let row: (String, String) = sqlx::query_as(
            "SELECT status, current_session_id FROM creator_schedules \
             WHERE schedule_id = 'SCH-LAZY-RECON'",
        )
        .fetch_one(state.pool().expect("pool published after attach"))
        .await
        .expect("schedule row after attach");
        assert_eq!(
            row,
            ("completed".to_string(), "SES-LAZY-RECON".to_string()),
            "lazy attach must settle the pre-existing unsettled row from the \
             durable terminal session before reporting the runtime ready"
        );

        match original_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
    }

    /// Bugbot PR #234: `request_shutdown` must BROADCAST on the shared
    /// `shutdown_notify`. Boot's engine drainer + HTTP accept loop, the
    /// cron supervisor, refresh scheduler, auto-chronology tick, connect
    /// relay/watcher, and every live embedded MCP session all wait on the
    /// ONE Notify; a single-permit `notify_one` lets one arbitrary waiter
    /// steal the permit and hangs the rest at shutdown. Four concurrent
    /// waiters must ALL wake, each within the per-waiter timeout — under
    /// `notify_one` this test times out.
    #[tokio::test]
    async fn request_shutdown_wakes_all_concurrent_waiters() {
        const WAITERS: usize = 4;

        let state = WorkspaceState::new_for_testing(
            std::env::temp_dir().join("shutdown-waiters-test-home"),
            std::env::temp_dir().join("shutdown-waiters-test.db"),
            None,
        )
        .await;

        assert!(
            !state.shutdown_requested(),
            "gate must start lower before request_shutdown"
        );
        let notify = state.shutdown_notify();
        let waiters: Vec<_> = (0..WAITERS)
            .map(|i| {
                let notify = notify.clone();
                tokio::spawn(async move {
                    notify.notified().await;
                    i
                })
            })
            .collect();
        // Let every waiter register before the broadcast: `notify_waiters`
        // wakes only waiters registered at fire time.
        tokio::task::yield_now().await;

        state.request_shutdown();

        let deadline = std::time::Duration::from_secs(5);
        let mut woken = std::collections::HashSet::new();
        for waiter in waiters {
            let i = tokio::time::timeout(deadline, waiter)
                .await
                .expect(
                    "waiter must wake within 5s — request_shutdown must broadcast \
                     (notify_waiters), not notify_one",
                )
                .expect("waiter task joins");
            assert!(woken.insert(i), "waiter {i} woke more than once");
        }
        assert_eq!(
            woken.len(),
            WAITERS,
            "EVERY concurrent waiter must observe the shutdown broadcast"
        );
        assert!(
            state.shutdown_requested(),
            "request_shutdown must raise the shutdown_requested gate"
        );
    }
}
