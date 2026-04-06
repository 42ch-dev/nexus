//! nexus42 — Nexus Creative World-Building CLI
//!
//! A command-line interface for managing creative worlds, manuscripts,
//! and Creator entities through the Nexus platform.

mod api;
mod auth;
mod commands;
mod config;
mod errors;

use clap::{Parser, Subcommand};
use commands::{
    auth::AuthCommand, context::ContextCommand, creator::CreatorCommand, daemon::DaemonCommand,
    init::InitCommand, manuscript::ManuscriptCommand, research::ResearchCommand, sync::SyncCommand,
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

    /// Synchronize workspace with platform
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
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
        Some(Commands::Sync { command }) => commands::sync::run(command, &config).await,
        Some(Commands::Creator { command }) => commands::creator::run(command, &config).await,
        Some(Commands::Manuscript { command }) => commands::manuscript::run(command, &config).await,
        Some(Commands::Research { command }) => commands::research::run(command, &config).await,
        Some(Commands::Context { command }) => commands::context::run(command, &config).await,
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
