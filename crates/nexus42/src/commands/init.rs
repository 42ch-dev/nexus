//! Init Command — Initialize a Nexus workspace (creative tree + ADR-014 operational layout)

use crate::config::{
    find_workspace_root, nexus_home, workspace_config_path, workspace_nexus_dir, CliConfig,
    DEFAULT_WORKSPACE_SLUG,
};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum InitCommand {
    /// Initialize creative workspace + operational registration under ~/.nexus42/creators/...
    #[command(name = "workspace")]
    Workspace {
        /// Workspace display name (defaults to current directory name)
        name: Option<String>,
        /// Creator id for operational paths (default: local)
        #[arg(long)]
        creator_id: Option<String>,
        /// Operational workspace slug (default: default)
        #[arg(long)]
        workspace_slug: Option<String>,
        /// Creative root directory (default: ~/Documents/nexus/<creator_id>/<workspace_slug>)
        #[arg(long)]
        creative_root: Option<PathBuf>,
    },
}

#[derive(serde::Serialize)]
struct WorkspaceMeta {
    schema_version: u32,
    creator_id: String,
    workspace_slug: String,
    local_root: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    created_at: String,
}

/// Initialize a Nexus workspace
pub async fn run(cmd: InitCommand) -> Result<()> {
    match cmd {
        InitCommand::Workspace {
            name,
            creator_id,
            workspace_slug,
            creative_root,
        } => init_workspace(name, creator_id, workspace_slug, creative_root).await,
    }
}

pub(crate) fn default_creative_root(creator_id: &str, workspace_slug: &str) -> Result<PathBuf> {
    let docs = dirs::document_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Documents")))
        .ok_or_else(|| CliError::Other("Cannot resolve Documents directory".into()))?;
    Ok(docs.join("nexus").join(creator_id).join(workspace_slug))
}

pub(crate) fn validate_slug(label: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
    {
        return Err(CliError::Other(format!(
            "Invalid {} {:?} (must be a single path segment)",
            label, value
        )));
    }
    Ok(())
}

/// Writes creative tree, `meta.json`, and initializes workspace `state.db` (ADR-014).
pub(crate) async fn materialize_adr014_workspace(
    user_home: &std::path::Path,
    creator_id: &str,
    workspace_slug: &str,
    creative_root: &std::path::Path,
    workspace_display_name: &str,
) -> Result<std::path::PathBuf> {
    let nexus_dir = workspace_nexus_dir(creative_root);
    let stories_dir = creative_root.join("Stories");
    let references_dir = creative_root.join("References");

    std::fs::create_dir_all(&nexus_dir)?;
    std::fs::create_dir_all(&stories_dir)?;
    std::fs::create_dir_all(&references_dir)?;

    let workspace_config = serde_json::json!({
        "name": workspace_display_name,
        "version": 1,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "creator_id": creator_id,
        "workspace_slug": workspace_slug,
    });

    let config_path = workspace_config_path(creative_root);
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&workspace_config)?,
    )?;

    let gitignore_content =
        "# Nexus local state (do not commit)\n*.db\n*.db-wal\n*.db-shm\nstate.db\n";
    std::fs::write(nexus_dir.join(".gitignore"), gitignore_content)?;

    let op_dir = crate::paths::operational_workspace_dir(user_home, creator_id, workspace_slug);
    std::fs::create_dir_all(&op_dir)?;

    let op_meta = op_dir.join("meta.json");
    let meta = WorkspaceMeta {
        schema_version: 1,
        creator_id: creator_id.to_string(),
        workspace_slug: workspace_slug.to_string(),
        local_root: creative_root.to_path_buf(),
        workspace_id: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    std::fs::write(op_meta, serde_json::to_string_pretty(&meta)?)?;

    let db_path = crate::paths::state_db_path(user_home, creator_id, workspace_slug);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Use async sqlx pool for DB initialization
    crate::db::Schema::init(&db_path).await?;
    // Pool is dropped here (short-lived for CLI)

    Ok(db_path)
}

pub(crate) fn persist_cli_workspace_selection(
    creative_root: PathBuf,
    creator_id: String,
    workspace_slug: String,
) -> Result<()> {
    let mut config = CliConfig::load()?;
    config.workspace_path = Some(creative_root);
    config.active_creator_id = Some(creator_id.clone());
    config
        .active_workspace_slug_by_creator
        .insert(creator_id, workspace_slug);
    config.save()?;
    Ok(())
}

/// Create workspace structure
async fn init_workspace(
    name: Option<String>,
    creator_id: Option<String>,
    workspace_slug: Option<String>,
    creative_root_arg: Option<PathBuf>,
) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let user_home = dirs::home_dir()
        .ok_or_else(|| CliError::Other("Cannot determine home directory".into()))?;

    let creator_id = creator_id.unwrap_or_else(|| "local".to_string());
    let workspace_slug = workspace_slug.unwrap_or_else(|| DEFAULT_WORKSPACE_SLUG.to_string());
    validate_slug("creator_id", &creator_id)?;
    validate_slug("workspace_slug", &workspace_slug)?;

    let op_meta = crate::paths::operational_workspace_dir(&user_home, &creator_id, &workspace_slug)
        .join("meta.json");
    if op_meta.exists() {
        println!(
            "Workspace already registered for creator {} / {}.",
            creator_id, workspace_slug
        );
        return Ok(());
    }

    if find_workspace_root().is_some() {
        println!("Workspace already initialized in this directory tree.");
        return Ok(());
    }

    let creative_root = match creative_root_arg {
        Some(p) => {
            if p.is_absolute() {
                p
            } else {
                current_dir.join(p)
            }
        }
        None => default_creative_root(&creator_id, &workspace_slug)?,
    };

    let workspace_name = name.unwrap_or_else(|| {
        creative_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string())
    });

    let db_path = materialize_adr014_workspace(
        &user_home,
        &creator_id,
        &workspace_slug,
        &creative_root,
        &workspace_name,
    )
    .await?;

    persist_cli_workspace_selection(
        creative_root.clone(),
        creator_id.clone(),
        workspace_slug.clone(),
    )?;

    let nh = nexus_home()?;
    std::fs::create_dir_all(&nh)?;

    let op_dir = crate::paths::operational_workspace_dir(&user_home, &creator_id, &workspace_slug);

    println!("✓ Workspace initialized: {}", workspace_name);
    println!("  Creative root: {}", creative_root.display());
    println!("  Operational: {}", op_dir.display());
    println!("  state.db: {}", db_path.display());
    println!("  Stories/   — manuscript files");
    println!("  References/ — research sources");
    println!("  .nexus42/  — workspace configuration (creative root)");
    println!();
    println!("Next steps:");
    println!("  nexus42 auth login    — authenticate with the platform");
    println!("  nexus42 creator register — create a Creator entity");

    Ok(())
}
