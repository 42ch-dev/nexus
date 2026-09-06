//! Daemon HTTP Client
//!
//! Communicates with the daemon runtime via the Daemon API (HTTP JSON on port 8420).
//! Configurable timeouts prevent infinite hangs when the daemon is unresponsive.
//!
//! # API Key (V1.20+)
//!
//! The client reads `NEXUS42_DAEMON_API_KEY` from the environment at construction
//! time and attaches it as `X-API-Key` header on all requests except health/status
//! probes. When the key is empty or unset, no header is attached (keyless-localhost mode).
//!
//! Remote endpoints require HTTPS. Loopback HTTP bypasses proxies and pins
//! localhost resolution; redirects are disabled for both JSON and SSE requests.
//! API key headers are marked sensitive so Debug output cannot expose them.

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use nexus_contracts::daemon_api::memory::{
    CountPendingReviewsResponse, DeletePendingReviewResponse, ListMemoryFragmentsResponse,
    ListPendingReviewsResponse, ReviewResponse,
};
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
    "/v1/daemon/runtime/health",
    "/v1/daemon/runtime/status",
    "/v1/daemon/runtime/cert-fingerprint",
    "/v1/daemon/daemon/status",
];

/// Client for the Daemon API
#[derive(Debug, Clone)]
pub struct DaemonClient {
    base_url: String,
    http: reqwest::Client,
    /// Daemon API key read from `NEXUS42_DAEMON_API_KEY` env var.
    /// `None` when unset/empty (keyless-localhost mode).
    api_key: Option<reqwest::header::HeaderValue>,
}

/// Keep local HTTP local, and require TLS for remote daemon connections.
fn daemon_transport(base_url: &str) -> Result<reqwest::ClientBuilder> {
    let url =
        url::Url::parse(base_url).map_err(|_| CliError::Config("invalid daemon URL".into()))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(CliError::Config(
            "daemon URL must not contain credentials, a query, or a fragment".into(),
        ));
    }
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(host)) => host == "localhost",
        None => false,
    };
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(CliError::Config(
            "remote daemon URLs require HTTPS; HTTP is allowed only on loopback".into(),
        ));
    }
    // Custom X-API-Key headers must never follow a redirect to another origin.
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .https_only(!loopback);
    if loopback {
        // A proxy or DNS override must not turn local cleartext into remote traffic.
        builder = builder.no_proxy().resolve_to_addrs(
            "localhost",
            &[
                std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
                std::net::SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 0)),
            ],
        );
    }
    Ok(builder)
}

impl DaemonClient {
    /// Create a new daemon client from config with default timeouts.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport configuration is invalid.
    pub fn from_config(config: &CliConfig) -> Result<Self> {
        Self::new(&config.daemon_url)
    }

    /// Create a new daemon client with a custom base URL and default timeouts.
    ///
    /// # Errors
    ///
    /// Returns an error if the transport configuration is invalid.
    pub fn new(base_url: &str) -> Result<Self> {
        Self::with_timeouts(base_url, DEFAULT_CONNECT_TIMEOUT, DEFAULT_REQUEST_TIMEOUT)
    }

    /// Create a new daemon client with custom timeouts.
    ///
    /// # Errors
    ///
    /// Returns a configuration error for insecure/invalid URLs or invalid API
    /// key headers, or a reqwest builder error on construction failure.
    pub fn with_timeouts(
        base_url: &str,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Result<Self> {
        // `From<reqwest::Error> for CliError` maps connect/timeout errors to
        // `DaemonNotRunning`; construction failures here are builder-level
        // (invalid TLS/configuration), surfaced as `CliError::Network`.
        let http = daemon_transport(base_url)?
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            .build()?;

        // Read API key from environment (trimmed; empty becomes None)
        let api_key = std::env::var(DAEMON_API_KEY_ENV)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut value = reqwest::header::HeaderValue::from_str(&s)
                    .map_err(|_| CliError::Config("invalid daemon API key header".into()))?;
                value.set_sensitive(true);
                Ok::<_, CliError>(value)
            })
            .transpose()?;

        Ok(Self {
            base_url: base_url.to_string(),
            http,
            api_key,
        })
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
        let url = format!("{}/v1/daemon/runtime/health", self.base_url);
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
        self.get("/v1/daemon/runtime/status").await
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


    /// Poll-friendly GET for Character operation outcomes.
    ///
    /// Returns `None` when the daemon responds **404** (expired or not yet
    /// registered). Other non-success statuses surface as [`CliError::Api`].
    pub async fn get_character_operation_result(
        &self,
        path: &str,
    ) -> Result<Option<nexus_contracts::daemon_api::agent_host::character_operation_result::CharacterOperationResult>> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.send_authenticated(self.http.get(&url), path).await?;
        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }
        let data = resp.json().await?;
        Ok(Some(data))
    }

    /// Open a streaming GET (no whole-body timeout) for SSE endpoints.
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status,
    /// or a network error if the request fails.
    pub async fn stream_get(&self, path: &str) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let http = daemon_transport(&self.base_url)?
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .build()?;
        let resp = self.send_authenticated(http.get(&url), path).await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }
        Ok(resp)
    }

    /// Send a POST request with JSON body.
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status,
    /// or a network/deserialization error if the request or parsing fails.
    #[allow(dead_code)] // For upcoming sync / daemon API commands
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
    #[allow(dead_code)] // For upcoming sync / daemon API commands
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
    /// Execute a tool through the daemon spine, preserving the wire outcome
    /// (V1.174 P0 T5, AR-70 #4).
    ///
    /// Unlike [`Self::post_raw`], this does NOT fold non-2xx statuses into
    /// `CliError::Api` — the spine's structured error body
    /// (`{ success: false, error: { code, message, details } }`) is what the
    /// MCP bridge maps (unroutable vs executed-but-failed vs
    /// auth-rejected). Network failures (connect/timeout) still surface as
    /// errors, bounded by the client's connect/request timeouts.
    ///
    /// # Errors
    ///
    /// Returns a network (or other non-`Api`) error when the request cannot
    /// be delivered; spine error responses are returned as
    /// [`SpineToolExecution`], never as errors.
    #[allow(clippy::future_not_send)]
    pub async fn post_execution_raw(
        &self,
        tool_name: &str,
        parameters: serde_json::Value,
    ) -> Result<crate::api::models::SpineToolExecution> {
        let path = "/v1/daemon/agent-host/internal/tool-executions";
        let url = format!("{}{}", self.base_url, path);
        let body = serde_json::json!({
            "tool_name": tool_name,
            "parameters": parameters,
        });
        let resp = self
            .send_authenticated(self.http.post(&url).json(&body), path)
            .await?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let body: serde_json::Value =
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        Ok(crate::api::models::SpineToolExecution { status, body })
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

    /// Send a DELETE request expecting a success status with no meaningful
    /// body (the daemon's 204 No Content deletes — reading progress /
    /// annotations). Unlike [`Self::delete`], the response body is never
    /// parsed: a 204 response is empty and would fail JSON deserialization.
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP
    /// status, or a network error if the request fails.
    #[allow(clippy::future_not_send)]
    pub async fn delete_no_content(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .send_authenticated(self.http.delete(&url), path)
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        Ok(())
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

    /// List workspaces via daemon API (`GET /v1/daemon/workspaces`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status.
    pub async fn list_workspaces(
        &self,
        creator_id: Option<&str>,
    ) -> Result<crate::api::models::ListWorkspacesResponse> {
        let path = creator_id.map_or_else(
            || "/v1/daemon/workspaces".to_string(),
            |cid| format!("/v1/daemon/workspaces?creator_id={cid}"),
        );
        self.get(&path).await
    }

    /// Create a workspace via daemon API (`POST /v1/daemon/workspaces`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` if the daemon returns a non-success HTTP status
    /// (e.g., 409 CONFLICT if workspace already exists).
    pub async fn create_workspace(
        &self,
        req: &crate::api::models::CreateWorkspaceRequest,
    ) -> Result<crate::api::models::CreateWorkspaceResponse> {
        self.post("/v1/daemon/workspaces", req).await
    }

    /// Get the active workspace selection (`GET /v1/daemon/workspaces/active`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 409 if no workspace is initialized.
    pub async fn get_active_workspace(
        &self,
    ) -> Result<crate::api::models::ActiveWorkspaceResponse> {
        self.get("/v1/daemon/workspaces/active").await
    }

    /// Set the active workspace (`PUT /v1/daemon/workspaces/active`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 404 if the workspace doesn't exist.
    pub async fn set_active_workspace(
        &self,
        req: &crate::api::models::SetActiveWorkspaceRequest,
    ) -> Result<crate::api::models::SetActiveWorkspaceResponse> {
        self.put("/v1/daemon/workspaces/active", req).await
    }

    // ─── Creator management methods (V1.20 Batch 5) ───────────────────

    /// Get the active creator (`GET /v1/daemon/creators/active`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 409 if no creator is active.
    pub async fn get_active_creator(&self) -> Result<crate::api::models::ActiveCreatorResponse> {
        self.get("/v1/daemon/creators/active").await
    }

    /// Set the active creator (`PUT /v1/daemon/creators/active`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 404 if the creator doesn't exist.
    pub async fn set_active_creator(
        &self,
        req: &crate::api::models::SetActiveCreatorRequest,
    ) -> Result<crate::api::models::SetActiveCreatorResponse> {
        self.put("/v1/daemon/creators/active", req).await
    }

    /// Logout a creator (`POST /v1/daemon/creators/{id}:logout`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` on failure.
    pub async fn logout_creator(
        &self,
        creator_id: &str,
    ) -> Result<crate::api::models::LogoutCreatorResponse> {
        self.post(&format!("/v1/daemon/creators/{creator_id}:logout"), &())
            .await
    }

    // ─── Preset management methods (V1.20 Batch 5) ────────────────────

    /// List presets grouped by source (`GET /v1/daemon/presets`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` on failure.
    pub async fn list_presets(&self) -> Result<crate::api::models::ListPresetsGroupedResponse> {
        self.get("/v1/daemon/presets").await
    }

    /// Scaffold a user preset (`POST /v1/daemon/presets`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 409 if the preset already exists.
    pub async fn scaffold_preset(
        &self,
        req: &crate::api::models::ScaffoldPresetRequest,
    ) -> Result<crate::api::models::ScaffoldPresetResponse> {
        self.post("/v1/daemon/presets", req).await
    }

    /// Validate a preset YAML (`POST /v1/daemon/presets:validate`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` on failure.
    pub async fn validate_preset(
        &self,
        req: &crate::api::models::ValidatePresetRequest,
    ) -> Result<crate::api::models::ValidatePresetResponse> {
        self.post("/v1/daemon/presets:validate", req).await
    }

    /// Reload a preset (`POST /v1/daemon/presets/{id}:reload`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 404 if the preset doesn't exist.
    pub async fn reload_preset(
        &self,
        preset_id: &str,
    ) -> Result<crate::api::models::ReloadPresetResponse> {
        self.post(&format!("/v1/daemon/presets/{preset_id}:reload"), &())
            .await
    }

    // ─── KB methods (V1.20 Batch 5) ────────────────────────────────────

    /// List KB entries (`GET /v1/daemon/kb/entries`).
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
        let mut path = format!("/v1/daemon/kb/entries?creator_id={creator_id}");
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

    /// Add a KB entry (`POST /v1/daemon/kb/entries`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` on failure.
    pub async fn add_kb_entry(
        &self,
        req: &crate::api::models::AddKbEntryRequest,
    ) -> Result<crate::api::models::AddKbEntryResponse> {
        self.post("/v1/daemon/kb/entries", req).await
    }

    /// Get a KB entry (`GET /v1/daemon/kb/entries/{id}`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 404 if the entry doesn't exist.
    pub async fn get_kb_entry(
        &self,
        entry_id: &str,
    ) -> Result<crate::api::models::GetKbEntryResponse> {
        self.get(&format!("/v1/daemon/kb/entries/{entry_id}")).await
    }

    /// Delete a KB entry (`DELETE /v1/daemon/kb/entries/{id}`).
    ///
    /// # Errors
    ///
    /// Returns `CliError::Api` with 404 if the entry doesn't exist.
    pub async fn delete_kb_entry(
        &self,
        entry_id: &str,
    ) -> Result<crate::api::models::DeleteKbEntryResponse> {
        self.delete(&format!("/v1/daemon/kb/entries/{entry_id}"))
            .await
    }

    /// Attach `X-API-Key` header (if configured and path is guarded) and send the request.
    async fn send_authenticated(
        &self,
        req: reqwest::RequestBuilder,
        path: &str,
    ) -> Result<reqwest::Response> {
        if !path.starts_with('/') || path.starts_with("//") || path.contains('\\') {
            return Err(CliError::Config(
                "daemon API path must be origin-relative".into(),
            ));
        }
        let req = self.with_api_key(req, path);
        req.send().await.map_err(Into::into)
    }

    /// Attach `X-API-Key` header if a key is configured and the path is guarded.
    fn with_api_key(&self, req: reqwest::RequestBuilder, path: &str) -> reqwest::RequestBuilder {
        if let Some(ref key) = self.api_key {
            if !UNGUARDED_PATHS.contains(&path) {
                return req.header("X-API-Key", key.clone());
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
                if let Some(details) = &detail.details {
                    if let Some(field) = details.get("field").and_then(|v| v.as_str()) {
                        write!(message, " (field: {field})").expect("infallible");
                    }
                    // CAS/OCC conflict family (strategy/outline 409s): render
                    // the structured conflict fields so the CLI error names
                    // the current revision, the conflicting node, the
                    // conflicting path, and the recovery hint (AR-83 #5 /
                    // PL-5 — never swallowed).
                    if let Some(rev) = details
                        .get("current_revision")
                        .and_then(serde_json::Value::as_u64)
                    {
                        write!(message, " (current_revision: {rev})").expect("infallible");
                    }
                    if let Some(node) = details.get("node_id").and_then(|v| v.as_str()) {
                        write!(message, " (node_id: {node})").expect("infallible");
                    }
                    if let Some(path) = details.get("conflicting_path").and_then(|v| v.as_str()) {
                        write!(message, " (conflicting_path: {path})").expect("infallible");
                    }
                    if let Some(hint) = details.get("recovery_hint").and_then(|v| v.as_str()) {
                        write!(message, " (recovery_hint: {hint})").expect("infallible");
                    }
                    // World-kb 409s (`WorldKbConflict`, F-12 / AR-85 #1):
                    // the OCC conflict echoes the stale `expected_version`
                    // as `current_version` + the `entity_id` — rendered so
                    // the retry guidance names the exact fields the daemon
                    // prints (never swallowed, PL-5).
                    if let Some(ver) = details
                        .get("current_version")
                        .and_then(serde_json::Value::as_u64)
                    {
                        write!(message, " (current_version: {ver})").expect("infallible");
                    }
                    if let Some(entity) = details.get("entity_id").and_then(|v| v.as_str()) {
                        write!(message, " (entity_id: {entity})").expect("infallible");
                    }
                    // Domain validation summaries (strategy/outline/world-kb
                    // 422s): the daemon's top-level message is generic
                    // ("Outline validation failed"); the actionable rule
                    // text lives in `details.validation_summary.errors`.
                    // Render each error so the CLI surfaces the real
                    // violation (PL-5 — never swallowed).
                    if let Some(errors) = details
                        .get("validation_summary")
                        .and_then(|v| v.get("errors"))
                        .and_then(serde_json::Value::as_array)
                    {
                        for error in errors {
                            if let Some(text) = error.as_str() {
                                write!(message, " (validation: {text})").expect("infallible");
                            }
                        }
                    }
                }

                // Append request ID if available for support correlation
                if let Some(req_id) = &detail.request_id {
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

    // POST /v1/daemon/context/assemble — Retired (KCA-002 B2).
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
    pub async fn review_pending_memories(&self, creator_id: &str) -> Result<ReviewResponse> {
        let path = "/v1/daemon/memory/review";
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
                // V1.43 (P1 §3): daemon not reachable → canonical remediation
                return Err(CliError::daemon_not_reachable_with_remediation());
            }
        };

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: ReviewResponse = resp.json().await?;
        Ok(data)
    }

    /// Retrieves stored memory fragments for the given creator, returning
    /// the full `ListMemoryFragmentsResponse` wrapper (`{ "fragments": […] }`)
    /// so `--json` can emit the wire DTO verbatim (AR-83 #3).
    ///
    /// # Errors
    ///
    /// Returns `CliError::DaemonNotReachable` if the daemon is not running
    /// (connection refused or timeout).
    pub async fn list_memory_fragments(
        &self,
        creator_id: &str,
    ) -> Result<ListMemoryFragmentsResponse> {
        let path = "/v1/daemon/memory/fragments";

        let url = format!("{}{}?creator_id={}", self.base_url, path, creator_id);
        let resp = match self.send_authenticated(self.http.get(&url), path).await {
            Ok(resp) => resp,
            Err(e) => {
                if let CliError::Api { .. } = &e {
                    return Err(e);
                }
                // V1.43 (P1 §3): daemon not reachable → canonical remediation
                return Err(CliError::daemon_not_reachable_with_remediation());
            }
        };

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let wrapper: ListMemoryFragmentsResponse = resp.json().await?;
        Ok(wrapper)
    }

    // ─── Pending review methods ──────────────────────────────────────────

    /// List pending reviews for a creator via daemon API.
    ///
    /// `GET /v1/daemon/memory/pending-review?creator_id=...`
    ///
    /// # Errors
    ///
    /// Returns `CliError::DaemonNotReachable` if the daemon is not running.
    pub async fn list_pending_reviews(
        &self,
        creator_id: &str,
        cursor: Option<&str>,
    ) -> Result<ListPendingReviewsResponse> {
        let path = "/v1/daemon/memory/pending-review";

        let mut url = format!("{}{}?creator_id={}", self.base_url, path, creator_id);
        if let Some(cursor) = cursor {
            url.push_str("&cursor=");
            url.push_str(cursor);
        }
        let resp = match self.send_authenticated(self.http.get(&url), path).await {
            Ok(resp) => resp,
            Err(e) => {
                if let CliError::Api { .. } = &e {
                    return Err(e);
                }
                // V1.43 (P1 §3): daemon not reachable → canonical remediation
                return Err(CliError::daemon_not_reachable_with_remediation());
            }
        };

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: ListPendingReviewsResponse = resp.json().await?;
        Ok(data)
    }

    /// Count pending reviews for a creator via daemon API.
    ///
    /// `GET /v1/daemon/memory/pending-review/count?creator_id=...`
    ///
    /// # Errors
    ///
    /// Returns `CliError::DaemonNotReachable` if the daemon is not running.
    pub async fn count_pending_reviews(
        &self,
        creator_id: &str,
    ) -> Result<CountPendingReviewsResponse> {
        let path = "/v1/daemon/memory/pending-review/count";

        let url = format!("{}{}?creator_id={}", self.base_url, path, creator_id);
        let resp = match self.send_authenticated(self.http.get(&url), path).await {
            Ok(resp) => resp,
            Err(e) => {
                if let CliError::Api { .. } = &e {
                    return Err(e);
                }
                // V1.43 (P1 §3): daemon not reachable → canonical remediation
                return Err(CliError::daemon_not_reachable_with_remediation());
            }
        };

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: CountPendingReviewsResponse = resp.json().await?;
        Ok(data)
    }

    /// Dismiss (delete) a pending review via daemon API.
    ///
    /// `DELETE /v1/daemon/memory/pending-review/{id}?creator_id=...`
    ///
    /// # Errors
    ///
    /// Returns `CliError::DaemonNotReachable` if the daemon is not running.
    pub async fn dismiss_pending_review(
        &self,
        pending_id: &str,
        creator_id: &str,
    ) -> Result<DeletePendingReviewResponse> {
        let path = format!("/v1/daemon/memory/pending-review/{pending_id}");

        let url = format!("{}{}?creator_id={}", self.base_url, path, creator_id);
        let resp = match self.send_authenticated(self.http.delete(&url), &path).await {
            Ok(resp) => resp,
            Err(e) => {
                if let CliError::Api { .. } = &e {
                    return Err(e);
                }
                // V1.43 (P1 §3): daemon not reachable → canonical remediation
                return Err(CliError::daemon_not_reachable_with_remediation());
            }
        };

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(&url, status, resp).await);
        }

        let data: DeletePendingReviewResponse = resp.json().await?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_or_insecure_daemon_urls() {
        for base_url in [
            "",
            "http://198.51.100.1:8420",
            "http://daemon.example",
            "http://localhost.example",
            "http://localhost@daemon.example",
            "ftp://127.0.0.1:8420",
        ] {
            assert!(
                matches!(DaemonClient::new(base_url), Err(CliError::Config(_))),
                "insecure daemon endpoint accepted: {base_url}"
            );
        }
    }

    #[test]
    #[serial_test::serial]
    fn api_key_is_redacted_from_client_and_request_debug() {
        let original_key = std::env::var_os(DAEMON_API_KEY_ENV);
        std::env::set_var(DAEMON_API_KEY_ENV, "  sentinel-daemon-secret  ");
        let client = DaemonClient::new("http://127.0.0.1:8420").expect("valid loopback URL");
        match original_key {
            Some(value) => std::env::set_var(DAEMON_API_KEY_ENV, value),
            None => std::env::remove_var(DAEMON_API_KEY_ENV),
        }
        let request = client
            .with_api_key(client.http.get("http://127.0.0.1:8420/private"), "/private")
            .build()
            .unwrap();
        assert!(!format!("{client:?}").contains("sentinel-daemon-secret"));
        assert!(!format!("{request:?}").contains("sentinel-daemon-secret"));
        assert_eq!(request.headers()["X-API-Key"], "sentinel-daemon-secret");
        for path in [
            "/v1/daemon/runtime/health",
            "/v1/daemon/runtime/cert-fingerprint",
        ] {
            let probe = client
                .with_api_key(
                    client.http.get(format!("http://127.0.0.1:8420{path}")),
                    path,
                )
                .build()
                .unwrap();
            assert!(!probe.headers().contains_key("X-API-Key"));
        }
    }

    #[tokio::test]
    async fn authenticated_requests_do_not_follow_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let destination = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let redirect_address = destination.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut connection, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 4096];
                let _ = connection.read(&mut request).await.unwrap();
                connection.write_all(format!(
                    "HTTP/1.1 302 Found\r\nLocation: http://{redirect_address}/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ).as_bytes()).await.unwrap();
            }
        });
        let mut client = DaemonClient::with_timeouts(
            &format!("http://{address}"),
            Duration::from_millis(200),
            Duration::from_millis(400),
        )
        .unwrap();
        client.api_key = Some("redirect-secret".parse().unwrap());
        assert!(matches!(
            client.get::<serde_json::Value>("/private").await,
            Err(CliError::Api { status: 302, .. })
        ));
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), client.stream_get("/events"))
                .await
                .unwrap(),
            Err(CliError::Api { status: 302, .. })
        ));
        server.await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), destination.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_health_check_returns_false_on_connection_refused() {
        // Use a port that nothing is listening on — should return Ok(false) quickly
        let client = DaemonClient::with_timeouts(
            "http://127.0.0.1:19998",
            Duration::from_secs(1),
            Duration::from_secs(2),
        )
        .expect("M-2: health check client builds");
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
        // A listening socket that never responds proves the request deadline,
        // without depending on external routing or reaching a remote HTTP host.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = DaemonClient::with_timeouts(
            &format!("http://{address}"),
            Duration::from_millis(100),
            Duration::from_millis(200),
        )
        .expect("M-2: timeout test client builds");

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
