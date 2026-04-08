//! Sync Command — Synchronize workspace state with platform
//!
//! Provides push, pull, status, and resolve subcommands for sync operations.
//! The `resolve` subcommand includes a safety confirmation prompt for
//! auto-reject resolution (SYNC-R13), which can be bypassed with `--force`.

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

/// Supported conflict resolution strategies.
///
/// Mirrors `nexus_sync::conflict::ConflictResolution` for CLI use,
/// avoiding a direct dependency on the sync crate in the CLI layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ResolutionStrategy {
    /// Accept server state, discard local changes.
    AutoAccept,
    /// Keep local state, discard server changes (destructive).
    AutoReject,
    /// Present conflict to user for manual resolution.
    ManualReview,
}

impl std::fmt::Display for ResolutionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AutoAccept => write!(f, "auto_accept"),
            Self::AutoReject => write!(f, "auto_reject"),
            Self::ManualReview => write!(f, "manual_review"),
        }
    }
}

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

    /// Resolve a sync conflict with a specific strategy
    Resolve {
        /// Outbox entry ID to resolve
        outbox_entry_id: String,

        /// Resolution strategy to apply
        #[arg(long, value_enum)]
        resolution: ResolutionStrategy,

        /// Skip confirmation prompt (for automation/scripts)
        #[arg(long)]
        force: bool,
    },
}

/// Sync status response from the daemon
#[derive(Debug, serde::Deserialize)]
pub struct SyncStatusResponse {
    pub pending_count: u64,
    pub failed_count: u64,
    pub last_sync_at: Option<String>,
    pub conflict_count: u64,
}

/// Check whether the user confirms a destructive auto-reject action.
///
/// Returns `true` if the user confirms or if `force` is set.
/// Returns `false` if the user declines.
///
/// In non-interactive (no TTY) contexts, confirmation defaults to `false`
/// unless `force` is set.
pub fn confirm_auto_reject(force: bool) -> bool {
    if force {
        return true;
    }

    // dialoguer::Confirm returns Err when there is no TTY (non-interactive).
    // In that case, default to false (reject the destructive action).
    match dialoguer::Confirm::new()
        .with_prompt("Auto-reject will discard all conflicting server changes. Continue?")
        .default(false)
        .interact()
    {
        Ok(confirmed) => confirmed,
        Err(_) => {
            eprintln!("Non-interactive terminal: auto-reject requires --force flag.");
            false
        }
    }
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
        SyncCommand::Resolve {
            outbox_entry_id,
            resolution,
            force,
        } => {
            if !client.health_check().await? {
                return Err(crate::errors::CliError::DaemonNotRunning);
            }

            // Safety prompt for auto-reject (SYNC-R13)
            if resolution == ResolutionStrategy::AutoReject && !confirm_auto_reject(force) {
                println!("Auto-reject cancelled.");
                return Ok(());
            }

            println!(
                "Resolving conflict for entry {} with strategy: {}",
                outbox_entry_id, resolution
            );
            println!("⚠ V1.0 skeleton: conflict resolution not yet implemented.");
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

    // ── Resolution strategy tests (SYNC-R13) ─────────────────────

    #[test]
    fn test_resolution_strategy_display() {
        assert_eq!(ResolutionStrategy::AutoAccept.to_string(), "auto_accept");
        assert_eq!(ResolutionStrategy::AutoReject.to_string(), "auto_reject");
        assert_eq!(
            ResolutionStrategy::ManualReview.to_string(),
            "manual_review"
        );
    }

    #[test]
    fn test_confirm_auto_reject_force_bypasses_prompt() {
        // When force is true, confirmation should always succeed
        assert!(confirm_auto_reject(true));
    }

    #[test]
    fn test_confirm_auto_reject_without_force_in_no_tty() {
        // In test environments there is no TTY, so dialoguer returns Err.
        // confirm_auto_reject should return false (reject the destructive action).
        assert!(!confirm_auto_reject(false));
    }

    #[test]
    fn test_resolution_strategy_from_str() {
        // Verify clap ValueEnum parsing works
        let auto_accept: ResolutionStrategy =
            clap::ValueEnum::from_str("auto-accept", true).expect("parse auto-accept");
        assert_eq!(auto_accept, ResolutionStrategy::AutoAccept);

        let auto_reject: ResolutionStrategy =
            clap::ValueEnum::from_str("auto-reject", true).expect("parse auto-reject");
        assert_eq!(auto_reject, ResolutionStrategy::AutoReject);

        let manual: ResolutionStrategy =
            clap::ValueEnum::from_str("manual-review", true).expect("parse manual-review");
        assert_eq!(manual, ResolutionStrategy::ManualReview);
    }
}
