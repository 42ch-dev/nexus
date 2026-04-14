//! Runtime mode management command.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_domain::runtime_mode::DomainRuntimeMode;

#[derive(Debug, Subcommand)]
pub enum RuntimeModeCommand {
    /// Show current runtime mode
    Show,
    /// Set runtime mode (local_only, local_first, cloud_enhanced)
    Set {
        /// Target runtime mode
        mode: String,
    },
}

pub async fn run(command: RuntimeModeCommand, config: &CliConfig) -> Result<()> {
    match command {
        RuntimeModeCommand::Show => show(config),
        RuntimeModeCommand::Set { mode } => set(config, &mode),
    }
}

fn show(config: &CliConfig) -> Result<()> {
    let mode = config.runtime_mode();
    println!("Runtime mode: {}", mode);
    println!();
    println!(
        "Platform dependency: {}",
        if mode.allows_platform() {
            "allowed"
        } else {
            "prohibited"
        }
    );
    println!(
        "Platform LLM: {}",
        if mode.allows_platform_llm() {
            "allowed"
        } else {
            "prohibited"
        }
    );
    println!();
    if mode.is_local_only() {
        println!(
            "Blocked operations: sync, publish, auth login/register, platform context assemble, explore"
        );
    }
    Ok(())
}

fn set(config: &CliConfig, mode_str: &str) -> Result<()> {
    let new_mode = DomainRuntimeMode::parse(mode_str)
        .map_err(|e| crate::errors::CliError::Config(e.to_string()))?;

    let old_mode = config.runtime_mode();
    if old_mode == new_mode {
        println!("Runtime mode is already '{}'.", new_mode);
        return Ok(());
    }

    println!("Switching runtime mode: {} → {}", old_mode, new_mode);

    // Warn about platform requirements
    if new_mode.allows_platform() {
        println!(
            "Note: {} mode requires platform connectivity for some operations.",
            new_mode
        );
    }

    // Persist
    let mut updated = config.clone();
    updated.runtime_mode = new_mode;
    updated.save()?;

    println!("Runtime mode set to '{}'.", new_mode);
    Ok(())
}
