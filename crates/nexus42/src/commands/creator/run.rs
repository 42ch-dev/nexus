//! `nexus42 creator run <preset_id>` — generic preset runner (V1.45 §4).
//!
//! Replaces the V1.33–V1.44 bespoke subcommand dispatch (`start`, `continue`,
//! `stage`, `resume`, `audit-chapter`, `review-master`) with a single generic
//! entry point:
//!
//! `nexus42 creator run <preset_id> [<work_id>] [global flags] [preset args...]`
//!
//! FL-E stage-advance presets (`research`, `novel-writing`, `reflection-loop`,
//! `kb-extract`) are dispatched to `stage_advance`; all other presets are
//! scheduled directly via the daemon Local API.
//!
//! Legacy handler code is preserved as `#[allow(dead_code)]` for P1/P2
//! migration reference.

use crate::config::CliConfig;
use crate::errors::Result;
use nexus_contracts::local::orchestration::preset::{PresetCliArg, PresetCliArgType};
use nexus_contracts::local::schedule::http::AddScheduleRequest;
use nexus_orchestration::preset::validation::stage_for_preset;
use nexus_orchestration::stage_gates::{self, WorkFields, WorkStageState};

// Legacy imports (preserved for P1/P2 migration).
#[allow(unused_imports)]
use clap::Subcommand;
#[allow(unused_imports)]
use nexus_contracts::local::orchestration::{stage_index, FL_E_STAGES};

// ── V1.45 generic RunCommand struct ─────────────────────────────────────────

/// `nexus42 creator run <preset_id> [<work_id>]` — generic preset dispatch.
///
/// Global flags (`--json`, `--force-gates`, `--reason`) must appear before
/// preset-specific trailing args. Once trailing args start consuming, all
/// remaining tokens (including `--flag`-shaped values) are captured into
/// `extra` verbatim.
#[derive(Debug, clap::Args)]
pub struct RunCommand {
    /// Preset ID to run (e.g. `novel-brainstorm`, `novel-manuscript-audit-review`)
    pub preset_id: String,

    /// Work ID (`wrk_...`). If omitted, the active pool Work is used.
    pub work_id: Option<String>,

    /// Emit machine-readable JSON instead of human text
    #[arg(long, default_value_t = false)]
    pub json: bool,

    /// Force gate bypass with audit reason (requires `--reason`)
    #[arg(long, default_value_t = false)]
    pub force_gates: bool,

    /// Audit reason for `--force-gates` (required when `--force-gates` is set)
    #[arg(long)]
    pub reason: Option<String>,

    /// Preset-specific trailing args (captured after global flags; parsed
    /// against `preset.cli_args` at runtime). Everything after the last
    /// recognized positional is captured verbatim.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, num_args = 0..)]
    pub extra: Vec<String>,
}

// ── V1.45 generic handle_run ────────────────────────────────────────────────

/// Run the `creator run <preset_id>` generic dispatch (V1.45 §4).
///
/// # Errors
///
/// Returns an error if the daemon API call fails, the preset is unknown,
/// or required CLI args are missing.
pub async fn handle_run(cmd: RunCommand, config: &CliConfig) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);

    // Validate --force-gates requires --reason (same rule as legacy `start`).
    if cmd.force_gates && cmd.reason.is_none() {
        return Err(crate::errors::CliError::Config(
            "--force-gates requires --reason \"<text>\" (audit-logged)".to_string(),
        ));
    }
    // W-5: Cap and sanitize reason.
    if let Some(ref r) = cmd.reason {
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

    let RunCommand {
        preset_id,
        work_id,
        json,
        force_gates,
        reason,
        extra,
    } = cmd;

    // Resolve work_id: if omitted, try the pool active Work.
    let resolved_work_id = resolve_work_id(&client, work_id).await?;

    // FL-E stage-advance presets: dispatch to stage_advance.
    if let Some(target_stage) = stage_for_preset(&preset_id) {
        tracing::info!(
            preset_id = %preset_id,
            stage = %target_stage,
            "FL-E stage-advance preset; dispatching to stage_advance"
        );
        return stage_advance(
            &resolved_work_id,
            target_stage,
            false, // force: stage ordering is enforced for generic dispatch
            force_gates,
            reason.as_deref(),
            json,
            config,
            &client,
        )
        .await;
    }

    // Non-FL-E preset: resolve manifest to get cli_args schema, parse trailing
    // args, build AddScheduleRequest, POST to daemon.
    let nexus_home = crate::config::nexus_home()
        .map_err(|e| crate::errors::CliError::Config(format!("Cannot resolve nexus home: {e}")))?;
    let caps = nexus_orchestration::capability::CapabilityRegistry::with_builtins();

    let loaded = nexus_orchestration::preset::resolve_preset(&preset_id, &nexus_home, &caps)
        .map_err(|e| {
            crate::errors::CliError::Config(format!(
                "Unknown preset '{preset_id}': {e}. \
                 Run `nexus42 creator presets list` to see available presets."
            ))
        })?;

    // Parse trailing args against preset.cli_args declarations.
    let input = parse_preset_cli_args(&loaded.manifest.preset.cli_args, &extra)?;

    let resolved_creator_id = config
        .active_creator_id
        .clone()
        .ok_or(crate::errors::CliError::CreatorNotSelected)?;

    let request = AddScheduleRequest {
        creator_id: resolved_creator_id,
        preset_id: preset_id.clone(),
        seed: None,
        label: None,
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: Some(input),
        force_gates,
        reason,
    };

    let resp: serde_json::Value = client
        .post::<serde_json::Value, _>(
            "/v1/local/orchestration/schedules",
            &request,
        )
        .await?;

    let schedule_id = resp
        .get("schedule_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Preset '{preset_id}' scheduled: {schedule_id}");
        println!("Work: {resolved_work_id}");
    }

    Ok(())
}

/// Resolve `work_id` from CLI arg or the pool active Work.
async fn resolve_work_id(
    client: &crate::api::DaemonClient,
    work_id: Option<String>,
) -> Result<String> {
    if let Some(id) = work_id {
        return Ok(id);
    }
    let resp: serde_json::Value = client
        .get::<serde_json::Value>("/v1/local/works?limit=1&status=active")
        .await?;
    resp.get("works")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|w| w.get("work_id"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| {
            crate::errors::CliError::Config(
                "No active Work found. Specify <work_id> or run `nexus42 creator works use <work_id>`.".to_string(),
            )
        })
}

/// Parse trailing CLI args against `preset.cli_args` declarations (V1.45 §3.3).
///
/// Returns a JSON object mapping arg names to coerced values, suitable for
/// `AddScheduleRequest.input`.
fn parse_preset_cli_args(
    cli_args: &[PresetCliArg],
    raw: &[String],
) -> Result<serde_json::Value> {
    use std::collections::HashMap;

    // If the preset declares no cli_args, ignore trailing args silently.
    if cli_args.is_empty() {
        return Ok(serde_json::json!({}));
    }

    // Build a lookup: kebab-name → PresetCliArg
    let lookup: HashMap<&str, &PresetCliArg> = cli_args
        .iter()
        .map(|a| (a.name.as_str(), a))
        .collect();

    // Parse `--name value` pairs from the raw trailing args.
    let mut parsed: HashMap<String, serde_json::Value> = HashMap::new();
    let mut i = 0;
    while i < raw.len() {
        let token = &raw[i];
        let name = token
            .strip_prefix("--")
            .ok_or_else(|| {
                crate::errors::CliError::Config(format!(
                    "Unexpected positional '{token}' in preset args. \
                     Preset-specific args must use --flag syntax."
                ))
            })?;

        let arg = lookup.get(name).ok_or_else(|| {
            crate::errors::CliError::Config(format!(
                "Unknown preset flag '--{name}'. \
                 This preset accepts: {}",
                cli_args
                    .iter()
                    .map(|a| format!("--{}", a.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;

        match arg.r#type {
            PresetCliArgType::Boolean => {
                // Boolean flags don't consume a value (presence = true).
                parsed.insert(arg.name.clone(), serde_json::json!(true));
                i += 1;
            }
            PresetCliArgType::Integer => {
                i += 1;
                let val = raw.get(i).ok_or_else(|| {
                    crate::errors::CliError::Config(format!(
                        "Flag '--{name}' requires an integer value"
                    ))
                })?;
                let n: i64 = val.parse().map_err(|_| {
                    crate::errors::CliError::Config(format!(
                        "Flag '--{name}' expects an integer; got '{val}'"
                    ))
                })?;
                parsed.insert(arg.name.clone(), serde_json::json!(n));
                i += 1;
            }
            PresetCliArgType::String => {
                i += 1;
                let val = raw.get(i).ok_or_else(|| {
                    crate::errors::CliError::Config(format!(
                        "Flag '--{name}' requires a string value"
                    ))
                })?;
                parsed.insert(arg.name.clone(), serde_json::json!(val));
                i += 1;
            }
        }
    }

    // Apply defaults and check required args.
    for arg in cli_args {
        if parsed.contains_key(&arg.name) {
            continue;
        }
        if let Some(ref default) = arg.default {
            parsed.insert(arg.name.clone(), default.clone());
        } else if arg.required {
            return Err(crate::errors::CliError::Config(format!(
                "Required flag '--{}' is missing for preset",
                arg.name
            )));
        }
    }

    Ok(serde_json::Value::Object(parsed.into_iter().collect()))
}

// ── Legacy enum + handlers (preserved for P1/P2 migration) ──────────────────

/// Legacy subcommand enum (V1.33–V1.44). Preserved for P1/P2 migration.
#[allow(dead_code)]
#[derive(Debug, clap::Subcommand)]
pub enum LegacyRunCommand {
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
        /// Start new Work lineage from a completed Work (DF-60 §5.2).
        /// Creates a new Work with `lineage_from_work_id` set.
        #[arg(long)]
        from_work: Option<String>,
        /// After start, set pool `active` to new Work (DF-60 §1.1).
        #[arg(long, default_value_t = false)]
        set_default: bool,
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
    ///
    /// On a completed Work (after completion-lock release), requires `--reopen`
    /// with audited `--reason` (DF-60 §3.2).
    Resume {
        /// Work ID (wrk_...) to resume
        work_id: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Reopen a completed Work for further writing (DF-60 §3.2).
        /// Requires --reason. Only valid after completion-lock release.
        #[arg(long, default_value_t = false)]
        reopen: bool,
        /// Audit reason for reopening a completed Work (required with --reopen).
        #[arg(long)]
        reason: Option<String>,
        /// Extend `total_planned_chapters` when reopening (required when §6
        /// completion criteria still hold after reopen).
        #[arg(long)]
        extend_chapters: Option<i32>,
    },
    /// On-demand chapter audit — review or extract without entering FL-E auto-chain (DF-69)
    ///
    /// Audits an already-written chapter body. Two modes:
    ///   - review:  structured 五問 review report → Logs/review/
    ///   - extract: synchronous World KB extraction (World-bound Works only)
    ///
    /// This command does NOT create an FL-E driver schedule or advance auto-chain state.
    AuditChapter {
        /// Work ID (wrk_...) to audit
        work_id: String,
        /// Audit mode: "review" (structured review report) or "extract" (World KB extract)
        #[arg(long, value_enum)]
        mode: AuditMode,
        /// Chapter number to audit (required)
        #[arg(long)]
        chapter: i32,
        /// Volume number (default 1; required for multi-volume Works)
        #[arg(long, default_value_t = 1)]
        volume: i32,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run the master-decision review on open findings (V1.44 P1).
    ///
    /// Lists open findings with `target_executor=master` and optionally
    /// enqueues the `novel-review-master` preset for a specific finding.
    /// Distinct from `stage advance --stage review` which runs the
    /// `reflection-loop` FL-E review stage.
    ///
    /// See docs/novel-writing-quickstart.md §5 for usage patterns.
    ReviewMaster {
        /// Work ID (wrk_...) to review
        work_id: String,
        /// Run review-master preset scoped to a specific finding
        #[arg(long)]
        finding_id: Option<String>,
        /// Opt-in: enqueue novel-review-master when this Work has stale
        /// (96h+) findings. Scoped to the supplied `work_id` — only stale
        /// findings belonging to this Work trigger the schedule.
        #[arg(long, default_value_t = false)]
        auto_schedule: bool,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Audit mode for `creator run audit-chapter` (DF-69). Preserved for P1/P2.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum AuditMode {
    /// Structured 五問 review report under Logs/review/
    Review,
    /// Synchronous World KB extract (World-bound Works only)
    Extract,
}

impl std::fmt::Display for AuditMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Review => write!(f, "review"),
            Self::Extract => write!(f, "extract"),
        }
    }
}

/// FL-E stage subcommands (V1.34 cli-spec §6.2E). Preserved for P1/P2.
#[allow(dead_code)]
#[derive(Debug, clap::Subcommand)]
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
/// Legacy handler for V1.33–V1.44 subcommand dispatch. Preserved for P1/P2.
///
/// # Errors
///
/// Returns an error if the daemon API call fails.
#[allow(dead_code)]
#[allow(clippy::too_many_lines)]
#[allow(clippy::missing_panics_doc)] // expect on constant URL string; never panics
async fn handle_run_legacy(cmd: LegacyRunCommand, config: &CliConfig) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);

    match cmd {
        LegacyRunCommand::Start {
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
            from_work,
            set_default,
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

            // DF-60 §5.2: lineage from completed Work.
            if let Some(ref fw) = from_work {
                if let Some(o) = body.as_object_mut() {
                    o.insert(
                        "lineage_from_work_id".to_string(),
                        serde_json::Value::String(fw.clone()),
                    );
                }
            }

            // DF-60 §1.1: set pool `active` after creation.
            if set_default {
                if let Some(o) = body.as_object_mut() {
                    o.insert("set_pool_active".to_string(), serde_json::Value::Bool(true));
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
        LegacyRunCommand::Continue {
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
        LegacyRunCommand::Stage { command } => handle_stage(command, config, &client).await?,
        LegacyRunCommand::ReconcileChapters { work_id, json } => {
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
        LegacyRunCommand::Resume {
            work_id,
            json,
            reopen,
            reason,
            extend_chapters,
        } => {
            // DF-60 §3.2: reopen a completed Work.
            if reopen {
                if reason.is_none() {
                    return Err(crate::errors::CliError::Config(
                        "--reopen requires --reason \"<text>\" (audit-logged)".to_string(),
                    ));
                }
                if let Some(ref r) = reason {
                    if r.len() > 512 {
                        return Err(crate::errors::CliError::Config(format!(
                            "--reason exceeds maximum length (512 chars); got {} chars",
                            r.len()
                        )));
                    }
                }

                let mut patch = serde_json::json!({
                    "novel_completion_status": "reopened",
                    "completion_locked_at": null,
                });
                if let Some(ext) = extend_chapters {
                    if let Some(o) = patch.as_object_mut() {
                        o.insert(
                            "total_planned_chapters".to_string(),
                            serde_json::Value::Number(ext.into()),
                        );
                    }
                }

                let resp: serde_json::Value = client
                    .patch::<serde_json::Value, _>(&format!("/v1/local/works/{work_id}"), &patch)
                    .await?;

                if json {
                    println!("{}", serde_json::to_string_pretty(&resp)?);
                } else {
                    let ext_msg = extend_chapters
                        .map(|n| format!(" (chapters extended to {n})"))
                        .unwrap_or_default();
                    println!(
                        "Work {work_id} reopened for further writing.{ext_msg}\n\
                         Reason: {}",
                        reason.as_deref().unwrap_or("(none)")
                    );
                }
            } else {
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
        LegacyRunCommand::AuditChapter {
            work_id,
            mode,
            chapter,
            volume,
            json,
        } => {
            handle_audit_chapter(&work_id, mode, chapter, volume, json, config, &client).await?;
        }
        LegacyRunCommand::ReviewMaster {
            work_id,
            finding_id,
            auto_schedule,
            json,
        } => {
            handle_review_master(
                &work_id,
                finding_id.as_deref(),
                auto_schedule,
                json,
                config,
                &client,
            )
            .await?;
        }
    }

    Ok(())
}

// ── Review-master CLI (V1.44 P1) ────────────────────────────────────────────

/// Fetch common work context fields for review-master schedule input.
///
/// Returns `(work_ref, topic, world_id, work_json)` where `work_json` is the
/// full Work response (used by `--finding-id` path for `body_path` extraction).
/// Extracted to avoid duplicate Work fetch in `--finding-id` and
/// `--auto-schedule` paths (R-V144P1-005).
/// Fetch common work context fields for review-master schedule input. (P1/P2)
#[allow(dead_code)]
async fn fetch_work_context(
    client: &crate::api::DaemonClient,
    work_id: &str,
) -> Result<(String, String, Option<String>, serde_json::Value)> {
    let work: serde_json::Value = client
        .get::<serde_json::Value>(&format!("/v1/local/works/{work_id}"))
        .await?;
    let work_ref = work
        .get("work_ref")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let topic = work
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("novel")
        .to_string();
    let world_id = work
        .get("world_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    Ok((work_ref, topic, world_id, work))
}

/// Handle `creator run review-master <work_id>` (V1.44 P1). Preserved for P1/P2.
///
/// Lists open findings with `target_executor=master` and optionally enqueues
/// the `novel-review-master` preset for a specific finding or on opt-in
/// auto-schedule.
///
/// # Errors
///
/// Returns an error if the daemon API call fails or the work is not found.
#[allow(dead_code)]
#[allow(clippy::too_many_lines)]
async fn handle_review_master(
    work_id: &str,
    finding_id: Option<&str>,
    auto_schedule: bool,
    json: bool,
    config: &CliConfig,
    client: &crate::api::DaemonClient,
) -> Result<()> {
    // Fetch open findings for the Work.
    // Uses limit=200 to reduce truncation risk for high-volume works;
    // client-side filter to master-targeted findings follows.
    // For works with >200 open findings, the summary may be incomplete
    // (R-V144P1-006: documented cap; daemon-side target_executor filter
    // is deferred to a future iteration).
    let findings: Vec<serde_json::Value> = client
        .get::<Vec<serde_json::Value>>(&format!(
            "/v1/local/works/{work_id}/findings?status=open&limit=200"
        ))
        .await?;

    // Filter to master-targeted findings
    let master_findings: Vec<&serde_json::Value> = findings
        .iter()
        .filter(|f| f.get("target_executor").and_then(|v| v.as_str()) == Some("master"))
        .collect();

    if json {
        let output = serde_json::json!({
            "work_id": work_id,
            "master_findings_count": master_findings.len(),
            "master_findings": master_findings,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if master_findings.is_empty() {
        println!("No master findings for Work {work_id}.");
        println!("See docs/novel-writing-quickstart.md §5 for the quality-loop workflow.");
    } else {
        // Summary: open master findings count + top 3 by severity
        let severity_order = |s: &str| match s {
            "blocker" => 0,
            "major" => 1,
            "minor" => 2,
            "info" => 3,
            _ => 4,
        };
        let mut sorted: Vec<&&serde_json::Value> = master_findings.iter().collect();
        sorted.sort_by_key(|f| {
            severity_order(f.get("severity").and_then(|v| v.as_str()).unwrap_or("info"))
        });

        println!(
            "Master findings for Work {work_id}: {} open",
            master_findings.len()
        );
        println!();

        let top = sorted.iter().take(3);
        for (i, f) in top.enumerate() {
            let finding_id_val = f.get("finding_id").and_then(|v| v.as_str()).unwrap_or("?");
            let severity = f.get("severity").and_then(|v| v.as_str()).unwrap_or("?");
            let title = f.get("title").and_then(|v| v.as_str()).unwrap_or("?");
            println!("  #{idx} [{severity}] {title}", idx = i + 1);
            println!("     finding_id: {finding_id_val}");
        }
        println!();
        if master_findings.len() > 3 {
            println!(
                "  ... and {} more master finding(s).",
                master_findings.len() - 3
            );
            println!();
        }
        println!("Next: nexus42 creator run review-master {work_id} --finding-id <id>");
    }

    // --finding-id: enqueue novel-review-master preset scoped to one finding
    if let Some(fid) = finding_id {
        let creator_id = config
            .active_creator_id
            .as_deref()
            .ok_or(crate::errors::CliError::CreatorNotSelected)?;

        // Fetch the specific finding to get its details
        let finding: serde_json::Value = client
            .get::<serde_json::Value>(&format!("/v1/local/works/{work_id}/findings/{fid}"))
            .await?;

        // R-V144P1-004: assert the finding is master-targeted before enqueuing
        let target_executor = finding.get("target_executor").and_then(|v| v.as_str());
        if target_executor != Some("master") {
            let actual = target_executor.unwrap_or("(missing)");
            return Err(crate::errors::CliError::Config(format!(
                "finding {fid} has target_executor={actual}, not 'master'; \
                 review-master only enqueues master-targeted findings"
            )));
        }

        let finding_title = finding
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("(untitled)");

        // R-V144P1-005: use shared work context helper (avoids duplicate fetch)
        let (work_ref, topic, world_id, work) = fetch_work_context(client, work_id).await?;

        let body_path = finding
            .get("chapter")
            .and_then(serde_json::Value::as_i64)
            .and_then(|ch| {
                work.get("chapters")
                    .and_then(|v| v.as_array())
                    .and_then(|chapters| {
                        chapters.iter().find(|c| {
                            c.get("chapter").and_then(serde_json::Value::as_i64) == Some(ch)
                        })
                    })
                    .and_then(|c| c.get("body_path").and_then(|v| v.as_str()))
            });

        // Serialize the single finding as open_findings input
        let open_findings_json = serde_json::to_string(&vec![&finding])?;

        let mut input = serde_json::json!({
            "work_id": work_id,
            "work_ref": work_ref,
            "topic": topic,
            "open_findings": open_findings_json,
        });
        if let Some(wid) = world_id {
            if let Some(o) = input.as_object_mut() {
                o.insert("world_id".to_string(), serde_json::Value::String(wid));
            }
        }
        if let Some(bp) = body_path {
            if let Some(o) = input.as_object_mut() {
                o.insert(
                    "body_path".to_string(),
                    serde_json::Value::String(bp.to_string()),
                );
            }
        }

        let schedule_request = AddScheduleRequest {
            creator_id: creator_id.to_string(),
            preset_id: "novel-review-master".to_string(),
            seed: Some(format!("Review finding: {finding_title}")),
            label: None,
            depends_on: None,
            concurrency: None,
            scheduled_at: None,
            input: Some(input),
            force_gates: false,
            reason: None,
        };

        let sched_resp: serde_json::Value = client
            .post::<serde_json::Value, _>("/v1/local/orchestration/schedules", &schedule_request)
            .await?;

        let schedule_id = sched_resp
            .get("schedule_id")
            .and_then(|v| v.as_str())
            .unwrap_or("?");

        if json {
            let output = serde_json::json!({
                "work_id": work_id,
                "finding_id": fid,
                "schedule_id": schedule_id,
                "preset": "novel-review-master",
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!();
            println!("Enqueued novel-review-master for finding {fid} ({finding_title}).");
            println!("  Schedule ID: {schedule_id}");
            println!("  The daemon will run the review-master preset via ACP.");
        }
    }

    // --auto-schedule: opt-in enqueue when 96h stale findings exist for this Work
    if auto_schedule {
        let creator_id = config
            .active_creator_id
            .as_deref()
            .ok_or(crate::errors::CliError::CreatorNotSelected)?;

        // R-V144P1-003: scope stale check to the current Work.
        // The daemon /stale endpoint is creator-global; we filter client-side
        // to count only stale findings belonging to this work_id, so that
        // auto-schedule fires only when this Work has stale master findings.
        let stale: serde_json::Value = client
            .get::<serde_json::Value>("/v1/local/findings/stale")
            .await?;

        let stale_findings = stale
            .get("findings")
            .and_then(|v| v.as_array())
            .map_or(&[] as &[serde_json::Value], |v| v.as_slice());
        let work_stale_count = stale_findings
            .iter()
            .filter(|f| f.get("work_id").and_then(|v| v.as_str()) == Some(work_id))
            .count();

        if work_stale_count == 0 {
            if !json {
                println!("No stale findings for this Work — auto-schedule skipped.");
            }
        } else {
            // R-V144P1-005: use shared work context helper
            let (work_ref, topic, world_id, _work) = fetch_work_context(client, work_id).await?;

            // Fetch open master findings for the input
            let open_findings_json = serde_json::to_string(&master_findings)?;

            let mut input = serde_json::json!({
                "work_id": work_id,
                "work_ref": work_ref,
                "topic": topic,
                "open_findings": open_findings_json,
            });
            if let Some(wid) = world_id {
                if let Some(o) = input.as_object_mut() {
                    o.insert("world_id".to_string(), serde_json::Value::String(wid));
                }
            }

            let schedule_request = AddScheduleRequest {
                creator_id: creator_id.to_string(),
                preset_id: "novel-review-master".to_string(),
                seed: Some("Auto-scheduled master review (stale findings)".to_string()),
                label: None,
                depends_on: None,
                concurrency: None,
                scheduled_at: None,
                input: Some(input),
                force_gates: false,
                reason: None,
            };

            let sched_resp: serde_json::Value = client
                .post::<serde_json::Value, _>(
                    "/v1/local/orchestration/schedules",
                    &schedule_request,
                )
                .await?;

            let schedule_id = sched_resp
                .get("schedule_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");

            if json {
                let output = serde_json::json!({
                    "work_id": work_id,
                    "auto_schedule": true,
                    "stale_count": work_stale_count,
                    "schedule_id": schedule_id,
                    "preset": "novel-review-master",
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!();
                println!(
                    "Auto-scheduled novel-review-master for {work_stale_count} stale finding(s) in this Work."
                );
                println!("  Schedule ID: {schedule_id}");
            }
        }
    }

    Ok(())
}

// ── On-demand chapter audit (DF-69, V1.44 P0) ─────────────────────────────

/// Handle `creator run audit-chapter` subcommand (DF-69).
///
/// Creates a schedule for the `novel-manuscript-audit-review` or
/// `novel-manuscript-audit-extract` preset based on mode, with the
/// given chapter and volume. Does NOT enter the FL-E auto-chain driver.
///
/// # Runtime lock invariant (R-V144P0-010)
///
/// The CLI handler creates a schedule (not a direct Work mutation), so the
/// per-Work `runtime_lock_holder` is not acquired here. The daemon supervisor
/// serializes schedule execution per `Serial` concurrency, preventing concurrent
/// same-Work mutation during audit execution. The `novel-manuscript-audit-extract`
/// preset's `world_binding: required` gate provides an additional boundary.
///
/// # Errors
///
/// Returns an error if:
/// - The Work does not exist or is not a novel Work.
/// - Extract mode is requested on a worldless Work (422).
/// - The chapter cannot be resolved (missing `body_path`).
/// - The daemon API call fails.
#[allow(dead_code)]
#[allow(clippy::too_many_lines)] // Single-entry CLI handler; splitting would create a >7-arg helper
async fn handle_audit_chapter(
    work_id: &str,
    mode: AuditMode,
    chapter: i32,
    volume: i32,
    json: bool,
    config: &CliConfig,
    client: &crate::api::DaemonClient,
) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(crate::errors::CliError::CreatorNotSelected)?;

    // Fetch Work state to extract work_ref and world_id
    let resp: serde_json::Value = client
        .get::<serde_json::Value>(&format!("/v1/local/works/{work_id}"))
        .await?;

    let work_ref = resp
        .get("work_ref")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let world_id = resp
        .get("world_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    let body_path = resolve_audit_body_path(&resp, chapter, volume);

    // R-V144P0-007: fail fast if body_path cannot be resolved
    if body_path.is_none() {
        return Err(crate::errors::CliError::Config(format!(
            "Cannot resolve chapter {chapter} (volume {volume}) for Work {work_id}. \
             Verify the chapter exists in the Work's chapters array, \
             or provide a valid body path override."
        )));
    }

    // Extract mode: validate World-bound precondition (R-V144P0-008: typed error)
    if matches!(mode, AuditMode::Extract) && world_id.is_none() {
        return Err(crate::errors::CliError::WorldRequiredForExtract {
            work_id: work_id.to_string(),
        });
    }

    // Build preset input
    let mut audit_input = serde_json::json!({
        "work_id": work_id,
        "work_ref": work_ref,
        "mode": mode.to_string(),
        "chapter": chapter,
        "volume": volume,
        "creator_id": creator_id,
        "upsert_findings": true,
    });

    // Set body_path if resolved
    if let Some(ref bp) = body_path {
        if let Some(o) = audit_input.as_object_mut() {
            o.insert(
                "body_path".to_string(),
                serde_json::Value::String(bp.clone()),
            );
        }
    }

    // Set world_id for extract mode
    if let Some(ref wid) = world_id {
        if let Some(o) = audit_input.as_object_mut() {
            o.insert(
                "world_id".to_string(),
                serde_json::Value::String(wid.clone()),
            );
        }
    }

    // R-V144P0-001: dispatch to the correct split preset based on mode
    let preset_id = match mode {
        AuditMode::Review => "novel-manuscript-audit-review",
        AuditMode::Extract => "novel-manuscript-audit-extract",
    };

    let request = AddScheduleRequest {
        creator_id: creator_id.to_string(),
        preset_id: preset_id.to_string(),
        seed: Some(format!(
            "audit-chapter {work_id} mode={mode} ch={chapter} vol={volume}"
        )),
        label: Some(format!(
            "On-demand audit: {mode} ch{chapter} v{volume} ({work_id})"
        )),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: Some(audit_input),
        force_gates: false,
        reason: None,
    };

    let mut sched_resp: serde_json::Value = client
        .post::<serde_json::Value, _>("/v1/local/orchestration/schedules", &request)
        .await?;

    let schedule_id = sched_resp
        .get("schedule_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();

    if json {
        if let Some(o) = sched_resp.as_object_mut() {
            o.insert(
                "audit_mode".to_string(),
                serde_json::Value::String(mode.to_string()),
            );
            o.insert(
                "chapter".to_string(),
                serde_json::Value::Number(chapter.into()),
            );
            o.insert(
                "volume".to_string(),
                serde_json::Value::Number(volume.into()),
            );
        }
        println!("{}", serde_json::to_string_pretty(&sched_resp)?);
    } else {
        println!("Audit schedule created: {mode} mode for Work {work_id} ch{chapter} v{volume}");
        println!("  Schedule: {schedule_id} (preset: {preset_id}, status: pending)");
        println!("  The daemon will execute this schedule asynchronously.");
        if matches!(mode, AuditMode::Review) {
            println!(
                "  On completion, the review report will be under Works/{work_ref}/Logs/review/."
            );
        } else {
            println!(
                "  On completion, KB extraction results will be available for World {}.",
                world_id.as_deref().unwrap_or("?")
            );
        }
    }

    Ok(())
}

/// Resolve the `body_path` for the audit chapter from the Work response.
///
/// Looks up the chapter row matching the given chapter/volume in the
/// Work's chapters array. Returns `None` if not found.
///
/// # Path validation (R-V144P0-004)
///
/// Rejects paths that are absolute, contain `..`, or do not start with
/// the expected `Works/` layout prefix.
/// Resolve chapter body path for audit (P1/P2). Preserved for P1/P2.
#[allow(dead_code)]
fn resolve_audit_body_path(
    work_resp: &serde_json::Value,
    chapter: i32,
    volume: i32,
) -> Option<String> {
    let chapters = work_resp.get("chapters").and_then(|v| v.as_array())?;

    // R-V144P0-003: filter by volume when volume > 1 or multiple matches exist
    let ch_row = chapters
        .iter()
        .find(|c| {
            let matches_chapter =
                c.get("chapter").and_then(serde_json::Value::as_i64) == Some(i64::from(chapter));
            let matches_volume =
                c.get("volume").and_then(serde_json::Value::as_i64) == Some(i64::from(volume));
            matches_chapter && matches_volume
        })
        .or_else(|| {
            // Fallback: match chapter only (backward compat for Works without volume field)
            chapters.iter().find(|c| {
                c.get("chapter").and_then(serde_json::Value::as_i64) == Some(i64::from(chapter))
            })
        })?;

    let raw_path = ch_row.get("body_path").and_then(|v| v.as_str())?;

    // R-V144P0-004: validate path safety
    validate_body_path(raw_path)
}

/// Validate a resolved `body_path` against path traversal and layout rules.
///
/// Returns `Some(path)` if the path is safe, `None` if it fails validation.
/// Validate body path safety (P1/P2). Preserved for P1/P2.
#[allow(dead_code)]
fn validate_body_path(path: &str) -> Option<String> {
    // Reject absolute paths
    if path.starts_with('/') {
        return None;
    }
    // Reject path traversal
    if path.contains("..") {
        return None;
    }
    // Must be under expected layout prefix
    if !path.starts_with("Works/") {
        return None;
    }
    // Reject control characters
    if path.chars().any(char::is_control) {
        return None;
    }
    Some(path.to_string())
}

// ── FL-E stage management (V1.34) ───────────────────────────────────────────

/// Handle `creator run stage` subcommands (V1.34 FL-E §3, cli-spec §6.2E). Preserved for P1/P2.
///
/// # Errors
///
/// Returns an error if the daemon API call fails or stage validation rejects the advance.
#[allow(dead_code)]
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

/// List FL-E stages and current status for a Work. Preserved for P1/P2.
///
/// Fetches the Work from the daemon and displays all stages with
/// markers for the current stage and status.
#[allow(dead_code)]
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
        // V1.43 (P1 §3 remediation — work completed): cite quickstart §6.
        return Err(crate::errors::CliError::Other(format!(
            "This Work is complete; see docs/novel-writing-quickstart.md §6. \
              Use `nexus42 creator works status {work_id}` or advance to the 'persist' stage."
        )));
    }
    Ok(())
}

/// V1.40 P2 (QC3 C-1 fix): assemble the World KB context block for a Work.
///
/// Opens the local workspace KB store, queries all characters/locations/rules
/// for the given world, and returns the YAML-rendered block string.
/// Returns empty string on missing data (world has no KB items).
///
/// # Errors
///
/// Returns an error if the local DB cannot be opened or the KB query fails.
async fn assemble_world_kb_block(
    world_id: &str,
    config: &CliConfig,
) -> crate::errors::Result<String> {
    use nexus_moment_context_assembly::{build_chapter_kb_block, ChapterKbBlockParams};

    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;
    let store = nexus_local_db::kb_store::SqliteKbStore::new(pool);

    let params = ChapterKbBlockParams {
        world_id: world_id.to_string(),
        world_name: String::new(), // Populated from KB if available
        current_timeline: String::new(),
        world_refs: vec![], // Empty: falls back to all characters/locations
        chapter_text: None,
        max_tokens: None,
    };

    match build_chapter_kb_block(&store, &params).await {
        Ok(Some(block)) => Ok(block.to_yaml()),
        Ok(None) => Ok(String::new()),
        Err(e) => Err(crate::errors::CliError::Other(format!(
            "World KB query failed for world {world_id}: {e}"
        ))),
    }
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

    // V1.44 P3 (R-V138P1-07): audit-log chapter context extraction to aid
    // production debugging when chapter selection behaves unexpectedly.
    tracing::debug!(
        target: "fl_e.stage",
        work_id = %work_id,
        next_chapter = ?next_chapter,
        chapter_label = ?chapter_label,
        outline_path = ?outline_path,
        body_path = ?body_path,
        slug = ?slug,
        "stage_advance chapter context extracted"
    );

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

    // V1.40 P2 (QC3 C-1 fix): build World KB context block for World-bound Works.
    // When the Work has a world_id, open the local KB store and assemble the
    // chapter KB block. For worldless Works (world_id == None), the block is
    // left empty so the template guard `{{#if world_kb_block}}` omits it.
    let world_id = resp
        .get("world_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    let world_kb_block = if let Some(ref wid) = world_id {
        // Best-effort: assemble the block. On error (no DB, missing world, etc.),
        // log a warning and continue with empty block so the schedule still proceeds.
        match assemble_world_kb_block(wid, config).await {
            Ok(block) => Some(block),
            Err(e) => {
                tracing::warn!(
                    target: "fl_e.stage",
                    world_id = %wid,
                    error = %e,
                    "Failed to assemble World KB block; proceeding with empty block"
                );
                None
            }
        }
    } else {
        None
    };

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
        research_artifacts_dir: None,
        workspace_dir: None,
        world_kb_block,
        world_id,
        volume: None,
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
        // V1.43 (P1 §3 remediation — work completed): error cites quickstart §6.
        let result = reject_produce_when_novel_complete("produce", None, "wrk_done");
        let err = result.expect_err("expected NOVEL_COMPLETE error when next_chapter=None");
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("Work is complete"),
            "error should say 'Work is complete': {err_msg}"
        );
        assert!(
            err_msg.contains("novel-writing-quickstart.md §6"),
            "error should cite quickstart §6: {err_msg}"
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

    // -----------------------------------------------------------------------
    // V1.44 P0 (DF-69): audit-chapter CLI tests
    // -----------------------------------------------------------------------

    #[test]
    fn audit_mode_display_review() {
        assert_eq!(AuditMode::Review.to_string(), "review");
    }

    #[test]
    fn audit_mode_display_extract() {
        assert_eq!(AuditMode::Extract.to_string(), "extract");
    }

    #[test]
    fn resolve_audit_body_path_finds_chapter() {
        let resp = serde_json::json!({
            "chapters": [
                {"chapter": 1, "volume": 1, "body_path": "Works/novel/Stories/ch01.md"},
                {"chapter": 3, "volume": 1, "body_path": "Works/novel/Stories/ch03.md"},
            ]
        });
        let result = resolve_audit_body_path(&resp, 3, 1);
        assert_eq!(result.as_deref(), Some("Works/novel/Stories/ch03.md"));
    }

    #[test]
    fn resolve_audit_body_path_returns_none_for_missing() {
        let resp = serde_json::json!({
            "chapters": [
                {"chapter": 1, "volume": 1, "body_path": "Works/novel/Stories/ch01.md"},
            ]
        });
        let result = resolve_audit_body_path(&resp, 99, 1);
        assert!(result.is_none());
    }

    #[test]
    fn resolve_audit_body_path_returns_none_for_empty_chapters() {
        let resp = serde_json::json!({"work_id": "wrk_test"});
        let result = resolve_audit_body_path(&resp, 1, 1);
        assert!(result.is_none());
    }

    // R-V144P0-003: volume-aware lookup
    #[test]
    fn resolve_audit_body_path_filters_by_volume() {
        let resp = serde_json::json!({
            "chapters": [
                {"chapter": 1, "volume": 1, "body_path": "Works/novel/Stories/v1/ch01.md"},
                {"chapter": 1, "volume": 2, "body_path": "Works/novel/Stories/v2/ch01.md"},
            ]
        });
        let result = resolve_audit_body_path(&resp, 1, 2);
        assert_eq!(result.as_deref(), Some("Works/novel/Stories/v2/ch01.md"));
    }

    #[test]
    fn resolve_audit_body_path_falls_back_without_volume_field() {
        let resp = serde_json::json!({
            "chapters": [
                {"chapter": 1, "body_path": "Works/novel/Stories/ch01.md"},
            ]
        });
        let result = resolve_audit_body_path(&resp, 1, 1);
        assert_eq!(result.as_deref(), Some("Works/novel/Stories/ch01.md"));
    }

    // R-V144P0-004: path validation
    #[test]
    fn validate_body_path_rejects_absolute() {
        assert!(validate_body_path("/etc/passwd").is_none());
    }

    #[test]
    fn validate_body_path_rejects_traversal() {
        assert!(validate_body_path("Works/novel/../../etc/passwd").is_none());
    }

    #[test]
    fn validate_body_path_rejects_non_works_prefix() {
        assert!(validate_body_path("tmp/evil.md").is_none());
    }

    #[test]
    fn validate_body_path_accepts_valid_path() {
        assert_eq!(
            validate_body_path("Works/my-novel/Stories/ch01.md").as_deref(),
            Some("Works/my-novel/Stories/ch01.md")
        );
    }

    #[test]
    fn validate_body_path_rejects_control_chars() {
        assert!(validate_body_path("Works/novel/Stories/ch\x01.md").is_none());
    }

    // R-V144P0-008: typed error variant
    #[test]
    fn world_required_for_extract_error_display() {
        let err = crate::errors::CliError::WorldRequiredForExtract {
            work_id: "wrk_test123".to_string(),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("422 world_required_for_extract"),
            "error must contain error code: {msg}"
        );
        assert!(
            msg.contains("wrk_test123"),
            "error must contain work_id: {msg}"
        );
        assert!(
            msg.contains("Suggestion:"),
            "error must contain suggestion: {msg}"
        );
    }
}
