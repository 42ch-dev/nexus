//! Manuscript publish workflow — daemon-mediated platform calls.
//!
//! Requires a running nexus42d and `NEXUS_SYNC_PLATFORM_URL` + `NEXUS_SYNC_PLATFORM_TOKEN`.

use crate::api::DaemonClient;
use crate::commands::context::validate_world_id;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::{
    PublishHistoryRequest, PublishHistoryResponse, PublishStoryRequest, PublishStoryResponse,
};
use serde::{Deserialize, Serialize};

fn validate_manuscript_id(s: &str) -> std::result::Result<String, String> {
    if s.starts_with("mss_") && s.len() > 4 && s[4..].chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "Invalid manuscript id '{s}': expected mss_ followed by alphanumeric characters"
        ))
    }
}

fn validate_story_manifest_id(s: &str) -> std::result::Result<String, String> {
    if s.starts_with("stm_") && s.len() > 4 && s[4..].chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "Invalid story manifest id '{s}': expected stm_ followed by alphanumeric characters"
        ))
    }
}

fn validate_limit(s: &str) -> std::result::Result<i64, String> {
    let n: i64 = s
        .parse()
        .map_err(|_| "limit must be an integer".to_string())?;
    if !(1..=100).contains(&n) {
        return Err("limit must be between 1 and 100".into());
    }
    Ok(n)
}

#[derive(Debug, Subcommand)]
pub enum PublishCommand {
    /// Submit a publish request for a manuscript (POST /v1/publish/story)
    Story {
        #[arg(long, value_parser = validate_world_id)]
        world_id: String,
        #[arg(long, value_parser = validate_manuscript_id)]
        manuscript_id: String,
        #[arg(long, value_parser = validate_story_manifest_id)]
        story_manifest_id: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// List publish history for a manuscript (POST /v1/publish/history)
    History {
        #[arg(long, value_parser = validate_world_id)]
        world_id: String,
        #[arg(long, value_parser = validate_manuscript_id)]
        manuscript_id: String,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, value_parser = validate_limit)]
        limit: Option<i64>,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishStoryLocalResponse {
    pub success: bool,
    pub result: Option<PublishStoryResponse>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishHistoryLocalResponse {
    pub success: bool,
    pub history: Option<PublishHistoryResponse>,
    pub error: Option<String>,
}

fn is_json_output(output_format: &str) -> bool {
    output_format.eq_ignore_ascii_case("json")
}

/// Run publish subcommands
pub async fn run(cmd: PublishCommand, config: &CliConfig, output_format: &str) -> Result<()> {
    let client = DaemonClient::from_config(config);
    let json_out = is_json_output(output_format);

    match cmd {
        PublishCommand::Story {
            world_id,
            manuscript_id,
            story_manifest_id,
            dry_run,
        } => {
            let req = PublishStoryRequest {
                schema_version: 1,
                world_id: world_id.clone(),
                manuscript_id: manuscript_id.clone(),
                story_manifest_id: story_manifest_id.clone(),
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
                .post::<PublishStoryLocalResponse, PublishStoryRequest>(
                    "/v1/local/publish/story",
                    &req,
                )
                .await
            {
                Ok(resp) => {
                    if json_out {
                        println!("{}", serde_json::to_string(&resp).map_err(CliError::Json)?);
                        return Ok(());
                    }
                    if resp.success {
                        if let Some(r) = resp.result {
                            println!("outcome: {}", r.outcome.as_str());
                            if let Some(m) = &r.message {
                                println!("message: {m}");
                            }
                            if let Some(a) = &r.published_artifact_id {
                                println!("published_artifact_id: {a}");
                            }
                            if let Some(e) = &r.error_code {
                                println!("error_code: {e}");
                            }
                        } else {
                            println!("Publish completed (no result body).");
                        }
                    } else if let Some(err) = resp.error {
                        eprintln!("Publish story failed: {}", err);
                    }
                }
                Err(e) => {
                    eprintln!("Publish story request failed: {}", e);
                    return Err(e);
                }
            }
        }
        PublishCommand::History {
            world_id,
            manuscript_id,
            cursor,
            limit,
            dry_run,
        } => {
            let req = PublishHistoryRequest {
                schema_version: 1,
                world_id: world_id.clone(),
                manuscript_id: manuscript_id.clone(),
                cursor,
                limit,
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
                .post::<PublishHistoryLocalResponse, PublishHistoryRequest>(
                    "/v1/local/publish/history",
                    &req,
                )
                .await
            {
                Ok(resp) => {
                    if json_out {
                        println!("{}", serde_json::to_string(&resp).map_err(CliError::Json)?);
                        return Ok(());
                    }
                    if resp.success {
                        if let Some(h) = resp.history {
                            println!("entries: {}, has_more: {}", h.entries.len(), h.has_more);
                            if let Some(c) = &h.next_cursor {
                                println!("next_cursor: {c}");
                            }
                            for (i, e) in h.entries.iter().enumerate() {
                                let line = serde_json::to_string(e).map_err(CliError::Json)?;
                                println!("  [{i}] {line}");
                            }
                        } else {
                            println!("History completed (no body).");
                        }
                    } else if let Some(err) = resp.error {
                        eprintln!("Publish history failed: {}", err);
                    }
                }
                Err(e) => {
                    eprintln!("Publish history request failed: {}", e);
                    return Err(e);
                }
            }
        }
    }

    Ok(())
}
