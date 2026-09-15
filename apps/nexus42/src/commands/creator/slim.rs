//! Slim `creator` surface for the `basic-cli` cohort.

use super::world;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

/// Creator subcommands available in the basic-cli build.
#[derive(Debug, Subcommand)]
pub enum CreatorCommand {
    /// World KB graph + entity patch.
    World {
        #[command(subcommand)]
        command: world::WorldCommand,
    },
}

/// Run a slim creator subcommand.
pub async fn run(cmd: CreatorCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        CreatorCommand::World { command } => world::run(command, config).await,
    }
}
