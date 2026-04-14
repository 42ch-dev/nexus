//! Memory management commands.
//!
//! CRUD operations for long-term memories, review pipeline,
//! and fragment management.

use crate::config;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_domain::memory_io;
use nexus_domain::LongTermMemory;
use std::str::FromStr;

#[derive(Debug, Subcommand)]
pub enum MemoryCommand {
    /// List all long-term memories for current creator
    List,

    /// Create a new long-term memory
    Create {
        /// Memory slug (filename, path-safe)
        slug: String,

        /// Memory kind (story_summary, research_material, etc.)
        #[arg(long, default_value = "custom")]
        kind: String,

        /// Initial content (if empty, opens editor)
        #[arg(long)]
        content: Option<String>,
    },

    /// Show a specific memory
    Show {
        slug: String,
    },

    /// Edit an existing memory (opens in editor)
    Edit {
        slug: String,
    },

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

pub async fn run(command: MemoryCommand, config: &CliConfig) -> Result<()> {
    let creator_id = config.active_creator_id.as_deref().ok_or_else(|| {
        crate::errors::CliError::Other(
            "No active creator set. Run `nexus42 identity use <id>` first.".to_string(),
        )
    })?;

    match command {
        MemoryCommand::List => list(config, creator_id),
        MemoryCommand::Create { slug, kind, content } => {
            create(config, creator_id, &slug, &kind, content).await
        }
        MemoryCommand::Show { slug } => show(config, creator_id, &slug),
        MemoryCommand::Edit { slug } => edit(config, creator_id, &slug).await,
        MemoryCommand::Delete { slug, force } => delete(config, creator_id, &slug, force),
        MemoryCommand::Review => review(config, creator_id).await,
        MemoryCommand::Fragments => fragments(config).await,
    }
}

fn list(_config: &CliConfig, creator_id: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let slugs = memory_io::list_memories(&home, creator_id)?;

    if slugs.is_empty() {
        println!("No long-term memories for creator '{}'.", creator_id);
        return Ok(());
    }

    println!("Long-term memories for creator '{}':\n", creator_id);

    // Header
    println!(
        "{:<30} {:<20} {}",
        "SLUG", "KIND", "UPDATED AT"
    );
    println!("{}", "-".repeat(80));

    for slug in &slugs {
        match memory_io::load_memory(&home, creator_id, slug) {
            Ok(mem) => {
                let kind = &mem.frontmatter.memory_kind;
                let updated = &mem.frontmatter.updated_at;
                println!("{:<30} {:<20} {}", slug, kind, updated);
            }
            Err(_) => {
                println!("{:<30} {:<20} {}", slug, "(unreadable)", "");
            }
        }
    }

    println!("\n{} memories", slugs.len());
    Ok(())
}

async fn create(
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
            "Invalid slug '{}': must not contain '..', '/', '\\\\', or control characters.",
            slug
        )));
    }

    // Check if memory already exists
    match memory_io::load_memory(&home, creator_id, slug) {
        Ok(_) => {
            return Err(crate::errors::CliError::Other(format!(
                "Memory '{}' already exists for creator '{}'. Use `memory edit {}` to modify it.",
                slug, creator_id, slug
            )));
        }
        Err(_) => {} // Expected: memory doesn't exist yet
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

    println!(
        "Memory '{}' created for creator '{}'.",
        slug, creator_id
    );
    println!("  Kind: {}", kind);
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
    println!("slug: {}", slug);
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

async fn edit(_config: &CliConfig, creator_id: &str, slug: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let mut memory = memory_io::load_memory(&home, creator_id, slug)?;

    let new_body = open_editor_temp("Memory content", &memory.body)?;
    memory.set_body(&new_body);
    memory_io::save_memory(&home, creator_id, slug, &memory)?;

    println!("Memory '{}' updated.", slug);
    Ok(())
}

fn delete(_config: &CliConfig, creator_id: &str, slug: &str, force: bool) -> Result<()> {
    let home = config::user_home_dir()?;

    // Verify memory exists
    memory_io::load_memory(&home, creator_id, slug)?;

    if !force {
        // Confirm deletion
        println!("Delete memory '{}' for creator '{}'? [y/N]", slug, creator_id);
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    memory_io::delete_memory(&home, creator_id, slug)?;
    println!("Memory '{}' deleted.", slug);
    Ok(())
}

async fn review(_config: &CliConfig, _creator_id: &str) -> Result<()> {
    // Review requires the daemon to access pending review queue
    // Fall back gracefully if daemon is not available
    println!("Review queue requires a running daemon.");
    println!("Start the daemon with `nexus42 daemon start` and retry.");
    Ok(())
}

async fn fragments(_config: &CliConfig) -> Result<()> {
    // Fragments require the daemon API
    println!("Fragment listing requires a running daemon.");
    println!("Start the daemon with `nexus42 daemon start` and retry.");
    Ok(())
}

/// Open a temporary file in the user's $EDITOR, return the edited content.
fn open_editor_temp(prefix: &str, initial_content: &str) -> Result<String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

    let temp_dir = std::env::temp_dir();
    let safe_prefix = prefix.to_lowercase().replace(' ', "-");
    let file_name = format!("nexus42-{}-{}.md", safe_prefix, uuid::Uuid::new_v4().simple());
    let temp_path = temp_dir.join(&file_name);

    std::fs::write(&temp_path, initial_content)?;

    let status = std::process::Command::new(&editor)
        .arg(&temp_path)
        .status()
        .map_err(|e| {
            crate::errors::CliError::Other(format!(
                "Failed to open editor {}: {}",
                editor, e
            ))
        })?;

    if !status.success() {
        let _ = std::fs::remove_file(&temp_path);
        return Err(crate::errors::CliError::Other(format!(
            "Editor {} exited with non-zero status.",
            editor
        )));
    }

    let content = std::fs::read_to_string(&temp_path)?;
    let _ = std::fs::remove_file(&temp_path);
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_command_enum_exists() {
        let _cmd = MemoryCommand::List;
        let _cmd = MemoryCommand::Create {
            slug: "test".to_string(),
            kind: "custom".to_string(),
            content: None,
        };
        let _cmd = MemoryCommand::Show {
            slug: "test".to_string(),
        };
        let _cmd = MemoryCommand::Edit {
            slug: "test".to_string(),
        };
        let _cmd = MemoryCommand::Delete {
            slug: "test".to_string(),
            force: false,
        };
        let _cmd = MemoryCommand::Review;
        let _cmd = MemoryCommand::Fragments;
    }
}
