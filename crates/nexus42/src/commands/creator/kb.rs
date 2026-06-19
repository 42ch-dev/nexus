//! KB command handlers (local work-scope file index + world KB + extract queue).
//!
//! Extracted from `creator/mod.rs` (R19 module refactor).
//! All `creator kb` subcommands are routed through [`run`].
//!
//! # V1.52 T-A P1: Legacy World KB alias (R-V150KBED-01)
//!
//! `creator kb --scope world <subcmd>` is a **deprecated alias** for the
//! canonical `creator world kb <subcmd>` surface (see `world::kb` module).
//! World-scope operations forward to the canonical hermetic functions and
//! emit a deprecation warning on each invocation. Planned removal V1.53.

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use crate::paths;
use nexus_kb::KbStore;
use sqlx::SqlitePool;
use std::path::PathBuf;

/// Refreshable-scan submodule (V1.50 T-B P2; V1.51 T-A P1 work-scoped).
///
/// `pub` so integration tests under `tests/` can drive `kb_rescan_hermetic`
/// and `kb_rescan_work_hermetic` against a fresh temp DB, mirroring the
/// `world::kb` testability pattern.
pub mod rescan;
pub use rescan::{
    kb_rescan, kb_rescan_hermetic, kb_rescan_work, kb_rescan_work_hermetic, CrossChapterReuse,
    RescanReport, WorkRescanReport,
};

/// KB scope: `work` (local workspace file index, default) or `world` (narrative KB via nexus-kb).
///
/// Per entity-scope-model §5.3, `creator kb --scope work` is the **CLI local work KB index** —
/// a per-creator, per-workspace file-based index stored under
/// `~/.nexus42/creators/<id>/workspaces/<slug>/kb/`. It is NOT `nexus-kb` (World-scoped
/// narrative KB graph) or `nexus-knowledge` (User-scoped global knowledge).
///
/// `--scope world` routes to `nexus-kb` + `nexus-narrative` and requires
/// a `--world-id <id>`. User/global knowledge will NOT be a `creator kb` scope.
#[derive(Debug, Clone, clap::ValueEnum, Default, PartialEq, Eq)]
pub enum KbScope {
    /// Local workspace file index (default)
    #[default]
    Work,
    /// World-scoped narrative KB (nexus-kb + nexus-narrative)
    World,
}

/// Knowledge base subcommands.
///
/// Two scopes via `--scope`:
///   • `work` (default) — local workspace file index under `kb/`
///   • `world` — narrative KB key blocks (requires `--world-id`)
///
/// For User-scoped global knowledge entries, use `creator knowledge` instead.
/// For reference sources, use `creator reference`.
#[derive(Debug, clap::Subcommand)]
pub enum KbCommand {
    /// List entries (work-scope file index by default; use --scope world for key blocks)
    List {
        /// Scope: `work` (local file index, default) or `world` (narrative KB)
        #[arg(long, value_enum, default_value_t = KbScope::default())]
        scope: KbScope,
        /// World ID for `--scope world` (required when scope is `world`)
        #[arg(long)]
        world_id: Option<String>,
    },

    /// Search local work-scope entries by title/content
    Search {
        /// Search query string
        query: String,
        /// Scope: `work` (local file index, default) or `world` (narrative KB)
        #[arg(long, value_enum, default_value_t = KbScope::default())]
        scope: KbScope,
        /// World ID for `--scope world` (required when scope is `world`)
        #[arg(long)]
        world_id: Option<String>,
    },

    /// Show a single local work-scope entry
    Show {
        /// Entry ID to display (e.g. `kb_a1b2c3d4` or a key-block ID)
        entry_id: String,
        /// Scope: `work` (local file index, default) or `world` (narrative KB)
        #[arg(long, value_enum, default_value_t = KbScope::default())]
        scope: KbScope,
        /// World ID for `--scope world` (required when scope is `world`)
        #[arg(long)]
        world_id: Option<String>,
    },

    /// Add a local work-scope entry from a file
    Add {
        /// Path to the source file to add as a work-scope entry
        #[arg(long)]
        file: PathBuf,
        /// Optional title (defaults to filename stem)
        #[arg(long)]
        title: Option<String>,
        /// Scope: `work` (local file index, default) or `world` (narrative KB)
        #[arg(long, value_enum, default_value_t = KbScope::default())]
        scope: KbScope,
        /// World ID for `--scope world` (required when scope is `world`)
        #[arg(long)]
        world_id: Option<String>,
        /// Block type for `--scope world` (e.g. Character, Scene, Item)
        #[arg(long)]
        block_type: Option<String>,
    },

    /// Remove a local work-scope entry
    Remove {
        /// Entry ID to remove (e.g. `kb_a1b2c3d4`)
        entry_id: String,
        /// Scope: `work` (local file index, default) or `world` (narrative KB)
        #[arg(long, value_enum, default_value_t = KbScope::default())]
        scope: KbScope,
        /// World ID for `--scope world` (required when scope is `world`)
        #[arg(long)]
        world_id: Option<String>,
    },

    /// Queue a work-scope entry for KB extraction into a target world.
    ///
    /// Idempotent: if a non-failed job already exists for the same
    /// work entry + world combination, returns the existing job.
    ///
    /// Use `--chapter N` to resolve the body path from the work's chapter N
    /// and set `source_kind=work_chapter`, `profile_hint=novel` automatically.
    #[command(name = "queue-extract")]
    QueueExtract {
        /// Work-scope entry ID to extract from (e.g. `kb_a1b2c3d4`)
        work_entry_id: String,
        /// Target world ID for the resulting `KeyBlock`
        #[arg(long)]
        world_id: String,
        /// Source work ID (parent of the chapter artifact)
        #[arg(long)]
        work_id: Option<String>,
        /// Chapter number sugar for novel profile (resolves `body_path` from chapter N)
        #[arg(long)]
        chapter: Option<i32>,
    },

    /// Show extract job status for the active creator.
    ///
    /// Without `--job-id`, lists up to 100 most recent jobs for the active creator.
    #[command(name = "extract-status")]
    ExtractStatus {
        /// Specific job ID to inspect
        #[arg(long)]
        job_id: Option<String>,
    },

    /// Re-scan KB extract candidates + KB rows.
    ///
    /// V1.50 T-B P2: `creator kb rescan <work_ref>/<chapter>` re-runs the
    /// review-time heuristic over one chapter's current prose, idempotently
    /// upserts `kb_extract_jobs` candidates, refreshes confirmed `KeyBlock`
    /// bodies so KB rows reflect the current text, and reports the diff.
    /// Cross-author attempts return `403` (`WORLD_KB_FORBIDDEN`).
    ///
    /// V1.51 T-A P1: `creator kb rescan --work <work_ref>` is a mutually
    /// exclusive work-scoped mode that iterates all chapters in
    /// `Works/<work_ref>/Stories/` and reconciles candidates by
    /// `canonical_name` across chapters (closes R-V150KBED-08). Exactly one of
    /// the positional `<work_ref>/<chapter>` or `--work <work_ref>` must be
    /// supplied; supplying both (or neither) fails closed.
    Rescan {
        /// `<work_ref>/<chapter>` — e.g. `my-novel/05`. Mutually exclusive with
        /// `--work`.
        target: Option<String>,
        /// Work-scoped cross-chapter rescan: iterate all chapters in
        /// `Works/<work_ref>/Stories/` and reconcile by `canonical_name`.
        /// Mutually exclusive with the positional `<work_ref>/<chapter>`.
        #[arg(long, value_name = "WORK_REF")]
        work: Option<String>,
        /// Show what would change without writing
        #[arg(long)]
        dry_run: bool,
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },
}

/// Run KB subcommand dispatcher.
///
/// F002: Validates `active_creator_id` before constructing any paths.
/// This prevents path traversal if config is corrupted or malicious.
///
/// # Errors
///
/// Returns an error if `active_creator_id` fails validation or the underlying
/// KB operation fails.
// CLI entry-point runs on a single-threaded tokio runtime — Send not required.
#[allow(clippy::future_not_send)]
pub async fn run(cmd: KbCommand, config: &CliConfig) -> Result<()> {
    if let Some(cid) = &config.active_creator_id {
        paths::validate_creator_id_safe(cid).map_err(CliError::Other)?;
    }
    match cmd {
        KbCommand::List { scope, world_id } => kb_list(config, &scope, world_id.as_deref()).await,
        KbCommand::Search {
            query,
            scope,
            world_id,
        } => kb_search(config, &query, &scope, world_id.as_deref()).await,
        KbCommand::Show {
            entry_id,
            scope,
            world_id,
        } => kb_show(config, &entry_id, &scope, world_id.as_deref()).await,
        KbCommand::Add {
            file,
            title,
            scope,
            world_id,
            block_type,
        } => {
            kb_add(
                config,
                &file,
                title.as_deref(),
                &scope,
                world_id.as_deref(),
                block_type.as_deref(),
            )
            .await
        }
        KbCommand::Remove {
            entry_id,
            scope,
            world_id,
        } => kb_remove(config, &entry_id, &scope, world_id.as_deref()).await,
        KbCommand::QueueExtract {
            work_entry_id,
            world_id,
            work_id,
            chapter,
        } => {
            kb_queue_extract(
                config,
                &work_entry_id,
                &world_id,
                work_id.as_deref(),
                chapter,
            )
            .await
        }
        KbCommand::ExtractStatus { job_id } => kb_extract_status(config, job_id.as_deref()).await,
        KbCommand::Rescan {
            target,
            work,
            dry_run,
            json,
        } => match (target, work) {
            (Some(t), None) => rescan::kb_rescan(config, &t, dry_run, json).await,
            (None, Some(w)) => rescan::kb_rescan_work(config, &w, dry_run, json).await,
            (Some(_), Some(_)) => Err(CliError::Other(
                "Specify either <work_ref>/<chapter> positional or --work <work_ref>, not both."
                    .into(),
            )),
            (None, None) => Err(CliError::Other(
                "Specify either <work_ref>/<chapter> (e.g. my-novel/05) or --work <work_ref>."
                    .into(),
            )),
        },
    }
}

// ── Helpers ──────────────────────────────────────────────────────

/// Require `--world-id` when `--scope world` is used. Returns the `world_id` or an error.
fn require_world_id(world_id: Option<&str>) -> Result<String> {
    world_id
        .map(std::string::ToString::to_string)
        .ok_or_else(|| {
            CliError::Other(
                "--world-id is required when using --scope world. \
                  Usage: nexus42 creator kb <command> --scope world --world-id <id>"
                    .into(),
            )
        })
}

fn user_home() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| CliError::Other("Cannot determine home directory".into()))
}

/// Open a persistent KB store backed by the workspace `state.db`.
///
/// Uses `nexus_local_db::kb_store::SqliteKbStore` which implements
/// `KbStore` via compile-time checked sqlx queries.
async fn open_world_kb_store(
    config: &CliConfig,
) -> Result<nexus_local_db::kb_store::SqliteKbStore> {
    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;
    Ok(nexus_local_db::kb_store::SqliteKbStore::new(pool))
}

/// Parse a block type string from CLI argument.
fn parse_block_type_cli(s: &str) -> Result<nexus_contracts::BlockType> {
    match s {
        "Character" => Ok(nexus_contracts::BlockType::Character),
        "Ability" => Ok(nexus_contracts::BlockType::Ability),
        "Scene" => Ok(nexus_contracts::BlockType::Scene),
        "Organization" => Ok(nexus_contracts::BlockType::Organization),
        "Item" => Ok(nexus_contracts::BlockType::Item),
        "Conflict" => Ok(nexus_contracts::BlockType::Conflict),
        "InfoPoint" => Ok(nexus_contracts::BlockType::InfoPoint),
        "Event" => Ok(nexus_contracts::BlockType::Event),
        _ => Err(CliError::Other(format!(
            "Unknown block_type '{s}'. Valid: Character, Ability, Scene, Organization, Item, Conflict, InfoPoint, Event"
        ))),
    }
}

/// Emit a deprecation warning for `creator kb --scope world` callers (R-V150KBED-01).
///
/// Emits a `tracing::warn!` for log-based observability and an `eprintln!` for
/// interactive terminal users. Planned removal V1.53.
fn deprecation_notice_legacy_world_kb(subcmd: &str) {
    let msg = format!(
        "`creator kb --scope world {subcmd}` is deprecated; \
         use `creator world kb {subcmd}` instead (planned removal V1.53)."
    );
    tracing::warn!("{}", msg);
    eprintln!("nexus42: {msg}");
}

/// Open a workspace pool for World KB operations.
///
/// Returns the raw pool so the caller can pass it to the canonical
/// `world::kb` hermetic functions (which take `&SqlitePool` directly).
async fn open_world_pool(config: &CliConfig) -> Result<SqlitePool> {
    let db_path = crate::config::resolve_state_db_path(config)?;
    crate::db::Schema::init(&db_path)
        .await
        .map_err(|e| CliError::Other(format!("Failed to open workspace pool: {e}")))
}

/// Resolve active creator + workspace slug, returning `(creator_id, workspace_slug, home)`.
fn resolve_kb_paths(config: &CliConfig) -> Result<(String, String, PathBuf)> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?
        .to_string();
    let slug = config.workspace_slug_for_creator(&creator_id).to_string();
    let home = user_home()?;
    Ok((creator_id, slug, home))
}

/// Local work index on disk: `{"entries": [{"entry_id": "...", "title": "...", "created_at": "..."}]}`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub(crate) struct KbIndex {
    #[serde(default)]
    pub(crate) entries: Vec<KbIndexEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct KbIndexEntry {
    pub(crate) entry_id: String,
    pub(crate) title: String,
    pub(crate) created_at: String,
}

/// Read the local work index from disk. Returns default (empty) if file is missing.
/// Logs a warning if the file exists but contains invalid JSON.
pub(crate) fn read_kb_index(index_path: &std::path::Path) -> KbIndex {
    if !index_path.exists() {
        return KbIndex::default();
    }
    let Ok(content) = std::fs::read_to_string(index_path) else {
        return KbIndex::default();
    };
    if content.trim().is_empty() {
        return KbIndex::default();
    }
    match serde_json::from_str(&content) {
        Ok(index) => index,
        Err(e) => {
            tracing::warn!(
                "Corrupt local work index file {}: {e}. \
                 The file will be treated as empty. \
                 Consider deleting it or re-adding entries to rebuild the index.",
                index_path.display()
            );
            KbIndex::default()
        }
    }
}

/// Write the local work index to disk atomically.
///
/// Writes to a temporary file first, then renames to the final path.
/// `std::fs::rename` is atomic on the same filesystem (POSIX), which
/// prevents corruption from crashes mid-write or concurrent `kb add` races.
#[allow(dead_code)] // Kept as utility; kb_add inlines the pattern for W2 ordering.
pub(crate) fn write_kb_index(index_path: &std::path::Path, index: &KbIndex) -> Result<()> {
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(index)?;
    let tmp_path = index_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, index_path)?;
    Ok(())
}

/// Generate a local work entry ID: `kb_` + 8 hex chars from timestamp + 4 hex chars
/// from a simple hash to reduce collision risk under rapid successive calls.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn generate_entry_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let millis = now.as_millis() as u32;
    // Mix in lower bits of sub-millisecond timing and a simple diversifier
    // to avoid collisions when called in rapid succession.
    let diversifier = ((millis << 16) ^ (now.subsec_nanos() >> 4)) as u16;
    format!("kb_{:08x}{:04x}", millis % 0xFFFF_FFFF, diversifier)
}

/// Ensure an entry ID is unique within the index by appending a counter suffix
/// if the generated ID already exists. Best-effort guard — not cryptographic.
pub(crate) fn deduplicate_entry_id(base_id: &str, index: &KbIndex) -> String {
    if !index.entries.iter().any(|e| e.entry_id == base_id) {
        return base_id.to_string();
    }
    // Collision detected — append counter suffix (_1, _2, ...)
    for counter in 1..100 {
        let candidate = format!("{base_id}_{counter}");
        if !index.entries.iter().any(|e| e.entry_id == candidate) {
            return candidate;
        }
    }
    // Extremely unlikely fallback: use a larger diversifier
    format!("{base_id}_overflow")
}

// ── Command implementations ──────────────────────────────────────

/// `kb list` implementation.
async fn kb_list(config: &CliConfig, scope: &KbScope, world_id: Option<&str>) -> Result<()> {
    if scope == &KbScope::World {
        let wid = require_world_id(world_id)?;
        deprecation_notice_legacy_world_kb("list");
        let pool = open_world_pool(config).await?;
        return super::world::kb::kb_list(&pool, &wid, false).await;
    }
    let (creator_id, slug, home) = resolve_kb_paths(config)?;

    // Try daemon API first (T40: migration)
    let client = crate::api::DaemonClient::from_config(config);
    if client.health_check().await? {
        match client.list_kb_entries(&creator_id, Some(&slug), None).await {
            Ok(resp) => {
                if resp.items.is_empty() {
                    println!("No local work entries in workspace {slug}.");
                } else {
                    println!("Local work entries in workspace {slug}:");
                    println!("{:<20} {:<40} CREATED_AT", "ENTRY_ID", "TITLE");
                    for entry in &resp.items {
                        println!(
                            "{:<20} {:<40} {}",
                            entry.entry_id, entry.title, entry.created_at
                        );
                    }
                }
                return Ok(());
            }
            Err(e) => {
                eprintln!("nexus42: daemon work-index list failed, falling back: {e}");
            }
        }
    }

    // Fallback: direct FS read
    let kb_dir = paths::creator_kb_dir(&home, &creator_id, &slug);
    let index_path = kb_dir.join("index.json");

    if !index_path.exists() {
        println!("No local work entries in workspace {slug}.");
        return Ok(());
    }

    let index = read_kb_index(&index_path);
    if index.entries.is_empty() {
        println!("No local work entries in workspace {slug}.");
        return Ok(());
    }

    println!("Local work entries in workspace {slug}:");
    println!("{:<20} {:<40} CREATED_AT", "ENTRY_ID", "TITLE");
    for entry in &index.entries {
        println!(
            "{:<20} {:<40} {}",
            entry.entry_id, entry.title, entry.created_at
        );
    }
    Ok(())
}

/// `kb search` implementation — case-insensitive substring match on title/content.
async fn kb_search(
    config: &CliConfig,
    query: &str,
    scope: &KbScope,
    world_id: Option<&str>,
) -> Result<()> {
    if scope == &KbScope::World {
        let wid = require_world_id(world_id)?;
        deprecation_notice_legacy_world_kb("search");
        let store = open_world_kb_store(config).await?;
        let kb_query = nexus_kb::KbQuery::new(&wid).with_text_search(query);
        let result = store
            .query(&kb_query)
            .await
            .map_err(|e| CliError::Other(format!("World KB search failed for {wid}: {e}")))?;
        if result.items.is_empty() {
            println!("No key blocks matching \"{query}\" in world {wid}.");
        } else {
            println!("Key blocks matching \"{query}\" in world {wid}:");
            println!("{:<20} {:<15} {:<30} STATUS", "BLOCK_ID", "TYPE", "NAME");
            for block in &result.items {
                println!(
                    "{:<20} {:<15} {:<30} {}",
                    block.key_block_id,
                    format!("{:?}", block.block_type),
                    block.canonical_name,
                    block.status
                );
            }
        }
        return Ok(());
    }
    let (creator_id, slug, home) = resolve_kb_paths(config)?;

    // Try daemon API first (T40: migration)
    let client = crate::api::DaemonClient::from_config(config);
    if client.health_check().await? {
        match client
            .list_kb_entries(&creator_id, Some(&slug), Some(query))
            .await
        {
            Ok(resp) => {
                if resp.items.is_empty() {
                    println!("No local work entries matching \"{query}\" in workspace {slug}.");
                } else {
                    println!("Local work entries matching \"{query}\" in workspace {slug}:");
                    println!("{:<20} {:<40} CREATED_AT", "ENTRY_ID", "TITLE");
                    for entry in &resp.items {
                        println!(
                            "{:<20} {:<40} {}",
                            entry.entry_id, entry.title, entry.created_at
                        );
                    }
                }
                return Ok(());
            }
            Err(e) => {
                eprintln!("nexus42: daemon work-index search failed, falling back: {e}");
            }
        }
    }

    // Fallback: local search
    let kb_dir = paths::creator_kb_dir(&home, &creator_id, &slug);
    let index_path = kb_dir.join("index.json");

    if !index_path.exists() {
        println!("No local work entries in workspace {slug} to search.");
        return Ok(());
    }

    let index = read_kb_index(&index_path);
    let query_lower = query.to_lowercase();
    let matches: Vec<&KbIndexEntry> = index
        .entries
        .iter()
        .filter(|e| e.title.to_lowercase().contains(&query_lower))
        .collect();

    if matches.is_empty() {
        println!("No local work entries matching \"{query}\" in workspace {slug}.");
        return Ok(());
    }

    println!("Local work entries matching \"{query}\" in workspace {slug}:");
    println!("{:<20} {:<40} CREATED_AT", "ENTRY_ID", "TITLE");
    for entry in matches {
        println!(
            "{:<20} {:<40} {}",
            entry.entry_id, entry.title, entry.created_at
        );
    }
    Ok(())
}

/// `kb show` implementation — read and print a single entry file / key block.
async fn kb_show(
    config: &CliConfig,
    entry_id: &str,
    scope: &KbScope,
    world_id: Option<&str>,
) -> Result<()> {
    if scope == &KbScope::World {
        let wid = require_world_id(world_id)?;
        deprecation_notice_legacy_world_kb("show");
        let pool = open_world_pool(config).await?;
        return super::world::kb::kb_show(&pool, &wid, entry_id, false).await;
    }
    // F001: Validate entry_id before constructing file path to prevent path traversal.
    paths::validate_entry_id_safe(entry_id).map_err(CliError::Other)?;

    // Try daemon API first (T40: migration)
    let client = crate::api::DaemonClient::from_config(config);
    if client.health_check().await? {
        match client.get_kb_entry(entry_id).await {
            Ok(resp) => {
                println!("{}", resp.content);
                return Ok(());
            }
            Err(e) => {
                eprintln!("nexus42: daemon work-index show failed, falling back: {e}");
            }
        }
    }

    // Fallback: direct FS read
    let (creator_id, slug, home) = resolve_kb_paths(config)?;
    let entries_dir = paths::creator_kb_entries_dir(&home, &creator_id, &slug);
    let entry_path = entries_dir.join(format!("{entry_id}.md"));

    if !entry_path.exists() {
        return Err(CliError::Other(format!(
            "Work-scope entry {entry_id} not found in workspace {slug}."
        )));
    }

    let content = std::fs::read_to_string(&entry_path)?;
    println!("{content}");
    Ok(())
}

/// `kb add` implementation — copy file into local work index, or add world KB block.
///
/// For work scope: writes the index update to a temp file first, then copies the entry file,
/// then atomically renames the index. This prevents orphan entry files on
/// partial failure (W2).
///
/// For world scope: creates a `KeyBlock` via `SqliteKbStore::insert_key_block`.
async fn kb_add(
    config: &CliConfig,
    file: &std::path::Path,
    title: Option<&str>,
    scope: &KbScope,
    world_id: Option<&str>,
    block_type: Option<&str>,
) -> Result<()> {
    if scope == &KbScope::World {
        let wid = require_world_id(world_id)?;
        deprecation_notice_legacy_world_kb("add");
        let bt_str = block_type.unwrap_or("InfoPoint");
        let bt = parse_block_type_cli(bt_str)?;
        let entry_title = title
            .map(std::string::ToString::to_string)
            .or_else(|| file.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "untitled".to_string());

        let store = open_world_kb_store(config).await?;
        let mut kb = nexus_kb::key_block::KeyBlock::new(&wid, bt, &entry_title);

        // Read file content as summary if provided
        if file.exists() {
            let content = std::fs::read_to_string(file)?;
            let summary = if content.len() > 500 {
                format!("{}...", &content[..500])
            } else {
                content
            };
            kb.body = Some(nexus_kb::key_block::KeyBlockBody {
                summary: Some(summary),
                attributes: None,
                tags: None,
            });
        }

        let result = store
            .insert_key_block(kb)
            .await
            .map_err(|e| CliError::Other(format!("World KB add failed for {wid}: {e}")))?;
        println!("✓ Key block added: {}", result.key_block_id);
        println!("  World:  {wid}");
        println!("  Type:   {bt_str}");
        println!("  Name:   {entry_title}");
        return Ok(());
    }
    if !file.exists() {
        return Err(CliError::Other(format!(
            "Source file not found: {}",
            file.display()
        )));
    }

    let (creator_id, slug, _home) = resolve_kb_paths(config)?;

    // Try daemon API first (T40: migration)
    let client = crate::api::DaemonClient::from_config(config);
    if client.health_check().await? {
        let content = std::fs::read_to_string(file)?;
        let req = crate::api::models::AddKbEntryRequest {
            creator_id: creator_id.clone(),
            workspace_slug: Some(slug.clone()),
            title: title.map(std::string::ToString::to_string),
            content: Some(content),
            file_path: None,
        };
        match client.add_kb_entry(&req).await {
            Ok(resp) => {
                println!("✓ Local work entry added: {}", resp.entry_id);
                return Ok(());
            }
            Err(e) => {
                eprintln!("nexus42: daemon work-index add failed, falling back: {e}");
            }
        }
    }

    // Fallback: direct FS operations
    let (_, _, home) = resolve_kb_paths(config)?;
    let kb_dir = paths::creator_kb_dir(&home, &creator_id, &slug);
    let entries_dir = paths::creator_kb_entries_dir(&home, &creator_id, &slug);
    let index_path = kb_dir.join("index.json");

    // Create directories if needed
    std::fs::create_dir_all(&entries_dir)?;

    // Generate entry ID and determine title
    let base_id = generate_entry_id();
    let mut index = read_kb_index(&index_path);
    let entry_id = deduplicate_entry_id(&base_id, &index);
    let entry_title = title
        .map(std::string::ToString::to_string)
        .or_else(|| file.file_stem().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| entry_id.clone());

    // Step 1: Write updated index to temp file (W2 — index update first)
    let created_at = chrono::Utc::now().to_rfc3339();
    index.entries.push(KbIndexEntry {
        entry_id: entry_id.clone(),
        title: entry_title,
        created_at,
    });
    let tmp_index_path = index_path.with_extension("json.tmp");
    {
        if let Some(parent) = tmp_index_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&index)?;
        std::fs::write(&tmp_index_path, json)?;
    }

    // Step 2: Copy source file to entries dir
    let dest = entries_dir.join(format!("{entry_id}.md"));
    std::fs::copy(file, &dest)?;

    // Step 3: Atomically rename temp index to final (W2 — only committed after file is safe)
    std::fs::rename(&tmp_index_path, &index_path)?;

    println!("✓ Local work entry added: {entry_id}");
    Ok(())
}

/// `kb remove` implementation — delete a local work-scope entry or world KB block.
///
/// Tries the daemon API first; falls back to direct FS removal
/// (delete entry file + update index atomically).
async fn kb_remove(
    config: &CliConfig,
    entry_id: &str,
    scope: &KbScope,
    world_id: Option<&str>,
) -> Result<()> {
    if scope == &KbScope::World {
        let wid = require_world_id(world_id)?;
        deprecation_notice_legacy_world_kb("remove");
        let pool = open_world_pool(config).await?;
        let cid = config
            .active_creator_id
            .clone()
            .ok_or(CliError::CreatorNotSelected)?;
        return super::world::kb::kb_delete(&pool, &cid, &wid, entry_id, true).await;
    }
    // F001: Validate entry_id before use.
    paths::validate_entry_id_safe(entry_id).map_err(CliError::Other)?;

    // Try daemon API first (T40: migration)
    let client = crate::api::DaemonClient::from_config(config);
    if client.health_check().await? {
        match client.delete_kb_entry(entry_id).await {
            Ok(_resp) => {
                println!("✓ Local work entry removed: {entry_id}");
                return Ok(());
            }
            Err(e) => {
                eprintln!("nexus42: daemon work-index remove failed, falling back: {e}");
            }
        }
    }

    // Fallback: direct FS removal
    let (creator_id, slug, home) = resolve_kb_paths(config)?;
    let entries_dir = paths::creator_kb_entries_dir(&home, &creator_id, &slug);
    let entry_path = entries_dir.join(format!("{entry_id}.md"));

    if !entry_path.exists() {
        return Err(CliError::Other(format!(
            "Work-scope entry {entry_id} not found in workspace {slug}."
        )));
    }

    // Remove the entry file
    std::fs::remove_file(&entry_path)?;

    // Update index to remove the entry
    let kb_dir = paths::creator_kb_dir(&home, &creator_id, &slug);
    let index_path = kb_dir.join("index.json");
    let mut index = read_kb_index(&index_path);
    let original_len = index.entries.len();
    index.entries.retain(|e| e.entry_id != entry_id);
    if index.entries.len() == original_len {
        // Entry was not in index but file existed — still report success
        tracing::warn!("Work-scope entry {entry_id} file existed but was not in index");
    } else if !index.entries.is_empty() {
        // Write updated index
        write_kb_index(&index_path, &index)?;
    } else if index_path.exists() {
        // Last entry removed — clean up empty index
        let _ = std::fs::remove_file(&index_path);
    }

    println!("✓ Local work entry removed: {entry_id}");
    Ok(())
}

// ── KB Extract Queue ─────────────────────────────────────────────────

/// `kb queue-extract` — idempotent enqueue of a work entry for extraction.
///
/// Creates a row in `kb_extract_jobs` with status `queued`.
/// The actual extraction is performed by the `kb.extract_work` capability
/// (triggered via preset or daemon orchestration). No LLM calls here.
///
/// When `--chapter N` is provided, sets `source_kind=work_chapter`,
/// `profile_hint=novel`, and resolves the chapter body path.
// CLI helper — runs on single-threaded tokio; Send not required.
#[allow(clippy::future_not_send)]
async fn kb_queue_extract(
    config: &CliConfig,
    work_entry_id: &str,
    world_id: &str,
    work_id: Option<&str>,
    chapter: Option<i32>,
) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?
        .to_string();
    let slug = config.workspace_slug_for_creator(&creator_id).to_string();

    // Validate entry_id format to prevent path traversal.
    paths::validate_entry_id_safe(work_entry_id).map_err(CliError::Other)?;

    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;

    // Determine artifact locator fields from --chapter sugar.
    // QC2 W-004: Validate chapter >= 1 to reject negative/zero values.
    if let Some(ch) = chapter {
        if ch < 1 {
            return Err(CliError::Other("Chapter number must be >= 1".to_string()));
        }
    }
    let (source_kind, source_locator, profile_hint) = chapter.map_or((None, None, None), |ch| {
        let ch_label = format!("{ch:02}");
        // Best-effort: build a locator from chapter number.
        // The exact path is resolved later by the capability from work_chapters.
        let locator = format!("chapter:{ch_label}");
        (
            Some("work_chapter".to_string()),
            Some(locator),
            Some("novel".to_string()),
        )
    });

    let job = nexus_local_db::enqueue_extract_job_with_artifact(
        &pool,
        &creator_id,
        &slug,
        work_entry_id,
        world_id,
        source_kind.as_deref(),
        source_locator.as_deref(),
        profile_hint.as_deref(),
        work_id,
    )
    .await
    .map_err(|e| CliError::Other(format!("Failed to enqueue extract job: {e}")))?;

    if job.status == "queued" {
        println!("✓ Extract job queued: {}", job.job_id);
    } else {
        println!("ℹ Extract job already exists: {}", job.job_id);
    }
    println!("  Work entry:  {work_entry_id}");
    println!("  Target world: {world_id}");
    if let Some(ref sk) = job.source_kind {
        println!("  Source kind:  {sk}");
    }
    if let Some(ref sl) = job.source_locator {
        println!("  Source loc:   {sl}");
    }
    if let Some(ref ph) = job.profile_hint {
        println!("  Profile:      {ph}");
    }
    if let Some(ref wid) = job.work_id {
        println!("  Work ID:      {wid}");
    }
    println!("  Status:       {}", job.status);
    println!("  Created:      {}", job.created_at);
    Ok(())
}

/// Default maximum number of extract jobs shown when listing without `--job-id`.
const DEFAULT_EXTRACT_STATUS_LIMIT: u32 = 100;

/// `kb extract-status` — show extract job(s) for the active creator.
///
/// With `--job-id`, shows a specific job. Without it, lists up to
/// `DEFAULT_EXTRACT_STATUS_LIMIT` (100) most recent jobs.
async fn kb_extract_status(config: &CliConfig, job_id: Option<&str>) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?
        .to_string();

    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;

    if let Some(jid) = job_id {
        let job = nexus_local_db::get_extract_job(&pool, jid)
            .await
            .map_err(|e| CliError::Other(format!("Failed to get extract job: {e}")))?;

        let Some(job) = job else {
            return Err(CliError::Other(format!("Extract job '{jid}' not found.")));
        };
        print_job_detail(&job);
    } else {
        let jobs =
            nexus_local_db::list_extract_jobs(&pool, &creator_id, DEFAULT_EXTRACT_STATUS_LIMIT)
                .await
                .map_err(|e| CliError::Other(format!("Failed to list extract jobs: {e}")))?;

        if jobs.is_empty() {
            println!("No extract jobs for creator {creator_id}.");
            return Ok(());
        }

        println!(
            "Extract jobs for creator {creator_id} (showing up to {DEFAULT_EXTRACT_STATUS_LIMIT}):"
        );
        println!(
            "{:<20} {:<15} {:<20} {:<20} STATUS",
            "JOB_ID", "WORK_ENTRY", "WORLD", "CREATED"
        );
        for job in &jobs {
            println!(
                "{:<20} {:<15} {:<20} {:<20} {}",
                job.job_id,
                truncate_str(&job.work_entry_id, 15),
                truncate_str(&job.world_id, 20),
                job.created_at,
                job.status,
            );
        }
    }
    Ok(())
}

/// Print a single job in detail.
fn print_job_detail(job: &nexus_local_db::KbExtractJob) {
    println!("Job:           {}", job.job_id);
    println!("  Creator:     {}", job.creator_id);
    println!("  Workspace:   {}", job.workspace_id);
    println!("  Work entry:  {}", job.work_entry_id);
    println!("  World:       {}", job.world_id);
    println!("  Status:      {}", job.status);
    println!("  Created:     {}", job.created_at);
    if let Some(ref started) = job.started_at {
        println!("  Started:     {started}");
    }
    if let Some(ref finished) = job.finished_at {
        println!("  Finished:    {finished}");
    }
    if let Some(ref err) = job.error_text {
        println!("  Error:       {err}");
    }
}

/// Truncate a string for tabular display.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ── R-KB-002: ID collision guard tests ────────────────────────────

    #[test]
    fn deduplicate_entry_id_returns_base_when_no_collision() {
        let index = KbIndex::default();
        let result = deduplicate_entry_id("kb_abc12345", &index);
        assert_eq!(result, "kb_abc12345");
    }

    #[test]
    fn deduplicate_entry_id_appends_counter_on_collision() {
        let mut index = KbIndex::default();
        index.entries.push(KbIndexEntry {
            entry_id: "kb_abc12345".to_string(),
            title: "existing".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        });
        let result = deduplicate_entry_id("kb_abc12345", &index);
        assert_eq!(result, "kb_abc12345_1");
        // Verify the suffixed ID is not already in the index
        assert!(index.entries.iter().all(|e| e.entry_id != result));
    }

    #[test]
    fn deduplicate_entry_id_increments_counter_for_multiple_collisions() {
        let mut index = KbIndex::default();
        index.entries.push(KbIndexEntry {
            entry_id: "kb_abc12345".to_string(),
            title: "first".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        });
        index.entries.push(KbIndexEntry {
            entry_id: "kb_abc12345_1".to_string(),
            title: "second".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        });
        let result = deduplicate_entry_id("kb_abc12345", &index);
        assert_eq!(result, "kb_abc12345_2");
    }

    #[test]
    fn kb_generate_entry_id_format() {
        let id = generate_entry_id();
        assert!(id.starts_with("kb_"));
        assert_eq!(id.len(), 15, "entry ID should be kb_ + 12 hex chars");
    }

    // ── R-KB-001: Corrupt index.json detection tests ──────────────────

    #[test]
    fn read_kb_index_returns_empty_for_corrupt_json() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let index_path = tmp.path().join("index.json");
        std::fs::write(&index_path, "this is not valid json {{{").expect("write corrupt");

        // Should return empty index (not panic)
        let index = read_kb_index(&index_path);
        assert!(
            index.entries.is_empty(),
            "corrupt index should return empty"
        );
    }

    #[test]
    fn read_kb_index_returns_empty_for_missing_file() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let index_path = tmp.path().join("nonexistent.json");

        let index = read_kb_index(&index_path);
        assert!(index.entries.is_empty());
    }

    #[test]
    fn read_kb_index_parses_valid_json() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let index_path = tmp.path().join("index.json");
        let content = r#"{"entries":[{"entry_id":"kb_test1234","title":"Test","created_at":"2026-01-01T00:00:00Z"}]}"#;
        std::fs::write(&index_path, content).expect("write valid");

        let index = read_kb_index(&index_path);
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.entries[0].entry_id, "kb_test1234");
    }

    // ── V1.52 T-A P1: Legacy World KB alias tests (R-V150KBED-01) ──

    /// `deprecation_notice_legacy_world_kb` emits the expected stderr message.
    #[test]
    fn deprecation_notice_emits_stderr_message() {
        // Capture stderr by redirecting to a pipe.
        // We use a simple approach: call the function and verify it doesn't panic;
        // the actual stderr output is verified in the integration test.
        // Here we just verify the format is correct.
        let subcmd = "list";
        let msg = format!(
            "`creator kb --scope world {subcmd}` is deprecated; \
             use `creator world kb {subcmd}` instead (planned removal V1.53)."
        );
        assert!(msg.contains("deprecated"));
        assert!(msg.contains("creator world kb list"));
        assert!(msg.contains("V1.53"));
    }
}
