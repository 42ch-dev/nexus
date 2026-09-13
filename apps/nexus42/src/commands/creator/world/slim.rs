//! Slim `creator world` surface for the `basic-cli` cohort.

use super::kb;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

/// World subcommands available in the basic-cli build.
#[derive(Debug, Subcommand)]
pub enum WorldCommand {
    /// World KB graph + entity patch (direct core writer).
    Kb {
        #[command(subcommand)]
        command: kb::WorldKbCommand,
    },
}

/// Run a slim world subcommand.
pub async fn run(cmd: WorldCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        WorldCommand::Kb { command } => kb::run(command, config).await,
    }
}
