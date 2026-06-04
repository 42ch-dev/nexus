//! `nexus42 creator run` — Work lifecycle CLI (V1.33 §7.3).
//!
//! Subcommands:
//! - `start` — Create a new Work and run the initial preset
//! - `continue` — Append inspiration / direction to an existing Work
//! - `list` — List all Works for the active creator
//! - `status` — Show details of a single Work

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum RunCommand {
    /// Start a new Work and run the initial preset
    Start {
        /// Initial creative idea (one or more sentences)
        #[arg(long)]
        idea: String,
        /// Override the primary production preset (default: derived from policy)
        #[arg(long)]
        preset: Option<String>,
        /// Optional title for the work
        #[arg(long)]
        title: Option<String>,
        /// Optional world binding
        #[arg(long)]
        world_id: Option<String>,
        /// Idempotency key (UUID); repeat calls with same key return same `work_id`
        #[arg(long)]
        client_request_id: Option<String>,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Append inspiration / direction to an existing Work
    Continue {
        /// The Work id (wrk_...) to continue
        work_id: String,
        /// New inspiration / direction note
        #[arg(long)]
        note: String,
        /// Optional preset to run (default: same `primary_preset_id`)
        #[arg(long)]
        preset: Option<String>,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List all Works for the active creator
    List {
        /// Filter by status
        #[arg(long)]
        status: Option<String>,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show details of a single Work
    Status {
        work_id: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Run the `creator run` command.
///
/// # Errors
///
/// Returns an error if the daemon API call fails.
#[allow(clippy::too_many_lines)]
pub async fn handle_run(cmd: RunCommand, config: &CliConfig) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);

    match cmd {
        RunCommand::Start {
            idea,
            preset,
            title,
            world_id,
            client_request_id,
            json,
        } => {
            let work_title = title.unwrap_or_else(|| {
                let max_len = idea.chars().take(60).collect::<String>();
                if idea.len() > max_len.len() {
                    format!("{max_len}...")
                } else {
                    max_len
                }
            });

            let body = serde_json::json!({
                "title": work_title,
                "long_term_goal": "Complete creative work",
                "initial_idea": idea,
                "primary_preset_id": preset,
                "world_id": world_id,
                "client_request_id": client_request_id,
            });
            // Remove null fields
            let body = body
                .as_object()
                .map(|obj| {
                    obj.iter()
                        .filter(|(_, v)| !v.is_null())
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect::<serde_json::Map<String, serde_json::Value>>()
                })
                .map(serde_json::Value::Object)
                .unwrap_or(body);

            let resp: serde_json::Value = client
                .post::<serde_json::Value, _>("/v1/local/works", &body)
                .await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&resp)?);
            } else {
                let work_id = resp.get("work_id").and_then(|v| v.as_str()).unwrap_or("?");
                let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                println!("Work created: {work_id} (status: {status})");
                println!();
                println!("Next: nexus42 creator run continue {work_id} --note \"<direction>\"");
            }
        }
        RunCommand::Continue {
            work_id,
            note,
            preset: _preset,
            json,
        } => {
            let body = serde_json::json!({ "note": note });
            let resp: serde_json::Value = client
                .post::<serde_json::Value, _>(
                    &format!("/v1/local/works/{work_id}/inspiration"),
                    &body,
                )
                .await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&resp)?);
            } else {
                println!("Inspiration appended to {work_id}");
            }
        }
        RunCommand::List { status, json } => {
            let mut path = "/v1/local/works".to_string();
            if let Some(ref s) = status {
                path = format!("{path}?status={s}");
            }

            let resp: serde_json::Value = client.get::<serde_json::Value>(&path).await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&resp)?);
            } else {
                let works = resp.get("works").and_then(|v| v.as_array());
                match works {
                    Some(works) if works.is_empty() => {
                        println!("No works found.");
                    }
                    Some(works) => {
                        println!(
                            "{:<36} {:30} {:12} {:12} UPDATED",
                            "WORK_ID", "TITLE", "STATUS", "INTAKE"
                        );
                        for w in works {
                            let id = w.get("work_id").and_then(|v| v.as_str()).unwrap_or("?");
                            let title = w.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                            let status = w.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                            let intake = w
                                .get("intake_status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            let updated =
                                w.get("updated_at").and_then(|v| v.as_str()).unwrap_or("?");
                            let display_title = if title.len() > 28 {
                                format!("{}…", &title[..28])
                            } else {
                                title.to_string()
                            };
                            println!(
                                "{id:<36} {display_title:30} {status:12} {intake:12} {updated}"
                            );
                        }
                        println!("\n{} work(s)", works.len());
                    }
                    None => {
                        println!("No works found.");
                    }
                }
            }
        }
        RunCommand::Status { work_id, json } => {
            let resp: serde_json::Value = client
                .get::<serde_json::Value>(&format!("/v1/local/works/{work_id}"))
                .await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&resp)?);
            } else {
                // Key-value dump
                let fields = [
                    ("work_id", "work_id"),
                    ("title", "title"),
                    ("status", "status"),
                    ("intake_status", "intake_status"),
                    ("long_term_goal", "long_term_goal"),
                    ("initial_idea", "initial_idea"),
                    ("primary_preset_id", "primary_preset_id"),
                    ("world_id", "world_id"),
                    ("story_ref", "story_ref"),
                    ("created_at", "created_at"),
                    ("updated_at", "updated_at"),
                ];
                for (label, key) in &fields {
                    let val = resp
                        .get(key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("(not set)");
                    println!("{label:>20}: {val}");
                }
            }
        }
    }

    Ok(())
}
