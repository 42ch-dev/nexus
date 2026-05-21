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
    long_about = "Nexus creative world-building CLI — orchestration-first.\n\n\
        Use `nexus42 daemon schedule --preset <id>` to start a preset-driven workflow.\n\
        Run `nexus42 creator workspace init` to set up a new workspace.",
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
    /// Manage the daemon runtime
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Synchronize workspace with platform
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },

    /// Manage Creator entities (register, pair, credentials, workspace, soul, memory)
    Creator {
        #[command(subcommand)]
        command: CreatorCommand,
    },

    /// ACP capability plane (agents, registry, skills, connectivity)
    Acp {
        #[command(subcommand)]
        command: AcpCommand,
    },

    /// Hidden: ACP worker subprocess entry point (daemon-managed)
    #[command(hide = true)]
    AcpWorker(AcpWorkerArgs),

    /// Hidden: Internal daemon-run entry point (self-spawned by daemon start)
    #[command(hide = true)]
    DaemonRun(DaemonRunArgs),

    /// System management (presets, diagnostics, config, identity, etc.)
    System {
        #[command(subcommand)]
        command: SystemCommand,
    },

    /// Platform interaction (auth, explore, context, publish)
    Platform {
        #[command(subcommand)]
        command: PlatformCommand,
    },
}

/// Build the full `nexus42` clap `Command` for completion generation.
///
/// This is used by `system completion` to produce shell completion scripts.
#[must_use]
pub fn build_command() -> clap::Command {
    <Cli as clap::CommandFactory>::command()
}
