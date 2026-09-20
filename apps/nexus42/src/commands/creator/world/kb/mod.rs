//! World KB module — router plus the shared `graph` / `entity patch` surface.
//!
//! v1.189 P1-T3 (architecture §2.1): the clap declarations and the human/JSON
//! formatting for `creator world kb graph` and `creator world kb entity patch`
//! live HERE, in the existing KB module; only the service invocation lives in
//! [`service`]. Both cohorts (`basic-cli`, `legacy-cli`) reuse these
//! declarations, so these verbs have exactly one parser in the product.

pub mod service;

use crate::config::CliConfig;
use crate::errors::Result;
use clap::{Args, Subcommand};
use nexus_contracts::{
    world_kb_patch_entity_request::NexusWorldKbEntityPatchBlockType, WorldKbGraphResponse,
    WorldKbPatchEntityResponse,
};

/// Relationship projection cap enforced by the core graph projection
/// (mirrors the daemon constant). The wire DTO carries no `truncated` flag, so
/// the CLI mirrors the constant to surface a possibly truncated listing
/// honestly on the human path (qc3 W-002).
const GRAPH_RELATIONSHIP_CAP: usize = 1000;

/// `creator world kb entity` verbs (core direct-writer OCC surface).
#[derive(Debug, Subcommand)]
pub enum KbEntityCommand {
    /// Patch a World KB entity through the core direct-writer route (CAS on revision).
    ///
    /// CAS-guarded: `--expected-version` must match the per-row version observed
    /// on the last canonical read (`creator world kb graph`). On a
    /// `world_kb_conflict`, refetch the graph and reapply with the new version.
    /// Distinct from the local-DB `creator world kb edit` (direct SQLite, no
    /// OCC) — this leaf writes through the core direct-writer only.
    Patch {
        /// World ID (wld_...).
        #[arg(long, value_name = "WORLD_ID")]
        world_id: String,
        /// Entity ID (kb_...) to patch.
        #[arg(long, value_name = "ENTITY_ID")]
        entity_id: String,
        /// Per-row version observed on the last canonical read (CAS).
        #[arg(long, value_name = "N")]
        expected_version: u64,
        /// New canonical name (display title).
        #[arg(long)]
        title: Option<String>,
        /// Replacement body JSON (`{"summary":...,"attributes":...,"tags":...}`).
        #[arg(long)]
        body: Option<String>,
        /// Replacement alias list (comma-separated).
        #[arg(long, value_delimiter = ',')]
        aliases: Option<Vec<String>>,
        /// Re-classify the entity (valid `BlockType`).
        #[arg(long, value_enum)]
        block_type: Option<BlockTypeArg>,
        /// Per-entry functional-dialect modules JSON (first-level key upsert;
        /// `{}` is a no-op).
        #[arg(long)]
        modules: Option<String>,
        /// Author audience: `shared`, `author-only`, or `character-private`.
        /// Omitted preserves the stored holder/disclosure pair; `shared`
        /// clears both. Governance moves under the same `--expected-version`
        /// CAS as the content.
        #[arg(long, value_name = "AUDIENCE")]
        audience: Option<String>,
        /// Character a `character-private` audience resolves to (must be a
        /// Character this Creator owns and that is bound to this World).
        #[arg(long, value_name = "CHARACTER_ID", requires = "audience")]
        audience_character: Option<String>,
        /// Emit machine-readable JSON (the `WorldKbPatchEntityResponse` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `--block-type` value for `entity patch` (V1.73 wire vocabulary).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum BlockTypeArg {
    Character,
    Ability,
    Scene,
    Organization,
    Item,
    Conflict,
    #[value(name = "info_point")]
    InfoPoint,
    Event,
    Species,
    Faction,
    #[value(name = "magic_system")]
    MagicSystem,
    Technology,
    Deity,
    Level,
    #[value(name = "economy_tier")]
    EconomyTier,
    Dialogue,
    Beat,
    Act,
    Era,
}

impl BlockTypeArg {
    pub(crate) const fn to_generated(self) -> NexusWorldKbEntityPatchBlockType {
        match self {
            Self::Character => NexusWorldKbEntityPatchBlockType::Character,
            Self::Ability => NexusWorldKbEntityPatchBlockType::Ability,
            Self::Scene => NexusWorldKbEntityPatchBlockType::Scene,
            Self::Organization => NexusWorldKbEntityPatchBlockType::Organization,
            Self::Item => NexusWorldKbEntityPatchBlockType::Item,
            Self::Conflict => NexusWorldKbEntityPatchBlockType::Conflict,
            Self::InfoPoint => NexusWorldKbEntityPatchBlockType::InfoPoint,
            Self::Event => NexusWorldKbEntityPatchBlockType::Event,
            Self::Species => NexusWorldKbEntityPatchBlockType::Species,
            Self::Faction => NexusWorldKbEntityPatchBlockType::Faction,
            Self::MagicSystem => NexusWorldKbEntityPatchBlockType::MagicSystem,
            Self::Technology => NexusWorldKbEntityPatchBlockType::Technology,
            Self::Deity => NexusWorldKbEntityPatchBlockType::Deity,
            Self::Level => NexusWorldKbEntityPatchBlockType::Level,
            Self::EconomyTier => NexusWorldKbEntityPatchBlockType::EconomyTier,
            Self::Dialogue => NexusWorldKbEntityPatchBlockType::Dialogue,
            Self::Beat => NexusWorldKbEntityPatchBlockType::Beat,
            Self::Act => NexusWorldKbEntityPatchBlockType::Act,
            Self::Era => NexusWorldKbEntityPatchBlockType::Era,
        }
    }
}

/// `creator world kb graph` flags — shared by both cohorts.
///
/// Flattened into each cohort's `WorldKbCommand::Graph` variant so the
/// declaration exists once while the CLI shape stays
/// `creator world kb graph --world-id ...`.
#[derive(Debug, Args)]
pub struct GraphArgs {
    /// World ID (wld_...).
    #[arg(long, value_name = "WORLD_ID")]
    pub world_id: String,
    /// Include `needs_review = 1` (extraction-suggested) relationships.
    #[arg(long, default_value_t = false)]
    pub include_suggested: bool,
    /// Emit machine-readable JSON (the `WorldKbGraphResponse` DTO verbatim)
    /// instead of human text.
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

/// Human/JSON rendering for a successful `entity patch`.
pub(crate) fn render_patch_response(
    world_id: &str,
    entity_id: &str,
    resp: &WorldKbPatchEntityResponse,
    json: bool,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(resp)?);
    } else {
        println!(
            "Patched entity '{entity_id}' in world '{world_id}' (new version {}).",
            resp.version
        );
        println!("  canonical_name: {}", resp.entity.canonical_name.as_str());
        if !resp.validation_summary.errors.is_empty() {
            println!("  validation warnings:");
            for e in &resp.validation_summary.errors {
                println!("    - {e}");
            }
        }
    }
    Ok(())
}

/// Human/JSON rendering for a successful `graph` read.
pub(crate) fn render_graph_response(
    world_id: &str,
    resp: &WorldKbGraphResponse,
    json: bool,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(resp)?);
        return Ok(());
    }
    println!("World KB graph for world '{world_id}':\n");
    println!(
        "{:<36} {:<24} {:12} {:8} VERSION",
        "KEY_BLOCK_ID", "CANONICAL_NAME", "BLOCK_TYPE", "STATUS"
    );
    println!("{}", "-".repeat(100));
    for e in &resp.entities {
        println!(
            "{:<36} {:<24} {:12} {:8} {}",
            e.key_block_id,
            e.canonical_name.as_str(),
            e.block_type,
            e.status,
            e.version
        );
    }
    println!("\n{} entities", resp.entities.len());
    if !resp.relationships.is_empty() {
        println!("{} relationships", resp.relationships.len());
    }
    if !resp.source_anchors.is_empty() {
        println!("{} source anchors", resp.source_anchors.len());
    }
    if resp.relationships.len() >= GRAPH_RELATIONSHIP_CAP {
        println!(
            "Note: the graph projects at most {GRAPH_RELATIONSHIP_CAP} relationships \
             (no wire `truncated` flag exists yet); the listing may be truncated."
        );
    }
    Ok(())
}

// ── cohort router ─────────────────────────────────────────────────────────────
//
// `legacy-cli` owns the full author surface (list/show/edit/delete/pending/…)
// and reuses the shared graph/patch declarations above. `basic-cli` — the
// daemon-free cohort — exposes ONLY those shared graph/patch declarations.
//
// Both cohorts route `entity patch` / `graph` through [`service`], whose
// `open_direct_core` owns admission (`require_materialized_workspace`) and runs
// it *before* `CoreService::open`. `legacy_impl::run` is the only place a local
// KB leaf may open the legacy workspace pool, and the order there is
// load-bearing: `open_workspace_pool` runs `Schema::init`, which migrates — and
// therefore creates — the selected workspace, so the seam's pre-flight must have
// admitted the selection first.

#[cfg(feature = "legacy-cli")]
#[path = "legacy_impl.rs"]
mod legacy_impl;
#[cfg(feature = "legacy-cli")]
pub use legacy_impl::*;

#[cfg(all(feature = "basic-cli", not(feature = "legacy-cli")))]
mod slim;
#[cfg(all(feature = "basic-cli", not(feature = "legacy-cli")))]
pub use slim::*;

/// Run `creator world kb graph` (both cohorts).
pub(crate) async fn run_graph(args: GraphArgs, config: &CliConfig) -> Result<()> {
    service::run_graph(config, args.world_id, args.include_suggested, args.json).await
}
