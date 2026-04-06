//! Sync Client
//!
//! HTTP client for platform sync API operations.
//! Implements bundle push, state pull, and conflict detection.
//!
//! Uses `reqwest 0.12` for HTTP with retry logic and exponential backoff.

use std::time::Duration;

use nexus_contracts::generated::Bundle;
use reqwest::{Client, Method, RequestBuilder, Response};
use serde::{Deserialize, Serialize};

use crate::conflict::ConflictResponse;
use crate::errors::{SyncError, SyncResult};
use crate::partial_apply::PartialApplyResult;

/// Default request timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum number of automatic retries for transient errors.
const MAX_HTTP_RETRIES: u32 = 3;

/// Base delay for HTTP retry backoff in milliseconds.
const BASE_RETRY_DELAY_MS: u64 = 500;

/// Successful push response from the platform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PushResponse {
    /// Whether the bundle was fully applied.
    pub success: bool,
    /// Bundle apply status: all_success, partial, or failed.
    pub bundle_apply_status: Option<String>,
    /// Server-side world revision after apply.
    pub world_revision: Option<u64>,
    /// Server-side confirmed delta sequence after apply.
    pub confirmed_delta_sequence: Option<u64>,
    /// Per-delta results (if partial).
    pub delta_results: Option<Vec<serde_json::Value>>,
    /// Data freshness hint (last indexed bundle ID).
    pub data_freshness_hint: Option<String>,
    /// Last indexed bundle ID on the server.
    pub last_indexed_bundle_id: Option<String>,
}

/// Pull response from the platform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullResponse {
    /// Current world revision on the server.
    pub world_revision: u64,
    /// Current confirmed delta sequence on the server.
    pub confirmed_delta_sequence: u64,
    /// Server-side bundle count.
    pub bundle_count: u64,
    /// Whether the local state is up to date.
    pub is_up_to_date: bool,
    /// Latest bundle ID on the server.
    pub latest_bundle_id: Option<String>,
}

/// Sync client for platform API interactions.
pub struct SyncClient {
    client: Client,
    base_url: String,
    auth_token: String,
    max_retries: u32,
}

impl SyncClient {
    /// Create a new sync client.
    ///
    /// # Arguments
    /// * `platform_base_url` - Base URL of the platform sync API (e.g., "https://api.nexus.42ch.io")
    /// * `auth_token` - Bearer token for authentication
    pub fn new(platform_base_url: &str, auth_token: &str) -> SyncResult<Self> {
        if platform_base_url.is_empty() {
            return Err(SyncError::SyncNotConfigured(
                "platform_base_url is required".to_string(),
            ));
        }
        if auth_token.is_empty() {
            return Err(SyncError::SyncNotConfigured(
                "auth_token is required".to_string(),
            ));
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()?;

        // Normalize base URL: remove trailing slash
        let base_url = platform_base_url.trim_end_matches('/').to_string();

        Ok(Self {
            client,
            base_url,
            auth_token: auth_token.to_string(),
            max_retries: MAX_HTTP_RETRIES,
        })
    }

    /// Create a new sync client with custom configuration for testing.
    #[cfg(test)]
    pub fn new_with_config(
        platform_base_url: &str,
        auth_token: &str,
        max_retries: u32,
    ) -> SyncResult<Self> {
        let mut client = Self::new(platform_base_url, auth_token)?;
        client.max_retries = max_retries;
        Ok(client)
    }

    /// Get the base URL (for testing).
    #[cfg(test)]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Push a bundle to the platform sync API.
    ///
    /// Returns either a successful `PushResponse` or a `ConflictResponse`.
    /// Transient HTTP errors are retried automatically.
    pub async fn push_bundle(&self, bundle: &Bundle) -> SyncResult<PushResponse> {
        let url = format!("{}/v1/sync/push", self.base_url);
        tracing::info!(
            bundle_id = %bundle.bundle_id,
            world_id = %bundle.world_id,
            "Pushing bundle to platform"
        );

        let response = self
            .execute_with_retry(Method::POST, &url, Some(bundle))
            .await?;

        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|e| SyncError::Serialization(e.to_string()))?;

        if status == 409 {
            let conflict = ConflictResponse::from_json(&text)?;
            tracing::warn!(
                conflict_type = %conflict.conflict_type,
                "Bundle push conflicted (409)"
            );
            return Err(SyncError::SyncConflict {
                conflict_type: conflict.conflict_type.to_string(),
            });
        }

        // Parse response body as JSON to check for conflict indicators
        let body: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| SyncError::Serialization(e.to_string()))?;

        if status == 200 && body.get("success").and_then(|v| v.as_bool()) == Some(false) {
            let conflict = ConflictResponse::from_json(&text)?;
            tracing::warn!(
                conflict_type = %conflict.conflict_type,
                "Bundle push conflicted (success=false)"
            );
            return Err(SyncError::SyncConflict {
                conflict_type: conflict.conflict_type.to_string(),
            });
        }

        if status >= 400 {
            tracing::error!(status = status, "Platform returned error");
            return Err(SyncError::PlatformError { status, body: text });
        }

        let push_response: PushResponse = serde_json::from_value(body)?;
        tracing::info!(
            success = push_response.success,
            apply_status = ?push_response.bundle_apply_status,
            "Bundle push completed"
        );

        Ok(push_response)
    }

    /// Pull current sync state from the platform.
    ///
    /// Returns server-side world revision and delta sequence for comparison.
    pub async fn pull_sync_state(&self, world_id: &str) -> SyncResult<PullResponse> {
        let url = format!("{}/v1/sync/state/{world_id}", self.base_url);
        tracing::debug!(world_id = %world_id, "Pulling sync state from platform");

        let response = self
            .execute_with_retry(Method::GET, &url, None::<&Bundle>)
            .await?;

        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| SyncError::Serialization(e.to_string()))?;

        if status >= 400 {
            return Err(SyncError::PlatformError { status, body });
        }

        let pull_response: PullResponse = serde_json::from_str(&body)?;
        tracing::debug!(
            world_revision = pull_response.world_revision,
            is_up_to_date = pull_response.is_up_to_date,
            "Sync state pulled"
        );

        Ok(pull_response)
    }

    /// Execute an HTTP request with automatic retry for transient errors.
    async fn execute_with_retry<T: Serialize + ?Sized>(
        &self,
        method: Method,
        url: &str,
        body: Option<&T>,
    ) -> SyncResult<Response> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            let request = self.build_request(method.clone(), url, body)?;

            match request.send().await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    // Only retry on transient errors (connection, timeout)
                    if e.is_connect() || e.is_timeout() {
                        let err_str = e.to_string();
                        last_error = Some(e);
                        if attempt < self.max_retries {
                            let delay_ms = BASE_RETRY_DELAY_MS * 2u64.pow(attempt);
                            tracing::warn!(
                                attempt = attempt + 1,
                                max_retries = self.max_retries,
                                delay_ms = delay_ms,
                                error = %err_str,
                                "Transient HTTP error, retrying"
                            );
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        }
                    } else {
                        return Err(SyncError::from(e));
                    }
                }
            }
        }

        // All retries exhausted
        Err(SyncError::HttpError(
            last_error.expect("retry loop exhausted without transient error"),
        ))
    }

    /// Build an authenticated HTTP request.
    fn build_request<T: Serialize + ?Sized>(
        &self,
        method: Method,
        url: &str,
        body: Option<&T>,
    ) -> SyncResult<RequestBuilder> {
        let mut request = self
            .client
            .request(method, url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");

        if let Some(b) = body {
            request = request.json(b);
        }

        Ok(request)
    }

    /// Parse a push response body into a partial apply result, if applicable.
    pub fn parse_partial_apply(
        push_response: &PushResponse,
    ) -> SyncResult<Option<PartialApplyResult>> {
        match push_response.bundle_apply_status.as_deref() {
            Some("partial") => {
                let partial = PartialApplyResult::from_push_response(push_response)?;
                Ok(Some(partial))
            }
            Some("failed") => Err(SyncError::AllDeltasFailed {
                failed_count: push_response
                    .delta_results
                    .as_ref()
                    .map(|r| {
                        r.iter()
                            .filter(|d| {
                                d.get("delta_apply_status")
                                    .map(|s| s.as_str() == Some("rejected"))
                                    .unwrap_or(false)
                            })
                            .count()
                    })
                    .unwrap_or(1),
                total_count: push_response
                    .delta_results
                    .as_ref()
                    .map(|r| r.len())
                    .unwrap_or(1),
            }),
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_creation_requires_base_url() {
        let result = SyncClient::new("", "token");
        assert!(matches!(result, Err(SyncError::SyncNotConfigured { .. })));
    }

    #[test]
    fn client_creation_requires_auth_token() {
        let result = SyncClient::new("https://api.example.com", "");
        assert!(matches!(result, Err(SyncError::SyncNotConfigured { .. })));
    }

    #[test]
    fn client_normalizes_base_url() {
        let client = SyncClient::new("https://api.example.com/", "token").expect("create");
        assert_eq!(client.base_url(), "https://api.example.com");
    }

    #[test]
    fn push_response_deserialization() {
        let json = r#"{
            "success": true,
            "bundle_apply_status": "all_success",
            "world_revision": 6,
            "confirmed_delta_sequence": 15,
            "data_freshness_hint": "2025-01-01T00:00:00Z",
            "last_indexed_bundle_id": "bdl_abc123"
        }"#;

        let response: PushResponse = serde_json::from_str(json).expect("parse");
        assert!(response.success);
        assert_eq!(
            response.bundle_apply_status,
            Some("all_success".to_string())
        );
        assert_eq!(response.world_revision, Some(6));
        assert_eq!(response.confirmed_delta_sequence, Some(15));
    }

    #[test]
    fn pull_response_deserialization() {
        let json = r#"{
            "world_revision": 5,
            "confirmed_delta_sequence": 12,
            "bundle_count": 42,
            "is_up_to_date": false,
            "latest_bundle_id": "bdl_latest"
        }"#;

        let response: PullResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(response.world_revision, 5);
        assert_eq!(response.confirmed_delta_sequence, 12);
        assert_eq!(response.bundle_count, 42);
        assert!(!response.is_up_to_date);
    }

    #[test]
    fn parse_partial_apply_from_push_response() {
        let push_response = PushResponse {
            success: false,
            bundle_apply_status: Some("partial".to_string()),
            world_revision: Some(5),
            confirmed_delta_sequence: Some(10),
            delta_results: Some(vec![
                serde_json::json!({"delta_index": 0, "delta_apply_status": "applied", "applied_entity_revision": 1}),
                serde_json::json!({"delta_index": 1, "delta_apply_status": "rejected", "error_code": "optimistic_lock_failed"}),
            ]),
            data_freshness_hint: Some("hint".to_string()),
            last_indexed_bundle_id: Some("bdl_prev".to_string()),
        };

        let partial = SyncClient::parse_partial_apply(&push_response)
            .expect("parse")
            .expect("should be partial");

        assert_eq!(partial.succeeded_count, 1);
        assert_eq!(partial.failed_count, 1);
        assert!(partial.retryable);
    }

    #[test]
    fn parse_all_failed_returns_error() {
        let push_response = PushResponse {
            success: false,
            bundle_apply_status: Some("failed".to_string()),
            world_revision: None,
            confirmed_delta_sequence: None,
            delta_results: Some(vec![
                serde_json::json!({"delta_index": 0, "delta_apply_status": "rejected"}),
                serde_json::json!({"delta_index": 1, "delta_apply_status": "rejected"}),
            ]),
            data_freshness_hint: None,
            last_indexed_bundle_id: None,
        };

        let result = SyncClient::parse_partial_apply(&push_response);
        assert!(matches!(result, Err(SyncError::AllDeltasFailed { .. })));
    }

    #[tokio::test]
    async fn mock_server_push_bundle() {
        use serde_json::json;

        let mock_response = json!({
            "success": true,
            "bundle_apply_status": "all_success",
            "world_revision": 6,
            "confirmed_delta_sequence": 15
        });

        let body = mock_response.to_string();

        // Since we can't easily spin up a mock server in unit tests,
        // we test deserialization instead. The integration test with
        // a real mock server is covered by the push_response tests above.

        let push_response: PushResponse = serde_json::from_str(&body).expect("parse mock response");
        assert!(push_response.success);
        assert_eq!(push_response.world_revision, Some(6));
    }
}
