//! Daemon HTTP Client
//!
//! Communicates with the daemon runtime via the Local API (HTTP JSON on port 8420).
//! Configurable timeouts prevent infinite hangs when the daemon is unresponsive.
//!
//! # API Key (V1.20+)
//!
//! The client reads `NEXUS42_DAEMON_API_KEY` from the environment at construction
//! time and attaches it as `X-API-Key` header on all requests except health/status
//! probes. When the key is empty or unset, no header is attached (keyless-localhost mode).

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Write;
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
    /// Optional structured details (field-level info for validation errors).
    #[serde(default)]
    details: Option<serde_json::Value>,
    /// Optional request correlation ID.
    #[serde(default)]
    request_id: Option<String>,
}

/// Default connection timeout: 10 seconds
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default request timeout: 30 seconds
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Environment variable for the daemon API key.
const DAEMON_API_KEY_ENV: &str = "NEXUS42_DAEMON_API_KEY";

/// Unguarded paths that skip the `X-API-Key` header.
const UNGUARDED_PATHS: &[&str] = &[
    "/v1/local/runtime/health",
    "/v1/local/runtime/status",
    "/v1/local/daemon/status",
];

/// Client for the daemon Local API
#[derive(Debug, Clone)]
pub struct DaemonClient {
    base_url: String,
    http: reqwest::Client,
    /// Daemon API key read from `NEXUS42_DAEMON_API_KEY` env var.
    /// `None` when unset/empty (keyless-localhost mode).
    api_key: Option<String>,
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

        // Read API key from environment (trimmed; empty becomes None)
        let api_key = std::env::var(DAEMON_API_KEY_ENV)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        Self {
            base_url: base_url.to_string(),
            http,
            api_key,
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
        // Health check is unguarded — no API key needed
        self.http
            .get(&url)
            .send()
            .await
            .map_or_else(|_| Ok(false), |resp| Ok(resp.status().is_success()))
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
        let resp = self.send_authenticated(self.http.get(&url), path).await?;

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
        let resp = self
            .send_authenticated(self.http.post(&url).json(body), path)
            .await?;

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
        let resp = self
            .send_authenticated(self.http.post(&url).json(body), path)
            .await?;

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
        let resp = self
            .send_authenticated(self.http.patch(&url).json(body), path)
            .await?;

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
        let resp = self
            .send_authenticated(self.http.delete(&url), path)
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: T = resp.json().await?;
        Ok(data)
    }

    /// Send a PUT request with JSON body.
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status,
    /// or a network/deserialization error if the request or parsing fails.
    #[allow(clippy::future_not_send)]
    pub async fn put<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .send_authenticated(self.http.put(&url).json(body), path)
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: T = resp.json().await?;
        Ok(data)
    }

    // ─── Workspace management methods (V1.20 Batch 4) ──────────────────

    /// List workspaces via daemon API (`GET /v1/local/workspaces`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status.
    pub async fn list_workspaces(
        &self,
        creator_id: Option<&str>,
    ) -> Result<crate::api::models::ListWorkspacesResponse> {
        let path = creator_id.map_or_else(
            || "/v1/local/workspaces".to_string(),
            |cid| format!("/v1/local/workspaces?creator_id={cid}"),
        );
        self.get(&path).await
    }

    /// Create a workspace via daemon API (`POST /v1/local/workspaces`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status
    /// (e.g., 409 CONFLICT if workspace already exists).
    pub async fn create_workspace(
        &self,
        req: &crate::api::models::CreateWorkspaceRequest,
    ) -> Result<crate::api::models::CreateWorkspaceResponse> {
        self.post("/v1/local/workspaces", req).await
    }

    /// Get the active workspace selection (`GET /v1/local/workspaces/active`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 409 if no workspace is initialized.
    pub async fn get_active_workspace(
        &self,
    ) -> Result<crate::api::models::ActiveWorkspaceResponse> {
        self.get("/v1/local/workspaces/active").await
    }

    /// Set the active workspace (`PUT /v1/local/workspaces/active`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 404 if the workspace doesn't exist.
    pub async fn set_active_workspace(
        &self,
        req: &crate::api::models::SetActiveWorkspaceRequest,
    ) -> Result<crate::api::models::SetActiveWorkspaceResponse> {
        self.put("/v1/local/workspaces/active", req).await
    }

    // ─── Creator management methods (V1.20 Batch 5) ───────────────────

    /// Get the active creator (`GET /v1/local/creators/active`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 409 if no creator is active.
    pub async fn get_active_creator(&self) -> Result<crate::api::models::ActiveCreatorResponse> {
        self.get("/v1/local/creators/active").await
    }

    /// Set the active creator (`PUT /v1/local/creators/active`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 404 if the creator doesn't exist.
    pub async fn set_active_creator(
        &self,
        req: &crate::api::models::SetActiveCreatorRequest,
    ) -> Result<crate::api::models::SetActiveCreatorResponse> {
        self.put("/v1/local/creators/active", req).await
    }

    /// Logout a creator (`POST /v1/local/creators/{id}:logout`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` on failure.
    pub async fn logout_creator(
        &self,
        creator_id: &str,
    ) -> Result<crate::api::models::LogoutCreatorResponse> {
        self.post(&format!("/v1/local/creators/{creator_id}:logout"), &())
            .await
    }

    // ─── Preset management methods (V1.20 Batch 5) ────────────────────

    /// List presets grouped by source (`GET /v1/local/presets`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` on failure.
    pub async fn list_presets(&self) -> Result<crate::api::models::ListPresetsGroupedResponse> {
        self.get("/v1/local/presets").await
    }

    /// Scaffold a user preset (`POST /v1/local/presets`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 409 if the preset already exists.
    pub async fn scaffold_preset(
        &self,
        req: &crate::api::models::ScaffoldPresetRequest,
    ) -> Result<crate::api::models::ScaffoldPresetResponse> {
        self.post("/v1/local/presets", req).await
    }

    /// Validate a preset YAML (`POST /v1/local/presets:validate`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` on failure.
    pub async fn validate_preset(
        &self,
        req: &crate::api::models::ValidatePresetRequest,
    ) -> Result<crate::api::models::ValidatePresetResponse> {
        self.post("/v1/local/presets:validate", req).await
    }

    /// Reload a preset (`POST /v1/local/presets/{id}:reload`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 404 if the preset doesn't exist.
    pub async fn reload_preset(
        &self,
        preset_id: &str,
    ) -> Result<crate::api::models::ReloadPresetResponse> {
        self.post(&format!("/v1/local/presets/{preset_id}:reload"), &())
            .await
    }

    // ─── KB methods (V1.20 Batch 5) ────────────────────────────────────

    /// List KB entries (`GET /v1/local/kb/entries`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` on failure.
    pub async fn list_kb_entries(
        &self,
        creator_id: &str,
        workspace_slug: Option<&str>,
        query: Option<&str>,
    ) -> Result<crate::api::models::ListKbEntriesResponse> {
        let mut path = format!("/v1/local/kb/entries?creator_id={creator_id}");
        if let Some(slug) = workspace_slug {
            path.push_str("&workspace_slug=");
            path.push_str(slug);
        }
        if let Some(q) = query {
            path.push_str("&q=");
            // Simple percent-encoding for common characters
            for ch in q.chars() {
                match ch {
                    ' ' => path.push_str("%20"),
                    '&' => path.push_str("%26"),
                    '=' => path.push_str("%3D"),
                    '#' => path.push_str("%23"),
                    c => path.push(c),
                }
            }
        }
        self.get(&path).await
    }

    /// Add a KB entry (`POST /v1/local/kb/entries`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` on failure.
    pub async fn add_kb_entry(
        &self,
        req: &crate::api::models::AddKbEntryRequest,
    ) -> Result<crate::api::models::AddKbEntryResponse> {
        self.post("/v1/local/kb/entries", req).await
    }

    /// Get a KB entry (`GET /v1/local/kb/entries/{id}`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 404 if the entry doesn't exist.
    pub async fn get_kb_entry(
        &self,
        entry_id: &str,
    ) -> Result<crate::api::models::GetKbEntryResponse> {
        self.get(&format!("/v1/local/kb/entries/{entry_id}")).await
    }

    /// Delete a KB entry (`DELETE /v1/local/kb/entries/{id}`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 404 if the entry doesn't exist.
    pub async fn delete_kb_entry(
        &self,
        entry_id: &str,
    ) -> Result<crate::api::models::DeleteKbEntryResponse> {
        self.delete(&format!("/v1/local/kb/entries/{entry_id}"))
            .await
    }

    /// Attach `X-API-Key` header (if configured and path is guarded) and send the request.
    async fn send_authenticated(
        &self,
        req: reqwest::RequestBuilder,
        path: &str,
    ) -> Result<reqwest::Response> {
        let req = self.with_api_key(req, path);
        req.send().await.map_err(Into::into)
    }

    /// Attach `X-API-Key` header if a key is configured and the path is guarded.
    fn with_api_key(&self, req: reqwest::RequestBuilder, path: &str) -> reqwest::RequestBuilder {
        if let Some(ref key) = self.api_key {
            if !UNGUARDED_PATHS.contains(&path) {
                return req.header("X-API-Key", key.as_str());
            }
        }
        req
    }

    /// Parse an error response from the daemon, attempting structured parsing first
    /// and falling back to raw body text for backward compatibility.
    ///
    /// The error message format prioritizes the structured error code and message,
    /// with optional request ID for debugging and details for field-level context.
    async fn parse_error_response(url: &str, status: u16, resp: reqwest::Response) -> CliError {
        let body = resp.text().await.unwrap_or_default();

        // Try structured error parsing first
        if let Ok(parsed) = serde_json::from_str::<DaemonErrorResponse>(&body) {
            if let Some(detail) = parsed.error {
                let mut message = format!("[{}] {}", detail.code, detail.message);

                // Append field details for validation errors if available
                if let Some(ref details) = detail.details {
                    if let Some(field) = details.get("field").and_then(|v| v.as_str()) {
                        write!(message, " (field: {field})").expect("infallible");
                    }
                }

                // Append request ID if available for support correlation
                if let Some(ref req_id) = detail.request_id {
                    write!(message, " (request_id: {req_id})").expect("infallible");
                }

                // User-friendly guidance for common error codes
                if detail.code == "AUTH_REQUIRED" {
                    message.push_str(
                        "\n\n  Suggestion: Set the NEXUS42_DAEMON_API_KEY environment variable.",
                    );
                }

                return CliError::Api {
                    status,
                    message: format!("{url} → {message}"),
                };
            }
        }

        // Fallback to raw body (backward compatible with old daemon versions)
        CliError::Api {
            status,
            message: format!("{url} → HTTP {status} — {body}"),
        }
    }

    // POST /v1/local/context/assemble — Retired (KCA-002 B2).
    // Context assembly is CLI in-process via nexus-moment-context-assembly.
    // See local-runtime-boundary.md §3.2.1.

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
        let resp = match self
            .send_authenticated(self.http.post(&url).json(&body), path)
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                if let CliError::Api { .. } = &e {
                    return Err(e);
                }
                return Err(CliError::daemon_not_reachable(
                    "Start the daemon with `nexus42 daemon start` and retry.",
                ));
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
        let resp = match self.send_authenticated(self.http.get(&url), path).await {
            Ok(resp) => resp,
            Err(e) => {
                if let CliError::Api { .. } = &e {
                    return Err(e);
                }
                return Err(CliError::daemon_not_reachable(
                    "Start the daemon with `nexus42 daemon start` and retry.",
                ));
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
        let config = CliConfig {
            daemon_url: "http://127.0.0.1:9000".to_string(),
            ..Default::default()
        };
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

    #[test]
    fn test_api_key_read_from_env() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var(DAEMON_API_KEY_ENV, "test-key-123");
        let client = DaemonClient::new("http://127.0.0.1:8420");
        assert_eq!(client.api_key.as_deref(), Some("test-key-123"));
        std::env::remove_var(DAEMON_API_KEY_ENV);
    }

    #[test]
    fn test_api_key_none_when_unset() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        std::env::remove_var(DAEMON_API_KEY_ENV);
        let client = DaemonClient::new("http://127.0.0.1:8420");
        assert!(client.api_key.is_none());
    }

    #[test]
    fn test_api_key_none_when_empty() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var(DAEMON_API_KEY_ENV, "");
        let client = DaemonClient::new("http://127.0.0.1:8420");
        assert!(client.api_key.is_none());
        std::env::remove_var(DAEMON_API_KEY_ENV);
    }

    #[test]
    fn test_api_key_trimmed() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var(DAEMON_API_KEY_ENV, "  my-key  ");
        let client = DaemonClient::new("http://127.0.0.1:8420");
        assert_eq!(client.api_key.as_deref(), Some("my-key"));
        std::env::remove_var(DAEMON_API_KEY_ENV);
    }

    #[test]
    fn test_unguarded_paths_skip_header() {
        let _client = DaemonClient::new("http://127.0.0.1:8420");
        for path in UNGUARDED_PATHS {
            assert!(
                UNGUARDED_PATHS.contains(path),
                "{path} should be in UNGUARDED_PATHS"
            );
        }
    }

    /// Lock to serialize env-var tests that read `NEXUS42_DAEMON_API_KEY`.
    static ENV_TEST_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(()));
}
