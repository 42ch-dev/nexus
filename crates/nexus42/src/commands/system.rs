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
    Doctor {
        #[command(subcommand)]
        command: super::doctor::DoctorCommand,
    },

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
        SystemCommand::Doctor { command } => super::doctor::run(command, config).await,
        SystemCommand::Completion { shell } => {
            print_completion_help(&shell);
            Ok(())
        }
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

/// Print shell completion help text.
///
/// `clap_complete` is not yet a dependency, so this provides a helpful
/// message directing users to generate completions via their shell's
/// built-in mechanism.
fn print_completion_help(shell: &str) {
    println!("Shell completion for '{shell}' is not yet available.");
    println!();
    println!("To generate completions manually:");
    println!("  eval \"$(nexus42 --help | tail -n +2)\"");
    println!();
    println!("Full completion support will be added in a future release.");
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
    fn print_completion_help_does_not_error() {
        print_completion_help("bash");
    }
}
