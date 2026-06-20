//! Shared CLI definitions for nexus42.
//!
//! This module contains the `Cli` struct and `Commands` enum so they can be
//! accessed from both the binary entry point (`main.rs`) and library modules
//! (e.g. `system::print_completion` for shell completion generation).

use crate::commands::{
    acp::AcpCommand, acp_worker::AcpWorkerArgs, creator::CreatorCommand, daemon::DaemonCommand,
    daemon_run::DaemonRunArgs, platform::PlatformCommand, sync::SyncCommand, system::SystemCommand,
};
use clap::{Parser, Subcommand};

/// Nexus CLI — creative world-building command-line interface
#[derive(Parser, Debug)]
#[command(
    name = "nexus42",
    version,
    about = "Nexus creative world-building CLI",
    long_about = "Nexus creative world-building CLI — creator-first.\n\n\
        Quick start:\n\
          nexus42 creator workspace init    Set up a new workspace\n\
          nexus42 creator works status      Show your active Work\n\n\
        Platform sync (requires login):\n\
          nexus42 platform sync pull        Pull bundles from platform\n\
          nexus42 platform sync push        Push local changes to platform\n\n\
        Advanced:\n\
          nexus42 daemon schedule --preset <id>  Start a preset-driven workflow",
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Output format (text or json)
    #[arg(short = 'o', long = "output", global = true, default_value = "text")]
    output_format: String,
}

impl Cli {
    /// Returns whether verbose logging is enabled.
    #[must_use]
    pub const fn verbose(&self) -> bool {
        self.verbose
    }

    /// Returns the output format string.
    #[must_use]
    pub fn output_format(&self) -> &str {
        &self.output_format
    }

    /// Consumes `self` and returns the inner `Commands` enum, if any.
    #[must_use]
    pub fn into_command(self) -> Option<Commands> {
        self.command
    }
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Manage Creator entities (register, pair, credentials, workspace, soul, memory, kb)
    Creator {
        #[command(subcommand)]
        command: CreatorCommand,
    },

    /// Manage the daemon runtime
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// ACP capability plane (agents, registry, connectivity)
    Acp {
        #[command(subcommand)]
        command: AcpCommand,
    },

    /// Platform interaction (auth, explore, context, publish, **sync**)
    Platform {
        #[command(subcommand)]
        command: PlatformCommand,
    },

    /// System management (presets, diagnostics, config, identity, etc.)
    System {
        #[command(subcommand)]
        command: SystemCommand,
    },

    /// Hidden: deprecated top-level sync alias — use `platform sync` instead.
    /// Kept callable for ≥1 iteration (V1.35) per cli-command-ia.md §5.
    #[command(hide = true)]
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },

    /// Hidden: ACP worker subprocess entry point (daemon-managed)
    #[command(hide = true)]
    AcpWorker(AcpWorkerArgs),

    /// Hidden: Internal daemon-run entry point (self-spawned by daemon start)
    #[command(hide = true)]
    DaemonRun(DaemonRunArgs),
}

/// Build the full `nexus42` clap `Command` for completion generation.
///
/// This is used by `system completion` to produce shell completion scripts.
#[must_use]
pub fn build_command() -> clap::Command {
    <Cli as clap::CommandFactory>::command()
}
