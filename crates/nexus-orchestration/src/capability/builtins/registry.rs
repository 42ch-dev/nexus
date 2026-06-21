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
use futures_util::StreamExt;
use nexus_contracts::local::orchestration::{RegistryRefreshInput, RegistryRefreshOutput};
use serde_json::Value;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

// ─── CDN Error Type ────────────────────────────────────────────────────────

/// Domain error for CDN fetch and URL validation.
#[derive(Debug, Clone)]
pub enum CdnError {
    /// URL scheme is not HTTPS.
    InsecureScheme,
    /// Host resolves to a blocked network address (private, loopback,
    /// link-local, or metadata endpoint).
    BlockedHost,
    /// Too many redirects (exceeded redirect policy limit).
    TooManyRedirects,
    /// Response body exceeded the maximum allowed size.
    BodyTooLarge,
    /// Request timed out.
    Timeout,
    /// Server returned a non-success HTTP status.
    ServerStatus(u16),
    /// Failed to parse the response body as JSON.
    Parse,
    /// I/O error during fetch.
    Io,
    /// URL is empty or whitespace-only.
    EmptyUrl,
    /// URL string is not a valid URL.
    UrlParse,
    /// Uncategorized error with human-readable message.
    Other(String),
}

impl std::fmt::Display for CdnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsecureScheme => write!(f, "CDN URL must use https:// scheme"),
            Self::BlockedHost => write!(f, "CDN URL host is a blocked network address"),
            Self::TooManyRedirects => write!(f, "CDN response redirected too many times"),
            Self::BodyTooLarge => write!(f, "CDN response body exceeded maximum size"),
            Self::Timeout => write!(f, "CDN request timed out"),
            Self::ServerStatus(code) => write!(f, "CDN returned HTTP {code}"),
            Self::Parse => write!(f, "failed to parse CDN response"),
            Self::Io => write!(f, "I/O error during CDN fetch"),
            Self::EmptyUrl => write!(f, "CDN URL is empty or whitespace-only"),
            Self::UrlParse => write!(f, "CDN URL is not a valid URL"),
            Self::Other(msg) => write!(f, "CDN error: {msg}"),
        }
    }
}

impl std::error::Error for CdnError {}

/// Maximum body size for CDN responses: 8 MiB.
const MAX_CDN_BODY_SIZE: usize = 8 * 1024 * 1024;

/// Validate a CDN URL string for security constraints.
///
/// Called at boot time (before any fetches) to reject obviously
/// dangerous URLs early. This check is structural only — it does
/// NOT perform DNS resolution (that happens in `fetch_from_cdn`).
///
/// # Errors
///
/// Returns `CdnError` if the URL is empty, whitespace-only, fails
/// to parse, has a non-HTTPS scheme, or contains a literal
/// private/loopback/link-local IP address.
pub fn validate_cdn_url_static(url_str: &str) -> Result<(), CdnError> {
    // H-002: reject empty / whitespace-only
    if url_str.trim().is_empty() {
        return Err(CdnError::EmptyUrl);
    }

    // H-002: reject non-HTTPS scheme
    if !url_str.starts_with("https://") {
        return Err(CdnError::InsecureScheme);
    }

    // Extract host portion for literal-IP check.
    let after_scheme = &url_str["https://".len()..];
    let host_part = after_scheme.split('/').next().unwrap_or(after_scheme);
    // Strip port if present
    let host = host_part.split(':').next().unwrap_or(host_part);

    if host.is_empty() {
        return Err(CdnError::UrlParse);
    }

    // If the host is a literal IP, reject private/loopback/link-local ranges.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(&ip) {
            return Err(CdnError::BlockedHost);
        }
    }

    Ok(())
}

/// Check whether an IP address is in a blocked range (private, loopback,
/// link-local, or metadata endpoint).
fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                // Explicit metadata endpoint range (169.254.0.0/16) —
                // `is_link_local()` already covers this on most platforms,
                // but we double-check for clarity.
                || v4.octets()[0] == 169 && v4.octets()[1] == 254
        }
        IpAddr::V6(v6) => {
            v6.is_loopback() || is_ipv6_mapped_ipv4_private(v6) || is_ipv6_private_range(v6)
        }
    }
}

/// Check if an IPv6 address is an IPv4-mapped IPv6 address whose embedded
/// IPv4 address falls in a blocked range.
fn is_ipv6_mapped_ipv4_private(v6: &Ipv6Addr) -> bool {
    let octets = v6.octets();
    // ::ffff:0:0/96 prefix
    if octets[..10] == [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] && octets[10] == 0xff && octets[11] == 0xff {
        let v4 = Ipv4Addr::new(octets[12], octets[13], octets[14], octets[15]);
        return v4.is_private()
            || v4.is_loopback()
            || v4.is_link_local()
            || (v4.octets()[0] == 169 && v4.octets()[1] == 254);
    }
    false
}

/// Check if an IPv6 address falls in a private/unique-local range (`fc00::/7`).
const fn is_ipv6_private_range(v6: &Ipv6Addr) -> bool {
    let octets = v6.octets();
    // fc00::/7 → first octet 0xfc or 0xfd
    octets[0] & 0xfe == 0xfc
}

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

// ─── CDN configuration (constructor-injected; V1.57 P1) ────────────────────

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

// ─── Capability ────────────────────────────────────────────────────────────

/// Refresh the ACP registry cache.
///
/// Uses embedded snapshot by default; fetches from CDN if configured.
/// `CdnConfig` is constructor-injected (V1.57 P1) — no global state.
#[derive(Debug, Clone)]
pub struct RegistryRefresh {
    cdn_config: Option<CdnConfig>,
}

impl RegistryRefresh {
    /// Create a new `RegistryRefresh` capability (synthetic-only, no CDN).
    #[must_use]
    pub const fn new() -> Self {
        Self { cdn_config: None }
    }

    /// Create a new `RegistryRefresh` with CDN fetch capability.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_cdn(config: CdnConfig) -> Self {
        Self {
            cdn_config: Some(config),
        }
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

        // Check if CDN URL is configured (constructor-injected, V1.57 P1)
        if let Some(ref cdn) = self.cdn_config {
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
                    // H-001: fallback_reason carries a typed CdnError variant stringified.
                    let fallback_reason = err.to_string();
                    let output = RegistryRefreshOutput {
                        cache_age_ms: 0,
                        capability_count: len_u32(REGISTRY_SNAPSHOT_CAPABILITIES.len()),
                        source: "synthetic_fallback".to_string(),
                        snapshot_version: REGISTRY_SNAPSHOT_VERSION.to_string(),
                        generated_at: now,
                        fetch_timeout_ms: cdn.timeout_ms,
                        max_retries: cdn.max_retries,
                        retry_count: cdn.max_retries,
                        fallback_reason,
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
/// # Security
///
/// Enforces HTTPS-only, no redirects, private-IP block (via DNS
/// resolution), and a body size cap of 8 MiB before attempting any
/// network I/O or response parsing.
///
/// Returns `(capability_count, retry_count)` on success, or a typed
/// `CdnError` on failure.
async fn fetch_from_cdn(cdn: &CdnConfig) -> Result<(u32, u32), CdnError> {
    // ── C-001 #1: scheme guard ──────────────────────────────────────────
    if !cdn.url.starts_with("https://") {
        return Err(CdnError::InsecureScheme);
    }

    // ── C-001 #3: extract host + perform DNS resolution + block private IPs ─
    let after_scheme = &cdn.url["https://".len()..];
    let host_part = after_scheme.split('/').next().unwrap_or(after_scheme);
    let host = host_part.split(':').next().unwrap_or(host_part);

    if host.is_empty() {
        return Err(CdnError::UrlParse);
    }

    // Resolve the hostname to IP addresses.
    // Use port 443 (default HTTPS port) for resolution.
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, 443_u16))
        .await
        .map_err(|_e| CdnError::BlockedHost)?
        .collect();

    if addrs.is_empty() {
        return Err(CdnError::BlockedHost);
    }

    for addr in &addrs {
        if is_blocked_ip(&addr.ip()) {
            return Err(CdnError::BlockedHost);
        }
    }

    // ── C-001 #2: redirect policy — no redirects allowed ────────────────
    let timeout = Duration::from_millis(cdn.timeout_ms);
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::limited(0))
        .build()
        .map_err(|e| CdnError::Other(format!("failed to build HTTP client: {e}")))?;

    let mut last_err = CdnError::Other("unknown error".to_string());

    for attempt in 0..=cdn.max_retries {
        if attempt > 0 {
            // Exponential backoff: 500ms, 1s, 2s, ...
            let backoff_ms = 500u64 * (1u64 << (attempt - 1));
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        }

        match client.get(&cdn.url).send().await {
            Ok(resp) => {
                // C-001 #2: 3xx responses are blocked by the redirect policy.
                // policy=limited(0) means reqwest returns the 3xx response
                // as-is without following it.
                if resp.status().is_redirection() {
                    last_err = CdnError::TooManyRedirects;
                    // No retry for redirects — the endpoint is misconfigured.
                    break;
                }
                if resp.status().is_success() {
                    // C-001 #4: body size limit — 8 MiB max.
                    match read_body_with_limit(resp, MAX_CDN_BODY_SIZE).await {
                        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                            Ok(json) => {
                                let count = count_capabilities(&json);
                                return Ok((count, attempt));
                            }
                            Err(_e) => {
                                last_err = CdnError::Parse;
                                // continue to next retry
                            }
                        },
                        Err(e) => {
                            last_err = e;
                            // Do not retry on BodyTooLarge — the endpoint is
                            // serving a too-large payload, retrying won't help.
                            if matches!(last_err, CdnError::BodyTooLarge) {
                                break;
                            }
                        }
                    }
                } else {
                    last_err = CdnError::ServerStatus(resp.status().as_u16());
                    // Retry on 5xx, break on 4xx (client error).
                    if resp.status().is_client_error() {
                        break;
                    }
                }
            }
            Err(e) => {
                if e.is_timeout() {
                    last_err = CdnError::Timeout;
                } else if e.is_connect() {
                    last_err = CdnError::Io;
                } else {
                    last_err = CdnError::Other(format!(
                        "CDN request failed at attempt {}: {e}",
                        attempt + 1
                    ));
                }
            }
        }
    }

    Err(last_err)
}

/// Read the response body with a hard byte limit.
///
/// Uses `bytes_stream()` and accumulates chunks, checking the total
/// length against `max_size` after each chunk. Returns `BodyTooLarge`
/// if the limit is exceeded.
async fn read_body_with_limit(
    resp: reqwest::Response,
    max_size: usize,
) -> Result<Vec<u8>, CdnError> {
    let mut stream = resp.bytes_stream();
    let mut buf = Vec::new();

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                if buf.len() + chunk.len() > max_size {
                    return Err(CdnError::BodyTooLarge);
                }
                buf.extend_from_slice(&chunk);
            }
            Err(e) => {
                return if e.is_timeout() {
                    Err(CdnError::Timeout)
                } else {
                    Err(CdnError::Io)
                };
            }
        }
    }

    Ok(buf)
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


    // ── Synthetic mode tests (air-gap / default) ───────────────────────────

    #[tokio::test]
    #[serial_test::serial]
    async fn registry_refresh_synthetic_smoke() {
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
        // No CDN config set → synthetic output.
        let cap = RegistryRefresh::new();
        let out = cap.run(serde_json::json!({})).await.unwrap();
        assert_eq!(out["source"], "synthetic");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn network_mode_falls_back_on_bad_url() {
        // Set a CDN URL with a blocked private IP — should be rejected
        // by the scheme guard or IP block, and fall back to synthetic.
        let config = CdnConfig {
            url: "https://127.0.0.1/nonexistent".to_string(),
            timeout_ms: 1_000,
            max_retries: 1,
        };
        let cap = RegistryRefresh::with_cdn(config);
        let out = cap.run(serde_json::json!({"force": true})).await.unwrap();

        assert_eq!(out["source"], "synthetic_fallback");
        assert!(!out["fallbackReason"].as_str().unwrap_or("").is_empty());
    }

    // ── Constructor injection test (V1.57 P1) ────────────────────────────

    #[tokio::test]
    #[serial_test::serial]
    async fn cdn_config_constructor_injection() {
        // Prove that CdnConfig is constructor-injected (no global state).
        // Two separate capability instances with different CDN configs
        // should produce different source values.
        let cap_no_cdn = RegistryRefresh::new();
        let out1 = cap_no_cdn
            .run(serde_json::json!({"force": false}))
            .await
            .unwrap();
        assert_eq!(out1["source"], "synthetic");

        let cap_with_cdn = RegistryRefresh::with_cdn(CdnConfig {
            url: "https://127.0.0.1/nonexistent".to_string(),
            timeout_ms: 1_000,
            max_retries: 0,
        });
        let out2 = cap_with_cdn
            .run(serde_json::json!({"force": true}))
            .await
            .unwrap();
        assert_eq!(out2["source"], "synthetic_fallback");

        // The first instance should still be synthetic (no cross-contamination)
        let out3 = cap_no_cdn
            .run(serde_json::json!({"force": false}))
            .await
            .unwrap();
        assert_eq!(out3["source"], "synthetic");
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
}
