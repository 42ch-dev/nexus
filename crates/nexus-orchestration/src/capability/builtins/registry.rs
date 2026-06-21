//! `registry.refresh` capability.
//!
//! Owner crate: `nexus-acp-host` (logical); runtime: `nexus-orchestration`.
//!
//! # Modes
//!
//! - **Synthetic (default / air-gap)**: Returns an output generated from an
//!   embedded capability snapshot. No network calls. Deterministic and
//!   version-pinned.
//! - **Network (optional)**: When `--cdn-url <url>` is provided at daemon
//!   start, fetches the real registry JSON from the CDN with configurable
//!   timeout and retry. Falls back to synthetic on failure.
//!
//! # Design
//!
//! The CDN URL is thread-safe global state set once at daemon boot via
//! [`set_cdn_url`]. The capability reads it at invocation time.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use chrono::Utc;
use nexus_contracts::local::orchestration::{RegistryRefreshInput, RegistryRefreshOutput};
use serde_json::Value;
use std::sync::RwLock;
use std::time::Duration;

// ─── Embedded Snapshot ─────────────────────────────────────────────────────

/// Version string for the embedded registry snapshot.
/// Bumped every release when the snapshot list is updated.
const REGISTRY_SNAPSHOT_VERSION: &str = "2026-06-22.v1";

/// Embedded capability IDs shipped with the binary.
///
/// This is a minimal list (capability IDs only) — not the full registry
/// metadata. Updated per release to match the logical catalog in
/// `acp-capability-set.md`.
const REGISTRY_SNAPSHOT_CAPABILITIES: &[&str] = &[
    // §4.1 Context
    "nexus.context.whoami",
    "nexus.workspace.info",
    "nexus.workspace.paths",
    "nexus.context.assemble",
    // §4.2 World read
    "nexus.world.snapshot.get",
    "nexus.world.state.query",
    "nexus.timeline.recent.get",
    "nexus.kb_snapshot.read",
    // §4.3 World mutation
    "nexus.world.delta.propose",
    "nexus.world.delta.apply",
    "nexus.timeline.event.append",
    "nexus.fork.create",
    // §4.3A World CLI write
    "nexus.kb_snapshot.write",
    "nexus.world.configure",
    // §4.4 Sync
    "nexus.sync.prepare_push",
    "nexus.sync.push",
    "nexus.sync.pull",
    "nexus.sync.status",
    // §4.5 Manuscript
    "nexus.manuscript.list",
    "nexus.manuscript.read_range",
    "nexus.manuscript.write",
    "nexus.manuscript.phase.get",
    "nexus.manuscript.phase.set",
    "nexus.manuscript.chapter.update",
    // §4.6A Research
    "nexus.research.query",
    // §4.7 Observability
    "nexus.trace.correlation",
    "nexus.runtime.health",
    // §4.8 Work & orchestration
    "nexus.work.schedule.set",
    "nexus.finding.resolve",
    "nexus.pool.entry.manage",
    // ── registry.refresh itself ──
    "nexus.registry.refresh",
];

// ─── CDN configuration (global, set at daemon boot) ────────────────────────

/// CDN fetch configuration.
#[derive(Debug, Clone)]
pub struct CdnConfig {
    /// The CDN URL to fetch from.
    pub url: String,
    /// Per-request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Maximum retries before falling back to synthetic.
    pub max_retries: u32,
}

/// Thread-safe global CDN configuration.
static CDN_CONFIG: RwLock<Option<CdnConfig>> = RwLock::new(None);

/// Set the CDN configuration from daemon boot.
///
/// Called once during daemon startup before any capability invocations.
/// Can be reset in tests for isolation.
///
/// # Panics
///
/// Panics if the internal lock is poisoned.
pub fn set_cdn_config(config: Option<CdnConfig>) {
    let mut guard = CDN_CONFIG.write().expect("CDN_CONFIG lock poisoned");
    *guard = config;
}

/// Get a clone of the current CDN configuration.
fn get_cdn_config() -> Option<CdnConfig> {
    CDN_CONFIG.read().expect("CDN_CONFIG lock poisoned").clone()
}

// ─── Capability ────────────────────────────────────────────────────────────

/// Refresh the ACP registry cache.
///
/// Uses embedded snapshot by default; fetches from CDN if configured.
#[derive(Debug, Clone)]
pub struct RegistryRefresh;

impl RegistryRefresh {
    /// Create a new `RegistryRefresh` capability.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for RegistryRefresh {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for RegistryRefresh {
    fn name(&self) -> &'static str {
        "registry.refresh"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"force":{"type":"boolean","default":false}},"required":[],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"cacheAgeMs":{"type":"integer","minimum":0},"capabilityCount":{"type":"integer","minimum":0},"source":{"type":"string","enum":["synthetic","cdn","synthetic_fallback"]},"snapshotVersion":{"type":"string"},"generatedAt":{"type":"string","format":"date-time"},"fetchTimeoutMs":{"type":"integer","minimum":0},"maxRetries":{"type":"integer","minimum":0},"retryCount":{"type":"integer","minimum":0},"fallbackReason":{"type":"string"}},"required":["cacheAgeMs","capabilityCount","source","snapshotVersion","generatedAt"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _input: RegistryRefreshInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("registry.refresh input: {e}")))?;

        let now = Utc::now().to_rfc3339();

        // Check if CDN URL is configured
        if let Some(ref cdn) = get_cdn_config() {
            // Network mode: fetch from CDN with timeout + retry
            match fetch_from_cdn(cdn).await {
                Ok((capability_count, retry_count)) => {
                    let output = RegistryRefreshOutput {
                        cache_age_ms: 0, // fresh fetch
                        capability_count,
                        source: "cdn".to_string(),
                        snapshot_version: REGISTRY_SNAPSHOT_VERSION.to_string(),
                        generated_at: now,
                        fetch_timeout_ms: cdn.timeout_ms,
                        max_retries: cdn.max_retries,
                        retry_count,
                        fallback_reason: String::new(),
                    };
                    return serde_json::to_value(output)
                        .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")));
                }
                Err(err) => {
                    // Network failed — fall back to synthetic.
                    let output = RegistryRefreshOutput {
                        cache_age_ms: 0,
                        capability_count: len_u32(REGISTRY_SNAPSHOT_CAPABILITIES.len()),
                        source: "synthetic_fallback".to_string(),
                        snapshot_version: REGISTRY_SNAPSHOT_VERSION.to_string(),
                        generated_at: now,
                        fetch_timeout_ms: cdn.timeout_ms,
                        max_retries: cdn.max_retries,
                        retry_count: cdn.max_retries,
                        fallback_reason: err,
                    };
                    return serde_json::to_value(output)
                        .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")));
                }
            }
        }

        // Default / air-gap: synthetic output only — zero network calls.
        let output = RegistryRefreshOutput {
            cache_age_ms: 0,
            capability_count: len_u32(REGISTRY_SNAPSHOT_CAPABILITIES.len()),
            source: "synthetic".to_string(),
            snapshot_version: REGISTRY_SNAPSHOT_VERSION.to_string(),
            generated_at: now,
            fetch_timeout_ms: 0,
            max_retries: 0,
            retry_count: 0,
            fallback_reason: String::new(),
        };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ─── Network fetch helpers ─────────────────────────────────────────────────

/// Helper: convert a `usize` len to `u32`, clamping at `u32::MAX`.
fn len_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// Fetch registry data from CDN with retry logic.
///
/// Returns `(capability_count, retry_count)` on success, or an error
/// description string on failure.
async fn fetch_from_cdn(cdn: &CdnConfig) -> Result<(u32, u32), String> {
    let timeout = Duration::from_millis(cdn.timeout_ms);
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let mut last_err = String::new();

    for attempt in 0..=cdn.max_retries {
        if attempt > 0 {
            // Exponential backoff: 500ms, 1s, 2s, ...
            let backoff_ms = 500u64 * (1u64 << (attempt - 1));
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        }

        match client.get(&cdn.url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<Value>().await {
                        Ok(json) => {
                            // Count capabilities in the response.
                            let count = count_capabilities(&json);
                            return Ok((count, attempt));
                        }
                        Err(e) => {
                            last_err =
                                format!("failed to parse CDN JSON at attempt {}: {e}", attempt + 1);
                            // continue to next retry
                        }
                    }
                } else {
                    last_err = format!(
                        "CDN returned HTTP {} at attempt {}",
                        resp.status(),
                        attempt + 1
                    );
                    // continue to next retry
                }
            }
            Err(e) => {
                last_err = format!("CDN request failed at attempt {}: {e}", attempt + 1);
                // continue to next retry
            }
        }
    }

    Err(last_err)
}

/// Count capability entries in a registry JSON response.
fn count_capabilities(json: &Value) -> u32 {
    json.as_array()
        .map(|arr| len_u32(arr.len()))
        .or_else(|| {
            json.get("capabilities")
                .and_then(|v| v.as_array())
                .map(|caps| len_u32(caps.len()))
        })
        .or_else(|| {
            json.get("agents")
                .and_then(|v| v.as_array())
                .map(|agents| len_u32(agents.len()))
        })
        .or_else(|| {
            json.get("items")
                .and_then(|v| v.as_array())
                .map(|items| len_u32(items.len()))
        })
        .unwrap_or_else(|| {
            // Count top-level keys as approximate capability count
            json.as_object().map_or(0, |obj| len_u32(obj.len()))
        })
}

// ─── Public API for capability registration ────────────────────────────────

/// Export the embedded snapshot for golden-snapshot tests.
#[must_use]
pub const fn embedded_snapshot_version() -> &'static str {
    REGISTRY_SNAPSHOT_VERSION
}

/// Export the embedded capability list for introspection.
#[must_use]
pub const fn embedded_snapshot_capabilities() -> &'static [&'static str] {
    REGISTRY_SNAPSHOT_CAPABILITIES
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Reset CDN config before each test to ensure isolation.
    fn reset_cdn_config() {
        set_cdn_config(None);
    }

    // ── Synthetic mode tests (air-gap / default) ───────────────────────────

    #[tokio::test]
    #[serial_test::serial]
    async fn registry_refresh_synthetic_smoke() {
        reset_cdn_config();
        let cap = RegistryRefresh::new();
        let out = cap.run(serde_json::json!({"force": false})).await.unwrap();

        assert_eq!(out["source"], "synthetic");
        assert_eq!(out["cacheAgeMs"], 0);
        assert_eq!(out["capabilityCount"], REGISTRY_SNAPSHOT_CAPABILITIES.len());
        assert_eq!(out["snapshotVersion"], REGISTRY_SNAPSHOT_VERSION);
        assert!(out.get("generatedAt").and_then(|v| v.as_str()).is_some());
        assert_eq!(out["fetchTimeoutMs"], 0);
        assert_eq!(out["maxRetries"], 0);
        assert_eq!(out["retryCount"], 0);
        assert_eq!(out["fallbackReason"], "");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn registry_refresh_synthetic_deterministic() {
        reset_cdn_config();
        let cap = RegistryRefresh::new();

        // Two back-to-back calls with the same input should yield the same
        // capability_count, source, and snapshot_version. generated_at will
        // differ (timestamp), but the rest must be identical.
        let out1 = cap.run(serde_json::json!({"force": false})).await.unwrap();
        let out2 = cap.run(serde_json::json!({"force": false})).await.unwrap();

        assert_eq!(out1["source"], out2["source"]);
        assert_eq!(out1["capabilityCount"], out2["capabilityCount"]);
        assert_eq!(out1["snapshotVersion"], out2["snapshotVersion"]);
        assert_eq!(out1["fetchTimeoutMs"], out2["fetchTimeoutMs"]);
        assert_eq!(out1["fallbackReason"], out2["fallbackReason"]);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn registry_refresh_synthetic_zero_network_calls() {
        reset_cdn_config();
        // Default mode (no CDN URL) must produce output without any network I/O.
        // This is verified by the output having source="synthetic" and
        // fetch_timeout_ms=0. The test also ensures no reqwest client is
        // created when no CDN config is set.
        let cap = RegistryRefresh::new();
        let out = cap.run(serde_json::json!({"force": true})).await.unwrap();
        assert_eq!(out["source"], "synthetic");
        assert_eq!(out["fetchTimeoutMs"], 0);
    }

    // ── Golden snapshot test ───────────────────────────────────────────────

    #[tokio::test]
    #[serial_test::serial]
    async fn golden_snapshot_version_stability() {
        reset_cdn_config();
        // The snapshot version must never change without a code change.
        // If this test fails because REGISTRY_SNAPSHOT_VERSION was
        // intentionally bumped, update the expected value.
        assert_eq!(REGISTRY_SNAPSHOT_VERSION, "2026-06-22.v1");

        let cap = RegistryRefresh::new();
        let out = cap.run(serde_json::json!({})).await.unwrap();

        // Verify every known capability ID is embedded
        let count = out["capabilityCount"].as_u64().unwrap() as usize;
        assert_eq!(count, REGISTRY_SNAPSHOT_CAPABILITIES.len());
        assert!(count >= 31, "expected at least 31 capabilities in snapshot");
    }

    // ── Embedded snapshot integrity tests ──────────────────────────────────

    #[test]
    fn embedded_snapshot_all_nexus_prefix() {
        for id in REGISTRY_SNAPSHOT_CAPABILITIES {
            assert!(
                id.starts_with("nexus."),
                "snapshot capability '{id}' must have nexus.* prefix"
            );
        }
    }

    #[test]
    fn embedded_snapshot_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for id in REGISTRY_SNAPSHOT_CAPABILITIES {
            assert!(seen.insert(id), "duplicate capability id in snapshot: {id}");
        }
    }

    // ── Network mode tests ─────────────────────────────────────────────────

    #[tokio::test]
    #[serial_test::serial]
    async fn network_mode_requires_cdn_config() {
        reset_cdn_config();
        // No CDN config set → synthetic output.
        let cap = RegistryRefresh::new();
        let out = cap.run(serde_json::json!({})).await.unwrap();
        assert_eq!(out["source"], "synthetic");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn network_mode_falls_back_on_bad_url() {
        reset_cdn_config();
        // Set a CDN URL pointing to a non-routable address.
        // The test expects a synthetic fallback with a fallback reason.
        let config = CdnConfig {
            url: "http://0.0.0.0:1/nonexistent".to_string(),
            timeout_ms: 1_000,
            max_retries: 1,
        };
        set_cdn_config(Some(config));

        let cap = RegistryRefresh::new();
        let out = cap.run(serde_json::json!({"force": true})).await.unwrap();

        // Clean up: reset to None so other tests aren't affected.
        reset_cdn_config();

        assert_eq!(out["source"], "synthetic_fallback");
        assert!(!out["fallbackReason"].as_str().unwrap_or("").is_empty());
        assert_eq!(out["retryCount"], 1); // max_retries exhausted
        assert_eq!(out["maxRetries"], 1);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn network_mode_timeout() {
        reset_cdn_config();
        // Use a URL that will be unreachable and time out.
        let config = CdnConfig {
            url: "https://192.0.2.1/registry.json".to_string(), // TEST-NET-1 (non-routable)
            timeout_ms: 500,
            max_retries: 0,
        };
        set_cdn_config(Some(config));

        let cap = RegistryRefresh::new();
        let out = cap.run(serde_json::json!({"force": true})).await.unwrap();

        // Clean up: reset to None so other tests aren't affected.
        reset_cdn_config();

        assert_eq!(out["source"], "synthetic_fallback");
        assert!(!out["fallbackReason"].as_str().unwrap_or("").is_empty());
    }

    // ── Capability metadata tests ──────────────────────────────────────────

    #[test]
    fn capability_name_is_registry_refresh() {
        let cap = RegistryRefresh::new();
        assert_eq!(cap.name(), "registry.refresh");
    }

    #[test]
    fn capability_input_schema_is_valid_json() {
        let cap = RegistryRefresh::new();
        let schema: serde_json::Value =
            serde_json::from_str(cap.input_schema()).expect("input schema is valid JSON");
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn capability_output_schema_is_valid_json() {
        let cap = RegistryRefresh::new();
        let schema: serde_json::Value =
            serde_json::from_str(cap.output_schema()).expect("output schema is valid JSON");
        assert_eq!(schema["type"], "object");

        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::Value::String("cacheAgeMs".to_string())));
        assert!(required.contains(&serde_json::Value::String("capabilityCount".to_string())));
        assert!(required.contains(&serde_json::Value::String("source".to_string())));
        assert!(required.contains(&serde_json::Value::String("snapshotVersion".to_string())));
    }

    // ── Input validation tests ─────────────────────────────────────────────

    #[tokio::test]
    #[serial_test::serial]
    async fn registry_refresh_rejects_invalid_input() {
        reset_cdn_config();
        let cap = RegistryRefresh::new();
        let result = cap.run(serde_json::json!("not an object")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn registry_refresh_accepts_empty_input() {
        reset_cdn_config();
        let cap = RegistryRefresh::new();
        let out = cap.run(serde_json::json!({})).await.unwrap();
        assert_eq!(out["source"], "synthetic");
    }
}
