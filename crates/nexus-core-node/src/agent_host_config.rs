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

use crate::NativeCore;

fn validate(config: &AgentHostConfig) -> Result<()> {
    if config.policy.unknown_provider != "deny" && config.policy.unknown_provider != "allow" {
        return Err(Error::from_reason(
            "unknown_provider must be \"deny\" or \"allow\"",
        ));
    }
    for provider in &config.providers {
        if provider.id.trim().is_empty() {
            return Err(Error::from_reason("provider id is required"));
        }
        if provider.protocol != "acp" && provider.protocol != "native_cli" {
            return Err(Error::from_reason(format!(
                "provider '{}' has unsupported protocol '{}' (expected acp or native_cli)",
                provider.id, provider.protocol
            )));
        }
    }
    Ok(())
}

impl NativeCore {
    fn agent_host_home(&self) -> Result<PathBuf> {
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
