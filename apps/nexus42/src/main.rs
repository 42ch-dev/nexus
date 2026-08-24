//! nexus42 — Nexus Creative World-Building CLI
//!
//! A command-line interface for managing creative worlds, Creators,
//! and preset-driven orchestration workflows through the Nexus platform.

use clap::Parser;
use nexus42::cli::{Cli, Commands};
use nexus42::config::CliConfig;
use nexus42::errors::Result;

fn main() {
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
    // The MCP stdio child's stdout is the JSON-RPC transport — logging must
    // go to stderr there (AR-72), so the writer decision happens before
    // the subscriber is initialized.
    init_logging(cli.verbose(), cli.is_mcp_serve());

    // V1.101 Class B: enrich PATH *before* Tokio starts. GUI-launched desktop
    // sidecars inherit a minimal macOS PATH; `setenv` must not race concurrent
    // `getenv` on a live multi-threaded runtime (Greptile P2 on run_daemon).
    // Logging is already initialized so join_paths failures surface as warnings.
    nexus_daemon_runtime::path_enrichment::apply_process_path_enrichment();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build Tokio runtime");
    if let Err(e) = runtime.block_on(async_main(cli)) {
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
        } else if let nexus42::errors::CliError::ComputeExit { code, .. } = e {
            // V1.170 P0 (AR-9): the compute group owns its exit-code
            // vocabulary (1 build, 2 validation, 3 sha mismatch, 4 daemon).
            code
        } else {
            1
        };
        // M4: flush stdout before exit — compute commands print status/JSON
        // to stdout, and `std::process::exit` skips the normal stdout
        // teardown; piped consumers would lose the buffered tail.
        let _ = std::io::Write::flush(&mut std::io::stdout());
        std::process::exit(code);
    }
}

async fn async_main(cli: Cli) -> Result<()> {
    // Load configuration
    let mut config = CliConfig::load().unwrap_or_default();

    // Resolve persistent device ID (UUID v4) for platform HTTP requests.
    // Only create the device-id file if the nexus home already exists
    // (i.e., the user has already run `init workspace` or equivalent).
    // `get_or_create_device_id` takes the RAW home and joins `.nexus42`
    // itself (canonical `~/.nexus42/device-id`; device_id_path contract).
    if let (Ok(nexus_home), Some(raw_home)) = (nexus42::config::nexus_home(), dirs::home_dir()) {
        if nexus_home.exists() {
            match nexus_cloud_sync::device_id::get_or_create_device_id(&raw_home) {
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
    match cli.into_command() {
        Some(Commands::Daemon { command }) => {
            nexus42::commands::daemon::run(command, &config).await
        }
        #[cfg(feature = "connect-host")]
        Some(Commands::Connect { command }) => nexus42::commands::connect::run(command).await,
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
        Some(Commands::Compute { command }) => {
            nexus42::commands::compute::run(command, &config, &output_format).await
        }
        Some(Commands::Capability { command }) => {
            nexus42::commands::capability::run(command, &config, &output_format).await
        }
        Some(Commands::AcpWorker(args)) => nexus42::commands::acp_worker::run(args).await,
        Some(Commands::DaemonRun(args)) => nexus42::commands::daemon_run::run(args).await,
        #[cfg(feature = "connect-client")]
        Some(Commands::Mcp { command }) => nexus42::commands::mcp::run(command, &config).await,
        Some(Commands::System { command }) => {
            nexus42::commands::system::run(command, &config).await
        }
        Some(Commands::Preset { command }) => {
            nexus42::commands::preset::run(command, &config).await
        }
        Some(Commands::Desktop { command }) => nexus42::commands::desktop::run(command).await,
        Some(Commands::Platform { command }) => {
            nexus42::commands::platform::run(command, &config, &output_format).await
        }
        Some(Commands::HostCall(args)) => nexus42::commands::host_call::run(args, &config).await,
        None => {
            Cli::parse_from(["nexus42", "--help"]);
            Ok(())
        }
    }
}

/// Initialize the tracing subscriber.
///
/// `stderr_only` routes all tracing to stderr — REQUIRED for the MCP stdio
/// bridge child (`nexus42 mcp serve`), whose stdout is the JSON-RPC
/// transport (V1.174 P0 T5, AR-72: stdout must stay clean).
fn init_logging(verbose: bool, stderr_only: bool) {
    let filter = if verbose {
        tracing_subscriber::EnvFilter::new("debug")
    } else {
        tracing_subscriber::EnvFilter::new("warn")
    };

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time();
    if stderr_only {
        builder.with_writer(std::io::stderr).init();
    } else {
        builder.init();
    }
}
