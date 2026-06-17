//! World KB key-block author surface — `creator world kb list/show/edit/delete`.
//!
//! V1.50 T-B P0. This is the canonical author CLI for inspecting and editing
//! World-scoped `KeyBlock` rows (per entity-scope-model.md §5.5), distinct from
//! the legacy ingest path `creator kb --scope world`.
//!
//! Read paths (`list`/`show`) are local-first and do not perform an owner gate.
//! Edit/delete author identity gating and Novel validation are added in a
//! follow-up commit (T3–T5).

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_kb::key_block::KeyBlock;
use nexus_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use sqlx::SqlitePool;

/// `creator world kb` subcommands.
#[derive(Debug, Subcommand)]
pub enum WorldKbCommand {
    /// List all `KeyBlocks` in a world (id / `canonical_name` / `block_type` / status)
    List {
        /// World reference — the world ID (e.g. `wld_abc123`)
        world_ref: String,
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Show full body + provenance + status for a single `KeyBlock`
    Show {
        /// World reference — the world ID (e.g. `wld_abc123`)
        world_ref: String,
        /// `KeyBlock` ID (e.g. `kb_...`)
        block_id: String,
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },
}

/// Run a `creator world kb` subcommand.
///
/// Resolves the active workspace pool, then delegates to the hermetic logic
/// functions below.
///
/// # Errors
///
/// Returns `CliError` if the active creator is unset or the database is
/// unavailable.
// CLI entry-point runs on a single-threaded tokio runtime — Send not required.
#[allow(clippy::future_not_send)]
pub async fn run(cmd: WorldKbCommand, config: &CliConfig) -> Result<()> {
    let _creator_id = super::active_creator_id(config)?;
    let pool = super::open_workspace_pool(config).await?;
    match cmd {
        WorldKbCommand::List { world_ref, json } => kb_list(&pool, &world_ref, json).await,
        WorldKbCommand::Show {
            world_ref,
            block_id,
            json,
        } => kb_show(&pool, &world_ref, &block_id, json).await,
    }
}

// ── Hermetic logic functions ──────────────────────────────────────────
//
// These take an explicit `&SqlitePool` so integration tests can drive them
// against a fresh temp DB without touching `$HOME`-resolved paths. They are
// `pub` specifically to enable hermetic round-trip tests; the `run` entrypoint
// above remains the only caller that resolves the pool from `CliConfig`.

/// `creator world kb list` — list all active `KeyBlocks` in a world.
///
/// # Errors
///
/// Returns `CliError` if the underlying KB store query or JSON serialization fails.
pub async fn kb_list(pool: &SqlitePool, world_id: &str, json: bool) -> Result<()> {
    let store = SqliteKbStore::new(pool.clone());
    let blocks = store
        .list_by_world(world_id)
        .await
        .map_err(|e| CliError::Other(format!("World KB list failed for {world_id}: {e}")))?;

    if json {
        let items: Vec<serde_json::Value> = blocks.iter().map(block_summary_json).collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if blocks.is_empty() {
        println!("No key blocks in world {world_id}.");
        return Ok(());
    }

    println!("Key blocks in world {world_id}:");
    println!("{:<20} {:<15} {:<30} STATUS", "BLOCK_ID", "TYPE", "NAME");
    for block in &blocks {
        println!(
            "{:<20} {:<15} {:<30} {}",
            block.key_block_id,
            format!("{:?}", block.block_type),
            block.canonical_name,
            block.status
        );
    }
    Ok(())
}

/// `creator world kb show` — show full body + provenance + status.
///
/// # Errors
///
/// Returns `CliError` if the block is missing, does not belong to the world,
/// or JSON serialization fails.
pub async fn kb_show(pool: &SqlitePool, world_id: &str, block_id: &str, json: bool) -> Result<()> {
    let store = SqliteKbStore::new(pool.clone());
    let block = store
        .get_key_block(block_id)
        .await
        .map_err(|e| CliError::Other(format!("Key block '{block_id}' not found: {e}")))?;

    if block.world_id != world_id {
        return Err(CliError::Other(format!(
            "Key block '{block_id}' does not belong to world '{world_id}' \
             (it belongs to '{}').",
            block.world_id
        )));
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&block)?);
        return Ok(());
    }

    println!("Key Block: {}", block.key_block_id);
    println!("  World:      {}", block.world_id);
    println!("  Name:       {}", block.canonical_name);
    println!("  Type:       {:?}", block.block_type);
    println!("  Status:     {}", block.status);
    if let Some(rev) = block.revision {
        println!("  Revision:   {rev}");
    }
    println!("  Created:    {}", block.created_at);
    if let Some(updated) = &block.updated_at {
        println!("  Updated:    {updated}");
    }
    if let Some(body) = &block.body {
        if let Some(summary) = &body.summary {
            println!("  Summary:    {summary}");
        }
        if let Some(attrs) = &body.attributes {
            println!("  Attributes: {attrs}");
        }
        if let Some(tags) = &body.tags {
            println!("  Tags:       {}", tags.join(", "));
        }
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Build the JSON summary object for `--json` list output.
fn block_summary_json(block: &KeyBlock) -> serde_json::Value {
    serde_json::json!({
        "key_block_id": block.key_block_id,
        "canonical_name": block.canonical_name,
        "block_type": serde_json::to_value(block.block_type)
            .unwrap_or_else(|_| serde_json::json!(format!("{:?}", block.block_type))),
        "status": block.status,
    })
}
