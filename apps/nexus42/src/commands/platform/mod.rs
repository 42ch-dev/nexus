//! Platform Command — Platform interaction group.
//!
//! Implements the `nexus42 platform` top-level command with subcommands:
//! - `auth` — User authentication (login/logout/status)
//! - `context` — Context assembly (`assemble-moment` is the local SSOT)
//! - `sync` — Synchronize workspace with platform (V1.35 canonical location)
//!
//! v1.193 P1-T5: the deferred `explore` and `publish` leaves were removed —
//! neither ever performed work (the first always returned `CliError::Config`,
//! the second printed a coming-soon notice and returned `Ok`). Platform content
//! browsing is served by the platform HTTP API directly, not by a local leaf.

pub mod auth;
pub mod context;
pub mod sync;

use crate::commands::sync::SyncCommand;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum PlatformCommand {
    /// Authentication (login/logout/status)
    Auth {
        #[command(subcommand)]
        command: auth::AuthCommand,
    },

    /// Context assembly
    Context {
        // ContextCommand is large (many `Option<String>` flags, incl. `--stage`);
        // boxed to keep `PlatformCommand` small (type-size hygiene; no behavior).
        #[command(subcommand)]
        command: Box<context::ContextCommand>,
    },

    /// Synchronize workspace with platform (pull, push, status, resolve, world, retry)
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },
}

/// Run platform command.
///
/// # Errors
///
/// Returns `CliError` if the delegated command fails.
pub async fn run(cmd: PlatformCommand, config: &CliConfig, _output_format: &str) -> Result<()> {
    match cmd {
        PlatformCommand::Auth { command } => auth::run(command, config).await,
        PlatformCommand::Context { command } => context::run(*command, config).await,
        PlatformCommand::Sync { command } => sync::run(command, config).await,
    }
}
