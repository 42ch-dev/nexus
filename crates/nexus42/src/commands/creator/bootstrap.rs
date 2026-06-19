//! `nexus42 creator bootstrap` — composite Work onboarding (V1.45 P2).
//!
//! Three-plane IA (cli-command-ia.md):
//! - **`creator bootstrap`** = sole composite entry (create Work + schedule intake/production)
//! - **`creator works`** = atomic single-purpose ops
//! - **`creator run <preset_id>`** = strategy / preset dispatch
//!
//! This module extracts the V1.33 `run start` handler into a top-level command.
//! Flags are preserved 1:1; hint strings updated to V1.45 command surface.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Args;
use nexus_contracts::local::schedule::http::AddScheduleRequest;

/// Arguments for `creator bootstrap` (V1.45 P2).
///
/// Composite Work onboarding: creates a new Work, optionally schedules an init
/// preset, schedules intake (unless `--skip-intake`), and optionally chains
/// production directly.
///
/// Flags are 1:1 with the former `creator run start` handler.
#[derive(Debug, Args)]
/// 1:1 with `RunCommand::Start` flags (P0 owns the generic runner).
#[allow(clippy::struct_excessive_bools)] // CLI flag bag mirrors RunCommand::Start
pub struct BootstrapArgs {
    /// Initial creative idea (one or more sentences)
    #[arg(long)]
    pub idea: String,

    /// Work profile: 'novel' (default) or 'essay' (V1.52 T-A P2).
    /// Sets `work_profile` on the Work and selects the default init preset
    /// (`novel-project-init` for novel, `essay-init` for essay).
    #[arg(long, default_value = "novel")]
    pub profile: String,

    /// Override the primary production preset (default: derived from policy)
    #[arg(long)]
    pub preset: Option<String>,

    /// Optional title for the work
    #[arg(long)]
    pub title: Option<String>,

    /// Optional world binding (V1.36 §3.5; passes through to Work)
    #[arg(long)]
    pub world_id: Option<String>,

    /// Run an init preset before production (V1.36 §5.4)
    /// Accepts: novel-project-init
    #[arg(long)]
    pub init_preset: Option<String>,

    /// Skip the creative brief intake and start the production preset directly
    #[arg(long, default_value_t = false)]
    pub skip_intake: bool,

    /// After intake completes, print the next-stage command for the user
    /// to run manually (C-V133P2-03 partial). When `--skip-intake` is also
    /// set, scheduling of the production preset happens directly instead.
    /// Default true. Opt-out syntax: `--chain-novel-writing=false`. Full
    /// daemon `on_complete` auto-chain is a future enhancement (DF-53 partial).
    #[arg(
        long,
        default_value_t = true,
        value_parser = clap::builder::BoolishValueParser::new(),
        action = clap::ArgAction::Set
    )]
    pub chain_novel_writing: bool,

    /// Disable daemon-side auto-chain for this Work (V1.39 §5.4).
    /// When set, the daemon will NOT automatically advance FL-E stages
    /// or loop chapters after each stage completes.
    /// Default: auto-chain enabled (--no-auto-chain opts out).
    #[arg(long, default_value_t = false)]
    pub no_auto_chain: bool,

    /// Force gate bypass with audit reason (V1.36 §5.3.5)
    /// Requires --reason to be set alongside
    #[arg(long, default_value_t = false)]
    pub force_gates: bool,

    /// Audit reason for --force-gates (required when --force-gates is set)
    #[arg(long)]
    pub reason: Option<String>,

    /// Idempotency key (UUID); repeat calls with same key return same `work_id`
    #[arg(long)]
    pub client_request_id: Option<String>,

    /// Emit machine-readable JSON instead of human text
    #[arg(long, default_value_t = false)]
    pub json: bool,

    /// Start new Work lineage from a completed Work (DF-60 §5.2).
    /// Creates a new Work with `lineage_from_work_id` set.
    #[arg(long)]
    pub from_work: Option<String>,

    /// After start, set pool `active` to new Work (DF-60 §1.1).
    #[arg(long, default_value_t = false)]
    pub set_default: bool,
}

/// Handle `creator bootstrap` — composite Work onboarding.
///
/// Creates a new Work, optionally schedules an init preset, schedules intake
/// (unless skipped), and optionally chains production. Extracted from the
/// former `creator run start` handler (V1.45 P2).
///
/// # Errors
///
/// Returns an error if:
/// - `--force-gates` is set without `--reason`
/// - No active creator is selected
/// - The daemon API call fails
#[allow(clippy::too_many_lines)]
pub async fn handle_bootstrap(args: BootstrapArgs, config: &CliConfig) -> Result<()> {
    let BootstrapArgs {
        idea,
        preset,
        title,
        world_id,
        profile,
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
    } = args;

    let client = crate::api::DaemonClient::from_config(config);

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

    // V1.52 T-A P2: derive primary_preset_id from --profile when --preset not set.
    let primary_preset_id = preset.unwrap_or_else(|| match profile.as_str() {
        "essay" => "essay".to_string(),
        _ => "novel-writing".to_string(),
    });

    let mut body = serde_json::json!({
        "title": work_title,
        "long_term_goal": "Complete creative work",
        "initial_idea": idea,
        "primary_preset_id": primary_preset_id,
        "world_id": world_id,
        "client_request_id": client_request_id,
        "work_profile": profile,
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

    // V1.52 T-A P2: resolve effective init_preset from --profile when --init-preset
    // isn't explicitly set. Essay profile defaults to `essay-init`; novel profile
    // has no default init preset (user must pass --init-preset for novel scaffold).
    let effective_init_preset = init_preset.or_else(|| {
        if profile == "essay" {
            Some("essay-init".to_string())
        } else {
            None
        }
    });

    // V1.36: Schedule init preset if requested (before intake)
    let mut init_schedule_id: Option<String> = None;
    if let Some(ref ip) = effective_init_preset {
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
            .post::<serde_json::Value, _>("/v1/local/orchestration/schedules", &init_request)
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
            .post::<serde_json::Value, _>("/v1/local/orchestration/schedules", &intake_request)
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
        // Default to chapter 1 for the bootstrap path (first run).
        let novel_input = serde_json::json!({
            "work_id": work_id,
            "work_ref": work_title.to_lowercase().replace(' ', "-"),
            "topic": idea,
            "vibe": "literary",
            "chapter": 1,
        });
        let production_preset = primary_preset_id.as_str();
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
            .post::<serde_json::Value, _>("/v1/local/orchestration/schedules", &novel_request)
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
            println!(
                "Init preset scheduled: {iid} (preset: {})",
                effective_init_preset.as_deref().unwrap_or("?")
            );
            println!();
            println!("The init preset will bootstrap your Work's scaffold via ACP conversation.");
        }
        if let Some(sid) = &schedule_id {
            println!("Intake scheduled: {sid} (preset: creative-brief-intake)");
            println!();
            println!("The intake will run via ACP multi-turn conversation.");
            // V1.45 P2: hint updated from `run stage advance --stage produce`
            // to the generic runner command `creator run novel-writing`.
            println!("Once intake completes, advance to production with:");
            println!("  nexus42 creator run {primary_preset_id} {work_id}");
        } else if let Some(nid) = &novel_schedule_id {
            // Intake skipped, production scheduled directly.
            let production_preset = primary_preset_id.as_str();
            println!(
                "Production scheduled: {nid} (preset: {production_preset}, \
                 intake skipped)"
            );
        }
        println!();
        // V1.45 P2: hint updated from `run continue` to `works inspire`.
        println!("Next: nexus42 creator works inspire {work_id} --note \"<direction>\"");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Minimal CLI struct for hermetic parsing tests of `creator bootstrap`.
    #[derive(Parser)]
    struct BootstrapCli {
        #[command(subcommand)]
        command: BootstrapCmd,
    }

    #[derive(clap::Subcommand)]
    enum BootstrapCmd {
        Bootstrap(BootstrapArgs),
    }

    #[test]
    fn bootstrap_parses_with_idea() {
        let cli = BootstrapCli::try_parse_from([
            "nexus42",
            "bootstrap",
            "--idea",
            "A space opera about found family",
        ])
        .expect("bootstrap --idea should parse");
        match cli.command {
            BootstrapCmd::Bootstrap(args) => {
                assert_eq!(args.idea, "A space opera about found family");
                assert!(args.preset.is_none());
                assert!(args.title.is_none());
                assert!(!args.skip_intake);
                assert!(args.chain_novel_writing);
                assert!(!args.no_auto_chain);
                assert!(!args.force_gates);
                assert!(!args.json);
                assert!(!args.set_default);
            }
        }
    }

    #[test]
    fn bootstrap_parses_all_flags() {
        let cli = BootstrapCli::try_parse_from([
            "nexus42",
            "bootstrap",
            "--idea",
            "Test idea",
            "--title",
            "My Novel",
            "--preset",
            "novel-writing",
            "--world-id",
            "wld_test",
            "--init-preset",
            "novel-project-init",
            "--skip-intake",
            "--no-auto-chain",
            "--force-gates",
            "--reason",
            "testing",
            "--client-request-id",
            "abc-123",
            "--json",
            "--from-work",
            "wrk_old",
            "--set-default",
        ])
        .expect("all flags should parse");
        match cli.command {
            BootstrapCmd::Bootstrap(args) => {
                assert_eq!(args.idea, "Test idea");
                assert_eq!(args.title.as_deref(), Some("My Novel"));
                assert_eq!(args.preset.as_deref(), Some("novel-writing"));
                assert_eq!(args.world_id.as_deref(), Some("wld_test"));
                assert_eq!(args.init_preset.as_deref(), Some("novel-project-init"));
                assert!(args.skip_intake);
                assert!(args.no_auto_chain);
                assert!(args.force_gates);
                assert_eq!(args.reason.as_deref(), Some("testing"));
                assert_eq!(args.client_request_id.as_deref(), Some("abc-123"));
                assert!(args.json);
                assert_eq!(args.from_work.as_deref(), Some("wrk_old"));
                assert!(args.set_default);
            }
        }
    }

    #[test]
    fn bootstrap_requires_idea() {
        let result = BootstrapCli::try_parse_from(["nexus42", "bootstrap"]);
        assert!(
            result.is_err(),
            "bootstrap without --idea should fail to parse"
        );
    }

    #[test]
    fn bootstrap_chain_novel_writing_opt_out() {
        let cli = BootstrapCli::try_parse_from([
            "nexus42",
            "bootstrap",
            "--idea",
            "test",
            "--chain-novel-writing=false",
        ])
        .expect("opt-out should parse");
        match cli.command {
            BootstrapCmd::Bootstrap(args) => {
                assert!(!args.chain_novel_writing);
            }
        }
    }

    #[test]
    fn bootstrap_profile_default_is_novel() {
        let cli =
            BootstrapCli::try_parse_from(["nexus42", "bootstrap", "--idea", "A thoughtful essay"])
                .expect("bootstrap without --profile should parse");
        match cli.command {
            BootstrapCmd::Bootstrap(args) => {
                assert_eq!(args.profile, "novel");
            }
        }
    }

    #[test]
    fn bootstrap_profile_essay_parses() {
        let cli = BootstrapCli::try_parse_from([
            "nexus42",
            "bootstrap",
            "--idea",
            "A thoughtful essay",
            "--profile",
            "essay",
        ])
        .expect("bootstrap --profile essay should parse");
        match cli.command {
            BootstrapCmd::Bootstrap(args) => {
                assert_eq!(args.profile, "essay");
            }
        }
    }
}
