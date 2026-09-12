//! Provider adapters: ACP and native CLI.
//!
//! The ACP provider adapter wraps [`nexus_acp_host::AcpSdkAdapter`] behind the
//! [`ProviderAdapter`] trait. Native CLI adapters manage subprocess lifecycles.
//!
//! v1.188 P2: one factory consumes a selected catalog entry and returns the
//! correct adapter implementation.

pub mod acp;
pub mod native_cli;

use std::sync::Arc;

use crate::capability::model::{ProtocolKind, ProviderHealth};
use crate::config::{ProviderConfig, TimeoutConfig};
use crate::error::{HostError, HostResult};
use crate::ids::ProviderId;
use crate::policy::permission::HostPermissionResolver;
use crate::providers::native_cli::{
    claude::ClaudeCliProvider, codex::CodexNativeProvider, dsh::DshNativeProvider,
};
use crate::{LaunchStrategy, ProviderAdapter, ProviderCatalogEntry};

/// Native provider IDs accepted by the factory (architecture §5.1).
pub const NATIVE_PROVIDER_IDS: &[&str] = &["dsh-native", "codex-native", "claude-native"];

/// Health for a configured/discovered candidate that has not yet passed probe.
#[must_use]
pub fn candidate_unavailable_health(provider_id: &ProviderId, message: &str) -> ProviderHealth {
    ProviderHealth {
        provider_id: provider_id.clone(),
        available: false,
        latency_ms: None,
        message: Some(message.to_string()),
    }
}

/// Validate one enabled provider config entry.
///
/// Rejects empty IDs/commands, unknown native IDs, and unsupported argv.
///
/// # Errors
///
/// Returns [`HostError::InternalHostError`] for invalid combinations.
pub fn validate_provider_config(pc: &ProviderConfig) -> HostResult<()> {
    if pc.id.trim().is_empty() {
        return Err(HostError::internal("provider id must be non-empty"));
    }
    if !pc.enabled {
        return Ok(());
    }
    let protocol = pc.protocol.as_str();
    if protocol != "acp" && protocol != "native_cli" {
        return Err(HostError::internal(format!(
            "unsupported protocol '{}' for provider '{}'",
            pc.protocol, pc.id
        )));
    }
    // A command is an executable name, not a shell line: whitespace-only input
    // is empty input.
    if pc
        .command
        .as_deref()
        .is_none_or(|command| command.trim().is_empty())
    {
        return Err(HostError::internal(format!(
            "enabled provider '{}' requires a non-empty command",
            pc.id
        )));
    }
    if protocol == "native_cli" && !NATIVE_PROVIDER_IDS.contains(&pc.id.as_str()) {
        return Err(HostError::internal(format!(
            "unsupported native_cli provider id '{}'; expected one of {}",
            pc.id,
            NATIVE_PROVIDER_IDS.join(", ")
        )));
    }
    if protocol == "native_cli" && !pc.args.is_empty() {
        return Err(HostError::internal(format!(
            "native provider '{}' does not accept configured args",
            pc.id
        )));
    }
    Ok(())
}

/// Validate every provider row in a loaded config.
///
/// # Errors
///
/// Returns on the first invalid enabled provider entry.
pub fn validate_agent_host_config_providers(
    config: &crate::config::AgentHostConfig,
) -> HostResult<()> {
    for pc in &config.providers {
        validate_provider_config(pc)?;
    }
    Ok(())
}

/// The protocol a launch strategy implies — the ONE derivation used to build
/// a [`ProviderConfig`] from a catalog entry.
const fn protocol_of(launch: &LaunchStrategy) -> &'static str {
    match launch {
        LaunchStrategy::Acp { .. } => "acp",
        LaunchStrategy::NativeCli { .. } => "native_cli",
    }
}

/// Build a [`ProviderConfig`] from a selected catalog entry for factory dispatch.
fn provider_config_from_entry(entry: &ProviderCatalogEntry) -> HostResult<ProviderConfig> {
    let (command, args, env) = match &entry.launch {
        LaunchStrategy::Acp { command, args, env }
        | LaunchStrategy::NativeCli { command, args, env } => {
            (command.clone(), args.clone(), env.clone())
        }
    };
    if command.trim().is_empty() {
        return Err(HostError::internal(format!(
            "provider '{}' has empty launch command",
            entry.provider_id
        )));
    }
    Ok(ProviderConfig {
        id: entry.provider_id.0.clone(),
        protocol: protocol_of(&entry.launch).to_string(),
        command: Some(command),
        args,
        env,
        enabled: true,
    })
}

/// Construct a provider adapter from one selected catalog entry.
///
/// Native IDs are exactly `dsh-native`, `codex-native`, and `claude-native`.
/// Generic configured ACP recipes use [`acp::AcpProvider::from_config`].
///
/// # Errors
///
/// Returns when the entry cannot be constructed (unknown native id, unsupported
/// argv, invalid recipe, disabled ACP config, etc.).
pub fn adapter_from_catalog_entry(
    entry: &ProviderCatalogEntry,
    timeouts: TimeoutConfig,
    permission_resolver: HostPermissionResolver,
) -> HostResult<Arc<dyn ProviderAdapter>> {
    let provider_config = provider_config_from_entry(entry)?;
    validate_provider_config(&provider_config)?;

    // The entry's DECLARED protocol must match the protocol its launch
    // strategy implies. Dispatch below selects the adapter from
    // `entry.protocol_kind`, while `provider_config_from_entry` derives
    // validation from the launch strategy — so an inconsistent pair (for
    // example a `NativeCli` entry carrying an ACP launch with the dsh-native
    // id) would validate as one protocol and construct the other. Require
    // agreement instead of silently constructing the wrong adapter.
    let declared = match entry.protocol_kind {
        ProtocolKind::Acp => "acp",
        ProtocolKind::NativeCli => "native_cli",
    };
    let implied = protocol_of(&entry.launch);
    if declared != implied {
        return Err(HostError::internal(format!(
            "provider '{}' declares protocol '{declared}' but carries a '{implied}' launch strategy",
            entry.provider_id
        )));
    }

    match entry.protocol_kind {
        ProtocolKind::Acp => {
            let provider =
                acp::AcpProvider::from_config(provider_config, timeouts, permission_resolver)?;
            Ok(Arc::new(provider))
        }
        ProtocolKind::NativeCli => match entry.provider_id.0.as_str() {
            "dsh-native" => {
                let dsh_bin = Some(provider_config.command.clone().unwrap_or_default());
                let provider = DshNativeProvider::new(
                    entry.provider_id.clone(),
                    entry.display_name.clone(),
                    dsh_bin,
                    &provider_config.args,
                    provider_config.env,
                    timeouts,
                )?;
                Ok(Arc::new(provider))
            }
            "codex-native" => {
                let command = provider_config.command.unwrap_or_default();
                let provider = CodexNativeProvider::new(
                    entry.provider_id.clone(),
                    entry.display_name.clone(),
                    command,
                    provider_config.env,
                    timeouts,
                );
                Ok(Arc::new(provider))
            }
            "claude-native" => {
                let command = provider_config.command.unwrap_or_default();
                let provider = ClaudeCliProvider::new(
                    entry.provider_id.clone(),
                    entry.display_name.clone(),
                    command,
                    provider_config.env,
                    timeouts,
                );
                Ok(Arc::new(provider))
            }
            other => Err(HostError::internal(format!(
                "unsupported native provider id '{other}'"
            ))),
        },
    }
}
