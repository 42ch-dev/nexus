//! Shared CLI definitions for nexus42.
//!
//! This module contains the `Cli` struct and `Commands` enum so they can be
//! accessed from both the binary entry point (`main.rs`) and library modules
//! (e.g. `system::print_completion` for shell completion generation).

use crate::commands::{
    acp::AcpCommand, acp_worker::AcpWorkerArgs, auth::AuthCommand, clone::CloneArgs,
    config::ConfigCommand, context::ContextCommand, creator::CreatorCommand, daemon::DaemonCommand,
    daemon_run::DaemonRunArgs, db::DbCommand, debug::DebugCommand, doctor::DoctorCommand,
    explore::ExploreCommand, identity::IdentityCommand, init::InitCommand, memory::MemoryCommand,
    permission::PermissionCommand, platform::PlatformCommand, policy::PolicyCommand,
    preset::PresetCommand, runtime_mode::RuntimeModeCommand, schedule::ScheduleCommand,
    session::SessionCommand, soul::SoulCommand, sync::SyncCommand, system::SystemCommand,
    world::WorldCommand,
};
use clap::{Parser, Subcommand};

/// Nexus CLI — creative world-building command-line interface
#[derive(Parser, Debug)]
#[command(
    name = "nexus42",
    version,
    about = "Nexus creative world-building CLI",
    long_about = "Nexus creative world-building CLI — orchestration-first.\n\n\
        Use `nexus42 schedule --preset <id>` to start a preset-driven workflow,\n\
        or `nexus42 preset list` to see available presets.\n\
        Run `nexus42 init workspace` to set up a new workspace.",
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
    /// Initialize a Nexus workspace (deprecated: use `nexus42 creator workspace init`)
    #[command(hide = true)]
    Init {
        #[command(subcommand)]
        command: InitCommand,
    },

    /// Authentication (deprecated: use `nexus42 platform auth`)
    #[command(hide = true)]
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },

    /// Manage the daemon runtime
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Database status and management (deprecated: use `nexus42 system db`)
    #[command(hide = true)]
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },

    /// Internal debugging utilities (deprecated: use `nexus42 system debug`)
    #[command(hide = true)]
    Debug {
        #[command(subcommand)]
        command: DebugCommand,
    },

    /// Diagnostic health checks (deprecated: use `nexus42 system doctor`)
    #[command(hide = true)]
    Doctor {
        #[command(subcommand)]
        command: DoctorCommand,
    },

    /// Synchronize workspace with platform
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },

    /// World fork and snapshot (deprecated: use `nexus42 sync world`)
    #[command(hide = true)]
    World {
        #[command(subcommand)]
        command: WorldCommand,
    },

    /// Clone a world from platform or local source (deprecated: use `nexus42 creator workspace clone`)
    #[command(hide = true)]
    Clone {
        #[command(flatten)]
        args: CloneArgs,
    },

    /// Configuration file management (deprecated: use `nexus42 system config`)
    #[command(hide = true)]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },

    /// Explore browse and search (deprecated: use `nexus42 platform explore`)
    #[command(hide = true)]
    Explore {
        #[command(subcommand)]
        command: ExploreCommand,
    },

    /// Manage Creator entities (register, pair, credentials)
    Creator {
        #[command(subcommand)]
        command: CreatorCommand,
    },

    /// Context assembly (deprecated: use `nexus42 platform context`)
    #[command(hide = true)]
    Context {
        #[command(subcommand)]
        command: ContextCommand,
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

    /// ACP session persistence management (deprecated: use `nexus42 acp` commands)
    #[command(hide = true)]
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },

    /// Permission policy management (deprecated: use `nexus42 acp` commands)
    #[command(hide = true)]
    Policy {
        #[command(subcommand)]
        command: PolicyCommand,
    },

    /// Agent-scoped permission management (deprecated: use `nexus42 acp` commands)
    #[command(hide = true)]
    Permission {
        #[command(subcommand)]
        command: PermissionCommand,
    },

    /// Preset management (deprecated: use `nexus42 system preset` or `nexus42 preset`)
    #[command(hide = true)]
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },

    /// Local identity management (deprecated: use `nexus42 system identity`)
    #[command(hide = true)]
    Identity {
        #[command(subcommand)]
        command: IdentityCommand,
    },

    /// Runtime mode management (deprecated: use `nexus42 system runtime-mode`)
    #[command(hide = true)]
    RuntimeMode {
        #[command(subcommand)]
        command: RuntimeModeCommand,
    },

    /// SOUL management (deprecated: use `nexus42 creator soul`)
    #[command(hide = true)]
    Soul {
        #[command(subcommand)]
        command: SoulCommand,
    },

    /// Long-term memory management (deprecated: use `nexus42 creator memory`)
    #[command(hide = true)]
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },

    /// Schedule preset-driven orchestration workflows (deprecated: use `nexus42 daemon schedule`)
    #[command(hide = true)]
    Schedule {
        #[command(subcommand)]
        command: ScheduleCommand,
    },

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
