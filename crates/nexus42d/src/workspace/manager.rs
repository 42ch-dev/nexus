//! Workspace Manager — higher-level workspace operations

use anyhow::Result;

/// Initialize workspace directory structure
pub fn create_workspace_structure(path: &str) -> Result<()> {
    let base = std::path::Path::new(path);
    let nexus_dir = base.join(".nexus42");

    std::fs::create_dir_all(&nexus_dir)?;
    std::fs::create_dir_all(base.join("Stories"))?;
    std::fs::create_dir_all(base.join("References"))?;

    // Create .gitignore
    let gitignore = nexus_dir.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(
            &gitignore,
            "# Nexus local state\n*.db\n*.db-wal\n*.db-shm\n",
        )?;
    }

    // Create workspace config
    let config_path = nexus_dir.join("workspace.json");
    if !config_path.exists() {
        let name = base
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string());

        let config = serde_json::json!({
            "name": name,
            "version": 1,
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    }

    Ok(())
}
