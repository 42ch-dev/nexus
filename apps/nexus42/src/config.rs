//! Nexus CLI Configuration

use crate::domain::{DegradationSnapshot, DomainRuntimeMode};
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

    /// Persisted degradation guard state (inline in config.toml for V1.2 MVP).
    /// Written by the daemon/runtime when degradation occurs; read-only for CLI display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation_snapshot: Option<DegradationSnapshot>,

    /// Persistent machine identifier (UUID v4) for rate-limiting and device tracking.
    /// Not serialized to config.toml — resolved at startup from `~/.nexus42/device-id`.
    #[serde(skip)]
    pub device_id: String,
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
    /// Load configuration from the standard location.
    ///
    /// Migration: if `config.toml` does not exist but `config.json` does,
    /// the JSON file is read, converted to TOML, and the original is renamed
    /// to `config.json.migrated`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Nexus home directory cannot be resolved
    /// - File read operations fail
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::config_path()?;
        let nexus = nexus_home()?;

        // 1. Try loading config.toml
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            if content.trim().is_empty() {
                return Ok(Self::default());
            }
            return match toml::from_str::<Self>(&content) {
                Ok(cfg) => Ok(cfg),
                Err(e) => {
                    // Backup corrupted file and re-initialize with defaults
                    tracing::warn!(
                        "Config file corrupted, backing up and re-initializing: {}",
                        e
                    );
                    let bak = config_path.with_extension("toml.bak");
                    if let Err(rename_err) = std::fs::rename(&config_path, &bak) {
                        tracing::error!("Failed to backup corrupted config: {}", rename_err);
                    }
                    Ok(Self::default())
                }
            };
        }

        // 2. Migration: try loading legacy config.json
        let json_path = nexus.join("config.json");
        if json_path.exists() {
            let content = std::fs::read_to_string(&json_path)?;
            if content.trim().is_empty() {
                // Empty JSON file — just rename it and return defaults
                std::fs::rename(&json_path, nexus.join("config.json.migrated"))?;
                return Ok(Self::default());
            }
            match serde_json::from_str::<Self>(&content) {
                Ok(cfg) => {
                    // Write config.toml and rename legacy file
                    let toml_str = toml::to_string_pretty(&cfg)?;
                    if let Some(parent) = config_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&config_path, toml_str)?;
                    std::fs::rename(&json_path, nexus.join("config.json.migrated"))?;
                    tracing::info!("Migrated config.json → config.toml");
                    return Ok(cfg);
                }
                Err(e) => {
                    tracing::warn!(
                        "Legacy config.json corrupted, backing up and re-initializing: {}",
                        e
                    );
                    let bak = json_path.with_extension("json.bak");
                    if let Err(rename_err) = std::fs::rename(&json_path, &bak) {
                        tracing::error!("Failed to backup corrupted config: {}", rename_err);
                    }
                    return Ok(Self::default());
                }
            };
        }

        // 3. No config file — return defaults
        Ok(Self::default())
    }

    /// Current runtime mode (defaults to `local_only` for V1.2 MVP).
    #[must_use]
    pub const fn runtime_mode(&self) -> DomainRuntimeMode {
        self.runtime_mode
    }

    /// Persisted degradation guard snapshot, if available.
    #[must_use]
    pub const fn degradation_snapshot(&self) -> Option<&DegradationSnapshot> {
        self.degradation_snapshot.as_ref()
    }

    /// Save configuration to the standard location.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Config file path cannot be resolved
    /// - Directory creation fails
    /// - TOML serialization fails
    /// - File write operation fails
    pub fn save(&self) -> anyhow::Result<()> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    /// Path to the configuration file.
    ///
    /// # Errors
    ///
    /// Returns an error if the Nexus home directory cannot be resolved.
    fn config_path() -> anyhow::Result<PathBuf> {
        Ok(nexus_home()?.join("config.toml"))
    }

    /// Public path to the configuration file (for `config path` command).
    ///
    /// # Errors
    ///
    /// Returns an error if the Nexus home directory cannot be resolved.
    pub fn path() -> anyhow::Result<PathBuf> {
        Self::config_path()
    }

    /// Operational workspace slug for `creator_id` (falls back to [`DEFAULT_WORKSPACE_SLUG`]).
    #[must_use]
    pub fn workspace_slug_for_creator(&self, creator_id: &str) -> &str {
        self.active_workspace_slug_by_creator
            .get(creator_id)
            .map(std::string::String::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_WORKSPACE_SLUG)
    }

    /// Get a configuration value by key name.
    /// Returns the value as a JSON string representation.
    /// MVP: only supports top-level fields listed in [`VALID_CONFIG_KEYS`].
    /// For fields with defaults (`platform_url`, `daemon_url`), returns the default if empty.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is not in [`VALID_CONFIG_KEYS`] or is unsupported.
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
            _ => Err(anyhow::anyhow!("Unsupported config key: {key}")),
        }
    }

    /// Set a configuration value by key name.
    /// MVP: only supports top-level fields listed in [`VALID_CONFIG_KEYS`].
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The key is not in [`VALID_CONFIG_KEYS`]
    /// - The value is invalid for the key (e.g., invalid `runtime_mode`)
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
                    .map_err(|e| anyhow::anyhow!("Invalid runtime_mode '{value}': {e}"))?;
            }
            _ => Err(anyhow::anyhow!("Unsupported config key: {key}"))?,
        }
        Ok(())
    }

    /// Unset (remove) a configuration key.
    /// MVP: only supports top-level fields listed in [`VALID_CONFIG_KEYS`].
    /// For string fields, clears to empty/default.
    ///
    /// # Errors
    ///
    /// Returns an error if the key is not in [`VALID_CONFIG_KEYS`].
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
            _ => Err(anyhow::anyhow!("Unsupported config key: {key}"))?,
        }
        Ok(())
    }

    /// Validate that a key is in the allowed set.
    fn validate_key(key: &str) -> anyhow::Result<()> {
        if VALID_CONFIG_KEYS.contains(&key) {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Invalid config key '{}'. Valid keys: {}",
                key,
                VALID_CONFIG_KEYS.join(", ")
            ))
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
    fn workspace_slug_roundtrips_via_toml() {
        let mut c = CliConfig::default();
        c.active_workspace_slug_by_creator
            .insert("ctr_a".into(), "staging".into());
        assert_eq!(c.workspace_slug_for_creator("ctr_a"), "staging");
        let toml_str = toml::to_string(&c).unwrap();
        let back: CliConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(back.workspace_slug_for_creator("ctr_a"), "staging");
    }

    #[test]
    fn runtime_mode_defaults_to_local_only() {
        let c = CliConfig::default();
        assert!(c.runtime_mode().is_local_only());
    }

    #[test]
    fn runtime_mode_roundtrips_via_toml() {
        let c = CliConfig {
            runtime_mode: DomainRuntimeMode::parse("cloud_enhanced").expect("valid runtime_mode"),
            ..Default::default()
        };
        let toml_str = toml::to_string(&c).expect("toml serialize");
        let back: CliConfig = toml::from_str(&toml_str).expect("toml deserialize");
        assert_eq!(back.runtime_mode().to_string(), "cloud_enhanced");
    }

    #[test]
    fn runtime_mode_parses_from_config_toml() {
        let toml_str = "runtime_mode = \"local_first\"";
        let c: CliConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(c.runtime_mode().to_string(), "local_first");
    }

    #[test]
    fn degradation_snapshot_defaults_to_none() {
        let c = CliConfig::default();
        assert!(c.degradation_snapshot().is_none());
    }

    #[test]
    fn degradation_snapshot_roundtrips_via_toml() {
        use crate::domain::degradation::DegradationState;
        use crate::domain::HealthCheckSnapshot;

        let c = CliConfig {
            degradation_snapshot: Some(DegradationSnapshot {
                state: DegradationState::DegradedLevel1,
                failure_count: 3,
                last_health_check: Some(HealthCheckSnapshot {
                    is_healthy: false,
                    checked_at: "2026-04-15T10:30:00Z".to_string(),
                }),
                last_upgrade_attempt: None,
            }),
            ..Default::default()
        };

        let toml_str = toml::to_string(&c).expect("toml serialize");
        let back: CliConfig = toml::from_str(&toml_str).expect("toml deserialize");

        let snap = back.degradation_snapshot().expect("degradation_snapshot");
        assert_eq!(snap.state, DegradationState::DegradedLevel1);
        assert_eq!(snap.failure_count, 3);
        let hc = snap.last_health_check.as_ref().expect("health_check");
        assert!(!hc.is_healthy);
        assert_eq!(hc.checked_at, "2026-04-15T10:30:00Z");
    }

    #[test]
    fn degradation_snapshot_absent_key_loads_as_none() {
        let toml_str = "runtime_mode = \"local_only\"";
        let c: CliConfig = toml::from_str(toml_str).unwrap();
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
        let c = CliConfig {
            workspace_path: Some(PathBuf::from("/test/path")),
            active_creator_id: Some("ctr_test".to_string()),
            ..Default::default()
        };
        assert_eq!(
            c.get("workspace_path").expect("get workspace_path"),
            "/test/path"
        );
        assert_eq!(
            c.get("active_creator_id").expect("get active_creator_id"),
            "ctr_test"
        );
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
        let mut c = CliConfig {
            workspace_path: Some(PathBuf::from("/test")),
            ..Default::default()
        };
        c.set("workspace_path", "").expect("set workspace_path");
        assert!(c.workspace_path.is_none());
        c.active_creator_id = Some("ctr_old".to_string());
        c.set("active_creator_id", "")
            .expect("set active_creator_id");
        assert!(c.active_creator_id.is_none());
    }

    #[test]
    fn set_invalid_key_returns_error() {
        let mut c = CliConfig::default();
        let err = c
            .set("invalid_key", "value")
            .expect_err("set invalid_key should fail");
        assert!(err.to_string().contains("Invalid config key"));
    }

    #[test]
    fn unset_optional_field_clears_it() {
        let mut c = CliConfig {
            workspace_path: Some(PathBuf::from("/test")),
            ..Default::default()
        };
        c.unset("workspace_path").expect("unset workspace_path");
        assert!(c.workspace_path.is_none());
        c.active_creator_id = Some("ctr_test".to_string());
        c.unset("active_creator_id")
            .expect("unset active_creator_id");
        assert!(c.active_creator_id.is_none());
    }

    #[test]
    fn unset_string_field_reverts_to_default() {
        let mut c = CliConfig {
            platform_url: "https://custom.api.io".to_string(),
            ..Default::default()
        };
        c.unset("platform_url").expect("unset platform_url");
        assert_eq!(c.platform_url, "https://api.nexus42.io");
        c.daemon_url = "http://custom:9999".to_string();
        c.unset("daemon_url").expect("unset daemon_url");
        assert_eq!(c.daemon_url, "http://127.0.0.1:8420");
    }

    #[test]
    fn unset_runtime_mode_reverts_to_default() {
        let mut c = CliConfig {
            runtime_mode: DomainRuntimeMode::parse("cloud_enhanced").expect("valid runtime_mode"),
            ..Default::default()
        };
        c.unset("runtime_mode").expect("unset runtime_mode");
        assert!(c.runtime_mode.is_local_only());
    }

    #[test]
    fn unset_invalid_key_returns_error() {
        let mut c = CliConfig::default();
        let err = c.unset("invalid_key").unwrap_err();
        assert!(err.to_string().contains("Invalid config key"));
    }

    #[test]
    fn path_returns_config_toml_path() {
        let path = CliConfig::path().unwrap();
        assert!(path.ends_with("config.toml"));
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

    #[test]
    fn migration_converts_json_to_toml() {
        let _home = crate::testutil::isolated_home();
        let nexus_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default()
            .join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");

        // Write a legacy config.json with non-default values
        let json_content = r#"{
  "active_creator_id": "ctr_test",
  "platform_url": "https://custom.api.io",
  "daemon_url": "http://127.0.0.1:9999",
  "runtime_mode": "cloud_enhanced"
}"#;
        std::fs::write(nexus_dir.join("config.json"), json_content).expect("write json");

        let result = CliConfig::load().expect("load should succeed");

        // Verify values were loaded correctly
        assert_eq!(result.active_creator_id.as_deref(), Some("ctr_test"));
        assert_eq!(result.platform_url, "https://custom.api.io");
        assert_eq!(result.daemon_url, "http://127.0.0.1:9999");

        // Verify config.toml was created
        assert!(
            nexus_dir.join("config.toml").exists(),
            "config.toml should be created"
        );

        // Verify config.toml content is valid TOML
        let toml_content = std::fs::read_to_string(nexus_dir.join("config.toml")).unwrap();
        let loaded: CliConfig = toml::from_str(&toml_content).unwrap();
        assert_eq!(loaded.active_creator_id.as_deref(), Some("ctr_test"));

        // Verify config.json was renamed to config.json.migrated
        assert!(
            !nexus_dir.join("config.json").exists(),
            "config.json should be renamed"
        );
        assert!(
            nexus_dir.join("config.json.migrated").exists(),
            "config.json.migrated should exist"
        );
    }

    #[test]
    fn migration_with_empty_json_returns_default() {
        let _home = crate::testutil::isolated_home();
        let nexus_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default()
            .join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");

        // Write an empty config.json
        std::fs::write(nexus_dir.join("config.json"), "").expect("write empty json");

        let result = CliConfig::load().expect("load should succeed");

        // Should return defaults
        assert_eq!(result.active_creator_id, None);

        // config.json should be renamed
        assert!(nexus_dir.join("config.json.migrated").exists());
    }

    #[test]
    fn load_reads_toml_when_present() {
        let _home = crate::testutil::isolated_home();
        let nexus_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default()
            .join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");

        // Write a config.toml directly
        let toml_content = r#"active_creator_id = "ctr_direct"
platform_url = "https://direct.api.io"
"#;
        std::fs::write(nexus_dir.join("config.toml"), toml_content).expect("write toml");

        let result = CliConfig::load().expect("load should succeed");

        assert_eq!(result.active_creator_id.as_deref(), Some("ctr_direct"));
        assert_eq!(result.platform_url, "https://direct.api.io");
    }

    #[test]
    fn save_writes_toml() {
        let _home = crate::testutil::isolated_home();
        let nexus_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default()
            .join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");

        let cfg = CliConfig {
            active_creator_id: Some("ctr_save_test".to_string()),
            platform_url: "https://save.test.io".to_string(),
            ..Default::default()
        };

        cfg.save().expect("save should succeed");

        let config_path = nexus_dir.join("config.toml");
        assert!(config_path.exists(), "config.toml should exist after save");

        let content = std::fs::read_to_string(&config_path).expect("read config.toml");
        let loaded: CliConfig = toml::from_str(&content).expect("parse config.toml");
        assert_eq!(loaded.active_creator_id.as_deref(), Some("ctr_save_test"));
        assert_eq!(loaded.platform_url, "https://save.test.io");
    }

    // -----------------------------------------------------------------------
    // UserAgentsConfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn user_agents_config_loads_missing_file_as_default() {
        let _home = crate::testutil::isolated_home();
        let cfg = UserAgentsConfig::load().expect("load should succeed");
        assert!(cfg.strategies.is_empty());
    }

    #[test]
    fn user_agents_config_loads_valid_toml() {
        let _home = crate::testutil::isolated_home();
        let nexus_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default()
            .join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");

        let toml_content = r#"
[strategies.novel-writing.roles.writer]
agent = "claude-sonnet-4-20250514"
model = "claude-3-opus"

[strategies.novel-writing.roles.reviewer]
agent = "codex-acp"
model = "o3"

[strategies.default.roles.editor]
agent = "claude-sonnet-4-20250514"
"#;
        std::fs::write(nexus_dir.join("agents.toml"), toml_content).expect("write toml");

        let cfg = UserAgentsConfig::load().expect("load should succeed");

        assert_eq!(cfg.strategies.len(), 2);

        // Check novel-writing strategy
        let novel = cfg
            .strategies
            .get("novel-writing")
            .expect("novel-writing strategy");
        let writer = novel.roles.get("writer").expect("writer role");
        assert_eq!(writer.agent.as_deref(), Some("claude-sonnet-4-20250514"));
        assert_eq!(writer.model.as_deref(), Some("claude-3-opus"));

        let reviewer = novel.roles.get("reviewer").expect("reviewer role");
        assert_eq!(reviewer.agent.as_deref(), Some("codex-acp"));
        assert_eq!(reviewer.model.as_deref(), Some("o3"));

        // Check default strategy
        let default_strat = cfg.strategies.get("default").expect("default strategy");
        let editor = default_strat.roles.get("editor").expect("editor role");
        assert_eq!(editor.agent.as_deref(), Some("claude-sonnet-4-20250514"));
        assert!(editor.model.is_none());
    }

    #[test]
    fn user_agents_config_returns_defaults_on_corrupt_file() {
        let _home = crate::testutil::isolated_home();
        let nexus_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default()
            .join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");

        std::fs::write(nexus_dir.join("agents.toml"), "this is not toml {{{")
            .expect("write corrupt");

        let cfg = UserAgentsConfig::load().expect("load should succeed with defaults");

        assert!(cfg.strategies.is_empty());
    }

    #[test]
    fn user_agents_config_empty_file_returns_default() {
        let _home = crate::testutil::isolated_home();
        let nexus_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default()
            .join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");

        std::fs::write(nexus_dir.join("agents.toml"), "").expect("write empty");

        let cfg = UserAgentsConfig::load().expect("load should succeed");

        assert!(cfg.strategies.is_empty());
    }

    #[test]
    fn user_agents_config_roundtrips_via_toml() {
        let mut cfg = UserAgentsConfig::default();
        let mut roles = HashMap::new();
        roles.insert(
            "writer".to_string(),
            RoleOverride::new(
                Some("claude-sonnet-4-20250514".to_string()),
                Some("claude-3-opus".to_string()),
            ),
        );
        let strat = StrategyOverrides { roles };
        cfg.strategies.insert("novel-writing".to_string(), strat);

        let toml_str = toml::to_string(&cfg).unwrap();
        let back: UserAgentsConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(back.strategies.len(), 1);
        let writer = back
            .strategies
            .get("novel-writing")
            .and_then(|s| s.roles.get("writer"))
            .expect("writer");
        assert_eq!(writer.agent.as_deref(), Some("claude-sonnet-4-20250514"));
        assert_eq!(writer.model.as_deref(), Some("claude-3-opus"));
    }

    #[test]
    fn user_agents_config_path_is_correct() {
        let path = UserAgentsConfig::config_path().unwrap();
        assert!(path.ends_with("agents.toml"));
        assert!(path.to_string_lossy().contains(".nexus42"));
    }

    #[test]
    fn user_agents_resolve_role_checks_strategy_then_default() {
        let mut cfg = UserAgentsConfig::default();

        // Add a "default" strategy with an editor role.
        let mut default_roles = HashMap::new();
        default_roles.insert(
            "editor".to_string(),
            RoleOverride::new(Some("default-agent".to_string()), None),
        );
        cfg.strategies.insert(
            "default".to_string(),
            StrategyOverrides {
                roles: default_roles,
            },
        );

        // No "novel-writing" strategy — should fall back to "default".
        let result = cfg.resolve_role("novel-writing", "editor");
        assert!(result.is_some());
        assert_eq!(
            result.expect("found").agent.as_deref(),
            Some("default-agent")
        );

        // Role not in either — should return None.
        assert!(cfg.resolve_role("novel-writing", "writer").is_none());
    }

    // -----------------------------------------------------------------------
    // parse_agent_ref tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_agent_ref_two_segments() {
        let (role, agent, model) = parse_agent_ref("writer:claude-acp").unwrap();
        assert_eq!(role, "writer");
        assert_eq!(agent, "claude-acp");
        assert!(model.is_none());
    }

    #[test]
    fn parse_agent_ref_three_segments() {
        let (role, agent, model) = parse_agent_ref("reviewer:codex-acp:o3").unwrap();
        assert_eq!(role, "reviewer");
        assert_eq!(agent, "codex-acp");
        assert_eq!(model.as_deref(), Some("o3"));
    }

    #[test]
    fn parse_agent_ref_three_segments_empty_model() {
        let (role, agent, model) = parse_agent_ref("writer:claude-acp:").unwrap();
        assert_eq!(role, "writer");
        assert_eq!(agent, "claude-acp");
        assert!(model.is_none());
    }

    #[test]
    fn parse_agent_ref_single_segment_errors() {
        let err = parse_agent_ref("just-one").unwrap_err();
        assert!(err.to_string().contains("invalid --agent-ref format"));
        assert!(err.to_string().contains("must have at least 2"));
    }

    #[test]
    fn parse_agent_ref_empty_string_errors() {
        let err = parse_agent_ref("").unwrap_err();
        assert!(err.to_string().contains("invalid --agent-ref format"));
    }

    #[test]
    fn parse_agent_ref_empty_role_errors() {
        let err = parse_agent_ref(":claude-acp").unwrap_err();
        assert!(err.to_string().contains("role ID must not be empty"));
    }

    #[test]
    fn parse_agent_ref_empty_agent_errors() {
        let err = parse_agent_ref("writer:").unwrap_err();
        assert!(err.to_string().contains("agent ID must not be empty"));
    }

    // -----------------------------------------------------------------------
    // resolve_agent_model tests
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_cli_overrides_take_highest_priority() {
        let mut cli = HashMap::new();
        cli.insert(
            "writer".to_string(),
            RoleOverride::new(Some("cli-agent".to_string()), Some("cli-model".to_string())),
        );

        let user_cfg = UserAgentsConfig::default();
        let preset = vec!["preset-model".to_string()];

        let (agent, model) =
            resolve_agent_model("writer", "novel-writing", &preset, &user_cfg, &cli);
        assert_eq!(agent.as_deref(), Some("cli-agent"));
        assert_eq!(model.as_deref(), Some("cli-model"));
    }

    #[test]
    fn resolve_user_config_used_when_no_cli_override() {
        let mut user_cfg = UserAgentsConfig::default();
        let mut roles = HashMap::new();
        roles.insert(
            "writer".to_string(),
            RoleOverride::new(
                Some("user-agent".to_string()),
                Some("user-model".to_string()),
            ),
        );
        user_cfg
            .strategies
            .insert("novel-writing".to_string(), StrategyOverrides { roles });

        let cli = HashMap::new();
        let preset = vec!["preset-model".to_string()];

        let (agent, model) =
            resolve_agent_model("writer", "novel-writing", &preset, &user_cfg, &cli);
        assert_eq!(agent.as_deref(), Some("user-agent"));
        assert_eq!(model.as_deref(), Some("user-model"));
    }

    #[test]
    fn resolve_preset_fallback_when_no_cli_or_user() {
        let user_cfg = UserAgentsConfig::default();
        let cli = HashMap::new();
        let preset = vec!["preset-model-v1".to_string(), "preset-model-v2".to_string()];

        let (agent, model) =
            resolve_agent_model("writer", "novel-writing", &preset, &user_cfg, &cli);
        assert!(agent.is_none());
        assert_eq!(model.as_deref(), Some("preset-model-v1"));
    }

    #[test]
    fn resolve_nothing_when_all_layers_empty() {
        let user_cfg = UserAgentsConfig::default();
        let cli = HashMap::new();
        let preset: Vec<String> = vec![];

        let (agent, model) =
            resolve_agent_model("writer", "novel-writing", &preset, &user_cfg, &cli);
        assert!(agent.is_none());
        assert!(model.is_none());
    }

    #[test]
    fn resolve_user_config_fallback_to_default_strategy() {
        let mut user_cfg = UserAgentsConfig::default();
        let mut default_roles = HashMap::new();
        default_roles.insert(
            "writer".to_string(),
            RoleOverride::new(Some("default-agent".to_string()), None),
        );
        user_cfg.strategies.insert(
            "default".to_string(),
            StrategyOverrides {
                roles: default_roles,
            },
        );

        let cli = HashMap::new();
        let preset: Vec<String> = vec![];

        let (agent, model) =
            resolve_agent_model("writer", "some-strategy", &preset, &user_cfg, &cli);
        assert_eq!(agent.as_deref(), Some("default-agent"));
        assert!(model.is_none());
    }
}

// ---------------------------------------------------------------------------
// Multi-agent user configuration (`~/.nexus42/agents.toml`)
//
// These types and functions are public API for multi-agent worker configuration.
// They are consumed by the lib crate (tests, future schedule/worker wiring).
// The binary crate re-declares `mod config` privately, so dead_code is expected
// until wiring is complete (T8+).
// ---------------------------------------------------------------------------

/// Per-role agent/model override specified by the user.
///
/// Used in CLI `--agent-ref` flags and user config file to override
/// the agent ACP ID and/or model for a given role.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleOverride {
    /// ACP agent identifier (e.g. `claude-sonnet-4-20250514`, `codex-acp`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,

    /// Model override (e.g. `o3`, `claude-3-opus`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[allow(dead_code)]
impl RoleOverride {
    /// Create a new override with both agent and model.
    #[must_use]
    pub const fn new(agent: Option<String>, model: Option<String>) -> Self {
        Self { agent, model }
    }
}

/// Per-strategy role overrides from user config.
///
/// In the TOML file this appears as a flat table keyed by role ID:
/// ```toml
/// [strategies.novel-writing.roles.writer]
/// agent = "claude-sonnet-4-20250514"
/// model = "claude-3-opus"
/// ```
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyOverrides {
    /// Role ID → override mapping.
    #[serde(default)]
    pub roles: HashMap<String, RoleOverride>,
}

/// User-level multi-agent configuration file (`~/.nexus42/agents.toml`).
///
/// This file provides persistent agent/model overrides per strategy and role.
/// It is the middle layer in the priority resolution chain:
/// CLI flags > user config > preset `recommended_skills`.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserAgentsConfig {
    /// Strategy ID → per-role overrides.
    #[serde(default)]
    pub strategies: HashMap<String, StrategyOverrides>,
}

#[allow(dead_code)]
impl UserAgentsConfig {
    /// File name for the user agents configuration.
    const FILENAME: &str = "agents.toml";

    /// Load the user agents configuration from `~/.nexus42/agents.toml`.
    ///
    /// Returns a default (empty) config if the file does not exist.
    /// Logs a warning and returns defaults if the file exists but cannot be parsed.
    ///
    /// # Errors
    ///
    /// Returns an error if the Nexus home directory cannot be resolved.
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)?;
        if content.trim().is_empty() {
            return Ok(Self::default());
        }

        match toml::from_str::<Self>(&content) {
            Ok(cfg) => Ok(cfg),
            Err(e) => {
                tracing::warn!("agents.toml parse error, using defaults: {}", e);
                Ok(Self::default())
            }
        }
    }

    /// Return the path to `~/.nexus42/agents.toml`.
    ///
    /// # Errors
    ///
    /// Returns an error if the Nexus home directory cannot be resolved.
    pub fn config_path() -> anyhow::Result<PathBuf> {
        Ok(nexus_home()?.join(Self::FILENAME))
    }

    /// Look up a role override for a given strategy.
    ///
    /// Returns `None` if the strategy or role is not configured.
    #[must_use]
    pub fn get_role_override(&self, strategy_id: &str, role_id: &str) -> Option<&RoleOverride> {
        self.strategies
            .get(strategy_id)
            .and_then(|s| s.roles.get(role_id))
    }

    /// Look up a role override across all strategies (fallback).
    ///
    /// Checks the specified strategy first, then falls back to a
    /// `"default"` strategy if it exists.
    #[must_use]
    pub fn resolve_role(&self, strategy_id: &str, role_id: &str) -> Option<&RoleOverride> {
        self.get_role_override(strategy_id, role_id)
            .or_else(|| self.get_role_override("default", role_id))
    }
}

/// Parse an `--agent-ref` string into `(role_id, acp_agent_id, model)`.
///
/// Accepted formats:
/// - `role:acp_agent_id` (2 segments) — model is `None`
/// - `role:acp_agent_id:model` (3 segments) — all fields present
///
/// # Errors
///
/// Returns a descriptive error if the string has fewer than 2 or more than 3
/// colon-separated segments, or if any segment is empty.
#[allow(dead_code)]
pub fn parse_agent_ref(ref_str: &str) -> anyhow::Result<(String, String, Option<String>)> {
    let segments: Vec<&str> = ref_str.splitn(3, ':').collect();

    if segments.len() < 2 {
        anyhow::bail!(
            "invalid --agent-ref format '{ref_str}': expected 'role:agent_id' or 'role:agent_id:model' \
             (must have at least 2 colon-separated segments)"
        );
    }

    let role_id = segments[0];
    let acp_agent_id = segments[1];

    if role_id.is_empty() {
        anyhow::bail!("invalid --agent-ref '{ref_str}': role ID must not be empty");
    }
    if acp_agent_id.is_empty() {
        anyhow::bail!("invalid --agent-ref '{ref_str}': agent ID must not be empty");
    }

    // Third segment (model) is optional.
    let model = if segments.len() == 3 {
        let m = segments[2];
        if m.is_empty() {
            None
        } else {
            Some(m.to_string())
        }
    } else {
        None
    };

    Ok((role_id.to_string(), acp_agent_id.to_string(), model))
}

/// Resolve the effective agent and model for a role using priority chain.
///
/// Priority: CLI `--agent-ref` overrides > user config > preset `recommended_skills[0]`.
///
/// # Arguments
///
/// * `role_id` — the role to resolve (e.g. `"writer"`, `"reviewer"`)
/// * `strategy_id` — the strategy context (e.g. `"novel-writing"`)
/// * `preset_recommended` — from `PresetRoleDefinition.recommended_skills` (from T6)
/// * `user_config` — loaded `UserAgentsConfig`
/// * `cli_overrides` — map of `role_id` → `RoleOverride` from `--agent-ref` flags
///
/// # Returns
///
/// `(Option<agent>, Option<model>)` — the resolved agent ACP ID and model.
#[allow(dead_code)]
#[must_use]
pub fn resolve_agent_model<S: std::hash::BuildHasher>(
    role_id: &str,
    strategy_id: &str,
    preset_recommended: &[String],
    user_config: &UserAgentsConfig,
    cli_overrides: &HashMap<String, RoleOverride, S>,
) -> (Option<String>, Option<String>) {
    // 1. CLI overrides (highest priority)
    if let Some(cli_override) = cli_overrides.get(role_id) {
        return (cli_override.agent.clone(), cli_override.model.clone());
    }

    // 2. User config
    if let Some(user_override) = user_config.resolve_role(strategy_id, role_id) {
        return (user_override.agent.clone(), user_override.model.clone());
    }

    // 3. Preset recommended_skills[0] — may be "agent_id:model_name" or plain model name
    if let Some(preset) = preset_recommended.first() {
        if let Some((agent, model)) = preset.split_once(':') {
            let agent = agent.to_string();
            let model = model.to_string();
            return (Some(agent), Some(model));
        }
        return (None, Some(preset.clone()));
    }
    (None, None)
}

/// Get the nexus42 home directory (`$HOME/.nexus42`).
///
/// # Errors
///
/// Returns an error if the home directory cannot be resolved.
pub fn nexus_home() -> anyhow::Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
    Ok(home.join(NEXUS_DIR))
}

/// User home directory (`$HOME`).
///
/// # Errors
///
/// Returns an error if the home directory cannot be resolved.
pub fn user_home_dir() -> anyhow::Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))
}

/// Resolve workspace `state.db` under ADR-014 (`creators/<creator_id>/workspaces/<slug>/state.db`).
///
/// # Errors
///
/// Returns an error if:
/// - The home directory cannot be resolved
/// - No active creator is configured
pub fn resolve_state_db_path(config: &CliConfig) -> anyhow::Result<PathBuf> {
    let user_home = user_home_dir()?;
    let cid = config.active_creator_id.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "No active creator configured. Run `nexus42 creator workspace init workspace` or `nexus42 creator use <id>`."
        )
    })?;
    let slug = config.workspace_slug_for_creator(cid);
    Ok(crate::paths::state_db_path(&user_home, cid, slug))
}

/// Load config and resolve the local `SQLite` database path.
///
/// # Errors
///
/// Returns an error if:
/// - CLI configuration cannot be loaded
/// - State DB path cannot be resolved
pub fn state_db_path() -> anyhow::Result<PathBuf> {
    resolve_state_db_path(&CliConfig::load()?)
}

/// Get the path to the auth storage file.
///
/// # Errors
///
/// Returns an error if the Nexus home directory cannot be resolved.
pub fn auth_store_path() -> anyhow::Result<PathBuf> {
    Ok(nexus_home()?.join("auth.json"))
}

/// Check if the current directory (or any parent) contains a workspace
#[must_use]
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
#[must_use]
pub fn workspace_nexus_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(NEXUS_DIR)
}

/// Get workspace config file path
#[must_use]
pub fn workspace_config_path(workspace_root: &Path) -> PathBuf {
    workspace_nexus_dir(workspace_root).join("workspace.json")
}
