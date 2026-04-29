//! Memory management commands.
//!
//! CRUD operations for long-term memories, review pipeline,
//! and fragment management.

use crate::api::daemon_client::DaemonClient;
use crate::config;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_domain::memory_io;
use nexus_domain::LongTermMemory;
use std::io::Write;
use std::str::FromStr;

#[derive(Debug, Subcommand)]
pub enum MemoryCommand {
    /// List all long-term memories for current creator
    List,

    /// Create a new long-term memory
    Create {
        /// Memory slug (filename, path-safe)
        slug: String,

        /// Memory kind (`story_summary`, `research_material`, etc.)
        #[arg(long, default_value = "custom")]
        kind: String,

        /// Initial content (if empty, opens editor)
        #[arg(long)]
        content: Option<String>,
    },

    /// Show a specific memory
    Show { slug: String },

    /// Edit an existing memory (opens in editor)
    Edit { slug: String },

    /// Delete a memory
    Delete {
        slug: String,

        /// Skip confirmation
        #[arg(long)]
        force: bool,
    },

    /// Trigger review of pending queue
    Review,

    /// List memory fragments (requires daemon)
    Fragments,
}

/// Run memory command.
///
/// # Errors
///
/// Returns an error if:
/// - No active creator is set
/// - Database operations fail
/// - File I/O operations fail
pub async fn run(command: MemoryCommand, config: &CliConfig) -> Result<()> {
    let creator_id = config.active_creator_id.as_deref().ok_or_else(|| {
        crate::errors::CliError::Other(
            "No active creator set. Run `nexus42 identity use <id>` first.".to_string(),
        )
    })?;

    match command {
        MemoryCommand::List => list(config, creator_id),
        MemoryCommand::Create {
            slug,
            kind,
            content,
        } => create(config, creator_id, &slug, &kind, content),
        MemoryCommand::Show { slug } => show(config, creator_id, &slug),
        MemoryCommand::Edit { slug } => edit(config, creator_id, &slug),
        MemoryCommand::Delete { slug, force } => delete(config, creator_id, &slug, force),
        MemoryCommand::Review => review(config, creator_id).await,
        MemoryCommand::Fragments => fragments(config, creator_id).await,
    }
}

fn list(_config: &CliConfig, creator_id: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let slugs = memory_io::list_memories(&home, creator_id)?;

    if slugs.is_empty() {
        println!("No long-term memories for creator '{creator_id}'.");
        return Ok(());
    }

    println!("Long-term memories for creator '{creator_id}':\n");

    // Header
    println!("{:<30} {:<20} UPDATED_AT", "SLUG", "KIND");
    println!("{}", "-".repeat(80));

    for slug in &slugs {
        match memory_io::load_memory(&home, creator_id, slug) {
            Ok(mem) => {
                let kind = &mem.frontmatter.memory_kind;
                let updated = &mem.frontmatter.updated_at;
                println!("{slug:<30} {kind:<20} {updated}");
            }
            Err(_) => {
                println!("{:<30} {:<20} ", slug, "(unreadable)");
            }
        }
    }

    println!("\n{} memories", slugs.len());
    Ok(())
}

fn create(
    _config: &CliConfig,
    creator_id: &str,
    slug: &str,
    kind: &str,
    content: Option<String>,
) -> Result<()> {
    let home = config::user_home_dir()?;

    // Validate slug
    if !nexus_domain::long_term_memory::slug_is_safe(slug) {
        return Err(crate::errors::CliError::Other(format!(
            "Invalid slug '{slug}': must not contain '..', '/', '\\\\', or control characters."
        )));
    }

    // Check if memory already exists
    if memory_io::load_memory(&home, creator_id, slug).is_ok() {
        return Err(crate::errors::CliError::Other(format!(
            "Memory '{slug}' already exists for creator '{creator_id}'. Use `memory edit {slug}` to modify it."
        )));
    }

    // Validate kind
    if nexus_domain::memory_item::MemoryKind::from_str(kind).is_err() {
        return Err(crate::errors::CliError::Other(format!(
            "Invalid memory kind '{}'. Valid kinds: {}",
            kind,
            nexus_domain::memory_item::MemoryKind::all_as_strings().join(", ")
        )));
    }

    // Get content
    let body = match content {
        Some(c) => c,
        None => open_editor_temp("Memory content", "")?,
    };

    let mut memory = LongTermMemory::new(kind);
    memory.set_body(&body);
    memory_io::save_memory(&home, creator_id, slug, &memory)?;

    println!("Memory '{slug}' created for creator '{creator_id}'.");
    println!("  Kind: {kind}");
    println!(
        "  Path: {}",
        memory_io::memory_path(&home, creator_id, slug).display()
    );
    Ok(())
}

fn show(_config: &CliConfig, creator_id: &str, slug: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let memory = memory_io::load_memory(&home, creator_id, slug)?;

    // Display frontmatter
    println!("slug: {slug}");
    println!("memory_id: {}", memory.frontmatter.memory_id);
    println!("memory_kind: {}", memory.frontmatter.memory_kind);
    println!("updated_at: {}", memory.frontmatter.updated_at);
    if !memory.frontmatter.source_session_ids.is_empty() {
        println!(
            "source_sessions: {}",
            memory.frontmatter.source_session_ids.join(", ")
        );
    }
    println!();

    // Display body
    println!("{}", memory.body);
    Ok(())
}

fn edit(_config: &CliConfig, creator_id: &str, slug: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let mut memory = memory_io::load_memory(&home, creator_id, slug)?;

    let new_body = open_editor_temp("Memory content", &memory.body)?;
    memory.set_body(&new_body);
    memory_io::save_memory(&home, creator_id, slug, &memory)?;

    println!("Memory '{slug}' updated.");
    Ok(())
}

fn delete(_config: &CliConfig, creator_id: &str, slug: &str, force: bool) -> Result<()> {
    let home = config::user_home_dir()?;

    // Verify memory exists
    memory_io::load_memory(&home, creator_id, slug)?;

    if !force {
        // S-005: Confirm deletion. Empty input (just pressing Enter) = cancel.
        println!(
            "Delete memory '{slug}' for creator '{creator_id}'? [y/N]"
        );
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if trimmed.is_empty() {
            // User pressed Enter without typing anything — treat as cancel
            println!("Aborted (empty input).");
            return Ok(());
        }
        if !trimmed.eq_ignore_ascii_case("y") && !trimmed.eq_ignore_ascii_case("yes") {
            println!("Aborted.");
            return Ok(());
        }
    }

    memory_io::delete_memory(&home, creator_id, slug)?;
    println!("Memory '{slug}' deleted.");
    Ok(())
}

async fn review(config: &CliConfig, creator_id: &str) -> Result<()> {
    let client = DaemonClient::from_config(config);
    let result = client.review_pending_memories(creator_id).await?;
    if result.promoted + result.fragmented + result.dropped == 0 {
        println!("No pending memories to review.");
    } else {
        println!(
            "Review completed: promoted={}, fragmented={}, dropped={}",
            result.promoted, result.fragmented, result.dropped
        );
    }
    Ok(())
}

async fn fragments(config: &CliConfig, creator_id: &str) -> Result<()> {
    let client = DaemonClient::from_config(config);
    let rows = client.list_memory_fragments(creator_id).await?;

    if rows.is_empty() {
        println!("No memory fragments found.");
        return Ok(());
    }

    println!("Memory fragments:\n");
    println!("{:<30} {:<20} SUMMARY", "FRAGMENT_ID", "");
    println!("{}", "-".repeat(80));

    for f in &rows {
        println!("{:<30} {}", f.fragment_id, f.summary);
    }

    println!("\n{} fragments", rows.len());
    Ok(())
}

/// Open a temporary file in the user's $EDITOR, return the edited content.
///
/// Uses `tempfile::NamedTempFile` for automatic cleanup on drop (W-004),
/// preventing temp file leaks if the process crashes or the editor exits
/// abnormally.
fn open_editor_temp(prefix: &str, initial_content: &str) -> Result<String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

    let safe_prefix = prefix.to_lowercase().replace(' ', "-");
    let file_name = format!(
        "nexus42-{}-{}.md",
        safe_prefix,
        uuid::Uuid::new_v4().simple()
    );

    // Use tempfile::NamedTempFile for automatic cleanup on drop (W-004).
    // The file persists long enough for the editor to read it, but is
    // automatically deleted when the NamedTempFile goes out of scope.
    let mut temp_file = tempfile::NamedTempFile::with_prefix(file_name).map_err(|e| {
        crate::errors::CliError::Other(format!("Failed to create temp file: {e}"))
    })?;
    temp_file
        .write_all(initial_content.as_bytes())
        .map_err(|e| crate::errors::CliError::Other(format!("Failed to write temp file: {e}")))?;

    let temp_path = temp_file.path().to_path_buf();

    let status = std::process::Command::new(&editor)
        .arg(&temp_path)
        .status()
        .map_err(|e| {
            crate::errors::CliError::Other(format!("Failed to open editor {editor}: {e}"))
        })?;

    if !status.success() {
        // NamedTempFile auto-deletes on drop — no manual cleanup needed
        return Err(crate::errors::CliError::Other(format!(
            "Editor {editor} exited with non-zero status."
        )));
    }

    let content = std::fs::read_to_string(&temp_path)?;
    // NamedTempFile auto-deletes on drop — no manual cleanup needed
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_command_enum_exists() {
        let _ = MemoryCommand::List;
        let _ = MemoryCommand::Create {
            slug: "test".to_string(),
            kind: "custom".to_string(),
            content: None,
        };
        let _ = MemoryCommand::Show {
            slug: "test".to_string(),
        };
        let _ = MemoryCommand::Edit {
            slug: "test".to_string(),
        };
        let _ = MemoryCommand::Delete {
            slug: "test".to_string(),
            force: false,
        };
        let _ = MemoryCommand::Review;
        let _ = MemoryCommand::Fragments;
    }
}
