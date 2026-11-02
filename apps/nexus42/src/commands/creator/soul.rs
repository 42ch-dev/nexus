//! SOUL management commands.
//!
//! Since NXM-6 the SOUL document is the provider's authored `personality`
//! profile (`memory.profile.get` / `memory.profile.update`) instead of a bearer
//! file: every leaf reads that document through the direct core, edits it with
//! the pure `SoulDocument` type, and writes it back on the revision it read.
//! The provider bridge projects an unmigrated `SOUL.md` as that same profile,
//! and the first write replaces the projection (the bridge keeps a stored kind
//! over it), so no leaf ever touches the bearer file.

use crate::commands::creator::memory::memory_identity;
use crate::config::CliConfig;
use crate::core::{map_core_error, with_direct_core, with_read_only_core};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_core::ProfileDocument;
use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::soul::SoulDocument;

#[derive(Debug, Subcommand)]
pub enum SoulCommand {
    /// Initialize a new SOUL document for the active creator
    Init,
    /// Show the current SOUL document
    Show,
    /// Edit the personality section of the SOUL document
    EditPersonality {
        /// New personality content (markdown). Use "-" to read from stdin.
        content: Option<String>,
    },
    /// Validate the SOUL document structure and sections
    Validate,
    /// Push the personality section to long-term memory
    PushPersonality,
    /// Refresh the ## Experience section from long-term memories (deterministic, no LLM)
    RefreshExperience,
}

/// Run soul command.
///
/// # Errors
///
/// Returns an error if:
/// - No active creator is set
/// - No SOUL document is authored yet
/// - The mapped core refusal reaches the leaf
pub async fn run(command: SoulCommand, config: &CliConfig) -> Result<()> {
    let creator_id = config.active_creator_id.as_deref().ok_or_else(|| {
        crate::errors::CliError::Other(
            "No active creator set. Run `nexus42 system identity use <id>` first.".to_string(),
        )
    })?;

    match command {
        SoulCommand::Init => init(config, creator_id).await,
        SoulCommand::Show => show(config, creator_id).await,
        SoulCommand::EditPersonality { content } => {
            edit_personality(config, creator_id, content).await
        }
        SoulCommand::Validate => validate(config, creator_id).await,
        SoulCommand::PushPersonality => push_personality(config, creator_id).await,
        SoulCommand::RefreshExperience => refresh_experience(config, creator_id).await,
    }
}

/// The retained "no SOUL document" refusal. The document has no file path after
/// the cutover, so the wording drops the old `at <path>` suffix.
fn soul_not_found(creator_id: &str) -> CliError {
    CliError::Other(format!("SOUL.md not found for creator '{creator_id}'."))
}

/// Map a SOUL document failure onto the retained `Memory error:` envelope the
/// file-backed leaf rendered.
fn soul_error(error: MemoryError) -> CliError {
    CliError::Other(format!("Memory error: {error}"))
}

/// Read the authored SOUL document body and its provider revision.
async fn read_soul(config: &CliConfig, creator_id: &str) -> Result<(String, u64)> {
    with_read_only_core(config, async |core, principal| {
        let profile: ProfileDocument = core
            .get_creator_soul(principal)
            .await
            .map_err(map_core_error)?
            .ok_or_else(|| soul_not_found(creator_id))?;
        Ok((profile.document.body, profile.revision.get()))
    })
    .await
}

async fn init(config: &CliConfig, creator_id: &str) -> Result<()> {
    let document = SoulDocument::for_creator(creator_id).render();
    let text = with_direct_core(config, async |core, principal| {
        if core
            .get_creator_soul(principal)
            .await
            .map_err(map_core_error)?
            .is_some()
        {
            return Err(CliError::Other(format!(
                "SOUL.md already exists for creator '{creator_id}'. Use `soul show` to view it."
            )));
        }
        core.set_creator_soul(principal, &document, None)
            .await
            .map_err(map_core_error)?;
        Ok(format!("SOUL.md initialized for creator '{creator_id}'."))
    })
    .await?;
    println!("{text}");
    Ok(())
}

async fn show(config: &CliConfig, creator_id: &str) -> Result<()> {
    let body = with_read_only_core(config, async |core, principal| {
        let profile = core
            .get_creator_soul(principal)
            .await
            .map_err(map_core_error)?
            .ok_or_else(|| soul_not_found(creator_id))?;
        Ok(profile.document.body)
    })
    .await?;
    println!("{body}");
    Ok(())
}

async fn edit_personality(
    config: &CliConfig,
    creator_id: &str,
    content: Option<String>,
) -> Result<()> {
    let (body, revision) = read_soul(config, creator_id).await?;
    let mut soul = SoulDocument::parse(&body).map_err(soul_error)?;
    let new_content = match content.as_deref() {
        Some("-") => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
        Some(text) => text.to_string(),
        None => {
            return Err(CliError::Other(
                "Provide personality content or use '-' to read from stdin.".to_string(),
            ))
        }
    };
    soul.set_personality(new_content);
    let rendered = soul.render();

    with_direct_core(config, async |core, principal| {
        core.set_creator_soul(principal, &rendered, Some(revision))
            .await
            .map_err(map_core_error)
    })
    .await?;
    println!("Personality section updated for creator '{creator_id}'.");
    Ok(())
}

async fn validate(config: &CliConfig, creator_id: &str) -> Result<()> {
    let (body, _) = read_soul(config, creator_id).await?;
    let soul = SoulDocument::parse(&body).map_err(soul_error)?;
    soul.validate().map_err(soul_error)?;
    println!("SOUL.md for creator '{creator_id}' is valid.");
    println!("  Sections: Personality ✓, Experience ✓");
    if !soul.extra_sections.is_empty() {
        println!(
            "  Extra sections: {}",
            soul.extra_sections
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

async fn push_personality(config: &CliConfig, creator_id: &str) -> Result<()> {
    let (body, _) = read_soul(config, creator_id).await?;
    let soul = SoulDocument::parse(&body).map_err(soul_error)?;
    let personality = soul.personality.unwrap_or_default();

    let record = with_direct_core(config, async |core, principal| {
        core.push_creator_personality(principal, &personality)
            .await
            .map_err(map_core_error)
    })
    .await?;
    let (memory_id, kind) = memory_identity(&record)
        .unwrap_or_else(|| (String::new(), "personality_core".to_string()));

    println!("Personality pushed to long-term memory for creator '{creator_id}'.");
    println!("  Memory ID: {memory_id}");
    println!("  Kind: {kind}");
    Ok(())
}

async fn refresh_experience(config: &CliConfig, creator_id: &str) -> Result<()> {
    let (markdown, processed) = with_direct_core(config, async |core, principal| {
        core.refresh_creator_experience(principal)
            .await
            .map_err(map_core_error)
    })
    .await?;

    println!("Experience refreshed for creator '{creator_id}'.");
    println!("  Memories processed: {processed}");
    println!("  Method: deterministic (no LLM)");
    if markdown.is_empty() {
        println!("  Note: no experience-kind memories found; Experience section cleared.");
    } else {
        println!("  Warning: hand-edits under ## Experience are overwritten on refresh.");
    }
    Ok(())
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
        let _ = SoulCommand::PushPersonality;
        let _ = SoulCommand::RefreshExperience;
    }
}
