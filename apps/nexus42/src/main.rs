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
    //
    // R-V146P2-QC1-S1: the library entry returns the rendered help rather
    // than calling `std::process::exit` itself; the binary owns the exit so
    // the library call is unit-testable and never terminates a consumer.
    if let Some(help) = nexus42::commands::creator::run::maybe_render_preset_run_help() {
        // R-V146P2-QC3-S1: flush stdout before exit so the buffered `print!`
        // text is not dropped when the process terminates. Without the flush,
        // `std::process::exit(0)` skips the normal stdout teardown and piped
        // consumers (e.g. `nexus42 ... --help | less`) can lose the tail.
        print!("{help}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        std::process::exit(0);
    }

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
        Some(Commands::HostCall(args)) => nexus42::commands::host_call::run(args, &config).await,
        None => {
            Cli::parse_from(["nexus42", "--help"]);
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        // V1.51 T-B P0: exit code mapping for advisory lock errors.
        // - E_LOCK   (contention, temporary):  exit 75 (EX_TEMPFAIL)
        // - E_LOCK_IO (I/O failure, config):   exit 78 (EX_CONFIG)
        // V1.51 T-B P1: exit code mapping for OCC version conflicts.
        // - E_VERSION (CAS mismatch):          exit 76
        // - All other errors:                   exit 1
        let code = if matches!(e, nexus42::errors::CliError::Locked { .. }) {
            75
        } else if matches!(e, nexus42::errors::CliError::LockIo(_)) {
            78
        } else if matches!(e, nexus42::errors::CliError::VersionConflict { .. }) {
            76
        } else {
            1
        };
        std::process::exit(code);
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
