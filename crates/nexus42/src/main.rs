//! nexus42 — Nexus Creative World-Building CLI
//!
//! A command-line interface for managing creative worlds, Creators,
//! and preset-driven orchestration workflows through the Nexus platform.

use clap::Parser;
use nexus42::cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize tracing
    init_logging(cli.verbose());

    // Load configuration
    let mut config = nexus42::config::CliConfig::load().unwrap_or_default();

    // Resolve persistent device ID (UUID v4) for platform HTTP requests.
    // Only create the device-id file if the nexus home already exists
    // (i.e., the user has already run `init workspace` or equivalent).
    if let Ok(nexus_home) = nexus42::config::nexus_home() {
        if nexus_home.exists() {
            match nexus_cloud_sync::device_id::get_or_create_device_id(&nexus_home) {
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
    let output_format = cli.output_format().to_string();
    let result = match cli.into_command() {
        Some(Commands::Init { command }) => nexus42::commands::init::run(command).await,
        Some(Commands::Auth { command }) => nexus42::commands::auth::run(command, &config).await,
        Some(Commands::Daemon { command }) => {
            nexus42::commands::daemon::run(command, &config).await
        }
        Some(Commands::Db { command }) => nexus42::commands::db::run(command, &config).await,
        Some(Commands::Debug { command }) => nexus42::commands::debug::run(command, &config).await,
        Some(Commands::Doctor { command }) => {
            eprintln!("Note: `nexus42 doctor` is deprecated. Use `nexus42 system doctor` instead.");
            nexus42::commands::doctor::run(command, &config).await
        }
        Some(Commands::Sync { command }) => nexus42::commands::sync::run(command, &config).await,
        Some(Commands::World { command }) => nexus42::commands::world::run(command).await,
        Some(Commands::Clone { args }) => nexus42::commands::clone::run(args, &config).await,
        Some(Commands::Config { command }) => nexus42::commands::config::run(command, &config),
        Some(Commands::Explore { command }) => {
            eprintln!(
                "Note: `nexus42 explore` is deprecated. Use `nexus42 platform explore` instead."
            );
            nexus42::commands::explore::run(command).await
        }
        Some(Commands::Creator { command }) => {
            nexus42::commands::creator::run(command, &config).await
        }
        Some(Commands::Context { command }) => {
            nexus42::commands::context::run(command, &config).await
        }
        Some(Commands::Acp { command }) => nexus42::commands::acp::run(command, &config).await,
        Some(Commands::AcpWorker(args)) => nexus42::commands::acp_worker::run(args).await,
        Some(Commands::DaemonRun(args)) => nexus42::commands::daemon_run::run(args).await,
        Some(Commands::Session { command }) => {
            eprintln!("Note: `nexus42 session` is deprecated. Use `nexus42 acp session` instead.");
            nexus42::commands::session::run(command, &config)
        }
        Some(Commands::Policy { command }) => {
            eprintln!("Note: `nexus42 policy` is deprecated. Use `nexus42 acp policy` instead.");
            nexus42::commands::policy::run(command)
        }
        Some(Commands::Permission { command }) => {
            eprintln!(
                "Note: `nexus42 permission` is deprecated. Use `nexus42 acp permission` instead."
            );
            nexus42::commands::permission::run(command)
        }
        Some(Commands::Preset { command }) => {
            nexus42::commands::preset::run(command, &config).await
        }
        Some(Commands::Identity { command }) => {
            nexus42::commands::identity::run(command, &config).await
        }
        Some(Commands::RuntimeMode { command }) => {
            nexus42::commands::runtime_mode::run(command, &config)
        }
        Some(Commands::Soul { command }) => nexus42::commands::soul::run(command, &config).await,
        Some(Commands::Memory { command }) => {
            nexus42::commands::memory::run(command, &config).await
        }
        Some(Commands::Schedule { command }) => {
            nexus42::commands::schedule::run(command, &config).await
        }
        Some(Commands::System { command }) => {
            nexus42::commands::system::run(command, &config).await
        }
        Some(Commands::Platform { command }) => {
            nexus42::commands::platform::run(command, &config, &output_format).await
        }
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
