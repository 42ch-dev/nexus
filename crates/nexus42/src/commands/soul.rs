//! SOUL management commands.

#![deny(clippy::unwrap_used)]

use crate::config;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_domain::soul_io;

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
    println!("SOUL.md initialized for creator '{}'.", creator_id);
    println!(
        "Path: {}",
        soul_io::soul_path(&home, creator_id).display()
    );
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
    println!(
        "Personality section updated for creator '{}'.",
        creator_id
    );
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
