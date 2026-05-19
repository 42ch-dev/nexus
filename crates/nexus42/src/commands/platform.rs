//! Platform Command — Platform interaction group.
//!
//! Implements the `nexus42 platform` top-level command with subcommands:
//! - `auth` — User authentication (login/logout/status)
//! - `explore` — Browse and search platform content (platform-only, not proxied)
//! - `context` — Context assembly
//! - `publish` — Publish content (stub, coming soon)
//!
//! # Architecture
//!
//! Thin delegation layer — each variant delegates to the existing command
//! module's `run()` function. No business logic lives here.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum PlatformCommand {
    /// Authentication (login/logout/status)
    Auth {
        #[command(subcommand)]
        command: super::auth::AuthCommand,
    },

    /// Explore browse and search (platform-only, not proxied through local daemon)
    Explore {
        #[command(subcommand)]
        command: super::explore::ExploreCommand,
    },

    /// Context assembly
    Context {
        #[command(subcommand)]
        command: super::context::ContextCommand,
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
        PlatformCommand::Auth { command } => super::auth::run(command, config).await,
        PlatformCommand::Explore { command } => super::explore::run(command).await,
        PlatformCommand::Context { command } => super::context::run(command, config).await,
        PlatformCommand::Publish => {
            println!("publish command coming soon");
            println!("  This feature will be implemented in a follow-up plan.");
            Ok(())
        }
    }
}
