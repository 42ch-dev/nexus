//! SOUL management commands.

use crate::config;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_creator_memory::soul_io;
use nexus_local_db::{
    get_soul_meta as db_get_soul_meta, upsert_soul_meta as db_upsert_soul_meta, SoulMeta,
};

#[derive(Debug, Subcommand)]
pub enum SoulCommand {
    /// Initialize a new SOUL.md for the active creator
    Init,
    /// Show current SOUL.md content
    Show,
    /// Edit the personality section of SOUL.md
    EditPersonality {
        /// New personality content (markdown). Use "-" to read from stdin.
        content: Option<String>,
    },
    /// Validate SOUL.md structure and sections
    Validate,
    /// Push personality section to long-term memory
    PushPersonality,
}

/// Run soul command.
///
/// # Errors
///
/// Returns an error if:
/// - No active creator is set
/// - Database operations fail
/// - File I/O operations fail
/// - Daemon API calls fail
pub async fn run(command: SoulCommand, config: &CliConfig) -> Result<()> {
    let creator_id = config.active_creator_id.as_deref().ok_or_else(|| {
        crate::errors::CliError::Other(
            "No active creator set. Run `nexus42 identity use <id>` first.".to_string(),
        )
    })?;

    match command {
        SoulCommand::Init => init(config, creator_id).await,
        SoulCommand::Show => show(config, creator_id),
        SoulCommand::EditPersonality { content } => {
            edit_personality(config, creator_id, content).await
        }
        SoulCommand::Validate => validate(config, creator_id),
        SoulCommand::PushPersonality => push_personality(config, creator_id),
    }
}

async fn init(_config: &CliConfig, creator_id: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    if soul_io::exists(&home, creator_id) {
        return Err(crate::errors::CliError::Other(format!(
            "SOUL.md already exists for creator '{creator_id}'. Use `soul show` to view it."
        )));
    }
    let doc = soul_io::create(&home, creator_id)?;
    doc.validate()?;

    // Persist metadata to DB (best-effort; file I/O is the primary action).
    if let Some(pool) = open_global_db().await? {
        let path = soul_io::soul_path(&home, creator_id);
        let now = chrono::Utc::now().to_rfc3339();
        let meta = SoulMeta {
            creator_id: creator_id.to_string(),
            file_path: path.display().to_string(),
            schema_version: 1,
            personality_hash: doc.personality.as_ref().map(|p| simple_hash(p)),
            experience_hash: doc.experience.as_ref().map(|e| simple_hash(e)),
            created_at: now.clone(),
            updated_at: now,
        };
        if let Err(e) = db_upsert_soul_meta(&pool, &meta).await {
            eprintln!("warning: failed to persist soul metadata: {e}");
        }
    }

    println!("SOUL.md initialized for creator '{creator_id}'.");
    println!("Path: {}", soul_io::soul_path(&home, creator_id).display());
    Ok(())
}

fn show(_config: &CliConfig, creator_id: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let doc = soul_io::load(&home, creator_id)?;
    println!("{}", doc.render());
    Ok(())
}

async fn edit_personality(
    _config: &CliConfig,
    creator_id: &str,
    content: Option<String>,
) -> Result<()> {
    let home = config::user_home_dir()?;
    let mut doc = soul_io::load(&home, creator_id)?;
    let new_content = match content.as_deref() {
        Some("-") => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
        Some(text) => text.to_string(),
        None => {
            return Err(crate::errors::CliError::Other(
                "Provide personality content or use '-' to read from stdin.".to_string(),
            ))
        }
    };
    doc.set_personality(new_content);
    soul_io::save(&home, creator_id, &doc)?;

    // Update metadata in DB (best-effort).
    if let Some(pool) = open_global_db().await? {
        let path = soul_io::soul_path(&home, creator_id);
        let now = chrono::Utc::now().to_rfc3339();
        let existing_created_at = db_get_soul_meta(&pool, creator_id)
            .await
            .ok()
            .flatten()
            .map(|m| m.created_at);
        let meta = SoulMeta {
            creator_id: creator_id.to_string(),
            file_path: path.display().to_string(),
            schema_version: 1,
            personality_hash: doc.personality.as_ref().map(|p| simple_hash(p)),
            experience_hash: doc.experience.as_ref().map(|e| simple_hash(e)),
            created_at: existing_created_at.unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        if let Err(e) = db_upsert_soul_meta(&pool, &meta).await {
            eprintln!("warning: failed to update soul metadata: {e}");
        }
    }

    println!("Personality section updated for creator '{creator_id}'.");
    Ok(())
}

fn validate(_config: &CliConfig, creator_id: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let doc = soul_io::validate(&home, creator_id)?;
    println!("SOUL.md for creator '{creator_id}' is valid.");
    println!("  Sections: Personality ✓, Experience ✓");
    if !doc.extra_sections.is_empty() {
        println!(
            "  Extra sections: {}",
            doc.extra_sections
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

fn push_personality(_config: &CliConfig, creator_id: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let soul = soul_io::load(&home, creator_id)?;

    let memory =
        nexus_creator_memory::personality_sync::push_personality_to_memory(&home, creator_id, &soul)?;

    println!("Personality pushed to long-term memory for creator '{creator_id}'.");
    println!("  Memory ID: {}", memory.frontmatter.memory_id);
    println!("  Kind: {}", memory.frontmatter.memory_kind);
    if let Some(path) = &memory.source_path {
        println!("  Path: {}", path.display());
    }
    Ok(())
}

/// Open or create the global database, returning `Some(pool)` on success
/// or `None` if the DB is not available (e.g., first run before `nexus42 init`).
///
/// This mirrors `identity.rs::open_global_db()` but returns `None` instead of
/// erroring so SOUL operations degrade gracefully when no DB exists yet.
async fn open_global_db() -> Result<Option<nexus_local_db::SqlitePool>> {
    let home = config::user_home_dir()?;
    let nexus_dir = home.join(".nexus42");

    // If the nexus dir doesn't exist, no DB to open — that's OK for SOUL ops.
    if !nexus_dir.exists() {
        return Ok(None);
    }

    let db_path = nexus_dir.join("state.db");
    if !db_path.exists() {
        return Ok(None);
    }

    let pool = crate::db::Schema::init(&db_path).await?;
    Ok(Some(pool))
}

/// Cheap deterministic hash for SOUL section content (not cryptographic).
fn simple_hash(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soul_command_enum_exists() {
        // Verify the enum can be constructed (compile-time check)
        let _ = SoulCommand::Init;
        let _ = SoulCommand::Show;
        let _ = SoulCommand::Validate;
        let _ = SoulCommand::EditPersonality {
            content: Some("test".to_string()),
        };
    }
}
