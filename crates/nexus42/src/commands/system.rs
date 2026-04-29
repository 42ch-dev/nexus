//! `nexus42 system preset list` — show registered system presets.
//!
//! Lists system presets discovered from `~/.nexus42/presets/_system/<name>/`.

#![allow(clippy::print_literal)]

use crate::config::CliConfig;
use crate::errors::Result;
use clap::{Parser, Subcommand};

const ORCHESTRATION_BASE: &str = "/v1/local/orchestration";

#[derive(Debug, Subcommand)]
pub enum SystemPresetCommand {
    /// Show registered system presets
    Preset {
        #[command(subcommand)]
        command: SystemPresetSubcommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum SystemPresetSubcommand {
    /// List all registered system presets
    List,
}

/// Wrapper for parsing `SystemPresetCommand` in tests.
#[derive(Debug, Parser)]
#[command(subcommand_required = true, name = "system")]
struct SystemPresetCli {
    #[command(subcommand)]
    command: SystemPresetCommand,
}

/// Run the system preset command.
pub async fn run(cmd: SystemPresetCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        SystemPresetCommand::Preset { command } => match command {
            SystemPresetSubcommand::List => list_system_presets(config).await,
        },
    }
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
mod tests {
    use super::*;

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
}
