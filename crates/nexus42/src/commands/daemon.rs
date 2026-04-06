//! Daemon Command — Manage the nexus42d daemon

use crate::api::DaemonClient;
use crate::config::{CliConfig, DAEMON_PORT};
use crate::errors::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Start the nexus42d daemon
    Start {
        /// Port to listen on (default: 8420)
        #[arg(long, default_value_t = DAEMON_PORT)]
        port: u16,
    },

    /// Stop the running daemon
    Stop,

    /// Check daemon status / health
    Status,
}

/// Run daemon command
pub async fn run(cmd: DaemonCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        DaemonCommand::Start { port } => start_daemon(port).await,
        DaemonCommand::Stop => stop_daemon(config).await,
        DaemonCommand::Status => daemon_status(config).await,
    }
}

/// Start the daemon process
async fn start_daemon(port: u16) -> Result<()> {
    // Check if already running
    let client = DaemonClient::new(&format!("http://127.0.0.1:{}", port));
    if client.health_check().await? {
        println!("Daemon is already running on port {}", port);
        return Ok(());
    }

    // Try to spawn the daemon process
    // In production, this would spawn `nexus42d` as a background process
    println!("Starting nexus42d daemon on port {}...", port);
    println!();
    println!("⚠ V1.0 skeleton: run manually with:");
    println!("  cargo run -p nexus42d -- --port {}", port);
    println!("  (or) ./target/debug/nexus42d --port {}", port);
    println!();
    println!("To run in the background:");
    println!("  nohup ./target/debug/nexus42d --port {} &", port);

    Ok(())
}

/// Stop the daemon
async fn stop_daemon(config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);

    if !client.health_check().await? {
        println!("Daemon is not running.");
        return Ok(());
    }

    // In production, send a shutdown signal to the daemon
    println!("⚠ V1.0 skeleton: stop daemon manually with:");
    println!("  kill $(lsof -ti:8420)");
    Ok(())
}

/// Check daemon status
async fn daemon_status(config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);

    println!("Daemon Status:");
    println!("  URL: {}", config.daemon_url);

    if client.health_check().await? {
        println!("  Status: ✓ Running");
        // Try to get more info
        if let Ok(status) = client
            .get::<serde_json::Value>("/v1/local/runtime/status")
            .await
        {
            if let Some(version) = status.get("version") {
                println!("  Version: {}", version);
            }
        }
    } else {
        println!("  Status: ✗ Not running");
        println!();
        println!("Start with: nexus42 daemon start");
    }

    Ok(())
}
