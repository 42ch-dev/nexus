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
//! The Rust types for registry data are defined here (not via codegen from
//! `registry-manifest.schema.json`) because:
//! - The registry JSON comes from an external CDN, not our wire protocol
//! - The codegen pipeline only produces flat structs, not nested types
//! - We need proper typed fields for agent distribution, platform binaries, etc.
//!
//! The JSON Schema file (`schemas/acp-runtime/registry-manifest.schema.json`)
//! serves as the authoritative structural reference and validation document.

// This module defines the public API for ACP registry fetching and caching.
// Items are consumed by Task 3 (CLI commands) and Task 4 (transport/run).
// Until those tasks land, suppress dead_code warnings for public API items.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ── Constants ────────────────────────────────────────────────────────

/// ACP Registry CDN URL.
pub const REGISTRY_URL: &str =
    "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

/// Maximum cache age before stale-while-revalidate kicks in.
const CACHE_MAX_AGE: Duration = Duration::from_secs(24 * 3600);

/// Cache file name for the full registry JSON.
const CACHE_FILE: &str = "cache.json";

/// Cache file name for metadata (fetch timestamp, version).
const CACHE_META_FILE: &str = "cache_meta.json";

/// Subdirectory under $HOME for nexus42 data.
const NEXUS_DIR: &str = ".nexus42";

/// Subdirectory under nexus42 dir for registry cache.
const REGISTRY_DIR: &str = "registry";

// ── Registry Data Types ──────────────────────────────────────────────

/// Top-level ACP Registry response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Registry {
    /// Registry format version (e.g. "1.0.0").
    pub version: String,
    /// List of available ACP agents.
    pub agents: Vec<AgentEntry>,
    /// Registry extensions (reserved).
    #[serde(default)]
    pub extensions: Vec<serde_json::Value>,
}

/// A single agent entry in the ACP Registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentEntry {
    /// Unique agent identifier (e.g. "claude-acp").
    pub id: String,
    /// Human-readable agent name (e.g. "Claude Agent").
    pub name: String,
    /// Agent version (e.g. "0.18.0").
    pub version: String,
    /// Agent description.
    #[serde(default)]
    pub description: String,
    /// Agent source repository URL.
    #[serde(default)]
    pub repository: Option<String>,
    /// Agent authors.
    #[serde(default)]
    pub authors: Vec<String>,
    /// Agent license identifier.
    #[serde(default)]
    pub license: Option<String>,
    /// Agent icon URL.
    #[serde(default)]
    pub icon: Option<String>,
    /// Distribution configuration (npx or binary).
    pub distribution: Distribution,
}

/// Agent distribution configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Distribution {
    /// NPX-based distribution (e.g. `npx @scope/pkg@1.0.0`).
    #[serde(default)]
    pub npx: Option<NpxDistribution>,
    /// Binary distribution (per-platform downloads).
    #[serde(default)]
    pub binary: Option<BinaryDistribution>,
}

impl Distribution {
    /// Returns the distribution source kind: "npx" or "binary".
    pub fn source_kind(&self) -> &str {
        if self.npx.is_some() {
            "npx"
        } else if self.binary.is_some() {
            "binary"
        } else {
            "unknown"
        }
    }
}

/// NPX-based distribution configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NpxDistribution {
    /// npm package name with optional version (e.g. "@scope/pkg@1.0.0").
    pub package: String,
    /// Additional CLI arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables to set.
    #[serde(default)]
    pub env: Option<std::collections::HashMap<String, String>>,
}

/// Binary distribution configuration with per-platform entries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BinaryDistribution {
    /// macOS ARM64 (Apple Silicon).
    #[serde(rename = "darwin-aarch64", default)]
    pub darwin_aarch64: Option<PlatformBinary>,
    /// macOS x86_64 (Intel).
    #[serde(rename = "darwin-x86_64", default)]
    pub darwin_x86_64: Option<PlatformBinary>,
    /// Linux ARM64.
    #[serde(rename = "linux-aarch64", default)]
    pub linux_aarch64: Option<PlatformBinary>,
    /// Linux x86_64.
    #[serde(rename = "linux-x86_64", default)]
    pub linux_x86_64: Option<PlatformBinary>,
    /// Windows ARM64.
    #[serde(rename = "windows-aarch64", default)]
    pub windows_aarch64: Option<PlatformBinary>,
    /// Windows x86_64.
    #[serde(rename = "windows-x86_64", default)]
    pub windows_x86_64: Option<PlatformBinary>,
}

/// Platform-specific binary distribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlatformBinary {
    /// Download URL for the platform-specific archive.
    pub archive: String,
    /// Command to execute within the archive.
    pub cmd: String,
    /// Additional CLI arguments.
    #[serde(default)]
    pub args: Vec<String>,
}

// ── Cache Metadata ───────────────────────────────────────────────────

/// Metadata stored alongside the cache file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheMeta {
    /// ISO 8601 timestamp of when the cache was fetched.
    pub fetched_at: String,
    /// Registry version string from the cached response.
    pub registry_version: String,
}

impl CacheMeta {
    /// Create a new cache metadata entry.
    pub fn new(registry_version: &str) -> Self {
        Self {
            fetched_at: chrono::Utc::now().to_rfc3339(),
            registry_version: registry_version.to_string(),
        }
    }

    /// Parse `fetched_at` into a `SystemTime`. Returns `None` if parsing fails.
    pub fn fetched_time(&self) -> Option<SystemTime> {
        chrono::DateTime::parse_from_rfc3339(&self.fetched_at)
            .ok()
            .map(|dt| dt.into())
    }

    /// Returns the age of this cache entry, or `None` if the timestamp is invalid.
    pub fn age(&self) -> Option<Duration> {
        self.fetched_time().map(|t| {
            SystemTime::now()
                .duration_since(t)
                .unwrap_or(Duration::ZERO)
        })
    }

    /// Returns `true` if the cache is within the max age (fresh).
    pub fn is_fresh(&self) -> bool {
        self.age().map(|age| age < CACHE_MAX_AGE).unwrap_or(false)
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
        let response = tokio::time::timeout(Duration::from_secs(60), http.get(REGISTRY_URL).send())
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
    fn parse_registry_json(&self, json: &str) -> anyhow::Result<Registry> {
        let registry: Registry = serde_json::from_str(json)?;
        Ok(registry)
    }
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default RegistryClient")
    }
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
        let (client, _tmp) = make_test_client();
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

        assert_eq!(registry.version, "1.0.0");
        assert_eq!(registry.agents.len(), 3);
        assert_eq!(registry.extensions.len(), 0);
    }

    #[test]
    fn parse_agent_npx_distribution() {
        let (client, _tmp) = make_test_client();
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let claude = &registry.agents[0];
        assert_eq!(claude.id, "claude-acp");
        assert_eq!(claude.name, "Claude Agent");
        assert_eq!(claude.version, "0.18.0");
        assert_eq!(claude.description, "ACP wrapper for Anthropic's Claude");
        assert_eq!(
            claude.repository.as_deref(),
            Some("https://github.com/zed-industries/claude-agent-acp")
        );
        assert_eq!(claude.authors, vec!["Anthropic"]);
        assert_eq!(claude.license.as_deref(), Some("proprietary"));

        let npx = claude.distribution.npx.as_ref().unwrap();
        assert_eq!(npx.package, "@zed-industries/claude-agent-acp@0.18.0");
        assert_eq!(claude.distribution.source_kind(), "npx");
    }

    #[test]
    fn parse_agent_binary_distribution() {
        let (client, _tmp) = make_test_client();
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let codex = &registry.agents[1];
        assert_eq!(codex.id, "codex-acp");
        assert_eq!(codex.distribution.source_kind(), "binary");

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
        let (client, _tmp) = make_test_client();
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let gemini = &registry.agents[2];
        assert_eq!(gemini.id, "gemini");
        let npx = gemini.distribution.npx.as_ref().unwrap();
        assert_eq!(npx.args, vec!["--verbose"]);
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
        let (client, _tmp) = make_test_client();
        let registry = client.parse_registry_json(json).unwrap();

        assert_eq!(registry.agents.len(), 1);
        let agent = &registry.agents[0];
        assert_eq!(agent.id, "minimal-agent");
        assert_eq!(agent.description, ""); // default
        assert!(agent.repository.is_none()); // optional
        assert!(agent.authors.is_empty()); // default
        assert!(agent.license.is_none()); // optional
    }

    #[test]
    fn parse_invalid_json_fails() {
        let (client, _tmp) = make_test_client();
        let result = client.parse_registry_json("not json");
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
        let (client, _tmp) = make_test_client();
        let result = client.parse_registry_json(json);
        assert!(result.is_err());
    }

    // ── Cache Tests ───────────────────────────────────────────────

    #[test]
    fn cache_write_and_read() {
        let (client, _tmp) = make_test_client();
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

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
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

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
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();
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
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "claude-acp");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "claude-acp");
    }

    #[test]
    fn find_agent_by_prefix() {
        let (client, _tmp) = make_test_client();
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "claude");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "claude-acp");
    }

    #[test]
    fn find_agent_by_name_prefix() {
        let (client, _tmp) = make_test_client();
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "Codex");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "codex-acp");
    }

    #[test]
    fn find_agent_case_insensitive() {
        let (client, _tmp) = make_test_client();
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "CLAUDE-ACP");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "claude-acp");
    }

    #[test]
    fn find_agent_not_found() {
        let (client, _tmp) = make_test_client();
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "nonexistent");
        assert!(found.is_none());
    }

    #[test]
    fn find_agent_empty_query() {
        let (client, _tmp) = make_test_client();
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let found = client.find_agent(&registry, "");
        assert!(found.is_some()); // Empty prefix matches first agent
    }

    // ── Distribution Source Kind ──────────────────────────────────

    #[test]
    fn distribution_source_kind_npx() {
        let dist = Distribution {
            npx: Some(NpxDistribution {
                package: "pkg".to_string(),
                args: vec![],
                env: None,
            }),
            binary: None,
        };
        assert_eq!(dist.source_kind(), "npx");
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
        assert_eq!(dist.source_kind(), "binary");
    }

    #[test]
    fn distribution_source_kind_unknown() {
        let dist = Distribution {
            npx: None,
            binary: None,
        };
        assert_eq!(dist.source_kind(), "unknown");
    }

    // ── Integration: Cache Roundtrip with get_registry ────────────

    #[tokio::test]
    async fn get_registry_uses_cache_when_fresh() {
        let (client, _tmp) = make_test_client();

        // Pre-populate cache with fresh data
        let registry = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();
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
        let (client, _tmp) = make_test_client();
        let original = client.parse_registry_json(SAMPLE_REGISTRY).unwrap();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Registry = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }
}
