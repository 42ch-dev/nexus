//! `nexus42 system` — System management command group.
//!
//! Implements the `nexus42 system` top-level command with subcommands:
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

/// Run the system command (extended).
///
/// # Errors
///
/// Returns an error if the delegated command fails.
pub async fn run(cmd: SystemCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        SystemCommand::Version => {
            println!("nexus42 {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        SystemCommand::Doctor => run_combined_doctor().await,
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

/// Run combined diagnostics: ACP registry + home directory.
///
/// This is the `nexus42 system doctor` implementation — a unified diagnostic
/// that combines infrastructure checks in a single pass. Both checks are local
/// or registry-backed; a stopped local HTTP service is not a doctor failure.
async fn run_combined_doctor() -> Result<()> {
    println!("nexus42 system doctor — combined diagnostics");
    println!();

    let mut issues = 0u32;

    // Check 1: ACP registry reachability
    print!("  [1/2] ACP registry reachability... ");
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

    // Check 2: Home directory health
    print!("  [2/2] Home directory (~/.nexus42/)... ");
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
