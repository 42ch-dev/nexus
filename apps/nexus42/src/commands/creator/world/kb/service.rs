//! World KB direct core service leaves — `creator world kb graph` + `entity patch`.
//!
//! v1.189 P1-T3: graph/patch invoke `nexus-core` with `CoreAccess::DirectWriter`
//! (same path when `legacy-cli` is enabled — no daemon HTTP for these verbs).

use crate::config::{user_home_dir, CliConfig};
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
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};
use std::collections::HashMap;

/// Relationship projection cap enforced by core graph projection (mirrors daemon constant).
const GRAPH_RELATIONSHIP_CAP: usize = 1000;

/// `creator world kb entity` verbs (direct core OCC surface).
#[derive(Debug, Subcommand)]
pub enum KbEntityCommand {
    /// Patch a World KB entity through the core direct-writer route (CAS on revision).
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
        /// Replacement body JSON.
        #[arg(long)]
        body: Option<String>,
        /// Replacement alias list (comma-separated).
        #[arg(long, value_delimiter = ',')]
        aliases: Option<Vec<String>>,
        /// Re-classify the entity (valid `BlockType`).
        #[arg(long, value_enum)]
        block_type: Option<BlockTypeArg>,
        /// Per-entry functional-dialect modules JSON (first-level key upsert).
        #[arg(long)]
        modules: Option<String>,
        /// Emit machine-readable JSON (the `WorldKbPatchEntityResponse` DTO verbatim).
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

async fn open_direct_core(_config: &CliConfig) -> Result<CoreService> {
    let user_home = user_home_dir().map_err(|e| CliError::Config(e.to_string()))?;
    CoreService::open(CoreOpenOptions {
        user_home,
        access: CoreAccess::DirectWriter,
    })
    .await
    .map_err(map_core_error)
}

fn map_core_error(err: CoreError) -> CliError {
    match err {
        CoreError::Uninitialized | CoreError::AuthRequired => CliError::CreatorNotSelected,
        CoreError::Forbidden { resource } => CliError::Api {
            status: 403,
            message: resource,
        },
        CoreError::NotFound { resource } => CliError::Api {
            status: 404,
            message: resource,
        },
        CoreError::InvalidInput { field, reason } => {
            CliError::Other(format!("invalid input ({field}): {reason}"))
        }
        CoreError::WorldKbConflict(conflict) => CliError::VersionConflict {
            table: "kb_key_blocks".to_string(),
            row_id: conflict.entity_id,
            expected_version: conflict.current_version as i64,
            actual_version: Some(conflict.current_version as i64),
        },
        CoreError::WorldKbValidation(v) => CliError::Api {
            status: 422,
            message: format!(
                "world_kb_validation_failed: {}",
                v.validation_summary.errors.join("; ")
            ),
        },
        CoreError::OwnerBusy | CoreError::Busy => CliError::Locked {
            holder_pid: 0,
            holder_name: "workspace writer".to_string(),
            stale: false,
        },
        CoreError::WriterFenced | CoreError::SchemaMismatch => CliError::Config(
            "workspace writer protocol mismatch — upgrade or restart host".to_string(),
        ),
        CoreError::Closing | CoreError::Interrupted => CliError::Other(err.to_string()),
        CoreError::Internal { category } => CliError::Other(category),
    }
}

/// Run `creator world kb entity patch`.
#[allow(clippy::too_many_arguments)]
pub async fn run_entity_patch(
    config: &CliConfig,
    world_id: String,
    entity_id: String,
    expected_version: u64,
    title: Option<String>,
    body: Option<String>,
    aliases: Option<Vec<String>>,
    block_type: Option<BlockTypeArg>,
    modules: Option<String>,
    json: bool,
) -> Result<()> {
    let mut patch = NexusWorldKbEntityPatch {
        aliases: aliases.unwrap_or_default(),
        block_type: block_type.map(BlockTypeArg::to_generated),
        body: serde_json::Map::new(),
        modules: HashMap::new(),
        title: None,
    };
    if let Some(body_str) = body {
        let value: serde_json::Value = serde_json::from_str(&body_str)
            .map_err(|e| CliError::Other(format!("--body must be a JSON object: {e}")))?;
        let obj = value
            .as_object()
            .ok_or_else(|| CliError::Other("--body must be a JSON object".to_string()))?;
        patch.body.clone_from(obj);
    }
    if let Some(modules_str) = modules {
        let value: serde_json::Value = serde_json::from_str(&modules_str)
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
                    NexusWorldKbEntityPatchModulesValue::Object(m.clone())
                }
                serde_json::Value::Array(a) => {
                    NexusWorldKbEntityPatchModulesValue::Array(a.clone())
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
        entity_id: entity_id.clone(),
        expected_version,
        patch,
    };

    let core = open_direct_core(config).await?;
    let principal = core.active_principal().await.map_err(map_core_error)?;
    let resp = core
        .patch_world_kb_entity(&principal, world_id.clone(), req)
        .await
        .map_err(map_core_error)?;
    let _ = core.close().await;

    render_patch_response(&world_id, &entity_id, &resp, json)
}

/// Run `creator world kb graph`.
pub async fn run_graph(
    config: &CliConfig,
    world_id: String,
    include_suggested: bool,
    json: bool,
) -> Result<()> {
    let core = open_direct_core(config).await?;
    let principal = core.active_principal().await.map_err(map_core_error)?;
    let resp = core
        .world_kb_graph(&principal, world_id.clone(), include_suggested)
        .await
        .map_err(map_core_error)?;
    let _ = core.close().await;

    render_graph_response(&world_id, &resp, json)
}

fn render_patch_response(
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


fn render_graph_response(world_id: &str, resp: &WorldKbGraphResponse, json: bool) -> Result<()> {
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
            "\nNote: the graph projects at most {GRAPH_RELATIONSHIP_CAP} relationships (no wire              `truncated` flag exists yet); the graph may be truncated."
        );
    }
    Ok(())
}

