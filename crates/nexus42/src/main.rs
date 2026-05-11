//! nexus42 — Nexus Creative World-Building CLI
//!
//! A command-line interface for managing creative worlds, Creators,
//! and preset-driven orchestration workflows through the Nexus platform.

mod api;
mod auth;
mod challenge;
mod commands;
mod config;
mod context;
mod db;
mod errors;
mod paths;
mod session_capture;

use clap::{Parser, Subcommand};
use commands::{
    acp_worker::AcpWorkerArgs, agent::AgentCommand, auth::AuthCommand, clone::CloneArgs,
    config::ConfigCommand, context::ContextCommand, creator::CreatorCommand, daemon::DaemonCommand,
    db::DbCommand, debug::DebugCommand, doctor::DoctorCommand, explore::ExploreCommand,
    identity::IdentityCommand, init::InitCommand, memory::MemoryCommand,
    permission::PermissionCommand, policy::PolicyCommand, preset::PresetCommand,
    runtime_mode::RuntimeModeCommand, schedule::ScheduleCommand, session::SessionCommand,
    soul::SoulCommand, sync::SyncCommand, system::SystemPresetCommand, world::WorldCommand,
};

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
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Output format (text or json)
    #[arg(short = 'o', long = "output", global = true, default_value = "text")]
    output_format: String,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Initialize a Nexus workspace
    Init {
        #[command(subcommand)]
        command: InitCommand,
    },

    /// Authentication (login/logout/status)
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },

    /// Manage the nexus42d daemon
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Database status and management
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },

    /// Internal debugging utilities
    Debug {
        #[command(subcommand)]
        command: DebugCommand,
    },

    /// Diagnostic health checks
    Doctor {
        #[command(subcommand)]
        command: DoctorCommand,
    },

    /// Synchronize workspace with platform
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },

    /// World fork and snapshot (platform via daemon)
    World {
        #[command(subcommand)]
        command: WorldCommand,
    },

    /// Clone a world from platform or local source
    Clone {
        #[command(flatten)]
        args: CloneArgs,
    },

    /// Configuration file management
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },

    /// Explore browse and search (read-only, platform via daemon)
    Explore {
        #[command(subcommand)]
        command: ExploreCommand,
    },

    /// Manage Creator entities (register, pair, credentials)
    Creator {
        #[command(subcommand)]
        command: CreatorCommand,
    },

    /// Context assembly (V1.1+)
    Context {
        #[command(subcommand)]
        command: ContextCommand,
    },

    /// Agent management (ACP integration)
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },

    /// Hidden: ACP worker subprocess entry point (daemon-managed)
    #[command(hide = true)]
    AcpWorker(AcpWorkerArgs),

    /// ACP session persistence management
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },

    /// Permission policy management (ACP-R7)
    Policy {
        #[command(subcommand)]
        command: PolicyCommand,
    },

    /// Agent-scoped permission management (V1.6)
    Permission {
        #[command(subcommand)]
        command: PermissionCommand,
    },

    /// Preset management (init, list, validate) — orchestration templates
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },

    /// Local identity management (`local_only` mode)
    Identity {
        #[command(subcommand)]
        command: IdentityCommand,
    },

    /// Runtime mode management (`local_only` / `local_first` / `cloud_enhanced`)
    RuntimeMode {
        #[command(subcommand)]
        command: RuntimeModeCommand,
    },

    /// SOUL management (local personality and experience)
    Soul {
        #[command(subcommand)]
        command: SoulCommand,
    },

    /// Long-term memory management
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },

    /// Schedule preset-driven orchestration workflows
    Schedule {
        #[command(subcommand)]
        command: ScheduleCommand,
    },

    /// System management (presets, diagnostics)
    System {
        #[command(subcommand)]
        command: SystemPresetCommand,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize tracing
    init_logging(cli.verbose);

    // Load configuration
    let mut config = config::CliConfig::load().unwrap_or_default();

    // Resolve persistent device ID (UUID v4) for platform HTTP requests.
    // Only create the device-id file if the nexus home already exists
    // (i.e., the user has already run `init workspace` or equivalent).
    if let Ok(nexus_home) = config::nexus_home() {
        if nexus_home.exists() {
            match nexus_sync::device_id::get_or_create_device_id(&nexus_home) {
                Ok(device_id) => config.device_id = device_id,
                Err(e) => {
                    // Device ID failure is non-fatal: platform falls back to
                    // IP-based rate limiting when X-Device-ID is absent.
                    // Still visible to the user so they understand degraded mode.
                    eprintln!(
                        "nexus42: device identity unavailable — {e} (platform rate-limit will use IP-based identification)"
                    );
                }
            }
        }
    }

    // Execute command
    let result = match cli.command {
        Some(Commands::Init { command }) => commands::init::run(command).await,
        Some(Commands::Auth { command }) => commands::auth::run(command, &config).await,
        Some(Commands::Daemon { command }) => commands::daemon::run(command, &config).await,
        Some(Commands::Db { command }) => commands::db::run(command, &config).await,
        Some(Commands::Debug { command }) => commands::debug::run(command, &config).await,
        Some(Commands::Doctor { command }) => commands::doctor::run(command, &config).await,
        Some(Commands::Sync { command }) => commands::sync::run(command, &config).await,
        Some(Commands::World { command }) => commands::world::run(command, &config).await,
        Some(Commands::Clone { args }) => commands::clone::run(args, &config).await,
        Some(Commands::Config { command }) => commands::config::run(command, &config),
        Some(Commands::Explore { command }) => {
            commands::explore::run(command, &config, &cli.output_format).await
        }
        Some(Commands::Creator { command }) => commands::creator::run(command, &config).await,
        Some(Commands::Context { command }) => commands::context::run(command, &config).await,
        Some(Commands::Agent { command }) => commands::agent::run(command, &config).await,
        Some(Commands::AcpWorker(args)) => commands::acp_worker::run(args).await,
        Some(Commands::Session { command }) => commands::session::run(command, &config),
        Some(Commands::Policy { command }) => commands::policy::run(command),
        Some(Commands::Permission { command }) => commands::permission::run(command),
        Some(Commands::Preset { command }) => commands::preset::run(command, &config),
        Some(Commands::Identity { command }) => commands::identity::run(command, &config).await,
        Some(Commands::RuntimeMode { command }) => commands::runtime_mode::run(command, &config),
        Some(Commands::Soul { command }) => commands::soul::run(command, &config).await,
        Some(Commands::Memory { command }) => commands::memory::run(command, &config).await,
        Some(Commands::Schedule { command }) => commands::schedule::run(command, &config).await,
        Some(Commands::System { command }) => commands::system::run(command, &config).await,
        None => {
            Cli::parse_from(["nexus42", "--help"]);
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

/// Initialize tracing subscriber
fn init_logging(verbose: bool) {
    let filter = if verbose {
        tracing_subscriber::EnvFilter::new("debug")
    } else {
        tracing_subscriber::EnvFilter::new("warn")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .init();
}
