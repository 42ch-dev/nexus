//! `creator knowledge` subcommand — User-scoped global knowledge entries.
//!
//! Manages unstructured knowledge entries scoped to the User (not Creator).
//! For the Work-scope file index, use `creator kb`; for World narrative KB
//! entries, use `creator world kb`. See entity-scope-model §5.3–5.4 for the
//! three KB namespaces.
//!
//! All three leaves run on the shared direct-call seam (`crate::core`): one
//! owner-scoped `CoreService` is opened, the typed call is issued
//! (`add_user_knowledge`, `list_user_knowledge`, `search_user_knowledge`), and
//! the writer is released before anything is rendered. The core's own
//! work-write admission and store validation are the authority, so the CLI
//! holds no second `SqliteKnowledgeStore` caller and never probes (or falls
//! back to) the daemon.
//!
//! Entries are stored under the core's default local user identity
//! (`user_default` until the platform `usr_*` mapping exists), which is why
//! these leaves carry no `--user-id`: the typed seam has no such parameter, and
//! a flag no owner honours would be a silent no-op.

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::Result;
use clap::Subcommand;
use nexus_knowledge::{KnowledgeResult, KnowledgeTag, UserKnowledgeEntry};

/// Knowledge subcommands (User-scoped global knowledge; NOT Work-scope or World KB).
///
/// For the Work-scope file index, use `creator kb`.
/// For World narrative knowledge entries, use `creator world kb`.
#[derive(Debug, Subcommand)]
pub enum KnowledgeCommand {
    /// Add a new User-scoped knowledge entry
    Add {
        /// Content text for the knowledge entry
        content: String,
        /// Comma-separated tags (e.g. "rust,tutorial")
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,
    },

    /// List knowledge entries
    List {
        /// Filter by comma-separated tags
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,
        /// Maximum entries to return
        #[arg(long, default_value_t = 50)]
        limit: u32,
        /// Number of entries to skip
        #[arg(long, default_value_t = 0)]
        offset: u32,
    },

    /// Search knowledge entries by text
    Search {
        /// Search query text
        query: String,
        /// Filter by comma-separated tags
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,
        /// Maximum entries to return
        #[arg(long, default_value_t = 50)]
        limit: u32,
        /// Number of entries to skip
        #[arg(long, default_value_t = 0)]
        offset: u32,
    },
}

/// Run a knowledge subcommand.
///
/// # Errors
///
/// Returns `CliError` when the seam cannot be opened (including a selection
/// that names no materialized workspace), the core refuses the operation
/// (admission, store validation), or the writer release did not settle.
pub async fn run(cmd: KnowledgeCommand, config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        match cmd {
            KnowledgeCommand::Add { content, tags } => {
                let entry = core
                    .add_user_knowledge(&principal, content, tags)
                    .await
                    .map_err(map_core_error)?;
                Ok(render_added(&entry))
            }
            KnowledgeCommand::List {
                tags,
                limit,
                offset,
            } => {
                let result = core
                    .list_user_knowledge(&principal, tags, limit, offset)
                    .await
                    .map_err(map_core_error)?;
                Ok(render_list(&result))
            }
            KnowledgeCommand::Search {
                query,
                tags,
                limit,
                offset,
            } => {
                let result = core
                    .search_user_knowledge(&principal, &query, tags, limit, offset)
                    .await
                    .map_err(map_core_error)?;
                Ok(render_search(&query, &result))
            }
        }
    }
    .await;

    let text = finish_direct(&core, outcome).await?;
    println!("{text}");
    Ok(())
}

/// Render the `knowledge add` confirmation.
fn render_added(entry: &UserKnowledgeEntry) -> String {
    [
        format!("✓ Knowledge entry added: {}", entry.id),
        format!("  User:    {}", entry.user_id),
        format!("  Tags:    {}", join_tags(&entry.tags)),
        format!("  Content: {}", truncate(&entry.content, 80)),
    ]
    .join("\n")
}

/// Render the `knowledge list` page.
fn render_list(result: &KnowledgeResult) -> String {
    if result.entries.is_empty() {
        return "No knowledge entries.".to_string();
    }
    let mut lines = vec![format!(
        "Knowledge entries ({} total, showing {}):",
        result.total_count,
        result.entries.len()
    )];
    lines.push(entry_header());
    lines.extend(result.entries.iter().map(entry_row));
    lines.join("\n")
}

/// Render the `knowledge search` page.
fn render_search(query: &str, result: &KnowledgeResult) -> String {
    if result.entries.is_empty() {
        return format!("No knowledge entries matching \"{query}\".");
    }
    let mut lines = vec![format!(
        "Entries matching \"{query}\" ({} total):",
        result.total_count
    )];
    lines.push(entry_header());
    lines.extend(result.entries.iter().map(entry_row));
    lines.join("\n")
}

/// The shared table header for the two read leaves.
fn entry_header() -> String {
    format!(
        "{:<40} {:<30} {:<20} TAGS",
        "ENTRY_ID", "CONTENT", "CREATED_AT"
    )
}

/// One table row for the two read leaves.
fn entry_row(entry: &UserKnowledgeEntry) -> String {
    format!(
        "{:<40} {:<30} {:<20} {}",
        entry.id,
        truncate(&entry.content, 30),
        &entry.created_at[..19.min(entry.created_at.len())],
        join_tags(&entry.tags)
    )
}

/// Comma-join the tag labels.
fn join_tags(tags: &[KnowledgeTag]) -> String {
    tags.iter()
        .map(KnowledgeTag::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

/// Truncate a string to `max_len` with ellipsis if needed.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
