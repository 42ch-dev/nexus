//! KB command handlers (local work-scope file index + extract queue).
//!
//! Extracted from `creator/mod.rs` (R19 module refactor).
//! All `creator kb` subcommands are routed through `run`.
//!
//! The work-scope index leaves (`list|search|show|add|remove`) run on the
//! shared direct-call seam (`crate::core`): one owner-scoped `CoreService` is
//! opened, the typed index call is issued (`list_kb_entries`, `get_kb_entry`,
//! `add_kb_entry`, `delete_kb_entry`), and the writer is released before
//! anything is rendered. There is no daemon probe and no HTTP fallback.
//!
//! `rescan`, `queue-extract` and `extract-status` are rebound onto the World KB
//! surface (`creator world kb`) — they queue and scan through the core KB
//! extract authority, never through the file index. This module keeps them only
//! as delegating spellings until the group is retired (v231 retire-file-index
//! Task 1 owns the rebind; the removal is a later task).
//!
//! World-scoped narrative KB lives on the canonical `creator world kb` surface
//! (`commands::creator::world::kb`); `creator kb` serves the work-scope file
//! index only (entity-scope-model §5.3).

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use crate::paths;
use nexus_contracts::{
    AddKbEntryRequest, DeleteKbEntryResponse, GetKbEntryResponse, ListKbEntriesQuery,
    ListKbEntriesResponse,
};
use nexus_core::Principal;
use std::path::PathBuf;

/// Knowledge base subcommands.
///
/// All of them serve the work-scope file index (entity-scope-model §5.3).
/// For World-scoped narrative KB entries use `creator world kb`.
#[derive(Debug, clap::Subcommand)]
pub enum KbCommand {
    /// List work-scope entries from the local workspace file index
    List,

    /// Search local work-scope entries by title
    Search {
        /// Search query string
        query: String,
    },

    /// Show a single local work-scope entry
    Show {
        /// Entry ID to display (e.g. `kb_a1b2c3d4`)
        entry_id: String,
    },

    /// Add a local work-scope entry from a file
    Add {
        /// Path to the source file to add as a work-scope entry
        #[arg(long)]
        file: PathBuf,
        /// Optional title (defaults to filename stem)
        #[arg(long)]
        title: Option<String>,
    },

    /// Remove a local work-scope entry
    Remove {
        /// Entry ID to remove (e.g. `kb_a1b2c3d4`)
        entry_id: String,
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
        /// Target world ID for the resulting `KnowledgeEntryRecord`
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
    /// upserts `kb_extract_jobs` candidates, refreshes confirmed `KnowledgeEntryRecord`
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
        KbCommand::List => kb_list(config).await,
        KbCommand::Search { query } => kb_search(config, &query).await,
        KbCommand::Show { entry_id } => kb_show(config, &entry_id).await,
        KbCommand::Add { file, title } => kb_add(config, &file, title.as_deref()).await,
        KbCommand::Remove { entry_id } => kb_remove(config, &entry_id).await,
        KbCommand::QueueExtract {
            work_entry_id,
            world_id,
            work_id,
            chapter,
        } => {
            crate::commands::creator::world::kb::extract::queue_extract(
                config,
                &work_entry_id,
                &world_id,
                work_id.as_deref(),
                chapter,
            )
            .await
        }
        KbCommand::ExtractStatus { job_id } => {
            crate::commands::creator::world::kb::extract::extract_status(config, job_id.as_deref())
                .await
        }
        KbCommand::Rescan {
            target,
            work,
            dry_run,
            json,
        } => {
            crate::commands::creator::world::kb::rescan::run(
                config,
                target.as_deref(),
                work.as_deref(),
                dry_run,
                json,
            )
            .await
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────

/// The typed work-index query for the admitted principal.
///
/// `creator_id`/`workspace_slug` name the selection the principal was minted
/// from, so the core's ownership re-check compares the request against its own
/// authority; `q` is the title filter the search leaf passes (list passes none).
fn kb_index_query(principal: &Principal, q: Option<&str>) -> ListKbEntriesQuery {
    ListKbEntriesQuery {
        creator_id: Some(principal.creator_id().to_string()),
        workspace_slug: Some(principal.workspace_slug().to_string()),
        q: q.map(std::string::ToString::to_string),
        ..ListKbEntriesQuery::default()
    }
}

// ── Command implementations ──────────────────────────────────────

/// `kb list` implementation — work-scope entries from the local file index.
///
/// One typed core call over the admitted principal; nothing is rendered before
/// `finish_direct` releases the writer.
async fn kb_list(config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        let slug = principal.workspace_slug().to_string();
        let response = core
            .list_kb_entries(&principal, kb_index_query(&principal, None))
            .await
            .map_err(map_core_error)?;
        Ok((slug, response))
    }
    .await;
    let (slug, response) = finish_direct(&core, outcome).await?;

    print_index_entries(&response, None, &slug);
    Ok(())
}

/// `kb search` implementation — case-insensitive title match in the local index.
async fn kb_search(config: &CliConfig, query: &str) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        let slug = principal.workspace_slug().to_string();
        let response = core
            .list_kb_entries(&principal, kb_index_query(&principal, Some(query)))
            .await
            .map_err(map_core_error)?;
        Ok((slug, response))
    }
    .await;
    let (slug, response) = finish_direct(&core, outcome).await?;

    print_index_entries(&response, Some(query), &slug);
    Ok(())
}

/// Render the local work-index table, with or without a search query.
fn print_index_entries(response: &ListKbEntriesResponse, query: Option<&str>, slug: &str) {
    if response.items.is_empty() {
        match query {
            Some(q) => println!("No local work entries matching \"{q}\" in workspace {slug}."),
            None => println!("No local work entries in workspace {slug}."),
        }
        return;
    }
    match query {
        Some(q) => println!("Local work entries matching \"{q}\" in workspace {slug}:"),
        None => println!("Local work entries in workspace {slug}:"),
    }
    println!("{:<20} {:<40} CREATED_AT", "ENTRY_ID", "TITLE");
    for entry in &response.items {
        println!(
            "{:<20} {:<40} {}",
            entry.entry_id, entry.title, entry.created_at
        );
    }
}

/// `kb show` implementation — print one work-scope entry's content.
///
/// The core owns the entry-id validation and the ownership classification
/// (foreign entries are refused, never silently hidden), so the leaf renders
/// only after the read and the writer release both settled.
async fn kb_show(config: &CliConfig, entry_id: &str) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.get_kb_entry(&principal, entry_id.to_string())
            .await
            .map_err(map_core_error)
    }
    .await;
    let entry: GetKbEntryResponse = finish_direct(&core, outcome).await?;

    println!("{}", entry.content);
    Ok(())
}

/// `kb add` implementation — copy a file into the local work-scope index.
///
/// The core owns the crash-consistent write sequence (index temp rename
/// commits the metadata, then the content temp rename commits the entry), the
/// entry-id generation/dedup and the missing-source refusal; this leaf only
/// maps its flags onto the typed request. Title defaults to the file stem, as
/// the flag documents; the core falls back to the entry id when both are
/// absent.
async fn kb_add(config: &CliConfig, file: &std::path::Path, title: Option<&str>) -> Result<()> {
    let entry_title = title
        .map(std::string::ToString::to_string)
        .or_else(|| file.file_stem().map(|s| s.to_string_lossy().to_string()));
    let file_path = file.display().to_string();

    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        let request = AddKbEntryRequest {
            content: None,
            creator_id: principal.creator_id().to_string(),
            file_path: Some(file_path),
            scope: None,
            title: entry_title,
            workspace_slug: Some(principal.workspace_slug().to_string()),
        };
        core.add_kb_entry(&principal, request)
            .await
            .map_err(map_core_error)
    }
    .await;
    let response = finish_direct(&core, outcome).await?;

    println!("✓ Local work entry added: {}", response.entry_id);
    Ok(())
}

/// `kb remove` implementation — delete one work-scope entry.
///
/// The core owns the entry-id validation, the ownership classification and the
/// index update (the entry file and its index row disappear together).
async fn kb_remove(config: &CliConfig, entry_id: &str) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.delete_kb_entry(&principal, entry_id.to_string())
            .await
            .map_err(map_core_error)
    }
    .await;
    let response: DeleteKbEntryResponse = finish_direct(&core, outcome).await?;

    println!("✓ Local work entry removed: {}", response.entry_id);
    Ok(())
}
