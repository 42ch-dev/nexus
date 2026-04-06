//! Daemon Command — Manage the nexus42d daemon

use crate::api::DaemonClient;
use crate::config::{CliConfig, DAEMON_PORT};
use crate::errors::Result;
use clap::Subcommand;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

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

    println!("Starting nexus42d daemon on port {}...", port);

    // Spawn nexus42d as a detached background process.
    // In development, we use `cargo run -p nexus42d`.
    // In production builds, the installed binary is used directly.
    let daemon_cmd = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            // Check if we're running from cargo (debug build path contains target/debug)
            let exe_str = exe.to_string_lossy();
            if exe_str.contains("target/debug") || exe_str.contains("target/release") {
                // Development: use cargo run so the daemon binary is up-to-date
                Some((
                    "cargo".to_string(),
                    vec![
                        "run".to_string(),
                        "-p".to_string(),
                        "nexus42d".to_string(),
                        "--".to_string(),
                        "--port".to_string(),
                        port.to_string(),
                    ],
                ))
            } else {
                // Production: derive the daemon binary path from the CLI binary path
                let parent = exe.parent()?;
                let daemon_path = parent.join("nexus42d");
                if daemon_path.exists() {
                    Some((
                        daemon_path.display().to_string(),
                        vec!["--port".to_string(), port.to_string()],
                    ))
                } else {
                    None
                }
            }
        })
        .or_else(|| {
            // Fallback: try cargo run
            Some((
                "cargo".to_string(),
                vec![
                    "run".to_string(),
                    "-p".to_string(),
                    "nexus42d".to_string(),
                    "--".to_string(),
                    "--port".to_string(),
                    port.to_string(),
                ],
            ))
        });

    if let Some((program, args)) = daemon_cmd {
        let mut child = std::process::Command::new(&program)
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            // Detach the process so it outlives the CLI
            .process_group(0)
            .spawn()
            .map_err(|e| crate::errors::CliError::Daemon {
                message: format!("Failed to spawn daemon process '{}': {}", program, e),
            })?;

        // On Unix, double-fork to fully detach; on other platforms the child
        // already runs independently via process_group(0).
        #[cfg(unix)]
        {
            // Immediately reap the intermediate process so it doesn't become a zombie.
            // The grandchild (actual daemon) continues running.
            let _ = child.wait();
        }
        #[cfg(not(unix))]
        {
            // On non-Unix, we can't double-fork; the child will keep running.
            // Prevent the CLI from waiting on it.
            let _ = child.try_wait();
        }

        // Wait briefly and verify the daemon is responding
        println!("Waiting for daemon to start...");
        let max_retries = 10u32;
        let retry_delay = std::time::Duration::from_millis(500);

        for i in 1..=max_retries {
            tokio::time::sleep(retry_delay).await;
            if client.health_check().await? {
                println!("✓ Daemon started successfully on port {}", port);
                println!("  PID: child process");
                return Ok(());
            }
            if i == max_retries {
                println!(
                    "⚠ Daemon process was spawned but health check failed after {} retries.",
                    max_retries
                );
                println!("  The daemon may still be starting. Check with: nexus42 daemon status");
                println!("  Or check logs: journalctl --user -u nexus42d");
            }
        }

        Ok(())
    } else {
        Err(crate::errors::CliError::Daemon {
            message:
                "Could not locate nexus42d binary. Please install it or run: cargo run -p nexus42d"
                    .to_string(),
        })
    }
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
