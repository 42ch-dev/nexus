//! Slim `creator world kb` surface for the `basic-cli` cohort.
//!
//! The daemon-free cohort exposes ONLY the shared graph/patch declarations
//! from the parent KB module; everything else in the author surface
//! (`list`/`show`/`edit`/`delete`/`pending`/`adopt`/`pack`/…) is daemon- or
//! local-DB-mediated and therefore `legacy-cli`-only.

use super::{service, GraphArgs, KbEntityCommand};
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

/// `creator world kb` subcommands available in the `basic-cli` build.
#[derive(Debug, Subcommand)]
pub enum WorldKbCommand {
    /// Patch a World KB entity through the core direct-writer route (CAS on revision).
    ///
    /// CAS-guarded: `--expected-version` must match the per-row version observed
    /// on the last canonical read (`creator world kb graph`). On a
    /// `world_kb_conflict`, refetch the graph and reapply with the new version.
    Entity {
        #[command(subcommand)]
        command: KbEntityCommand,
    },
    /// Show the World KB entity graph (direct core read).
    ///
    /// Prints the `WorldKbGraphResponse` DTO: entities (with per-row `version`
    /// — the `--expected-version` for `entity patch`), relationships, and
    /// source anchors. `--json` emits the DTO verbatim.
    Graph {
        #[command(flatten)]
        args: GraphArgs,
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
        WorldKbCommand::Graph { args } => super::run_graph(args, config).await,
    }
}
