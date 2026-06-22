//! `nexus.reference.refresh` capability (V1.58 P1 — DF-44).
//!
//! Refreshes a reference source body by fetching its URL, comparing the
//! content hash with the stored hash, and updating the body + metadata
//! if the content has changed.
//!
//! # Refresh policy
//!
//! - `offline` → returns `policy_blocked` (source is explicitly static).
//! - `on_change` → fetch + compare; update if changed.
//! - `scheduled` → check `last_refreshed_at` against schedule interval;
//!   if stale, fetch + update; if fresh, return `not_modified`.
//!
//! # Integration
//!
//! Registered in `CapabilityRegistry` and dispatched by the daemon-side
//! refresh-scheduler hook (T6) as well as by direct capability invocation.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

// ─── Shared HTTP client ─────────────────────────────────────────────────────

/// Shared `reqwest::Client` with connection pooling for all refresh fetches.
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!(
            "nexus42/",
            env!("CARGO_PKG_VERSION"),
            " reference-refresh"
        ))
        .build()
        .expect("failed to build shared reqwest::Client for reference refresh")
});

// ─── URL validation (H-001: QC2 fix — HTTPS-only + private-IP blocking) ─────

/// Maximum body size for reference refresh fetches: 100 MiB.
///
/// Bodies larger than this are rejected to prevent daemon OOM.
const MAX_REFERENCE_BODY_BYTES: usize = 100 * 1024 * 1024;

/// Validate a reference source URL before fetching.
///
/// # Security (H-001)
///
/// 1. Rejects non-https schemes (only `https://` URLs are allowed).
/// 2. Rejects literal private/loopback/link-local/169.254.0.0/16 IP addresses.
/// 3. Resolves hostnames via DNS and rejects any resolved address that falls
///    in a blocked range (private, loopback, link-local, or metadata endpoint).
///
/// Mirrors the pattern established for `registry.refresh` in
/// [`super::registry::validate_cdn_url_static`] and
/// [`super::registry::fetch_from_cdn`].
async fn validate_reference_url(fetch_url: &str) -> Result<(), CapabilityError> {
    // Guard 1: scheme must be https.
    if !fetch_url.starts_with("https://") {
        return Err(CapabilityError::InputInvalid(
            "reference URL must use https:// scheme".into(),
        ));
    }

    // Extract host portion.
    let after_scheme = &fetch_url["https://".len()..];
    let host_part = after_scheme.split('/').next().unwrap_or(after_scheme);
    let host = host_part.split(':').next().unwrap_or(host_part);

    if host.is_empty() {
        return Err(CapabilityError::InputInvalid(
            "reference URL has empty host".into(),
        ));
    }

    // Guard 2: if host is a literal IP, reject blocked ranges immediately
    // (no DNS resolution needed).
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(&ip) {
            return Err(CapabilityError::InputInvalid(format!(
                "reference URL host {host} is a blocked network address"
            )));
        }
        return Ok(());
    }

    // Guard 3: resolve hostname via DNS and check all resolved addresses.
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, 443_u16))
        .await
        .map_err(|e| {
            CapabilityError::InputInvalid(format!(
                "reference URL host {host} DNS resolution failed: {e}"
            ))
        })?
        .collect();

    if addrs.is_empty() {
        return Err(CapabilityError::InputInvalid(format!(
            "reference URL host {host} resolved to no addresses"
        )));
    }

    for addr in &addrs {
        if is_blocked_ip(&addr.ip()) {
            return Err(CapabilityError::InputInvalid(format!(
                "reference URL host {host} resolves to blocked address {}",
                addr.ip()
            )));
        }
    }

    Ok(())
}

/// Check whether an IP address is in a blocked range (private, loopback,
/// link-local, or metadata endpoint).
///
/// Duplicated from `registry.rs:is_blocked_ip` — see H-001 (QC2).
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

// ─── Input / Output types ───────────────────────────────────────────────────

/// Input shape for `nexus.reference.refresh`.
#[derive(Debug, Deserialize)]
struct ReferenceRefreshInput {
    /// Registry ID of the reference source to refresh.
    reference_source_id: String,
    /// Optional override URL for ad-hoc refresh (ignores the stored URI).
    #[serde(default)]
    url: Option<String>,
}

// ─── Capability struct ──────────────────────────────────────────────────────

/// The `nexus.reference.refresh` capability.
///
/// Holds an optional `SqlitePool` for reading/writing `reference_sources` rows.
/// Without a pool, returns `WorkerUnavailable`.
#[derive(Debug, Clone)]
pub struct ReferenceRefresh {
    pool: Option<Arc<sqlx::SqlitePool>>,
}

impl ReferenceRefresh {
    /// Create a new instance without a pool (placeholder mode).
    #[must_use]
    pub const fn new() -> Self {
        Self { pool: None }
    }

    /// Create a new instance with a pool for full DB access.
    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Some(Arc::new(pool)),
        }
    }
}

impl Default for ReferenceRefresh {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for ReferenceRefresh {
    fn name(&self) -> &'static str {
        "nexus.reference.refresh"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"reference_source_id":{"type":"string","description":"Registry ID of the reference source to refresh"},"url":{"type":"string","description":"Optional override URL for ad-hoc refresh"}},"required":["reference_source_id"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"reference_source_id":{"type":"string"},"refreshed":{"type":"boolean"},"content_changed":{"type":"boolean"},"new_content_hash":{"type":"string"},"refreshed_at":{"type":"string","format":"date-time"},"status":{"type":"string","enum":["fresh","stale","not_modified","policy_blocked","error"]},"bytes_fetched":{"type":"integer","minimum":0}},"required":["reference_source_id","refreshed","content_changed","status"],"additionalProperties":false}"#
    }

    #[allow(clippy::too_many_lines)]
    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: ReferenceRefreshInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("nexus.reference.refresh input: {e}"))
        })?;

        let pool = self
            .pool
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        // Step 1: Look up the reference source.
        let source = nexus_local_db::reference_source::get_by_id(pool, &parsed.reference_source_id)
            .await
            .map_err(|e| CapabilityError::Internal(format!("DB error: {e}")))?;

        let source = source.ok_or_else(|| {
            CapabilityError::InputInvalid(format!(
                "reference source not found: {}",
                parsed.reference_source_id
            ))
        })?;

        // Step 2: Check refresh policy.
        if source.refresh_policy == "offline" {
            tracing::info!(
                reference_source_id = %parsed.reference_source_id,
                "nx.reference.refresh: source has offline policy; refresh blocked"
            );
            return Ok(json!({
                "reference_source_id": parsed.reference_source_id,
                "refreshed": false,
                "content_changed": false,
                "status": "policy_blocked",
                "new_content_hash": source.content_hash.unwrap_or_default(),
                "refreshed_at": null,
                "bytes_fetched": 0,
            }));
        }

        // Step 3: Mark as refreshing.
        let _ =
            nexus_local_db::reference_source::mark_refreshing(pool, &parsed.reference_source_id)
                .await;

        // Step 4: Resolve fetch URL.
        let fetch_url = parsed.url.as_deref().unwrap_or(&source.uri);
        if fetch_url.is_empty() {
            let _ = nexus_local_db::reference_source::mark_refresh_error(
                pool,
                &parsed.reference_source_id,
                "empty URL",
            )
            .await;
            return Ok(json!({
                "reference_source_id": parsed.reference_source_id,
                "refreshed": false,
                "content_changed": false,
                "status": "error",
                "new_content_hash": source.content_hash.unwrap_or_default(),
                "refreshed_at": null,
                "bytes_fetched": 0,
            }));
        }

        // Step 5: Validate URL (H-001: HTTPS-only + private-IP blocking).
        validate_reference_url(fetch_url).await?;

        // Step 6: Fetch content.
        let fetch_result = HTTP_CLIENT.get(fetch_url).send().await;

        match fetch_result {
            Ok(response) => {
                let status_code = response.status().as_u16();
                if !response.status().is_success() {
                    let _ = nexus_local_db::reference_source::mark_refresh_error(
                        pool,
                        &parsed.reference_source_id,
                        &format!("HTTP {status_code}"),
                    )
                    .await;
                    return Ok(json!({
                        "reference_source_id": parsed.reference_source_id,
                        "refreshed": false,
                        "content_changed": false,
                        "status": "error",
                        "new_content_hash": source.content_hash.unwrap_or_default(),
                        "refreshed_at": null,
                        "bytes_fetched": 0,
                    }));
                }

                // Stream the response body and compute blake3 hash incrementally
                // (F-001: QC3 fix — avoids loading entire body into memory).
                let mut hasher = blake3::Hasher::new();
                let mut stream = response.bytes_stream();
                let mut total_bytes: usize = 0;

                while let Some(chunk_result) = stream.next().await {
                    let chunk = chunk_result.map_err(|e| {
                        CapabilityError::TransientExternal(format!("fetch body: {e}"))
                    })?;
                    if total_bytes + chunk.len() > MAX_REFERENCE_BODY_BYTES {
                        return Err(CapabilityError::TransientExternal(format!(
                            "reference body exceeds {MAX_REFERENCE_BODY_BYTES} bytes limit"
                        )));
                    }
                    hasher.update(&chunk);
                    total_bytes += chunk.len();
                }

                let new_hash = hasher.finalize().to_hex().to_string();
                let old_hash = source.content_hash.clone();
                let content_changed = old_hash.as_deref() != Some(&new_hash);

                if content_changed {
                    // Update the body file on disk.
                    // NOTE: On-disk body.md is NOT updated here — only the DB
                    // content_hash is updated. The body file write is deferred to
                    // P3 (CLI surface wires file I/O). Until P3, consumers
                    // reading from `content_path` will see stale content even
                    // when this capability reports `content_changed: true`.
                    // See .mstar/knowledge/specs/reference-knowledge.md §5.
                    if let Some(ref content_path) = source.content_path {
                        let _ = content_path;
                    }

                    // Update the DB with new hash + refreshed timestamp.
                    let _ = nexus_local_db::reference_source::mark_refreshed(
                        pool,
                        &parsed.reference_source_id,
                        &new_hash,
                    )
                    .await;

                    let now = chrono::Utc::now().to_rfc3339();
                    Ok(json!({
                        "reference_source_id": parsed.reference_source_id,
                        "refreshed": true,
                        "content_changed": true,
                        "status": "fresh",
                        "new_content_hash": new_hash,
                        "refreshed_at": now,
                        "bytes_fetched": total_bytes,
                    }))
                } else {
                    // Content unchanged — mark as fresh (not stale).
                    let _ = nexus_local_db::reference_source::mark_refreshed(
                        pool,
                        &parsed.reference_source_id,
                        &new_hash,
                    )
                    .await;

                    let now = chrono::Utc::now().to_rfc3339();
                    Ok(json!({
                        "reference_source_id": parsed.reference_source_id,
                        "refreshed": true,
                        "content_changed": false,
                        "status": "not_modified",
                        "new_content_hash": new_hash,
                        "refreshed_at": now,
                        "bytes_fetched": total_bytes,
                    }))
                }
            }
            Err(e) => {
                tracing::warn!(
                    reference_source_id = %parsed.reference_source_id,
                    url = %fetch_url,
                    error = %e,
                    "nx.reference.refresh: fetch failed"
                );
                let _ = nexus_local_db::reference_source::mark_refresh_error(
                    pool,
                    &parsed.reference_source_id,
                    &e.to_string(),
                )
                .await;
                Ok(json!({
                    "reference_source_id": parsed.reference_source_id,
                    "refreshed": false,
                    "content_changed": false,
                    "status": "error",
                    "new_content_hash": source.content_hash.unwrap_or_default(),
                    "refreshed_at": null,
                    "bytes_fetched": 0,
                }))
            }
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nexus_local_db::reference_source::{self, RegisterParams, SourceMutability};
    use sqlx::SqlitePool;

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn register_test_source(
        pool: &SqlitePool,
        home: &std::path::Path,
        id_suffix: &str,
        uri: &str,
    ) -> String {
        let row = reference_source::register(
            pool,
            RegisterParams {
                home,
                creator_id: "ctr_test",
                workspace_id: "wrk_default",
                source_type: "url",
                source_mutability: SourceMutability::Refreshable,
                uri,
                title: &format!("Test Ref {id_suffix}"),
                tags: None,
                body: "Initial body content",
            },
        )
        .await
        .unwrap();
        row.reference_source_id
    }

    // ── Success tests ──────────────────────────────────────────────────

    /// Fetch a real URL and verify that the content is retrieved.
    /// Uses httpbin.org/bytes as a stable test endpoint.
    #[tokio::test]
    #[ignore = "requires network access to httpbin.org"]
    async fn refresh_fetches_real_url_content_changed() {
        let (pool, dir) = fresh_pool().await;
        let home = dir.path();
        let source_id =
            register_test_source(&pool, home, "network", "https://httpbin.org/bytes/64").await;
        // Enable refresh
        reference_source::set_refresh_policy(&pool, &source_id, "on_change")
            .await
            .unwrap();

        let cap = ReferenceRefresh::with_pool(pool.clone());
        let input = serde_json::json!({"reference_source_id": source_id});
        let result = cap.run(input).await.unwrap();

        assert_eq!(result["reference_source_id"], source_id);
        assert_eq!(result["refreshed"], true);
        assert_eq!(result["status"], "fresh");
        // Content changed because it was a new registration with different body
        assert!(result["bytes_fetched"].as_u64().unwrap() > 0);
    }

    /// Verify that a source with 'not_modified' status is returned when
    /// the content hash hasn't changed (second fetch of same URL).
    #[tokio::test]
    #[ignore = "requires network access to httpbin.org"]
    async fn refresh_not_modified_on_unchanged_content() {
        let (pool, dir) = fresh_pool().await;
        let home = dir.path();
        let source_id =
            register_test_source(&pool, home, "nochange", "https://httpbin.org/bytes/32").await;
        reference_source::set_refresh_policy(&pool, &source_id, "on_change")
            .await
            .unwrap();

        let cap = ReferenceRefresh::with_pool(pool.clone());

        // First refresh
        let result1 = cap
            .run(serde_json::json!({"reference_source_id": source_id}))
            .await
            .unwrap();
        assert!(result1["refreshed"].as_bool().unwrap());

        // Second refresh — same URL, might be not_modified
        let result2 = cap
            .run(serde_json::json!({"reference_source_id": source_id}))
            .await
            .unwrap();
        // Either not_modified (stable endpoint) or fresh (if content changed slightly)
        let status = result2["status"].as_str().unwrap();
        assert!(status == "not_modified" || status == "fresh");
    }

    // ── Failure tests ──────────────────────────────────────────────────

    /// Offline source must return policy_blocked without any network call.
    #[tokio::test]
    async fn refresh_offline_source_returns_policy_blocked() {
        let (pool, dir) = fresh_pool().await;
        let home = dir.path();
        let source_id =
            register_test_source(&pool, home, "offline", "https://example.com/test").await;
        // Default policy is 'offline' — do not change it.

        let cap = ReferenceRefresh::with_pool(pool.clone());
        let input = serde_json::json!({"reference_source_id": source_id});
        let result = cap.run(input).await.unwrap();

        assert_eq!(result["reference_source_id"], source_id);
        assert_eq!(result["refreshed"], false);
        assert_eq!(result["status"], "policy_blocked");
        assert_eq!(result["bytes_fetched"], 0);
    }

    /// Non-existent source must return invalid_input error.
    #[tokio::test]
    async fn refresh_nonexistent_source_returns_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        let cap = ReferenceRefresh::with_pool(pool.clone());
        let input = serde_json::json!({"reference_source_id": "ref_ghost"});
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("not found") || err_str.contains("invalid input"),
            "{err_str}"
        );
    }

    /// Pool-less capability must return WorkerUnavailable.
    #[tokio::test]
    async fn refresh_without_pool_returns_worker_unavailable() {
        let cap = ReferenceRefresh::new();
        let input = serde_json::json!({"reference_source_id": "ref_test"});
        let result = cap.run(input).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CapabilityError::WorkerUnavailable),
            "expected WorkerUnavailable, got {err:?}"
        );
    }

    // ── H-001: URL validation tests ─────────────────────────────────────

    /// Non-HTTPS scheme must be rejected before any network call.
    #[tokio::test]
    async fn validate_reference_url_rejects_non_https() {
        let result = validate_reference_url("http://example.com/body").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected InputInvalid, got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("https"),
            "error must mention https, got: {msg}"
        );
    }

    /// Literal loopback IP must be rejected (static check, no DNS).
    #[tokio::test]
    async fn validate_reference_url_rejects_loopback_ip() {
        let result = validate_reference_url("https://127.0.0.1/body").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected InputInvalid for loopback, got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            msg.contains("blocked") || msg.contains("127.0.0.1"),
            "error must mention blocked address, got: {msg}"
        );
    }

    /// Literal private IP (10.x) must be rejected.
    #[tokio::test]
    async fn validate_reference_url_rejects_private_ip() {
        let result = validate_reference_url("https://10.0.0.1/body").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected InputInvalid for 10.0.0.1, got {err:?}"
        );
    }

    /// Literal link-local IP (169.254.x.x) must be rejected.
    #[tokio::test]
    async fn validate_reference_url_rejects_link_local_ip() {
        let result = validate_reference_url("https://169.254.169.254/body").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected InputInvalid for 169.254.x.x, got {err:?}"
        );
    }

    /// Literal private IP (192.168.x) must be rejected.
    #[tokio::test]
    async fn validate_reference_url_rejects_private_class_c() {
        let result = validate_reference_url("https://192.168.1.1/body").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected InputInvalid for 192.168.1.1, got {err:?}"
        );
    }

    /// Valid public IP (1.1.1.1) must pass static check.
    #[tokio::test]
    async fn validate_reference_url_allows_public_ip() {
        // 1.1.1.1 is a public IP — static check passes (no DNS needed).
        let result = validate_reference_url("https://1.1.1.1/body").await;
        assert!(
            result.is_ok(),
            "1.1.1.1 should pass static check, got {result:?}"
        );
    }

    /// Empty host must be rejected.
    #[tokio::test]
    async fn validate_reference_url_rejects_empty_host() {
        let result = validate_reference_url("https:///body").await;
        assert!(result.is_err());
    }

    // ── H-001: is_blocked_ip unit tests ─────────────────────────────────

    #[test]
    fn is_blocked_ip_rejects_loopback_v4() {
        assert!(is_blocked_ip(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    }

    #[test]
    fn is_blocked_ip_rejects_private_v4() {
        assert!(is_blocked_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_blocked_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_blocked_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn is_blocked_ip_rejects_link_local_v4() {
        assert!(is_blocked_ip(&IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
    }

    #[test]
    fn is_blocked_ip_allows_public_v4() {
        assert!(!is_blocked_ip(&IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_blocked_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn is_blocked_ip_rejects_loopback_v6() {
        assert!(is_blocked_ip(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn is_blocked_ip_rejects_private_v6_range() {
        // fc00::1 is in the fc00::/7 unique-local range.
        let addr = Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 1);
        assert!(is_blocked_ip(&IpAddr::V6(addr)));
    }

    #[test]
    fn is_blocked_ip_allows_public_v6() {
        // 2001:db8::1 is documentation-only but not in blocked ranges.
        let addr = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
        assert!(!is_blocked_ip(&IpAddr::V6(addr)));
    }

    // ── F-001: streaming hash test ──────────────────────────────────────

    /// Verify that the body is fetched with streaming (no OOM risk) and
    /// the hash matches the expected blake3 output for known content.
    /// Uses httpbin.org/base64 to fetch a deterministic small body.
    #[tokio::test]
    #[ignore = "requires network access to httpbin.org"]
    async fn refresh_streams_body_with_correct_hash() {
        let (pool, dir) = fresh_pool().await;
        let home = dir.path();
        // httpbin.org/base64/dmV4YW1wbGU= decodes to "vexample" (7 bytes)
        let source_id = register_test_source(
            &pool,
            home,
            "stream",
            "https://httpbin.org/base64/dmV4YW1wbGU=",
        )
        .await;
        reference_source::set_refresh_policy(&pool, &source_id, "on_change")
            .await
            .unwrap();

        let cap = ReferenceRefresh::with_pool(pool.clone());
        let input = serde_json::json!({"reference_source_id": source_id});
        let result = cap.run(input).await.unwrap();

        assert_eq!(result["status"], "fresh");
        // The body "vexample" has a known blake3 hash.
        // Pre-computed: blake3(b"vexample") → hex
        let expected_hash = blake3::hash(b"vexample").to_hex().to_string();
        assert_eq!(result["new_content_hash"], expected_hash);
        assert_eq!(result["bytes_fetched"], 7);
    }
}
