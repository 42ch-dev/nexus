//! Nexus CLI Configuration

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default nexus42 home directory name
const NEXUS_DIR: &str = ".nexus42";

/// Default daemon port
pub const DAEMON_PORT: u16 = 8420;

/// CLI configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CliConfig {
    /// Active workspace path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<PathBuf>,

    /// Currently active creator ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_creator_id: Option<String>,

    /// Platform API base URL
    #[serde(default = "default_platform_url")]
    pub platform_url: String,

    /// Daemon local API base URL
    #[serde(default = "default_daemon_url")]
    pub daemon_url: String,
}

fn default_platform_url() -> String {
    "https://api.nexus42.io".to_string()
}

fn default_daemon_url() -> String {
    format!("http://127.0.0.1:{DAEMON_PORT}")
}

impl CliConfig {
    /// Load configuration from the standard location
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::config_path()?;
        if !config_path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&config_path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save configuration to the standard location
    pub fn save(&self) -> anyhow::Result<()> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    /// Path to the configuration file
    fn config_path() -> anyhow::Result<PathBuf> {
        Ok(nexus_home()?.join("config.json"))
    }
}

/// Get the nexus42 home directory (`$HOME/.nexus42`)
pub fn nexus_home() -> anyhow::Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
    Ok(home.join(NEXUS_DIR))
}

/// Get the path to the local SQLite database
pub fn state_db_path() -> anyhow::Result<PathBuf> {
    Ok(nexus_home()?.join("state.db"))
}

/// Get the path to the auth storage file
pub fn auth_store_path() -> anyhow::Result<PathBuf> {
    Ok(nexus_home()?.join("auth.json"))
}

/// Check if the current directory (or any parent) contains a workspace
pub fn find_workspace_root() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        let nexus_dir = current.join(NEXUS_DIR);
        if nexus_dir.is_dir() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Get the workspace directory path for a given root
pub fn workspace_nexus_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(NEXUS_DIR)
}

/// Get workspace config file path
pub fn workspace_config_path(workspace_root: &Path) -> PathBuf {
    workspace_nexus_dir(workspace_root).join("workspace.json")
}
