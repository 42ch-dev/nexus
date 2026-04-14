//! Context Command — `nexus42 context assemble` and `nexus42 context assemble-local`

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::context::client::ContextClient;
use crate::context::types::ContextAssembleRequestV1;
use crate::context::types::{error_code, error_message, is_error, MemoryKind};
use crate::errors::Result;
use clap::Subcommand;
use nexus_domain::runtime_guard;

/// Validate WorldId format: must start with 'wld_' followed by alphanumeric characters
pub fn validate_world_id(s: &str) -> std::result::Result<String, String> {
    // Check prefix
    if !s.starts_with("wld_") {
        return Err(format!(
            "WorldId must start with 'wld_' prefix (got '{}')",
            s
        ));
    }

    // Check that there's content after prefix
    let suffix = &s[4..]; // Skip "wld_" prefix (4 chars)
    if suffix.is_empty() {
        return Err("WorldId must have alphanumeric characters after 'wld_' prefix".to_string());
    }

    // Check that suffix contains only alphanumeric characters
    if !suffix.chars().all(|c| c.is_alphanumeric()) {
        return Err(format!(
            "WorldId must contain only alphanumeric characters after 'wld_' prefix (got '{}')",
            suffix
        ));
    }

    // Return the validated string
    Ok(s.to_string())
}

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Assemble context for a world via the Local API
    Assemble {
        /// World ID (required for context assembly, format: wld_[a-zA-Z0-9]+)
        #[arg(long, value_parser = validate_world_id)]
        world_id: String,

        /// Workspace ID (defaults to current workspace)
        #[arg(long)]
        workspace_id: Option<String>,

        /// Creator ID (defaults to active creator)
        #[arg(long)]
        creator_id: Option<String>,

        /// Include memory items in assembled context
        #[arg(long, default_value_t = true)]
        include_memory: bool,

        /// Include timeline events in assembled context
        #[arg(long, default_value_t = true)]
        include_timeline: bool,

        /// Include story summaries in assembled context
        #[arg(long, default_value_t = true)]
        include_story_summaries: bool,

        /// Maximum number of recent timeline events (null = platform default)
        #[arg(long)]
        max_timeline_events: Option<u64>,

        /// Maximum number of story summaries (null = platform default)
        #[arg(long)]
        max_story_summaries: Option<u64>,

        /// Maximum file size in bytes for summary generation (null = no limit)
        #[arg(long)]
        max_file_size: Option<u64>,

        /// Output file path (default: stdout as JSON)
        #[arg(long)]
        output_file: Option<String>,
    },

    /// Run Stage-0 local context assembly and output result (local-only)
    AssembleLocal {
        /// Max tokens for context (optional budget)
        #[arg(long)]
        max_tokens: Option<usize>,

        /// Include fragment keywords section
        #[arg(long, default_value_t = true)]
        include_fragments: bool,
    },
}

/// Run context command
pub async fn run(cmd: ContextCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        ContextCommand::Assemble {
            world_id,
            workspace_id,
            creator_id,
            include_memory,
            include_timeline,
            include_story_summaries,
            max_timeline_events,
            max_story_summaries,
            max_file_size,
            output_file,
        } => {
            // Guard: context assemble requires platform (ADR-017 D5)
            runtime_guard::require_platform(&config.runtime_mode(), "context assemble")?;

            // Resolve workspace_id and creator_id from config if not provided
            let workspace_id = workspace_id.unwrap_or_else(|| {
                config
                    .workspace_path
                    .as_ref()
                    .map(|_| "wrk_current".to_string())
                    .unwrap_or_else(|| "wrk_unknown".to_string())
            });

            let creator_id = creator_id.unwrap_or_else(|| {
                config
                    .active_creator_id
                    .clone()
                    .unwrap_or_else(|| "ctr_unknown".to_string())
            });

            // Build the request
            let request = ContextAssembleRequestV1 {
                request_id: format!("req_{}", uuid::Uuid::new_v4().simple()),
                workspace_id,
                creator_id,
                world_id,
                include_memory: Some(include_memory),
                include_timeline: Some(include_timeline),
                include_story_summaries: Some(include_story_summaries),
                memory_kinds: Some(vec![
                    MemoryKind::StorySummary.to_string(),
                    MemoryKind::ResearchMaterial.to_string(),
                    MemoryKind::ReviewNote.to_string(),
                ]),
                max_timeline_events: max_timeline_events.map(|v| v as i64),
                max_story_summaries: max_story_summaries.map(|v| v as i64),
            };

            // Note: max_file_size is not yet passed to the daemon API
            // It will be used when SummaryGenerator is integrated into the context assembly workflow
            // For now, suppress unused warning
            let _ = max_file_size;

            // Create daemon client and context client
            let daemon = DaemonClient::from_config(config);
            let client = ContextClient::new(daemon);

            // Call the Local API
            let response = client.assemble(&request).await?;

            // Handle error responses
            if is_error(&response) {
                let ec = error_code(&response).unwrap_or("unknown");
                let em = error_message(&response).unwrap_or("No details available");
                eprintln!("Error: Context assembly failed ({})", ec);
                eprintln!("  {}", em);
                if ec == "auth_expired" {
                    eprintln!("  Run `nexus42 auth login` to re-authenticate.");
                } else if ec == "world_not_found" {
                    eprintln!("  Check the world ID and ensure the world exists on the platform.");
                } else if ec == "platform_unavailable" {
                    eprintln!("  The platform may be temporarily unavailable. Try again later.");
                }
                std::process::exit(1);
            }

            // Output the response
            let output_json = serde_json::to_string_pretty(&response)?;
            match output_file {
                Some(path) => {
                    std::fs::write(&path, &output_json)?;
                    eprintln!("Context assembly written to {}", path);
                }
                None => {
                    println!("{}", output_json);
                }
            }

            Ok(())
        }
        ContextCommand::AssembleLocal {
            max_tokens,
            include_fragments,
        } => assemble_local(config, max_tokens, include_fragments),
    }
}

/// Stage-0 local context assembly (spec §9, §9.2).
///
/// Assembles context from SOUL.md, long-term memories, and fragment keywords
/// without requiring platform connectivity or the daemon.
fn assemble_local(
    config: &CliConfig,
    max_tokens: Option<usize>,
    include_fragments: bool,
) -> Result<()> {
    let creator_id = config.active_creator_id.as_deref().ok_or_else(|| {
        crate::errors::CliError::Other(
            "No active creator set. Run `nexus42 identity use <id>` first.".to_string(),
        )
    })?;

    let home = crate::config::user_home_dir()?;

    // 1. Load SOUL.md
    let soul = nexus_domain::soul_io::load(&home, creator_id)?;

    // 2. List long-term memories (skip personality_core — already in SOUL personality)
    let slugs = nexus_domain::memory_io::list_memories(&home, creator_id)?;
    let mut long_term_memories = Vec::new();
    for slug in &slugs {
        if let Ok(mem) = nexus_domain::memory_io::load_memory(&home, creator_id, slug) {
            // Skip personality_core memories (already in SOUL personality section)
            if mem.frontmatter.memory_kind == "personality_core" {
                continue;
            }
            long_term_memories.push(mem);
        }
    }

    // 3. Build fragment keywords (best-effort from daemon, optional)
    let fragment_keywords = if include_fragments {
        collect_fragment_keywords(config)
    } else {
        Vec::new()
    };

    // 4. Build Stage0Assembly
    let assembly = nexus_domain::Stage0Assembly {
        personality: soul.personality.clone().unwrap_or_default(),
        experience: soul.experience.clone().unwrap_or_default(),
        long_term_memories,
        fragment_keywords,
        system_prefix: String::new(),
        user_prompt: String::new(),
        max_tokens,
    };

    // 5. Assemble
    let output = if max_tokens.is_some() {
        assembly.assemble_with_truncation()
    } else {
        assembly.assemble()
    };

    println!("{}", output);
    Ok(())
}

/// Best-effort collection of fragment keywords from the daemon.
/// Returns empty vec if daemon is unavailable.
fn collect_fragment_keywords(config: &CliConfig) -> Vec<String> {
    // Try to fetch fragment keywords from daemon API.
    // This is best-effort: if daemon is down, return empty.
    match try_fetch_fragment_keywords(config) {
        Ok(keywords) => keywords,
        Err(_) => Vec::new(),
    }
}

/// Attempt to fetch fragment keywords from the daemon.
fn try_fetch_fragment_keywords(config: &CliConfig) -> std::result::Result<Vec<String>, ()> {
    // The daemon API is async but we call it synchronously with a minimal runtime.
    // For local-only mode, we just return empty — fragments require the daemon.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| ())?;

    rt.block_on(async {
        let url = format!("{}/v1/local/memory/fragments", config.daemon_url);

        let response = reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
            .map_err(|_| ())?;

        if !response.status().is_success() {
            return Err(());
        }

        let data: serde_json::Value = response.json().await.map_err(|_| ())?;

        // Extract keywords from fragment records
        let mut keywords = Vec::new();
        if let Some(fragments) = data.get("fragments").and_then(|v| v.as_array()) {
            for fragment in fragments {
                if let Some(kws) = fragment.get("keywords").and_then(|v| v.as_array()) {
                    for kw in kws {
                        if let Some(s) = kw.as_str() {
                            keywords.push(s.to_string());
                        }
                    }
                }
            }
        }

        Ok(keywords)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test valid WorldId formats
    #[test]
    fn validate_world_id_accepts_valid_formats() {
        // Valid: starts with wld_ followed by alphanumeric
        assert!(validate_world_id("wld_abc123").is_ok());
        assert!(validate_world_id("wld_test").is_ok());
        assert!(validate_world_id("wld_ABCDEF123456").is_ok());
        assert!(validate_world_id("wld_1").is_ok());
    }

    /// Test invalid WorldId formats - missing prefix
    #[test]
    fn validate_world_id_rejects_missing_prefix() {
        let result = validate_world_id("abc123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with 'wld_'"));
    }

    /// Test invalid WorldId formats - wrong prefix
    #[test]
    fn validate_world_id_rejects_wrong_prefix() {
        let result = validate_world_id("world_123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with 'wld_'"));
    }

    /// Test invalid WorldId formats - empty
    #[test]
    fn validate_world_id_rejects_empty() {
        let result = validate_world_id("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with 'wld_'"));
    }

    /// Test invalid WorldId formats - special characters
    #[test]
    fn validate_world_id_rejects_special_characters() {
        let result = validate_world_id("wld_test-123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("alphanumeric characters"));

        let result = validate_world_id("wld_test@123");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("alphanumeric characters"));
    }

    /// Test invalid WorldId formats - only prefix
    #[test]
    fn validate_world_id_rejects_only_prefix() {
        let result = validate_world_id("wld_");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("alphanumeric characters"));
    }

    /// Test that AssembleLocal variant exists
    #[test]
    fn context_command_assemble_local_exists() {
        let _cmd = ContextCommand::AssembleLocal {
            max_tokens: Some(1000),
            include_fragments: true,
        };
        let _cmd = ContextCommand::AssembleLocal {
            max_tokens: None,
            include_fragments: false,
        };
    }
}
