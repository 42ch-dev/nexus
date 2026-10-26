//! nexus42 — Nexus Creative World-Building CLI
//!
//! A command-line interface for managing creative worlds, Creators,
//! and local authoring workflows through the Nexus platform.
//!
//! Built only for the `cli` cohort (`[[bin]] nexus42` declares
//! `required-features = ["cli"]`); the optional `connect-host` feature adds
//! the `connect` group to this same parser.

use clap::Parser;
use nexus42::cli::{Cli, Commands};
use nexus42::config::CliConfig;
use nexus42::errors::Result;

fn main() {
    let cli = Cli::parse();
    // Data-output commands (`ops inspect`) print machine-readable JSON on
    // stdout — logging must go to stderr there, so the writer decision
    // happens before the subscriber is initialized.
    init_logging(cli.verbose(), cli.is_data_output());

    // V1.101 Class B: enrich PATH *before* Tokio starts. GUI-launched desktop
    // sidecars inherit a minimal macOS PATH; `setenv` must not race concurrent
    // `getenv` on a live multi-threaded runtime.
    // Logging is already initialized so join_paths failures surface as warnings.
    // v1.193 P2-T2: the helper lives with the provider-discovery owner.
    nexus_agent_host::discovery::path_enrichment::apply_process_path_enrichment();

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
        } else if matches!(
            e,
            nexus42::errors::CliError::LockIo(_) | nexus42::errors::CliError::Config(_)
        ) {
            78
        } else if matches!(
            e,
            nexus42::errors::CliError::VersionConflict { .. }
                | nexus42::errors::CliError::WorldKbConflict { .. }
        ) {
            76
        } else if let nexus42::errors::CliError::ComputeExit { code, .. } = e {
            // V1.170 P0 (AR-9): the compute group owns its exit-code
            // vocabulary (1 build, 2 validation, 3 sha mismatch, 4 module).
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

/// Loads the startup config, degrading to defaults with a user-facing
/// warning message when the file cannot be parsed.
///
/// A corrupt `config.toml` must not fail startup: read-only commands keep
/// working on defaults. The message carries the config path and parse reason
/// from `CliConfig::load`'s error context.
fn load_startup_config() -> (CliConfig, Option<String>) {
    match CliConfig::load() {
        Ok(config) => (config, None),
        Err(error) => {
            // `{:#}` keeps the cause chain (e.g. the TOML parser's reason).
            let mut message = format!("config unreadable — {error:#}");
            // Non-parse failures (e.g. invalid UTF-8) carry no path, so name
            // the loader's own config path unless the error already does.
            if let Ok(path) = CliConfig::path() {
                let path = path.display().to_string();
                if !message.contains(&path) {
                    message = format!("{message} ({path})");
                }
            }
            message.push_str(" (continuing with defaults)");
            (CliConfig::default(), Some(message))
        }
    }
}

async fn async_main(cli: Cli) -> Result<()> {
    // Load configuration
    let (mut config, config_warning) = load_startup_config();
    if let Some(warning) = config_warning {
        eprintln!("nexus42: {warning}");
    }

    // Resolve persistent device ID (UUID v4) for platform HTTP requests.
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
        Some(Commands::Creator { command }) => {
            nexus42::commands::creator::run(command, &config).await
        }
        #[cfg(feature = "connect-host")]
        Some(Commands::Connect { command }) => nexus42::commands::connect::run(command).await,
        Some(Commands::Acp { command }) => nexus42::commands::acp::run(command, &config).await,
        Some(Commands::Compute { command }) => {
            nexus42::commands::compute::run(command, &config, &output_format)
        }
        Some(Commands::Capability { command }) => {
            nexus42::commands::capability::run(command, &config, &output_format)
        }
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
        Some(Commands::Ops { command }) => nexus42::commands::ops::run(command, &config).await,
        None => {
            Cli::parse_from(["nexus42", "--help"]);
            Ok(())
        }
    }
}

/// Initialize the tracing subscriber.
///
/// `stderr_only` routes all tracing to stderr — REQUIRED for data-output
/// commands (`ops inspect`), whose stdout is machine-readable and must stay
/// free of diagnostics.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The env var `dirs::home_dir()` reads on this platform (mirrors
    /// `bin/nexus-runtime.rs::resolve_home`).
    #[cfg(windows)]
    const HOME_ENV: &str = "USERPROFILE";
    #[cfg(not(windows))]
    const HOME_ENV: &str = "HOME";

    /// Points the platform home at a fresh temp dir and restores it on drop.
    struct TempHome {
        tmp: tempfile::TempDir,
        previous: Option<std::ffi::OsString>,
    }

    impl TempHome {
        fn new() -> Self {
            let previous = std::env::var_os(HOME_ENV);
            let tmp = tempfile::TempDir::new().expect("tempdir");
            std::env::set_var(HOME_ENV, tmp.path());
            Self { tmp, previous }
        }

        fn nexus_dir(&self) -> std::path::PathBuf {
            self.tmp.path().join(".nexus42")
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            match &self.previous {
                Some(home) => std::env::set_var(HOME_ENV, home),
                None => std::env::remove_var(HOME_ENV),
            }
        }
    }

    #[test]
    #[serial_test::serial]
    fn corrupt_config_yields_defaults_and_path_warning() {
        let home = TempHome::new();
        let nexus = home.nexus_dir();
        std::fs::create_dir_all(&nexus).expect("create nexus dir");
        std::fs::write(nexus.join("config.toml"), "not = [valid toml").expect("write config");

        let (config, warning) = load_startup_config();
        let warning = warning.expect("corrupt config must produce a warning");
        assert!(
            warning.contains(&nexus.join("config.toml").display().to_string()),
            "warning must name the config path: {warning}"
        );
        assert!(
            warning.contains("unclosed array"),
            "warning must carry the TOML reason: {warning}"
        );
        assert_eq!(config.platform_url, CliConfig::default().platform_url);
    }

    #[test]
    #[serial_test::serial]
    fn unreadable_config_warning_still_names_the_path() {
        let home = TempHome::new();
        let nexus = home.nexus_dir();
        std::fs::create_dir_all(&nexus).expect("create nexus dir");
        // Invalid UTF-8 fails in `read_to_string` before any TOML context is
        // attached, so this exercises the path-less error class.
        std::fs::write(nexus.join("config.toml"), [0xff, 0xfe]).expect("write config");

        let (config, warning) = load_startup_config();
        let warning = warning.expect("unreadable config must produce a warning");
        assert!(
            warning.contains(&nexus.join("config.toml").display().to_string()),
            "warning must name the config path: {warning}"
        );
        assert!(
            warning.contains("UTF-8"),
            "warning must carry the read-failure reason: {warning}"
        );
        assert_eq!(config.platform_url, CliConfig::default().platform_url);
    }

    #[test]
    #[serial_test::serial]
    fn valid_config_loads_without_warning() {
        let home = TempHome::new();
        let nexus = home.nexus_dir();
        std::fs::create_dir_all(&nexus).expect("create nexus dir");
        std::fs::write(
            nexus.join("config.toml"),
            "platform_url = \"https://example.test\"\n",
        )
        .expect("write config");

        let (config, warning) = load_startup_config();
        assert!(warning.is_none(), "valid config must not warn: {warning:?}");
        assert_eq!(config.platform_url, "https://example.test");
    }
}
