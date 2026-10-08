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
//! The extract queue (`queue-extract`, `extract-status`) runs on the same
//! direct-call seam: the core KB extract authority
//! ([`nexus_core::CoreService::queue_kb_extract`],
//! `list_kb_extract_jobs`, `get_kb_extract_job`) owns the entry-id
//! sanitization, the `--chapter` locator sugar, the idempotency key and the
//! creator/workspace binding. The enqueue leaf takes the workspace's
//! engine-owned admission (`crate::core::open_engine_owned_core`) because
//! `kb_extract_jobs` is guarded to a migration/engine writer at the schema
//! level; the status leaf reads over the direct-writer one. The refreshable
//! scan (`rescan`) stays on the existing local stores; it is admitted at the
//! seam (`crate::core::require_materialized_workspace`) before its pool open,
//! so no `creator kb` entrance can migrate — and therefore create — a
//! workspace the selection never had.
//!
//! World-scoped narrative KB lives on the canonical `creator world kb` surface
//! (`commands::creator::world::kb`); `creator kb` serves the work-scope file
//! index only (entity-scope-model §5.3). User-scoped global knowledge is
//! `creator knowledge`.

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core, open_engine_owned_core};
use crate::errors::{CliError, Result};
use crate::paths;
use nexus_contracts::{
    AddKbEntryRequest, DeleteKbEntryResponse, GetKbEntryResponse, ListKbEntriesQuery,
    ListKbEntriesResponse,
};
use nexus_core::{Principal, QueueKbExtractParams};
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

/// Knowledge base subcommands.
///
/// All of them serve the work-scope file index (entity-scope-model §5.3).
/// For World-scoped narrative KB entries use `creator world kb`; for
/// User-scoped global knowledge entries use `creator knowledge`.
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

// ── KB Extract Queue ─────────────────────────────────────────────────

/// `kb queue-extract` — idempotent enqueue of a work entry for extraction.
///
/// Creates a row in `kb_extract_jobs` with status `queued` through the core
/// KB extract authority ([`nexus_core::CoreService::queue_kb_extract`]); the
/// core owns the entry-id sanitization, the chapter-locator sugar and the
/// idempotency key, so this leaf only maps its flags onto the typed request
/// and renders the job. The actual extraction is performed by the
/// `kb.extract_work` capability (triggered via preset or daemon
/// orchestration). No LLM calls here.
///
/// The core is opened over the workspace's **engine-owned** admission
/// ([`crate::core::open_engine_owned_core`]): `kb_extract_jobs` is guarded to a
/// migration/engine writer, so the direct-writer pool this leaf used to open
/// could never write the row.
///
/// When `--chapter N` is provided, the core sets `source_kind=work_chapter`,
/// `profile_hint=novel`, and derives the chapter body locator.
// CLI helper — runs on single-threaded tokio; Send not required.
#[allow(clippy::future_not_send)]
async fn kb_queue_extract(
    config: &CliConfig,
    work_entry_id: &str,
    world_id: &str,
    work_id: Option<&str>,
    chapter: Option<i32>,
) -> Result<()> {
    // QC2 W-004: reject negative/zero chapter values before opening the core,
    // so the legacy refusal wording is the one the caller reads. The producer
    // refuses the same input fail-closed for every other transport.
    if let Some(ch) = chapter {
        if ch < 1 {
            return Err(CliError::Other("Chapter number must be >= 1".to_string()));
        }
    }

    // The queue is engine-owned job state (`kb_extract_jobs` is guarded to a
    // migration/engine writer), so the enqueue leaf takes the engine admission;
    // the direct-writer pool cannot write that table.
    let core = open_engine_owned_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.queue_kb_extract(
            &principal,
            QueueKbExtractParams {
                work_entry_id: work_entry_id.to_string(),
                world_id: world_id.to_string(),
                work_id: work_id.map(std::string::ToString::to_string),
                chapter,
            },
        )
        .await
        .map_err(map_core_error)
    }
    .await;
    let job = finish_direct(&core, outcome).await?;

    if job.status == "queued" {
        println!("✓ Extract job queued: {}", job.job_id);
    } else {
        println!("ℹ Extract job already exists: {}", job.job_id);
    }
    println!("  Work entry:  {}", job.work_entry_id);
    println!("  Target world: {world_id}");
    if let Some(sk) = &job.source_kind {
        println!("  Source kind:  {sk}");
    }
    if let Some(sl) = &job.source_locator {
        println!("  Source loc:   {sl}");
    }
    if let Some(ph) = &job.profile_hint {
        println!("  Profile:      {ph}");
    }
    if let Some(wid) = &job.work_id {
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
/// `DEFAULT_EXTRACT_STATUS_LIMIT` (100) most recent jobs. Each arm reads
/// through the core and renders only after [`finish_direct`] released the
/// writer.
async fn kb_extract_status(config: &CliConfig, job_id: Option<&str>) -> Result<()> {
    if let Some(jid) = job_id {
        return show_extract_job(config, jid).await;
    }
    list_extract_jobs(config).await
}

/// `kb extract-status --job-id <id>` — one job in detail.
async fn show_extract_job(config: &CliConfig, job_id: &str) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.get_kb_extract_job(&principal, job_id)
            .await
            .map_err(map_core_error)
    }
    .await;
    let job = finish_direct(&core, outcome).await?;

    let Some(job) = job else {
        return Err(CliError::Other(format!(
            "Extract job '{job_id}' not found."
        )));
    };
    print_job_detail(&job);
    Ok(())
}

/// `kb extract-status` — the creator's most recent jobs.
async fn list_extract_jobs(config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        let creator_id = principal.creator_id().to_string();
        let jobs = core
            .list_kb_extract_jobs(&principal, DEFAULT_EXTRACT_STATUS_LIMIT)
            .await
            .map_err(map_core_error)?;
        Ok((creator_id, jobs))
    }
    .await;
    let (creator_id, jobs) = finish_direct(&core, outcome).await?;

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
