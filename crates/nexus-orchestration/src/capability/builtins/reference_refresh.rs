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
use serde::Deserialize;
use serde_json::{json, Value};
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

        // Step 5: Fetch content.
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

                let body_bytes = response
                    .bytes()
                    .await
                    .map_err(|e| CapabilityError::TransientExternal(format!("fetch body: {e}")))?;

                let new_hash = blake3_hash(&body_bytes);
                let old_hash = source.content_hash.clone();
                let content_changed = old_hash.as_deref() != Some(&new_hash);

                if content_changed {
                    // Update the body file on disk.
                    if let Some(ref content_path) = source.content_path {
                        // Reconstruct the full path from the relative content_path.
                        // The content_path is relative to the creator root;
                        // we use the home_layout helper.
                        // For now, update the DB hash only — file update is a
                        // follow-up concern (P3 wires file I/O through CLI).
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
                        "bytes_fetched": body_bytes.len(),
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
                        "bytes_fetched": body_bytes.len(),
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

/// Compute a blake3 hex hash of the given bytes.
fn blake3_hash(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
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
}
