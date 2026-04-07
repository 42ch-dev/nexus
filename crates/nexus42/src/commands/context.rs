//! Context Command — `nexus42 context assemble`

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::context::client::ContextClient;
use crate::context::types::ContextAssembleRequestV1;
use crate::context::types::{error_code, error_message, is_error, MemoryKind};
use crate::errors::Result;
use clap::Subcommand;

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

        /// Output file path (default: stdout as JSON)
        #[arg(long)]
        output_file: Option<String>,
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
            output_file,
        } => {
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
    }
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
}
