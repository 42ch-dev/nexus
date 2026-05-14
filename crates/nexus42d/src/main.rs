//! nexus42d — Nexus Daemon Binary
//!
//! **TEMPORARY parallel path**: this binary now delegates to
//! `nexus_daemon_runtime::boot::run_daemon()`. It will be removed in Batch 3.

use std::path::PathBuf;

use clap::Parser;
use nexus_daemon_runtime::boot::{run_daemon, DaemonConfig};

/// Nexus Daemon — local supervisor for the CLI
#[derive(Parser, Debug)]
#[command(
    name = "nexus42d",
    version,
    about = "Nexus local daemon — manages workspace, auth, and sync"
)]
pub struct DaemonArgs {
    /// Port to listen on (default: 8420, ignored when --socket-path is set)
    #[arg(short, long, default_value_t = 8420)]
    port: u16,

    /// Bind address (default: 127.0.0.1, ignored when --socket-path is set)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Use Unix domain socket at the given path instead of HTTP
    #[arg(long)]
    socket_path: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Shutdown grace period in milliseconds (default: 20000)
    #[arg(long, default_value_t = 20000)]
    shutdown_grace_ms: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = DaemonArgs::parse();

    let config = DaemonConfig {
        port: args.port,
        host: args.host,
        socket_path: args.socket_path,
        verbose: args.verbose,
        shutdown_grace_ms: args.shutdown_grace_ms,
    };

    run_daemon(config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_grace_default() {
        let args = DaemonArgs::parse_from(["nexus42d"]);
        assert_eq!(args.shutdown_grace_ms, 20000);
    }
}
