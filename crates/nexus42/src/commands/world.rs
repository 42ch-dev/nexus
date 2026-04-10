//! World fork and snapshot — daemon-mediated platform calls.
//!
//! Requires a running nexus42d and `NEXUS_SYNC_PLATFORM_URL` + `NEXUS_SYNC_PLATFORM_TOKEN`
//! on the daemon process. Workspace sync binding (if set) must match `parent` / `world-id`.

use crate::api::DaemonClient;
use crate::commands::context::validate_world_id;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::{WorldForkRequest, WorldSnapshotRequest};
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
    },
    /// Request a read-only world snapshot cursor from the platform
    Snapshot {
        #[arg(long, value_parser = validate_world_id)]
        world_id: String,
        /// Optional anchor event (evt_…); omit for server-defined head
        #[arg(long, value_parser = validate_event_id)]
        at_event: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Deserialize)]
pub struct WorldForkLocalResponse {
    pub success: bool,
    pub fork_branch: Option<serde_json::Value>,
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
                parent_world_id: parent.clone(),
                child_world_id: child.clone(),
                forked_from_event_id: at_event.clone(),
                created_by_creator_id: creator_id,
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
                            match serde_json::to_string_pretty(&fb) {
                                Ok(s) => println!("{}", s),
                                Err(_) => println!("{}", fb),
                            }
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
            dry_run,
        } => {
            let req = WorldSnapshotRequest {
                schema_version: 1,
                world_id: world_id.clone(),
                at_event_id: at_event.clone(),
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
