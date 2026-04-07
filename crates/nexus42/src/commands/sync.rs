//! Sync Command — Synchronize workspace state with platform

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum SyncCommand {
    /// Push local changes to platform
    Push {
        /// Force push even if conflicts detected
        #[arg(long)]
        force: bool,
    },

    /// Pull platform changes to local workspace
    Pull,

    /// Show sync status
    Status,
}

/// Sync status response from the daemon
#[derive(Debug, serde::Deserialize)]
pub struct SyncStatusResponse {
    pub pending_count: u64,
    pub failed_count: u64,
    pub last_sync_at: Option<String>,
    pub conflict_count: u64,
}

/// Run sync command
pub async fn run(cmd: SyncCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);

    match cmd {
        SyncCommand::Push { force } => {
            if !client.health_check().await? {
                return Err(crate::errors::CliError::DaemonNotRunning);
            }
            println!("Pushing local changes to platform...");
            println!("⚠ V1.0 skeleton: sync not yet implemented.");
            if force {
                println!("  --force flag noted (will override conflict checks).");
            }
        }
        SyncCommand::Pull => {
            if !client.health_check().await? {
                return Err(crate::errors::CliError::DaemonNotRunning);
            }
            println!("Pulling platform changes...");
            println!("⚠ V1.0 skeleton: sync not yet implemented.");
        }
        SyncCommand::Status => {
            if !client.health_check().await? {
                println!("Sync Status:");
                println!("  Daemon: not running");
                println!();
                println!("Start the daemon with: nexus42 daemon start");
                return Ok(());
            }

            match client
                .get::<SyncStatusResponse>("/v1/local/sync/status")
                .await
            {
                Ok(status) => {
                    println!("Sync Status:");
                    println!("  Pending bundles: {}", status.pending_count);
                    println!("  Failed bundles:  {}", status.failed_count);
                    println!("  Conflicts:        {}", status.conflict_count);

                    match &status.last_sync_at {
                        Some(ts) => println!("  Last sync:        {}", ts),
                        None => println!("  Last sync:        never"),
                    }

                    if status.pending_count > 0 {
                        println!();
                        println!("  Sync pending changes with: nexus42 sync push");
                    }
                    if status.failed_count > 0 {
                        println!();
                        println!(
                            "  ⚠ {} failed bundle(s) need attention.",
                            status.failed_count
                        );
                    }
                }
                Err(e) => {
                    println!("Sync Status:");
                    println!("  Error: {}", e);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_response_deserialization() {
        let json = r#"{
            "pending_count": 3,
            "failed_count": 1,
            "last_sync_at": "2026-04-07T00:00:00Z",
            "conflict_count": 0
        }"#;
        let resp: SyncStatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.pending_count, 3);
        assert_eq!(resp.failed_count, 1);
        assert_eq!(resp.conflict_count, 0);
        assert_eq!(resp.last_sync_at, Some("2026-04-07T00:00:00Z".to_string()));
    }

    #[test]
    fn test_sync_status_response_no_last_sync() {
        let json = r#"{
            "pending_count": 0,
            "failed_count": 0,
            "last_sync_at": null,
            "conflict_count": 0
        }"#;
        let resp: SyncStatusResponse = serde_json::from_str(json).unwrap();
        assert!(resp.last_sync_at.is_none());
    }
}
