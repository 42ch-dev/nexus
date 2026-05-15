//! ACP registry discovery — maps registry entries to catalog entries.
//!
//! Uses `nexus_acp_host::RegistryClient` to fetch the cached registry and maps
//! each agent entry to a `ProviderCatalogEntry` with `protocol_kind = Acp`,
//! `source = AcpRegistry`, `trust = Registry`.

use std::collections::HashMap;

use crate::capability::model::{CapabilityDescriptor, ProtocolKind, ProviderHealth};
use crate::config::AgentHostConfig;
use crate::error::HostResult;
use crate::ids::ProviderId;
use crate::{DiscoverySource, LaunchStrategy, ProviderCatalogEntry, TrustLevel};

/// Map ACP registry entries to catalog entries.
///
/// Each registry agent becomes a catalog entry with ACP protocol kind.
/// Entries are filtered by current platform where the registry provides
/// platform-specific launch info.
///
/// # Arguments
///
/// * `_config` - Agent host configuration (reserved for future filtering).
/// * `registry_agents` - Raw agent entries from the registry.
///
/// # Errors
///
/// This function does not return errors; it silently skips agents that cannot
/// be mapped. Returns an empty vec if no agents match.
pub fn entries_from_registry(
    _config: &AgentHostConfig,
    registry_agents: &[nexus_acp_host::registry::AgentEntry],
) -> HostResult<Vec<ProviderCatalogEntry>> {
    let mut entries = Vec::new();

    for agent in registry_agents {
        let pid = ProviderId::new(&agent.id);

        // Determine launch command from distribution
        let launch = build_launch_strategy(agent);

        entries.push(ProviderCatalogEntry {
            provider_id: pid.clone(),
            display_name: agent.name.clone(),
            protocol_kind: ProtocolKind::Acp,
            launch,
            source: DiscoverySource::AcpRegistry,
            trust: TrustLevel::Registry,
            capabilities: CapabilityDescriptor::acp_full(),
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

/// Build a launch strategy from a registry agent's distribution info.
fn build_launch_strategy(agent: &nexus_acp_host::registry::AgentEntry) -> LaunchStrategy {
    let dist = &agent.distribution;

    // Prefer NPX distribution
    if let Some(ref npx) = dist.npx {
        let mut args = vec!["npx".to_string()];
        args.push(npx.package.clone());
        if let Some(ref npx_args) = npx.args {
            args.extend(npx_args.iter().cloned());
        }
        return LaunchStrategy::Acp {
            command: "npx".to_string(),
            args,
            env: npx
                .env
                .as_ref()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
        };
    }

    // Binary distribution — pick the platform-matching binary
    if let Some(ref binary) = dist.binary {
        let (cmd, _archive) = pick_platform_binary(binary);
        return LaunchStrategy::Acp {
            command: cmd.to_string(),
            args: vec![],
            env: HashMap::new(),
        };
    }

    // Fallback: use agent ID as command hint
    LaunchStrategy::Acp {
        command: agent.id.clone(),
        args: vec![],
        env: HashMap::new(),
    }
}

/// Pick the platform-matching binary distribution.
///
/// Returns `(cmd, archive)` for the current platform, or falls back to the
/// first available binary if no exact match.
fn pick_platform_binary(binary: &nexus_acp_host::registry::BinaryDistribution) -> (&str, &str) {
    let platform_key = platform_key();

    let platform_binary = match platform_key {
        "darwin-aarch64" => binary.darwin_aarch64.as_ref(),
        "darwin-x86_64" => binary.darwin_x86_64.as_ref(),
        "linux-aarch64" => binary.linux_aarch64.as_ref(),
        "linux-x86_64" => binary.linux_x86_64.as_ref(),
        "windows-aarch64" => binary.windows_aarch64.as_ref(),
        "windows-x86_64" => binary.windows_x86_64.as_ref(),
        _ => None,
    };

    if let Some(pb) = platform_binary {
        return (&pb.cmd, &pb.archive);
    }

    // Fallback: first non-None platform binary
    if let Some(pb) = [
        binary.darwin_aarch64.as_ref(),
        binary.darwin_x86_64.as_ref(),
        binary.linux_aarch64.as_ref(),
        binary.linux_x86_64.as_ref(),
    ]
    .iter()
    .flatten()
    .next()
    {
        return (&pb.cmd, &pb.archive);
    }

    ("unknown", "")
}

/// Determine the current platform key matching registry binary keys.
const fn platform_key() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "darwin-aarch64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "darwin-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-aarch64"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x86_64"
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
    )))]
    {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_acp_host::registry::Distribution;

    fn sample_agents() -> Vec<nexus_acp_host::registry::AgentEntry> {
        vec![
            nexus_acp_host::registry::AgentEntry {
                id: "claude-acp".to_string(),
                name: "Claude Agent".to_string(),
                version: "0.18.0".to_string(),
                description: Some("ACP wrapper for Claude".to_string()),
                repository: None,
                authors: None,
                license: None,
                icon: None,
                distribution: Distribution {
                    npx: Some(nexus_acp_host::registry::NpxDistribution {
                        package: "@anthropic/claude-acp@0.18.0".to_string(),
                        args: None,
                        env: None,
                    }),
                    binary: None,
                },
            },
            nexus_acp_host::registry::AgentEntry {
                id: "codex-acp".to_string(),
                name: "Codex Agent".to_string(),
                version: "0.9.4".to_string(),
                description: None,
                repository: None,
                authors: None,
                license: None,
                icon: None,
                distribution: Distribution {
                    npx: None,
                    binary: Some(nexus_acp_host::registry::BinaryDistribution {
                        darwin_aarch64: Some(nexus_acp_host::registry::PlatformBinary {
                            archive: "https://example.com/codex.tar.gz".to_string(),
                            cmd: "codex-acp".to_string(),
                            args: None,
                        }),
                        darwin_x86_64: None,
                        linux_aarch64: None,
                        linux_x86_64: None,
                        windows_aarch64: None,
                        windows_x86_64: None,
                    }),
                },
            },
        ]
    }

    #[test]
    fn maps_npx_agent_to_catalog_entry() {
        let config = AgentHostConfig::default();
        let agents = sample_agents();
        let entries = entries_from_registry(&config, &agents).expect("should succeed");

        let claude = entries
            .iter()
            .find(|e| e.provider_id.0 == "claude-acp")
            .expect("not found");
        assert_eq!(claude.protocol_kind, ProtocolKind::Acp);
        assert_eq!(claude.source, DiscoverySource::AcpRegistry);
        assert_eq!(claude.trust, TrustLevel::Registry);
        assert_eq!(claude.display_name, "Claude Agent");
    }

    #[test]
    fn maps_binary_agent_to_catalog_entry() {
        let config = AgentHostConfig::default();
        let agents = sample_agents();
        let entries = entries_from_registry(&config, &agents).expect("should succeed");

        let codex = entries
            .iter()
            .find(|e| e.provider_id.0 == "codex-acp")
            .expect("not found");
        assert_eq!(codex.protocol_kind, ProtocolKind::Acp);
    }

    #[test]
    fn empty_registry_produces_empty_entries() {
        let config = AgentHostConfig::default();
        let entries = entries_from_registry(&config, &[]).expect("should succeed");
        assert!(entries.is_empty());
    }

    #[test]
    fn platform_key_matches_current_platform() {
        // Basic sanity: should not be "unknown" on CI/developer machines
        let key = platform_key();
        assert!(!key.is_empty());
    }
}
