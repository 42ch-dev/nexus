//! `nexus42 creator run <preset_id>` — generic preset runner (V1.45 §4).
//!
//! Replaces the V1.33–V1.44 bespoke subcommand dispatch (start, continue,
//! stage, resume, and the chapter-audit / master-review subcommands) with a
//! entry point:
//!
//! `nexus42 creator run <preset_id> [<work_id>] [global flags] [preset args...]`
//!
//! FL-E stage-advance presets (`research`, `novel-writing`, `reflection-loop`,
//! `kb-extract`) are dispatched to `stage_advance`; all other presets are
//! scheduled directly via the daemon Local API.

use crate::config::CliConfig;
use crate::errors::Result;
use nexus_contracts::local::orchestration::preset::{PresetCliArg, PresetCliArgType};
use nexus_contracts::local::orchestration::stage_index;
use nexus_contracts::local::schedule::http::AddScheduleRequest;
use nexus_orchestration::preset::validation::stage_for_preset;
use nexus_orchestration::stage_gates::{self, WorkFields, WorkStageState};

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
    let resolved_work_id = super::work_utils::resolve_active_work_id(&client, work_id).await?;

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

    // QC3 W-1: try O(1) direct path lookup before falling back to full scan.
    let loaded =
        match nexus_orchestration::preset::lookup_preset_by_id(&preset_id, &nexus_home, &caps) {
            Some(loaded) => loaded,
            None => nexus_orchestration::preset::resolve_preset(&preset_id, &nexus_home, &caps)
                .map_err(|e| {
                    crate::errors::CliError::Config(format!(
                        "Unknown preset '{preset_id}': {e}. \
                     Run `nexus42 creator presets list` to see available presets."
                    ))
                })?,
        };

    // Parse trailing args against preset.cli_args declarations.
    let mut input = parse_preset_cli_args(&loaded.manifest.preset.cli_args, &extra)?;

    // C-1 fix: inject resolved work_id so the daemon can evaluate gates and
    // execute the preset. Gated presets (all three audit presets + the novel
    // master-review preset) return 422 when input["work_id"] is absent.
    // work_id is NOT in RESERVED_INPUT_KEYS (schedules.rs:72).
    if let serde_json::Value::Object(ref mut map) = input {
        map.entry("work_id".to_string())
            .or_insert(serde_json::Value::String(resolved_work_id.clone()));
    }

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
        .post::<serde_json::Value, _>("/v1/local/orchestration/schedules", &request)
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

/// Parse trailing CLI args against `preset.cli_args` declarations (V1.45 §3.3).
///
/// Returns a JSON object mapping arg names to coerced values, suitable for
/// `AddScheduleRequest.input`.
///
/// Supports both `--flag value` (space-separated) and `--flag=value` (inline
/// equals) syntaxes. Boolean flags accept `--flag` (presence = true) and
/// `--flag=true`/`--flag=false` (explicit).
fn parse_preset_cli_args(cli_args: &[PresetCliArg], raw: &[String]) -> Result<serde_json::Value> {
    use std::collections::HashMap;

    // If the preset declares no cli_args, ignore trailing args silently.
    if cli_args.is_empty() {
        return Ok(serde_json::json!({}));
    }

    // Build a lookup: kebab-name → PresetCliArg
    let lookup: HashMap<&str, &PresetCliArg> =
        cli_args.iter().map(|a| (a.name.as_str(), a)).collect();

    // Parse `--name value` / `--name=value` pairs from the raw trailing args.
    let mut parsed: HashMap<String, serde_json::Value> = HashMap::new();
    let mut i = 0;
    while i < raw.len() {
        let token = &raw[i];
        let stripped = token.strip_prefix("--").ok_or_else(|| {
            crate::errors::CliError::Config(format!(
                "Unexpected positional '{token}' in preset args. \
                     Preset-specific args must use --flag syntax."
            ))
        })?;

        // Split on `=` to support `--flag=value` inline syntax (QC1 W-2).
        let (name, inline_value) = match stripped.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (stripped, None),
        };

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
                // Boolean flags: `--flag` (presence = true) or `--flag=true/false`.
                match inline_value {
                    Some(v) => {
                        let b: bool = v.parse().map_err(|_| {
                            crate::errors::CliError::Config(format!(
                                "Flag '--{name}' expects a boolean (true/false); got '{v}'"
                            ))
                        })?;
                        parsed.insert(arg.name.clone(), serde_json::json!(b));
                    }
                    None => {
                        parsed.insert(arg.name.clone(), serde_json::json!(true));
                    }
                }
                i += 1; // boolean: always 1 token (--flag or --flag=value)
            }
            PresetCliArgType::Integer => {
                let (val, advance) = if let Some(v) = inline_value {
                    (v, 1) // --flag=value: 1 token
                } else {
                    let next = raw.get(i + 1).cloned().ok_or_else(|| {
                        crate::errors::CliError::Config(format!(
                            "Flag '--{name}' requires an integer value"
                        ))
                    })?;
                    (next, 2) // --flag value: 2 tokens
                };
                let n: i64 = val.parse().map_err(|_| {
                    crate::errors::CliError::Config(format!(
                        "Flag '--{name}' expects an integer; got '{val}'"
                    ))
                })?;
                parsed.insert(arg.name.clone(), serde_json::json!(n));
                i += advance;
            }
            PresetCliArgType::String => {
                let (val, advance) = if let Some(v) = inline_value {
                    (v, 1) // --flag=value: 1 token
                } else {
                    let next = raw.get(i + 1).cloned().ok_or_else(|| {
                        crate::errors::CliError::Config(format!(
                            "Flag '--{name}' requires a string value"
                        ))
                    })?;
                    (next, 2) // --flag value: 2 tokens
                };
                parsed.insert(arg.name.clone(), serde_json::json!(val));
                i += advance;
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
              Hint: re-run `nexus42 creator works status {work_id}` to inspect,\n\
              or re-seed the work via `nexus42 creator bootstrap --init-preset novel-project-init`.\n\
              (V1.45: `creator run status` → `creator works status`; `creator run start` → `creator bootstrap`.)"
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

                // Attempt to restore previous stage state (QC3 W-2: propagate
                // rollback failure so operators can detect orphaned state).
                let rollback = serde_json::json!({
                    "current_stage": current_stage,
                    "stage_status": current_status,
                });
                let rollback_result = client
                    .patch::<serde_json::Value, _>(&format!("/v1/local/works/{work_id}"), &rollback)
                    .await;

                return Err(match rollback_result {
                    Ok(_) => crate::errors::CliError::Other(format!(
                        "FL_E_SCHEDULE_CREATE_FAILED: failed to create stage schedule for '{target_stage}': {e}. \
                         Stage advance rolled back to '{current_stage}' ({current_status})."
                    )),
                    Err(rollback_err) => crate::errors::CliError::Other(format!(
                        "schedule creation failed AND stage rollback failed: \
                         schedule_error={e}; rollback_error={rollback_err}; \
                         Work {work_id} may be in inconsistent state — \
                         run `nexus42 creator works status {work_id}` to inspect"
                    )),
                });
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
    // R-V144P0-008: typed error variant (tests errors module, kept after W-1
    // legacy deletion since the error type lives in errors.rs)
    // -----------------------------------------------------------------------
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

    // -----------------------------------------------------------------------
    // V1.45 B1 (QC1 W-2): parse_preset_cli_args --flag=value support
    // -----------------------------------------------------------------------

    /// Helper: build a PresetCliArg with sensible defaults.
    fn make_arg(name: &str, ty: PresetCliArgType) -> PresetCliArg {
        PresetCliArg {
            name: name.to_string(),
            r#type: ty,
            required: false,
            default: None,
            description: String::new(),
        }
    }

    #[test]
    fn parse_preset_cli_args_inline_equals_integer() {
        let cli_args = vec![
            make_arg("chapter", PresetCliArgType::Integer),
            make_arg("volume", PresetCliArgType::Integer),
        ];
        let raw = vec!["--chapter=5".to_string(), "--volume=2".to_string()];
        let result = parse_preset_cli_args(&cli_args, &raw).unwrap();
        assert_eq!(result["chapter"], 5);
        assert_eq!(result["volume"], 2);
    }

    #[test]
    fn parse_preset_cli_args_inline_equals_string() {
        let cli_args = vec![make_arg("note", PresetCliArgType::String)];
        let raw = vec!["--note=hello world".to_string()];
        let result = parse_preset_cli_args(&cli_args, &raw).unwrap();
        assert_eq!(result["note"], "hello world");
    }

    #[test]
    fn parse_preset_cli_args_inline_equals_boolean_true() {
        let cli_args = vec![make_arg("verbose", PresetCliArgType::Boolean)];
        let raw = vec!["--verbose=true".to_string()];
        let result = parse_preset_cli_args(&cli_args, &raw).unwrap();
        assert_eq!(result["verbose"], true);
    }

    #[test]
    fn parse_preset_cli_args_inline_equals_boolean_false() {
        let cli_args = vec![make_arg("verbose", PresetCliArgType::Boolean)];
        let raw = vec!["--verbose=false".to_string()];
        let result = parse_preset_cli_args(&cli_args, &raw).unwrap();
        assert_eq!(result["verbose"], false);
    }

    #[test]
    fn parse_preset_cli_args_mixed_inline_and_space_syntax() {
        let cli_args = vec![
            make_arg("chapter", PresetCliArgType::Integer),
            make_arg("note", PresetCliArgType::String),
            make_arg("verbose", PresetCliArgType::Boolean),
        ];
        let raw = vec![
            "--chapter=3".to_string(),
            "--note=my note".to_string(),
            "--verbose".to_string(),
        ];
        let result = parse_preset_cli_args(&cli_args, &raw).unwrap();
        assert_eq!(result["chapter"], 3);
        assert_eq!(result["note"], "my note");
        assert_eq!(result["verbose"], true);
    }

    #[test]
    fn parse_preset_cli_args_space_syntax_regression() {
        // Regression: ensure the original space-separated syntax still works.
        let cli_args = vec![make_arg("chapter", PresetCliArgType::Integer)];
        let raw = vec!["--chapter".to_string(), "7".to_string()];
        let result = parse_preset_cli_args(&cli_args, &raw).unwrap();
        assert_eq!(result["chapter"], 7);
    }

    #[test]
    fn parse_preset_cli_args_boolean_presence_regression() {
        // Regression: bare --flag without value still means true.
        let cli_args = vec![make_arg("dry-run", PresetCliArgType::Boolean)];
        let raw = vec!["--dry-run".to_string()];
        let result = parse_preset_cli_args(&cli_args, &raw).unwrap();
        assert_eq!(result["dry-run"], true);
    }

    // -----------------------------------------------------------------------
    // V1.45 B1 (C-1): work_id injection into AddScheduleRequest.input
    // -----------------------------------------------------------------------

    #[test]
    fn work_id_injection_into_parsed_input() {
        // Simulate the injection logic from handle_run (C-1 fix).
        let mut input = serde_json::json!({"chapter": 5, "volume": 1});
        let resolved_work_id = "wrk_abc123";

        if let serde_json::Value::Object(ref mut map) = input {
            map.entry("work_id".to_string())
                .or_insert(serde_json::Value::String(resolved_work_id.to_string()));
        }

        assert_eq!(
            input["work_id"], "wrk_abc123",
            "work_id must be present in input"
        );
        assert_eq!(input["chapter"], 5, "existing args must be preserved");
        assert_eq!(input["volume"], 1, "existing args must be preserved");
    }

    #[test]
    fn work_id_injection_does_not_override_explicit() {
        // If the preset's cli_args somehow already includes work_id, the
        // injection must NOT override it (or_insert semantics).
        let mut input = serde_json::json!({"work_id": "wrk_explicit"});
        let resolved_work_id = "wrk_abc123";

        if let serde_json::Value::Object(ref mut map) = input {
            map.entry("work_id".to_string())
                .or_insert(serde_json::Value::String(resolved_work_id.to_string()));
        }

        assert_eq!(
            input["work_id"], "wrk_explicit",
            "explicit work_id must not be overridden"
        );
    }

    #[test]
    fn work_id_injection_into_empty_input() {
        // Presets with no cli_args produce an empty object — work_id must
        // still be injected.
        let mut input = serde_json::json!({});
        let resolved_work_id = "wrk_xyz";

        if let serde_json::Value::Object(ref mut map) = input {
            map.entry("work_id".to_string())
                .or_insert(serde_json::Value::String(resolved_work_id.to_string()));
        }

        assert_eq!(input["work_id"], "wrk_xyz");
    }

    // -----------------------------------------------------------------------
    // V1.45 B1 (QC3 W-2): stage_advance rollback error propagation
    // -----------------------------------------------------------------------

    /// When both schedule creation and rollback fail, the error surfaces both
    /// failure messages (QC3 W-2). Uses wiremock to mock the daemon.
    #[tokio::test]
    async fn rollback_failure_surfaces_both_errors() {
        use wiremock::matchers::{body_string_contains, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock = MockServer::start().await;
        let work_id = "wrk_rollback_test";

        // GET /v1/local/works/{work_id} — work at intake/complete.
        let work_body = serde_json::json!({
            "work_id": work_id,
            "current_stage": "intake",
            "stage_status": "complete",
            "intake_status": "complete",
            "creative_brief": "test brief",
            "inspiration_log": "[]",
            "title": "Test Work",
        });
        Mock::given(method("GET"))
            .and(path(format!("/v1/local/works/{work_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(work_body))
            .mount(&mock)
            .await;

        // Stage-advance PATCH — body contains "active" → 200.
        let stage_ok_body = serde_json::json!({
            "work_id": work_id,
            "current_stage": "research",
            "stage_status": "active",
            "title": "Test Work",
        });
        Mock::given(method("PATCH"))
            .and(path(format!("/v1/local/works/{work_id}")))
            .and(body_string_contains("active"))
            .respond_with(ResponseTemplate::new(200).set_body_json(stage_ok_body))
            .mount(&mock)
            .await;

        // Rollback PATCH — body contains "complete" → 500.
        Mock::given(method("PATCH"))
            .and(path(format!("/v1/local/works/{work_id}")))
            .and(body_string_contains("complete"))
            .respond_with(ResponseTemplate::new(500).set_body_string("rollback daemon error"))
            .mount(&mock)
            .await;

        // POST schedule — fail with 500.
        Mock::given(method("POST"))
            .and(path("/v1/local/orchestration/schedules"))
            .respond_with(
                ResponseTemplate::new(500).set_body_string("daemon schedule creation failed"),
            )
            .mount(&mock)
            .await;

        let client = crate::api::DaemonClient::new(&mock.uri());
        let config = CliConfig {
            active_creator_id: Some("creator_test".to_string()),
            daemon_url: mock.uri(),
            ..Default::default()
        };

        let result = stage_advance(
            work_id, "research", false, false, None, false, &config, &client,
        )
        .await;

        let err = result.expect_err(
            "stage_advance should fail when schedule creation fails and rollback also fails",
        );
        let err_msg = err.to_string();

        assert!(
            err_msg.contains("schedule creation failed AND stage rollback failed"),
            "error must indicate both failures: {err_msg}"
        );
        assert!(
            err_msg.contains("schedule_error"),
            "error must contain schedule_error: {err_msg}"
        );
        assert!(
            err_msg.contains("rollback_error"),
            "error must contain rollback_error: {err_msg}"
        );
        assert!(
            err_msg.contains(work_id),
            "error must contain work_id: {err_msg}"
        );
    }
}
