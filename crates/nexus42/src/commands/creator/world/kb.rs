//! World KB key-block author surface — `creator world kb list/show/edit/delete`.
//!
//! V1.50 T-B P0. This is the canonical author CLI for inspecting and editing
//! World-scoped `KeyBlock` rows (per entity-scope-model.md §5.5), distinct from
//! the legacy ingest path `creator kb --scope world`.
//!
//! # Author identity
//!
//! `KeyBlock`s are World-scoped (entity-scope-model §1.2/§5.1). The only
//! ownership field available on a World KB row is `narrative_worlds.owner_creator_id`
//! (there is no direct `works.creator_id` linkage on `kb_key_blocks`). Therefore
//! `edit`/`delete` gate on the **world owner** matching the active creator;
//! a cross-author attempt returns `403` with stable code `WORLD_KB_FORBIDDEN`.
//!
//! # Validation
//!
//! `edit` constructs a `SqliteKbStore` with `ValidationMode::Novel` so that
//! `update_key_block` re-runs the V1.40 P1 novel-profile validation
//! (`body.attributes.novel_category` requirements, per entity-scope-model §5.1.1).
//!
//! Read paths (`list`/`show`) are local-first and do not perform an owner gate.

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_kb::key_block::{KeyBlock, KeyBlockBody};
use nexus_kb::store::KbStoreError;
use nexus_kb::validation::ValidationMode;
use nexus_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use sqlx::SqlitePool;

/// Stable error code embedded in cross-author `403` messages.
pub const WORLD_KB_FORBIDDEN_CODE: &str = "WORLD_KB_FORBIDDEN";

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

    /// Edit a `KeyBlock` body in place (re-runs `ValidationMode::Novel`)
    Edit {
        /// World reference — the world ID (e.g. `wld_abc123`)
        world_ref: String,
        /// `KeyBlock` ID (e.g. `kb_...`)
        block_id: String,
        /// New body as JSON (`{"summary":...,"attributes":...,"tags":...}`)
        #[arg(long)]
        body: String,
        /// Emit machine-readable JSON confirmation
        #[arg(long)]
        json: bool,
    },

    /// Delete a `KeyBlock` (soft-delete; prompts unless `--yes`)
    Delete {
        /// World reference — the world ID (e.g. `wld_abc123`)
        world_ref: String,
        /// `KeyBlock` ID (e.g. `kb_...`)
        block_id: String,
        /// Skip the interactive confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

/// Run a `creator world kb` subcommand.
///
/// Resolves the active workspace pool and creator, then delegates to the
/// hermetic logic functions below.
///
/// # Errors
///
/// Returns `CliError` if the active creator is unset, the database is
/// unavailable, the world is not found, or the active creator does not own
/// the world (edit/delete only).
// CLI entry-point runs on a single-threaded tokio runtime — Send not required.
#[allow(clippy::future_not_send)]
pub async fn run(cmd: WorldKbCommand, config: &CliConfig) -> Result<()> {
    let creator_id = super::active_creator_id(config)?;
    let pool = super::open_workspace_pool(config).await?;
    match cmd {
        WorldKbCommand::List { world_ref, json } => kb_list(&pool, &world_ref, json).await,
        WorldKbCommand::Show {
            world_ref,
            block_id,
            json,
        } => kb_show(&pool, &world_ref, &block_id, json).await,
        WorldKbCommand::Edit {
            world_ref,
            block_id,
            body,
            json,
        } => kb_edit(&pool, &creator_id, &world_ref, &block_id, &body, json).await,
        WorldKbCommand::Delete {
            world_ref,
            block_id,
            yes,
        } => kb_delete(&pool, &creator_id, &world_ref, &block_id, yes).await,
    }
}

// ── Hermetic logic functions ──────────────────────────────────────────
//
// These take an explicit `&SqlitePool` (+ `creator_id` where an owner gate is
// needed) so integration tests can drive them against a fresh temp DB without
// touching `$HOME`-resolved paths. They are `pub` specifically to enable the
// `tests/world_kb_cli.rs` and `tests/world_kb_authz.rs` hermetic round-trips;
// the `run` entrypoint above remains the only caller that resolves the pool
// from `CliConfig`.

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
        .map_err(|e| map_kb_store_error("show", block_id, world_id, e))?;
    require_block_in_world(&block, world_id, block_id)?;

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

/// `creator world kb edit` — edit body in place with Novel validation re-run.
///
/// # Errors
///
/// Returns `CliError` (`Api { status: 403, .. }` with code `WORLD_KB_FORBIDDEN`)
/// if the active creator does not own the world. Returns a `ValidationError`
/// message if the new body fails `ValidationMode::Novel`. Returns other
/// `CliError` variants for missing blocks, JSON parse failures, or store errors.
pub async fn kb_edit(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
    block_id: &str,
    body_str: &str,
    json: bool,
) -> Result<()> {
    // Author identity gate: world owner must match the active creator.
    require_world_owner(pool, world_id, creator_id).await?;

    // Novel-mode store so update_key_block re-runs V1.40 P1 validation (§5.1.1).
    let store = SqliteKbStore::with_validation_mode(pool.clone(), ValidationMode::Novel);

    let mut block = store
        .get_key_block(block_id)
        .await
        .map_err(|e| map_kb_store_error("show", block_id, world_id, e))?;
    require_block_in_world(&block, world_id, block_id)?;

    let new_body: KeyBlockBody = serde_json::from_str(body_str).map_err(|e| {
        CliError::Other(format!(
            "Invalid --body JSON: {e}. \
             Expected a KeyBlockBody object: {{\"summary\":..., \"attributes\":..., \"tags\":...}}"
        ))
    })?;

    block.body = Some(new_body);
    block.updated_at = Some(chrono::Utc::now().to_rfc3339());

    store
        .update_key_block(block.clone())
        .await
        .map_err(|e| map_kb_store_error("update", block_id, world_id, e))?;

    if json {
        let value = serde_json::to_value(&block)?;
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("✓ Key block updated: {}", block.key_block_id);
        println!("  World:  {}", block.world_id);
        println!("  Name:   {}", block.canonical_name);
        println!("  Status: {}", block.status);
    }
    Ok(())
}

/// `creator world kb delete` — soft-delete with confirmation prompt.
///
/// # Errors
///
/// Returns `CliError` (`Api { status: 403, .. }` with code `WORLD_KB_FORBIDDEN`)
/// if the active creator does not own the world. Returns other `CliError`
/// variants for missing blocks or store errors.
pub async fn kb_delete(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
    block_id: &str,
    yes: bool,
) -> Result<()> {
    // Author identity gate: world owner must match the active creator.
    require_world_owner(pool, world_id, creator_id).await?;

    let store = SqliteKbStore::new(pool.clone());

    // Pre-check existence + world binding for a clean error before prompting.
    let block = store
        .get_key_block(block_id)
        .await
        .map_err(|e| map_kb_store_error("show", block_id, world_id, e))?;
    require_block_in_world(&block, world_id, block_id)?;

    if !yes && !confirm_delete(block_id, world_id) {
        println!("Delete cancelled.");
        return Ok(());
    }

    store
        .delete_key_block(block_id)
        .await
        .map_err(|e| map_kb_store_error("delete", block_id, world_id, e))?;

    println!("✓ Key block deleted: {block_id}");
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Verify the referenced `KeyBlock` actually belongs to the requested world.
fn require_block_in_world(block: &KeyBlock, world_id: &str, block_id: &str) -> Result<()> {
    if block.world_id != world_id {
        return Err(CliError::Other(format!(
            "Key block '{block_id}' does not belong to world '{world_id}' \
             (it belongs to '{}').",
            block.world_id
        )));
    }
    Ok(())
}

/// Author identity gate. Reads `narrative_worlds.owner_creator_id` and requires
/// it to match `creator_id`. Returns `403 WORLD_KB_FORBIDDEN` on mismatch.
///
/// Per entity-scope-model §1.2/§5.1, `KeyBlock`s are World-scoped and the
/// canonical ownership is the world's `owner_creator_id` (there is no direct
/// `works.creator_id` linkage on `kb_key_blocks`).
async fn require_world_owner(pool: &SqlitePool, world_id: &str, creator_id: &str) -> Result<()> {
    // SAFETY: SELECT against the known narrative_worlds table schema.
    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?")
            .bind(world_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| CliError::Other(format!("Failed to query world owner: {e}")))?
            .flatten();

    match owner {
        None => Err(CliError::Other(format!(
            "World '{world_id}' not found. \
             List worlds with: nexus42 creator world list"
        ))),
        Some(owner_id) if owner_id == creator_id => Ok(()),
        Some(owner_id) => Err(CliError::Api {
            status: 403,
            message: format!(
                "{WORLD_KB_FORBIDDEN_CODE}: active creator '{creator_id}' does not own \
                 world '{world_id}' (owner: '{owner_id}'). \
                 Cross-author World KB edits are not permitted."
            ),
        }),
    }
}

/// Confirm an interactive delete. Returns `false` on non-interactive terminals
/// (caller should require `--yes` in that case).
fn confirm_delete(block_id: &str, world_id: &str) -> bool {
    dialoguer::Confirm::new()
        .with_prompt(format!(
            "Delete key block '{block_id}' in world '{world_id}'?"
        ))
        .default(false)
        .interact()
        .unwrap_or_else(|_| {
            eprintln!("Non-interactive terminal: pass --yes to confirm delete.");
            false
        })
}

/// Map a `KbStoreError` to a user-facing `CliError`, surfacing validation
/// failures with a clear `ValidationError` prefix.
fn map_kb_store_error(verb: &str, block_id: &str, world_id: &str, e: KbStoreError) -> CliError {
    match e {
        KbStoreError::NotFound(_) => CliError::Other(format!(
            "Key block '{block_id}' not found in world '{world_id}'."
        )),
        KbStoreError::Validation(ve) => CliError::Other(format!("ValidationError: {ve}")),
        KbStoreError::ValidationLegacy(msg) => CliError::Other(format!("ValidationError: {msg}")),
        other => CliError::Other(format!(
            "Failed to {verb} key block '{block_id}' in world '{world_id}': {other}"
        )),
    }
}

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
