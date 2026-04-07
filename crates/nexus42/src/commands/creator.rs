//! Creator Command Module
//!
//! Creator is a V1.0 first-class citizen (roadmap §3.1.1, §3.1.2).
//! Subcommands: register, status, use, list, pair, unpair, credentials rotate.

use crate::auth;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_contracts::Creator;

#[derive(Debug, Subcommand)]
pub enum CreatorCommand {
    /// Register a new Creator entity
    Register {
        /// Display name for the Creator
        name: String,
        /// Short description / persona summary
        #[arg(long)]
        summary: Option<String>,
    },

    /// Show current Creator status
    Status {
        /// Specific creator ID to check (default: active creator)
        creator_id: Option<String>,
    },

    /// Switch the active Creator
    Use {
        /// Creator ID or display name
        creator_ref: String,
    },

    /// List all registered Creators
    List,

    /// Initiate pairing flow with a Creator
    Pair {
        /// Creator ID to pair
        creator_id: String,
    },

    /// Remove pairing with a Creator
    Unpair {
        /// Creator ID to unpair
        creator_id: String,
    },

    /// Rotate Creator API credentials
    #[command(name = "credentials")]
    Credentials {
        #[command(subcommand)]
        action: CredentialsAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum CredentialsAction {
    /// Rotate the API key for the active or specified Creator
    Rotate {
        /// Creator ID (default: active creator)
        creator_id: Option<String>,
    },
}

/// Run creator command
pub async fn run(cmd: CreatorCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        CreatorCommand::Register { name, summary } => register_creator(config, name, summary).await,
        CreatorCommand::Status { creator_id } => creator_status(config, creator_id).await,
        CreatorCommand::Use { creator_ref } => use_creator(config, creator_ref).await,
        CreatorCommand::List => list_creators(config).await,
        CreatorCommand::Pair { creator_id } => pair_creator(config, creator_id).await,
        CreatorCommand::Unpair { creator_id } => unpair_creator(config, creator_id).await,
        CreatorCommand::Credentials { action } => match action {
            CredentialsAction::Rotate { creator_id } => {
                rotate_credentials(config, creator_id).await
            }
        },
    }
}

/// Register a new Creator entity
async fn register_creator(
    _config: &CliConfig,
    name: String,
    _summary: Option<String>,
) -> Result<()> {
    // Platform API integration not yet available
    println!("⚠ V1.0 skeleton: Creator registration requires platform API.");
    println!("  Name: {}", name);
    println!("  Run `nexus42 auth login` to authenticate first when platform integration lands.");
    Ok(())
}

/// Show Creator status
async fn creator_status(config: &CliConfig, creator_id: Option<String>) -> Result<()> {
    let id = creator_id.unwrap_or_else(|| {
        config
            .active_creator_id
            .clone()
            .unwrap_or_else(|| "none".to_string())
    });

    if id == "none" {
        println!("No active Creator set.");
        println!("Use: nexus42 creator use <creator-id>");
        return Ok(());
    }

    let store = crate::auth::AuthStore::load()?;

    // Try to get from local cache first
    println!("Creator: {}", id);

    if store.is_creator_authenticated(&id) {
        println!("  Auth: ✓ Token cached");
    } else {
        println!("  Auth: ✗ No cached token");
    }

    println!();
    println!("⚠ V1.0 skeleton: full status requires daemon + platform API.");

    Ok(())
}

/// Switch active Creator
async fn use_creator(_config: &CliConfig, creator_ref: String) -> Result<()> {
    let mut cli_config = CliConfig::load()?;
    cli_config.active_creator_id = Some(creator_ref.clone());
    cli_config.save()?;

    println!("✓ Active Creator set to: {}", creator_ref);
    Ok(())
}

/// List all registered Creators
async fn list_creators(_config: &CliConfig) -> Result<()> {
    // In V1.0, list from local cache
    // In production, also fetch from platform
    let config = CliConfig::load()?;

    println!("Registered Creators:");
    println!();

    if let Some(active_id) = &config.active_creator_id {
        println!("  {} (active)", active_id);
    }

    println!();
    println!("⚠ V1.0 skeleton: full list requires daemon + platform API.");

    Ok(())
}

/// Initiate pairing flow
async fn pair_creator(_config: &CliConfig, creator_id: String) -> Result<()> {
    // Platform API integration not yet available
    println!("⚠ V1.0 skeleton: Creator pairing requires platform API.");
    println!("  Creator: {}", creator_id);
    Ok(())
}

/// Remove pairing
async fn unpair_creator(_config: &CliConfig, creator_id: String) -> Result<()> {
    // Platform API integration not yet available
    println!("⚠ V1.0 skeleton: Creator unpairing requires platform API.");
    println!("  Creator: {}", creator_id);
    Ok(())
}

/// Rotate Creator credentials
async fn rotate_credentials(config: &CliConfig, creator_id: Option<String>) -> Result<()> {
    let id = creator_id.unwrap_or_else(|| {
        config
            .active_creator_id
            .clone()
            .ok_or(crate::errors::CliError::CreatorNotSelected)
            .unwrap_or_default()
    });

    auth::creator_auth::rotate_credentials(config, &id).await
}

/// Cache a Creator locally in SQLite
#[allow(dead_code)]
fn cache_creator_locally(creator: &Creator) -> Result<()> {
    use crate::config::state_db_path;
    let db_path = state_db_path()?;

    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = rusqlite::Connection::open(&db_path)?;
    crate::db::Schema::init(&conn)?;

    conn.execute(
        "INSERT OR REPLACE INTO creators (creator_id, display_name, status, cached_at, data)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            creator.creator_id,
            creator.display_name,
            creator.status,
            chrono::Utc::now().to_rfc3339(),
            serde_json::to_string(creator)?,
        ],
    )?;

    Ok(())
}
