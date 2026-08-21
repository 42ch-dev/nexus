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
use clap::Subcommand;
use clap_complete::Shell;

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

/// PL-6 alias bridge: `system preset <sub>` forwards into the canonical
/// `nexus42 preset` group (AR-24). The alias keeps the same subcommand
/// surface (`list|validate`) for the one-release compatibility window.
impl From<SystemPresetSubcommand> for crate::commands::preset::PresetCommand {
    fn from(cmd: SystemPresetSubcommand) -> Self {
        match cmd {
            SystemPresetSubcommand::List { intent, json } => Self::List { intent, json },
            SystemPresetSubcommand::Validate {
                path,
                json,
                offline,
            } => Self::Validate {
                path,
                json,
                offline,
            },
        }
    }
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
        SystemCommand::Preset { command } => {
            // PL-6: `system preset` is a working compatibility alias for one
            // release (deactivate-not-delete, V1.169). Forward into the
            // canonical `nexus42 preset` group with a deprecation notice.
            eprintln!(
                "Warning: `nexus42 system preset` is deprecated. Use `nexus42 preset` instead. \
                 The `system preset` alias will be removed in a future version."
            );
            crate::commands::preset::run(command.into(), config).await
        }
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    // Empty arms are deliberate compile-time shape checks of the clap variants.
    #[allow(clippy::match_same_arms)]
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
