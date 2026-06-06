//! `nexus42 creator run` — Work lifecycle CLI (V1.33 §7.3, V1.34 FL-E §3).
//!
//! Subcommands:
//! - `start` — Create a new Work and run the initial preset
//! - `continue` — Append inspiration / direction to an existing Work
//! - `list` — List all Works for the active creator
//! - `status` — Show details of a single Work
//! - `stage` — FL-E stage management (V1.34): list, advance

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_contracts::local::orchestration::{stage_index, FL_E_STAGES};
use nexus_contracts::local::schedule::http::AddScheduleRequest;
use nexus_orchestration::stage_gates::{self, WorkFields, WorkStageState};

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
        /// After intake completes, automatically chain into the production
        /// stage (C-V133P2-03). Default true; pass --chain-novel-writing=false
        /// to opt out and control stage advance manually.
        #[arg(long, default_value_t = true)]
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
    /// FL-E stage management (V1.34): list stages, advance stage
    Stage {
        #[command(subcommand)]
        command: StageCommand,
    },
}

/// FL-E stage subcommands (V1.34 cli-spec §6.2E).
#[derive(Debug, Subcommand)]
pub enum StageCommand {
    /// List FL-E stages and current status for a Work
    List {
        /// Work ID (wrk_...)
        work_id: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Advance a Work to the next FL-E stage
    Advance {
        /// Work ID (wrk_...)
        work_id: String,
        /// Target stage: research | produce | review | persist
        #[arg(long)]
        stage: String,
        /// Force advance even if current stage is not complete (audited)
        #[arg(long, default_value_t = false)]
        force: bool,
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
                let intake_request = AddScheduleRequest {
                    creator_id: String::new(),
                    preset_id: "creative-brief-intake".to_string(),
                    seed: Some(idea.clone()),
                    label: None,
                    depends_on: None,
                    concurrency: None,
                    scheduled_at: None,
                };

                match client
                    .post::<serde_json::Value, _>(
                        "/v1/local/orchestration/schedules",
                        &intake_request,
                    )
                    .await
                {
                    Ok(sched_resp) => {
                        schedule_id = sched_resp
                            .get("schedule_id")
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
                let novel_request = AddScheduleRequest {
                    creator_id: String::new(),
                    preset_id: production_preset.to_string(),
                    seed: Some(idea.clone()),
                    label: None,
                    depends_on: None,
                    concurrency: None,
                    scheduled_at: None,
                };

                match client
                    .post::<serde_json::Value, _>(
                        "/v1/local/orchestration/schedules",
                        &novel_request,
                    )
                    .await
                {
                    Ok(sched_resp) => {
                        novel_schedule_id = sched_resp
                            .get("schedule_id")
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
                    // V1.35 P4: default chain_novel_writing=true; both paths
                    // hint the FL-E stage advance command. Non-chain users
                    // will see the same hint but can choose not to follow it.
                    println!("Once intake completes, advance to production with:");
                    println!("  nexus42 creator run stage advance {work_id} --stage produce");
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
                    ("current_stage", "current_stage"),
                    ("stage_status", "stage_status"),
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
        RunCommand::Stage { command } => handle_stage(command, config, &client).await?,
    }

    Ok(())
}

// ── FL-E stage management (V1.34) ───────────────────────────────────────────

/// Handle `creator run stage` subcommands (V1.34 FL-E §3, cli-spec §6.2E).
///
/// # Errors
///
/// Returns an error if the daemon API call fails or stage validation rejects the advance.
async fn handle_stage(
    cmd: StageCommand,
    config: &CliConfig,
    client: &crate::api::DaemonClient,
) -> Result<()> {
    match cmd {
        StageCommand::List { work_id, json } => stage_list(&work_id, json, client).await,
        StageCommand::Advance {
            work_id,
            stage,
            force,
            json,
        } => stage_advance(&work_id, &stage, force, json, config, client).await,
    }
}

/// List FL-E stages and current status for a Work.
///
/// Fetches the Work from the daemon and displays all stages with
/// markers for the current stage and status.
async fn stage_list(work_id: &str, json: bool, client: &crate::api::DaemonClient) -> Result<()> {
    let resp: serde_json::Value = client
        .get::<serde_json::Value>(&format!("/v1/local/works/{work_id}"))
        .await?;

    let current_stage = resp
        .get("current_stage")
        .and_then(|v| v.as_str())
        .unwrap_or("intake");
    let stage_status = resp
        .get("stage_status")
        .and_then(|v| v.as_str())
        .unwrap_or("pending");

    if json {
        let output = serde_json::json!({
            "work_id": work_id,
            "current_stage": current_stage,
            "stage_status": stage_status,
            "stages": FL_E_STAGES,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("FL-E stages for Work {work_id}:");
        println!();
        for &s in FL_E_STAGES {
            let marker = if s == current_stage {
                format!("→ {s}")
            } else {
                format!("  {s}")
            };
            let status_label = if s == current_stage {
                format!("({stage_status})")
            } else if let Some(idx) = FL_E_STAGES.iter().position(|&x| x == current_stage) {
                let stage_idx = FL_E_STAGES.iter().position(|&x| x == s).unwrap_or(0);
                if stage_idx < idx {
                    "(complete)".to_string()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            println!("{marker:<20} {status_label}");
        }
        println!();
        println!("Current: {current_stage} ({stage_status}) — Work {work_id}");
    }

    Ok(())
}

/// Advance a Work to the next FL-E stage.
///
/// Validates:
/// 1. Target stage is a known FL-E stage
/// 2. Target stage is ahead of current stage (unless `--force`)
/// 3. Current `stage_status` is `complete` (unless `--force`)
///
/// Then `PATCH`es the work via daemon API with the new stage/status.
#[allow(clippy::too_many_lines)]
async fn stage_advance(
    work_id: &str,
    target_stage: &str,
    force: bool,
    json: bool,
    config: &CliConfig,
    client: &crate::api::DaemonClient,
) -> Result<()> {
    // Fetch current work state
    let resp: serde_json::Value = client
        .get::<serde_json::Value>(&format!("/v1/local/works/{work_id}"))
        .await?;

    let current_stage = resp
        .get("current_stage")
        .and_then(|v| v.as_str())
        .unwrap_or("intake");
    let current_status = resp
        .get("stage_status")
        .and_then(|v| v.as_str())
        .unwrap_or("pending");
    // V1.33 intake_status field — needed for intake gate (spec §3.3 gate 1).
    let intake_status = resp
        .get("intake_status")
        .and_then(|v| v.as_str())
        .unwrap_or("pending");

    let current_idx = stage_index(current_stage).unwrap_or(0);
    let target_idx = stage_index(target_stage).unwrap_or(0);

    // Shared gate validation (V1.34 creator-workflow §3.3)
    // Uses the same function as daemon PATCH stage path.
    let work_state = WorkStageState {
        current_stage: current_stage.to_string(),
        stage_status: current_status.to_string(),
        intake_status: intake_status.to_string(),
    };
    stage_gates::check_stage_advance(&work_state, target_stage, force)
        .map_err(|e| crate::errors::CliError::Other(format!("{}: {}", e.code, e.message)))?;

    // PATCH the work with new stage
    let patch = serde_json::json!({
        "current_stage": target_stage,
        "stage_status": "active",
    });

    let updated: serde_json::Value = client
        .patch::<serde_json::Value, _>(&format!("/v1/local/works/{work_id}"), &patch)
        .await?;

    // Audit log for --force usage (spec §3.1: "audited").
    // Structured log with target "fl_e.audit" for all force-triggered stage skips.
    if force {
        tracing::info!(
            target: "fl_e.audit",
            work_id = %work_id,
            from_stage = %current_stage,
            to_stage = %target_stage,
            from_status = %current_status,
            force = true,
            "FL-E stage advance forced (skipped gate)"
        );
    }

    // Create an FL-E stage schedule (spec §2 invariant #4, §5.3).
    // Uses the shared facade `build_schedule_for_stage` to construct a
    // correctly-shaped AddScheduleRequest (R-FL-E-P2-03).
    //
    // R-FL-E-P2-05: creator_id comes from CLI config's active_creator_id,
    // NOT from WorkApiDto (SEC-V131-01 omits creator_id from Work responses).
    let mut schedule_id: Option<String> = None;
    let preset_id = stage_gates::preset_for_stage(target_stage);

    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(crate::errors::CliError::CreatorNotSelected)?;

    // Build Work fields for the schedule request
    let creative_brief = resp
        .get("creative_brief")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let inspiration_log = resp
        .get("inspiration_log")
        .and_then(|v| v.as_str())
        .unwrap_or("[]");

    let fields = WorkFields {
        work_id: work_id.to_string(),
        fl_e_stage: target_stage.to_string(),
        creative_brief: creative_brief.to_string(),
        inspiration_log: inspiration_log.to_string(),
    };

    if let Some(request) = stage_gates::build_schedule_for_stage(target_stage, creator_id, &fields)
    {
        let pid = &request.preset_id;

        // Audit log before schedule creation attempt.
        tracing::info!(
            target: "fl_e.audit",
            work_id = %work_id,
            stage = %target_stage,
            preset_id = %pid,
            creator_id = %creator_id,
            "FL-E stage schedule creation requested"
        );

        match client
            .post::<serde_json::Value, _>("/v1/local/orchestration/schedules", &request)
            .await
        {
            Ok(sched_resp) => {
                schedule_id = sched_resp
                    .get("schedule_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                tracing::info!(
                    target: "fl_e.audit",
                    work_id = %work_id,
                    stage = %target_stage,
                    preset_id = %pid,
                    schedule_id = %schedule_id.as_deref().unwrap_or("?"),
                    "FL-E stage schedule created"
                );
            }
            Err(e) => {
                // Schedule creation failure is blocking — rollback the stage advance
                // so the Work does not end up in 'active' without a driver.
                tracing::error!(
                    target: "fl_e.audit",
                    work_id = %work_id,
                    stage = %target_stage,
                    error = %e,
                    "FL-E stage schedule creation failed; rolling back stage advance"
                );

                // Attempt to restore previous stage state
                let rollback = serde_json::json!({
                    "current_stage": current_stage,
                    "stage_status": current_status,
                });
                let _ = client
                    .patch::<serde_json::Value, _>(&format!("/v1/local/works/{work_id}"), &rollback)
                    .await;

                return Err(crate::errors::CliError::Other(format!(
                    "FL_E_SCHEDULE_CREATE_FAILED: failed to create stage schedule for '{target_stage}': {e}. \
                     Stage advance rolled back to '{current_stage}' ({current_status})."
                )));
            }
        }
    }

    if json {
        let mut output = updated;
        if let Some(sid) = &schedule_id {
            output.as_object_mut().map(|o| {
                o.insert(
                    "stage_schedule_id".to_string(),
                    serde_json::Value::String(sid.clone()),
                )
            });
        }
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let new_stage = updated
            .get("current_stage")
            .and_then(|v| v.as_str())
            .unwrap_or(target_stage);
        let new_status = updated
            .get("stage_status")
            .and_then(|v| v.as_str())
            .unwrap_or("active");
        let title = updated.get("title").and_then(|v| v.as_str()).unwrap_or("?");

        if force {
            let reason = if target_idx <= current_idx {
                "out of order"
            } else {
                "gate bypass"
            };
            println!(
                "Warning: --force used to advance from '{current_stage}' to '{target_stage}' \
                 ({reason})"
            );
        }
        println!("Work '{title}' advanced to stage: {new_stage} ({new_status})");
        println!("  Work ID: {work_id}");

        if let Some(sid) = &schedule_id {
            let pid = preset_id.unwrap_or("(unknown)");
            println!("  Stage schedule: {sid} (preset: {pid})");
        }

        if let Some(pid) = preset_id {
            println!("  Typical preset: {pid}");
        }
    }

    Ok(())
}
