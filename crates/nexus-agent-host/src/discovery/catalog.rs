//! Provider catalog — deterministic merge of config, PATH, and ACP registry entries.
//!
//! Priority: explicit config > PATH scan > ACP registry (compass §Discovery mechanism).
//! Deduplication applies per `provider_id`; disabled config entries suppress matching
//! auto-discovered entries.

use std::collections::HashSet;

use crate::capability::model::{CapabilityDescriptor, ProtocolKind, ProviderHealth};
use crate::config::AgentHostConfig;
use crate::error::HostResult;
use crate::ids::ProviderId;
use crate::{DiscoverySource, LaunchStrategy, ProviderCatalogEntry, TrustLevel};

/// Builder that merges config, PATH, and ACP registry entries deterministically.
#[derive(Debug, Clone, Default)]
pub struct ProviderCatalog {
    /// All discovered provider entries (deterministic order).
    pub entries: Vec<ProviderCatalogEntry>,
}

impl ProviderCatalog {
    /// Create an empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a catalog from explicit config, PATH scan, and ACP registry entries.
    ///
    /// Merging rules:
    /// 1. Config entries are added first (in config order).
    /// 2. PATH scan entries that don't collide with config `provider_ids` are added.
    /// 3. ACP registry entries that don't collide with config or PATH `provider_ids` are added.
    /// 4. Disabled config entries suppress matching auto-discovered entries.
    /// 5. Final ordering is deterministic: config → PATH (sorted by id) → registry (sorted by id).
    ///
    /// # Errors
    ///
    /// Returns `HostError` if any config provider has an invalid protocol string.
    pub fn build_from_sources(
        config: &AgentHostConfig,
        path_entries: Vec<ProviderCatalogEntry>,
        registry_entries: Vec<ProviderCatalogEntry>,
    ) -> HostResult<Self> {
        let mut entries = Vec::new();
        let mut seen_ids = HashSet::new();
        let mut suppressed_ids = HashSet::new();

        // Phase 1: config entries (enabled first, track disabled for suppression)
        for provider_config in &config.providers {
            let pid = ProviderId::new(&provider_config.id);
            if provider_config.enabled {
                let protocol_kind = provider_config.protocol_kind()?;
                let launch = match protocol_kind {
                    ProtocolKind::Acp => LaunchStrategy::Acp {
                        command: provider_config.command.clone().unwrap_or_default(),
                        args: provider_config.args.clone(),
                        env: provider_config.env.clone(),
                    },
                    ProtocolKind::NativeCli => LaunchStrategy::NativeCli {
                        command: provider_config.command.clone().unwrap_or_default(),
                        args: provider_config.args.clone(),
                        env: provider_config.env.clone(),
                    },
                };
                let caps = match protocol_kind {
                    ProtocolKind::Acp => CapabilityDescriptor::acp_full(),
                    ProtocolKind::NativeCli => CapabilityDescriptor::native_cli_limited(),
                };
                entries.push(ProviderCatalogEntry {
                    provider_id: pid.clone(),
                    display_name: provider_config.id.clone(),
                    protocol_kind,
                    launch,
                    source: DiscoverySource::Config,
                    trust: TrustLevel::Explicit,
                    capabilities: caps,
                    health: ProviderHealth {
                        provider_id: pid.clone(),
                        available: true,
                        latency_ms: None,
                        message: None,
                    },
                });
                seen_ids.insert(pid);
            } else {
                // Disabled: suppress auto-discovered entries for this provider_id
                suppressed_ids.insert(pid);
            }
        }

        // Phase 2: PATH scan entries (sorted by provider_id)
        let mut sorted_path = path_entries;
        sorted_path.sort_by(|a, b| a.provider_id.0.cmp(&b.provider_id.0));

        for entry in sorted_path {
            if seen_ids.contains(&entry.provider_id) || suppressed_ids.contains(&entry.provider_id)
            {
                continue;
            }
            seen_ids.insert(entry.provider_id.clone());
            entries.push(entry);
        }

        // Phase 3: ACP registry entries (sorted by provider_id)
        let mut sorted_registry = registry_entries;
        sorted_registry.sort_by(|a, b| a.provider_id.0.cmp(&b.provider_id.0));

        for entry in sorted_registry {
            if seen_ids.contains(&entry.provider_id) || suppressed_ids.contains(&entry.provider_id)
            {
                continue;
            }
            seen_ids.insert(entry.provider_id.clone());
            entries.push(entry);
        }

        Ok(Self { entries })
    }

    /// Find an entry by provider ID.
    #[must_use]
    pub fn find(&self, provider_id: &ProviderId) -> Option<&ProviderCatalogEntry> {
        self.entries.iter().find(|e| e.provider_id == *provider_id)
    }

    /// Return entries filtered by protocol kind.
    #[must_use]
    pub fn by_protocol(&self, kind: ProtocolKind) -> Vec<&ProviderCatalogEntry> {
        self.entries
            .iter()
            .filter(|e| e.protocol_kind == kind)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn default_config() -> AgentHostConfig {
        AgentHostConfig::default()
    }

    fn config_with_provider(id: &str, protocol: &str, enabled: bool) -> AgentHostConfig {
        AgentHostConfig {
            providers: vec![crate::config::ProviderConfig {
                id: id.to_string(),
                protocol: protocol.to_string(),
                command: Some("test-cmd".to_string()),
                args: vec![],
                env: HashMap::new(),
                enabled,
            }],
            ..AgentHostConfig::default()
        }
    }

    fn path_entry(id: &str) -> ProviderCatalogEntry {
        ProviderCatalogEntry {
            provider_id: ProviderId::new(id),
            display_name: id.to_string(),
            protocol_kind: ProtocolKind::NativeCli,
            launch: LaunchStrategy::NativeCli {
                command: id.to_string(),
                args: vec![],
                env: HashMap::new(),
            },
            source: DiscoverySource::PathScan,
            trust: TrustLevel::LocalPath,
            capabilities: CapabilityDescriptor::native_cli_limited(),
            health: ProviderHealth {
                provider_id: ProviderId::new(id),
                available: true,
                latency_ms: None,
                message: None,
            },
        }
    }

    fn registry_entry(id: &str) -> ProviderCatalogEntry {
        ProviderCatalogEntry {
            provider_id: ProviderId::new(id),
            display_name: format!("{id} (registry)"),
            protocol_kind: ProtocolKind::Acp,
            launch: LaunchStrategy::Acp {
                command: format!("npx {id}"),
                args: vec![],
                env: HashMap::new(),
            },
            source: DiscoverySource::AcpRegistry,
            trust: TrustLevel::Registry,
            capabilities: CapabilityDescriptor::acp_full(),
            health: ProviderHealth {
                provider_id: ProviderId::new(id),
                available: true,
                latency_ms: None,
                message: None,
            },
        }
    }

    #[test]
    fn empty_sources_produce_empty_catalog() {
        let config = default_config();
        let catalog =
            ProviderCatalog::build_from_sources(&config, vec![], vec![]).expect("should succeed");
        assert!(catalog.entries.is_empty());
    }

    #[test]
    fn config_entries_appear_first() {
        let config = config_with_provider("my-provider", "acp", true);
        let catalog = ProviderCatalog::build_from_sources(
            &config,
            vec![path_entry("path-provider")],
            vec![registry_entry("reg-provider")],
        )
        .expect("should succeed");

        assert_eq!(catalog.entries.len(), 3);
        assert_eq!(catalog.entries[0].provider_id.0, "my-provider");
        assert_eq!(catalog.entries[0].source, DiscoverySource::Config);
    }

    #[test]
    fn path_and_registry_sorted_by_id() {
        let config = default_config();
        let catalog = ProviderCatalog::build_from_sources(
            &config,
            vec![path_entry("z-path"), path_entry("a-path")],
            vec![registry_entry("z-reg"), registry_entry("a-reg")],
        )
        .expect("should succeed");

        assert_eq!(catalog.entries.len(), 4);
        // PATH entries first, sorted
        assert_eq!(catalog.entries[0].provider_id.0, "a-path");
        assert_eq!(catalog.entries[1].provider_id.0, "z-path");
        // Registry entries second, sorted
        assert_eq!(catalog.entries[2].provider_id.0, "a-reg");
        assert_eq!(catalog.entries[3].provider_id.0, "z-reg");
    }

    #[test]
    fn disabled_config_suppresses_matching_auto_discovery() {
        let config = config_with_provider("claude-native", "native_cli", false);
        let catalog =
            ProviderCatalog::build_from_sources(&config, vec![path_entry("claude-native")], vec![])
                .expect("should succeed");

        assert!(
            catalog.entries.is_empty(),
            "disabled config should suppress PATH entry"
        );
    }

    #[test]
    fn config_wins_over_path_collision() {
        let config = config_with_provider("claude-native", "native_cli", true);
        let catalog =
            ProviderCatalog::build_from_sources(&config, vec![path_entry("claude-native")], vec![])
                .expect("should succeed");

        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.entries[0].source, DiscoverySource::Config);
    }

    #[test]
    fn distinct_ids_coexist() {
        // R-004, R-008: ACP registry 'claude' and PATH 'claude-native' coexist
        let config = default_config();
        let catalog = ProviderCatalog::build_from_sources(
            &config,
            vec![path_entry("claude-native")],
            vec![registry_entry("claude")],
        )
        .expect("should succeed");

        assert_eq!(catalog.entries.len(), 2);
        let ids: Vec<&str> = catalog
            .entries
            .iter()
            .map(|e| e.provider_id.0.as_str())
            .collect();
        assert!(ids.contains(&"claude"));
        assert!(ids.contains(&"claude-native"));
    }

    #[test]
    fn find_by_provider_id() {
        let config = default_config();
        let catalog =
            ProviderCatalog::build_from_sources(&config, vec![path_entry("test-provider")], vec![])
                .expect("should succeed");

        assert!(catalog.find(&ProviderId::new("test-provider")).is_some());
        assert!(catalog.find(&ProviderId::new("nonexistent")).is_none());
    }

    #[test]
    fn filter_by_protocol_kind() {
        let config = default_config();
        let catalog = ProviderCatalog::build_from_sources(
            &config,
            vec![path_entry("native-provider")],
            vec![registry_entry("acp-provider")],
        )
        .expect("should succeed");

        let native = catalog.by_protocol(ProtocolKind::NativeCli);
        assert_eq!(native.len(), 1);
        assert_eq!(native[0].provider_id.0, "native-provider");

        let acp = catalog.by_protocol(ProtocolKind::Acp);
        assert_eq!(acp.len(), 1);
        assert_eq!(acp[0].provider_id.0, "acp-provider");
    }
}
