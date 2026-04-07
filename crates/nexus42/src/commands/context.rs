//! Context Command — `nexus42 context assemble`

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::context::client::ContextClient;
use crate::context::types::ContextAssembleRequestV1;
use crate::context::types::{error_code, error_message, is_error, MemoryKind};
use crate::errors::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Assemble context for a world via the Local API
    Assemble {
        /// World ID (required for context assembly)
        #[arg(long)]
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
