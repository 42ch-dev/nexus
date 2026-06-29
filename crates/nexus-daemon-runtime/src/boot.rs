//! Daemon boot sequence — extracted from the former standalone daemon binary.
//!
//! Provides `run_daemon()` as the single callable entry point for both
//! the `nexus42 daemon-run` hidden command and the
//! `nexus42 daemon start --foreground` path.

use std::path::PathBuf;
use std::sync::Arc;

use crate::api;
use crate::lifecycle::{Event, Lifecycle, StatigLifecycle, SubsystemKind};
use crate::worker_provider::ProductionWorkerProvider;
use crate::workspace::WorkspaceState;
use nexus_orchestration::worker::{WorkerManagerSpawner, WorkerRegistry};
use nexus_orchestration::{
    engine::{EngineSignal, OrchestrationEngine},
    schedule::supervisor::ScheduleSupervisor,
    storage::sqlite::SqliteSessionStorage,
    system_preset_dir, CapabilityRegistry, CapabilityRuntimeDeps, GraphFlowEngine, WorkerManager,
};
use tracing_subscriber::EnvFilter;

/// Local API transport configuration.
#[derive(Debug, Clone)]
pub enum Transport {
    /// HTTP over TCP loopback (default)
    Http { port: u16, host: String },
    /// Unix domain socket
    UnixSocket { path: PathBuf },
}

/// Configuration for the daemon runtime, derived from CLI arguments.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Port for HTTP transport (default: 8420).
    pub port: u16,
    /// Bind address for HTTP transport (default: 127.0.0.1).
    pub host: String,
    /// Unix domain socket path (overrides HTTP if set).
    pub socket_path: Option<PathBuf>,
    /// Enable verbose (debug-level) logging.
    pub verbose: bool,
    /// Shutdown grace period in milliseconds.
    pub shutdown_grace_ms: u64,
    /// Optional CDN URL for `registry.refresh` network mode.
    /// When set, enables fetching the ACP registry from a CDN
    /// with built-in timeout and retry. When absent (default),
    /// `registry.refresh` returns synthetic output only.
    pub cdn_url: Option<String>,
}

impl DaemonConfig {
    /// Resolve transport from config and environment variables.
    ///
    /// Priority:
    /// 1. `socket_path` field
    /// 2. `NEXUS_DAEMON_SOCKET_PATH` env var
    /// 3. `port` / `host` fields (HTTP)
    /// 4. `NEXUS_DAEMON_PORT` env var (HTTP fallback)
    /// 5. Default: HTTP on 127.0.0.1:8420
    #[must_use]
    pub fn resolve_transport(&self) -> Transport {
        // Unix socket takes priority
        if let Some(ref path) = self.socket_path {
            return Transport::UnixSocket { path: path.clone() };
        }

        if let Ok(path) = std::env::var("NEXUS_DAEMON_SOCKET_PATH") {
            return Transport::UnixSocket {
                path: PathBuf::from(path),
            };
        }

        // HTTP fallback
        let port = if self.port == 8420 {
            std::env::var("NEXUS_DAEMON_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(self.port)
        } else {
            self.port
        };

        Transport::Http {
            port,
            host: self.host.clone(),
        }
    }
}

/// Run the daemon runtime to completion.
///
/// This is the main daemon entry point, extracted from the former standalone daemon
/// binary. It handles the full lifecycle: initialization → serve → shutdown.
///
/// # Errors
///
/// Returns an error if workspace initialization, database pool creation,
/// engine wiring, or the HTTP/Unix socket server fails to start.
///
/// # Panics
///
/// Panics if Unix signal handlers (SIGTERM, SIGINT) cannot be registered,
/// which should only happen in severely broken environments.
// 250+ lines is inherent to the orchestrated initialization sequence.
#[allow(clippy::too_many_lines)]
pub async fn run_daemon(config: DaemonConfig) -> anyhow::Result<()> {
    // --- Section 1: Logging ---
    // Initialize tracing subscriber with configurable verbosity. Use try_init
    // so the daemon can be invoked both as a standalone process and from a
    // parent CLI that has already installed a subscriber (e.g. foreground
    // `nexus42 daemon start` or the `daemon-run` subprocess).
    let filter = if config.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();

    tracing::info!("Starting daemon-runtime v{}", env!("CARGO_PKG_VERSION"));

    // --- Section 1.5: Resolve CDN config for registry.refresh ---
    // V1.57 P1: CdnConfig is constructor-injected (no global state).
    // Validated and resolved here; passed through CapabilityRuntimeDeps below.
    let cdn_config = if let Some(ref cdn_url) = config.cdn_url {
        if let Err(e) = nexus_orchestration::capability::builtins::validate_cdn_url_static(cdn_url)
        {
            anyhow::bail!("--cdn-url is invalid: {e}");
        }
        tracing::info!(%cdn_url, "CDN URL configured for registry.refresh network mode");
        Some(nexus_orchestration::capability::builtins::CdnConfig {
            url: cdn_url.clone(),
            timeout_ms: 10_000,
            max_retries: 3,
            max_body_bytes:
                nexus_orchestration::capability::builtins::registry::DEFAULT_MAX_CDN_BODY_SIZE,
        })
    } else {
        tracing::info!("No CDN URL configured — registry.refresh will use synthetic output only");
        None
    };

    // --- Section 2: Workspace initialization ---
    // Initialize workspace state (database only; no cloud-sync outbox on daemon).
    let mut state = WorkspaceState::initialize().await?;
    tracing::info!("Workspace state initialized");

    // --- Section 3: Orchestration engine + worker manager ---
    let db_pool: sqlx::SqlitePool = state.pool().clone();
    let sqlite_storage = Arc::new(SqliteSessionStorage::new(Arc::new(db_pool)));

    // V1.51 T-A P0 (QC3 F-001): construct the shared worker registry BEFORE
    // the capability registry so `ProductionWorkerProvider` can be injected
    // into `CapabilityRegistry::with_runtime_deps`. The same registry is
    // later shared with `WorkerMgrSubsystem` (see `create_subsystems`), so a
    // worker spawned by the engine/preset-session path is visible to the
    // capability-layer LLM dispatch path and vice versa.
    //
    // `pool: None` preserves the existing pool-less behavior of
    // `with_builtins()` for `kb.extract_work` / `novel.project_scaffold` /
    // `novel.chapter_transition` (they remain in placeholder mode — same as
    // before this change). Only the worker_provider wiring changes, closing
    // the V1.51 T-A P0 production gap where `nexus.llm.extract` (and the
    // sibling LLM caps) returned `WorkerUnavailable` on every production call.
    let shared_worker_registry: Arc<tokio::sync::Mutex<WorkerRegistry<WorkerManagerSpawner>>> = {
        let manager = Arc::new(tokio::sync::Mutex::new(WorkerManager::new()));
        let spawner = WorkerManagerSpawner::new(manager);
        Arc::new(tokio::sync::Mutex::new(WorkerRegistry::new(
            crate::lifecycle::subsystems::DEFAULT_MAX_WORKERS,
            spawner,
        )))
    };
    let worker_provider = ProductionWorkerProvider::new(shared_worker_registry.clone());

    // V1.61 P-last T1/T2/T3: build ONE daemon-wide `WasmEngine` + `ModuleCache`
    // and inject them into `narrative.compute` so module compilation happens
    // exactly once process-wide (closes R-V161P3-PERF-001/002). The cache is
    // pre-warmed with embedded modules (T2) and user-installed modules under
    // `~/.nexus42/modules/` (T3) before any capability runs.
    //
    // `WasmEngine::new()` failing (e.g. JIT unavailable on an exotic host) is
    // non-fatal: the daemon still boots and the registry falls back to the
    // non-wasm wiring, in which case `narrative.compute` returns
    // `WorkerUnavailable` if a caller actually invokes it.
    let wasm_singleton: Option<(
        Arc<nexus_wasm_host::WasmEngine>,
        Arc<nexus_wasm_host::ModuleCache>,
    )> = match nexus_wasm_host::WasmEngine::new() {
        Ok(engine) => {
            let engine = Arc::new(engine);
            let cache = Arc::new(nexus_wasm_host::ModuleCache::new());
            // T2: embedded modules (compiled by build.rs into embedded-modules/).
            match cache.warm_embedded(&engine) {
                Ok(n) => tracing::info!("warmed {n} embedded WASM module(s) into the daemon cache"),
                Err(e) => tracing::warn!(error = %e, "embedded WASM module warmup had errors"),
            }
            // T3: user-installed modules under ~/.nexus42/modules/.
            let user_modules_dir = nexus_home_layout::user_modules_dir(state.nexus_home());
            match cache.warm_dir(&engine, &user_modules_dir) {
                Ok(n) => {
                    if n > 0 {
                        tracing::info!(
                            dir = %user_modules_dir.display(),
                            "warmed {n} user WASM module(s) into the daemon cache"
                        );
                    }
                }
                Err(e) => tracing::warn!(
                    dir = %user_modules_dir.display(),
                    error = %e,
                    "user WASM module warmup had errors"
                ),
            }
            tracing::info!(
                "WASM module cache ready: {} module(s) loaded (engine singleton)",
                cache.len()
            );
            Some((engine, cache))
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "WasmEngine init failed; narrative.compute will be unavailable"
            );
            None
        }
    };

    let runtime_deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: Some(std::sync::Arc::new(worker_provider)),
        daemon_tool_dispatch: None,
        cdn_config,
    };
    let capabilities = Arc::new(match wasm_singleton {
        Some((engine, cache)) => {
            CapabilityRegistry::with_runtime_deps_and_wasm(&runtime_deps, engine, cache)
        }
        None => CapabilityRegistry::with_runtime_deps(&runtime_deps),
    });
    tracing::info!(
        "Capability registry built (production wiring + WASM singleton where available)"
    );

    // V1.42 P3 (DF-47): wire daemon-side tool dispatch adapter.
    // Stored in WorkspaceState so schedule-executed HostToolCallTask instances
    // can invoke nexus.* tools directly through HostToolExecutor::dispatch_for_schedule.
    let tool_dispatch = Arc::new(
        crate::api::handlers::host_tool_executor::DaemonToolDispatchAdapter::new(
            WorkspaceState::clone(&state),
        ),
    );
    state.set_daemon_tool_dispatch(tool_dispatch.clone());
    tracing::info!("Daemon tool dispatch adapter wired");

    let mut concrete_engine =
        GraphFlowEngine::new_with_storage(sqlite_storage.clone(), capabilities.clone());

    // DF-47 (V1.42 P3): wire the daemon tool dispatch into the engine so
    // HostTool enter actions in preset graphs can invoke nexus.* tools.
    concrete_engine.set_daemon_tool_dispatch(tool_dispatch.clone());

    // WS2 R1: Recover persisted non-terminal sessions into in-memory tracker.
    match sqlite_storage.list_non_terminal_sessions().await {
        Ok(summaries) => {
            if !summaries.is_empty() {
                tracing::info!(
                    "recovering {} persisted session(s) into in-memory tracker",
                    summaries.len()
                );
                concrete_engine.recover_sessions(summaries).await;
            }
        }
        Err(e) => {
            tracing::warn!("failed to recover persisted sessions: {}", e);
        }
    }

    // --- WS-D: Discover and start system presets from directory ---
    let system_presets_dir = state.nexus_home().clone();
    match system_preset_dir::ensure_maintenance_preset(&system_presets_dir) {
        Ok(true) => tracing::info!("auto-created _system.maintenance preset directory"),
        Ok(false) => {}
        Err(e) => tracing::warn!("failed to auto-create _system.maintenance: {}", e),
    }

    let engine_ref: Arc<dyn OrchestrationEngine> = Arc::new(concrete_engine.clone());
    let scan_result = system_preset_dir::scan_system_presets(&system_presets_dir, &capabilities);
    for entry in &scan_result.presets {
        let graph = nexus_orchestration::preset::loader::build_wired_outer_graph(
            &entry.loaded,
            &engine_ref.clone(),
            &capabilities.clone(),
            Some(tool_dispatch.clone()),
        );
        let graph = Arc::new(graph);

        match concrete_engine
            .start_session(&entry.qualified_id, graph)
            .await
        {
            Ok(sid) => {
                tracing::info!(
                    session_id = sid.0,
                    preset_id = %entry.qualified_id,
                    "started system preset session"
                );
            }
            Err(e) => {
                tracing::warn!(
                    preset_id = %entry.qualified_id,
                    error = %e,
                    "failed to start system preset session"
                );
            }
        }
    }

    let engine: Arc<dyn OrchestrationEngine> = Arc::new(concrete_engine);
    let workers = Arc::new(WorkerManager::new());

    state.set_engine(engine);
    state.set_worker_manager(workers);
    state.set_capability_registry(capabilities);
    tracing::info!("Orchestration engine wired");

    // --- Section 4: Schedule supervisor + core context manager ---
    // Replace .expect() with graceful error handling for database pool creation.
    let schedule_pool: sqlx::SqlitePool =
        match nexus_local_db::open_pool(std::path::Path::new(&state.database_path())).await {
            Ok(pool) => pool,
            Err(e) => {
                tracing::error!("Fatal: failed to open database pool for schedule supervisor: {e}");
                anyhow::bail!("Failed to open database pool for schedule supervisor: {e}");
            }
        };
    // V1.51 T-A P0: thread the capability registry into the supervisor so the
    // review-time KB extraction hook can invoke `nexus.llm.extract`. Falls back
    // to the heuristic when the worker is unavailable (llm-extract.md §5.1).
    let supervisor_registry = state.capability_registry();
    let mut schedule_supervisor_builder = ScheduleSupervisor::new_with_workspace(
        Arc::new(schedule_pool),
        state.workspace_path().map(std::path::PathBuf::from),
    );
    if let Some(reg) = supervisor_registry {
        schedule_supervisor_builder = schedule_supervisor_builder.with_capability_registry(reg);
    }
    let schedule_supervisor = Arc::new(schedule_supervisor_builder);
    state.set_schedule_supervisor(schedule_supervisor.clone());

    match schedule_supervisor
        .resume_running_as_paused("daemon_restart")
        .await
    {
        Ok(count) => {
            if count > 0 {
                tracing::info!(
                    "resumed {} running schedule(s) as paused after daemon restart",
                    count
                );
            }
        }
        Err(e) => {
            tracing::warn!("failed to resume running schedules on boot: {}", e);
        }
    }

    // V1.39 §5.5 (T5 + Fix 2): Auto-chain boot recovery.
    // Find Works whose auto-chain driver schedule is no longer running
    // (interrupted by daemon restart) and auto-resume them by evaluating
    // the next step and enqueuing a new schedule.
    {
        let recovery_pool = state.pool();
        match nexus_orchestration::auto_chain::find_resumable_works(recovery_pool).await {
            Ok(resumable) => {
                if !resumable.is_empty() {
                    tracing::info!(
                        "found {} auto-chain work(s) with interrupted driver schedule(s), resuming",
                        resumable.len()
                    );
                    for work in &resumable {
                        // Reload from DB (SSOT)
                        let fresh = nexus_local_db::works::get_work(
                            recovery_pool,
                            &work.creator_id,
                            &work.work_id,
                        )
                        .await;

                        if let Ok(Some(latest)) = fresh {
                            // V1.42 F-001: When the persist stage just completed,
                            // use the volume-aware evaluator to correctly handle
                            // cross-volume auto-chain (Plan Goal 4 / AC2). For all
                            // other stages, the flat evaluator suffices.
                            let action = if latest.current_stage == "persist"
                                && latest.stage_status == "complete"
                            {
                                match nexus_orchestration::auto_chain::evaluate_after_persist_volume_aware(recovery_pool, &latest).await {
                                        Ok(vol_action) => vol_action,
                                        Err(e) => {
                                            tracing::warn!(
                                                work_id = %latest.work_id,
                                                error = %e,
                                                "auto-chain boot resume: volume-aware eval failed, falling back to flat"
                                            );
                                            nexus_orchestration::auto_chain::evaluate_next_step(&latest)
                                        }
                                    }
                            } else {
                                nexus_orchestration::auto_chain::evaluate_next_step(&latest)
                            };

                            match action {
                                nexus_orchestration::auto_chain::ChainAction::AdvanceStage {
                                    ref work_id,
                                    ref next_stage,
                                } => {
                                    match resume_auto_chain_work(
                                        recovery_pool,
                                        &latest.creator_id,
                                        work_id,
                                        next_stage,
                                        None,
                                        &latest,
                                    )
                                    .await
                                    {
                                        Ok(sid) => tracing::info!(
                                            work_id = %work_id,
                                            stage = %next_stage,
                                            schedule_id = %sid,
                                            "auto-chain boot resume: enqueued next stage"
                                        ),
                                        Err(e) => tracing::warn!(
                                            work_id = %work_id,
                                            error = %e,
                                            "auto-chain boot resume: failed to enqueue next stage"
                                        ),
                                    }
                                }
                                nexus_orchestration::auto_chain::ChainAction::NextChapter {
                                    ref work_id,
                                    ref next_chapter,
                                    next_volume,
                                } => {
                                    match resume_auto_chain_work(
                                        recovery_pool,
                                        &latest.creator_id,
                                        work_id,
                                        "produce",
                                        Some(*next_chapter),
                                        &latest,
                                    )
                                    .await
                                    {
                                        Ok(sid) => tracing::info!(
                                            work_id = %work_id,
                                            chapter = *next_chapter,
                                            volume = next_volume,
                                            schedule_id = %sid,
                                            "auto-chain boot resume: enqueued next chapter (volume-aware)"
                                        ),
                                        Err(e) => tracing::warn!(
                                            work_id = %work_id,
                                            error = %e,
                                            "auto-chain boot resume: failed to enqueue next chapter"
                                        ),
                                    }
                                }
                                nexus_orchestration::auto_chain::ChainAction::WorkComplete {
                                    ref work_id,
                                } => {
                                    match nexus_orchestration::auto_chain::mark_work_completed(
                                        recovery_pool,
                                        &latest.creator_id,
                                        work_id,
                                    )
                                    .await
                                    {
                                        Ok(_) => {
                                            // Write completion-lock file (best-effort; DB is SSOT)
                                            if let Some(ws_path) = state.workspace_path() {
                                                if let Ok(Some(refreshed)) =
                                                    nexus_local_db::works::get_work(
                                                        recovery_pool,
                                                        &latest.creator_id,
                                                        work_id,
                                                    )
                                                    .await
                                                {
                                                    if let Some(ref locked_at) =
                                                        refreshed.completion_locked_at
                                                    {
                                                        if let Err(e) =
                                                            nexus_orchestration::auto_chain::write_completion_lock_for_work(
                                                                std::path::Path::new(&ws_path),
                                                                &refreshed,
                                                                locked_at,
                                                            )
                                                        {
                                                            tracing::warn!(
                                                                work_id = %work_id,
                                                                error = %e,
                                                                "auto-chain boot resume: \
                                                                 completion-lock file write failed \
                                                                 (non-fatal; DB is SSOT)"
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                            tracing::info!(
                                                work_id = %work_id,
                                                "auto-chain boot resume: work completed"
                                            );
                                        }
                                        Err(e) => tracing::warn!(
                                            work_id = %work_id,
                                            error = %e,
                                            "auto-chain boot resume: failed to mark work completed"
                                        ),
                                    }
                                }
                                nexus_orchestration::auto_chain::ChainAction::NoAction => {
                                    tracing::info!(
                                        work_id = %latest.work_id,
                                        current_stage = %latest.current_stage,
                                        stage_status = %latest.stage_status,
                                        "auto-chain boot resume: no action needed"
                                    );
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("failed to query auto-chain resumable works: {}", e);
            }
        }
    }
    tracing::info!("Schedule supervisor wired");

    // --- Section 4b: Master-decision timeout watcher (V1.39 P4 T1) ---
    //
    // Periodically scans `findings` for open rows older than 96h (the
    // master-decision SLA) and logs a structured `warn!` per stale row.
    // Non-blocking: errors are logged and the loop continues. The task
    // exits cleanly when `shutdown_notify` fires. See
    // `crates/nexus-daemon-runtime/src/stale_findings_watcher.rs`.
    {
        let watcher_pool = state.pool().clone();
        let watcher_shutdown = state.shutdown_notify();
        let watcher_config = crate::stale_findings_watcher::StaleFindingsWatcherConfig::from_env();
        let _watcher_handle = crate::stale_findings_watcher::spawn_stale_findings_watcher(
            watcher_pool,
            watcher_shutdown,
            watcher_config,
        );
    }

    // --- Section 4c: Cron supervisor (V1.50 T-A P1) ---
    //
    // Periodically (default 1-min) evaluates per-Work cron configs
    // (`works.schedule_json`) for the novel-writing three-role staggering and
    // enqueues pending `Schedule`s for any matching role (brainstorm + write
    // in T-A P1), then admits due schedules. Non-blocking: errors are logged
    // and the loop continues. See
    // `crates/nexus-daemon-runtime/src/cron_supervisor.rs`.
    {
        let cron_pool = state.pool().clone();
        let cron_workspace = state.workspace_path().map(std::path::PathBuf::from);
        let cron_supervisor = schedule_supervisor.clone();
        let cron_shutdown = state.shutdown_notify();
        let cron_config = crate::cron_supervisor::CronSupervisorConfig::from_env();
        // V1.51 T-B P0: pass workspace_dir for file-lock path construction.
        // If workspace_path is unset, use an empty path (defensive — a daemon
        // without a workspace should not have schedule_json-bearing Works).
        let cron_ws_path = cron_workspace.unwrap_or_default();
        let _cron_handle = crate::cron_supervisor::spawn_cron_supervisor(
            cron_pool,
            cron_ws_path,
            cron_supervisor,
            cron_shutdown,
            cron_config,
        );
    }

    // --- Section 4d: Auto-chronology task (V1.50 T-A P3) ---
    //
    // Periodically (default 5-min) scans Works with `auto_chronology = true`
    // and, when the current volume is complete (all chapters finalized + intake
    // complete + not locked), auto-creates the next volume outline + seeds +
    // chronology log (spec §3 / §4). Non-blocking: errors are logged and the
    // loop continues. See `crates/nexus-daemon-runtime/src/auto_chronology.rs`.
    {
        let chron_pool = state.pool().clone();
        let chron_workspace = state.workspace_path().map(std::path::PathBuf::from);
        let chron_shutdown = state.shutdown_notify();
        let chron_config = crate::auto_chronology::AutoChronologyConfig::from_env();
        let _chron_handle = crate::auto_chronology::spawn_auto_chronology_tick(
            chron_pool,
            chron_workspace,
            chron_shutdown,
            chron_config,
        );
    }

    // --- Section 4e: Reference refresh scheduler (V1.58 P1 DF-44) ---
    //
    // Periodically scans `reference_sources` for stale rows (on_change or
    // overdue scheduled) and dispatches `nexus.reference.refresh` for each
    // candidate. Non-blocking: errors are logged and the loop continues.
    // First refresh cycle fires after 60s initial delay. See
    // `crates/nexus-daemon-runtime/src/refresh_scheduler.rs`.
    {
        let refresh_pool = state.pool().clone();
        let refresh_shutdown = state.shutdown_notify();
        let refresh_config = crate::refresh_scheduler::RefreshSchedulerConfig::from_env();
        let _refresh_handle = crate::refresh_scheduler::spawn_refresh_scheduler(
            refresh_pool,
            refresh_shutdown,
            refresh_config,
        );
    }

    // --- Section 5: Agent Host subsystem ---
    let agent_host_facade: Arc<dyn nexus_agent_host::HostFacade> = {
        let manager = nexus_agent_host::core::manager::HostManager::new();
        Arc::new(manager)
    };
    state.set_agent_host(Arc::clone(&agent_host_facade));
    tracing::info!("Agent host facade wired");

    // --- Section 6: Lifecycle HSM initialization ---
    let subsystems = create_subsystems(
        &state,
        config.port,
        agent_host_facade,
        shared_worker_registry.clone(),
    );
    let lifecycle = Arc::new(StatigLifecycle::new_with_subsystems(
        subsystems,
        config.shutdown_grace_ms,
    ));

    state.set_lifecycle(Arc::clone(&lifecycle));
    tracing::info!("Lifecycle HSM initialized");

    // --- Section 7: Signal handlers and panic hook ---
    let lifecycle_for_signals = Arc::clone(&lifecycle);
    let state_for_signals = state.clone();
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to register SIGTERM handler");
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .expect("Failed to register SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("SIGTERM received");
                lifecycle_for_signals.dispatch(Event::ShutdownRequested {
                    source: "signal".into(),
                });
                state_for_signals.request_shutdown();
            }
            _ = sigint.recv() => {
                tracing::info!("SIGINT received (Ctrl+C)");
                lifecycle_for_signals.dispatch(Event::ShutdownRequested {
                    source: "signal".into(),
                });
                state_for_signals.request_shutdown();
            }
        }
    });

    // Set up panic hook
    let lifecycle_for_panic = Arc::clone(&lifecycle);
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("Panic occurred: {}", info);
        lifecycle_for_panic.dispatch(Event::FatalError {
            kind: SubsystemKind::Engine,
            err: info.to_string(),
        });
        previous_hook(info);
    }));

    // --- Section 8: Graceful shutdown watcher ---
    {
        let state_for_shutdown = state.clone();
        tokio::spawn(async move {
            state_for_shutdown.shutdown_notify().notified().await;
            tracing::info!("Shutdown notify received — draining engine sessions and workers");

            if let Some(supervisor) = state_for_shutdown.schedule_supervisor() {
                match supervisor.resume_running_as_paused("daemon_shutdown").await {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!(
                                "paused {} running schedule(s) for graceful shutdown",
                                count
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!("failed to pause running schedules on shutdown: {}", e);
                    }
                }
            }

            if let Some(engine) = state_for_shutdown.engine() {
                match engine
                    .list_active(nexus_orchestration::engine::SessionFilter::default())
                    .await
                {
                    Ok(sessions) => {
                        let count = sessions.len();
                        for s in sessions {
                            let _ = engine.signal(&s.session_id, EngineSignal::Cancel).await;
                        }
                        tracing::info!("cancelled {} active session(s)", count);
                    }
                    Err(e) => {
                        tracing::warn!("failed to list active sessions for shutdown: {}", e);
                    }
                }
            }

            tracing::info!("engine + worker shutdown complete");
        });
    }

    // --- Section 9: HTTP/Unix server + lifecycle start ---
    let shutdown_notify = state.shutdown_notify();

    // Resolve daemon API key configuration (T1: DaemonApiConfig)
    let auth_config = api::auth_middleware::DaemonApiConfig::from_env();
    // T7: startup warning is logged inside from_env() for keyless-localhost mode.

    let app = api::create_router(state, auth_config);

    // Resolve transport
    let transport = config.resolve_transport();

    // --- Section 10: Start lifecycle and spawn server ---
    lifecycle.dispatch(Event::ProcessStarted);
    tracing::info!("Lifecycle started");

    // Spawn HTTP/Unix server
    let _server_result = tokio::spawn(async move {
        match transport {
            Transport::Http { port, host } => {
                let addr = format!("{host}:{port}");
                let listener = tokio::net::TcpListener::bind(&addr).await?;

                tracing::info!("Local API listening on http://{}", addr);
                tracing::info!("Web UI available at http://{}", addr);
                tracing::info!("Press Ctrl+C to stop");

                axum::serve(listener, app)
                    .with_graceful_shutdown({
                        let notify = Arc::clone(&shutdown_notify);
                        async move {
                            notify.notified().await;
                        }
                    })
                    .await?;
            }
            Transport::UnixSocket { path } => {
                if path.exists() {
                    std::fs::remove_file(&path)?;
                    tracing::info!(?path, "Removed stale socket file");
                }

                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                #[cfg(unix)]
                {
                    use tokio::net::UnixListener;

                    let listener = UnixListener::bind(&path)?;

                    tracing::info!(?path, "Local API listening on Unix socket");
                    tracing::info!("Press Ctrl+C to stop");

                    loop {
                        let (unix_stream, _addr) = tokio::select! {
                            result = listener.accept() => {
                                match result {
                                    Ok(stream) => stream,
                                    Err(e) => {
                                        tracing::error!("Unix socket accept error: {}", e);
                                        continue;
                                    }
                                }
                            }
                            () = shutdown_notify.notified() => {
                                tracing::info!("Shutdown signal received");
                                break;
                            }
                        };

                        let app_clone = app.clone();

                        tokio::spawn(async move {
                            let io = hyper_util::rt::TokioIo::new(unix_stream);

                            let hyper_service =
                                hyper_util::service::TowerToHyperService::new(app_clone);

                            let builder = hyper::server::conn::http1::Builder::new();
                            let conn = builder.serve_connection(io, hyper_service);

                            if let Err(e) = conn.await {
                                tracing::warn!("Unix socket connection error: {}", e);
                            }
                        });
                    }

                    let _ = std::fs::remove_file(&path);
                }

                #[cfg(not(unix))]
                {
                    anyhow::bail!(
                        "Unix socket transport is not supported on this platform. \
                         Use HTTP transport instead (default)."
                    );
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    });

    // Wait for lifecycle to reach terminal state (Failed)
    lifecycle.wait_until_terminal().await;

    tracing::info!("Lifecycle reached terminal state");

    Ok(())
}

/// Create subsystem bootstraps for lifecycle.
fn create_subsystems(
    state: &WorkspaceState,
    port: u16,
    agent_host_facade: Arc<dyn nexus_agent_host::HostFacade>,
    worker_registry: Arc<tokio::sync::Mutex<WorkerRegistry<WorkerManagerSpawner>>>,
) -> Vec<Arc<dyn crate::lifecycle::SubsystemBootstrap>> {
    use crate::lifecycle::{AgentHostSubsystem, DbSubsystem, HttpSubsystem, WorkerMgrSubsystem};

    let nexus_home = state.nexus_home();
    let agent_host_config_path = nexus_home.join("agent-host").join("config.toml");
    let workspace_root = state
        .workspace_path()
        .map_or_else(|| nexus_home.clone(), std::path::PathBuf::from);

    let mut subsystems: Vec<Arc<dyn crate::lifecycle::SubsystemBootstrap>> = vec![
        Arc::new(HttpSubsystem::new(port)),
        Arc::new(DbSubsystem::new(Some(state.database_path()))),
        // V1.51 T-A P0 (QC3 F-001): share the same worker registry that
        // backs `ProductionWorkerProvider` so workers spawned by either side
        // are visible to the other.
        Arc::new(WorkerMgrSubsystem::with_registry(worker_registry)),
    ];

    // Agent Host is an optional subsystem — failure does not block daemon startup
    subsystems.push(Arc::new(AgentHostSubsystem::new(
        agent_host_facade,
        agent_host_config_path,
        workspace_root,
    )));

    subsystems
}

/// Create a new schedule for an auto-chain work at boot recovery.
///
/// Delegates to the shared `auto_chain::enqueue_auto_chain_schedule` helper
/// (Fix A / W-A) so that the ID-mint + INSERT + `set_driver` logic is not
/// duplicated between the boot and supervisor paths.
async fn resume_auto_chain_work(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    work_id: &str,
    stage: &str,
    chapter: Option<i32>,
    work: &nexus_local_db::works::WorkRecord,
) -> Result<String, String> {
    nexus_orchestration::auto_chain::enqueue_auto_chain_schedule(
        pool, creator_id, work_id, stage, chapter, None, work,
    )
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_defaults_to_http() {
        let config = DaemonConfig {
            port: 8420,
            host: "127.0.0.1".to_string(),
            socket_path: None,
            verbose: false,
            shutdown_grace_ms: 20000,
            cdn_url: None,
        };

        std::env::remove_var("NEXUS_DAEMON_SOCKET_PATH");

        let transport = config.resolve_transport();
        match transport {
            Transport::Http { port, host } => {
                assert_eq!(port, 8420);
                assert_eq!(host, "127.0.0.1");
            }
            Transport::UnixSocket { .. } => panic!("Expected HTTP transport"),
        }
    }

    #[test]
    fn socket_path_takes_priority() {
        let config = DaemonConfig {
            port: 9000,
            host: "127.0.0.1".to_string(),
            socket_path: Some(PathBuf::from("/tmp/test.sock")),
            verbose: false,
            shutdown_grace_ms: 20000,
            cdn_url: None,
        };

        let transport = config.resolve_transport();
        match transport {
            Transport::UnixSocket { path } => {
                assert_eq!(path, PathBuf::from("/tmp/test.sock"));
            }
            Transport::Http { .. } => panic!("Expected Unix socket transport"),
        }
    }

    #[test]
    fn http_transport_uses_config_port() {
        let config = DaemonConfig {
            port: 9999,
            host: "0.0.0.0".to_string(),
            socket_path: None,
            verbose: false,
            shutdown_grace_ms: 20000,
            cdn_url: None,
        };

        std::env::remove_var("NEXUS_DAEMON_SOCKET_PATH");

        let transport = config.resolve_transport();
        match transport {
            Transport::Http { port, host } => {
                assert_eq!(port, 9999);
                assert_eq!(host, "0.0.0.0");
            }
            Transport::UnixSocket { .. } => panic!("Expected HTTP transport"),
        }
    }
}
