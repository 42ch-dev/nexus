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
        /// Skip the creative brief intake and start the production preset directly
        #[arg(long, default_value_t = false)]
        skip_intake: bool,
        /// After intake completes, automatically schedule the novel-writing
        /// production preset (C-V133P2-03: chains intake → novel-writing).
        #[arg(long, default_value_t = false)]
        chain_novel_writing: bool,
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
#[allow(clippy::missing_panics_doc)] // expect on constant URL string; never panics
pub async fn handle_run(cmd: RunCommand, config: &CliConfig) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);

    match cmd {
        RunCommand::Start {
            idea,
            preset,
            title,
            world_id,
            skip_intake,
            chain_novel_writing,
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

            let work_id = resp
                .get("work_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();

            // Schedule intake preset if not skipped
            let mut schedule_id: Option<String> = None;
            if !skip_intake {
                let intake_body = serde_json::json!({
                    "presetId": "creative-brief-intake",
                    "seed": &idea,
                    "presetInput": {
                        "work_id": &work_id,
                        "initial_idea": &idea,
                    }
                });

                match client
                    .post::<serde_json::Value, _>("/v1/local/orchestration/schedules", &intake_body)
                    .await
                {
                    Ok(sched_resp) => {
                        schedule_id = sched_resp
                            .get("scheduleId")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    Err(e) => {
                        // Schedule creation failure is non-fatal — the Work is
                        // still created. Report the error but don't abort.
                        eprintln!("Warning: failed to schedule intake: {e}");
                    }
                }
            }

            // C-V133P2-03: auto-chain novel-writing after intake.
            // When --chain-novel-writing is set:
            //   - If intake was skipped: schedule novel-writing directly.
            //   - If intake ran: the follow-up novel-writing command is printed
            //     for the user to run after intake completes.
            //     The daemon does not yet support on_complete hooks for
            //     auto-scheduling follow-up presets (see note below).
            //
            // NOTE: Full daemon-side auto-chaining (on_complete trigger) is a
            // future enhancement. For V1.33, the CLI side provides explicit
            // chaining via --chain-novel-writing which either schedules
            // directly (skip-intake) or documents the follow-up command.
            let mut novel_schedule_id: Option<String> = None;
            if chain_novel_writing && skip_intake {
                // Intake skipped → schedule novel-writing directly.
                let production_preset = preset.as_deref().unwrap_or("novel-writing");
                let novel_body = serde_json::json!({
                    "presetId": production_preset,
                    "seed": &idea,
                    "presetInput": {
                        "work_id": &work_id,
                    }
                });

                match client
                    .post::<serde_json::Value, _>("/v1/local/orchestration/schedules", &novel_body)
                    .await
                {
                    Ok(sched_resp) => {
                        novel_schedule_id = sched_resp
                            .get("scheduleId")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to schedule production: {e}");
                    }
                }
            }

            if json {
                let mut output = resp;
                if let Some(sid) = &schedule_id {
                    output.as_object_mut().map(|o| {
                        o.insert(
                            "intake_schedule_id".to_string(),
                            serde_json::Value::String(sid.clone()),
                        )
                    });
                }
                if let Some(nid) = &novel_schedule_id {
                    output.as_object_mut().map(|o| {
                        o.insert(
                            "production_schedule_id".to_string(),
                            serde_json::Value::String(nid.clone()),
                        )
                    });
                }
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                println!("Work created: {work_id} (status: {status})");
                if let Some(sid) = &schedule_id {
                    println!("Intake scheduled: {sid} (preset: creative-brief-intake)");
                    println!();
                    println!("The intake will run via ACP multi-turn conversation.");
                    if chain_novel_writing {
                        // C-V133P2-03: chain → novel-writing after intake.
                        println!("Once intake completes, production will be ready:");
                        let production_preset = preset.as_deref().unwrap_or("novel-writing");
                        println!(
                            "  nexus42 daemon schedule add --preset {production_preset} \
                             --creator <creator-id> --seed \"{idea}\""
                        );
                    } else {
                        println!("Once intake completes, start production with:");
                        println!(
                            "  nexus42 daemon schedule add --preset novel-writing \
                             --creator <creator-id> --seed \"<topic>\""
                        );
                    }
                } else if let Some(nid) = &novel_schedule_id {
                    // Intake skipped, novel-writing scheduled directly.
                    let production_preset = preset.as_deref().unwrap_or("novel-writing");
                    println!(
                        "Production scheduled: {nid} (preset: {production_preset}, \
                         intake skipped)"
                    );
                }
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
            // R-V133P1-07: build query via url::Url to properly encode the
            // status filter value, preventing query-string injection.
            let base = "/v1/local/works";
            let path = status.as_ref().map_or_else(
                || base.to_string(),
                |s| {
                    let mut url = url::Url::parse("http://localhost").expect("valid base");
                    url.set_path(base);
                    url.query_pairs_mut().append_pair("status", s);
                    let q = url.query().unwrap_or("");
                    format!("{base}?{q}")
                },
            );

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
