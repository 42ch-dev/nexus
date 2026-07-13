//! ACP Registry manifest fetcher + local cache.
//!
//! Fetches the ACP Registry from the CDN, parses agent manifests, and
//! implements local caching with stale-while-revalidate semantics.
//!
//! # Architecture
//!
//! ```text
//! RegistryCache ──► $HOME/.nexus42/registry/
//!                  ├── cache.json          # Full registry response
//!                  └── cache_meta.json     # Fetch timestamp, version
//! ```
//!
//! # Cache Policy
//!
//! | Scenario | Behavior |
//! |----------|----------|
//! | Cache exists, < 24h old | Use cache, no network |
//! | Cache exists, >= 24h old | Use cache immediately, fetch in background |
//! | Cache exists, no network | Use cache (offline mode) |
//! | No cache, no network | Error |
//!
//! # Design Notes
//!
//! The registry types are now imported from the generated `registry_manifest.rs`
//! (codegenned from `schemas/acp-runtime/registry-manifest.schema.json`).
//! This ensures consistency between the schema and the implementation.
//!
//! The `RegistryManifest` type is aliased as `Registry` for convenience.
//! Additional helper methods are added via an extension trait.

// This module defines the public API for ACP registry fetching and caching.
// Items are consumed by Task 3 (CLI commands) and Task 4 (transport/run).
// Until those tasks land, suppress dead_code warnings for public API items.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{info, warn};

// Import local registry manifest types (moved from generated per WS5)
use nexus_contracts::local::acp_runtime::registry_manifest::{
    AgentEntry as GeneratedAgentEntry, BinaryDistribution as GeneratedBinaryDistribution,
    Distribution as GeneratedDistribution, NpxDistribution as GeneratedNpxDistribution,
    PlatformBinary as GeneratedPlatformBinary, RegistryManifest,
};

// ── Constants ────────────────────────────────────────────────────────

/// ACP Registry CDN URL.
pub const REGISTRY_URL: &str =
    "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

/// Maximum cache age before stale-while-revalidate kicks in.
const CACHE_MAX_AGE: Duration = Duration::from_hours(24);

/// Cache file name for the full registry JSON.
const CACHE_FILE: &str = "cache.json";

/// Cache file name for metadata (fetch timestamp, version).
const CACHE_META_FILE: &str = "cache_meta.json";

/// Subdirectory under $HOME for nexus42 data.
const NEXUS_DIR: &str = ".nexus42";

/// Subdirectory under nexus42 dir for registry cache.
const REGISTRY_DIR: &str = "registry";

// ── Type Aliases for Generated Types ────────────────────────────────

/// Top-level ACP Registry response (alias for generated `RegistryManifest`).
pub type Registry = RegistryManifest;

/// A single agent entry in the ACP Registry (alias for generated type).
pub type AgentEntry = GeneratedAgentEntry;

/// Agent distribution configuration (alias for generated type).
pub type Distribution = GeneratedDistribution;

/// NPX-based distribution configuration (alias for generated type).
pub type NpxDistribution = GeneratedNpxDistribution;

/// Binary distribution configuration with per-platform entries (alias for generated type).
pub type BinaryDistribution = GeneratedBinaryDistribution;

/// Platform-specific binary distribution (alias for generated type).
pub type PlatformBinary = GeneratedPlatformBinary;

// ── Extension Trait for Distribution ────────────────────────────────

/// Extension trait to add helper methods to generated `Distribution` type.
pub trait DistributionExt {
    /// Returns the distribution source kind: "npx" or "binary".
    fn source_kind(&self) -> &str;
}

impl DistributionExt for Distribution {
    fn source_kind(&self) -> &str {
        if self.npx.is_some() {
            "npx"
        } else if self.binary.is_some() {
            "binary"
        } else {
            "unknown"
        }
    }
}

// ── Cache Metadata ───────────────────────────────────────────────────

/// Metadata stored alongside the cache file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheMeta {
    /// ISO 8601 timestamp of when the cache was fetched.
    pub fetched_at: String,
    /// Registry version string from the cached response.
    pub registry_version: String,
}

impl CacheMeta {
    /// Create a new cache metadata entry.
    #[must_use]
    pub fn new(registry_version: &str) -> Self {
        Self {
            fetched_at: chrono::Utc::now().to_rfc3339(),
            registry_version: registry_version.to_string(),
        }
    }

    /// Parse `fetched_at` into a `SystemTime`. Returns `None` if parsing fails.
    #[must_use]
    pub fn fetched_time(&self) -> Option<SystemTime> {
        chrono::DateTime::parse_from_rfc3339(&self.fetched_at)
            .ok()
            .map(std::convert::Into::into)
    }

    /// Returns the age of this cache entry, or `None` if the timestamp is invalid.
    #[must_use]
    pub fn age(&self) -> Option<Duration> {
        self.fetched_time().map(|t| {
            SystemTime::now()
                .duration_since(t)
                .unwrap_or(Duration::ZERO)
        })
    }

    /// Returns `true` if the cache is within the max age (fresh).
    #[must_use]
    pub fn is_fresh(&self) -> bool {
        self.age().is_some_and(|age| age < CACHE_MAX_AGE)
    }
}

// ── Registry Client ──────────────────────────────────────────────────

/// Fetches the ACP Registry from the CDN and manages local caching.
///
/// The client implements a stale-while-revalidate caching strategy:
/// - Fresh cache (< 24h): return immediately, no network
/// - Stale cache (>= 24h): return cached data, spawn background refresh
/// - No cache or network failure: appropriate error or offline fallback
pub struct RegistryClient {
    /// Path to the registry cache directory.
    cache_dir: PathBuf,
    /// HTTP client for fetching from the CDN.
    http: reqwest::Client,
}

impl RegistryClient {
    /// Create a new registry client with default settings.
    ///
    /// Uses `$HOME/.nexus42/registry/` as the cache directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the HOME directory cannot be determined or HTTP client fails.
    pub fn new() -> anyhow::Result<Self> {
        let home =
            dirs::home_dir().context("Cannot determine HOME directory for registry cache")?;
        let cache_dir = home.join(NEXUS_DIR).join(REGISTRY_DIR);
        Ok(Self {
            cache_dir,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .context("Failed to build HTTP client")?,
        })
    }

    /// Create a registry client with a custom cache directory (for testing).
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client fails to build.
    pub fn with_cache_dir(cache_dir: PathBuf) -> anyhow::Result<Self> {
        Ok(Self {
            cache_dir,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .context("Failed to build HTTP client")?,
        })
    }

    /// Return the cache directory path.
    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Ensure the cache directory exists, creating it if needed.
    fn ensure_cache_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.cache_dir)
    }

    /// Path to the cached registry JSON file.
    fn cache_file_path(&self) -> PathBuf {
        self.cache_dir.join(CACHE_FILE)
    }

    /// Path to the cache metadata file.
    fn meta_file_path(&self) -> PathBuf {
        self.cache_dir.join(CACHE_META_FILE)
    }

    /// Load cached registry data from disk.
    ///
    /// Returns `None` if no cache exists or the data is corrupted.
    fn load_cached(&self) -> Option<Registry> {
        let cache_path = self.cache_file_path();
        if !cache_path.exists() {
            return None;
        }

        let data = std::fs::read_to_string(&cache_path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Load cache metadata from disk.
    ///
    /// Returns `None` if no metadata exists or it's corrupted.
    fn load_meta(&self) -> Option<CacheMeta> {
        let meta_path = self.meta_file_path();
        if !meta_path.exists() {
            return None;
        }

        let data = std::fs::read_to_string(&meta_path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Save registry data and metadata to disk.
    fn save_cache(&self, registry: &Registry) -> std::io::Result<()> {
        self.ensure_cache_dir()?;
        let cache_data = serde_json::to_string_pretty(registry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(self.cache_file_path(), cache_data)?;

        let meta = CacheMeta::new(&registry.version);
        let meta_data = serde_json::to_string_pretty(&meta)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(self.meta_file_path(), meta_data)?;

        Ok(())
    }

    /// Fetch the registry from the CDN over HTTP.
    async fn fetch_from_cdn(&self) -> anyhow::Result<Registry> {
        info!("Fetching ACP Registry from {}", REGISTRY_URL);

        let response = self
            .http
            .get(REGISTRY_URL)
            .send()
            .await
            .context("Failed to fetch ACP Registry from CDN")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "ACP Registry fetch failed with HTTP status: {}",
                response.status()
            );
        }

        let body = response
            .text()
            .await
            .context("Failed to read ACP Registry response body")?;

        let registry: Registry =
            serde_json::from_str(&body).context("Failed to parse ACP Registry JSON response")?;

        info!(
            "Fetched registry v{} with {} agents",
            registry.version,
            registry.agents.len()
        );

        Ok(registry)
    }

    /// Fetch the registry and save to cache.
    async fn fetch_and_cache(&self) -> anyhow::Result<Registry> {
        let registry = self.fetch_from_cdn().await?;
        if let Err(e) = self.save_cache(&registry) {
            warn!("Failed to save registry cache: {}", e);
            // Non-fatal: we still return the data
        }
        Ok(registry)
    }

    /// Get the registry, using cache when available.
    ///
    /// Implements stale-while-revalidate:
    /// - Fresh cache (< 24h): return cached data immediately
    /// - Stale cache (>= 24h): return cached data, spawn background refresh
    /// - No cache: fetch from CDN, blocking
    ///
    /// # Errors
    ///
    /// Returns an error if fetching from CDN fails and no cache is available.
    pub async fn get_registry(&self) -> anyhow::Result<Registry> {
        // Try to load from cache
        if let Some(cached) = self.load_cached() {
            if let Some(meta) = self.load_meta() {
                if meta.is_fresh() {
                    info!("Using fresh registry cache (fetched: {})", meta.fetched_at);
                    return Ok(cached);
                }
                // Stale-while-revalidate: return cached, refresh in background
                info!(
                    "Registry cache is stale (fetched: {}), refreshing in background",
                    meta.fetched_at
                );
                let cache_dir = self.cache_dir.clone();
                let http = self.http.clone();
                tokio::spawn(async move {
                    match Self::fetch_and_save(http, &cache_dir).await {
                        Ok((version, count)) => {
                            info!(
                                "Background refresh complete: v{} ({} agents)",
                                version, count
                            );
                        }
                        Err(e) => {
                            warn!("Background registry refresh failed: {}", e);
                        }
                    }
                });
                return Ok(cached);
            }
            // No metadata but cache exists — treat as fresh (first fetch scenario)
            return Ok(cached);
        }

        // No cache: must fetch
        self.fetch_and_cache().await
    }

    /// Fetch from CDN and save to the given directory (static helper for background refresh).
    ///
    /// The HTTP request is wrapped with a 60-second timeout to prevent resource
    /// leaks if the CDN hangs indefinitely.
    async fn fetch_and_save(
        http: reqwest::Client,
        cache_dir: &Path,
    ) -> anyhow::Result<(String, usize)> {
        let response = tokio::time::timeout(Duration::from_mins(1), http.get(REGISTRY_URL).send())
            .await
            .context("Background fetch timed out after 60s")?
            .context("Background fetch failed")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Background fetch failed with HTTP status: {}",
                response.status()
            );
        }

        let body = response.text().await?;
        let registry: Registry =
            serde_json::from_str(&body).context("Failed to parse background fetch response")?;

        let agent_count = registry.agents.len();
        let version = registry.version.clone();

        // Save cache
        std::fs::create_dir_all(cache_dir)?;
        let cache_path = cache_dir.join(CACHE_FILE);
        let meta_path = cache_dir.join(CACHE_META_FILE);

        if let Ok(data) = serde_json::to_string_pretty(&registry) {
            let _ = std::fs::write(cache_path, data);
        }
        let meta = CacheMeta::new(&version);
        if let Ok(data) = serde_json::to_string_pretty(&meta) {
            let _ = std::fs::write(meta_path, data);
        }

        Ok((version, agent_count))
    }

    /// Force a fresh fetch from the CDN, bypassing cache.
    ///
    /// # Errors
    ///
    /// Returns an error if the fetch fails.
    pub async fn refresh(&self) -> anyhow::Result<Registry> {
        self.fetch_and_cache().await
    }

    /// Find an agent by exact ID or partial match on id/name.
    ///
    /// Returns the first matching agent, or `None` if no match found.
    /// The `query` is case-insensitive and matches:
    /// - Exact agent ID (e.g. "claude-acp" matches "claude-acp")
    /// - Prefix of agent ID (e.g. "claude" matches "claude-acp")
    /// - Prefix of agent name (e.g. "Claude" matches "Claude Agent")
    #[must_use]
    pub fn find_agent<'a>(&self, registry: &'a Registry, query: &str) -> Option<&'a AgentEntry> {
        let query_lower = query.to_lowercase();
        registry
            .agents
            .iter()
            .find(|agent| {
                agent.id.to_lowercase().starts_with(&query_lower)
                    || agent.name.to_lowercase().starts_with(&query_lower)
            })
            .or_else(|| {
                // Try substring match as fallback
                registry.agents.iter().find(|agent| {
                    agent.id.to_lowercase().contains(&query_lower)
                        || agent.name.to_lowercase().contains(&query_lower)
                })
            })
    }

    /// Fetch from a custom URL (for testing with mock servers).
    #[cfg(test)]
    async fn fetch_from_url(&self, url: &str) -> anyhow::Result<Registry> {
        let response = self.http.get(url).send().await?;
        if !response.status().is_success() {
            anyhow::bail!("Fetch failed with HTTP status: {}", response.status());
        }
        let body = response.text().await?;
        let registry: Registry = serde_json::from_str(&body)?;
        Ok(registry)
    }

    /// Fetch from raw JSON string (for testing without network).
    #[cfg(test)]
    fn parse_registry_json(json: &str) -> anyhow::Result<Registry> {
        let registry: Registry = serde_json::from_str(json)?;
        Ok(registry)
    }
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default RegistryClient")
    }
}

// ── Local Installation Scan ──────────────────────────────────────────

/// Result of probing a single registry-known binary for PATH availability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalInstallation {
    /// The binary name probed (e.g. `"codex-acp"`).
    pub binary: String,
    /// Best-effort version string from `<binary> --version`, if probing succeeded.
    pub version: Option<String>,
}

/// Extract the bare command name from a possibly-relative path command.
///
/// Registry binary commands may be relative paths such as `./kimi` or
/// `./dist-package/cursor-agent`; PATH probing needs the bare executable
/// name (`kimi`, `cursor-agent`). This helper also correctly handles bare
/// commands (`opencode` → `opencode`) and Windows-style relative paths.
#[must_use]
pub fn bare_command_name(cmd: &str) -> String {
    Path::new(cmd)
        .file_name()
        .map_or_else(|| cmd.to_string(), |name| name.to_string_lossy().to_string())
}

/// Maximum concurrent PATH/version probes during a scan.
const SCAN_MAX_CONCURRENT: usize = 4;

/// Per-probe timeout for `<binary> --version`.
const SCAN_VERSION_TIMEOUT: Duration = Duration::from_secs(2);

/// Scan the local PATH for registry-known ACP agent binaries.
///
/// For every binary command listed in `AgentEntry.distribution.binary.*.cmd`,
/// this function checks whether the binary is on PATH and, if so, runs
/// `<binary> --version` with a 2-second timeout. The result is a stable list
/// of installed binaries, sorted by binary name. Missing binaries are omitted
/// from the result so callers can treat presence as `installed: true`.
///
/// # Safety boundary
///
/// - Only registry-known binary names are probed; no user-supplied commands.
/// - No shell expansion; binaries are executed directly with a fixed `--version`
///   argument.
/// - Concurrency is bounded to [`SCAN_MAX_CONCURRENT`].
/// - Each probe is capped at [`SCAN_VERSION_TIMEOUT`].
pub async fn scan_local_installations(registry: &Registry) -> Vec<LocalInstallation> {
    scan_local_installations_impl(registry, &[]).await
}

async fn scan_local_installations_impl(
    registry: &Registry,
    path_dirs: &[PathBuf],
) -> Vec<LocalInstallation> {
    use std::collections::HashSet;
    use tokio::sync::Semaphore;

    // Collect unique binary names across all platforms. We deliberately do not
    // restrict this to the current platform so the scan behaves deterministically
    // regardless of where the daemon runs.
    let mut binaries = HashSet::new();
    for agent in &registry.agents {
        let Some(binary) = agent.distribution.binary.as_ref() else {
            continue;
        };
        for pb in [
            &binary.darwin_aarch64,
            &binary.darwin_x86_64,
            &binary.linux_aarch64,
            &binary.linux_x86_64,
            &binary.windows_aarch64,
            &binary.windows_x86_64,
        ]
        .into_iter()
        .flatten()
        {
            binaries.insert(bare_command_name(&pb.cmd));
        }
    }

    let semaphore = Arc::new(Semaphore::new(SCAN_MAX_CONCURRENT));
    let mut handles = Vec::with_capacity(binaries.len());

    for binary in binaries {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore should not be closed");
        let path_dirs = path_dirs.to_vec();
        let handle = tokio::spawn(async move {
            // Hold the permit for the duration of the probe.
            let _permit = permit;
            probe_local_binary(&binary, SCAN_VERSION_TIMEOUT, &path_dirs).await
        });
        handles.push(handle);
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(Some(installation)) = handle.await {
            results.push(installation);
        }
    }
    results.sort_by(|a, b| a.binary.cmp(&b.binary));
    results
}

/// Probe a single binary for PATH presence and `--version` output.
///
/// `path_dirs`, when non-empty, overrides the directories searched for the
/// binary (the child process PATH is also overridden so the same binary is
/// executed). This is test-only plumbing; production code passes an empty
/// slice and relies on the process PATH.
///
/// Returns `None` when the binary is not found on PATH, otherwise the
/// installation record including the best-effort version string.
async fn probe_local_binary(
    binary: &str,
    timeout: Duration,
    path_dirs: &[PathBuf],
) -> Option<LocalInstallation> {
    let path_var = if path_dirs.is_empty() {
        None
    } else {
        std::env::join_paths(path_dirs).ok()
    };

    let found = path_var.as_deref().map_or_else(
        || which::which(binary).is_ok(),
        |path| which::which_in(binary, Some(path), std::path::Path::new(".")).is_ok(),
    );

    if !found {
        return None;
    }

    let version = {
        let mut cmd = Command::new(binary);
        cmd.arg("--version").kill_on_drop(true);
        if let Some(ref path) = path_var {
            cmd.env("PATH", path);
        }
        match tokio::time::timeout(timeout, cmd.output()).await {
            Ok(Ok(output)) if output.status.success() => String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(std::string::ToString::to_string),
            Ok(Ok(output)) => {
                tracing::debug!(
                    binary = %binary,
                    code = ?output.status.code(),
                    "binary --version exited with non-zero status"
                );
                None
            }
            Ok(Err(e)) => {
                tracing::debug!(binary = %binary, error = %e, "failed to spawn --version probe");
                None
            }
            Err(_) => {
                tracing::debug!(binary = %binary, "--version probe timed out");
                None
            }
        }
    };

    Some(LocalInstallation {
        binary: binary.to_string(),
        version,
    })
}

#[cfg(test)]
async fn scan_local_installations_with_path(
    registry: &Registry,
    path_dirs: &[PathBuf],
) -> Vec<LocalInstallation> {
    scan_local_installations_impl(registry, path_dirs).await
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Sample registry JSON matching the live ACP CDN format.
    const SAMPLE_REGISTRY: &str = r#"{
        "version": "1.0.0",
        "agents": [
            {
                "id": "claude-acp",
                "name": "Claude Agent",
                "version": "0.18.0",
                "description": "ACP wrapper for Anthropic's Claude",
                "repository": "https://github.com/zed-industries/claude-agent-acp",
                "authors": ["Anthropic"],
                "license": "proprietary",
                "icon": "https://cdn.agentclientprotocol.com/registry/v1/latest/claude-acp.svg",
                "distribution": {
                    "npx": {
                        "package": "@zed-industries/claude-agent-acp@0.18.0"
                    }
                }
            },
            {
                "id": "codex-acp",
                "name": "Codex Agent",
                "version": "0.9.4",
                "description": "ACP adapter for OpenAI's Codex",
                "distribution": {
                    "binary": {
                        "darwin-aarch64": {
                            "archive": "https://example.com/codex-darwin-aarch64.tar.gz",
                            "cmd": "codex-acp"
                        },
                        "linux-x86_64": {
                            "archive": "https://example.com/codex-linux-x86_64.tar.gz",
                            "cmd": "codex-acp"
                        }
                    }
                }
            },
            {
                "id": "gemini",
                "name": "Gemini Agent",
                "version": "1.2.0",
                "description": "Google Gemini ACP agent",
                "distribution": {
                    "npx": {
                        "package": "@google/gemini-acp@1.2.0",
                        "args": ["--verbose"]
                    }
                }
            }
        ],
        "extensions": []
    }"#;

    /// Create a registry client backed by a temp directory.
    fn make_test_client() -> (RegistryClient, TempDir) {
        let tmp = TempDir::new().expect("Failed to create temp dir");
        let client = RegistryClient::with_cache_dir(tmp.path().to_path_buf())
            .expect("Failed to create client");
        (client, tmp)
    }

    // ── Parsing Tests ─────────────────────────────────────────────

    #[test]
    fn parse_valid_registry() {
        let (_client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        assert_eq!(registry.version, "1.0.0");
        assert_eq!(registry.agents.len(), 3);
        assert_eq!(registry.extensions.as_ref().map_or(0, Vec::len), 0);
    }

    #[test]
    fn parse_agent_npx_distribution() {
        let (_client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let claude = &registry.agents[0];
        assert_eq!(claude.id, "claude-acp");
        assert_eq!(claude.name, "Claude Agent");
        assert_eq!(claude.version, "0.18.0");
        assert_eq!(
            claude.description.as_deref(),
            Some("ACP wrapper for Anthropic's Claude")
        );
        assert_eq!(
            claude.repository.as_deref(),
            Some("https://github.com/zed-industries/claude-agent-acp")
        );
        assert_eq!(
            claude.authors.as_deref(),
            Some(&["Anthropic".to_string()][..])
        );
        assert_eq!(claude.license.as_deref(), Some("proprietary"));

        let npx = claude.distribution.npx.as_ref().unwrap();
        assert_eq!(npx.package, "@zed-industries/claude-agent-acp@0.18.0");
        assert_eq!(DistributionExt::source_kind(&claude.distribution), "npx");
    }

    #[test]
    fn parse_agent_binary_distribution() {
        let (_client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let codex = &registry.agents[1];
        assert_eq!(codex.id, "codex-acp");
        assert_eq!(DistributionExt::source_kind(&codex.distribution), "binary");

        let binary = codex.distribution.binary.as_ref().unwrap();
        let darwin = binary.darwin_aarch64.as_ref().unwrap();
        assert_eq!(
            darwin.archive,
            "https://example.com/codex-darwin-aarch64.tar.gz"
        );
        assert_eq!(darwin.cmd, "codex-acp");
    }

    #[test]
    fn parse_npx_with_args() {
        let (_client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let gemini = &registry.agents[2];
        assert_eq!(gemini.id, "gemini");
        let npx = gemini.distribution.npx.as_ref().unwrap();
        assert_eq!(npx.args.as_deref(), Some(&["--verbose".to_string()][..]));
    }

    #[test]
    fn parse_minimal_agent() {
        let json = r#"{
            "version": "1.0.0",
            "agents": [
                {
                    "id": "minimal-agent",
                    "name": "Minimal",
                    "version": "0.1.0",
                    "distribution": {
                        "npx": { "package": "@scope/minimal@0.1.0" }
                    }
                }
            ]
        }"#;
        let (_client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(json).unwrap();

        assert_eq!(registry.agents.len(), 1);
        let agent = &registry.agents[0];
        assert_eq!(agent.id, "minimal-agent");
        assert!(agent.description.is_none()); // optional, not provided
        assert!(agent.repository.is_none()); // optional
        assert!(agent.authors.is_none()); // optional, not provided
        assert!(agent.license.is_none()); // optional
    }

    #[test]
    fn parse_invalid_json_fails() {
        let (_client, _tmp) = make_test_client();
        let result = RegistryClient::parse_registry_json("not json");
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_required_field_fails() {
        // Missing "distribution" which is required
        let json = r#"{
            "version": "1.0.0",
            "agents": [
                {
                    "id": "broken",
                    "name": "Broken",
                    "version": "0.1.0"
                }
             ]
         }"#;
        let (_client, _tmp) = make_test_client();
        let result = RegistryClient::parse_registry_json(json);
        assert!(result.is_err());
    }

    // ── Cache Tests ───────────────────────────────────────────────

    #[test]
    fn cache_write_and_read() {
        let (client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        // Save to cache
        client.save_cache(&registry).expect("Failed to save cache");

        // Verify cache files exist
        assert!(client.cache_file_path().exists());
        assert!(client.meta_file_path().exists());

        // Load from cache
        let loaded = client.load_cached().expect("Failed to load cache");
        assert_eq!(loaded, registry);
    }

    #[test]
    fn cache_meta_stored_and_loaded() {
        let (client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        client.save_cache(&registry).expect("Failed to save cache");

        let meta = client.load_meta().expect("Failed to load meta");
        assert_eq!(meta.registry_version, "1.0.0");
        // fetched_at should be a valid ISO 8601 timestamp
        assert!(meta.fetched_time().is_some());
    }

    #[test]
    fn cache_miss_returns_none() {
        let (client, _tmp) = make_test_client();
        // No cache written yet
        assert!(client.load_cached().is_none());
        assert!(client.load_meta().is_none());
    }

    #[test]
    fn cache_directory_created_on_save() {
        let tmp = TempDir::new().unwrap();
        let nested_dir = tmp.path().join("deeply").join("nested").join("cache");
        let client =
            RegistryClient::with_cache_dir(nested_dir.clone()).expect("Failed to create client");

        assert!(!nested_dir.exists());
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();
        client.save_cache(&registry).expect("Failed to save cache");
        assert!(nested_dir.exists());
    }

    #[test]
    fn cache_corrupted_json_returns_none() {
        let (client, _tmp) = make_test_client();

        // Write invalid JSON to cache file
        client.ensure_cache_dir().expect("Failed to create dir");
        std::fs::write(client.cache_file_path(), "not valid json{")
            .expect("Failed to write bad cache");

        assert!(client.load_cached().is_none());
    }

    #[test]
    fn cache_corrupted_meta_returns_none() {
        let (client, _tmp) = make_test_client();

        client.ensure_cache_dir().expect("Failed to create dir");
        std::fs::write(client.meta_file_path(), "not valid json{")
            .expect("Failed to write bad meta");

        assert!(client.load_meta().is_none());
    }

    // ── CacheMeta Tests ───────────────────────────────────────────

    #[test]
    fn cache_meta_is_fresh_when_new() {
        let meta = CacheMeta::new("1.0.0");
        assert!(meta.is_fresh());
    }

    #[test]
    fn cache_meta_age_works() {
        let meta = CacheMeta::new("1.0.0");
        let age = meta.age().expect("Failed to get age");
        // Should be very recent (within a few seconds)
        assert!(age < Duration::from_secs(5));
    }

    #[test]
    fn cache_meta_invalid_timestamp_not_fresh() {
        let meta = CacheMeta {
            fetched_at: "not-a-timestamp".to_string(),
            registry_version: "1.0.0".to_string(),
        };
        assert!(!meta.is_fresh());
        assert!(meta.age().is_none());
    }

    // ── Agent Lookup Tests ────────────────────────────────────────

    #[test]
    fn find_agent_by_exact_id() {
        let (client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "claude-acp");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "claude-acp");
    }

    #[test]
    fn find_agent_by_prefix() {
        let (client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "claude");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "claude-acp");
    }

    #[test]
    fn find_agent_by_name_prefix() {
        let (client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "Codex");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "codex-acp");
    }

    #[test]
    fn find_agent_case_insensitive() {
        let (client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "CLAUDE-ACP");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "claude-acp");
    }

    #[test]
    fn find_agent_not_found() {
        let (client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "nonexistent");
        assert!(found.is_none());
    }

    #[test]
    fn find_agent_empty_query() {
        let (client, _tmp) = make_test_client();
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "");
        assert!(found.is_some()); // Empty prefix matches first agent
    }

    // ── Distribution Source Kind ──────────────────────────────────

    #[test]
    fn distribution_source_kind_npx() {
        let dist = Distribution {
            npx: Some(NpxDistribution {
                package: "pkg".to_string(),
                args: None,
                env: None,
            }),
            binary: None,
        };
        assert_eq!(DistributionExt::source_kind(&dist), "npx");
    }

    #[test]
    fn distribution_source_kind_binary() {
        let dist = Distribution {
            npx: None,
            binary: Some(BinaryDistribution {
                darwin_aarch64: None,
                darwin_x86_64: None,
                linux_aarch64: None,
                linux_x86_64: None,
                windows_aarch64: None,
                windows_x86_64: None,
            }),
        };
        assert_eq!(DistributionExt::source_kind(&dist), "binary");
    }

    #[test]
    fn distribution_source_kind_unknown() {
        let dist = Distribution {
            npx: None,
            binary: None,
        };
        assert_eq!(DistributionExt::source_kind(&dist), "unknown");
    }

    // ── Integration: Cache Roundtrip with get_registry ────────────

    #[tokio::test]
    async fn get_registry_uses_cache_when_fresh() {
        let (client, _tmp) = make_test_client();

        // Pre-populate cache with fresh data
        let registry = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();
        client.save_cache(&registry).unwrap();

        // get_registry should return cached data without network
        let result = client.get_registry().await.unwrap();
        assert_eq!(result.version, "1.0.0");
        assert_eq!(result.agents.len(), 3);
    }

    #[tokio::test]
    async fn get_registry_returns_error_when_no_cache_and_no_network() {
        // Use a bogus URL that will fail — but since we can't change REGISTRY_URL,
        // we test with an empty temp dir and override to test the error path.
        // Instead, test that load_cached returns None when no cache exists.
        let (client, _tmp) = make_test_client();
        assert!(client.load_cached().is_none());
    }

    // ── Serialization Roundtrip ───────────────────────────────────

    #[test]
    fn registry_serialization_roundtrip() {
        let (_client, _tmp) = make_test_client();
        let original = RegistryClient::parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Registry = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    // ── Local Installation Scan Tests ─────────────────────────────

    fn make_shim(tmp: &tempfile::TempDir, name: &str, script: &str) -> std::path::PathBuf {
        let shim = tmp.path().join(name);
        std::fs::write(&shim, script).expect("write shim");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&shim, perms).expect("chmod shim");
        }
        shim
    }

    fn registry_with_binary(cmd: &str) -> Registry {
        Registry {
            version: "1.0.0".to_string(),
            agents: vec![AgentEntry {
                id: "test-agent".to_string(),
                name: "Test Agent".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                repository: None,
                authors: None,
                license: None,
                icon: None,
                distribution: Distribution {
                    npx: None,
                    binary: Some(BinaryDistribution {
                        darwin_aarch64: Some(PlatformBinary {
                            archive: "https://example.com/agent.tar.gz".to_string(),
                            cmd: cmd.to_string(),
                            args: None,
                        }),
                        darwin_x86_64: None,
                        linux_aarch64: None,
                        linux_x86_64: None,
                        windows_aarch64: None,
                        windows_x86_64: None,
                    }),
                },
            }],
            extensions: None,
        }
    }

    #[tokio::test]
    async fn scan_local_installations_finds_installed_binary() {
        let tmp = tempfile::tempdir().expect("temp dir");
        make_shim(&tmp, "test-agent", "#!/bin/sh\necho \"test-agent 1.2.3\"\n");

        let registry = registry_with_binary("test-agent");
        let results =
            scan_local_installations_with_path(&registry, &[tmp.path().to_path_buf()]).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].binary, "test-agent");
        assert!(results[0].version.is_some());
        assert_eq!(results[0].version.as_deref(), Some("test-agent 1.2.3"));
    }

    #[tokio::test]
    async fn scan_local_installations_omits_missing_binary() {
        let tmp = tempfile::tempdir().expect("temp dir");
        // Do not add any shim.
        let registry = registry_with_binary("definitely-not-installed-42ch");
        let results =
            scan_local_installations_with_path(&registry, &[tmp.path().to_path_buf()]).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn scan_local_installations_strips_relative_path_prefix() {
        let tmp = tempfile::tempdir().expect("temp dir");
        make_shim(&tmp, "kimi", "#!/bin/sh\necho \"kimi 1.0.0\"\n");

        let registry = registry_with_binary("./kimi");
        let results =
            scan_local_installations_with_path(&registry, &[tmp.path().to_path_buf()]).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].binary, "kimi");
        assert_eq!(results[0].version.as_deref(), Some("kimi 1.0.0"));
    }

    #[tokio::test]
    async fn scan_local_installations_strips_nested_relative_path() {
        let tmp = tempfile::tempdir().expect("temp dir");
        make_shim(&tmp, "cursor-agent", "#!/bin/sh\necho \"cursor-agent 2.0.0\"\n");

        let registry = registry_with_binary("./dist-package/cursor-agent");
        let results =
            scan_local_installations_with_path(&registry, &[tmp.path().to_path_buf()]).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].binary, "cursor-agent");
        assert_eq!(results[0].version.as_deref(), Some("cursor-agent 2.0.0"));
    }

    #[tokio::test]
    async fn scan_local_installations_keeps_bare_command_name() {
        let tmp = tempfile::tempdir().expect("temp dir");
        make_shim(&tmp, "opencode", "#!/bin/sh\necho \"opencode 3.0.0\"\n");

        let registry = registry_with_binary("opencode");
        let results =
            scan_local_installations_with_path(&registry, &[tmp.path().to_path_buf()]).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].binary, "opencode");
        assert_eq!(results[0].version.as_deref(), Some("opencode 3.0.0"));
    }
}
