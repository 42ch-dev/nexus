//! World KB daemon-OCC leaves — `creator world kb entity patch` + `kb graph`
//! (V1.175 P1 Task 4, group 4).
//!
//! Thin daemon-HTTP leaves over the **existing** V1.73 world-KB routes
//! (AR-83 #1 / AR-85, F-12):
//! - `POST /v1/daemon/worlds/:world_id/kb/patch-entity` — entity id +
//!   `expected_version` ride in the **body** (there is no
//!   `entities/:id/patch` path route); per-row OCC on
//!   `kb_key_blocks.revision`.
//! - `GET /v1/daemon/worlds/:world_id/kb/graph` — entity graph projection.
//!
//! **Dual-write guard (AR-85 #3):** this is a NEW verb (`kb entity patch`),
//! never an overload of the local-DB `creator world kb edit` (direct
//! SQLite, no OCC — a different code path). Two write paths exist by
//! product decision; cli-spec rows state which store each writes.
//!
//! Error surface: a stale `--expected-version` returns **409
//! `world_kb_conflict`** echoing the stale version as `current_version` +
//! `entity_id` (rendered by `DaemonClient::parse_error_response`); `--help`
//! documents the re-read retry guidance (refetch the graph and reapply).
//! 422 `world_kb_validation_failed` (domain rules) and 400 `bad_request`
//! (other 400s) surface named, non-zero exit (PL-5).
//!
//! Conventions: human-readable default output, `--json` emits the daemon
//! DTO verbatim (generated contract types only — AR-83 #2/#3); write bodies
//! are typed long flags; `--body`/`--modules` are JSON-carrier string flags
//! (`--constraint '<json>'` precedent, F-21).

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::{
    world_kb_patch_entity_request::{
        NexusWorldKbEntityPatch, NexusWorldKbEntityPatchBlockType,
        NexusWorldKbEntityPatchModulesKey, NexusWorldKbEntityPatchModulesValue,
        NexusWorldKbEntityPatchTitle,
    },
    WorldKbGraphResponse, WorldKbPatchEntityRequest, WorldKbPatchEntityResponse,
};
use std::collections::HashMap;

/// `creator world kb entity` verbs (daemon OCC surface).
#[derive(Debug, Subcommand)]
pub enum KbEntityCommand {
    /// Patch a World KB entity through the daemon OCC route
    /// (`POST /v1/daemon/worlds/:world_id/kb/patch-entity`).
    ///
    /// CAS-guarded: `--expected-version` must match the per-row version
    /// observed on the last canonical read (`kb graph`). On 409
    /// `world_kb_conflict`, refetch the graph and reapply with the new
    /// version. Distinct from the local-DB `creator world kb edit` (direct
    /// SQLite, no OCC) — this leaf writes through the daemon only.
    Patch {
        /// World ID (wld_...).
        #[arg(long, value_name = "WORLD_ID")]
        world_id: String,
        /// Entity ID (kb_...) to patch.
        #[arg(long, value_name = "ENTITY_ID")]
        entity_id: String,
        /// Per-row version observed on the last canonical read (CAS). On a
        /// 409 `world_kb_conflict`, refetch the graph (`creator world kb
        /// graph`) and reapply with the new version.
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
        /// Per-entry functional-dialect modules JSON (first-level key
        /// upsert; `{}` is a no-op).
        #[arg(long)]
        modules: Option<String>,
        /// Emit machine-readable JSON (the `WorldKbPatchEntityResponse`
        /// DTO verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `creator world kb` daemon subcommands.
#[derive(Debug, Subcommand)]
pub enum KbDaemonCommand {
    /// Patch a World KB entity through the daemon OCC route (see
    /// `entity patch --help`).
    Entity {
        #[command(subcommand)]
        command: KbEntityCommand,
    },
    /// Show the World KB entity graph (`GET /v1/daemon/worlds/:world_id/kb/graph`).
    ///
    /// Prints the `WorldKbGraphResponse` DTO: entities (with per-row
    /// `version` — the `--expected-version` for `entity patch`),
    /// relationships, and source anchors. `--json` emits the DTO verbatim.
    Graph {
        /// World ID (wld_...).
        #[arg(long, value_name = "WORLD_ID")]
        world_id: String,
        /// Include `needs_review = 1` (extraction-suggested) relationships.
        #[arg(long, default_value_t = false)]
        include_suggested: bool,
        /// Emit machine-readable JSON (the `WorldKbGraphResponse` DTO
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
    /// Info point.
    #[value(name = "info_point")]
    InfoPoint,
    Event,
    Species,
    Faction,
    /// Magic system.
    #[value(name = "magic_system")]
    MagicSystem,
    Technology,
    Deity,
    Level,
    /// Economy tier.
    #[value(name = "economy_tier")]
    EconomyTier,
    Dialogue,
    Beat,
    Act,
    Era,
}

impl BlockTypeArg {
    const fn to_generated(self) -> NexusWorldKbEntityPatchBlockType {
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

/// Run a `creator world kb entity|graph` subcommand.
///
/// # Errors
///
/// Returns `CliError` on invalid input (no patch field, unparseable
/// `--body`/`--modules` JSON) or any daemon API / network failure (409
/// `world_kb_conflict`, 404 `not_found`, 422 `world_kb_validation_failed`,
/// 400 `bad_request` for other 400s — all named, non-zero exit).
pub async fn run(cmd: KbDaemonCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);
    match cmd {
        KbDaemonCommand::Entity { command } => match command {
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
                entity_patch(
                    &client,
                    &world_id,
                    &entity_id,
                    expected_version,
                    title.as_deref(),
                    body.as_deref(),
                    aliases.as_deref(),
                    block_type,
                    modules.as_deref(),
                    json,
                )
                .await
            }
        },
        KbDaemonCommand::Graph {
            world_id,
            include_suggested,
            json,
        } => kb_graph(&client, &world_id, include_suggested, json).await,
    }
}

/// `creator world kb entity patch --world-id <id> --entity-id <id>
/// --expected-version N [--title …] [--body <json>] [--aliases a,b]
/// [--block-type <t>] [--modules <json>]` — patch a World KB entity through
/// the daemon OCC route (`POST /v1/daemon/worlds/:world_id/kb/patch-entity`).
///
/// # Errors
///
/// Returns a named `CliError::Other` when no patch field is given or
/// `--body`/`--modules` do not parse as JSON, or `CliError` for daemon /
/// network failures (409 `world_kb_conflict`, 404 `not_found`, 422
/// `world_kb_validation_failed`, 400 `bad_request` for other 400s).
#[allow(clippy::too_many_arguments)] // CLI param plumbing — house pattern
async fn entity_patch(
    client: &DaemonClient,
    world_id: &str,
    entity_id: &str,
    expected_version: u64,
    title: Option<&str>,
    body: Option<&str>,
    aliases: Option<&[String]>,
    block_type: Option<BlockTypeArg>,
    modules: Option<&str>,
    json: bool,
) -> Result<()> {
    let mut patch = NexusWorldKbEntityPatch {
        aliases: aliases.map_or_else(Vec::new, <[String]>::to_vec),
        block_type: block_type.map(BlockTypeArg::to_generated),
        body: serde_json::Map::new(),
        modules: HashMap::new(),
        title: None,
    };
    if let Some(body_str) = body {
        let value: serde_json::Value = serde_json::from_str(body_str)
            .map_err(|e| CliError::Other(format!("--body must be a JSON object: {e}")))?;
        let obj = value
            .as_object()
            .ok_or_else(|| CliError::Other("--body must be a JSON object".to_string()))?;
        patch.body.clone_from(obj);
    }
    if let Some(modules_str) = modules {
        let value: serde_json::Value = serde_json::from_str(modules_str)
            .map_err(|e| CliError::Other(format!("--modules must be a JSON object: {e}")))?;
        let obj = value
            .as_object()
            .ok_or_else(|| CliError::Other("--modules must be a JSON object".to_string()))?;
        for (key, val) in obj {
            let k = key
                .parse::<NexusWorldKbEntityPatchModulesKey>()
                .map_err(|e| {
                    CliError::Other(format!(
                        "--modules key '{key}' is invalid (must match ^[a-z][a-z0-9_-]*$): {e}"
                    ))
                })?;
            let v = match val {
                serde_json::Value::Object(m) => {
                    NexusWorldKbEntityPatchModulesValue::Variant0(m.clone())
                }
                serde_json::Value::Array(a) => {
                    NexusWorldKbEntityPatchModulesValue::Variant1(a.clone())
                }
                _ => {
                    return Err(CliError::Other(format!(
                        "--modules value for '{key}' must be an object or array"
                    )));
                }
            };
            patch.modules.insert(k, v);
        }
    }
    if let Some(title_str) = title {
        patch.title = Some(
            title_str
                .parse::<NexusWorldKbEntityPatchTitle>()
                .map_err(|e| CliError::Other(format!("--title is invalid: {e}")))?,
        );
    }
    if patch.title.is_none()
        && patch.body.is_empty()
        && patch.aliases.is_empty()
        && patch.block_type.is_none()
        && patch.modules.is_empty()
    {
        return Err(CliError::Other(
            "at least one of --title/--body/--aliases/--block-type/--modules must be provided"
                .to_string(),
        ));
    }

    let req = WorldKbPatchEntityRequest {
        entity_id: entity_id.to_string(),
        expected_version,
        patch,
    };
    let resp: WorldKbPatchEntityResponse = client
        .post(
            &format!("/v1/daemon/worlds/{world_id}/kb/patch-entity"),
            &req,
        )
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
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

/// `creator world kb graph --world-id <id> [--include-suggested] [--json]` —
/// show the World KB entity graph (`GET /v1/daemon/worlds/:world_id/kb/graph`).
///
/// # Errors
///
/// Returns `CliError` for daemon / network failures (404 `not_found` for
/// an unknown world, 403 foreign world, 400 `bad_request` for other 400s).
async fn kb_graph(
    client: &DaemonClient,
    world_id: &str,
    include_suggested: bool,
    json: bool,
) -> Result<()> {
    let path = if include_suggested {
        format!("/v1/daemon/worlds/{world_id}/kb/graph?include_suggested=true")
    } else {
        format!("/v1/daemon/worlds/{world_id}/kb/graph")
    };
    let resp: WorldKbGraphResponse = client.get(&path).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
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
    Ok(())
}
