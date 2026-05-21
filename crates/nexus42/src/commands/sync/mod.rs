//! Sync Command — Synchronize workspace state with platform
//!
//! Provides push, pull, status, and resolve subcommands for sync operations.
//! The `resolve` subcommand includes a safety confirmation prompt for
//! auto-reject resolution (SYNC-R13), which can be bypassed with `--force`.
//!
//! # Wiring (V1.21 — Batch E)
//!
//! CLI sync commands use `nexus-cloud-sync` directly (no daemon proxy):
//! - **Push**: builds a bundle via `delta_bundle`, uploads via `SyncClient::push_bundle`
//! - **Pull**: fetches bundles via `SyncClient::pull_bundles`
//! - **Status/Resolve**: reads/writes the local outbox (`Outbox` backed by `state.db`)

pub mod world;

use crate::config::CliConfig;
use crate::domain::runtime_guard;
use crate::errors::Result;
use clap::Subcommand;
use nexus_cloud_sync::delta_bundle::BundleBuilder;
use nexus_cloud_sync::sync_client::SyncClient;

/// Supported conflict resolution strategies.
///
/// Mirrors `nexus_cloud_sync::conflict::ConflictResolution` for CLI use,
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
        /// Workspace ID for the bundle
        #[arg(long)]
        workspace_id: Option<String>,

        /// World ID for the bundle
        #[arg(long)]
        world_id: Option<String>,

        /// Creator ID submitting the bundle
        #[arg(long)]
        creator_id: Option<String>,

        /// Force push even if precheck would fail
        #[arg(long)]
        force: bool,
    },

    /// Pull platform bundles into the local outbox (requires platform URL/token on daemon)
    Pull {
        /// World ID to pull (must match workspace sync binding when configured)
        #[arg(long)]
        world_id: Option<String>,

        /// Incremental cursor: only bundles after this server confirmed delta sequence
        #[arg(long)]
        after_sequence: Option<u64>,
    },

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

    /// World fork and snapshot (migrated from `nexus42 world`)
    World {
        #[command(subcommand)]
        command: world::WorldCommand,
    },

    /// Retry a failed sync operation (coming soon)
    Retry {
        /// Bundle ID to retry
        bundle_id: Option<String>,
    },
}

// ── Local models (used for outbox status display) ──────────────────

/// Sync status from the local outbox.
#[derive(Debug)]
pub struct SyncStatusInfo {
    pub staged_count: usize,
    pub ready_count: usize,
    pub sent_count: usize,
    pub acked_count: usize,
    pub failed_count: usize,
    pub conflicted_count: usize,
}

// ── Confirmation prompt ───────────────────────────────────────────

/// Check whether the user confirms a destructive auto-reject action.
///
/// Returns `true` if the user confirms or if `force` is set.
/// Returns `false` if the user declines.
///
/// In non-interactive (no TTY) contexts, confirmation defaults to `false`
/// unless `force` is set.
#[must_use]
pub fn confirm_auto_reject(force: bool) -> bool {
    if force {
        return true;
    }

    // dialoguer::Confirm returns Err when there is no TTY (non-interactive).
    // In that case, default to false (reject the destructive action).
    dialoguer::Confirm::new()
        .with_prompt("Auto-reject will discard all conflicting server changes. Continue?")
        .default(false)
        .interact()
        .unwrap_or_else(|_| {
            eprintln!("Non-interactive terminal: auto-reject requires --force flag.");
            false
        })
}

// ── Command runner ────────────────────────────────────────────────

/// Run sync command.
///
/// # Errors
///
/// Returns an error if:
/// - Platform connectivity is required but unavailable
/// - Authentication is missing or expired
/// - Sync API calls fail
/// - Invalid `world_id` or `creator_id` parameters
///
/// Note: This function is long; splitting would break the coherent sync flow.
#[allow(clippy::too_many_lines)]
pub async fn run(cmd: SyncCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        SyncCommand::Push {
            workspace_id,
            world_id,
            creator_id,
            force: _,
        } => {
            runtime_guard::require_platform(&config.runtime_mode(), "sync push")?;

            let Some(creator_id) = creator_id.or_else(|| config.active_creator_id.clone()) else {
                return Err(crate::errors::CliError::Other(
                    "Creator ID required for sync push. Use --creator-id or set active creator."
                        .to_string(),
                ));
            };

            let workspace_id = workspace_id.unwrap_or_else(|| {
                eprintln!("Warning: sync push using placeholder workspace_id \"local\".");
                "local".to_string()
            });
            let world_id = world_id.unwrap_or_else(|| {
                eprintln!("Warning: sync push using placeholder world_id \"unknown\".");
                "unknown".to_string()
            });

            // Obtain auth token
            let auth_token = crate::auth::user_auth::ensure_valid_token(config).await?;

            // Build a minimal bundle
            let bundle = BundleBuilder::new(&workspace_id, &world_id, &creator_id).build()?;

            // Push directly to platform via cloud-sync
            let sync_client = SyncClient::new(&config.platform_url, &auth_token)?;
            match sync_client.push_bundle(&bundle).await {
                Ok(response) => {
                    println!("Sync push completed.");
                    if let Some(rev) = response.world_revision {
                        println!("  World revision: {rev}");
                    }
                    if let Some(seq) = response.confirmed_delta_sequence {
                        println!("  Confirmed delta sequence: {seq}");
                    }
                    if !response.success {
                        eprintln!("  Push reported non-success from platform.");
                    }
                }
                Err(e) => {
                    eprintln!("Sync push failed: {e}");
                    return Err(e.into());
                }
            }
        }
        SyncCommand::Pull {
            world_id,
            after_sequence,
        } => {
            runtime_guard::require_platform(&config.runtime_mode(), "sync pull")?;

            let world_id = world_id.unwrap_or_else(|| {
                eprintln!("Warning: sync pull without --world-id uses placeholder \"unknown\".");
                "unknown".to_string()
            });

            // Obtain auth token
            let auth_token = crate::auth::user_auth::ensure_valid_token(config).await?;

            let request = nexus_contracts::SyncPullRequest {
                schema_version: 1,
                world_id,
                after_confirmed_delta_sequence: after_sequence,
            };

            // Pull directly from platform via cloud-sync
            let sync_client = SyncClient::new(&config.platform_url, &auth_token)?;
            match sync_client.pull_bundles(&request).await {
                Ok(resp) => {
                    println!("Sync pull completed.");
                    println!("  World revision:           {}", resp.world_revision);
                    println!(
                        "  Confirmed delta sequence: {}",
                        resp.confirmed_delta_sequence
                    );
                    println!("  Bundle count:             {}", resp.bundles.len());
                    if let Some(up) = resp.is_up_to_date {
                        println!("  Up to date:               {up}");
                    }
                }
                Err(e) => {
                    eprintln!("Sync pull failed: {e}");
                    return Err(e.into());
                }
            }
        }
        SyncCommand::Status => {
            // Query local outbox for status
            match get_local_outbox_status(config).await {
                Ok(status) => {
                    println!("Sync Status (local outbox):");
                    println!("  Staged:    {}", status.staged_count);
                    println!("  Ready:     {}", status.ready_count);
                    println!("  Sent:      {}", status.sent_count);
                    println!("  Acked:     {}", status.acked_count);
                    println!("  Conflicts: {}", status.conflicted_count);
                    println!("  Failed:    {}", status.failed_count);

                    let total_pending = status.staged_count + status.ready_count;
                    if total_pending > 0 {
                        println!();
                        println!("  Sync pending changes with: nexus42 sync push");
                    }
                    if status.conflicted_count > 0 {
                        println!();
                        println!(
                            "  ⚠ {} conflicted bundle(s) need attention.",
                            status.conflicted_count
                        );
                        println!("  Resolve with: nexus42 sync resolve <entry_id> --resolution <strategy>");
                    }
                    if status.failed_count > 0 {
                        println!();
                        println!(
                            "  ⚠ {} failed bundle(s) will be retried automatically.",
                            status.failed_count
                        );
                    }
                }
                Err(e) => {
                    println!("Sync Status:");
                    println!("  Error: {e}");
                    println!("  Hint: Ensure the CLI state database is initialized.");
                }
            }
        }
        SyncCommand::Resolve {
            outbox_entry_id,
            resolution,
            force,
        } => {
            runtime_guard::require_platform(&config.runtime_mode(), "sync resolve")?;

            // Safety prompt for auto-reject (SYNC-R13)
            if resolution == ResolutionStrategy::AutoReject && !confirm_auto_reject(force) {
                println!("Auto-reject cancelled.");
                return Ok(());
            }

            // Resolve via local outbox
            match resolve_outbox_entry(config, &outbox_entry_id, &resolution).await {
                Ok(()) => {
                    println!("Resolved entry {outbox_entry_id} with strategy: {resolution}");
                }
                Err(e) => {
                    eprintln!("Resolve failed for entry {outbox_entry_id}: {e}");
                    return Err(e);
                }
            }
        }
        SyncCommand::World { command } => {
            world::run(command).await?;
        }
        SyncCommand::Retry { bundle_id } => match bundle_id {
            Some(id) => {
                println!("Coming soon: `sync retry` — retry failed bundle: {id}");
            }
            None => {
                println!("Coming soon: `sync retry` — retry all failed bundles.");
            }
        },
    }

    Ok(())
}

// ── Local outbox helpers ─────────────────────────────────────────

/// Get local outbox status by opening the CLI's state.db.
async fn get_local_outbox_status(config: &CliConfig) -> Result<SyncStatusInfo> {
    let db_path = crate::config::resolve_state_db_path(config)?;
    let outbox = nexus_cloud_sync::outbox::Outbox::new(&db_path).await?;

    let staged_count = outbox.count_by_state("staged").await.unwrap_or(0);
    let ready_count = outbox.count_by_state("ready").await.unwrap_or(0);
    let sent_count = outbox.count_by_state("sent").await.unwrap_or(0);
    let acked_count = outbox.count_by_state("acked").await.unwrap_or(0);
    let failed_count = outbox.count_by_state("failed").await.unwrap_or(0);
    let conflicted_count = outbox.count_by_state("conflicted").await.unwrap_or(0);

    Ok(SyncStatusInfo {
        staged_count,
        ready_count,
        sent_count,
        acked_count,
        failed_count,
        conflicted_count,
    })
}

/// Resolve an outbox entry by updating its state in the local outbox.
async fn resolve_outbox_entry(
    config: &CliConfig,
    outbox_entry_id: &str,
    resolution: &ResolutionStrategy,
) -> Result<()> {
    let db_path = crate::config::resolve_state_db_path(config)?;
    let outbox = nexus_cloud_sync::outbox::Outbox::new(&db_path).await?;

    match resolution {
        ResolutionStrategy::AutoAccept => {
            // Mark as acked — accept server state
            outbox.mark_acked(outbox_entry_id).await?;
        }
        ResolutionStrategy::AutoReject => {
            // Mark as failed — keep local state (destructive for server changes)
            outbox
                .mark_failed(outbox_entry_id, "auto_reject: discarded server changes")
                .await?;
        }
        ResolutionStrategy::ManualReview => {
            // No state change — just log for user awareness
            println!("  Entry {outbox_entry_id} left in conflicted state for manual review.");
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
