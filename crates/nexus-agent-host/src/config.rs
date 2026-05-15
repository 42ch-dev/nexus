//! Configuration loading for the agent host.
//!
//! Loads from `{NEXUS_HOME}/agent-host/config.toml` using `nexus-home-layout`
//! path helpers. Missing config yields safe defaults; invalid config returns
//! structured error.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{HostError, HostResult};
use crate::ids::ProviderId;

/// Agent host configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHostConfig {
    /// Maximum concurrent sessions (default: 4).
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,

    /// Maximum concurrent operations per session (default: 1).
    #[serde(default = "default_max_ops_per_session")]
    pub max_ops_per_session: usize,

    /// Timeout configuration.
    #[serde(default)]
    pub timeouts: TimeoutConfig,

    /// Policy configuration.
    #[serde(default)]
    pub policy: PolicyConfig,

    /// Explicit provider configurations.
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

/// Timeout configuration (all values in milliseconds).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// Provider launch timeout (default: 15s).
    #[serde(default = "default_launch_ms")]
    pub launch_ms: u64,

    /// ACP initialize handshake timeout (default: 15s).
    #[serde(default = "default_initialize_ms")]
    pub initialize_ms: u64,

    /// Session creation timeout (default: 30s).
    #[serde(default = "default_session_ms")]
    pub session_ms: u64,

    /// Prompt/operation execution timeout (default: 180s).
    #[serde(default = "default_prompt_ms")]
    pub prompt_ms: u64,

    /// Graceful shutdown timeout (default: 5s).
    #[serde(default = "default_shutdown_ms")]
    pub shutdown_ms: u64,
}

/// Policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// How to handle unknown (not explicitly configured) providers.
    /// "deny" or "allow" (default: "deny").
    #[serde(default = "default_unknown_provider")]
    pub unknown_provider: String,

    /// How to handle tools with unknown risk classification.
    /// "deny", "ask", or "allow" (default: "deny").
    #[serde(default = "default_unknown_tool_risk")]
    pub unknown_tool_risk: String,

    /// Whether to allow model fallback when `set_model` fails (default: true).
    #[serde(default = "default_allow_model_fallback")]
    pub allow_model_fallback: bool,
}

/// Per-provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider ID.
    pub id: String,

    /// Protocol kind: "acp" or `native_cli`.
    pub protocol: String,

    /// Command to execute (for native CLI providers).
    pub command: Option<String>,

    /// Command arguments.
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,

    /// Whether this provider is enabled (default: true).
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

// ── Default value functions ──────────────────────────────────────────

const fn default_max_sessions() -> usize {
    4
}

const fn default_max_ops_per_session() -> usize {
    1
}

const fn default_launch_ms() -> u64 {
    15_000
}

const fn default_initialize_ms() -> u64 {
    15_000
}

const fn default_session_ms() -> u64 {
    30_000
}

const fn default_prompt_ms() -> u64 {
    180_000
}

const fn default_shutdown_ms() -> u64 {
    5_000
}

fn default_unknown_provider() -> String {
    "deny".to_string()
}

fn default_unknown_tool_risk() -> String {
    "deny".to_string()
}

const fn default_allow_model_fallback() -> bool {
    true
}

const fn default_enabled() -> bool {
    true
}

// ── Default impls ────────────────────────────────────────────────────

impl Default for AgentHostConfig {
    fn default() -> Self {
        Self {
            max_sessions: default_max_sessions(),
            max_ops_per_session: default_max_ops_per_session(),
            timeouts: TimeoutConfig::default(),
            policy: PolicyConfig::default(),
            providers: Vec::new(),
        }
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            launch_ms: default_launch_ms(),
            initialize_ms: default_initialize_ms(),
            session_ms: default_session_ms(),
            prompt_ms: default_prompt_ms(),
            shutdown_ms: default_shutdown_ms(),
        }
    }
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            unknown_provider: default_unknown_provider(),
            unknown_tool_risk: default_unknown_tool_risk(),
            allow_model_fallback: default_allow_model_fallback(),
        }
    }
}

impl TimeoutConfig {
    /// Launch timeout as `std::time::Duration`.
    #[must_use]
    pub const fn launch_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.launch_ms)
    }

    /// Initialize timeout as `std::time::Duration`.
    #[must_use]
    pub const fn initialize_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.initialize_ms)
    }

    /// Session timeout as `std::time::Duration`.
    #[must_use]
    pub const fn session_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.session_ms)
    }

    /// Prompt timeout as `std::time::Duration`.
    #[must_use]
    pub const fn prompt_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.prompt_ms)
    }

    /// Shutdown timeout as `std::time::Duration`.
    #[must_use]
    pub const fn shutdown_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.shutdown_ms)
    }
}

impl PolicyConfig {
    /// Whether unknown providers are denied.
    #[must_use]
    pub fn deny_unknown_providers(&self) -> bool {
        self.unknown_provider == "deny"
    }

    /// Whether model fallback is allowed.
    #[must_use]
    pub const fn allow_model_fallback(&self) -> bool {
        self.allow_model_fallback
    }
}

impl ProviderConfig {
    /// Get the provider ID as a typed `ProviderId`.
    #[must_use]
    pub fn provider_id(&self) -> ProviderId {
        ProviderId::new(&self.id)
    }

    /// Parse the protocol string into a `ProtocolKind`.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the protocol string is not recognized.
    pub fn protocol_kind(&self) -> HostResult<crate::capability::model::ProtocolKind> {
        match self.protocol.as_str() {
            "acp" => Ok(crate::capability::model::ProtocolKind::Acp),
            "native_cli" => Ok(crate::capability::model::ProtocolKind::NativeCli),
            other => Err(HostError::internal(format!(
                "unknown protocol '{other}' for provider '{}'",
                self.id
            ))),
        }
    }
}

// ── Path helpers ─────────────────────────────────────────────────────

/// Get the agent-host config directory under the Nexus home.
///
/// `$HOME/.nexus42/agent-host/`
#[must_use]
pub fn agent_host_config_dir(home: &Path) -> PathBuf {
    nexus_home_layout::nexus_root_from_home(home).join("agent-host")
}

/// Get the agent-host config file path.
///
/// `$HOME/.nexus42/agent-host/config.toml`
#[must_use]
pub fn agent_host_config_path(home: &Path) -> PathBuf {
    agent_host_config_dir(home).join("config.toml")
}

/// Load agent host config from the Nexus home layout.
///
/// If the config file does not exist, returns safe defaults.
/// If the config file is invalid TOML, returns a structured error.
///
/// # Errors
///
/// Returns `HostError::InternalHostError` if the config file exists
/// but cannot be read or parsed.
pub fn load_config(home: &Path) -> HostResult<AgentHostConfig> {
    let config_path = agent_host_config_path(home);

    if !config_path.exists() {
        tracing::info!(
            path = %config_path.display(),
            "Config file not found, using defaults"
        );
        return Ok(AgentHostConfig::default());
    }

    let content = std::fs::read_to_string(&config_path).map_err(|e| {
        HostError::internal(format!(
            "failed to read config from {}: {e}",
            config_path.display()
        ))
    })?;

    let config: AgentHostConfig = toml::from_str(&content).map_err(|e| {
        HostError::internal(format!(
            "failed to parse config from {}: {e}",
            config_path.display()
        ))
    })?;

    tracing::info!(
        path = %config_path.display(),
        max_sessions = config.max_sessions,
        providers = config.providers.len(),
        "Loaded agent host config"
    );

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let config = AgentHostConfig::default();
        assert_eq!(config.max_sessions, 4);
        assert_eq!(config.max_ops_per_session, 1);
        assert_eq!(config.timeouts.launch_ms, 15_000);
        assert_eq!(config.timeouts.initialize_ms, 15_000);
        assert_eq!(config.timeouts.session_ms, 30_000);
        assert_eq!(config.timeouts.prompt_ms, 180_000);
        assert_eq!(config.timeouts.shutdown_ms, 5_000);
        assert_eq!(config.policy.unknown_provider, "deny");
        assert_eq!(config.policy.unknown_tool_risk, "deny");
        assert!(config.policy.allow_model_fallback);
        assert!(config.providers.is_empty());
    }

    #[test]
    fn default_timeouts_as_durations() {
        let timeouts = TimeoutConfig::default();
        assert_eq!(
            timeouts.launch_duration(),
            std::time::Duration::from_secs(15)
        );
        assert_eq!(
            timeouts.initialize_duration(),
            std::time::Duration::from_secs(15)
        );
        assert_eq!(
            timeouts.session_duration(),
            std::time::Duration::from_secs(30)
        );
        assert_eq!(
            timeouts.prompt_duration(),
            std::time::Duration::from_secs(180)
        );
        assert_eq!(
            timeouts.shutdown_duration(),
            std::time::Duration::from_secs(5)
        );
    }

    #[test]
    fn policy_config_helpers() {
        let policy = PolicyConfig::default();
        assert!(policy.deny_unknown_providers());
        assert!(policy.allow_model_fallback());
    }

    #[test]
    fn load_config_missing_file_returns_defaults() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let home = temp_dir.path();

        let config = load_config(home).expect("Should return defaults");
        assert_eq!(config.max_sessions, 4);
        assert!(config.providers.is_empty());
    }

    #[test]
    fn load_config_valid_file() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let home = temp_dir.path();

        let config_path = agent_host_config_path(home);
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create config dir");
        }

        let toml_content = r#"
max_sessions = 8
max_ops_per_session = 2

[timeouts]
launch_ms = 20000
initialize_ms = 20000
session_ms = 60000
prompt_ms = 300000
shutdown_ms = 10000

[policy]
unknown_provider = "allow"
unknown_tool_risk = "ask"
allow_model_fallback = false

[[providers]]
id = "claude-native"
protocol = "native_cli"
command = "claude"
args = ["-p"]
enabled = true
"#;
        std::fs::write(&config_path, toml_content).expect("Failed to write config");

        let config = load_config(home).expect("Should load config");
        assert_eq!(config.max_sessions, 8);
        assert_eq!(config.max_ops_per_session, 2);
        assert_eq!(config.timeouts.launch_ms, 20_000);
        assert_eq!(config.policy.unknown_provider, "allow");
        assert!(!config.policy.allow_model_fallback);
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].id, "claude-native");
        assert_eq!(config.providers[0].protocol, "native_cli");
    }

    #[test]
    fn load_config_invalid_toml_returns_error() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let home = temp_dir.path();

        let config_path = agent_host_config_path(home);
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create config dir");
        }

        std::fs::write(&config_path, "this is not valid toml {{{{")
            .expect("Failed to write config");

        let result = load_config(home);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.category(), "internal_host_error");
        assert!(err.to_string().contains("failed to parse config"));
    }

    #[test]
    fn load_config_partial_toml_uses_defaults() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let home = temp_dir.path();

        let config_path = agent_host_config_path(home);
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create config dir");
        }

        // Only override max_sessions; everything else should be defaults
        let toml_content = "max_sessions = 2\n";
        std::fs::write(&config_path, toml_content).expect("Failed to write config");

        let config = load_config(home).expect("Should load partial config");
        assert_eq!(config.max_sessions, 2);
        // Other fields should use defaults
        assert_eq!(config.max_ops_per_session, 1);
        assert_eq!(config.timeouts.launch_ms, 15_000);
        assert_eq!(config.policy.unknown_provider, "deny");
    }

    #[test]
    fn provider_config_protocol_kind() {
        let acp_config = ProviderConfig {
            id: "test-acp".to_string(),
            protocol: "acp".to_string(),
            command: None,
            args: vec![],
            env: std::collections::HashMap::new(),
            enabled: true,
        };
        assert_eq!(
            acp_config.protocol_kind().unwrap(),
            crate::capability::model::ProtocolKind::Acp
        );

        let native_config = ProviderConfig {
            id: "test-native".to_string(),
            protocol: "native_cli".to_string(),
            command: Some("claude".to_string()),
            args: vec![],
            env: std::collections::HashMap::new(),
            enabled: true,
        };
        assert_eq!(
            native_config.protocol_kind().unwrap(),
            crate::capability::model::ProtocolKind::NativeCli
        );
    }

    #[test]
    fn provider_config_unknown_protocol() {
        let config = ProviderConfig {
            id: "test-bad".to_string(),
            protocol: "unknown_proto".to_string(),
            command: None,
            args: vec![],
            env: std::collections::HashMap::new(),
            enabled: true,
        };
        let result = config.protocol_kind();
        assert!(result.is_err());
    }

    #[test]
    fn agent_host_config_path_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            agent_host_config_path(&home),
            PathBuf::from("/fake/home/.nexus42/agent-host/config.toml")
        );
    }
}
