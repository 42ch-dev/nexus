//! `nexus42 creator bootstrap` — composite Work onboarding (V1.45 P2).
//!
//! Three-plane IA (cli-command-ia.md):
//! - **`creator bootstrap`** = sole composite entry (create Work + schedule intake/production)
//! - **`creator works`** = atomic single-purpose ops
//! - **`creator run <preset_id>`** = strategy / preset dispatch
//!
//! This module extracts the V1.33 `run start` handler into a top-level command.
//! Flags are preserved 1:1; hint strings updated to V1.45 command surface.
//!
//! V1.176 P0 T1 (AR-88): this module also hosts the shared local-creator
//! bootstrap helper [`bootstrap_local_creator`] — the single identity-mint +
//! workspace-row materialization sequence both named local entry points
//! (`creator register --local`, `system identity create --persistent`) call.
//! V1.176 P0 T2 (AR-89): the helper is idempotent — re-running converges
//! (no-op / repair) or fails honestly on a name collision instead of
//! minting a duplicate.

use crate::commands::system::identity::open_global_db;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Args;
use nexus_contracts::local::schedule::http::AddScheduleRequest;
use nexus_creator::local_identity::LocalIdentity;
use nexus_local_db::create_local_identity;

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

    /// Work profile: 'novel' (default), 'essay' (V1.52 T-A P2),
    /// 'game-bible' (V1.54 P1), or 'script' (V1.55 P3). Sets `work_profile`
    /// on the Work and selects the default init preset
    /// (`novel-project-init` for novel, `essay-init` for essay,
    /// `game-bible-init` for game-bible, `script-init` for script).
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

    // V1.54 P1 fix-wave (C-001): normalize CLI spelling to canonical stored value.
    // Users may pass "--profile game-bible" (hyphen) or "--profile game_bible" (underscore).
    // The canonical stored value is "game_bible" (matches CHECK constraint, preset gates,
    // and profile helpers). Other profiles pass through unchanged.
    let profile = match profile.as_str() {
        "game-bible" => "game_bible".to_string(),
        other => other.to_string(),
    };

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
    // V1.54 P1: game-bible has no primary production preset yet (deferred to V1.55+);
    // return "game-bible" as the profile tag itself.
    let primary_preset_id = preset.unwrap_or_else(|| match profile.as_str() {
        "essay" => "essay".to_string(),
        "game_bible" => "game-bible".to_string(),
        "script" => "script".to_string(),
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
        .post::<serde_json::Value, _>("/v1/daemon/works", &body)
        .await?;

    let work_id = resp
        .get("work_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();

    // V1.52 T-A P2: resolve effective init_preset from --profile when --init-preset
    // isn't explicitly set. Essay profile defaults to `essay-init`; game-bible
    // defaults to `game-bible-init` (V1.54 P1); novel profile has no default init
    // preset (user must pass --init-preset for novel scaffold).
    let effective_init_preset = init_preset.or_else(|| match profile.as_str() {
        "essay" => Some("essay-init".to_string()),
        "game_bible" => Some("game-bible-init".to_string()),
        "script" => Some("script-init".to_string()),
        _ => None,
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
            // V1.54 P1 fix-wave (C-002): game-bible-init preset uses
            // {{preset.input.creator_id}} and {{preset.input.initial_idea}};
            // bootstrap must seed both so the capability receives them.
            "creator_id": resolved_creator_id,
            "initial_idea": idea,
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
            .post::<serde_json::Value, _>("/v1/daemon/orchestration/schedules", &init_request)
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
            .post::<serde_json::Value, _>("/v1/daemon/orchestration/schedules", &intake_request)
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
    // V1.54 P1 fix-wave (W-001): only novel profiles chain into production
    // scheduling. Game-bible and essay have no production preset yet;
    // auto-chaining from a non-novel profile violates "no auto-chain" spec.
    if chain_novel_writing && skip_intake && profile == "novel" {
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
            .post::<serde_json::Value, _>("/v1/daemon/orchestration/schedules", &novel_request)
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

/// Shared local-creator bootstrap helper (V1.176 P0 T1, AR-88).
///
/// Owns the single identity-mint + workspace-row materialization sequence
/// for both named local entry points: `creator register --local --name <n>`
/// and `system identity create --persistent [--name <n>]`. There is exactly
/// one minting + materialization sequence in the crate.
///
/// Converged end state (compass PL-3, checked by tests, not implied):
/// 1. a persistent `ctr_local*` row in `~/.nexus42/state.db` `local_identities`;
/// 2. that id is `active_creator_id` in the CLI config;
/// 3. the workspace `creators` row exists in the per-creator+workspace db
///    resolved by `config::resolve_state_db_path` (the same db `creator world
///    create` FK-prechecks), written via `nexus_local_db::ensure_creator_row`.
///
/// The `creator-identities.json` cache is **not** written (AR-88 #3): it is
/// best-effort display metadata for the platform path only; local display
/// SSOT is `local_identities`.
///
/// Re-entrancy is idempotent (AR-88 #6 / AR-89): a crash between stores
/// leaves exactly the DF-83 partial (identity without row), which the next
/// run repairs — the named 1-match leg converges that id (row upsert +
/// activation), and the nameless path converges the already-active
/// persistent identity. A name shared by 2+ persistent identities is an
/// honest `creator_name_collision` error, never a silent takeover.
///
/// # Errors
///
/// Returns `CliError` if the identity database, config, or workspace db
/// operations fail, or if the display name collides with 2+ existing
/// persistent identities (`CreatorNameCollision`).
pub(crate) async fn bootstrap_local_creator(name: Option<String>) -> Result<()> {
    // R3(identity): reject empty or whitespace-only names at the helper front
    // door — both entry points now enforce it here (AR-89 #1).
    let trimmed_name = name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    if let Some(raw) = &name {
        if raw.trim().is_empty() {
            return Err(CliError::Other(
                "Display name cannot be empty or whitespace-only.".to_string(),
            ));
        }
    }

    let pool = open_global_db().await?;

    // AR-89 decision tree (PL-5): named → 0/1/2+ persistent matches;
    // nameless → converge the already-active persistent identity if any,
    // else mint nameless.
    if let Some(trimmed) = trimmed_name {
        let matches = persistent_rows_with_name(&pool, trimmed).await?;
        match matches.len() {
            0 => mint_and_materialize(&pool, Some(trimmed)).await,
            1 => converge_identity(&matches[0]).await,
            _ => Err(CliError::CreatorNameCollision {
                display_name: trimmed.to_string(),
                matches: matches.into_iter().map(|r| r.creator_id).collect(),
            }),
        }
    } else {
        let cli_config = CliConfig::load()?;
        if let Some(active_id) = &cli_config.active_creator_id {
            if let Some(row) = nexus_local_db::get_local_identity(&pool, active_id).await? {
                if row.identity_type == "persistent" {
                    return converge_identity(&row).await;
                }
            }
        }
        mint_and_materialize(&pool, None).await
    }
}

/// Persistent `local_identities` rows whose display name is byte-exactly
/// `trimmed` (AR-89 #1: `str::trim` + `==` — no case-fold, no Unicode
/// normalization, no prefix/substring matching).
async fn persistent_rows_with_name(
    pool: &nexus_local_db::SqlitePool,
    trimmed: &str,
) -> Result<Vec<nexus_local_db::LocalIdentityRow>> {
    let rows = nexus_local_db::list_local_identities(pool).await?;
    Ok(rows
        .into_iter()
        .filter(|r| r.identity_type == "persistent" && r.display_name.as_deref() == Some(trimmed))
        .collect())
}

/// AR-89 mint leg: fresh persistent identity + INSERT + set active +
/// `ensure_creator_row`. Prints the recognizable "Created persistent
/// identity: …" shape (AR-88 #5 / AR-89 #5).
async fn mint_and_materialize(
    pool: &nexus_local_db::SqlitePool,
    trimmed_name: Option<&str>,
) -> Result<()> {
    let identity = LocalIdentity::create_persistent(trimmed_name);
    create_local_identity(
        pool,
        &identity.creator_id,
        identity.identity_type.as_str(),
        identity.display_name.as_deref(),
        &identity.created_at,
    )
    .await?;

    println!("Created persistent identity: {}", identity.creator_id);
    if let Some(name) = &identity.display_name {
        println!("  Name: {name}");
    }
    println!("  Stored in ~/.nexus42/state.db");

    // Set as active creator (store 2 of 3).
    let mut cli_config = CliConfig::load()?;
    cli_config.active_creator_id = Some(identity.creator_id.clone());
    cli_config.save()?;
    println!("  Set as active identity.");

    // Materialize the workspace `creators` row (store 3 of 3) in the same
    // per-creator+workspace db `creator world create` FK-prechecks.
    let db_path = crate::config::resolve_state_db_path(&cli_config)?;
    let workspace_pool = crate::db::Schema::init(&db_path).await?;
    let row_display_name = identity
        .display_name
        .clone()
        .unwrap_or_else(|| identity.creator_id.clone());
    nexus_local_db::ensure_creator_row(&workspace_pool, &identity.creator_id, &row_display_name)
        .await?;

    Ok(())
}

/// AR-89 no-op / repair leg: converge `row`'s identity — ensure the
/// workspace `creators` row (repair if missing) and activate if not already
/// active (session selection). Never prints "Created" (PL-5 hard pin).
async fn converge_identity(row: &nexus_local_db::LocalIdentityRow) -> Result<()> {
    let creator_id = &row.creator_id;
    let row_display_name = row
        .display_name
        .clone()
        .unwrap_or_else(|| creator_id.clone());

    let mut cli_config = CliConfig::load()?;
    let already_active = cli_config.active_creator_id.as_deref() == Some(creator_id.as_str());
    if !already_active {
        cli_config.active_creator_id = Some(creator_id.clone());
    }

    // The workspace db is per-creator (ADR-014): resolve it for the matched
    // identity, not the currently-active one.
    let db_path = crate::config::resolve_state_db_path(&cli_config)?;
    let workspace_pool = crate::db::Schema::init(&db_path).await?;
    let row_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
    )
    .bind(creator_id)
    .fetch_one(&workspace_pool)
    .await?;
    let repaired = row_exists == 0;
    // True no-op: row present + already active → read-only verification, no
    // workspace-row write (no `cached_at` churn). Write only when the row is
    // missing (repair) or a different identity is being activated (session
    // selection).
    if repaired || !already_active {
        nexus_local_db::ensure_creator_row(&workspace_pool, creator_id, &row_display_name).await?;
    }

    if !already_active {
        cli_config.save()?;
    }

    if repaired {
        println!("Workspace creators row materialized for {creator_id}.");
    } else if !already_active {
        println!("Identity {creator_id} is already registered; set as active identity.");
    } else {
        println!("Identity {creator_id} is already converged (active + workspace row present).");
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

    #[test]
    fn bootstrap_profile_game_bible_parses() {
        let cli = BootstrapCli::try_parse_from([
            "nexus42",
            "bootstrap",
            "--idea",
            "A dark fantasy RPG with faction warfare",
            "--profile",
            "game_bible",
        ])
        .expect("bootstrap --profile game_bible should parse");
        match cli.command {
            BootstrapCmd::Bootstrap(args) => {
                assert_eq!(args.profile, "game_bible");
            }
        }
    }

    #[test]
    fn bootstrap_profile_game_bible_init_preset_derived() {
        let cli = BootstrapCli::try_parse_from([
            "nexus42",
            "bootstrap",
            "--idea",
            "A space opera game bible",
            "--profile",
            "game_bible",
        ])
        .expect("bootstrap --profile game_bible should parse");
        match cli.command {
            BootstrapCmd::Bootstrap(args) => {
                assert_eq!(args.profile, "game_bible");
                assert!(
                    args.init_preset.is_none(),
                    "init_preset is not explicitly set; derived to game-bible-init in handler"
                );
                assert!(!args.skip_intake);
                assert!(!args.no_auto_chain);
            }
        }
    }

    // ── V1.54 P1 fix-wave regression tests ──────────────────────────────

    /// C-001: CLI accepts `--profile game-bible` (hyphen spelling).
    /// The normalization to canonical `game_bible` (underscore) happens in
    /// `handle_bootstrap`; this test proves the CLI parser does not reject
    /// the hyphenated form.
    #[test]
    fn bootstrap_profile_game_bible_hyphen_parses() {
        let cli = BootstrapCli::try_parse_from([
            "nexus42",
            "bootstrap",
            "--idea",
            "A tabletop RPG with alien civilizations",
            "--profile",
            "game-bible",
        ])
        .expect("bootstrap --profile game-bible (hyphen) should parse");
        match cli.command {
            BootstrapCmd::Bootstrap(args) => {
                assert_eq!(
                    args.profile, "game-bible",
                    "CLI accepts hyphen form; normalization happens in handler"
                );
            }
        }
    }

    /// W-001: verify that the production-scheduling gate excludes non-novel
    /// profiles. The gate `profile == "novel"` prevents game-bible and essay
    /// from auto-chaining into a production preset when `--skip-intake` is set.
    #[test]
    fn bootstrap_game_bible_skip_intake_no_production_schedule() {
        // This test validates the profile gate logic: for game-bible,
        // `--skip-intake --chain-novel-writing` (default true) should NOT
        // trigger production scheduling because `profile == "novel"` is false.
        // The gate is: `if chain_novel_writing && skip_intake && profile == "novel"`.
        //
        // We verify the CLI side: game-bible profile + skip_intake parses,
        // and chain_novel_writing defaults to true but the handler gate
        // prevents scheduling.
        let cli = BootstrapCli::try_parse_from([
            "nexus42",
            "bootstrap",
            "--idea",
            "A game design document",
            "--profile",
            "game-bible",
            "--skip-intake",
        ])
        .expect("bootstrap --profile game-bible --skip-intake should parse");
        match cli.command {
            BootstrapCmd::Bootstrap(args) => {
                assert_eq!(args.profile, "game-bible");
                assert!(args.skip_intake);
                assert!(
                    args.chain_novel_writing,
                    "chain_novel_writing defaults to true"
                );
                // Gate: handler only schedules production for novel profile.
                // This test proves CLI arg setup is correct; handler behavior
                // is verified via e2e test (W-004).
            }
        }
    }

    // ── V1.176 P0 T1 (AR-88): shared bootstrap helper ──────────────

    /// The helper converges all three stores (compass PL-3): a persistent
    /// `ctr_local*` row in `local_identities`, that id as `active_creator_id`,
    /// and the workspace `creators` row in the per-creator+workspace db —
    /// so `creator world create` succeeds immediately after.
    #[tokio::test]
    async fn bootstrap_local_creator_converges_three_stores() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("  Alice  ".to_string()))
            .await
            .expect("bootstrap should succeed");

        // Store 1: persistent row in the global identity store.
        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1, "exactly one identity minted");
        let row = &identities[0];
        assert!(
            row.creator_id.starts_with("ctr_local"),
            "expected ctr_local* id, got {}",
            row.creator_id
        );
        assert_eq!(row.identity_type, "persistent");
        assert_eq!(
            row.display_name.as_deref(),
            Some("Alice"),
            "R3-trimmed name"
        );

        // Store 2: active creator id in the CLI config.
        let config = CliConfig::load().expect("reload config");
        assert_eq!(
            config.active_creator_id.as_deref(),
            Some(row.creator_id.as_str()),
            "minted id must be active"
        );

        // Store 3: workspace `creators` row in the same db `creator world
        // create` FK-prechecks.
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        let creator_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
        )
        .bind(&row.creator_id)
        .fetch_one(&workspace_pool)
        .await
        .expect("query workspace creators row");
        assert_eq!(
            creator_exists, 1,
            "workspace creators row must exist for the bootstrapped creator"
        );

        // `creator world create` succeeds immediately (no FK miss).
        let result = nexus_local_db::create_world(
            &workspace_pool,
            &row.creator_id,
            "Test World",
            "test-world",
            "public",
            "manual",
        )
        .await
        .expect("create_world must succeed after bootstrap");
        assert!(result.world_id.starts_with("wld_"));
    }

    /// Nameless mint: the workspace row `display_name` falls back to the
    /// `creator_id` string itself (AR-88 #4 — never empty).
    #[tokio::test]
    async fn bootstrap_local_creator_nameless_uses_creator_id_as_display_name() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(None)
            .await
            .expect("bootstrap succeeds");

        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1);
        let row = &identities[0];
        assert!(row.display_name.is_none(), "nameless mint stores no name");

        let config = CliConfig::load().expect("reload config");
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        let display_name: String =
            sqlx::query_scalar("SELECT display_name FROM creators WHERE creator_id = ?")
                .bind(&row.creator_id)
                .fetch_one(&workspace_pool)
                .await
                .expect("query creators display_name");
        assert_eq!(
            display_name, row.creator_id,
            "nameless mint row display_name = creator_id (AR-88 #4)"
        );
    }

    /// The identity-cache store (`creator-identities.json`) is NOT written
    /// (AR-88 #3): local identities carry no handle; display SSOT is
    /// `local_identities`.
    #[tokio::test]
    async fn bootstrap_local_creator_does_not_write_identity_cache() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("Cache Free".to_string()))
            .await
            .expect("bootstrap succeeds");

        let cache_path = crate::creator_identity::cache_path().expect("cache path");
        assert!(
            !cache_path.exists(),
            "creator-identities.json must not be written by the local bootstrap"
        );
    }

    /// Whitespace-only names are rejected at the helper front door (R3).
    #[tokio::test]
    async fn bootstrap_local_creator_rejects_whitespace_only_name() {
        let _home = crate::testutil::isolated_home();

        let err = bootstrap_local_creator(Some("   ".to_string()))
            .await
            .expect_err("whitespace-only name must be rejected");
        let display = format!("{err}");
        assert!(
            display.contains("Display name cannot be empty or whitespace-only."),
            "unexpected error: {display}"
        );
    }

    // ── V1.176 P0 T2 (AR-89): idempotent re-register + partial-bootstrap recovery ──

    /// No-op success: re-running the same name against the already-converged
    /// identity must not mint a second identity (stdout honesty — no
    /// "Created" — is pinned at the e2e level where stdout is captured).
    #[tokio::test]
    async fn bootstrap_local_creator_noop_keeps_single_identity() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("  Alice  ".to_string()))
            .await
            .expect("first bootstrap mints");
        let pool = open_global_db().await.expect("open global db");
        let first = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(first.len(), 1);
        let first_id = first[0].creator_id.clone();

        bootstrap_local_creator(Some("Alice".to_string()))
            .await
            .expect("re-run converges, no collision");

        let after = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(after.len(), 1, "no second identity minted");
        assert_eq!(after[0].creator_id, first_id, "same identity id");

        // Still fully converged: active + workspace row present.
        let config = CliConfig::load().expect("reload config");
        assert_eq!(config.active_creator_id.as_deref(), Some(first_id.as_str()));
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        let row_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
        )
        .bind(&first_id)
        .fetch_one(&workspace_pool)
        .await
        .expect("query workspace creators row");
        assert_eq!(row_exists, 1, "workspace row still present");
    }

    /// Store-level no-op pin: the no-op leg must not UPDATE the workspace
    /// `creators` row (no `cached_at` churn). A sentinel `cached_at` survives
    /// the re-run untouched.
    #[tokio::test]
    async fn bootstrap_local_creator_noop_does_not_touch_workspace_row() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("Alice".to_string()))
            .await
            .expect("first bootstrap mints");
        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1);
        let id = identities[0].creator_id.clone();

        // Stamp the workspace row with a sentinel timestamp; a no-op re-run
        // must leave it untouched (read-only verification).
        let config = CliConfig::load().expect("reload config");
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        sqlx::query("UPDATE creators SET cached_at = ? WHERE creator_id = ?")
            .bind("2000-01-01T00:00:00Z")
            .bind(&id)
            .execute(&workspace_pool)
            .await
            .expect("stamp sentinel cached_at");

        bootstrap_local_creator(Some("Alice".to_string()))
            .await
            .expect("no-op re-run converges");

        let cached_at: String =
            sqlx::query_scalar("SELECT cached_at FROM creators WHERE creator_id = ?")
                .bind(&id)
                .fetch_one(&workspace_pool)
                .await
                .expect("query cached_at");
        assert_eq!(
            cached_at, "2000-01-01T00:00:00Z",
            "no-op must not rewrite the workspace row (cached_at churn)"
        );
    }

    /// Match-key negative (case): `Alice` vs `alice` are distinct byte-exact
    /// keys (AR-89 #1 — no case-fold). Seeding both and re-running `Alice`
    /// converges the exact `Alice` row (1 match) — never a collision.
    #[tokio::test]
    async fn bootstrap_local_creator_match_key_is_case_sensitive() {
        let _home = crate::testutil::isolated_home();

        let pool = open_global_db().await.expect("open global db");
        create_local_identity(
            &pool,
            "ctr_localcase1",
            "persistent",
            Some("Alice"),
            "2026-08-26T00:00:00Z",
        )
        .await
        .expect("seed Alice");
        create_local_identity(
            &pool,
            "ctr_localcase2",
            "persistent",
            Some("alice"),
            "2026-08-26T00:00:00Z",
        )
        .await
        .expect("seed alice");

        // Byte-exact: "Alice" matches only the "Alice" row → converge, no
        // collision (a case-folded key would match 2 rows and error).
        bootstrap_local_creator(Some("Alice".to_string()))
            .await
            .expect("exact-case match converges");

        let config = CliConfig::load().expect("reload config");
        assert_eq!(
            config.active_creator_id.as_deref(),
            Some("ctr_localcase1"),
            "exact byte match activated"
        );
    }

    /// Match-key negative (Unicode): no normalization — a decomposed twin
    /// (`Cafe\u{301}`, NFD) does not match the NFC row (`Café`) → 0 matches
    /// → mint a distinct identity (AR-89 #1).
    #[tokio::test]
    async fn bootstrap_local_creator_match_key_does_not_normalize_unicode() {
        let _home = crate::testutil::isolated_home();

        let pool = open_global_db().await.expect("open global db");
        create_local_identity(
            &pool,
            "ctr_localnfc1",
            "persistent",
            Some("Café"),
            "2026-08-26T00:00:00Z",
        )
        .await
        .expect("seed NFC row");

        // NFD twin: `e` + U+0301 combining acute. Byte-exact `==` → 0 matches.
        bootstrap_local_creator(Some("Cafe\u{301}".to_string()))
            .await
            .expect("decomposed twin mints a new identity");

        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 2, "decomposed twin is a distinct name");
        assert!(
            identities.iter().any(|r| r.creator_id == "ctr_localnfc1"),
            "NFC row still present"
        );
    }

    /// Repair: simulate the DF-83 partial (identity present, workspace row
    /// missing) by deleting the workspace row, then re-run — the row is
    /// materialized again and no new identity is minted.
    #[tokio::test]
    async fn bootstrap_local_creator_repairs_missing_workspace_row() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("Repair Me".to_string()))
            .await
            .expect("first bootstrap mints");
        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1);
        let id = identities[0].creator_id.clone();

        // Simulate the partial: delete the workspace `creators` row.
        let config = CliConfig::load().expect("reload config");
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        sqlx::query("DELETE FROM creators WHERE creator_id = ?")
            .bind(&id)
            .execute(&workspace_pool)
            .await
            .expect("delete workspace row");

        // Re-run the same name → repair leg.
        bootstrap_local_creator(Some("Repair Me".to_string()))
            .await
            .expect("re-run repairs");

        // Same identity, no new mint.
        let after = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(after.len(), 1, "no second identity minted");
        assert_eq!(after[0].creator_id, id);

        // Row is back.
        let row_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
        )
        .bind(&id)
        .fetch_one(&workspace_pool)
        .await
        .expect("query workspace creators row");
        assert_eq!(row_exists, 1, "workspace row repaired");
    }

    /// Honest collision: two persistent rows share the display name → the
    /// `CreatorNameCollision` error lists both ids (AR-89 #4) — never a
    /// silent takeover.
    #[tokio::test]
    async fn bootstrap_local_creator_collision_lists_matching_ids() {
        let _home = crate::testutil::isolated_home();

        // Seed two persistent rows sharing the display name directly.
        let pool = open_global_db().await.expect("open global db");
        for id in ["ctr_localcoll1", "ctr_localcoll2"] {
            create_local_identity(
                &pool,
                id,
                "persistent",
                Some("Shared Name"),
                "2026-08-26T00:00:00Z",
            )
            .await
            .expect("seed persistent row");
        }

        let err = bootstrap_local_creator(Some("Shared Name".to_string()))
            .await
            .expect_err("ambiguous name must collide");
        match err {
            CliError::CreatorNameCollision {
                display_name,
                matches,
            } => {
                assert_eq!(display_name, "Shared Name");
                assert_eq!(
                    matches,
                    vec!["ctr_localcoll1".to_string(), "ctr_localcoll2".to_string()]
                );
            }
            other => panic!("expected CreatorNameCollision, got {other:?}"),
        }
    }

    /// Session selection: a single name match on a *different* identity than
    /// the active one converges that id (activates it) — never a collision,
    /// never a silent takeover of an unmatched id (AR-89 #2).
    #[tokio::test]
    async fn bootstrap_local_creator_single_match_activates_matched_identity() {
        let _home = crate::testutil::isolated_home();

        // Mint one identity with a name, then switch active to a different
        // persistent identity.
        bootstrap_local_creator(Some("Target Name".to_string()))
            .await
            .expect("mint target");
        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1);
        let target_id = identities[0].creator_id.clone();

        // Seed a second persistent identity and make it active.
        create_local_identity(
            &pool,
            "ctr_localother",
            "persistent",
            Some("Other"),
            "2026-08-26T00:00:00Z",
        )
        .await
        .expect("seed other identity");
        let mut cli_config = CliConfig::load().expect("load config");
        cli_config.active_creator_id = Some("ctr_localother".to_string());
        cli_config.save().expect("save config");

        // Re-run the target name → single match → converge (activate) the target.
        bootstrap_local_creator(Some("Target Name".to_string()))
            .await
            .expect("single match converges");

        let config = CliConfig::load().expect("reload config");
        assert_eq!(
            config.active_creator_id.as_deref(),
            Some(target_id.as_str()),
            "matched identity activated (session selection)"
        );
    }

    /// Nameless `--persistent` converges the already-active persistent
    /// identity (AR-89 #2) — no new mint; a missing workspace row is repaired.
    #[tokio::test]
    async fn bootstrap_local_creator_nameless_converges_active_persistent() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("Active One".to_string()))
            .await
            .expect("mint active identity");
        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1);
        let active_id = identities[0].creator_id.clone();

        // Simulate the partial: delete the workspace row.
        let config = CliConfig::load().expect("reload config");
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        sqlx::query("DELETE FROM creators WHERE creator_id = ?")
            .bind(&active_id)
            .execute(&workspace_pool)
            .await
            .expect("delete workspace row");

        // Nameless re-run → converge the active persistent identity (repair).
        bootstrap_local_creator(None)
            .await
            .expect("nameless converges active persistent");

        let after = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(after.len(), 1, "no new mint");
        assert_eq!(after[0].creator_id, active_id);

        let row_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
        )
        .bind(&active_id)
        .fetch_one(&workspace_pool)
        .await
        .expect("query workspace creators row");
        assert_eq!(row_exists, 1, "workspace row repaired");
    }
}
