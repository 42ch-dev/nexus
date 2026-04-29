//! Daemon HTTP Client
//!
//! Communicates with the nexus42d daemon via the Local API (HTTP JSON on port 8420).
//! Configurable timeouts prevent infinite hangs when the daemon is unresponsive.

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use nexus_domain::AssembleResponse;
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

/// Structured error response from the daemon API
#[derive(Debug, serde::Deserialize)]
struct DaemonErrorResponse {
    #[allow(dead_code)]
    success: bool,
    #[serde(default)]
    error: Option<DaemonErrorDetail>,
}

#[derive(Debug, serde::Deserialize)]
struct DaemonErrorDetail {
    code: String,
    message: String,
}

/// Default connection timeout: 10 seconds
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default request timeout: 30 seconds
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Client for the nexus42d Local API
#[derive(Debug, Clone)]
pub struct DaemonClient {
    base_url: String,
    http: reqwest::Client,
}

impl DaemonClient {
    /// Create a new daemon client from config with default timeouts
    #[must_use] 
    pub fn from_config(config: &CliConfig) -> Self {
        Self::new(&config.daemon_url)
    }

    /// Create a new daemon client with a custom base URL and default timeouts
    #[must_use] 
    pub fn new(base_url: &str) -> Self {
        Self::with_timeouts(base_url, DEFAULT_CONNECT_TIMEOUT, DEFAULT_REQUEST_TIMEOUT)
    }

    /// Create a new daemon client with custom timeouts
    #[must_use] 
    pub fn with_timeouts(
        base_url: &str,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to build reqwest Client, using default");
                reqwest::Client::new()
            });
        Self {
            base_url: base_url.to_string(),
            http,
        }
    }

    /// Get the base URL for this daemon client.
    #[allow(dead_code)]
    #[must_use] 
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Check if the daemon is running and healthy.
    ///
    /// Uses the client's configured timeout. Returns `Ok(false)` on any error
    /// (connection refused, timeout, etc.) rather than propagating errors,
    /// since "not running" is a valid state for health checks.
    ///
    /// # Errors
    ///
    /// This function never returns an error; it absorbs all failures and returns `Ok(false)`.
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/v1/local/runtime/health", self.base_url);
        self.http.get(&url).send().await.map_or_else(|_| Ok(false), |resp| Ok(resp.status().is_success()))
    }

    /// Get runtime status from the daemon.
    ///
    /// Returns information about daemon health, uptime, workspace state,
    /// and ACP session statistics.
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status,
    /// or `CliError::Io`/network error if the request fails.
    pub async fn get_runtime_status(&self) -> Result<crate::api::models::RuntimeStatus> {
        self.get("/v1/local/runtime/status").await
    }

    /// Send a GET request.
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status,
    /// or a network/deserialization error if the request or parsing fails.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: T = resp.json().await?;
        Ok(data)
    }

    /// Send a POST request with JSON body.
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status,
    /// or a network/deserialization error if the request or parsing fails.
    #[allow(dead_code)] // For upcoming sync / local API commands
    #[allow(clippy::future_not_send)]
    pub async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.post(&url).json(body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: T = resp.json().await?;
        Ok(data)
    }

    /// Send a POST request with JSON body, returning raw response.
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status,
    /// or a network/deserialization error if the request or parsing fails.
    #[allow(dead_code)] // For upcoming sync / local API commands
    #[allow(clippy::future_not_send)]
    pub async fn post_raw<B: Serialize>(&self, path: &str, body: &B) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.post(&url).json(body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: serde_json::Value = resp.json().await?;
        Ok(data)
    }

    /// Send a PATCH request with JSON body.
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status,
    /// or a network/deserialization error if the request or parsing fails.
    #[allow(clippy::future_not_send)]
    pub async fn patch<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.patch(&url).json(body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: T = resp.json().await?;
        Ok(data)
    }

    /// Send a DELETE request.
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status,
    /// or a network/deserialization error if the request or parsing fails.
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.delete(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: T = resp.json().await?;
        Ok(data)
    }

    /// Parse an error response from the daemon, attempting structured parsing first
    /// and falling back to raw body text for backward compatibility.
    ///
    /// Includes the requested URL and HTTP status code in the error for easier debugging.
    async fn parse_error_response(url: &str, status: u16, resp: reqwest::Response) -> CliError {
        let body = resp.text().await.unwrap_or_default();

        // Try structured error parsing first
        if let Ok(parsed) = serde_json::from_str::<DaemonErrorResponse>(&body) {
            if let Some(detail) = parsed.error {
                return CliError::Api {
                    status,
                    message: format!("GET {} → [{}] {}", url, detail.code, detail.message),
                };
            }
        }

        // Fallback to raw body (backward compatible with old daemon versions)
        CliError::Api {
            status,
            message: format!("GET {url} → HTTP {status} — {body}"),
        }
    }

    /// Call platform `context/assemble` API via daemon proxy.
    ///
    /// Daemon forwards to platform or returns mock response for testing.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(AssembleResponse))` on success (platform or mock)
    /// - `Ok(None)` on platform unavailable (503) — triggers fallback
    /// - `Err(CliError)` on other errors
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with status 403 if `runtime_mode` is `local_only`
    /// (platform assemble is prohibited in that mode).
    pub async fn call_assemble(
        &self,
        creator_id: &str,
        workspace_slug: &str,
        runtime_mode: &str,
        prompt_hint: Option<&str>,
    ) -> Result<Option<AssembleResponse>> {
        let url = format!("{}/v1/local/context/assemble", self.base_url);

        let body = serde_json::json!({
            "creator_id": creator_id,
            "workspace_slug": workspace_slug,
            "runtime_mode": runtime_mode,
            "prompt_hint": prompt_hint,
        });

        let resp = self.http.post(&url).json(&body).send().await?;

        let status = resp.status().as_u16();

        if status == 503 {
            // Platform unavailable - return None (trigger fallback)
            return Ok(None);
        }

        if status == 403 {
            // local_only mode blocked - return structured error
            return Err(CliError::PlatformOperationProhibited {
                mode: "local_only".to_string(),
                operation: "context assemble".to_string(),
            });
        }

        if !resp.status().is_success() {
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let assemble_resp: AssembleResponse = resp.json().await?;
        Ok(Some(assemble_resp))
    }

    /// Trigger review of pending memories for a creator.
    ///
    /// Posts to the daemon's review endpoint, which processes the pending
    /// review queue and returns a summary of actions taken.
    ///
    /// # Errors
    ///
    /// Returns `CliError::DaemonNotReachable` if the daemon is not running
    /// (connection refused or timeout).
    pub async fn review_pending_memories(
        &self,
        creator_id: &str,
    ) -> Result<crate::api::models::ReviewResponse> {
        let path = "/v1/local/memory/review";
        let body = serde_json::json!({ "creator_id": creator_id });

        let url = format!("{}{}", self.base_url, path);
        let resp = match self.http.post(&url).json(&body).send().await {
            Ok(resp) => resp,
            Err(e) => {
                if e.is_connect() {
                    return Err(CliError::daemon_not_reachable(
                        "Start the daemon with `nexus42 daemon start` and retry.",
                    ));
                }
                return Err(CliError::from(e));
            }
        };

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: crate::api::models::ReviewResponse = resp.json().await?;
        Ok(data)
    }

    /// List memory fragments from the daemon.
    ///
    /// Retrieves stored memory fragments for the given creator, returning
    /// their IDs and summaries.
    ///
    /// # Errors
    ///
    /// Returns `CliError::DaemonNotReachable` if the daemon is not running
    /// (connection refused or timeout).
    pub async fn list_memory_fragments(
        &self,
        creator_id: &str,
    ) -> Result<Vec<crate::api::models::FragmentRow>> {
        let path = "/v1/local/memory/fragments";

        let url = format!("{}{}?creator_id={}", self.base_url, path, creator_id);
        let resp = match self.http.get(&url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                if e.is_connect() {
                    return Err(CliError::daemon_not_reachable(
                        "Start the daemon with `nexus42 daemon start` and retry.",
                    ));
                }
                return Err(CliError::from(e));
            }
        };

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: Vec<crate::api::models::FragmentRow> = resp.json().await?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_client_has_default_timeouts() {
        let client = DaemonClient::new("http://127.0.0.1:8420");
        assert_eq!(client.base_url, "http://127.0.0.1:8420");
    }

    #[test]
    fn test_with_timeouts_builds_client() {
        let client = DaemonClient::with_timeouts(
            "http://127.0.0.1:9999",
            Duration::from_secs(5),
            Duration::from_secs(15),
        );
        assert_eq!(client.base_url, "http://127.0.0.1:9999");
    }

    #[test]
    fn test_from_config_uses_daemon_url() {
        let mut config = CliConfig::default();
        config.daemon_url = "http://127.0.0.1:9000".to_string();
        let client = DaemonClient::from_config(&config);
        assert_eq!(client.base_url, "http://127.0.0.1:9000");
    }

    #[test]
    fn test_default_constants() {
        assert_eq!(DEFAULT_CONNECT_TIMEOUT, Duration::from_secs(10));
        assert_eq!(DEFAULT_REQUEST_TIMEOUT, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_health_check_returns_false_on_connection_refused() {
        // Use a port that nothing is listening on — should return Ok(false) quickly
        let client = DaemonClient::with_timeouts(
            "http://127.0.0.1:19998",
            Duration::from_secs(1),
            Duration::from_secs(2),
        );
        let result = client.health_check().await;
        assert!(
            result.is_ok(),
            "health_check should not error on connection refused"
        );
        assert!(
            !result.expect("health_check result"),
            "health_check should return false when daemon not running"
        );
    }

    #[tokio::test]
    async fn test_timeout_prevents_infinite_hang() {
        // Connect to a non-routable address to test timeout behavior.
        // 198.51.100.1 is TEST-NET-2 (RFC 5737) — should be unreachable and cause a timeout.
        let client = DaemonClient::with_timeouts(
            "http://198.51.100.1:1",
            Duration::from_millis(100),
            Duration::from_millis(200),
        );

        let start = std::time::Instant::now();
        let result = client.health_check().await;
        let elapsed = start.elapsed();

        // Should complete within a reasonable time (well under 5s)
        assert!(
            elapsed < Duration::from_secs(5),
            "Health check should timeout quickly, took {elapsed:?}"
        );
        // Should return Ok(false) regardless of timeout/connection error
        assert!(result.is_ok(), "health_check should absorb timeout errors");
        assert!(!result.expect("health check timeout test"));
    }
}
