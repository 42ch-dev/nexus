//! Nexus CLI Configuration

use nexus_domain::runtime_mode::DomainRuntimeMode;
use nexus_domain::DegradationSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

    /// Last-selected operational workspace slug per creator (path segment under ADR-014 layout).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub active_workspace_slug_by_creator: HashMap<String, String>,

    /// Platform API base URL
    #[serde(default = "default_platform_url")]
    pub platform_url: String,

    /// Daemon local API base URL
    #[serde(default = "default_daemon_url")]
    pub daemon_url: String,

    /// Runtime mode controlling platform dependency behavior.
    /// Defaults to `local_only` for V1.2 MVP (ADR-017).
    #[serde(default = "default_runtime_mode")]
    pub runtime_mode: DomainRuntimeMode,

    /// Persisted degradation guard state (inline in config.json for V1.2 MVP).
    /// Written by the daemon/runtime when degradation occurs; read-only for CLI display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_snapshot: Option<DegradationSnapshot>,
}

fn default_platform_url() -> String {
    "https://api.nexus42.io".to_string()
}

fn default_daemon_url() -> String {
    format!("http://127.0.0.1:{DAEMON_PORT}")
}

fn default_runtime_mode() -> DomainRuntimeMode {
    DomainRuntimeMode::default()
}

/// Default workspace slug when none is stored for a creator.
pub const DEFAULT_WORKSPACE_SLUG: &str = "default";

/// Valid configuration keys for `nexus42 config` commands.
/// MVP: only top-level fields with simple types; nested keys are not supported.
pub const VALID_CONFIG_KEYS: &[&str] = &[
    "workspace_path",
    "active_creator_id",
    "platform_url",
    "daemon_url",
    "runtime_mode",
];

impl CliConfig {
    /// Load configuration from the standard location
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::config_path()?;
        if !config_path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&config_path)?;
        // Handle empty files by returning default config
        if content.trim().is_empty() {
            return Ok(Self::default());
        }
        match serde_json::from_str::<CliConfig>(&content) {
            Ok(cfg) => Ok(cfg),
            Err(e) => {
                // Backup corrupted file and re-initialize with defaults
                tracing::warn!(
                    "Config file corrupted, backing up and re-initializing: {}",
                    e
                );
                let bak = config_path.with_extension("json.bak");
                if let Err(rename_err) = std::fs::rename(&config_path, &bak) {
                    tracing::error!("Failed to backup corrupted config: {}", rename_err);
                }
                Ok(Self::default())
            }
        }
    }

    /// Current runtime mode (defaults to `local_only` for V1.2 MVP).
    pub fn runtime_mode(&self) -> DomainRuntimeMode {
        self.runtime_mode
    }

    /// Persisted degradation guard snapshot, if available.
    pub fn degradation_snapshot(&self) -> Option<&DegradationSnapshot> {
        self.degradation_snapshot.as_ref()
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

    /// Public path to the configuration file (for `config path` command)
    pub fn path() -> anyhow::Result<PathBuf> {
        Self::config_path()
    }

    /// Operational workspace slug for `creator_id` (falls back to [`DEFAULT_WORKSPACE_SLUG`]).
    pub fn workspace_slug_for_creator(&self, creator_id: &str) -> &str {
        self.active_workspace_slug_by_creator
            .get(creator_id)
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_WORKSPACE_SLUG)
    }

    /// Get a configuration value by key name.
    /// Returns the value as a JSON string representation.
    /// MVP: only supports top-level fields listed in [`VALID_CONFIG_KEYS`].
    /// For fields with defaults (platform_url, daemon_url), returns the default if empty.
    pub fn get(&self, key: &str) -> anyhow::Result<String> {
        Self::validate_key(key)?;
        match key {
            "workspace_path" => Ok(self
                .workspace_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()),
            "active_creator_id" => Ok(self.active_creator_id.clone().unwrap_or_default()),
            "platform_url" => Ok(if self.platform_url.is_empty() {
                default_platform_url()
            } else {
                self.platform_url.clone()
            }),
            "daemon_url" => Ok(if self.daemon_url.is_empty() {
                default_daemon_url()
            } else {
                self.daemon_url.clone()
            }),
            "runtime_mode" => Ok(self.runtime_mode.to_string()),
            _ => Err(anyhow::anyhow!("Unsupported config key: {}", key)),
        }
    }

    /// Set a configuration value by key name.
    /// MVP: only supports top-level fields listed in [`VALID_CONFIG_KEYS`].
    pub fn set(&mut self, key: &str, value: &str) -> anyhow::Result<()> {
        Self::validate_key(key)?;
        match key {
            "workspace_path" => {
                self.workspace_path = if value.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(value))
                };
            }
            "active_creator_id" => {
                self.active_creator_id = if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                };
            }
            "platform_url" => {
                self.platform_url = value.to_string();
            }
            "daemon_url" => {
                self.daemon_url = value.to_string();
            }
            "runtime_mode" => {
                self.runtime_mode = DomainRuntimeMode::parse(value)
                    .map_err(|e| anyhow::anyhow!("Invalid runtime_mode '{}': {}", value, e))?;
            }
            _ => Err(anyhow::anyhow!("Unsupported config key: {}", key))?,
        }
        Ok(())
    }

    /// Unset (remove) a configuration key.
    /// MVP: only supports top-level fields listed in [`VALID_CONFIG_KEYS`].
    /// For string fields, clears to empty/default.
    pub fn unset(&mut self, key: &str) -> anyhow::Result<()> {
        Self::validate_key(key)?;
        match key {
            "workspace_path" => {
                self.workspace_path = None;
            }
            "active_creator_id" => {
                self.active_creator_id = None;
            }
            "platform_url" => {
                self.platform_url = default_platform_url();
            }
            "daemon_url" => {
                self.daemon_url = default_daemon_url();
            }
            "runtime_mode" => {
                self.runtime_mode = default_runtime_mode();
            }
            _ => Err(anyhow::anyhow!("Unsupported config key: {}", key))?,
        }
        Ok(())
    }

    /// Validate that a key is in the allowed set.
    fn validate_key(key: &str) -> anyhow::Result<()> {
        if !VALID_CONFIG_KEYS.contains(&key) {
            Err(anyhow::anyhow!(
                "Invalid config key '{}'. Valid keys: {}",
                key,
                VALID_CONFIG_KEYS.join(", ")
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_slug_defaults_when_unset() {
        let c = CliConfig::default();
        assert_eq!(
            c.workspace_slug_for_creator("ctr_any"),
            DEFAULT_WORKSPACE_SLUG
        );
    }

    #[test]
    fn workspace_slug_roundtrips_via_json() {
        let mut c = CliConfig::default();
        c.active_workspace_slug_by_creator
            .insert("ctr_a".into(), "staging".into());
        assert_eq!(c.workspace_slug_for_creator("ctr_a"), "staging");
        let json = serde_json::to_string(&c).unwrap();
        let back: CliConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.workspace_slug_for_creator("ctr_a"), "staging");
    }

    #[test]
    fn runtime_mode_defaults_to_local_only() {
        let c = CliConfig::default();
        assert!(c.runtime_mode().is_local_only());
    }

    #[test]
    fn runtime_mode_roundtrips_via_json() {
        let mut c = CliConfig::default();
        c.runtime_mode = DomainRuntimeMode::parse("cloud_enhanced").unwrap();
        let json = serde_json::to_string(&c).unwrap();
        let back: CliConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.runtime_mode().to_string(), "cloud_enhanced");
    }

    #[test]
    fn runtime_mode_parses_from_config_file() {
        let json = r#"{"runtime_mode": "local_first"}"#;
        let c: CliConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.runtime_mode().to_string(), "local_first");
    }

    #[test]
    fn degradation_snapshot_defaults_to_none() {
        let c = CliConfig::default();
        assert!(c.degradation_snapshot().is_none());
    }

    #[test]
    fn degradation_snapshot_roundtrips_via_json() {
        use nexus_domain::degradation::DegradationState;
        use nexus_domain::HealthCheckSnapshot;

        let mut c = CliConfig::default();
        c.degradation_snapshot = Some(DegradationSnapshot {
            state: DegradationState::DegradedLevel1,
            failure_count: 3,
            last_health_check: Some(HealthCheckSnapshot {
                is_healthy: false,
                checked_at: "2026-04-15T10:30:00Z".to_string(),
            }),
        });

        let json = serde_json::to_string(&c).unwrap();
        let back: CliConfig = serde_json::from_str(&json).unwrap();

        let snap = back.degradation_snapshot().unwrap();
        assert_eq!(snap.state, DegradationState::DegradedLevel1);
        assert_eq!(snap.failure_count, 3);
        let hc = snap.last_health_check.as_ref().unwrap();
        assert!(!hc.is_healthy);
        assert_eq!(hc.checked_at, "2026-04-15T10:30:00Z");
    }

    #[test]
    fn degradation_snapshot_absent_key_loads_as_none() {
        let json = r#"{"runtime_mode": "local_only"}"#;
        let c: CliConfig = serde_json::from_str(json).unwrap();
        assert!(c.degradation_snapshot().is_none());
    }

    #[test]
    fn get_valid_key_returns_value() {
        let c = CliConfig::default();
        assert_eq!(c.get("runtime_mode").unwrap(), "local_only");
        assert_eq!(c.get("platform_url").unwrap(), "https://api.nexus42.io");
        assert_eq!(c.get("daemon_url").unwrap(), "http://127.0.0.1:8420");
    }

    #[test]
    fn get_optional_field_returns_empty_when_unset() {
        let c = CliConfig::default();
        assert_eq!(c.get("workspace_path").unwrap(), "");
        assert_eq!(c.get("active_creator_id").unwrap(), "");
    }

    #[test]
    fn get_optional_field_returns_value_when_set() {
        let mut c = CliConfig::default();
        c.workspace_path = Some(PathBuf::from("/test/path"));
        c.active_creator_id = Some("ctr_test".to_string());
        assert_eq!(c.get("workspace_path").unwrap(), "/test/path");
        assert_eq!(c.get("active_creator_id").unwrap(), "ctr_test");
    }

    #[test]
    fn get_invalid_key_returns_error() {
        let c = CliConfig::default();
        let err = c.get("invalid_key").unwrap_err();
        assert!(err.to_string().contains("Invalid config key"));
        assert!(err.to_string().contains("invalid_key"));
    }

    #[test]
    fn set_runtime_mode_updates_value() {
        let mut c = CliConfig::default();
        c.set("runtime_mode", "cloud_enhanced").unwrap();
        assert_eq!(c.runtime_mode.to_string(), "cloud_enhanced");
    }

    #[test]
    fn set_invalid_runtime_mode_returns_error() {
        let mut c = CliConfig::default();
        let err = c.set("runtime_mode", "invalid_mode").unwrap_err();
        assert!(err.to_string().contains("Invalid runtime_mode"));
    }

    #[test]
    fn set_string_field_updates_value() {
        let mut c = CliConfig::default();
        c.set("platform_url", "https://custom.api.io").unwrap();
        assert_eq!(c.platform_url, "https://custom.api.io");
        c.set("active_creator_id", "ctr_new").unwrap();
        assert_eq!(c.active_creator_id, Some("ctr_new".to_string()));
    }

    #[test]
    fn set_optional_field_to_empty_clears_it() {
        let mut c = CliConfig::default();
        c.workspace_path = Some(PathBuf::from("/test"));
        c.set("workspace_path", "").unwrap();
        assert!(c.workspace_path.is_none());
        c.active_creator_id = Some("ctr_old".to_string());
        c.set("active_creator_id", "").unwrap();
        assert!(c.active_creator_id.is_none());
    }

    #[test]
    fn set_invalid_key_returns_error() {
        let mut c = CliConfig::default();
        let err = c.set("invalid_key", "value").unwrap_err();
        assert!(err.to_string().contains("Invalid config key"));
    }

    #[test]
    fn unset_optional_field_clears_it() {
        let mut c = CliConfig::default();
        c.workspace_path = Some(PathBuf::from("/test"));
        c.unset("workspace_path").unwrap();
        assert!(c.workspace_path.is_none());
        c.active_creator_id = Some("ctr_test".to_string());
        c.unset("active_creator_id").unwrap();
        assert!(c.active_creator_id.is_none());
    }

    #[test]
    fn unset_string_field_reverts_to_default() {
        let mut c = CliConfig::default();
        c.platform_url = "https://custom.api.io".to_string();
        c.unset("platform_url").unwrap();
        assert_eq!(c.platform_url, "https://api.nexus42.io");
        c.daemon_url = "http://custom:9999".to_string();
        c.unset("daemon_url").unwrap();
        assert_eq!(c.daemon_url, "http://127.0.0.1:8420");
    }

    #[test]
    fn unset_runtime_mode_reverts_to_default() {
        let mut c = CliConfig::default();
        c.runtime_mode = DomainRuntimeMode::parse("cloud_enhanced").unwrap();
        c.unset("runtime_mode").unwrap();
        assert!(c.runtime_mode.is_local_only());
    }

    #[test]
    fn unset_invalid_key_returns_error() {
        let mut c = CliConfig::default();
        let err = c.unset("invalid_key").unwrap_err();
        assert!(err.to_string().contains("Invalid config key"));
    }

    #[test]
    fn path_returns_config_json_path() {
        let path = CliConfig::path().unwrap();
        assert!(path.ends_with("config.json"));
        assert!(path.to_string_lossy().contains(".nexus42"));
    }

    #[test]
    fn valid_config_keys_are_documented() {
        assert!(VALID_CONFIG_KEYS.contains(&"runtime_mode"));
        assert!(VALID_CONFIG_KEYS.contains(&"platform_url"));
        assert!(VALID_CONFIG_KEYS.contains(&"daemon_url"));
        assert!(VALID_CONFIG_KEYS.contains(&"workspace_path"));
        assert!(VALID_CONFIG_KEYS.contains(&"active_creator_id"));
    }
}

/// Get the nexus42 home directory (`$HOME/.nexus42`)
pub fn nexus_home() -> anyhow::Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
    Ok(home.join(NEXUS_DIR))
}

/// User home directory (`$HOME`).
pub fn user_home_dir() -> anyhow::Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))
}

/// Resolve workspace `state.db` under ADR-014 (`creators/<creator_id>/workspaces/<slug>/state.db`).
pub fn resolve_state_db_path(config: &CliConfig) -> anyhow::Result<PathBuf> {
    let user_home = user_home_dir()?;
    let cid = config.active_creator_id.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "No active creator configured. Run `nexus42 init workspace` or `nexus42 creator use <id>`."
        )
    })?;
    let slug = config.workspace_slug_for_creator(cid);
    Ok(crate::paths::state_db_path(&user_home, cid, slug))
}

/// Load config and resolve the local SQLite database path.
pub fn state_db_path() -> anyhow::Result<PathBuf> {
    resolve_state_db_path(&CliConfig::load()?)
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
