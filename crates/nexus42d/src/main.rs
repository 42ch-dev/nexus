//! nexus42d — Nexus Daemon Binary
//!
//! Local supervisor service managing workspace, auth, and sync operations.
//! Provides the Local API (HTTP JSON on port 8420) for CLI communication.
//!
//! # Transport Options
//!
//! The daemon supports two transport mechanisms:
//! - **HTTP** (default): TCP loopback on configurable port (default 8420)
//! - **Unix socket**: Domain socket for better security and performance
//!
//! Switch via `NEXUS_DAEMON_SOCKET_PATH` environment variable or CLI flags.

use std::path::PathBuf;

use clap::Parser;
use nexus42d::api;
use nexus42d::workspace::WorkspaceState;
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
    let state = WorkspaceState::initialize().await?;
    tracing::info!("Workspace state initialized");

    // Build the router
    let app = api::create_router(state);

    // Resolve transport
    let transport = Transport::from_args(&args);

    // Start the server with the appropriate transport
    match transport {
        Transport::Http { port, host } => {
            let addr = format!("{}:{}", host, port);
            let listener = tokio::net::TcpListener::bind(&addr).await?;

            tracing::info!("Local API listening on http://{}", addr);
            tracing::info!("Press Ctrl+C to stop");

            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
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

                // axum 0.7's serve() only accepts TcpListener. For Unix sockets,
                // we replicate axum's serve loop manually but for Unix streams.
                // The key transformation is: Router → TowerToHyperService → hyper::serve_connection.
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
                        _ = shutdown_signal() => {
                            tracing::info!("Shutdown signal received");
                            break;
                        }
                    };

                    let app_clone = app.clone();

                    tokio::spawn(async move {
                        let io = hyper_util::rt::TokioIo::new(unix_stream);

                        // Convert the axum Router into a hyper-compatible HTTP service.
                        // axum 0.7 uses `hyper::body::Incoming` internally when serving,
                        // but the Router itself is `Service<Request<Body>>`. The
                        // TowerToHyperService handles the Incoming ↔ Body conversion
                        // automatically when used with hyper's serve_connection.
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

    tracing::info!("Daemon stopped.");
    Ok(())
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");
    tracing::info!("Shutdown signal received");
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
}
