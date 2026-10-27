//! Agent-host configuration read/write across napi (provider configuration).
//!
//! The service exposes `GET/PUT /v1/daemon/agent-host/config`; both lanes are
//! thin adapters over the native config file
//! (`{NEXUS_HOME}/agent-host/config.toml`). Readiness is not recomputed here:
//! the owner re-probes the selected providers at the next execution-owner
//! start, which is the same authority that publishes `provider_ready`.

use std::path::PathBuf;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use nexus_agent_host::config::{load_config, save_config, AgentHostConfig};
use nexus_agent_host::providers::validate_provider_config;
use nexus_contracts::{CoreError, CoreErrorCode};

use crate::NativeCore;

/// A client-side rejection in the wire `CoreError` envelope the service maps to
/// 400 `invalid_input` (never the sanitized 500 a bare reason would become).
fn invalid_input(message: impl Into<String>) -> Error {
    crate::core_error::napi_error_from_wire(CoreError {
        code: CoreErrorCode::InvalidInput,
        message: message.into(),
        details: Default::default(),
        http_status: Some(400),
    })
}

fn validate(config: &AgentHostConfig) -> Result<()> {
    if config.policy.unknown_provider != "deny" && config.policy.unknown_provider != "allow" {
        return Err(invalid_input("unknown_provider must be \"deny\" or \"allow\""));
    }
    for provider in &config.providers {
        // The launcher's own admission check is the single source of truth:
        // id non-empty; enabled entries need a supported protocol AND a
        // non-empty command. A save can never accept what the launcher rejects.
        validate_provider_config(provider)
            .map_err(|error| invalid_input(error.to_string()))?;
    }
    Ok(())
}

impl NativeCore {
    pub(crate) fn agent_host_home(&self) -> Result<PathBuf> {
        self.inner
            .user_home
            .lock()
            .map_err(|_| Error::from_reason("home mutex poisoned"))?
            .clone()
            .ok_or_else(|| Error::from_reason("home not available"))
    }
}

#[napi]
impl NativeCore {
    /// Read the agent-host config; a missing file yields the native defaults.
    #[napi]
    pub async fn get_agent_host_config(&self) -> Result<Buffer> {
        self.deny_service_only()?;
        let home = self.agent_host_home()?;
        let config = load_config(&home)
            .map_err(|error| Error::from_reason(format!("load agent host config: {error}")))?;
        Ok(Buffer::from(serde_json::to_vec(&config)?))
    }

    /// Replace the agent-host config file. Unknown-provider policy and every
    /// provider entry are validated before the atomic write.
    #[napi]
    pub async fn save_agent_host_config(&self, config_json: Buffer) -> Result<()> {
        self.deny_service_only()?;
        let home = self.agent_host_home()?;
        let config: AgentHostConfig = serde_json::from_slice(config_json.as_ref())?;
        validate(&config)?;
        save_config(&home, &config)
            .map_err(|error| Error::from_reason(format!("save agent host config: {error}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_agent_host::config::ProviderConfig;

    fn provider(command: Option<&str>, enabled: bool) -> ProviderConfig {
        ProviderConfig {
            id: "claude-acp".to_string(),
            protocol: "acp".to_string(),
            command: command.map(str::to_string),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            enabled,
        }
    }

    fn config_with(provider: ProviderConfig) -> AgentHostConfig {
        AgentHostConfig {
            providers: vec![provider],
            ..AgentHostConfig::default()
        }
    }

    fn wire(error: napi::Error) -> serde_json::Value {
        serde_json::from_str(&error.reason).expect("wire CoreError envelope")
    }

    #[test]
    fn rejects_enabled_provider_without_command_as_invalid_input() {
        let error = validate(&config_with(provider(None, true)))
            .expect_err("an enabled provider without a command must be rejected");
        let wire = wire(error);
        assert_eq!(wire["code"], "invalid_input");
        assert_eq!(wire["http_status"], 400);
        assert!(
            wire["message"]
                .as_str()
                .expect("message")
                .contains("requires a non-empty command"),
            "{wire}"
        );
    }

    #[test]
    fn rejects_enabled_provider_with_unsupported_protocol() {
        let mut invalid = provider(Some("/bin/true"), true);
        invalid.protocol = "shell".to_string();
        let wire = wire(validate(&config_with(invalid)).expect_err("bad protocol"));
        assert_eq!(wire["code"], "invalid_input");
        assert_eq!(wire["http_status"], 400);
    }

    #[test]
    fn accepts_disabled_provider_without_command() {
        // A stale entry disabled by replacement semantics keeps no command but
        // must not block the save: the launcher never selects it.
        validate(&config_with(provider(None, false))).expect("disabled entry is accepted");
    }
}
