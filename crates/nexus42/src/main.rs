//! nexus42 — Nexus Creative World-Building CLI
//!
//! A command-line interface for managing creative worlds, Creators,
//! and preset-driven orchestration workflows through the Nexus platform.

use clap::Parser;
use nexus42::cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    // V1.46 P2 (Grill #20, #21): intercept `creator run <preset_id> --help`
    // before clap parses so manifest-declared `cli_args` surface in --help.
    // Falls through silently for any non-matching invocation.
    nexus42::commands::creator::run::maybe_print_preset_run_help_and_exit();

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
        Some(Commands::Daemon { command }) => {
            nexus42::commands::daemon::run(command, &config).await
        }
        Some(Commands::Sync { command }) => {
            eprintln!(
                "Warning: `nexus42 sync` is deprecated. Use `nexus42 platform sync` instead. \
                 The top-level `sync` alias will be removed in a future version."
            );
            nexus42::commands::sync::run(command, &config).await
        }
        Some(Commands::Creator { command }) => {
            nexus42::commands::creator::run(command, &config).await
        }
        Some(Commands::Acp { command }) => nexus42::commands::acp::run(command, &config).await,
        Some(Commands::AcpWorker(args)) => nexus42::commands::acp_worker::run(args).await,
        Some(Commands::DaemonRun(args)) => nexus42::commands::daemon_run::run(args).await,
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
