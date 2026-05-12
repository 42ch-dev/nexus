//! Sync Command — Synchronize workspace state with platform
//!
//! Provides push, pull, status, and resolve subcommands for sync operations.
//! The `resolve` subcommand includes a safety confirmation prompt for
//! auto-reject resolution (SYNC-R13), which can be bypassed with `--force`.
//!
//! # Wiring (TD-1)
//!
//! CLI sync commands call daemon HTTP endpoints backed by `nexus-sync` (**Outbox**,
//! **`BundleBuilder`**, **precheck**). Push builds a bundle (including `canonical_hash`),
//! runs precheck, then **`Outbox::stage`** (`ready`). HTTP upload to the platform via
//! **`SyncClient`** is offline-first (queued locally; optional daemon follow-up).
//! Pull calls **`POST /v1/local/sync/pull`**, which uses **`SyncClient::pull_bundles`**
//! against the platform and stages returned bundles (idempotent by `bundle_id`).

use crate::api::DaemonClient;
use crate::commands::world::WorldCommand;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_contracts::SyncPullRequest;
use nexus_domain::runtime_guard;
use serde::{Deserialize, Serialize};

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
        command: WorldCommand,
    },

    /// Retry a failed sync operation (coming soon)
    Retry {
        /// Bundle ID to retry
        bundle_id: Option<String>,
    },
}

// ── Request/Response models for daemon communication ──────────────

/// Sync status response from the daemon
#[derive(Debug, Deserialize)]
pub struct SyncStatusResponse {
    pub staged_count: u64,
    pub ready_count: u64,
    pub sent_count: u64,
    pub acked_count: u64,
    pub failed_count: u64,
    pub conflicted_count: u64,
    pub last_sync_at: Option<String>,
}

/// Sync push request to the daemon
#[derive(Debug, Serialize)]
pub struct SyncPushRequest {
    pub workspace_id: String,
    pub world_id: String,
    pub creator_id: String,
    pub force: bool,
}

/// Sync push response from the daemon
#[derive(Debug, Deserialize)]
pub struct SyncPushResponse {
    pub success: bool,
    pub outbox_entry_id: Option<String>,
    pub bundle_id: Option<String>,
    pub precheck_result: Option<PrecheckSummaryResponse>,
    pub error: Option<String>,
}

/// Precheck summary from the daemon
#[derive(Debug, Deserialize)]
pub struct PrecheckSummaryResponse {
    pub valid: bool,
    pub error_count: usize,
    pub warning_count: usize,
    pub summary: String,
}

/// Sync resolve request to the daemon
#[derive(Debug, Serialize)]
pub struct SyncResolveRequest {
    pub outbox_entry_id: String,
    pub resolution: String,
    pub force: bool,
}

/// Sync pull response from the daemon (after platform `/v1/sync/pull` + local staging).
#[derive(Debug, Deserialize)]
pub struct SyncPullLocalResponse {
    pub success: bool,
    pub world_revision: u64,
    pub confirmed_delta_sequence: u64,
    pub bundles_received: usize,
    pub entries_staged: Vec<String>,
    pub skipped_known_bundles: usize,
    pub is_up_to_date: Option<bool>,
    pub error: Option<String>,
}

/// Sync resolve response from the daemon
#[derive(Debug, Deserialize)]
pub struct SyncResolveResponse {
    pub success: bool,
    pub delivery_state: Option<String>,
    pub error: Option<String>,
}

/// Sync replay response from the daemon
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SyncReplayResponse {
    pub replayable_count: usize,
    pub entries: Vec<OutboxEntrySummaryResponse>,
}

/// Outbox entry summary from the daemon
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct OutboxEntrySummaryResponse {
    pub outbox_entry_id: String,
    pub bundle_id: String,
    pub delivery_state: String,
    pub retry_count: i64,
    pub last_error: Option<String>,
    pub created_at: String,
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
/// - Daemon is not running
/// - Sync API calls fail
/// - Invalid `world_id` or `creator_id` parameters
///
/// Note: This function is 229 lines; splitting would break the coherent sync flow.
#[allow(clippy::too_many_lines)]
pub async fn run(cmd: SyncCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);

    match cmd {
        SyncCommand::Push {
            workspace_id,
            world_id,
            creator_id,
            force,
        } => {
            runtime_guard::require_platform(&config.runtime_mode(), "sync push")?;
            if !client.health_check().await? {
                return Err(crate::errors::CliError::DaemonNotRunning);
            }

            let mut default_id_fields: Vec<&'static str> = Vec::new();

            let workspace_id = workspace_id.unwrap_or_else(|| {
                default_id_fields.push("workspace_id");
                "local".to_string()
            });

            let world_id = world_id.unwrap_or_else(|| {
                default_id_fields.push("world_id");
                "unknown".to_string()
            });

            let creator_id = creator_id.unwrap_or_else(|| {
                config.active_creator_id.as_deref().map_or_else(
                    || {
                        default_id_fields.push("creator_id");
                        "unknown".to_string()
                    },
                    ToString::to_string,
                )
            });

            if !default_id_fields.is_empty() {
                eprintln!(
                    "Warning: sync push using placeholder IDs (missing {}). \
Real platform sync requires --workspace-id, --world-id, and --creator-id (or active_creator_id in config).",
                    default_id_fields.join(", ")
                );
            }

            let request = SyncPushRequest {
                workspace_id,
                world_id,
                creator_id,
                force,
            };

            match client
                .post::<SyncPushResponse, SyncPushRequest>("/v1/local/sync/push", &request)
                .await
            {
                Ok(response) => {
                    println!("Sync push staged successfully.");
                    if let Some(entry_id) = &response.outbox_entry_id {
                        println!("  Entry ID:  {entry_id}");
                    }
                    if let Some(bundle_id) = &response.bundle_id {
                        println!("  Bundle ID: {bundle_id}");
                    }
                    if let Some(precheck) = &response.precheck_result {
                        if precheck.valid {
                            println!("  Precheck:  PASSED");
                        } else {
                            println!(
                                "  Precheck:  FAILED ({} errors, {} warnings)",
                                precheck.error_count, precheck.warning_count
                            );
                            println!("  {}", precheck.summary);
                        }
                    }
                    if !response.success {
                        if let Some(error) = &response.error {
                            eprintln!("Error: {error}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Sync push failed: {e}");
                    return Err(e);
                }
            }
        }
        SyncCommand::Pull {
            world_id,
            after_sequence,
        } => {
            runtime_guard::require_platform(&config.runtime_mode(), "sync pull")?;
            if !client.health_check().await? {
                return Err(crate::errors::CliError::DaemonNotRunning);
            }

            let world_id = world_id.unwrap_or_else(|| {
                eprintln!(
                    "Warning: sync pull without --world-id uses placeholder \"unknown\". \
            Set --world-id for real platform sync (and ensure it matches workspace sync binding if set)."
                );
                "unknown".to_string()
            });

            let request = SyncPullRequest {
                schema_version: 1,
                world_id,
                after_confirmed_delta_sequence: after_sequence,
            };

            match client
                .post::<SyncPullLocalResponse, SyncPullRequest>("/v1/local/sync/pull", &request)
                .await
            {
                Ok(resp) => {
                    if resp.success {
                        println!("Sync pull completed.");
                        println!("  World revision:           {}", resp.world_revision);
                        println!(
                            "  Confirmed delta sequence: {}",
                            resp.confirmed_delta_sequence
                        );
                        println!("  Bundles received:         {}", resp.bundles_received);
                        println!("  New outbox entries:       {}", resp.entries_staged.len());
                        if resp.skipped_known_bundles > 0 {
                            println!("  Skipped (already local): {}", resp.skipped_known_bundles);
                        }
                        if let Some(up) = resp.is_up_to_date {
                            println!("  Server up-to-date flag:   {up}");
                        }
                        for id in &resp.entries_staged {
                            println!("    - {id}");
                        }
                    } else if let Some(err) = &resp.error {
                        eprintln!("Sync pull failed: {err}");
                    }
                }
                Err(e) => {
                    eprintln!("Sync pull request failed: {e}");
                    return Err(e);
                }
            }
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
                    println!("  Staged:    {}", status.staged_count);
                    println!("  Ready:    {}", status.ready_count);
                    println!("  Sent:      {}", status.sent_count);
                    println!("  Acked:     {}", status.acked_count);
                    println!("  Conflicts: {}", status.conflicted_count);
                    println!("  Failed:    {}", status.failed_count);

                    match &status.last_sync_at {
                        Some(ts) => println!("  Last sync: {ts}"),
                        None => println!("  Last sync: never"),
                    }

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
                }
            }
        }
        SyncCommand::Resolve {
            outbox_entry_id,
            resolution,
            force,
        } => {
            runtime_guard::require_platform(&config.runtime_mode(), "sync resolve")?;
            if !client.health_check().await? {
                return Err(crate::errors::CliError::DaemonNotRunning);
            }

            // Safety prompt for auto-reject (SYNC-R13)
            if resolution == ResolutionStrategy::AutoReject && !confirm_auto_reject(force) {
                println!("Auto-reject cancelled.");
                return Ok(());
            }

            let request = SyncResolveRequest {
                outbox_entry_id: outbox_entry_id.clone(),
                resolution: resolution.to_string(),
                force,
            };

            match client
                .post::<SyncResolveResponse, SyncResolveRequest>("/v1/local/sync/resolve", &request)
                .await
            {
                Ok(response) => {
                    if response.success {
                        println!("Resolved entry {outbox_entry_id} with strategy: {resolution}");
                        if let Some(state) = &response.delivery_state {
                            println!("  New state: {state}");
                        }
                    } else if let Some(error) = &response.error {
                        eprintln!("Resolution failed: {error}");
                        if let Some(state) = &response.delivery_state {
                            eprintln!("  Current state: {state}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Resolve request failed for entry {outbox_entry_id}: {e}");
                    return Err(e);
                }
            }
        }
        SyncCommand::World { command } => {
            super::world::run(command, config).await?;
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_response_deserialization() {
        let json = r#"{
            "staged_count": 2,
            "ready_count": 1,
            "sent_count": 0,
            "acked_count": 5,
            "failed_count": 1,
            "conflicted_count": 3,
            "last_sync_at": "2026-04-07T00:00:00Z"
        }"#;
        let resp: SyncStatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.staged_count, 2);
        assert_eq!(resp.ready_count, 1);
        assert_eq!(resp.acked_count, 5);
        assert_eq!(resp.conflicted_count, 3);
        assert_eq!(resp.failed_count, 1);
        assert_eq!(resp.last_sync_at, Some("2026-04-07T00:00:00Z".to_string()));
    }

    #[test]
    fn test_sync_status_response_no_last_sync() {
        let json = r#"{
            "staged_count": 0,
            "ready_count": 0,
            "sent_count": 0,
            "acked_count": 0,
            "failed_count": 0,
            "conflicted_count": 0,
            "last_sync_at": null
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

    #[test]
    fn test_sync_push_response_deserialization() {
        let json = r#"{
            "success": true,
            "outbox_entry_id": "obe_abc123",
            "bundle_id": "bdl_xyz789",
            "precheck_result": {
                "valid": true,
                "error_count": 0,
                "warning_count": 1,
                "summary": "All checks passed."
            },
            "error": null
        }"#;
        let resp: SyncPushResponse = serde_json::from_str(json).unwrap();
        assert!(resp.success);
        assert_eq!(resp.outbox_entry_id, Some("obe_abc123".to_string()));
        assert_eq!(resp.bundle_id, Some("bdl_xyz789".to_string()));
        assert!(resp.precheck_result.is_some());
        let precheck = resp.precheck_result.unwrap();
        assert!(precheck.valid);
        assert_eq!(precheck.error_count, 0);
    }

    #[test]
    fn test_sync_pull_local_response_deserialization() {
        let json = r#"{
            "success": true,
            "world_revision": 3,
            "confirmed_delta_sequence": 9,
            "bundles_received": 1,
            "entries_staged": ["obe_1"],
            "skipped_known_bundles": 0,
            "is_up_to_date": true,
            "error": null
        }"#;
        let resp: SyncPullLocalResponse = serde_json::from_str(json).unwrap();
        assert!(resp.success);
        assert_eq!(resp.world_revision, 3);
        assert_eq!(resp.entries_staged.len(), 1);
        assert_eq!(resp.skipped_known_bundles, 0);
    }

    #[test]
    fn test_sync_resolve_response_deserialization() {
        let json = r#"{
            "success": true,
            "delivery_state": "acked",
            "error": null
        }"#;
        let resp: SyncResolveResponse = serde_json::from_str(json).unwrap();
        assert!(resp.success);
        assert_eq!(resp.delivery_state, Some("acked".to_string()));
    }
}
