//! SOUL management commands.

use crate::config;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_domain::soul_io;
use nexus_local_db::{upsert_soul_meta, get_soul_meta, SoulMeta};

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
}

pub async fn run(command: SoulCommand, config: &CliConfig) -> Result<()> {
    let creator_id = config.active_creator_id.as_deref().ok_or_else(|| {
        crate::errors::CliError::Other(
            "No active creator set. Run `nexus42 identity use <id>` first.".to_string(),
        )
    })?;

    match command {
        SoulCommand::Init => init(config, creator_id).await,
        SoulCommand::Show => show(config, creator_id).await,
        SoulCommand::EditPersonality { content } => {
            edit_personality(config, creator_id, content).await
        }
        SoulCommand::Validate => validate(config, creator_id).await,
    }
}

async fn init(_config: &CliConfig, creator_id: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    if soul_io::exists(&home, creator_id) {
        return Err(crate::errors::CliError::Other(format!(
            "SOUL.md already exists for creator '{}'. Use `soul show` to view it.",
            creator_id
        )));
    }
    let doc = soul_io::create(&home, creator_id)?;
    doc.validate()?;

    // Persist metadata to DB (best-effort; file I/O is the primary action).
    if let Some(conn) = open_global_db()? {
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
        if let Err(e) = upsert_soul_meta(&conn, &meta) {
            eprintln!("warning: failed to persist soul metadata: {e}");
        }
    }

    println!("SOUL.md initialized for creator '{}'.", creator_id);
    println!("Path: {}", soul_io::soul_path(&home, creator_id).display());
    Ok(())
}

async fn show(_config: &CliConfig, creator_id: &str) -> Result<()> {
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
    if let Some(conn) = open_global_db()? {
        let path = soul_io::soul_path(&home, creator_id);
        let now = chrono::Utc::now().to_rfc3339();
        let meta = SoulMeta {
            creator_id: creator_id.to_string(),
            file_path: path.display().to_string(),
            schema_version: 1,
            personality_hash: doc.personality.as_ref().map(|p| simple_hash(p)),
            experience_hash: doc.experience.as_ref().map(|e| simple_hash(e)),
            // Preserve created_at by reading existing record, or use now.
            created_at: get_soul_meta(&conn, creator_id)
                .ok()
                .flatten()
                .map(|m| m.created_at)
                .unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        if let Err(e) = upsert_soul_meta(&conn, &meta) {
            eprintln!("warning: failed to update soul metadata: {e}");
        }
    }

    println!("Personality section updated for creator '{}'.", creator_id);
    Ok(())
}

async fn validate(_config: &CliConfig, creator_id: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let doc = soul_io::validate(&home, creator_id)?;
    println!("SOUL.md for creator '{}' is valid.", creator_id);
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

/// Open or create the global database, returning `Some(conn)` on success
/// or `None` if the DB is not available (e.g., first run before `nexus42 init`).
///
/// This mirrors `identity.rs::open_global_db()` but returns `None` instead of
/// erroring so SOUL operations degrade gracefully when no DB exists yet.
fn open_global_db() -> Result<Option<rusqlite::Connection>> {
    use nexus_local_db::{init, RuntimeRole};

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

    let conn = rusqlite::Connection::open(&db_path)?;
    init(&conn, RuntimeRole::Cli)?;
    nexus_local_db::run_migrations(&conn)?;
    Ok(Some(conn))
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
        let _cmd = SoulCommand::Init;
        let _cmd = SoulCommand::Show;
        let _cmd = SoulCommand::Validate;
        let _cmd = SoulCommand::EditPersonality {
            content: Some("test".to_string()),
        };
    }
}
