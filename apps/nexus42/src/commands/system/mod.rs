//! `nexus42 system` — System management command group.
//!
//! Implements the `nexus42 system` top-level command with subcommands:
//! - `preset` — Show registered system presets
//! - `version` — Print CLI version info
//! - `doctor` — Diagnostic health checks
//! - `completion` — Shell completion generation
//! - `config` — Configuration file management
//! - `debug` — Internal debugging utilities
//! - `db` — Database status and management
//! - `identity` — Local identity management
//! - `runtime-mode` — Runtime mode management

#![allow(clippy::print_literal)]

pub mod config;
pub mod db;
pub mod debug;
pub mod identity;
pub mod runtime_mode;

use crate::config::CliConfig;
use crate::errors::Result;
use crate::CliError;
use clap::Subcommand;
use clap_complete::Shell;

const ORCHESTRATION_BASE: &str = "/v1/daemon/orchestration";

#[derive(Debug, Subcommand)]
pub enum SystemCommand {
    /// Show registered system presets
    Preset {
        #[command(subcommand)]
        command: SystemPresetSubcommand,
    },

    /// Print CLI version info
    Version,

    /// Diagnostic health checks
    Doctor,

    /// Generate shell completion script
    Completion {
        /// Shell type (bash, zsh, fish, elvish, powershell)
        shell: String,
    },

    /// Configuration file management
    Config {
        #[command(subcommand)]
        command: config::ConfigCommand,
    },

    /// Internal debugging utilities
    Debug {
        #[command(subcommand)]
        command: debug::DebugCommand,
    },

    /// Database status and management
    Db {
        #[command(subcommand)]
        command: db::DbCommand,
    },

    /// Local identity management
    Identity {
        #[command(subcommand)]
        command: identity::IdentityCommand,
    },

    /// Runtime mode management
    RuntimeMode {
        #[command(subcommand)]
        command: runtime_mode::RuntimeModeCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum SystemPresetSubcommand {
    /// List all discoverable presets (embedded + user + system)
    List {
        /// Filter by `run_intent` (e.g. `work_init`, `knowledge_ingest`)
        #[arg(long)]
        intent: Option<String>,
        /// Emit machine-readable JSON
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
}

#[cfg(test)]
/// Legacy `SystemPresetCommand` used in tests for CLI parsing verification.
#[derive(Debug, Subcommand)]
enum SystemPresetCommand {
    /// Show registered system presets
    Preset {
        #[command(subcommand)]
        command: SystemPresetSubcommand,
    },
}

#[cfg(test)]
/// Wrapper for parsing `SystemPresetCommand` in tests.
#[derive(Debug, clap::Parser)]
#[command(subcommand_required = true, name = "system")]
struct SystemPresetCli {
    #[command(subcommand)]
    command: SystemPresetCommand,
}

/// Run the system command (extended).
///
/// # Errors
///
/// Returns an error if the delegated command fails.
pub async fn run(cmd: SystemCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        SystemCommand::Preset { command } => match command {
            SystemPresetSubcommand::List { intent, json } => {
                list_system_presets(config, intent.as_deref(), json).await
            }
            SystemPresetSubcommand::Validate {
                path,
                json,
                offline,
            } => validate_preset(config, &path, json, offline).await,
        },
        SystemCommand::Version => {
            println!("nexus42 {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        SystemCommand::Doctor => run_combined_doctor(config).await,
        SystemCommand::Completion { shell } => print_completion(&shell),
        SystemCommand::Config { command } => config::run(command, config),
        SystemCommand::Debug { command } => debug::run(command, config).await,
        SystemCommand::Db { command } => db::run(command, config).await,
        SystemCommand::Identity { command } => identity::run(command, config).await,
        SystemCommand::RuntimeMode { command } => runtime_mode::run(command, config),
    }
}

/// Generate shell completion script for the given shell.
///
/// Parses the shell name case-insensitively and generates a completion
/// script for the full `nexus42` CLI.
///
/// # Errors
///
/// Returns an error if the shell name is not recognized.
fn print_completion(shell_str: &str) -> Result<()> {
    use clap::ValueEnum;

    let shell = Shell::from_str(shell_str, true).map_err(|_| {
        anyhow::anyhow!(
            "Unknown shell: '{shell_str}'. Supported: bash, zsh, fish, elvish, powershell"
        )
    })?;
    let mut cmd = crate::cli::build_command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, &name, &mut std::io::stdout());
    Ok(())
}

/// Run combined diagnostics: daemon connectivity + ACP registry + home directory.
///
/// This is the `nexus42 system doctor` implementation — a unified diagnostic
/// that combines infrastructure checks in a single pass.
async fn run_combined_doctor(config: &CliConfig) -> Result<()> {
    println!("nexus42 system doctor — combined diagnostics");
    println!();

    let mut issues = 0u32;

    // Check 1: Daemon connectivity
    print!("  [1/3] Daemon connectivity... ");
    let client = crate::api::DaemonClient::from_config(config);
    match client.health_check().await {
        Ok(true) => println!("✓ Running"),
        Ok(false) => {
            println!("✗ Not responding at {}", config.daemon_url);
            issues += 1;
        }
        Err(e) => {
            println!("✗ Error: {e}");
            issues += 1;
        }
    }

    // Check 2: ACP registry reachability
    print!("  [2/3] ACP registry reachability... ");
    match nexus_acp_host::registry::RegistryClient::new() {
        Ok(reg_client) => match reg_client.get_registry().await {
            Ok(registry) => {
                println!(
                    "✓ Reachable (v{}, {} agents)",
                    registry.version,
                    registry.agents.len()
                );
            }
            Err(e) => {
                println!("✗ Error: {e}");
                issues += 1;
            }
        },
        Err(e) => {
            println!("✗ Error: {e}");
            issues += 1;
        }
    }

    // Check 3: Home directory health
    print!("  [3/3] Home directory (~/.nexus42/)... ");
    match crate::config::nexus_home() {
        Ok(home) => {
            if home.exists() && home.is_dir() {
                println!("✓ Found at {}", home.display());
            } else {
                println!("✗ Not found at {}", home.display());
                issues += 1;
            }
        }
        Err(e) => {
            println!("✗ Cannot resolve: {e}");
            issues += 1;
        }
    }

    println!();
    if issues == 0 {
        println!("✓ All checks passed — system is healthy.");
    } else {
        println!("✗ {issues} issue(s) found. See above for details.");
    }

    Ok(())
}

async fn list_system_presets(
    config: &CliConfig,
    intent_filter: Option<&str>,
    json_output: bool,
) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);

    let resp: nexus_contracts::local::orchestration::http::ListPresetsResponse =
        client.get(&format!("{ORCHESTRATION_BASE}/presets")).await?;

    // Build a display list with run_intents from the preset management endpoint
    let mgmt_resp: serde_json::Value = client
        .get::<serde_json::Value>("/v1/daemon/presets")
        .await
        .unwrap_or_else(|_| serde_json::json!({}));

    let embedded_intents: std::collections::HashMap<String, Vec<String>> = mgmt_resp
        .get("embedded")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let id = p.get("id")?.as_str()?.to_string();
                    let intents = p
                        .get("run_intents")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    Some((id, intents))
                })
                .collect()
        })
        .unwrap_or_default();

    let mut presets: Vec<(String, String, Vec<String>)> = Vec::new();

    // Collect embedded presets
    for id in &resp.presets {
        let intents = embedded_intents.get(id).cloned().unwrap_or_default();
        presets.push((id.clone(), "embedded".to_string(), intents));
    }

    // Filter by intent if specified
    if let Some(intent) = intent_filter {
        presets.retain(|(_, _, intents)| intents.iter().any(|i| i == intent));
    }

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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn system_preset_list_parses() {
        let cmd = SystemPresetCli::try_parse_from(["system", "preset", "list"]).unwrap();

        match cmd.command {
            SystemPresetCommand::Preset { command } => match command {
                SystemPresetSubcommand::List { intent: _, json: _ } => {} // expected
                SystemPresetSubcommand::Validate {
                    path: _,
                    json: _,
                    offline: _,
                } => {} // expected
            },
        }
    }

    #[test]
    fn system_preset_subcommand_required() {
        let result = SystemPresetCli::try_parse_from(["system"]);
        assert!(result.is_err());
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

    #[test]
    fn system_preset_validate_offline_flag_parses() {
        let cmd = SystemPresetCli::try_parse_from([
            "system",
            "preset",
            "validate",
            "some/path",
            "--offline",
        ])
        .unwrap();
        match cmd.command {
            SystemPresetCommand::Preset { command } => match command {
                SystemPresetSubcommand::Validate {
                    path,
                    json,
                    offline,
                } => {
                    assert_eq!(path, "some/path");
                    assert!(!json);
                    assert!(offline);
                }
                SystemPresetSubcommand::List { .. } => panic!("expected validate"),
            },
        }
    }

    #[test]
    fn print_completion_valid_shell_bash() {
        // Should not error for a valid shell name
        assert!(print_completion("bash").is_ok());
    }

    #[test]
    fn print_completion_rejects_unknown_shell() {
        let result = print_completion("invalid_shell");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Unknown shell"),
            "Expected 'Unknown shell' in error, got: {err_msg}"
        );
    }
}
