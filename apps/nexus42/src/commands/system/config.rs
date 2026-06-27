//! Config Command — Configuration file management
//!
//! Provides commands to view and modify the CLI configuration file.
//! MVP: only supports top-level fields listed in `config::VALID_CONFIG_KEYS`.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Get a configuration value
    Get {
        /// Configuration key (e.g., `runtime_mode`, `platform_url`)
        key: String,
    },

    /// Set a configuration value
    Set {
        /// Configuration key (e.g., `runtime_mode`, `platform_url`)
        key: String,
        /// Configuration value
        value: String,
    },

    /// Unset (reset to default) a configuration key
    Unset {
        /// Configuration key to reset
        key: String,
    },

    /// Show the configuration file path
    Path,
}

/// Run config command
///
/// # Errors
///
/// Returns `CliError` if:
/// - The configuration key is invalid
/// - The configuration file cannot be read or written
pub fn run(cmd: ConfigCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        ConfigCommand::Get { key } => get(config, &key),
        ConfigCommand::Set { key, value } => set(config, &key, &value),
        ConfigCommand::Unset { key } => unset(config, &key),
        ConfigCommand::Path => path(),
    }
}

fn get(config: &CliConfig, key: &str) -> Result<()> {
    let value = config.get(key)?;
    if value.is_empty() {
        println!("{key}: (unset)");
    } else {
        println!("{key}: {value}");
    }
    Ok(())
}

fn set(config: &CliConfig, key: &str, value: &str) -> Result<()> {
    let mut updated = config.clone();
    updated.set(key, value)?;
    updated.save()?;

    println!("Set {key} = {value}");
    Ok(())
}

fn unset(config: &CliConfig, key: &str) -> Result<()> {
    let mut updated = config.clone();
    updated.unset(key)?;
    updated.save()?;

    println!("Unset {key} (reverted to default)");
    Ok(())
}

fn path() -> Result<()> {
    let config_path = CliConfig::path()?;
    println!("{}", config_path.display());
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::config::VALID_CONFIG_KEYS;

    #[test]
    fn config_command_get_reads_runtime_mode() {
        let config = CliConfig::default();
        let value = config.get("runtime_mode").unwrap();
        assert_eq!(value, "local_only");
    }

    #[test]
    fn config_command_get_reads_platform_url() {
        let config = CliConfig::default();
        let value = config.get("platform_url").unwrap();
        // Default CliConfig has empty platform_url, but get() returns the default value
        assert_eq!(value, "https://api.nexus42.io");
    }

    #[test]
    fn config_command_set_updates_and_saves() {
        let mut config = CliConfig::default();
        config.set("platform_url", "https://custom.api.io").unwrap();
        assert_eq!(config.platform_url, "https://custom.api.io");
        // Note: we don't test save() here because it writes to disk
        // Integration tests should cover actual file persistence
    }

    #[test]
    fn config_command_unset_reverts_to_default() {
        let mut config = CliConfig::default();
        config.platform_url = "https://custom.api.io".to_string();
        config.unset("platform_url").unwrap();
        assert_eq!(config.platform_url, "https://api.nexus42.io");
    }

    #[test]
    fn config_command_path_returns_pathbuf() {
        let path = CliConfig::path().unwrap();
        assert!(path.ends_with("config.toml"));
    }

    #[test]
    fn get_invalid_key_propagates_error() {
        let config = CliConfig::default();
        let err = config.get("invalid_key").unwrap_err();
        assert!(err.to_string().contains("Invalid config key"));
    }

    #[test]
    fn set_invalid_key_propagates_error() {
        let mut config = CliConfig::default();
        let err = config.set("invalid_key", "value").unwrap_err();
        assert!(err.to_string().contains("Invalid config key"));
    }

    #[test]
    fn valid_keys_match_config_module_constant() {
        // Ensure we're using the same valid keys list
        assert!(VALID_CONFIG_KEYS.contains(&"runtime_mode"));
        assert!(VALID_CONFIG_KEYS.contains(&"platform_url"));
        assert!(VALID_CONFIG_KEYS.contains(&"daemon_url"));
        assert!(VALID_CONFIG_KEYS.contains(&"workspace_path"));
        assert!(VALID_CONFIG_KEYS.contains(&"active_creator_id"));
    }

    #[test]
    fn empty_optional_field_shows_unset() {
        let config = CliConfig::default();
        let value = config.get("workspace_path").unwrap();
        assert!(value.is_empty());
        let value = config.get("active_creator_id").unwrap();
        assert!(value.is_empty());
    }

    #[test]
    fn set_optional_field_to_value() {
        let mut config = CliConfig::default();
        config.set("active_creator_id", "ctr_test").unwrap();
        assert_eq!(config.active_creator_id, Some("ctr_test".to_string()));
    }

    #[test]
    fn unset_optional_field_clears_it() {
        let mut config = CliConfig::default();
        config.active_creator_id = Some("ctr_old".to_string());
        config.unset("active_creator_id").unwrap();
        assert!(config.active_creator_id.is_none());
    }
}
