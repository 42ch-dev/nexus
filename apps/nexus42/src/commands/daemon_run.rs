//! Hidden `__internal daemon-run` command — daemon subprocess entry point.
//!
//! This module provides the hidden internal command invoked by
//! `nexus42 daemon start` (background mode) via self-spawn. It calls
//! `nexus_daemon_runtime::boot::run_daemon()` directly.

use crate::errors::Result;
use clap::Args;

/// Hidden internal command: run the daemon runtime directly.
///
/// This is not shown in help output. It is invoked by the parent
/// `nexus42` process when background daemon start is requested.
#[derive(Debug, Args)]
#[command(hide = true)]
pub struct DaemonRunArgs {
    /// Port to listen on (default: 8420)
    #[arg(long, default_value_t = 8420)]
    pub port: u16,

    /// Bind address (default: 127.0.0.1)
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Use Unix domain socket at the given path instead of HTTP
    #[arg(long)]
    pub socket_path: Option<std::path::PathBuf>,

    /// Enable verbose logging
    #[arg(long)]
    pub verbose: bool,

    /// Shutdown grace period in milliseconds (default: 20000)
    #[arg(long, default_value_t = 20000)]
    pub shutdown_grace_ms: u64,

    /// Optional CDN URL for registry.refresh network mode.
    /// When set, enables fetching the ACP registry from a CDN
    /// with built-in timeout and retry (10s timeout, 3 retries).
    /// When absent, registry.refresh returns synthetic output only.
    ///
    /// # Security
    ///
    /// Must be a public HTTPS CDN URL. Non-HTTPS schemes, private IPs,
    /// loopback, link-local, and metadata endpoints are rejected.
    #[arg(long)]
    pub cdn_url: Option<String>,
}

/// Execute the internal daemon-run command.
///
/// # Errors
///
/// Propagates any error from the daemon runtime.
pub async fn run(args: DaemonRunArgs) -> Result<()> {
    // Validate CDN URL before boot (H-002).
    if let Some(ref url) = args.cdn_url {
        validate_cdn_url(url)?;
    }

    let config = nexus_daemon_runtime::boot::DaemonConfig {
        port: args.port,
        host: args.host,
        socket_path: args.socket_path,
        verbose: args.verbose,
        shutdown_grace_ms: args.shutdown_grace_ms,
        cdn_url: args.cdn_url,
    };

    nexus_daemon_runtime::boot::run_daemon(config)
        .await
        .map_err(|e| crate::errors::CliError::Daemon {
            message: format!("Daemon runtime error: {e}"),
        })
}

/// Validate a `--cdn-url` value against security constraints.
fn validate_cdn_url(url: &str) -> Result<()> {
    nexus_orchestration::capability::builtins::validate_cdn_url_static(url).map_err(|e| {
        crate::errors::CliError::Config(format!(
            "--cdn-url must be a public HTTPS CDN URL (https://...); \
             got {url:?}: {e}"
        ))
    })
}
