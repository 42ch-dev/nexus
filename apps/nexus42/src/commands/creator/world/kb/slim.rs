//! Slim `creator world kb` surface for the `basic-cli` cohort (graph + entity patch only).

use super::service::{self, KbEntityCommand};
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

/// `creator world kb` subcommands available in the basic-cli build.
#[derive(Debug, Subcommand)]
pub enum WorldKbCommand {
    /// Patch a World KB entity through the core direct-writer route (CAS).
    Entity {
        #[command(subcommand)]
        command: KbEntityCommand,
    },
    /// Show the World KB entity graph (direct core read).
    Graph {
        /// World ID (wld_...).
        #[arg(long, value_name = "WORLD_ID")]
        world_id: String,
        /// Include `needs_review = 1` (extraction-suggested) relationships.
        #[arg(long, default_value_t = false)]
        include_suggested: bool,
        /// Emit machine-readable JSON (the `WorldKbGraphResponse` DTO verbatim).
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Run a slim `creator world kb` subcommand.
pub async fn run(cmd: WorldKbCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        WorldKbCommand::Entity { command } => match command {
            KbEntityCommand::Patch {
                world_id,
                entity_id,
                expected_version,
                title,
                body,
                aliases,
                block_type,
                modules,
                json,
            } => {
                service::run_entity_patch(
                    config,
                    world_id,
                    entity_id,
                    expected_version,
                    title,
                    body,
                    aliases,
                    block_type,
                    modules,
                    json,
                )
                .await
            }
        },
        WorldKbCommand::Graph {
            world_id,
            include_suggested,
            json,
        } => service::run_graph(config, world_id, include_suggested, json).await,
    }
}
