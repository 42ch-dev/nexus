//! `creator knowledge` subcommand — User-scoped global knowledge entries.
//!
//! Manages unstructured knowledge entries scoped to the User (not Creator).
//! For Work-scope file index or World narrative KB key blocks, use `creator kb`.
//! See entity-scope-model §5.3–5.4 for the three KB namespaces.
//!
//! Product write path for User knowledge. Writes go through
//! `nexus_local_db::SqliteKnowledgeStore` which implements
//! `nexus_knowledge::KnowledgeStore`.
//!
//! Default `user_id` is `"user_default"` until platform `usr_*` mapping is available.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_knowledge::{KnowledgeEntry, KnowledgeQuery, KnowledgeStore, KnowledgeTag};

/// Default user ID for local CLI usage (until platform usr_* mapping).
const DEFAULT_USER_ID: &str = "user_default";

/// Knowledge subcommands (User-scoped global knowledge; NOT Work-scope or World KB).
///
/// For Work-scope file index, use `creator kb`.
/// For World narrative key blocks, use `creator kb --scope world`.
#[derive(Debug, Subcommand)]
pub enum KnowledgeCommand {
    /// Add a new User-scoped knowledge entry
    Add {
        /// Content text for the knowledge entry
        content: String,
        /// Comma-separated tags (e.g. "rust,tutorial")
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,
        /// User ID (default: `user_default`)
        #[arg(long, default_value = DEFAULT_USER_ID)]
        user_id: String,
    },

    /// List knowledge entries for a user
    List {
        /// User ID (default: `user_default`)
        #[arg(long, default_value = DEFAULT_USER_ID)]
        user_id: String,
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
        /// User ID (default: `user_default`)
        #[arg(long, default_value = DEFAULT_USER_ID)]
        user_id: String,
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

/// Open a DB pool and create a knowledge store.
async fn open_knowledge_store(config: &CliConfig) -> Result<nexus_local_db::SqliteKnowledgeStore> {
    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;
    Ok(nexus_local_db::SqliteKnowledgeStore::new(pool))
}

/// Run a knowledge subcommand.
///
/// # Errors
///
/// Returns `CliError` if the database is unavailable or any operation fails.
pub async fn run(cmd: KnowledgeCommand, config: &CliConfig) -> Result<()> {
    let store = open_knowledge_store(config).await?;
    match cmd {
        KnowledgeCommand::Add {
            content,
            tags,
            user_id,
        } => run_add(&store, &content, tags, &user_id).await,
        KnowledgeCommand::List {
            user_id,
            tags,
            limit,
            offset,
        } => run_list(&store, &user_id, tags, limit, offset).await,
        KnowledgeCommand::Search {
            query,
            user_id,
            tags,
            limit,
            offset,
        } => run_search(&store, &query, &user_id, tags, limit, offset).await,
    }
}

async fn run_add(
    store: &dyn KnowledgeStore,
    content: &str,
    tags: Option<Vec<String>>,
    user_id: &str,
) -> Result<()> {
    let tag_list = tags
        .unwrap_or_default()
        .into_iter()
        .map(|s| KnowledgeTag::new(&s))
        .collect();
    let entry = KnowledgeEntry::new(user_id, tag_list, content);
    let id = entry.id.clone();

    let stored = store.store(entry).await.map_err(|e| {
        crate::errors::CliError::Other(format!("Failed to add knowledge entry: {e}"))
    })?;

    println!("✓ Knowledge entry added: {id}");
    println!("  User:    {}", stored.user_id);
    println!(
        "  Tags:    {}",
        stored
            .tags
            .iter()
            .map(KnowledgeTag::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("  Content: {}", truncate(&stored.content, 80));
    Ok(())
}

async fn run_list(
    store: &dyn KnowledgeStore,
    user_id: &str,
    tags: Option<Vec<String>>,
    limit: u32,
    offset: u32,
) -> Result<()> {
    let mut query = KnowledgeQuery::for_user(user_id)
        .with_limit(limit)
        .with_offset(offset);
    if let Some(tag_strs) = tags {
        let tag_list: Vec<KnowledgeTag> = tag_strs
            .into_iter()
            .map(|s| KnowledgeTag::new(&s))
            .collect();
        query = query.with_tags(tag_list);
    }

    let result = store.list(&query).await.map_err(|e| {
        crate::errors::CliError::Other(format!("Failed to list knowledge entries: {e}"))
    })?;

    if result.entries.is_empty() {
        println!("No knowledge entries for user '{user_id}'.");
        return Ok(());
    }

    println!(
        "Knowledge entries for user '{user_id}' ({} total, showing {}):",
        result.total_count,
        result.entries.len()
    );
    println!(
        "{:<40} {:<30} {:<20} TAGS",
        "ENTRY_ID", "CONTENT", "CREATED_AT"
    );
    for entry in &result.entries {
        let tag_str = entry
            .tags
            .iter()
            .map(KnowledgeTag::as_str)
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{:<40} {:<30} {:<20} {}",
            entry.id,
            truncate(&entry.content, 30),
            &entry.created_at[..19.min(entry.created_at.len())],
            tag_str
        );
    }
    Ok(())
}

async fn run_search(
    store: &dyn KnowledgeStore,
    query_text: &str,
    user_id: &str,
    tags: Option<Vec<String>>,
    limit: u32,
    offset: u32,
) -> Result<()> {
    let tag_refs: Option<Vec<KnowledgeTag>> =
        tags.map(|ts| ts.into_iter().map(|s| KnowledgeTag::new(&s)).collect());
    let tag_slice = tag_refs.as_deref();

    let result = store
        .search(user_id, query_text, tag_slice, limit, offset)
        .await
        .map_err(|e| {
            crate::errors::CliError::Other(format!("Failed to search knowledge entries: {e}"))
        })?;

    if result.entries.is_empty() {
        println!("No knowledge entries matching \"{query_text}\" for user '{user_id}'.");
        return Ok(());
    }

    println!(
        "Entries matching \"{query_text}\" for user '{user_id}' ({} total):",
        result.total_count
    );
    println!(
        "{:<40} {:<30} {:<20} TAGS",
        "ENTRY_ID", "CONTENT", "CREATED_AT"
    );
    for entry in &result.entries {
        let tag_str = entry
            .tags
            .iter()
            .map(KnowledgeTag::as_str)
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{:<40} {:<30} {:<20} {}",
            entry.id,
            truncate(&entry.content, 30),
            &entry.created_at[..19.min(entry.created_at.len())],
            tag_str
        );
    }
    Ok(())
}

/// Truncate a string to `max_len` with ellipsis if needed.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
