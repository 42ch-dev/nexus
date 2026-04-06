//! nexus42d — Nexus Daemon Binary
//!
//! Local supervisor service managing workspace, auth, and sync operations.
//! Provides the Local API (HTTP JSON on port 8420) for CLI communication.

use clap::Parser;
use nexus42d::api;
use nexus42d::workspace::WorkspaceState;
use tracing_subscriber::EnvFilter;

/// Nexus Daemon — local supervisor for the CLI
#[derive(Parser, Debug)]
#[command(
    name = "nexus42d",
    version,
    about = "Nexus local daemon — manages workspace, auth, and sync"
)]
struct DaemonArgs {
    /// Port to listen on (default: 8420)
    #[arg(short, long, default_value_t = 8420)]
    port: u16,

    /// Bind address (default: 127.0.0.1)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

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

    // Initialize workspace state
    let state = WorkspaceState::initialize()?;
    tracing::info!("Workspace state initialized");

    // Build and start the HTTP server
    let app = api::create_router(state);
    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Local API listening on http://{}", addr);
    tracing::info!("Press Ctrl+C to stop");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

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
