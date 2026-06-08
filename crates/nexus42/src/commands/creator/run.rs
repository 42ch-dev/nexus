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
    /// Start a new Work and run the initial preset.
    ///
    /// When all chapters of a novel Work are finalized, the daemon auto-promotes
    /// the Work to "completed" (V1.36 §6). Further `novel-writing` schedule
    /// creation is rejected until a new Work is started via this command.
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
        /// Optional world binding (V1.36 §3.5; passes through to Work)
        #[arg(long)]
        world_id: Option<String>,
        /// Run an init preset before production (V1.36 §5.4)
        /// Accepts: novel-project-init
        #[arg(long)]
        init_preset: Option<String>,
        /// Skip the creative brief intake and start the production preset directly
        #[arg(long, default_value_t = false)]
        skip_intake: bool,
        /// After intake completes, print the next-stage command for the user
        /// to run manually (C-V133P2-03 partial). When `--skip-intake` is also
        /// set, scheduling of the production preset happens directly instead.
        /// Default true. Opt-out syntax: `--chain-novel-writing=false`. Full
        /// daemon `on_complete` auto-chain is a future enhancement (DF-53 partial).
        #[arg(long, default_value_t = true, value_parser = clap::builder::BoolishValueParser::new(), action = clap::ArgAction::Set)]
        chain_novel_writing: bool,
        /// Disable daemon-side auto-chain for this Work (V1.39 §5.4).
        /// When set, the daemon will NOT automatically advance FL-E stages
        /// or loop chapters after each stage completes. Manual stage advance
        /// via `creator run stage advance` is still available.
        /// Default: auto-chain enabled (--no-auto-chain opts out).
        #[arg(long, default_value_t = false)]
        no_auto_chain: bool,
        /// Force gate bypass with audit reason (V1.36 §5.3.5)
        /// Requires --reason to be set alongside
        #[arg(long, default_value_t = false)]
        force_gates: bool,
        /// Audit reason for --force-gates (required when --force-gates is set)
        #[arg(long)]
        reason: Option<String>,
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
    /// Rebuild `work_chapters` from filesystem (V1.36 §4.1.2, §8)
    ReconcileChapters {
        /// Work ID (wrk_...) to reconcile
        work_id: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Resume an auto-chain Work whose driver was interrupted (V1.39 §5.7).
    ///
    /// Re-evaluates the current Work state and enqueues the next auto-chain
    /// step (stage advance or next chapter) if applicable.
    Resume {
        /// Work ID (wrk_...) to resume
        work_id: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
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
        /// Force gate bypass with audit reason (V1.37 §7.9)
        /// Requires --gate-reason to be set alongside
        #[arg(long, default_value_t = false)]
        force_gates: bool,
        /// Audit reason for --force-gates (required when --force-gates is set)
        #[arg(long)]
        gate_reason: Option<String>,
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
            init_preset,
            skip_intake,
            chain_novel_writing,
            no_auto_chain,
            force_gates,
            reason,
            client_request_id,
            json,
        } => {
            // Validate --force-gates requires --reason
            if force_gates && reason.is_none() {
                return Err(crate::errors::CliError::Config(
                    "--force-gates requires --reason \"<text>\" (audit-logged)".to_string(),
                ));
            }
            // W-5: Cap and sanitize reason
            if let Some(ref r) = reason {
                if r.len() > 512 {
                    return Err(crate::errors::CliError::Config(format!(
                        "--reason exceeds maximum length (512 chars); got {} chars",
                        r.len()
                    )));
                }
                if r.contains('\x1b') || r.chars().any(|c| c.is_control() && c != '\n') {
                    return Err(crate::errors::CliError::Config(
                        "--reason contains ANSI escape sequences or control characters".to_string(),
                    ));
                }
            }

            // F7 (V1.36 P1, R-V136P1-01 resolved in V1.37): resolve active creator
            // once and populate AddScheduleRequest.creator_id for every schedule
            // we create below.
            //
            // V1.37 (R-V136P1-01): the `--init-preset` flow now threads grill-me
            // output (work_ref / total_planned_chapters / world_id) into
            // `preset.input.*` via the `input` field on AddScheduleRequest.
            let resolved_creator_id = config
                .active_creator_id
                .clone()
                .ok_or(crate::errors::CliError::CreatorNotSelected)?;

            let work_title = title.unwrap_or_else(|| {
                let max_len = idea.chars().take(60).collect::<String>();
                if idea.len() > max_len.len() {
                    format!("{max_len}...")
                } else {
                    max_len
                }
            });

            let mut body = serde_json::json!({
                "title": work_title,
                "long_term_goal": "Complete creative work",
                "initial_idea": idea,
                "primary_preset_id": preset,
                "world_id": world_id,
                "client_request_id": client_request_id,
            });

            // V1.36: pass init_preset through to the Work/schedule payload
            if let Some(ref ip) = init_preset {
                if let Some(o) = body.as_object_mut() {
                    o.insert(
                        "init_preset".to_string(),
                        serde_json::Value::String(ip.clone()),
                    );
                }
            }

            // V1.36: pass force_gates + reason through to Work creation body
            // (the force_gates flag also flows via AddScheduleRequest for
            // schedule-level gate evaluation at the daemon handler).
            if force_gates {
                if let Some(o) = body.as_object_mut() {
                    o.insert("force_gates".to_string(), serde_json::Value::Bool(true));
                    o.insert(
                        "force_gates_reason".to_string(),
                        serde_json::Value::String(reason.clone().unwrap_or_default()),
                    );
                }
            }

            // V1.39 §5.4: pass auto_chain_enabled through to Work creation.
            // Default is true (auto-chain active); --no-auto-chain opts out.
            if no_auto_chain {
                if let Some(o) = body.as_object_mut() {
                    o.insert(
                        "auto_chain_enabled".to_string(),
                        serde_json::Value::Bool(false),
                    );
                }
            }

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

            // V1.36: Schedule init preset if requested (before intake)
            let mut init_schedule_id: Option<String> = None;
            if let Some(ref ip) = init_preset {
                // V1.37 (R-V136P1-01): build structured input map from CLI flags
                // and work creation response so grill-me answers reach
                // preset.input.* for scaffold and prompt rendering.
                let init_input = serde_json::json!({
                    "work_id": work_id,
                    "work_ref": work_title.to_lowercase().replace(' ', "-"),
                    "title": work_title,
                    "total_planned_chapters": 1,
                    "world_id": world_id,
                });
                let init_request = AddScheduleRequest {
                    creator_id: resolved_creator_id.clone(),
                    preset_id: ip.clone(),
                    seed: Some(idea.clone()),
                    label: None,
                    depends_on: None,
                    concurrency: None,
                    scheduled_at: None,
                    input: Some(init_input),
                    force_gates,
                    reason: reason.clone(),
                };

                match client
                    .post::<serde_json::Value, _>(
                        "/v1/local/orchestration/schedules",
                        &init_request,
                    )
                    .await
                {
                    Ok(sched_resp) => {
                        init_schedule_id = sched_resp
                            .get("schedule_id")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to schedule init preset: {e}");
                    }
                }
            }

            // Schedule intake preset if not skipped
            let mut schedule_id: Option<String> = None;
            if !skip_intake {
                let intake_request = AddScheduleRequest {
                    creator_id: resolved_creator_id.clone(),
                    preset_id: "creative-brief-intake".to_string(),
                    seed: Some(idea.clone()),
                    label: None,
                    depends_on: None,
                    concurrency: None,
                    scheduled_at: None,
                    input: None,
                    force_gates: false,
                    reason: None,
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
                // V1.38 P0 (T4): include chapter input for multi-chapter selection.
                // Default to chapter 1 for the Start path (first run).
                let novel_input = serde_json::json!({
                    "work_id": work_id,
                    "work_ref": work_title.to_lowercase().replace(' ', "-"),
                    "topic": idea,
                    "vibe": "literary",
                    "chapter": 1,
                });
                let production_preset = preset.as_deref().unwrap_or("novel-writing");
                let novel_request = AddScheduleRequest {
                    creator_id: resolved_creator_id.clone(),
                    preset_id: production_preset.to_string(),
                    seed: Some(idea.clone()),
                    label: None,
                    depends_on: None,
                    concurrency: None,
                    scheduled_at: None,
                    input: Some(novel_input),
                    force_gates,
                    reason: reason.clone(),
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
                if let Some(iid) = &init_schedule_id {
                    output.as_object_mut().map(|o| {
                        o.insert(
                            "init_schedule_id".to_string(),
                            serde_json::Value::String(iid.clone()),
                        )
                    });
                }
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
                if let Some(iid) = &init_schedule_id {
                    println!("Init preset scheduled: {iid} (preset: {init_preset:?})");
                    println!();
                    println!(
                        "The init preset will bootstrap your Work's scaffold via ACP conversation."
                    );
                }
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
                let work_status = resp
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(not set)");
                let title = resp
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(untitled)");
                let work_profile = resp.get("work_profile").and_then(|v| v.as_str());
                let work_ref = resp
                    .get("work_ref")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no ref)");
                let intake_status = resp
                    .get("intake_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(not set)");
                let current_chapter = resp
                    .get("current_chapter")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                let total_planned = resp
                    .get("total_planned_chapters")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                let chapters = resp.get("chapters").and_then(|v| v.as_array());
                let next_ch_val = resp.get("next_chapter").and_then(serde_json::Value::as_i64);

                // V1.38 P0 (T8): per-chapter status UX per spec §8.1.
                // For novel profile works, show chapter-centric output.
                if let (Some("novel"), Some(ch_list)) = (work_profile, chapters) {
                    let finalized_count = ch_list
                        .iter()
                        .filter(|c| c.get("status").and_then(|v| v.as_str()) == Some("finalized"))
                        .count();
                    let total = ch_list.len();

                    let profile_tag = " (novel)".to_string();

                    if work_status == "completed" {
                        let updated_at = resp
                            .get("updated_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or("(unknown)");
                        println!("═══════════════════════════════════════════════════════");
                        println!("  \"{title}\" — Work {work_id}{profile_tag}");
                        println!("  COMPLETED at {updated_at}");
                        println!("  {total}/{total} chapters finalized.");
                        println!("  No further novel-writing schedules will be enqueued.");
                        println!();
                        println!("  To start a new Work, run:");
                        println!("    nexus42 creator run start \\");
                        println!("      --init-preset novel-project-init --idea \"...\"");
                        println!("═══════════════════════════════════════════════════════");
                    } else {
                        // Header
                        println!("Work: {work_id} — {title}{profile_tag}");
                        println!("work_ref: {work_ref}");
                        println!("intake: {intake_status}");
                        println!("progress: {finalized_count} / {total} chapters finalized");
                        println!("current_chapter: {current_chapter}");
                        println!("total_planned_chapters: {total_planned}");

                        // V1.39 T7: auto-chain checkpoint fields
                        let auto_chain = resp
                            .get("auto_chain_enabled")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(true);
                        let driver = resp
                            .get("driver_schedule_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("none");
                        let interrupted = resp
                            .get("auto_chain_interrupted")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        println!(
                            "auto_chain: {} (driver: {}, interrupted: {})",
                            if auto_chain { "enabled" } else { "disabled" },
                            driver,
                            interrupted
                        );
                        println!();

                        // Per-chapter rows
                        println!("Chapters:");
                        for ch in ch_list {
                            let ch_num = ch
                                .get("chapter")
                                .and_then(serde_json::Value::as_i64)
                                .unwrap_or(0);
                            let ch_status =
                                ch.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                            let words = ch
                                .get("actual_word_count")
                                .and_then(serde_json::Value::as_i64);
                            let body_path =
                                ch.get("body_path").and_then(|v| v.as_str()).unwrap_or("—");
                            let words_str = match words {
                                Some(w) if w > 0 => format!("{w}"),
                                _ => "—".to_string(),
                            };
                            println!(
                                "  ch{ch_num:02}  {ch_status:<12} words: {words_str:<8} path: {body_path}"
                            );
                        }

                        // T9: blocked/missing-file hints.
                        // Surface warnings for chapters whose body_path might be
                        // missing on disk. The CLI cannot check the filesystem
                        // directly here (no workspace root context), but the
                        // daemon reconcile-chapters operation validates this.
                        // The DB status remains the selection SSOT per §4.4.

                        // Next action hint (§8.1)
                        println!();
                        if let Some(nch) = next_ch_val {
                            // Find the status of the next chapter
                            let nch_status = ch_list
                                .iter()
                                .find(|c| {
                                    c.get("chapter").and_then(serde_json::Value::as_i64)
                                        == Some(nch)
                                })
                                .and_then(|c| c.get("status").and_then(|v| v.as_str()))
                                .unwrap_or("unknown");

                            match nch_status {
                                "not_started" => {
                                    println!(
                                        "Next action: Chapter {nch} is not started; \
                                         run `creator run continue {work_id}` to begin."
                                    );
                                }
                                "outlined" => {
                                    println!(
                                        "Next action: Chapter {nch} is outlined; \
                                         run `creator run continue {work_id}` to start drafting."
                                    );
                                }
                                "draft" => {
                                    println!(
                                        "Next action: Chapter {nch} is in draft; \
                                         run `creator run continue {work_id}` to resume."
                                    );
                                }
                                _ => {
                                    println!("Next action: run `creator run continue {work_id}`");
                                }
                            }
                        } else {
                            // No next chapter — check if complete
                            if work_status == "completed" {
                                println!(
                                    "Next action: All chapters finalized; novel Work is complete."
                                );
                            } else if intake_status != "complete" {
                                println!(
                                    "Next action: Complete intake first via \
                                     `creator run stage advance {work_id} --stage intake`."
                                );
                            } else {
                                println!("Next action: run `creator run continue {work_id}`");
                            }
                        }
                    }
                } else if work_status == "completed" {
                    // Non-novel completed work (V1.36 P4 banner)
                    let updated_at = resp
                        .get("updated_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(unknown)");
                    println!("═══════════════════════════════════════════════════════");
                    println!("  \"{title}\" — Work {work_id}");
                    println!("  COMPLETED at {updated_at}");
                    println!("═══════════════════════════════════════════════════════");
                } else {
                    // Non-novel work or work without chapters — key-value dump
                    let fields = [
                        ("work_id", "work_id"),
                        ("title", "title"),
                        ("status", "status"),
                        ("intake_status", "intake_status"),
                        ("current_stage", "current_stage"),
                        ("stage_status", "stage_status"),
                        ("auto_chain_enabled", "auto_chain_enabled"),
                        ("driver_schedule_id", "driver_schedule_id"),
                        ("auto_chain_interrupted", "auto_chain_interrupted"),
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
        RunCommand::Stage { command } => handle_stage(command, config, &client).await?,
        RunCommand::ReconcileChapters { work_id, json } => {
            let report: serde_json::Value = client
                .post(
                    &format!("/v1/local/works/{work_id}/reconcile-chapters"),
                    &serde_json::json!({}),
                )
                .await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                let created = report
                    .get("created")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let updated = report
                    .get("updated")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let preserved = report
                    .get("preserved")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                println!("Reconcile complete for Work {work_id}:");
                println!("  Created:   {created}");
                println!("  Updated:   {updated}");
                println!("  Preserved: {preserved}");
            }
        }
        RunCommand::Resume { work_id, json } => {
            // V1.39 §5.7 (T8): Resume an interrupted auto-chain Work.
            // This clears auto_chain_interrupted and re-evaluates the next step.
            let patch = serde_json::json!({
                "auto_chain_interrupted": false,
            });
            let resp: serde_json::Value = client
                .patch::<serde_json::Value, _>(&format!("/v1/local/works/{work_id}"), &patch)
                .await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&resp)?);
            } else {
                let stage = resp
                    .get("current_stage")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let status = resp
                    .get("stage_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let auto_chain = resp
                    .get("auto_chain_enabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true);

                if auto_chain {
                    println!(
                        "Work {work_id} auto-chain resumed at stage '{stage}' ({status}). \
                         The daemon will evaluate the next step automatically."
                    );
                } else {
                    println!(
                        "Work {work_id} auto-chain is disabled. \
                         Use manual stage advance: nexus42 creator run stage advance {work_id} --stage <stage>"
                    );
                }
            }
        }
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
            force_gates,
            gate_reason,
            json,
        } => {
            // Validate --force-gates requires --gate-reason
            if force_gates && gate_reason.is_none() {
                return Err(crate::errors::CliError::Config(
                    "--force-gates requires --gate-reason \"<text>\" (audit-logged)".to_string(),
                ));
            }
            // W-5: Cap and sanitize gate-reason
            if let Some(ref r) = gate_reason {
                if r.len() > 512 {
                    return Err(crate::errors::CliError::Config(format!(
                        "--gate-reason exceeds maximum length (512 chars); got {} chars",
                        r.len()
                    )));
                }
                if r.contains('\x1b') || r.chars().any(|c| c.is_control() && c != '\n') {
                    return Err(crate::errors::CliError::Config(
                        "--gate-reason contains ANSI escape sequences or control characters"
                            .to_string(),
                    ));
                }
            }
            stage_advance(
                &work_id,
                &stage,
                force,
                force_gates,
                gate_reason.as_deref(),
                json,
                config,
                client,
            )
            .await
        }
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
/// Validate that novel-writing ("produce") chapter context is present.
///
/// When the target stage is "produce" (novel-writing preset), a chapter was
/// selected (`next_chapter.is_some()`), but both `outline_path` and
/// `body_path` are `None`, the template rendering would fail because both
/// prompt templates declare these variables as `required: true`. This
/// function returns an actionable error instead of silently proceeding.
fn validate_produce_chapter_context(
    target_stage: &str,
    next_chapter: Option<i32>,
    outline_path: Option<&str>,
    body_path: Option<&str>,
    work_id: &str,
) -> crate::errors::Result<()> {
    if target_stage == "produce"
        && next_chapter.is_some()
        && outline_path.is_none()
        && body_path.is_none()
    {
        return Err(crate::errors::CliError::Other(format!(
            "novel-writing schedule requires chapter context (outline_path, body_path).\n\
              The daemon response is missing chapters[] or the selected chapter row.\n\
              Hint: re-run `nexus42 creator run status {work_id}` to inspect,\n\
              or re-seed the work via `nexus42 creator run start --init-preset novel-project-init`."
        )));
    }
    Ok(())
}

/// V1.39 P5 (R-V138P1-01): Reject `stage advance` to `produce` when the novel
/// is complete (no remaining active chapter).
///
/// When the target stage is "produce" but `next_chapter` is `None`, the
/// daemon has determined that every chapter is finalized/published per
/// novel-workflow-profile §4.5.2. Building a `novel-writing` schedule in
/// this state would create a run with empty chapter fields (no outline,
/// no body path, no chapter number) that the prompt templates cannot
/// render. The correct response is to refuse the advance and point the
/// user at the persist stage which finalizes the Work.
fn reject_produce_when_novel_complete(
    target_stage: &str,
    next_chapter: Option<i32>,
    work_id: &str,
) -> crate::errors::Result<()> {
    if target_stage == "produce" && next_chapter.is_none() {
        return Err(crate::errors::CliError::Other(format!(
            "NOVEL_COMPLETE: cannot advance Work {work_id} to stage 'produce' — \
              no remaining active chapter (novel-workflow-profile §4.5.2).\n\
              Hint: advance to the 'persist' stage instead to finalize the Work, \
              or use `nexus42 creator run status {work_id}` to inspect chapter status."
        )));
    }
    Ok(())
}

/// Validates:
/// 1. Target stage is a known FL-E stage
/// 2. Target stage is ahead of current stage (unless `--force`)
/// 3. Current `stage_status` is `complete` (unless `--force`)
///
/// Then `PATCH`es the work via daemon API with the new stage/status.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
async fn stage_advance(
    work_id: &str,
    target_stage: &str,
    force: bool,
    force_gates: bool,
    gate_reason: Option<&str>,
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

    // V1.38 P0 (T4): extract work_ref and next_chapter from Work response
    // for novel-writing preset input. The daemon computes next_chapter per §4.5.2.
    let work_ref = resp
        .get("work_ref")
        .and_then(|v| v.as_str())
        .map(String::from);
    let next_chapter = resp
        .get("next_chapter")
        .and_then(serde_json::Value::as_i64)
        .map(|n| i32::try_from(n).unwrap_or(1));

    // V1.38 P1: extract chapter context (outline_path, body_path, slug)
    // from the chapters array for the selected chapter.
    let (chapter_label, outline_path, body_path, slug) = next_chapter
        .and_then(|ch_num| {
            let ch_label = stage_gates::chapter_label(ch_num);
            let chapters = resp.get("chapters").and_then(|v| v.as_array())?;
            let ch_row = chapters.iter().find(|c| {
                c.get("chapter").and_then(serde_json::Value::as_i64) == Some(i64::from(ch_num))
            })?;
            let op = ch_row
                .get("outline_path")
                .and_then(|v| v.as_str())
                .map(String::from);
            let bp = ch_row
                .get("body_path")
                .and_then(|v| v.as_str())
                .map(String::from);
            let sl = ch_row
                .get("slug")
                .and_then(|v| v.as_str())
                .map(String::from);
            Some((Some(ch_label), op, bp, sl))
        })
        .unwrap_or_default();

    // W-1 fix: fail fast when novel-writing ("produce") expects chapter context
    // but the daemon response is missing the chapters[] array or the selected
    // chapter row. Without outline_path and body_path, template rendering would
    // fail silently. Only fires when a chapter IS selected (next_chapter=Some)
    // but the context extraction returned None for both paths.
    validate_produce_chapter_context(
        target_stage,
        next_chapter,
        outline_path.as_deref(),
        body_path.as_deref(),
        work_id,
    )?;

    // V1.39 P5 (R-V138P1-01): when target_stage is "produce" but no chapter is
    // active (novel complete), refuse to build an empty-chapter schedule.
    reject_produce_when_novel_complete(target_stage, next_chapter, work_id)?;

    let fields = WorkFields {
        work_id: work_id.to_string(),
        fl_e_stage: target_stage.to_string(),
        creative_brief: creative_brief.to_string(),
        inspiration_log: inspiration_log.to_string(),
        work_ref,
        chapter: next_chapter,
        chapter_label,
        outline_path,
        body_path,
        slug,
    };

    if let Some(mut request) =
        stage_gates::build_schedule_for_stage(target_stage, creator_id, &fields)
    {
        // V1.37: pass force_gates + gate_reason through the schedule request
        request.force_gates = force_gates;
        request.reason = gate_reason.map(String::from);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_advance_produce_chapter_missing_chapter_array_returns_error() {
        let result =
            validate_produce_chapter_context("produce", Some(2), None, None, "wrk_test123");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("novel-writing schedule requires chapter context"),
            "error should mention chapter context: {err_msg}"
        );
        assert!(
            err_msg.contains("outline_path, body_path"),
            "error should mention missing fields: {err_msg}"
        );
        assert!(
            err_msg.contains("wrk_test123"),
            "error should include work_id hint: {err_msg}"
        );
    }

    #[test]
    fn validate_produce_ok_when_chapter_context_present() {
        let result = validate_produce_chapter_context(
            "produce",
            Some(2),
            Some("path/to/outline.md"),
            Some("path/to/body.md"),
            "wrk_test",
        );
        assert!(result.is_ok(), "should succeed when paths are present");
    }

    #[test]
    fn validate_skips_when_next_chapter_is_none() {
        // The chapter-context guard handles only the "context missing" case.
        // Novel-completion (next_chapter=None) is handled by the separate
        // `reject_produce_when_novel_complete` guard — see test below.
        let result = validate_produce_chapter_context("produce", None, None, None, "wrk_completed");
        assert!(
            result.is_ok(),
            "validate_produce_chapter_context should NOT error when next_chapter is None: {result:?}"
        );
    }

    #[test]
    fn validate_skips_for_non_produce_stage() {
        let result = validate_produce_chapter_context("research", Some(3), None, None, "wrk_other");
        assert!(
            result.is_ok(),
            "should NOT error for non-produce stages: {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // V1.39 P5 (R-V138P1-01): reject_produce_when_novel_complete
    // -----------------------------------------------------------------------

    #[test]
    fn reject_produce_when_novel_complete_errors_on_none_next_chapter() {
        // R-V138P1-01: when target_stage is "produce" and next_chapter is None
        // (all chapters finalized), advance must be refused — no empty-chapter
        // schedule should be created.
        let result = reject_produce_when_novel_complete("produce", None, "wrk_done");
        let err = result.expect_err("expected NOVEL_COMPLETE error when next_chapter=None");
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("NOVEL_COMPLETE"),
            "error should be tagged NOVEL_COMPLETE: {err_msg}"
        );
        assert!(
            err_msg.contains("persist"),
            "error should hint at 'persist' stage: {err_msg}"
        );
        assert!(
            err_msg.contains("wrk_done"),
            "error should include work_id: {err_msg}"
        );
    }

    #[test]
    fn reject_produce_when_novel_complete_allows_chapter_present() {
        // Normal case: a chapter is selected — advance is allowed.
        let result = reject_produce_when_novel_complete("produce", Some(2), "wrk_active");
        assert!(
            result.is_ok(),
            "should allow advance when next_chapter is Some: {result:?}"
        );
    }

    #[test]
    fn reject_produce_when_novel_complete_skips_other_stages() {
        // Non-produce stages (research/review/persist) are not gated by this rule.
        for stage in ["research", "review", "persist", "intake"] {
            let result = reject_produce_when_novel_complete(stage, None, "wrk_x");
            assert!(
                result.is_ok(),
                "stage '{stage}' should NOT be gated by novel-complete check: {result:?}"
            );
        }
    }
}
