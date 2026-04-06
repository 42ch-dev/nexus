//! Init Command — Initialize a Nexus workspace

use crate::config::{find_workspace_root, nexus_home, workspace_config_path, workspace_nexus_dir, CliConfig};
use crate::errors::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum InitCommand {
    /// Initialize a new Nexus workspace in the current directory
    #[command(name = "workspace")]
    Workspace {
        /// Workspace name
        name: Option<String>,
    },
}

/// Initialize a Nexus workspace
pub async fn run(cmd: InitCommand) -> Result<()> {
    match cmd {
        InitCommand::Workspace { name } => init_workspace(name).await,
    }
}

/// Create workspace structure
async fn init_workspace(name: Option<String>) -> Result<()> {
    let current_dir = std::env::current_dir()?;

    // Check if already initialized
    if find_workspace_root().is_some() {
        println!("Workspace already initialized in this directory tree.");
        return Ok(());
    }

    let workspace_name = name.unwrap_or_else(|| {
        current_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string())
    });

    // Create directory structure
    let nexus_dir = workspace_nexus_dir(&current_dir);
    let stories_dir = current_dir.join("Stories");
    let references_dir = current_dir.join("References");

    std::fs::create_dir_all(&nexus_dir)?;
    std::fs::create_dir_all(&stories_dir)?;
    std::fs::create_dir_all(&references_dir)?;

    // Create workspace config
    let workspace_config = serde_json::json!({
        "name": workspace_name,
        "version": 1,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let config_path = workspace_config_path(&current_dir);
    std::fs::write(&config_path, serde_json::to_string_pretty(&workspace_config)?)?;

    // Create .gitignore in nexus directory
    let gitignore_content = "# Nexus local state (do not commit)\n*.db\n*.db-wal\n*.db-shm\nstate.db\n";
    std::fs::write(nexus_dir.join(".gitignore"), gitignore_content)?;

    // Update global config with workspace path
    let mut config = CliConfig::load()?;
    config.workspace_path = Some(current_dir.clone());
    config.save()?;

    // Ensure nexus home directory exists
    let home = nexus_home()?;
    std::fs::create_dir_all(&home)?;

    println!("✓ Workspace initialized: {}", workspace_name);
    println!("  Directory: {}", current_dir.display());
    println!("  Stories/   — manuscript files");
    println!("  References/ — research sources");
    println!("  .nexus42/  — workspace configuration");
    println!();
    println!("Next steps:");
    println!("  nexus42 auth login    — authenticate with the platform");
    println!("  nexus42 creator register — create a Creator entity");

    Ok(())
}
