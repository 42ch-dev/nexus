//! Provider readiness helpers — candidate identity and catalog merge.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::capability::model::{ProbeRequest, ProviderHealth, SessionOwner};
use crate::config::TimeoutConfig;
use crate::error::{HostError, HostResult};
use crate::ids::ProviderId;
use crate::policy::permission::HostPermissionResolver;
use crate::providers::{adapter_from_catalog_entry, candidate_unavailable_health};
use crate::{DiscoverySource, LaunchStrategy, ProviderAdapter, ProviderCatalogEntry};

/// Stable identity for a catalog candidate used to reject stale probe results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateIdentity {
    pub provider_id: ProviderId,
    pub source: DiscoverySource,
    pub launch_fingerprint: u64,
}

impl CandidateIdentity {
    #[must_use]
    pub fn from_metadata(metadata: &ProviderCatalogEntry) -> Self {
        Self {
            provider_id: metadata.provider_id.clone(),
            source: metadata.source.clone(),
            launch_fingerprint: fingerprint_launch(&metadata.launch),
        }
    }

    #[must_use]
    pub fn from_registration(provider_id: ProviderId, launch: &LaunchStrategy) -> Self {
        Self {
            provider_id,
            source: DiscoverySource::Config,
            launch_fingerprint: fingerprint_launch(launch),
        }
    }
}

/// One manager-owned provider row: metadata, optional adapter, live health.
pub struct ProviderEntry {
    pub metadata: ProviderCatalogEntry,
    pub adapter: Option<Arc<dyn ProviderAdapter>>,
    pub health: ProviderHealth,
    pub identity: CandidateIdentity,
}

impl ProviderEntry {
    #[must_use]
    pub const fn is_available(&self) -> bool {
        self.health.available
    }

    #[must_use]
    pub fn from_metadata(
        metadata: ProviderCatalogEntry,
        adapter: Option<Arc<dyn ProviderAdapter>>,
        unavailable_message: &str,
    ) -> Self {
        let identity = CandidateIdentity::from_metadata(&metadata);
        let health = if adapter.is_some() {
            candidate_unavailable_health(&metadata.provider_id, unavailable_message)
        } else {
            ProviderHealth {
                provider_id: metadata.provider_id.clone(),
                available: false,
                latency_ms: None,
                message: Some(unavailable_message.to_string()),
            }
        };
        Self {
            metadata,
            adapter,
            health,
            identity,
        }
    }

    #[must_use]
    pub fn from_registration(adapter: Arc<dyn ProviderAdapter>, launch: LaunchStrategy) -> Self {
        let desc = adapter.descriptor();
        let health = candidate_unavailable_health(
            &desc.provider_id,
            "registered candidate; bounded probe required",
        );
        let metadata = ProviderCatalogEntry {
            provider_id: desc.provider_id.clone(),
            display_name: desc.display_name.clone(),
            protocol_kind: desc.protocol_kind,
            launch,
            source: DiscoverySource::Config,
            trust: crate::TrustLevel::Explicit,
            capabilities: desc.capabilities,
            health: health.clone(),
        };
        let identity = CandidateIdentity::from_registration(desc.provider_id, &metadata.launch);
        Self {
            metadata,
            adapter: Some(adapter),
            health,
            identity,
        }
    }
}

#[must_use]
pub fn fingerprint_launch(launch: &LaunchStrategy) -> u64 {
    let mut hasher = DefaultHasher::new();
    match launch {
        LaunchStrategy::Acp { command, args, env } => {
            "acp".hash(&mut hasher);
            command.hash(&mut hasher);
            args.hash(&mut hasher);
            for (k, v) in env {
                k.hash(&mut hasher);
                v.hash(&mut hasher);
            }
        }
        LaunchStrategy::NativeCli { command, args, env } => {
            "native".hash(&mut hasher);
            command.hash(&mut hasher);
            args.hash(&mut hasher);
            for (k, v) in env {
                k.hash(&mut hasher);
                v.hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

/// Discover catalog candidates and construct adapters (no probe).
///
/// # Errors
///
/// Returns a [`HostError`] when the configured provider set is invalid, when
/// the PATH scan cannot read the process `PATH`, or when the catalog merge
/// rejects an entry. Adapter construction failures are NOT errors: that
/// provider is admitted as an unavailable candidate so the catalog can still
/// explain why it is not ready.
pub fn discover_provider_entries(
    host_config: &crate::config::AgentHostConfig,
    timeouts: &TimeoutConfig,
    permission_resolver: &HostPermissionResolver,
) -> HostResult<std::collections::HashMap<ProviderId, ProviderEntry>> {
    use crate::discovery::{catalog::ProviderCatalog, config, path_scan};

    crate::providers::validate_agent_host_config_providers(host_config)?;

    let suppressed = config::suppressed_ids(host_config);
    let path_entries = path_scan::scan_path(host_config, &suppressed)?;
    let catalog = ProviderCatalog::build_from_sources(host_config, path_entries, Vec::new())?;

    let mut providers = std::collections::HashMap::new();
    for metadata in catalog.entries {
        let adapter = match adapter_from_catalog_entry(
            &metadata,
            timeouts.clone(),
            permission_resolver.clone(),
        ) {
            Ok(adapter) => Some(adapter),
            Err(error) => {
                tracing::warn!(
                    provider = %metadata.provider_id,
                    error = %error,
                    "skipping provider adapter construction"
                );
                None
            }
        };
        let message = if adapter.is_some() {
            "candidate configured; bounded probe required"
        } else {
            "candidate adapter construction failed"
        };
        let entry = ProviderEntry::from_metadata(metadata, adapter, message);
        providers.insert(entry.metadata.provider_id.clone(), entry);
    }
    Ok(providers)
}

/// Build a probe request from verified owner context and timeout budget.
#[must_use]
pub fn probe_request_for_owner(
    owner: &SessionOwner,
    probe_cwd: &std::path::Path,
    timeout_ms: u64,
) -> ProbeRequest {
    ProbeRequest {
        timeout_ms,
        cwd: probe_cwd.to_path_buf(),
        owner: owner.clone(),
    }
}

/// Map probe/launch failures to safe diagnostic text (no subprocess/env dumps).
#[must_use]
pub fn safe_provider_message(error: &HostError) -> String {
    match error.category() {
        "operation_timeout" => "operation timed out".to_string(),
        "cleanup_unconfirmed" => "cleanup unconfirmed".to_string(),
        "launch_failed" => "launch failed".to_string(),
        "provider_unavailable" => "provider unavailable".to_string(),
        _ => error.category().to_string(),
    }
}

/// Whether a session launch failure should invalidate provider readiness.
#[must_use]
pub fn is_launch_class_failure(error: &HostError) -> bool {
    match error {
        HostError::LaunchFailed { .. }
        | HostError::CleanupUnconfirmed { .. }
        | HostError::ProviderUnavailable { .. } => true,
        // A timeout only invalidates readiness when it happened at a real
        // launch/initialize boundary. The typed `stage` distinguishes those
        // from an ordinary prompt/content/run timeout, which leaves an
        // already-initialized recipe healthy — blanket-classifying every
        // `operation_timeout` would tear down a provider that still works.
        HostError::OperationTimeout { stage, .. } => {
            matches!(stage.as_str(), "launch" | "initialize" | "session")
        }
        _ => false,
    }
}
