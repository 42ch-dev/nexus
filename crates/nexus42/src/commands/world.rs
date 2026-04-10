//! World fork and snapshot — daemon-mediated platform calls.
//!
//! Requires a running nexus42d and `NEXUS_SYNC_PLATFORM_URL` + `NEXUS_SYNC_PLATFORM_TOKEN`
//! on the daemon process. Workspace sync binding (if set) must match `parent` / `world-id`.

use crate::api::DaemonClient;
use crate::commands::context::validate_world_id;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::{ForkBranch, WorldForkRequest, WorldSnapshotRequest};
use serde::Deserialize;

/// Validate timeline event id prefix `evt_`.
fn validate_event_id(s: &str) -> std::result::Result<String, String> {
    if s.starts_with("evt_") && s.len() > 4 && s[4..].chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "Invalid event id '{s}': expected evt_ followed by alphanumeric characters"
        ))
    }
}

fn validate_fork_branch_id(s: &str) -> std::result::Result<String, String> {
    if s.starts_with("fbk_") && s.len() > 4 && s[4..].chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "Invalid fork branch id '{s}': expected fbk_ followed by alphanumeric characters"
        ))
    }
}

fn validate_key_block_limit(s: &str) -> std::result::Result<i64, String> {
    let n: i64 = s
        .parse()
        .map_err(|_| "key_block_limit must be an integer".to_string())?;
    if !(1..=500).contains(&n) {
        return Err("key_block_limit must be between 1 and 500".into());
    }
    Ok(n)
}

fn validate_timeline_event_limit(s: &str) -> std::result::Result<i64, String> {
    let n: i64 = s
        .parse()
        .map_err(|_| "timeline_event_limit must be an integer".to_string())?;
    if !(1..=200).contains(&n) {
        return Err("timeline_event_limit must be between 1 and 200".into());
    }
    Ok(n)
}

#[derive(Debug, Subcommand)]
pub enum WorldCommand {
    /// Fork a new world from a parent world at a timeline event (platform API)
    Fork {
        /// Parent (source) world id
        #[arg(long, value_parser = validate_world_id)]
        parent: String,
        /// New world id for the fork
        #[arg(long, value_parser = validate_world_id)]
        child: String,
        /// Timeline event id defining the fork point
        #[arg(long, value_parser = validate_event_id)]
        at_event: String,
        /// Creator id (defaults to active_creator_id from config when set)
        #[arg(long)]
        creator_id: Option<String>,
        /// Print the JSON request and exit without calling the daemon
        #[arg(long)]
        dry_run: bool,
        /// Skip interactive confirmation
        #[arg(long)]
        yes: bool,
        /// Optional label stored with the fork on the platform
        #[arg(long)]
        fork_title: Option<String>,
    },
    /// Request a read-only world snapshot cursor from the platform
    Snapshot {
        #[arg(long, value_parser = validate_world_id)]
        world_id: String,
        /// Optional anchor event (evt_…); omit for server-defined head
        #[arg(long, value_parser = validate_event_id)]
        at_event: Option<String>,
        #[arg(long, value_parser = validate_fork_branch_id)]
        branch_id: Option<String>,
        #[arg(long, value_parser = validate_key_block_limit)]
        key_block_limit: Option<i64>,
        #[arg(long, value_parser = validate_timeline_event_limit)]
        timeline_event_limit: Option<i64>,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Deserialize)]
pub struct WorldForkLocalResponse {
    pub success: bool,
    pub fork_branch: Option<ForkBranch>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorldSnapshotLocalResponse {
    pub success: bool,
    pub world_id: String,
    pub world_revision: u64,
    pub at_event_id: Option<String>,
    pub captured_at: Option<String>,
    pub error: Option<String>,
}

fn confirm_fork(yes: bool) -> bool {
    if yes {
        return true;
    }
    match dialoguer::Confirm::new()
        .with_prompt("Create a new forked world on the platform?")
        .default(false)
        .interact()
    {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Non-interactive terminal: pass --yes to confirm fork.");
            false
        }
    }
}

/// Run world subcommands
pub async fn run(cmd: WorldCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);

    match cmd {
        WorldCommand::Fork {
            parent,
            child,
            at_event,
            creator_id,
            dry_run,
            yes,
            fork_title,
        } => {
            let creator_id = match creator_id {
                Some(s) => s,
                None => match config.active_creator_id.as_deref() {
                    Some(s) => s.to_string(),
                    None => {
                        return Err(CliError::Config(
                            "fork requires --creator-id or active_creator_id in config".into(),
                        ));
                    }
                },
            };

            let req = WorldForkRequest {
                schema_version: 1,
                parent_world_id: Some(parent.clone()),
                child_world_id: Some(child.clone()),
                forked_from_event_id: Some(at_event.clone()),
                created_by_creator_id: Some(creator_id),
                fork_title: fork_title.clone(),
            };

            if dry_run {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&req).map_err(CliError::Json)?
                );
                return Ok(());
            }

            if parent == child {
                return Err(CliError::Config("--child must differ from --parent".into()));
            }

            if !confirm_fork(yes) {
                println!("Fork cancelled.");
                return Ok(());
            }

            if !client.health_check().await? {
                return Err(CliError::DaemonNotRunning);
            }

            match client
                .post::<WorldForkLocalResponse, WorldForkRequest>("/v1/local/world/fork", &req)
                .await
            {
                Ok(resp) => {
                    if resp.success {
                        println!("World fork completed.");
                        if let Some(fb) = resp.fork_branch {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&fb)
                                    .unwrap_or_else(|_| format!("{fb:#?}"))
                            );
                        }
                    } else if let Some(err) = resp.error {
                        eprintln!("World fork failed: {}", err);
                    }
                }
                Err(e) => {
                    eprintln!("World fork request failed: {}", e);
                    return Err(e);
                }
            }
        }
        WorldCommand::Snapshot {
            world_id,
            at_event,
            branch_id,
            key_block_limit,
            timeline_event_limit,
            dry_run,
        } => {
            let req = WorldSnapshotRequest {
                schema_version: 1,
                world_id: world_id.clone(),
                at_event_id: at_event.clone(),
                branch_id: branch_id.clone(),
                key_block_limit,
                timeline_event_limit,
            };

            if dry_run {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&req).map_err(CliError::Json)?
                );
                return Ok(());
            }

            if !client.health_check().await? {
                return Err(CliError::DaemonNotRunning);
            }

            match client
                .post::<WorldSnapshotLocalResponse, WorldSnapshotRequest>(
                    "/v1/local/world/snapshot",
                    &req,
                )
                .await
            {
                Ok(resp) => {
                    if resp.success {
                        println!("World snapshot:");
                        println!("  world_id:         {}", resp.world_id);
                        println!("  world_revision:   {}", resp.world_revision);
                        if let Some(e) = &resp.at_event_id {
                            println!("  at_event_id:      {}", e);
                        }
                        if let Some(c) = &resp.captured_at {
                            println!("  captured_at:      {}", c);
                        }
                    } else if let Some(err) = resp.error {
                        eprintln!("World snapshot failed: {}", err);
                    }
                }
                Err(e) => {
                    eprintln!("World snapshot request failed: {}", e);
                    return Err(e);
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
    fn validate_event_id_accepts_evt() {
        assert!(validate_event_id("evt_abc123").is_ok());
    }

    #[test]
    fn validate_event_id_rejects_bad_prefix() {
        assert!(validate_event_id("wld_abc").is_err());
    }

    #[test]
    fn world_fork_local_response_deser() {
        let j = r#"{"success":false,"error":"nope"}"#;
        let r: WorldForkLocalResponse = serde_json::from_str(j).unwrap();
        assert!(!r.success);
        assert_eq!(r.error.as_deref(), Some("nope"));
    }
}
