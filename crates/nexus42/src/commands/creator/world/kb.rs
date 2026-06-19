//! World KB key-block author surface — `creator world kb list/show/edit/delete`.
//!
//! V1.50 T-B P0. This is the canonical author CLI for inspecting and editing
//! World-scoped `KeyBlock` rows (per entity-scope-model.md §5.5), distinct from
//! the legacy ingest path `creator kb --scope world`.
//!
//! V1.50 T-B P1 adds the review-time promotion surface:
//! `creator world kb pending|adopt|reject` — list/confirm/dismiss candidates
//! extracted by the `novel-review-master` review-time hook
//! (`nexus_orchestration::quality_loop`).
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
use nexus_local_db::kb_extract_job::{
    get_promotion, list_pending_for_world, mark_confirmed_in_tx_with_cas, mark_rejected,
    KbExtractPromotion,
};
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

    /// List review-time KB candidates awaiting confirmation (V1.50 T-B P1).
    /// With `--missing-only`, list advisory finalize-time missing candidates
    /// from `Works/<work_ref>/Logs/kb/missing/` instead.
    Pending {
        /// World reference — the world ID (e.g. `wld_abc123`)
        world_ref: String,
        /// Maximum number of candidates to list
        #[arg(long, default_value_t = 100)]
        limit: i64,
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
        /// List advisory missing candidates detected at finalize time
        /// (scans `Works/<work_ref>/Logs/kb/missing/`)
        #[arg(long)]
        missing_only: bool,
    },

    /// Confirm a review-time KB candidate → promote to a `confirmed` `KeyBlock`
    Adopt {
        /// `kb_extract_jobs` job ID (e.g. `xj_...`)
        extract_job_id: String,
        /// Emit machine-readable JSON confirmation
        #[arg(long)]
        json: bool,
    },

    /// Dismiss a review-time KB candidate (archived to `Logs/kb/rejected/`)
    Reject {
        /// `kb_extract_jobs` job ID (e.g. `xj_...`)
        extract_job_id: String,
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
        WorldKbCommand::Pending {
            world_ref,
            limit,
            json,
            missing_only,
        } => {
            let ws_root = crate::config::find_workspace_root();
            kb_pending(
                &pool,
                &creator_id,
                &world_ref,
                Some(limit),
                json,
                missing_only,
                ws_root.as_deref(),
            )
            .await
        }
        WorldKbCommand::Adopt {
            extract_job_id,
            json,
        } => {
            let ws_root = crate::config::find_workspace_root();
            kb_adopt(
                &pool,
                &creator_id,
                &extract_job_id,
                ws_root.as_deref(),
                json,
            )
            .await
        }
        WorldKbCommand::Reject { extract_job_id } => {
            let ws_root = crate::config::find_workspace_root();
            kb_reject(&pool, &creator_id, &extract_job_id, ws_root.as_deref()).await
        }
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
        .map_err(|e| map_kb_store_error("load", block_id, world_id, e))?;
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
        .map_err(|e| map_kb_store_error("load", block_id, world_id, e))?;
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

// ── V1.50 T-B P1: review-time promotion surface ──────────────────────

/// `creator world kb pending` — list candidates awaiting confirmation.
///
/// With `missing_only = false` (default), lists `pending` `kb_extract_jobs` rows
/// (V1.50 behavior). With `missing_only = true`, scans advisory missing-KB logs
/// under `Works/<work_ref>/Logs/kb/missing/` for every Work bound to this World.
///
/// Gates on world ownership: a cross-author attempt returns `403` with code
/// `WORLD_KB_FORBIDDEN` (reuses the T-B P0 error code per acceptance §3).
///
/// # Errors
///
/// Returns `CliError` (`Api { status: 403, .. }`) on cross-author access, or
/// `CliError::Other` on store/serialization/log-scan failure.
pub async fn kb_pending(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
    limit: Option<i64>,
    json: bool,
    missing_only: bool,
    workspace_dir: Option<&std::path::Path>,
) -> Result<()> {
    // Author identity gate (same code path as edit/delete).
    require_world_owner(pool, world_id, creator_id).await?;

    if missing_only {
        return kb_pending_missing_only(pool, world_id, limit, json, workspace_dir).await;
    }

    let pending = list_pending_for_world(pool, world_id, limit)
        .await
        .map_err(|e| CliError::Other(format!("World KB pending list failed: {e}")))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&pending)?);
        return Ok(());
    }

    if pending.is_empty() {
        println!("No pending KB candidates in world {world_id}.");
        return Ok(());
    }

    println!("Pending KB candidates in world {world_id}:");
    println!(
        "{:<22} {:<15} {:<30} CHAPTER",
        "JOB_ID", "TYPE_GUESS", "NAME_GUESS"
    );
    for c in &pending {
        println!(
            "{:<22} {:<15} {:<30} {}",
            c.job_id,
            c.block_type_guess.as_deref().unwrap_or("?"),
            c.canonical_name_guess.as_deref().unwrap_or("?"),
            c.source_chapter_id
                .map_or_else(|| "-".to_string(), |n| n.to_string()),
        );
    }
    Ok(())
}

/// `creator world kb pending --missing-only` implementation.
///
/// Scans `Works/<work_ref>/Logs/kb/missing/*.md` for every Work bound to the
/// given World, parses YAML frontmatter, and prints advisory missing candidates.
async fn kb_pending_missing_only(
    pool: &SqlitePool,
    world_id: &str,
    limit: Option<i64>,
    json: bool,
    workspace_dir: Option<&std::path::Path>,
) -> Result<()> {
    let Some(ws_dir) = workspace_dir else {
        if json {
            println!("[]");
        } else {
            println!("No missing-KB logs found: no workspace directory is bound.");
        }
        return Ok(());
    };

    let mut entries = collect_missing_entries(pool, world_id, ws_dir).await?;

    // Stable ordering: chapter ascending, then canonical name.
    entries.sort_by(|a, b| {
        a.chapter
            .cmp(&b.chapter)
            .then_with(|| a.candidate.canonical_name.cmp(&b.candidate.canonical_name))
    });

    if let Some(lim) = limit {
        let lim = usize::try_from(lim.max(0)).unwrap_or(usize::MAX);
        entries.truncate(lim);
    }

    render_missing_entries(world_id, &entries, json)
}

/// Collect advisory missing-KB entries from log files.
async fn collect_missing_entries(
    pool: &SqlitePool,
    world_id: &str,
    ws_dir: &std::path::Path,
) -> Result<Vec<MissingKbEntry>> {
    let work_refs: Vec<String> =
        sqlx::query_as("SELECT COALESCE(work_ref, story_ref) FROM works WHERE world_id = ?")
            .bind(world_id)
            .fetch_all(pool)
            .await
            .map_err(|e| {
                CliError::Other(format!("Failed to list works for world {world_id}: {e}"))
            })?
            .into_iter()
            .filter_map(|(opt,): (Option<String>,)| opt)
            .collect();

    let mut entries: Vec<MissingKbEntry> = Vec::new();
    for work_ref in work_refs {
        let log_dir = ws_dir
            .join("Works")
            .join(&work_ref)
            .join("Logs")
            .join("kb")
            .join("missing");
        if !log_dir.is_dir() {
            continue;
        }
        let mut dir_entries: Vec<std::fs::DirEntry> = std::fs::read_dir(&log_dir)
            .map_err(|e| CliError::Other(format!("Cannot read missing-KB log dir: {e}")))?
            .filter_map(std::result::Result::ok)
            .collect();
        dir_entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in dir_entries {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "missing-kb: skip unreadable log");
                    continue;
                }
            };
            let (frontmatter, _body) = split_frontmatter(&text)
                .map_err(|e| CliError::Other(format!("Parse error in {}: {e}", path.display())))?;
            let log: MissingKbLogFrontmatter = serde_yaml::from_str(frontmatter)
                .map_err(|e| CliError::Other(format!("YAML error in {}: {e}", path.display())))?;
            if log.world_id != world_id {
                continue;
            }
            for candidate in log.candidates {
                entries.push(MissingKbEntry {
                    chapter: log.chapter,
                    world_id: log.world_id.clone(),
                    generated_at: log.generated_at.clone(),
                    candidate,
                });
            }
        }
    }
    Ok(entries)
}

/// Render collected missing-KB entries as text or JSON.
fn render_missing_entries(world_id: &str, entries: &[MissingKbEntry], json: bool) -> Result<()> {
    if json {
        let items: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "chapter": e.chapter,
                    "world_id": e.world_id,
                    "canonical_name": e.candidate.canonical_name,
                    "block_type": e.candidate.block_type,
                    "source_quote": e.candidate.source_quote,
                    "confidence": e.candidate.confidence,
                    "generated_at": e.generated_at,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if entries.is_empty() {
        println!("No missing KB candidates in world {world_id}.");
        return Ok(());
    }

    println!("Missing KB candidates in world {world_id}:");
    println!("{:<8} {:<15} {:<30} SOURCE", "CHAPTER", "TYPE", "NAME");
    for e in entries {
        let quote = e
            .candidate
            .source_quote
            .as_deref()
            .map_or_else(|| "-".to_string(), truncate_quote);
        println!(
            "[MISSING] {:<8} {:<15} {:<30} {}",
            e.chapter, e.candidate.block_type, e.candidate.canonical_name, quote
        );
    }
    Ok(())
}

/// Split a Markdown file into its YAML frontmatter and body.
///
/// Expects the file to start with `---` and contain a second `---` ending the
/// frontmatter block. Returns the frontmatter text (without delimiters) and the
/// body text (after the closing delimiter).
fn split_frontmatter(text: &str) -> std::result::Result<(&str, &str), String> {
    let rest = text
        .strip_prefix("---")
        .ok_or_else(|| "missing opening frontmatter delimiter".to_string())?;
    let Some(split) = rest.find("\n---") else {
        return Err("missing closing frontmatter delimiter".to_string());
    };
    let fm = &rest[..split];
    let body_start = split + "\n---".len();
    let body = &rest[body_start..];
    Ok((fm.trim(), body.trim_start()))
}

/// Truncate a source quote for terminal output.
fn truncate_quote(q: &str) -> String {
    let q = q.trim();
    if q.chars().count() > 50 {
        let head: String = q.chars().take(47).collect();
        format!("{head}...")
    } else {
        q.to_string()
    }
}

/// Parsed frontmatter of a missing-KB log file.
#[derive(Debug, serde::Deserialize)]
struct MissingKbLogFrontmatter {
    generated_at: String,
    world_id: String,
    chapter: i32,
    candidates: Vec<MissingKbLogCandidate>,
}

/// Parsed candidate inside a missing-KB log frontmatter.
#[derive(Debug, serde::Deserialize)]
struct MissingKbLogCandidate {
    canonical_name: String,
    block_type: String,
    source_quote: Option<String>,
    confidence: Option<f64>,
}

/// Flattened missing-KB entry used for sorting/output.
struct MissingKbEntry {
    chapter: i32,
    world_id: String,
    generated_at: String,
    candidate: MissingKbLogCandidate,
}

/// `creator world kb adopt` — confirm a candidate into a `confirmed` `KeyBlock`.
///
/// Steps (entity-scope-model.md §5.5.3 promotion gate):
/// 1. Load the promotion row; require it is in `pending` state.
/// 2. Author identity gate: the active creator must own the candidate's world.
/// 3. Parse `proposed_payload` into a `KeyBlockBody`; parse `block_type_guess`
///    into a wire `BlockType`.
/// 4. Build a `KeyBlock` with `status="confirmed"`.
/// 5. **Atomic promotion (R-V150KBED-03)**: wrap `insert_key_block` +
///    `mark_confirmed` in a single `SQLite` transaction. If the validation,
///    insert, or promotion flip fails (or the flip returns `Ok(false)` because
///    a concurrent writer raced us), the transaction rolls back and **no orphan
///    `KeyBlock` is persisted**. The candidate row is left in its pre-adopt
///    state.
/// 6. Validation uses `SqliteKbStore::with_validation_mode(Novel)` so V1.40 P1
///    validation re-runs (entity-scope-model §5.5.5).
///
/// # Errors
///
/// Returns `CliError` (`Api { status: 403, .. }`) on cross-author access.
/// Returns `CliError::Other` on missing/non-pending rows, validation failure,
/// transaction begin/commit failure, or store errors.
// V1.51 T-B P0 added the advisory file-lock block (commit 6dccee36, +18 lines);
// V1.51 T-B P1 added the CAS version check (+5 lines). Function has 8
// sequential concerns (candidate load → author gate → file lock →
// candidate gate → read+validate → conflict check → adopt transaction →
// audit log); splitting into helpers would fragment the linear adopt flow
// without reducing complexity.
#[allow(clippy::too_many_lines)]
pub async fn kb_adopt(
    pool: &SqlitePool,
    creator_id: &str,
    extract_job_id: &str,
    workspace_dir: Option<&std::path::Path>,
    json: bool,
) -> Result<()> {
    let candidate = load_pending_candidate(pool, extract_job_id).await?;
    let candidate_version = candidate.version; // V1.51 T-B P1: CAS preimage version
    let world_id = candidate.world_id.as_str();

    // Author identity gate.
    require_world_owner(pool, world_id, creator_id).await?;

    // V1.51 T-B P0: acquire advisory file lock before the DB transaction (W-001).
    // Resolve work_ref from the candidate's work_id, then acquire the lock.
    let _file_lock =
        if let (Some(ws_dir), Some(ref wid)) = (workspace_dir, candidate.work_id.as_deref()) {
            let work_ref: Option<String> =
                sqlx::query_scalar("SELECT story_ref FROM works WHERE work_id = ?")
                    .bind(wid)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| CliError::Other(format!("Failed to resolve work_ref: {e}")))?
                    .flatten();
            if let Some(ref wref) = work_ref {
                let work_dir = ws_dir.join("Works").join(wref);
                if work_dir.exists() {
                    match nexus_local_db::file_lock::try_acquire(&work_dir, "cli:kb-adopt") {
                        Ok(guard) => Some(guard),
                        Err(nexus_local_db::file_lock::FileLockError::Locked(locked)) => {
                            return Err(CliError::Locked {
                                holder_pid: locked.holder_pid,
                                holder_name: locked.holder_name,
                                stale: locked.stale,
                            });
                        }
                        Err(nexus_local_db::file_lock::FileLockError::Io(e)) => {
                            return Err(CliError::LockIo(e));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

    // Parse proposed body.
    let body: KeyBlockBody =
        serde_json::from_str(candidate.proposed_payload.as_deref().unwrap_or("{}"))
            .map_err(|e| CliError::Other(format!("Invalid proposed_payload JSON: {e}")))?;

    // Parse block_type guess → wire BlockType.
    let block_type_str = candidate.block_type_guess.as_deref().unwrap_or("character");
    let block_type = parse_block_type_cli(block_type_str)?;

    let canonical_name = candidate
        .canonical_name_guess
        .as_deref()
        .ok_or_else(|| CliError::Other("Candidate has no canonical_name_guess".to_string()))?
        .to_string();

    // Novel-mode store so insert re-runs V1.40 P1 validation (§5.1.1).
    let store = SqliteKbStore::with_validation_mode(pool.clone(), ValidationMode::Novel);

    let mut kb = KeyBlock::new(world_id, block_type, &canonical_name);
    kb.body = Some(body);
    // §5.5.1: adopt transitions to `confirmed` (terminal KeyBlock status).
    kb.status = "confirmed".to_string();
    kb.created_at = chrono::Utc::now().to_rfc3339();

    // R-V150KBED-03: atomic promotion. The KeyBlock insert and the promotion
    // row flip share a single transaction; any failure (validation, insert,
    // flip error, or `Ok(false)` race) rolls the whole thing back so no orphan
    // KeyBlock is persisted.
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| CliError::Other(format!("Failed to begin adopt transaction: {e}")))?;

    let insert_result = store
        .insert_key_block_in_tx(&mut tx, kb)
        .await
        .map_err(|e| map_kb_store_error("adopt", extract_job_id, world_id, e))?;
    // On `Err` above, `tx` is dropped → rolled back automatically by sqlx.

    // V1.51 T-B P1: CAS-aware flip — guards against stale preimage
    // (another writer raced between load_pending_candidate and this flip).
    // On version mismatch → E_VERSION (exit 76, retry).
    let flipped = mark_confirmed_in_tx_with_cas(&mut tx, extract_job_id, candidate_version)
        .await
        .map_err(|e| {
            if let nexus_local_db::LocalDbError::VersionMismatch { actual, .. } = &e {
                CliError::VersionConflict {
                    table: "kb_extract_jobs".to_string(),
                    row_id: extract_job_id.to_string(),
                    expected_version: candidate_version,
                    actual_version: *actual,
                }
            } else {
                CliError::Other(format!("Failed to mark candidate confirmed: {e}"))
            }
        })?;
    // On `Err` above, `tx` is dropped → rolled back automatically by sqlx.

    if !flipped {
        // Race: the row was confirmed/rejected between `load_pending_candidate`
        // and this flip. Explicit rollback so the orphan KeyBlock insert is
        // undone before we surface the error. Best-effort: a rollback failure
        // is logged but the row was never committed so no orphan persists.
        if let Err(e) = tx.rollback().await {
            tracing::error!(
                extract_job_id,
                error = %e,
                "kb-adopt: transaction rollback failed after mark_confirmed race"
            );
        }
        return Err(CliError::Other(format!(
            "Candidate '{extract_job_id}' was no longer pending (already confirmed/rejected). \
             The transaction was rolled back; no orphan row created."
        )));
    }

    tx.commit()
        .await
        .map_err(|e| CliError::Other(format!("Failed to commit adopt transaction: {e}")))?;

    // V1.51 T-A P0: surface LLM extraction metadata (cli-spec §6.2G). Read
    // the dedicated columns first; fall back to the proposed_payload JSON keys
    // for V1.50 rows where the columns are NULL (llm-extract.md §3.2).
    let (confidence, source_quote) = extract_llm_metadata(&candidate);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "extract_job_id": extract_job_id,
                "key_block_id": insert_result.key_block_id,
                "world_id": insert_result.world_id,
                "status": "confirmed",
                "llm_confidence": confidence,
                "llm_source_quote": source_quote,
            }))?
        );
    } else {
        println!("✓ KB candidate adopted: {extract_job_id}");
        println!("  Key block:   {}", insert_result.key_block_id);
        println!("  World:       {}", insert_result.world_id);
        println!("  Status:      confirmed");
        // Confidence is shown as 2-decimal or '-' for heuristic rows; source
        // quote is truncated for terminal width (full text in --json).
        let conf_display = confidence.map_or_else(|| "-".to_string(), |c| format!("{c:.2}"));
        let quote_display = source_quote.as_deref().map_or_else(
            || "-".to_string(),
            |q| {
                let q = q.trim();
                if q.is_empty() {
                    "-".to_string()
                } else if q.chars().count() > 60 {
                    // char-count truncation keeps multi-byte text correct.
                    let head: String = q.chars().take(57).collect();
                    format!("{head}...")
                } else {
                    q.to_string()
                }
            },
        );
        println!("  Confidence:  {conf_display}");
        println!("  Source:      {quote_display}");
    }
    Ok(())
}

/// `creator world kb reject` — dismiss a candidate (archived to
/// `Logs/kb/rejected/<YYYY-MM-DD>-<extract_job_id>.md`).
///
/// # Errors
///
/// Returns `CliError` (`Api { status: 403, .. }`) on cross-author access.
/// Returns `CliError::Other` on missing/non-pending rows, store errors, or
/// (R-V150KBED-05) when an audit log is required but the candidate's `work_id`
/// cannot be resolved to a human-readable `work_ref` (`works.story_ref`).
pub async fn kb_reject(
    pool: &SqlitePool,
    creator_id: &str,
    extract_job_id: &str,
    workspace_dir: Option<&std::path::Path>,
) -> Result<()> {
    let candidate = load_pending_candidate(pool, extract_job_id).await?;
    let world_id = candidate.world_id.as_str();

    // Author identity gate.
    require_world_owner(pool, world_id, creator_id).await?;

    // R-V150KBED-05: resolve the human-readable work_ref (works.story_ref) for
    // the reject audit-log path BEFORE the DB flip. A missing work_ref fails
    // cleanly here — no rejected row, no orphan audit log under the wrong path.
    // The prior behavior wrote under `Works/<work_id>/...` using the opaque DB
    // id, which violated the home-layout `Works/<work_ref>/` convention.
    let work_ref =
        resolve_work_ref_for_log(pool, candidate.work_id.as_deref(), workspace_dir).await?;

    let flipped = mark_rejected(pool, extract_job_id)
        .await
        .map_err(|e| CliError::Other(format!("Failed to mark candidate rejected: {e}")))?;
    if !flipped {
        return Err(CliError::Other(format!(
            "Candidate '{extract_job_id}' was no longer pending (already confirmed/rejected)."
        )));
    }

    // Best-effort audit log (entity-scope-model §5.5.4). Non-fatal: a missing
    // workspace dir (hermetic tests) or write failure does not undo the reject.
    if let Err(e) = write_rejected_log(
        workspace_dir,
        extract_job_id,
        &candidate,
        work_ref.as_deref(),
    ) {
        tracing::warn!(
            extract_job_id,
            error = %e,
            "kb-reject: failed to write audit log (non-fatal)"
        );
    }

    println!("✓ KB candidate rejected: {extract_job_id}");
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

// ── V1.50 T-B P1 helpers ─────────────────────────────────────────────

/// Load a promotion candidate by ID and require it is in `pending` state.
///
/// Surfaces a clean error if the row is missing, not a promotion candidate,
/// or already confirmed/rejected.
async fn load_pending_candidate(
    pool: &SqlitePool,
    extract_job_id: &str,
) -> Result<KbExtractPromotion> {
    let row = get_promotion(pool, extract_job_id)
        .await
        .map_err(|e| CliError::Other(format!("Failed to load candidate: {e}")))?
        .ok_or_else(|| {
            CliError::Other(format!(
                "KB extract job '{extract_job_id}' not found. \
                 List pending candidates with: nexus42 creator world kb pending <world_ref>"
            ))
        })?;
    if row.promotion_status != "pending" {
        return Err(CliError::Other(format!(
            "Candidate '{extract_job_id}' is not pending (status: {}). \
             Only pending candidates can be adopted or rejected.",
            row.promotion_status
        )));
    }
    Ok(row)
}

/// Parse a `snake_case` `block_type` string into a wire `BlockType`.
///
/// Accepts the wire format (e.g. `"character"`). Returns a clear error on
/// unknown values so the author can correct the `block_type_guess`.
fn parse_block_type_cli(s: &str) -> Result<nexus_contracts::BlockType> {
    serde_json::from_value::<nexus_contracts::BlockType>(serde_json::Value::String(
        s.to_string(),
    ))
    .map_err(|_| {
        CliError::Other(format!(
            "Unknown block_type guess '{s}'. \
             Valid values: character, ability, scene, organization, item, conflict, info_point, event."
        ))
    })
}

/// Resolve the LLM extraction metadata for an adopt display (V1.51 T-A P0,
/// cli-spec §6.2G).
///
/// Reads the dedicated `llm_confidence` / `llm_source_quote` columns first;
/// when they are `NULL` (V1.50 heuristic rows produced before the V1.51
/// migration), falls back to parsing the same keys from `proposed_payload`
/// JSON so adopt still surfaces them if the payload carries the LLM keys
/// (llm-extract.md §3.2). Returns `(None, None)` for pure heuristic rows.
fn extract_llm_metadata(candidate: &KbExtractPromotion) -> (Option<f64>, Option<String>) {
    let confidence = candidate.llm_confidence.or_else(|| {
        candidate
            .proposed_payload
            .as_deref()
            .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok())
            .and_then(|v| v.get("confidence").and_then(serde_json::Value::as_f64))
    });
    let source_quote = candidate.llm_source_quote.clone().or_else(|| {
        candidate
            .proposed_payload
            .as_deref()
            .and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok())
            .and_then(|v| {
                v.get("source_quote")
                    .and_then(|q| q.as_str())
                    .map(std::string::ToString::to_string)
            })
    });
    (confidence, source_quote)
}

/// Write the rejected-candidate audit log (entity-scope-model §5.5.4).
///
/// Path: `<workspace_dir>/Works/<work_ref>/Logs/kb/rejected/<YYYY-MM-DD>-<extract_job_id>.md`.
///
/// `work_ref` is the human-readable slug resolved upstream as `works.story_ref`
/// by [`resolve_work_ref_for_log`] (R-V150KBED-05), matching the home-layout
/// `Works/<work_ref>/` convention. When `workspace_dir` is `None` the function
/// is a no-op (hermetic test path).
///
/// Best-effort: returns an error that the caller logs at `warn!` but does not
/// propagate to the user (the DB row is already flipped to `rejected`).
fn write_rejected_log(
    workspace_dir: Option<&std::path::Path>,
    extract_job_id: &str,
    candidate: &KbExtractPromotion,
    work_ref: Option<&str>,
) -> std::result::Result<(), String> {
    let Some(ws_dir) = workspace_dir else {
        // No workspace bound (hermetic test) — skip log writing.
        return Ok(());
    };
    // R-V150KBED-05: work_ref is resolved upstream from works.story_ref; fall
    // back only if the caller passed None despite having a workspace dir
    // (defensive — kb_reject resolves before calling, so this is unreachable
    // in the CLI path but keeps the helper safe for direct callers).
    let work_ref = work_ref.unwrap_or("unknown-work");

    let date = chrono::Utc::now().format("%Y-%m-%d");
    let log_dir = ws_dir
        .join("Works")
        .join(work_ref)
        .join("Logs")
        .join("kb")
        .join("rejected");
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("create_dir_all: {e}"))?;
    let log_path = log_dir.join(format!("{date}-{extract_job_id}.md"));

    let body = format!(
        "# Rejected KB candidate\n\
         \n\
         - **extract_job_id**: {job_id}\n\
         - **world_id**: {world_id}\n\
         - **work_id**: {work_id}\n\
         - **work_ref**: {work_ref}\n\
         - **canonical_name_guess**: {name}\n\
         - **block_type_guess**: {btype}\n\
         - **source_chapter_id**: {chapter}\n\
         - **rejected_at**: {ts}\n",
        job_id = extract_job_id,
        world_id = candidate.world_id,
        work_id = candidate.work_id.as_deref().unwrap_or("-"),
        work_ref = work_ref,
        name = candidate.canonical_name_guess.as_deref().unwrap_or("-"),
        btype = candidate.block_type_guess.as_deref().unwrap_or("-"),
        chapter = candidate
            .source_chapter_id
            .map_or_else(|| "-".to_string(), |n| n.to_string()),
        ts = chrono::Utc::now().to_rfc3339(),
    );
    std::fs::write(&log_path, body).map_err(|e| format!("write {}: {e}", log_path.display()))?;
    Ok(())
}

/// Resolve the human-readable `work_ref` (`works.story_ref`) for the reject
/// audit-log path (entity-scope-model §5.5.4; home-layout `Works/<work_ref>/`
/// convention). R-V150KBED-05.
///
/// Returns `Ok(None)` when no audit log is needed (no workspace dir bound —
/// e.g. hermetic tests with `workspace_dir=None`). Returns `Err` if a log IS
/// needed but the candidate has no `work_id`, the `works` row is absent, or
/// `story_ref` is `NULL`. Failing before the DB flip keeps the reject
/// side-effect-free when the audit trail cannot be written under the correct
/// path.
async fn resolve_work_ref_for_log(
    pool: &SqlitePool,
    work_id: Option<&str>,
    workspace_dir: Option<&std::path::Path>,
) -> Result<Option<String>> {
    if workspace_dir.is_none() {
        // No workspace bound (hermetic test) — no log to write, no ref needed.
        return Ok(None);
    }
    let Some(wid) = work_id else {
        return Err(CliError::Other(
            "Cannot write reject audit log: candidate has no work_id.".to_string(),
        ));
    };
    // SAFETY: SELECT against the known works table schema (story_ref is
    // nullable TEXT, so fetch_optional returns Option<Option<String>>).
    let row: Option<Option<String>> =
        sqlx::query_scalar("SELECT story_ref FROM works WHERE work_id = ?")
            .bind(wid)
            .fetch_optional(pool)
            .await
            .map_err(|e| CliError::Other(format!("Failed to query work_ref: {e}")))?;
    match row {
        None => Err(CliError::Other(format!(
            "Cannot write reject audit log: work_id '{wid}' does not exist in the works table."
        ))),
        Some(None) => Err(CliError::Other(format!(
            "Cannot write reject audit log: work '{wid}' has no story_ref (work_ref). \
             Run `nexus42 creator bootstrap` or set story_ref before rejecting a candidate."
        ))),
        Some(Some(story_ref)) => Ok(Some(story_ref)),
    }
}
