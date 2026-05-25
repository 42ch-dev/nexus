//! `creator world` subcommand — create worlds, add events, list worlds.
//!
//! Product write path for narrative worlds. Writes go through
//! `nexus_local_db::narrative_write`, NOT through the `NarrativeGateway` trait.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_narrative::NarrativeGateway;

/// World subcommands.
#[derive(Debug, Subcommand)]
pub enum WorldCommand {
    /// Create a new narrative world
    Create {
        /// World title
        #[arg(long)]
        title: String,
        /// URL-friendly slug (defaults to title-derived slug)
        #[arg(long)]
        slug: Option<String>,
        /// Visibility: private (default) or public
        #[arg(long, default_value = "private")]
        visibility: String,
        /// Time policy: `manual` (default) or `owner_driven`
        #[arg(long, default_value = "manual")]
        time_policy: String,
    },

    /// Add a timeline event to a world
    #[command(name = "event-add")]
    EventAdd {
        /// World ID (required, e.g. `wld_abc123`)
        #[arg(long)]
        world_id: String,
        /// Branch ID (defaults to world's root branch)
        #[arg(long)]
        branch_id: Option<String>,
        /// Event type (default: `story_advance`)
        #[arg(long, default_value = "story_advance")]
        event_type: String,
        /// Event title
        #[arg(long)]
        title: Option<String>,
        /// Event summary
        #[arg(long)]
        summary: Option<String>,
    },

    /// List all worlds in the active workspace
    List,
}

/// Run a world subcommand.
///
/// # Errors
///
/// Returns `CliError` if the database is unavailable, the active creator is
/// not set, or any write/query operation fails.
pub async fn run(cmd: WorldCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        WorldCommand::Create {
            title,
            slug,
            visibility,
            time_policy,
        } => run_create(config, &title, slug.as_deref(), &visibility, &time_policy).await,
        WorldCommand::EventAdd {
            world_id,
            branch_id,
            event_type,
            title,
            summary,
        } => {
            run_event_add(
                config,
                &world_id,
                branch_id.as_deref(),
                &event_type,
                title.as_deref(),
                summary.as_deref(),
            )
            .await
        }
        WorldCommand::List => run_list(config).await,
    }
}

/// Derive a slug from a title: lowercase, spaces → hyphens, strip non-alphanumeric.
fn slug_from_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == '_' {
                '-'
            } else {
                c
            }
        })
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// Open a DB pool for the active workspace.
async fn open_workspace_pool(config: &CliConfig) -> Result<sqlx::SqlitePool> {
    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;
    Ok(pool)
}

/// Get the active creator ID or error.
fn active_creator_id(config: &CliConfig) -> Result<String> {
    config
        .active_creator_id
        .clone()
        .ok_or(crate::errors::CliError::CreatorNotSelected)
}

/// Run `creator world create`.
async fn run_create(
    config: &CliConfig,
    title: &str,
    slug: Option<&str>,
    visibility: &str,
    time_policy: &str,
) -> Result<()> {
    let creator_id = active_creator_id(config)?;
    let pool = open_workspace_pool(config).await?;

    let slug = slug.map_or_else(|| slug_from_title(title), std::string::ToString::to_string);

    let result =
        nexus_local_db::create_world(&pool, &creator_id, title, &slug, visibility, time_policy)
            .await
            .map_err(|e| crate::errors::CliError::Other(format!("Failed to create world: {e}")))?;

    println!("✓ World created: {}", result.world_id);
    println!("  Title:     {title}");
    println!("  Slug:      {slug}");
    println!("  Branch:    {}", result.root_fork_branch_id);
    println!("  Created:   {}", result.created_at);
    Ok(())
}

/// Run `creator world event-add`.
async fn run_event_add(
    config: &CliConfig,
    world_id: &str,
    branch_id: Option<&str>,
    event_type: &str,
    title: Option<&str>,
    summary: Option<&str>,
) -> Result<()> {
    let pool = open_workspace_pool(config).await?;

    // If no branch_id specified, look up the world's root_fork_branch_id
    let branch_id_resolved = if let Some(bid) = branch_id {
        bid.to_string()
    } else {
        // SAFETY: SELECT against known narrative_worlds table schema
        sqlx::query_scalar(
            "SELECT root_fork_branch_id FROM narrative_worlds WHERE world_id = ?",
        )
        .bind(world_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| crate::errors::CliError::Other(format!("Failed to query world: {e}")))?
        .flatten()
        .ok_or_else(|| {
            crate::errors::CliError::Other(format!(
                "World '{world_id}' not found or has no root branch. Use --world-id with a valid world ID."
            ))
        })?
    };

    let result = nexus_local_db::append_event(
        &pool,
        world_id,
        &branch_id_resolved,
        event_type,
        title,
        summary,
    )
    .await
    .map_err(|e| crate::errors::CliError::Other(format!("Failed to append event: {e}")))?;

    println!("✓ Event added: {}", result.event_id);
    println!("  World:     {world_id}");
    println!("  Branch:    {branch_id_resolved}");
    println!("  Sequence:  {}", result.sequence_no);
    if let Some(t) = title {
        println!("  Title:     {t}");
    }
    if let Some(s) = summary {
        println!("  Summary:   {s}");
    }
    Ok(())
}

/// Run `creator world list`.
async fn run_list(config: &CliConfig) -> Result<()> {
    let pool = open_workspace_pool(config).await?;
    let gw = nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(pool);

    let worlds = gw
        .list_worlds()
        .await
        .map_err(|e| crate::errors::CliError::Other(format!("Failed to list worlds: {e}")))?;

    if worlds.is_empty() {
        println!("No worlds found in the active workspace.");
        return Ok(());
    }

    println!(
        "{:<40} {:<25} {:<12} CREATED_AT",
        "WORLD_ID", "TITLE", "STATUS"
    );
    for world in &worlds {
        println!(
            "{:<40} {:<25} {:<12} {}",
            world.world_id, world.title, world.status, world.created_at
        );
    }
    Ok(())
}
