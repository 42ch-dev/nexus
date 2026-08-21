//! `nexus42 preset` — canonical preset CLI group (PL-5, AR-24, AR-25).
//!
//! Top-level developer-facing surface for presets:
//! `list|show|validate|scaffold|run|trigger`.
//!
//! - `validate` reuses today's validator job — daemon-backed default
//!   (`POST /v1/daemon/presets:validate`) + `--offline` in-process core
//!   (`validate_preset_offline`, moved here from `system`; V1.153 P3).
//! - `scaffold` reuses `POST /v1/daemon/presets` (`scaffold_preset`).
//! - `run` reuses the same daemon-API path `creator run` drives
//!   (`commands/creator/run.rs` `handle_run`) — no second orchestration
//!   engine (PL-5).
//! - `show <id>` fetches the AR-20 profile (daemon-backed) and prints lanes +
//!   orchestration fields; declared signals are labeled **Declared, not
//!   delivered** (AR-25, locked trigger-lane vocabulary).
//! - `trigger <id>` prints trigger-lane classification only — never cron
//!   authoring (cron authoring stays `creator works cron` until P2 owns the
//!   UI; PL-5 / PL-18).
//! - `--json` = daemon DTO verbatim (camelCase); no CLI-local renaming
//!   (AR-25).

#![allow(clippy::print_literal)]

use crate::api::models::ScaffoldPresetRequest;
use crate::config::CliConfig;
use crate::errors::Result;
use crate::CliError;
use clap::Subcommand;
use nexus_contracts::local::orchestration::http::{
    PresetProfileExitWhen, PresetProfileLanes, PresetProfileNext, PresetProfileResponse,
};

const ORCHESTRATION_BASE: &str = "/v1/daemon/orchestration";

#[derive(Debug, Subcommand)]
pub enum PresetCommand {
    /// List all discoverable presets (embedded + user + system)
    List {
        /// Filter by `run_intent` (e.g. `work_init`, `knowledge_ingest`)
        #[arg(long)]
        intent: Option<String>,
        /// Emit machine-readable JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Print the profile for a preset (lanes + orchestration fields)
    Show {
        /// Preset ID (embedded, user, or `_system.` qualified)
        id: String,
        /// Emit the daemon profile DTO verbatim (camelCase)
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Validate a preset YAML/bundle at a given path
    Validate {
        /// Path to preset.yaml (or bundle directory)
        path: String,
        /// Emit machine-readable JSON
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Validate in-process via the shared validator core — no daemon
        /// required (V1.153 P3: `nexus-runtime` does not serve the daemon
        /// HTTP router). Default: daemon-backed `POST /v1/daemon/presets:validate`.
        #[arg(long, default_value_t = false)]
        offline: bool,
    },
    /// Generate a strategy bundle from templates
    Scaffold {
        /// Name for the new user preset
        name: String,
        /// Emit machine-readable JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Start a session for a world (same daemon-API path as `creator run`)
    Run {
        #[command(flatten)]
        command: crate::commands::creator::run::RunCommand,
    },
    /// Print trigger-lane classification only (never cron authoring)
    Trigger {
        /// Preset ID (embedded, user, or `_system.` qualified)
        id: String,
        /// Emit machine-readable JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Run the `nexus42 preset` command group.
///
/// # Errors
///
/// Returns an error if the delegated command fails.
pub async fn run(cmd: PresetCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        PresetCommand::List { intent, json } => list_presets(config, intent.as_deref(), json).await,
        PresetCommand::Show { id, json } => show_preset(config, &id, json).await,
        PresetCommand::Validate {
            path,
            json,
            offline,
        } => validate_preset(config, &path, json, offline).await,
        PresetCommand::Scaffold { name, json } => scaffold_preset(config, &name, json).await,
        PresetCommand::Run { command } => {
            crate::commands::creator::run::handle_run(command, config).await
        }
        PresetCommand::Trigger { id, json } => trigger_preset(config, &id, json).await,
    }
}

/// List all discoverable presets (embedded + user + system).
///
/// Moved from `system preset list` (V1.153) — the shared listing job, not
/// re-implemented (AR-24).
///
/// The display list is built from the grouped management endpoint
/// (`GET /v1/daemon/presets` — embedded + system + user groups,
/// W-002/F-001), so user presets appear and each row is labeled by its real
/// source. `--intent` filtering runs across all groups.
async fn list_presets(
    config: &CliConfig,
    intent_filter: Option<&str>,
    json_output: bool,
) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);

    // Grouped management endpoint: embedded + system + user (W-002/F-001).
    // A failure here is a real daemon error — surface it instead of silently
    // degrading the list to no presets (F-001 reliability nit).
    let mgmt_resp: crate::api::models::ListPresetsGroupedResponse = client.list_presets().await?;

    let presets: Vec<(String, String, Vec<String>)> = build_preset_rows(&mgmt_resp, intent_filter);

    if json_output {
        let output: Vec<serde_json::Value> = presets
            .iter()
            .map(|(id, source, intents)| {
                serde_json::json!({
                    "id": id,
                    "source": source,
                    "run_intents": intents,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if presets.is_empty() {
        println!("No presets found.");
    } else {
        println!("Presets:");
        for (id, source, intents) in &presets {
            let intents_str = if intents.is_empty() {
                String::new()
            } else {
                format!(" [{}]", intents.join(", "))
            };
            println!("  {id} ({source}){intents_str}");
        }
        println!("\n{} preset(s)", presets.len());
    }

    Ok(())
}

/// Flatten the grouped management response into display rows
/// `(id, source, run_intents)`, optionally filtered by `run_intent`
/// (W-002/F-001). Pure over the daemon DTO — hermetically testable.
fn build_preset_rows(
    resp: &crate::api::models::ListPresetsGroupedResponse,
    intent_filter: Option<&str>,
) -> Vec<(String, String, Vec<String>)> {
    let mut presets: Vec<(String, String, Vec<String>)> = Vec::new();

    for group in [&resp.embedded, &resp.system, &resp.user] {
        for summary in group {
            presets.push((
                summary.id.clone(),
                summary.source.clone(),
                summary.run_intents.clone(),
            ));
        }
    }

    if let Some(intent) = intent_filter {
        presets.retain(|(_, _, intents)| intents.iter().any(|i| i == intent));
    }

    presets
}

/// Print the AR-20 profile for a preset (daemon-backed).
///
/// `--json` serializes the daemon DTO verbatim (camelCase, AR-25). Text
/// output names lanes + orchestration fields; declared signals are labeled
/// **Declared, not delivered**.
async fn show_preset(config: &CliConfig, id: &str, json: bool) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);
    let profile: PresetProfileResponse = client
        .get(&format!("{ORCHESTRATION_BASE}/presets/{id}/profile"))
        .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
    } else {
        print!("{}", format_profile_text(&profile));
    }
    Ok(())
}

/// Print trigger-lane classification only (AR-25) — never cron authoring.
async fn trigger_preset(config: &CliConfig, id: &str, json: bool) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);
    let profile: PresetProfileResponse = client
        .get(&format!("{ORCHESTRATION_BASE}/presets/{id}/profile"))
        .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&profile.lanes)?);
    } else {
        print!("{}", format_trigger_text(id, &profile.lanes));
    }
    Ok(())
}

/// Scaffold a user preset bundle from templates (`POST /v1/daemon/presets`).
async fn scaffold_preset(config: &CliConfig, name: &str, json: bool) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);
    let resp = client
        .scaffold_preset(&ScaffoldPresetRequest {
            name: name.to_string(),
        })
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Scaffolded preset '{}' at {}", resp.id, resp.path);
    }
    Ok(())
}

/// Validate a preset YAML/bundle at a given path.
///
/// With `offline = false` (default) validation is delegated to the daemon
/// (`POST /v1/daemon/presets:validate`). With `offline = true` the same
/// checks run in-process via the shared orchestration validator core — no
/// daemon required (V1.153 P3: `nexus-runtime` does not serve the daemon
/// HTTP router, so integrators need a daemon-free path).
///
/// An invalid preset produces a non-zero exit in both modes so scripts
/// (e.g. `strategy-samples/validate.sh`) can rely on the exit code.
///
/// Moved from `system preset validate` (V1.153) — the shared validator job,
/// not re-implemented (AR-24).
async fn validate_preset(
    config: &CliConfig,
    path: &str,
    json_output: bool,
    offline: bool,
) -> Result<()> {
    let resp: serde_json::Value = if offline {
        validate_preset_offline(path)?
    } else {
        let client = crate::api::DaemonClient::from_config(config);
        let body = serde_json::json!({ "path": path });
        client
            .post::<serde_json::Value, _>("/v1/daemon/presets:validate", &body)
            .await?
    };

    let valid = resp
        .get("valid")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    if json_output {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        print_validate_verdict(&resp);
    }

    if valid {
        Ok(())
    } else {
        let error_count = resp
            .get("errors")
            .and_then(|v| v.as_array())
            .map_or(0, std::vec::Vec::len);
        Err(CliError::Other(format!(
            "preset validation failed: {error_count} error(s)"
        )))
    }
}

/// Print the human-readable verdict for a validate response
/// (`{valid, id, version, state_count, errors, warnings}` — the daemon
/// response shape; `validate_preset_offline` produces the same shape).
fn print_validate_verdict(resp: &serde_json::Value) {
    let valid = resp
        .get("valid")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let errors: Vec<&str> = resp
        .get("errors")
        .and_then(|v| v.as_array())
        .map_or_else(Vec::new, |a| {
            a.iter().filter_map(serde_json::Value::as_str).collect()
        });
    let warnings: Vec<&str> = resp
        .get("warnings")
        .and_then(|v| v.as_array())
        .map_or_else(Vec::new, |a| {
            a.iter().filter_map(serde_json::Value::as_str).collect()
        });

    if valid {
        println!("✓ Valid preset");
    } else {
        println!("✗ Invalid preset ({} error(s))", errors.len());
    }
    for error in &errors {
        println!("  - {error}");
    }
    for warning in &warnings {
        println!("  - warning: {warning}");
    }
}

/// Run preset validation in-process, mirroring the daemon's
/// `POST /v1/daemon/presets:validate` composition
/// (`loader_validate_manifest_compat` + `validate_path_safety` +
/// `validate_preset_semantic` + `validate_assets_in_bundle`) so the
/// offline verdict is identical to the daemon-backed one.
///
/// The daemon handler — not `load_preset` — is deliberately the reference
/// composition: it parses the manifest directly and treats every
/// error-severity diagnostic as a failure, whereas `load_preset`
/// downgrades capability-arg-drift errors to warnings. Mirroring the
/// handler keeps `--offline` and daemon-backed validation answering the
/// same question with the same answer (and the same response shape).
///
/// `path` may be a bundle directory (containing `preset.yaml`), a
/// `preset.yaml` file (asset checks run against its parent), or a
/// standalone YAML file (no asset checks — same as the daemon's
/// `infer_bundle_root`).
///
/// Moved from `system` (V1.153 P3) — the shared validator core, not
/// re-implemented (AR-24).
fn validate_preset_offline(path: &str) -> Result<serde_json::Value> {
    use nexus_orchestration::preset::{
        loader_validate_manifest_compat, validate_assets_in_bundle, validate_path_safety,
        validate_preset_semantic, yaml_value_depth, DiagnosticSeverity,
        ValidationResult as PresetValidationResult, DEFAULT_MAX_YAML_DEPTH, DEFAULT_MAX_YAML_SIZE,
    };
    use nexus_orchestration::CapabilityRegistry;

    // Resolve the target file + optional bundle root (mirrors the daemon's
    // `infer_bundle_root`: only a file literally named `preset.yaml` has a
    // bundle root; a directory argument points at its preset.yaml).
    let file_path = std::path::Path::new(path);
    let (preset_yaml_path, bundle_root) = if file_path.is_dir() {
        (file_path.join("preset.yaml"), Some(file_path.to_path_buf()))
    } else {
        let is_bundle_manifest = file_path
            .file_name()
            .is_some_and(|name| name == "preset.yaml");
        (
            file_path.to_path_buf(),
            is_bundle_manifest
                .then(|| file_path.parent().map(std::path::Path::to_path_buf))
                .flatten(),
        )
    };

    if !preset_yaml_path.exists() {
        return Err(CliError::Other(format!(
            "File not found: {}",
            preset_yaml_path.display()
        )));
    }

    // Size limit (same 1 MiB limit the loader/daemon enforce).
    let metadata = std::fs::metadata(&preset_yaml_path)?;
    if metadata.len() > DEFAULT_MAX_YAML_SIZE as u64 {
        return Err(CliError::Other(format!(
            "Preset YAML exceeds maximum size ({} bytes, limit is {} bytes)",
            metadata.len(),
            DEFAULT_MAX_YAML_SIZE
        )));
    }

    let yaml = std::fs::read_to_string(&preset_yaml_path)?;

    // Parse + depth check (same path as `load_preset_from_str_with_limits`).
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&yaml)
        .map_err(|e| CliError::Other(format!("preset YAML parse error: {e}")))?;
    let depth = yaml_value_depth(&yaml_value);
    if depth > DEFAULT_MAX_YAML_DEPTH {
        return Err(CliError::Other(format!(
            "Nesting depth ({depth}) exceeds maximum ({DEFAULT_MAX_YAML_DEPTH})"
        )));
    }
    let manifest: nexus_contracts::local::orchestration::preset::PresetManifest =
        serde_yaml::from_value(yaml_value)
            .map_err(|e| CliError::Other(format!("preset structural error: {e}")))?;

    let caps = CapabilityRegistry::with_builtins();
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // C2: loader-equivalent structural checks (same defects the runtime
    //     loader would reject).
    for problem in loader_validate_manifest_compat(&manifest, &caps) {
        errors.push(format!("{}: {}", problem.path, problem.error));
    }

    // C3 + A5 + A3: shared validation facade. Bundle asset checks only run
    //     when the target resolves to a bundle root.
    let asset_result = bundle_root
        .as_deref()
        .map_or_else(PresetValidationResult::default, |root| {
            validate_assets_in_bundle(&manifest, root)
        });
    for diagnostic in validate_path_safety(&manifest)
        .diagnostics
        .iter()
        .chain(
            validate_preset_semantic(&manifest, &caps)
                .diagnostics
                .iter(),
        )
        .chain(asset_result.diagnostics.iter())
    {
        match diagnostic.severity {
            DiagnosticSeverity::Error => {
                errors.push(format!("{}: {}", diagnostic.path, diagnostic.message));
            }
            DiagnosticSeverity::Warning => {
                warnings.push(format!("{}: {}", diagnostic.path, diagnostic.message));
            }
        }
    }

    // Same response shape as the daemon endpoint (warnings omitted when
    // empty, matching the daemon's `skip_serializing_if`).
    let mut resp = serde_json::Map::new();
    resp.insert("valid".to_string(), serde_json::json!(errors.is_empty()));
    resp.insert("id".to_string(), serde_json::json!(manifest.preset.id));
    resp.insert(
        "version".to_string(),
        serde_json::json!(manifest.preset.version),
    );
    resp.insert(
        "state_count".to_string(),
        serde_json::json!(manifest.states.len()),
    );
    resp.insert("errors".to_string(), serde_json::json!(errors));
    if !warnings.is_empty() {
        resp.insert("warnings".to_string(), serde_json::json!(warnings));
    }
    Ok(serde_json::Value::Object(resp))
}

// ---------------------------------------------------------------------------
// Text renderers (pure over the daemon DTO — hermetically testable)
// ---------------------------------------------------------------------------

/// Render the human-readable profile text for `preset show`.
///
/// Declared signals are labeled **Declared, not delivered** (AR-25, locked
/// trigger-lane vocabulary).
fn format_profile_text(profile: &PresetProfileResponse) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "Preset: {} (v{})", profile.id, profile.version);
    let _ = writeln!(out, "Source hash: {}", profile.source_hash);
    let _ = writeln!(out);
    let _ = writeln!(out, "Trigger lanes:");
    write_lanes(&mut out, &profile.lanes);
    let _ = writeln!(out);
    let _ = writeln!(out, "States:");
    if profile.states.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for state in &profile.states {
            let _ = writeln!(out, "  {}", state.id);
            if let Some(desc) = &state.description {
                let _ = writeln!(out, "    description: {desc}");
            }
            if !state.enter.is_empty() {
                let _ = writeln!(out, "    enter:");
                for action in &state.enter {
                    let _ = writeln!(out, "      - {}: {}", action.kind, action.name);
                }
            }
            if let Some(exit_when) = &state.exit_when {
                let _ = writeln!(out, "    exit_when: {}", format_exit_when(exit_when));
            }
            if let Some(next) = &state.next {
                let _ = writeln!(out, "    next: {}", format_next(next));
            }
            let _ = writeln!(out, "    terminal: {}", yes_no(state.terminal));
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Roles:");
    if profile.roles.is_empty() {
        let _ = writeln!(out, "  (single-agent mode)");
    } else {
        for role in &profile.roles {
            let _ = writeln!(out, "  {}", role.id);
            let _ = writeln!(out, "    description: {}", role.description);
            let _ = writeln!(out, "    system_prompt_file: {}", role.system_prompt_file);
            if !role.recommended_skills.is_empty() {
                let _ = writeln!(
                    out,
                    "    recommended_skills: {}",
                    role.recommended_skills.join(", ")
                );
            }
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Required capabilities:");
    if profile.required_capabilities.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for cap in &profile.required_capabilities {
            let _ = writeln!(out, "  - {cap}");
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Declared signals — Declared, not delivered:");
    if profile.signals.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for signal in &profile.signals {
            let mut line = format!("  - {} (action: {})", signal.name, signal.action);
            if let Some(target) = &signal.target {
                let _ = write!(line, ", target: {target}");
            }
            let _ = writeln!(out, "{line}");
        }
    }
    out
}

/// Render the trigger-lane classification only (AR-25) — never cron
/// authoring.
fn format_trigger_text(id: &str, lanes: &PresetProfileLanes) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "Trigger lanes for {id}:");
    write_lanes(&mut out, lanes);
    out
}

/// Write the four locked trigger-lane rows (PL-3 vocabulary).
fn write_lanes(out: &mut String, lanes: &PresetProfileLanes) {
    use std::fmt::Write as _;
    let _ = writeln!(out, "  cron:        {}", yes_no(lanes.cron));
    let _ = writeln!(out, "  wall-clock:  {}", yes_no(lanes.wall_clock));
    let _ = writeln!(out, "  session:     {}", yes_no(lanes.session));
    let _ = writeln!(out, "  direct:      {}", yes_no(lanes.direct));
}

const fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// Render one exit condition (`llm_judge` / `rule` / `graph_complete` /
/// `manual` / `timer`).
fn format_exit_when(exit_when: &PresetProfileExitWhen) -> String {
    match exit_when.kind.as_str() {
        "llm_judge" => {
            let mut s = "llm_judge".to_string();
            if let Some(t) = &exit_when.template_file {
                let _ = std::fmt::Write::write_fmt(&mut s, format_args!(" (template: {t})"));
            }
            if let Some(j) = &exit_when.judge_capability {
                let _ = std::fmt::Write::write_fmt(&mut s, format_args!(" (judge: {j})"));
            }
            if let Some(m) = &exit_when.min_interval {
                let _ = std::fmt::Write::write_fmt(&mut s, format_args!(" (min_interval: {m})"));
            }
            s
        }
        "timer" => exit_when
            .duration
            .as_ref()
            .map_or_else(|| "timer".to_string(), |d| format!("timer (duration: {d})")),
        other => other.to_string(),
    }
}

/// Render one next-transition form (`linear` / `goNogo` / `labeled` /
/// `conditional` / `branches`).
fn format_next(next: &PresetProfileNext) -> String {
    match next.kind.as_str() {
        "linear" => next
            .target
            .as_ref()
            .map_or_else(|| "linear".to_string(), |t| format!("linear -> {t}")),
        "goNogo" => format!(
            "goNogo (go: {}, nogo: {})",
            next.go.as_deref().unwrap_or_default(),
            next.nogo.as_deref().unwrap_or_default()
        ),
        "labeled" => {
            let edges: Vec<String> = next
                .labeled
                .iter()
                .map(|e| format!("{} -> {}", e.label, e.target))
                .collect();
            format!("labeled [{}]", edges.join(", "))
        }
        "conditional" | "branches" => {
            let rules: Vec<String> = next
                .rules
                .iter()
                .chain(next.branches.iter())
                .map(|r| format!("{} -> {}", r.when, r.target))
                .collect();
            let mut s = format!("{} [{}]", next.kind, rules.join(", "));
            if let Some(d) = &next.default {
                let _ = std::fmt::Write::write_fmt(&mut s, format_args!(" (default: {d})"));
            }
            s
        }
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use clap::Parser;
    use nexus_contracts::local::orchestration::http::{PresetProfileSignal, PresetProfileState};

    /// Wrapper for parsing `PresetCommand` in tests.
    #[derive(Debug, clap::Parser)]
    #[command(subcommand_required = true, name = "preset")]
    struct PresetCli {
        #[command(subcommand)]
        command: PresetCommand,
    }

    #[test]
    fn preset_six_subcommands_parse() {
        for argv in [
            &["preset", "list"][..],
            &["preset", "show", "novel-writing"][..],
            &["preset", "validate", "some/path"][..],
            &["preset", "scaffold", "my-strategy"][..],
            &["preset", "run", "novel-writing"][..],
            &["preset", "trigger", "novel-writing"][..],
        ] {
            PresetCli::try_parse_from(argv)
                .unwrap_or_else(|e| panic!("{argv:?} should parse: {e}"));
        }
    }

    #[test]
    fn preset_show_json_flag_parses() {
        let cmd = PresetCli::try_parse_from(["preset", "show", "novel-writing", "--json"]).unwrap();
        match cmd.command {
            PresetCommand::Show { id, json } => {
                assert_eq!(id, "novel-writing");
                assert!(json);
            }
            other => panic!("expected show, got {other:?}"),
        }
    }

    #[test]
    fn preset_validate_offline_flag_parses() {
        let cmd =
            PresetCli::try_parse_from(["preset", "validate", "some/path", "--offline"]).unwrap();
        match cmd.command {
            PresetCommand::Validate {
                path,
                json,
                offline,
            } => {
                assert_eq!(path, "some/path");
                assert!(!json);
                assert!(offline);
            }
            other => panic!("expected validate, got {other:?}"),
        }
    }

    #[test]
    fn preset_trigger_json_flag_parses() {
        let cmd = PresetCli::try_parse_from(["preset", "trigger", "_system.maintenance", "--json"])
            .unwrap();
        match cmd.command {
            PresetCommand::Trigger { id, json } => {
                assert_eq!(id, "_system.maintenance");
                assert!(json);
            }
            other => panic!("expected trigger, got {other:?}"),
        }
    }

    #[test]
    fn preset_scaffold_parses() {
        let cmd = PresetCli::try_parse_from(["preset", "scaffold", "my-strategy"]).unwrap();
        match cmd.command {
            PresetCommand::Scaffold { name, json } => {
                assert_eq!(name, "my-strategy");
                assert!(!json);
            }
            other => panic!("expected scaffold, got {other:?}"),
        }
    }

    #[test]
    fn preset_run_flattens_creator_run_command() {
        let cmd = PresetCli::try_parse_from(["preset", "run", "novel-writing", "wrk_abc"]).unwrap();
        match cmd.command {
            PresetCommand::Run { command } => {
                assert_eq!(command.preset_id, "novel-writing");
                assert_eq!(command.work_id.as_deref(), Some("wrk_abc"));
            }
            other => panic!("expected run, got {other:?}"),
        }
    }

    #[test]
    fn preset_subcommand_required() {
        let result = PresetCli::try_parse_from(["preset"]);
        assert!(result.is_err());
    }

    // ── W-002/F-001: list rows from the grouped endpoint ─────────────────

    /// Grouped response fixture with all three sources.
    fn grouped_fixture() -> crate::api::models::ListPresetsGroupedResponse {
        crate::api::models::ListPresetsGroupedResponse {
            embedded: vec![crate::api::models::PresetSummary {
                id: "novel-writing".to_string(),
                source: "embedded".to_string(),
                run_intents: vec!["work_init".to_string()],
            }],
            system: vec![crate::api::models::PresetSummary {
                id: "_system.maintenance".to_string(),
                source: "system".to_string(),
                run_intents: vec![],
            }],
            user: vec![crate::api::models::PresetSummary {
                id: "my-strategy".to_string(),
                source: "user".to_string(),
                run_intents: vec!["work_continue".to_string()],
            }],
        }
    }

    #[test]
    fn list_rows_include_user_presets_with_real_source_labels() {
        // W-002/F-001: user presets must appear and each row must carry its
        // real source (not a blanket "embedded").
        let rows = build_preset_rows(&grouped_fixture(), None);
        assert_eq!(rows.len(), 3);
        assert!(rows.contains(&(
            "my-strategy".to_string(),
            "user".to_string(),
            vec!["work_continue".to_string()]
        )));
        assert!(rows.contains(&(
            "_system.maintenance".to_string(),
            "system".to_string(),
            vec![]
        )));
        assert!(rows.contains(&(
            "novel-writing".to_string(),
            "embedded".to_string(),
            vec!["work_init".to_string()]
        )));
    }

    #[test]
    fn list_rows_intent_filter_applies_across_groups() {
        // W-002/F-001: `--intent` filtering must work across all groups.
        let rows = build_preset_rows(&grouped_fixture(), Some("work_continue"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "my-strategy");
        assert_eq!(rows[0].1, "user");

        let rows = build_preset_rows(&grouped_fixture(), Some("work_init"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "novel-writing");

        // No match → empty.
        let rows = build_preset_rows(&grouped_fixture(), Some("nonexistent-intent"));
        assert!(rows.is_empty());
    }

    /// Minimal profile fixture for the text renderers.
    fn sample_profile() -> PresetProfileResponse {
        PresetProfileResponse {
            id: "novel-writing".to_string(),
            version: 9,
            source_hash: "a".repeat(64),
            lanes: PresetProfileLanes {
                cron: false,
                wall_clock: true,
                session: true,
                direct: true,
            },
            states: vec![PresetProfileState {
                id: "outline_chapter".to_string(),
                description: Some("Draft the chapter outline".to_string()),
                enter: vec![],
                exit_when: None,
                next: None,
                terminal: false,
            }],
            roles: vec![],
            required_capabilities: vec!["judge.llm".to_string()],
            signals: vec![PresetProfileSignal {
                name: "pause_signal".to_string(),
                action: "pause".to_string(),
                target: None,
            }],
        }
    }

    #[test]
    fn show_text_labels_declared_signals_not_delivered() {
        let text = format_profile_text(&sample_profile());
        assert!(
            text.contains("Declared, not delivered"),
            "show must carry the honest signal label, got: {text}"
        );
        assert!(text.contains("pause_signal"));
        assert!(text.contains("Trigger lanes:"));
        assert!(text.contains("wall-clock:  yes"));
        assert!(text.contains("Required capabilities:"));
        assert!(text.contains("judge.llm"));
    }

    #[test]
    fn show_text_omits_absent_manifest_fields() {
        let profile = sample_profile();
        // No roles, no enter actions, no exit_when/next → no invented rows.
        let text = format_profile_text(&profile);
        assert!(text.contains("(single-agent mode)"));
        assert!(!text.contains("exit_when:"));
        assert!(!text.contains("next:"));
        assert!(!text.contains("enter:"));
    }

    #[test]
    fn trigger_text_prints_lanes_only() {
        let lanes = PresetProfileLanes {
            cron: true,
            wall_clock: true,
            session: false,
            direct: true,
        };
        let text = format_trigger_text("novel-brainstorm", &lanes);
        assert!(text.contains("Trigger lanes for novel-brainstorm:"));
        assert!(text.contains("cron:        yes"));
        assert!(text.contains("wall-clock:  yes"));
        assert!(text.contains("session:     no"));
        assert!(text.contains("direct:      yes"));
        // Lanes only — no orchestration sections, no cron authoring.
        assert!(!text.contains("States:"));
        assert!(!text.contains("Declared"));
        assert!(!text.contains("cron add"));
    }

    #[test]
    fn show_json_serializes_dto_verbatim_camel_case() {
        let profile = sample_profile();
        let json = serde_json::to_value(&profile).unwrap();
        // camelCase wire keys (AR-25: no CLI-local renaming).
        assert!(json.get("sourceHash").is_some());
        assert!(json.get("source_hash").is_none());
        assert!(json.get("requiredCapabilities").is_some());
        assert!(json.get("wallClock").is_none());
        let lanes = json.get("lanes").unwrap();
        assert!(lanes.get("wallClock").is_some());
        assert!(lanes.get("wall_clock").is_none());
    }

    /// Minimal preset that passes the shared validation facade (semantic +
    /// assets + path safety). The bundle directory name must equal
    /// `preset.id` (`check_bundle_id_vs_directory`).
    const OFFLINE_VALID_YAML: &str = r"
preset:
  id: tiny-valid
  version: 1
  kind: creator
  description: minimal valid fixture for offline validation
  requires_capabilities: []
  run_intents:
    - work_init
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";

    /// Broken copy: `initial` names a state that does not exist (structural
    /// error caught by `loader_validate_manifest_compat`).
    const OFFLINE_BROKEN_YAML: &str = r"
preset:
  id: tiny-broken
  version: 1
  kind: creator
  description: broken fixture for offline validation
  requires_capabilities: []
  run_intents:
    - work_init
  initial: missing_state
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";

    #[test]
    fn offline_validate_accepts_valid_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("tiny-valid");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("preset.yaml"), OFFLINE_VALID_YAML).unwrap();

        let resp = validate_preset_offline(bundle.to_str().unwrap()).unwrap();
        assert_eq!(
            resp["valid"], true,
            "offline validation should accept a valid bundle: {resp}"
        );
        assert_eq!(resp["errors"].as_array().unwrap().len(), 0);
        assert_eq!(resp["id"], "tiny-valid");
        assert_eq!(resp["state_count"], 2);
    }

    #[test]
    fn offline_validate_rejects_broken_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("tiny-broken");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("preset.yaml"), OFFLINE_BROKEN_YAML).unwrap();

        let resp = validate_preset_offline(bundle.to_str().unwrap()).unwrap();
        assert_eq!(resp["valid"], false);
        let errors = resp["errors"].as_array().unwrap();
        assert!(!errors.is_empty());
        assert!(
            errors
                .iter()
                .any(|e| e.as_str().unwrap_or_default().contains("unknown state")),
            "expected an 'unknown state' diagnostic, got: {errors:?}"
        );
    }

    #[test]
    fn offline_validate_accepts_standalone_yaml_file() {
        // A standalone YAML file (not named preset.yaml) skips asset checks.
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("strategy.yaml");
        std::fs::write(&file, OFFLINE_VALID_YAML).unwrap();

        let resp = validate_preset_offline(file.to_str().unwrap()).unwrap();
        assert_eq!(
            resp["valid"], true,
            "standalone YAML file should validate: {resp}"
        );
    }

    #[test]
    fn offline_validate_missing_path_errors() {
        let err = validate_preset_offline("/nonexistent/definitely-missing").unwrap_err();
        assert!(
            err.to_string().contains("File not found"),
            "expected 'File not found' error, got: {err}"
        );
    }
}
