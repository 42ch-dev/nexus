//! Platform Command — Platform interaction group.
//!
//! Implements the `nexus42 platform` top-level command with subcommands:
//! - `auth` — User authentication (login/logout/status)
//! - `explore` — Browse and search platform content (platform-only, not proxied)
//! - `context` — Context assembly
//! - `publish` — Publish content (stub, coming soon)
//! - `sync` — Synchronize workspace with platform (V1.35 canonical location)

pub mod auth;
pub mod context;
pub mod explore;
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

    /// Explore browse and search (platform-only, not proxied through local daemon)
    Explore {
        #[command(subcommand)]
        command: explore::ExploreCommand,
    },

    /// Context assembly
    Context {
        #[command(subcommand)]
        command: context::ContextCommand,
    },

    /// Synchronize workspace with platform (pull, push, status, resolve, world, retry)
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },

    /// Publish content (coming soon)
    Publish,
}

/// Run platform command.
///
/// # Errors
///
/// Returns `CliError` if the delegated command fails.
pub async fn run(cmd: PlatformCommand, config: &CliConfig, _output_format: &str) -> Result<()> {
    match cmd {
        PlatformCommand::Auth { command } => auth::run(command, config).await,
        PlatformCommand::Explore { command } => explore::run(command).await,
        PlatformCommand::Context { command } => context::run(command, config).await,
        PlatformCommand::Sync { command } => sync::run(command, config).await,
        PlatformCommand::Publish => {
            println!("publish command coming soon");
            println!("  This feature will be implemented in a follow-up plan.");
            Ok(())
        }
    }
}
