//! nexus42 — Nexus Creative World-Building CLI
//!
//! A command-line interface for managing creative worlds, manuscripts,
//! and Creator entities through the Nexus platform.

mod acp;
mod api;
mod auth;
mod challenge;
mod commands;
mod config;
mod context;
mod db;
mod errors;
mod manuscript;
mod paths;

use clap::{Parser, Subcommand};
use commands::{
    acp_worker::AcpWorkerArgs, agent::AgentCommand, auth::AuthCommand, clone::CloneArgs,
    config::ConfigCommand, context::ContextCommand, creator::CreatorCommand, daemon::DaemonCommand,
    db::DbCommand, debug::DebugCommand, doctor::DoctorCommand, explore::ExploreCommand,
    identity::IdentityCommand, init::InitCommand, manuscript::ManuscriptCommand,
    memory::MemoryCommand, policy::PolicyCommand, publish::PublishCommand,
    research::ResearchCommand, runtime_mode::RuntimeModeCommand, session::SessionCommand,
    soul::SoulCommand, sync::SyncCommand, world::WorldCommand,
};

/// Nexus CLI — creative world-building command-line interface
#[derive(Parser, Debug)]
#[command(
    name = "nexus42",
    version,
    about = "Nexus creative world-building CLI",
    long_about = "Manage creative worlds, manuscripts, Creators, and research.\n\nUse `nexus42 init workspace` to start, or `nexus42 --help` for all commands.",
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

    /// Manuscript publish workflow (platform via daemon)
    Publish {
        #[command(subcommand)]
        command: PublishCommand,
    },

    /// Manage Creator entities (register, pair, credentials)
    Creator {
        #[command(subcommand)]
        command: CreatorCommand,
    },

    /// Manage manuscript phases and lifecycle
    Manuscript {
        #[command(subcommand)]
        command: ManuscriptCommand,
    },

    /// Research and reference source management
    Research {
        #[command(subcommand)]
        command: ResearchCommand,
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

    /// Local identity management (local_only mode)
    Identity {
        #[command(subcommand)]
        command: IdentityCommand,
    },

    /// Runtime mode management (local_only / local_first / cloud_enhanced)
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
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize tracing
    init_logging(cli.verbose);

    // Load configuration
    let config = config::CliConfig::load().unwrap_or_default();

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
        Some(Commands::Config { command }) => commands::config::run(command, &config).await,
        Some(Commands::Explore { command }) => {
            commands::explore::run(command, &config, &cli.output_format).await
        }
        Some(Commands::Publish { command }) => {
            commands::publish::run(command, &config, &cli.output_format).await
        }
        Some(Commands::Creator { command }) => commands::creator::run(command, &config).await,
        Some(Commands::Manuscript { command }) => commands::manuscript::run(command, &config).await,
        Some(Commands::Research { command }) => commands::research::run(command, &config).await,
        Some(Commands::Context { command }) => commands::context::run(command, &config).await,
        Some(Commands::Agent { command }) => commands::agent::run(command, &config).await,
        Some(Commands::AcpWorker(args)) => commands::acp_worker::run(args).await,
        Some(Commands::Session { command }) => commands::session::run(command, &config).await,
        Some(Commands::Policy { command }) => commands::policy::run(command).await,
        Some(Commands::Identity { command }) => commands::identity::run(command, &config).await,
        Some(Commands::RuntimeMode { command }) => {
            commands::runtime_mode::run(command, &config).await
        }
        Some(Commands::Soul { command }) => commands::soul::run(command, &config).await,
        Some(Commands::Memory { command }) => commands::memory::run(command, &config).await,
        None => {
            Cli::parse_from(["nexus42", "--help"]);
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
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
