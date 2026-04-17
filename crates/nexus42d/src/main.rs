//! nexus42d — Nexus Daemon Binary
//!
//! Local supervisor service managing workspace, auth, and sync operations.
//! Provides the Local API (HTTP JSON on port 8420) for CLI communication.
//!
//! # Lifecycle HSM (WS4 T6)
//!
//! The daemon uses a `statig`-based HSM for lifecycle management:
//! - States: `Stopped → Starting → Running ⇄ Degraded → Stopping → Failed`
//! - Signal handlers dispatch `ShutdownRequested` on SIGTERM/SIGINT
//! - Panic hook dispatches `FatalError` on panics
//! - Entry/exit actions manage subsystem lifecycle
//!
//! # Transport Options
//!
//! The daemon supports two transport mechanisms:
//! - **HTTP** (default): TCP loopback on configurable port (default 8420)
//! - **Unix socket**: Domain socket for better security and performance
//!
//! Switch via `NEXUS_DAEMON_SOCKET_PATH` environment variable or CLI flags.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use nexus42d::api;
use nexus42d::lifecycle::{Event, Lifecycle, StatigLifecycle, SubsystemKind};
use nexus42d::workspace::WorkspaceState;
use nexus_orchestration::{
    engine::{EngineSignal, OrchestrationEngine},
    system_preset, CapabilityRegistry, GraphFlowEngine, WorkerManager,
};
use tracing_subscriber::EnvFilter;

/// Local API transport configuration
#[derive(Debug, Clone)]
pub enum Transport {
    /// HTTP over TCP loopback (default)
    Http { port: u16, host: String },
    /// Unix domain socket
    UnixSocket { path: PathBuf },
}

impl Transport {
    /// Resolve transport from CLI arguments and environment variables.
    ///
    /// Priority:
    /// 1. `--socket-path` CLI flag
    /// 2. `NEXUS_DAEMON_SOCKET_PATH` environment variable
    /// 3. `--port` / `--host` CLI flags (HTTP)
    /// 4. `NEXUS_DAEMON_PORT` environment variable (HTTP fallback)
    /// 5. Default: HTTP on 127.0.0.1:8420
    pub fn from_args(args: &DaemonArgs) -> Self {
        // Unix socket takes priority
        if let Some(ref path) = args.socket_path {
            return Transport::UnixSocket { path: path.clone() };
        }

        if let Ok(path) = std::env::var("NEXUS_DAEMON_SOCKET_PATH") {
            return Transport::UnixSocket {
                path: PathBuf::from(path),
            };
        }

        // HTTP fallback
        let port = if args.port != 8420 {
            // User explicitly set a non-default port via CLI
            args.port
        } else {
            std::env::var("NEXUS_DAEMON_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(args.port)
        };

        Transport::Http {
            port,
            host: args.host.clone(),
        }
    }
}

/// Nexus Daemon — local supervisor for the CLI
#[derive(Parser, Debug)]
#[command(
    name = "nexus42d",
    version,
    about = "Nexus local daemon — manages workspace, auth, and sync"
)]
pub struct DaemonArgs {
    /// Port to listen on (default: 8420, ignored when --socket-path is set)
    #[arg(short, long, default_value_t = 8420)]
    port: u16,

    /// Bind address (default: 127.0.0.1, ignored when --socket-path is set)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Use Unix domain socket at the given path instead of HTTP
    #[arg(long)]
    socket_path: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Shutdown grace period in milliseconds (default: 20000)
    #[arg(long, default_value_t = 20000)]
    shutdown_grace_ms: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = DaemonArgs::parse();

    // Initialize logging
    let filter = if args.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    tracing::info!("Starting nexus42d v{}", env!("CARGO_PKG_VERSION"));

    // Initialize workspace state (async: initializes sync outbox)
    let mut state = WorkspaceState::initialize().await?;
    tracing::info!("Workspace state initialized");

    // --- WS2: Instantiate orchestration engine + worker manager ---
    // Re-use the same pool that WorkspaceState opened (nexus_local_db::open_pool).
    let db_pool: sqlx::SqlitePool = state.pool().clone();
    let storage = Arc::new(
        nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(std::sync::Arc::new(
            db_pool,
        )),
    );
    let concrete_engine = GraphFlowEngine::new_with_storage(storage);

    // Create shared capability registry — used by system_preset and worker manager.
    let capabilities = Arc::new(CapabilityRegistry::with_builtins());

    // Kick off _system.maintenance session on the concrete engine
    // (start_session is a GraphFlowEngine method, not on the trait).
    let sys_graph = system_preset::build(capabilities.clone());
    match concrete_engine
        .start_session("_system.maintenance", sys_graph)
        .await
    {
        Ok(sid) => {
            tracing::info!(session_id = sid.0, "started _system.maintenance session");
        }
        Err(e) => {
            tracing::warn!("failed to start _system.maintenance: {}", e);
            // Non-fatal: the daemon can still operate without the system preset.
        }
    }

    // Wrap in trait object for WorkspaceState.
    let engine: Arc<dyn OrchestrationEngine> = Arc::new(concrete_engine);
    let workers = Arc::new(WorkerManager::new());

    state.set_engine(engine);
    state.set_worker_manager(workers);
    state.set_capability_registry(capabilities);
    tracing::info!("Orchestration engine wired");

    // Create lifecycle HSM with subsystems
    let subsystems = create_subsystems(&state, args.port);
    let lifecycle = Arc::new(StatigLifecycle::new_with_subsystems(
        subsystems,
        args.shutdown_grace_ms,
    ));

    // Attach lifecycle to workspace state
    state.set_lifecycle(Arc::clone(&lifecycle));
    tracing::info!("Lifecycle HSM initialized");

    // Set up signal handlers
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
        // Dispatch FatalError to lifecycle
        lifecycle_for_panic.dispatch(Event::FatalError {
            kind: SubsystemKind::Engine, // Generic subsystem
            err: info.to_string(),
        });
        // Call previous hook if any (default will print panic message)
        previous_hook(info);
    }));

    // Spawn graceful shutdown watcher: drains engine sessions + terminates workers
    // when shutdown_notify fires.
    {
        let state_for_shutdown = state.clone();
        tokio::spawn(async move {
            state_for_shutdown.shutdown_notify().notified().await;
            tracing::info!("Shutdown notify received — draining engine sessions and workers");

            // Cancel all active engine sessions.
            if let Some(engine) = state_for_shutdown.engine() {
                match engine.list_active(Default::default()).await {
                    Ok(sessions) => {
                        let count = sessions.len();
                        for s in sessions {
                            let _ =
                                engine.signal(&s.session_id, EngineSignal::Cancel).await;
                        }
                        tracing::info!("cancelled {} active session(s)", count);
                    }
                    Err(e) => {
                        tracing::warn!("failed to list active sessions for shutdown: {}", e);
                    }
                }
            }

            // Worker manager: each WorkerHandle cancels its child on Drop,
            // and the supervisors send SIGTERM. The worker manager itself has
            // no stateful workers list (they're owned by callers), so there's
            // nothing extra to do here. Workers are cleaned up when their
            // handles drop.
            tracing::info!("engine + worker shutdown complete");
        });
    }

    // Build the router with lifecycle-enabled state
    let shutdown_notify = state.shutdown_notify();
    let app = api::create_router(state);

    // Resolve transport
    let transport = Transport::from_args(&args);

    // Start the lifecycle
    lifecycle.dispatch(Event::ProcessStarted);
    tracing::info!("Lifecycle started");

    // Spawn HTTP server (runs independently of lifecycle state machine)
    let _server_result = tokio::spawn(async move {
        match transport {
            Transport::Http { port, host } => {
                let addr = format!("{}:{}", host, port);
                let listener = tokio::net::TcpListener::bind(&addr).await?;

                tracing::info!("Local API listening on http://{}", addr);
                tracing::info!("Press Ctrl+C to stop");

                axum::serve(listener, app)
                    .with_graceful_shutdown({
                        let notify = Arc::clone(&shutdown_notify);
                        async move { notify.notified().await; }
                    })
                    .await?;
            }
            Transport::UnixSocket { path } => {
                // Remove existing socket file if present
                if path.exists() {
                    std::fs::remove_file(&path)?;
                    tracing::info!(?path, "Removed stale socket file");
                }

                // Ensure parent directory exists
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
                            _ = shutdown_notify.notified() => {
                                // Graceful shutdown triggered
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

                    // Clean up socket file on shutdown
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

    // Server will stop when lifecycle exits (via Failed.entry calling process::exit)
    tracing::info!("Lifecycle reached terminal state");

    // Note: The daemon will exit via enter_failed() calling std::process::exit
    // This return is only reached in test mode where we don't call process::exit
    Ok(())
}

/// Create subsystem bootstraps for lifecycle.
///
/// Engine and WorkerMgr subsystems are mock implementations for lifecycle
/// health reporting only. The real engine is instantiated directly in main()
/// and wired via WorkspaceState (WS2).
fn create_subsystems(
    state: &WorkspaceState,
    port: u16,
) -> Vec<Arc<dyn nexus42d::lifecycle::SubsystemBootstrap>> {
    use nexus42d::lifecycle::{
        DbSubsystem, EngineSubsystem, HttpSubsystem, SyncSubsystem, WorkerMgrSubsystem,
    };

    vec![
        Arc::new(HttpSubsystem::new(port)),
        Arc::new(DbSubsystem::new(Some(state.database_path()))),
        Arc::new(SyncSubsystem::new()),
        Arc::new(EngineSubsystem::new()), // Mock - WS2 will provide real impl
        Arc::new(WorkerMgrSubsystem::new()), // Mock - WS2 will provide real impl
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_defaults_to_http() {
        let args = DaemonArgs {
            port: 8420,
            host: "127.0.0.1".to_string(),
            socket_path: None,
            verbose: false,
            shutdown_grace_ms: 20000,
        };

        // Clear env var for test
        std::env::remove_var("NEXUS_DAEMON_SOCKET_PATH");

        let transport = Transport::from_args(&args);
        match transport {
            Transport::Http { port, host } => {
                assert_eq!(port, 8420);
                assert_eq!(host, "127.0.0.1");
            }
            Transport::UnixSocket { .. } => panic!("Expected HTTP transport"),
        }
    }

    #[test]
    fn socket_path_cli_flag_takes_priority() {
        let args = DaemonArgs {
            port: 9000,
            host: "127.0.0.1".to_string(),
            socket_path: Some(PathBuf::from("/tmp/test.sock")),
            verbose: false,
            shutdown_grace_ms: 20000,
        };

        let transport = Transport::from_args(&args);
        match transport {
            Transport::UnixSocket { path } => {
                assert_eq!(path, PathBuf::from("/tmp/test.sock"));
            }
            Transport::Http { .. } => panic!("Expected Unix socket transport"),
        }
    }

    #[test]
    fn http_transport_uses_cli_port() {
        let args = DaemonArgs {
            port: 9999,
            host: "0.0.0.0".to_string(),
            socket_path: None,
            verbose: false,
            shutdown_grace_ms: 20000,
        };

        std::env::remove_var("NEXUS_DAEMON_SOCKET_PATH");

        let transport = Transport::from_args(&args);
        match transport {
            Transport::Http { port, host } => {
                assert_eq!(port, 9999);
                assert_eq!(host, "0.0.0.0");
            }
            Transport::UnixSocket { .. } => panic!("Expected HTTP transport"),
        }
    }

    #[test]
    fn shutdown_grace_default() {
        let args = DaemonArgs::parse_from(["nexus42d"]);
        assert_eq!(args.shutdown_grace_ms, 20000);
    }
}
