//! Config-based discovery — parse explicit provider config entries into catalog entries.

use crate::capability::model::{CapabilityDescriptor, ProtocolKind, ProviderHealth};
use crate::config::AgentHostConfig;
use crate::error::HostResult;
use crate::ids::ProviderId;
use crate::{DiscoverySource, LaunchStrategy, ProviderCatalogEntry, TrustLevel};

/// Parse explicit provider config entries into catalog entries.
///
/// Only enabled providers are returned. Callers should also track disabled
/// entries to suppress matching auto-discovered entries.
///
/// # Errors
///
/// Returns an error if any provider config has an invalid protocol string.
pub fn entries_from_config(config: &AgentHostConfig) -> HostResult<Vec<ProviderCatalogEntry>> {
    let mut entries = Vec::new();
    for pc in &config.providers {
        if !pc.enabled {
            continue;
        }
        let protocol_kind = pc.protocol_kind()?;
        let launch = match protocol_kind {
            ProtocolKind::Acp => LaunchStrategy::Acp {
                command: pc.command.clone().unwrap_or_default(),
                args: pc.args.clone(),
                env: pc.env.clone(),
            },
            ProtocolKind::NativeCli => LaunchStrategy::NativeCli {
                command: pc.command.clone().unwrap_or_default(),
                args: pc.args.clone(),
                env: pc.env.clone(),
            },
        };
        let caps = match protocol_kind {
            ProtocolKind::Acp => CapabilityDescriptor::acp_full(),
            ProtocolKind::NativeCli => CapabilityDescriptor::native_cli_limited(),
        };
        let pid = pc.provider_id();
        entries.push(ProviderCatalogEntry {
            provider_id: pid.clone(),
            display_name: pc.id.clone(),
            protocol_kind,
            launch,
            source: DiscoverySource::Config,
            trust: TrustLevel::Explicit,
            capabilities: caps,
            health: ProviderHealth {
                provider_id: pid,
                available: true,
                latency_ms: None,
                message: None,
            },
        });
    }
    Ok(entries)
}

/// Return provider IDs that should be suppressed from auto-discovery.
///
/// A disabled config entry for `provider_id` "foo" means that PATH scan and
/// ACP registry entries for "foo" should not appear in the final catalog.
#[must_use]
pub fn suppressed_ids(config: &AgentHostConfig) -> Vec<ProviderId> {
    config
        .providers
        .iter()
        .filter(|pc| !pc.enabled)
        .map(super::super::config::ProviderConfig::provider_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_config(providers: Vec<crate::config::ProviderConfig>) -> AgentHostConfig {
        AgentHostConfig {
            providers,
            ..AgentHostConfig::default()
        }
    }

    #[test]
    fn no_providers_returns_empty() {
        let config = make_config(vec![]);
        let entries = entries_from_config(&config).expect("should succeed");
        assert!(entries.is_empty());
    }

    #[test]
    fn enabled_provider_produces_entry() {
        let config = make_config(vec![crate::config::ProviderConfig {
            id: "claude-native".to_string(),
            protocol: "native_cli".to_string(),
            command: Some("claude".to_string()),
            args: vec!["-p".to_string()],
            env: HashMap::new(),
            enabled: true,
        }]);
        let entries = entries_from_config(&config).expect("should succeed");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].provider_id.0, "claude-native");
        assert_eq!(entries[0].source, DiscoverySource::Config);
        assert_eq!(entries[0].trust, TrustLevel::Explicit);
        assert_eq!(entries[0].protocol_kind, ProtocolKind::NativeCli);
    }

    #[test]
    fn disabled_provider_skipped() {
        let config = make_config(vec![crate::config::ProviderConfig {
            id: "claude-native".to_string(),
            protocol: "native_cli".to_string(),
            command: Some("claude".to_string()),
            args: vec![],
            env: HashMap::new(),
            enabled: false,
        }]);
        let entries = entries_from_config(&config).expect("should succeed");
        assert!(entries.is_empty());
    }

    #[test]
    fn suppressed_ids_returns_disabled() {
        let config = make_config(vec![
            crate::config::ProviderConfig {
                id: "enabled".to_string(),
                protocol: "native_cli".to_string(),
                command: Some("cmd".to_string()),
                args: vec![],
                env: HashMap::new(),
                enabled: true,
            },
            crate::config::ProviderConfig {
                id: "disabled".to_string(),
                protocol: "native_cli".to_string(),
                command: Some("cmd".to_string()),
                args: vec![],
                env: HashMap::new(),
                enabled: false,
            },
        ]);
        let suppressed = suppressed_ids(&config);
        assert_eq!(suppressed.len(), 1);
        assert_eq!(suppressed[0].0, "disabled");
    }

    #[test]
    fn invalid_protocol_returns_error() {
        let config = make_config(vec![crate::config::ProviderConfig {
            id: "bad".to_string(),
            protocol: "unknown".to_string(),
            command: None,
            args: vec![],
            env: HashMap::new(),
            enabled: true,
        }]);
        assert!(entries_from_config(&config).is_err());
    }
}
