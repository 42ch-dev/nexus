//! `nexus42 system` — System management command group.
//!
//! Implements the `nexus42 system` top-level command with subcommands:
//! - `preset` — Show registered system presets (existing)
//! - `version` — Print CLI version info
//! - `doctor` — Diagnostic health checks
//! - `completion` — Shell completion generation
//! - `config` — Configuration file management
//! - `debug` — Internal debugging utilities
//! - `db` — Database status and management
//! - `identity` — Local identity management
//! - `runtime-mode` — Runtime mode management
//!
//! # Architecture
//!
//! Thin delegation layer — each variant delegates to the existing command
//! module's `run()` function. No business logic lives here.

#![allow(clippy::print_literal)]

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use clap_complete::Shell;

const ORCHESTRATION_BASE: &str = "/v1/local/orchestration";

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
        command: super::config::ConfigCommand,
    },

    /// Internal debugging utilities
    Debug {
        #[command(subcommand)]
        command: super::debug::DebugCommand,
    },

    /// Database status and management
    Db {
        #[command(subcommand)]
        command: super::db::DbCommand,
    },

    /// Local identity management
    Identity {
        #[command(subcommand)]
        command: super::identity::IdentityCommand,
    },

    /// Runtime mode management
    RuntimeMode {
        #[command(subcommand)]
        command: super::runtime_mode::RuntimeModeCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum SystemPresetSubcommand {
    /// List all registered system presets
    List,
}

/// Legacy `SystemPresetCommand` kept for backward compatibility.
/// New code should use `SystemCommand::Preset` directly.
#[derive(Debug, Subcommand)]
pub enum SystemPresetCommand {
    /// Show registered system presets
    Preset {
        #[command(subcommand)]
        command: SystemPresetSubcommand,
    },
}

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
            SystemPresetSubcommand::List => list_system_presets(config).await,
        },
        SystemCommand::Version => {
            println!("nexus42 {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        SystemCommand::Doctor => run_combined_doctor(config).await,
        SystemCommand::Completion { shell } => print_completion(&shell),
        SystemCommand::Config { command } => super::config::run(command, config),
        SystemCommand::Debug { command } => super::debug::run(command, config).await,
        SystemCommand::Db { command } => super::db::run(command, config).await,
        SystemCommand::Identity { command } => super::identity::run(command, config).await,
        SystemCommand::RuntimeMode { command } => super::runtime_mode::run(command, config),
    }
}

/// Run the legacy system-preset command (backward compat).
///
/// # Errors
///
/// Returns an error if:
/// - Daemon API calls fail
/// - Invalid preset parameters
#[allow(dead_code)]
pub async fn run_legacy(cmd: SystemPresetCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        SystemPresetCommand::Preset { command } => match command {
            SystemPresetSubcommand::List => list_system_presets(config).await,
        },
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

async fn list_system_presets(config: &CliConfig) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);

    let resp: nexus_contracts::local::orchestration::http::ListPresetsResponse =
        client.get(&format!("{ORCHESTRATION_BASE}/presets")).await?;

    // Filter to system presets only (prefixed with `_system.`).
    let system_presets: Vec<&String> = resp
        .presets
        .iter()
        .filter(|p| p.starts_with("_system."))
        .collect();

    if system_presets.is_empty() {
        println!("No system presets found.");
    } else {
        println!("System presets:");
        for id in &system_presets {
            println!("  {id}");
        }
        println!("\n{} system preset(s)", system_presets.len());
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
    fn system_preset_list_parses() {
        let cmd = SystemPresetCli::try_parse_from(["system", "preset", "list"]).unwrap();

        match cmd.command {
            SystemPresetCommand::Preset { command } => match command {
                SystemPresetSubcommand::List => {} // expected
            },
        }
    }

    #[test]
    fn system_preset_subcommand_required() {
        let result = SystemPresetCli::try_parse_from(["system"]);
        assert!(result.is_err());
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
