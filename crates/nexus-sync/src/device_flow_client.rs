//! Device Flow HTTP Client
//!
//! Lightweight HTTP client for RFC 8628 Device Authorization Grant against
//! the Nexus platform endpoints:
//! - `POST /api/v1/auth/device/code`  — request device authorization
//! - `POST /api/v1/auth/device/token` — poll for token exchange
//!
//! This client is intentionally separate from [`PlatformClient`] which
//! handles creator registration/verification. Device flow is an unauthenticated
//! flow (no bearer token required).

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::errors::{SyncError, SyncResult};

/// Default request timeout in seconds for device flow requests.
const DEVICE_FLOW_TIMEOUT_SECS: u64 = 30;

/// Device flow error codes returned by the platform (RFC 8628 §3.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceFlowError {
    /// User hasn't approved yet — keep polling.
    AuthorizationPending,
    /// Polling too fast — increase interval.
    SlowDown,
    /// Device code has expired — restart flow.
    ExpiredToken,
    /// User denied authorization.
    AccessDenied,
    /// Other error from the platform.
    Other(String),
}

/// Response from `POST /api/v1/auth/device/code`.
///
/// Platform response shape:
/// ```json
/// {
///   "success": true,
///   "data": {
///     "device_code": "...",
///     "user_code": "...",
///     "verification_uri": "...",
///     "verification_uri_complete": "...",
///     "expires_in": 900,
///     "interval": 5
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Optional verification URI with user_code pre-filled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_uri_complete: Option<String>,
    /// Seconds until the device code expires.
    pub expires_in: u64,
    /// Minimum seconds between polling attempts.
    pub interval: u64,
}

/// Response from `POST /api/v1/auth/device/token` on success.
///
/// Platform response shape:
/// ```json
/// {
///   "success": true,
///   "data": {
///     "access_token": "...",
///     "token_type": "Bearer",
///     "expires_in": 3600
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

/// Client for RFC 8628 Device Authorization Grant against the Nexus platform.
pub struct DeviceFlowClient {
    client: Client,
    base_url: String,
}

impl DeviceFlowClient {
    /// Create a new device flow client.
    ///
    /// # Arguments
    /// * `platform_base_url` - Base URL of the platform API (e.g. `https://api.nexus42.io`)
    pub fn new(platform_base_url: &str) -> SyncResult<Self> {
        if platform_base_url.is_empty() {
            return Err(SyncError::SyncNotConfigured(
                "platform_base_url is required".to_string(),
            ));
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(DEVICE_FLOW_TIMEOUT_SECS))
            .build()?;

        let base_url = platform_base_url.trim_end_matches('/').to_string();

        Ok(Self { client, base_url })
    }

    /// Request a device authorization code.
    ///
    /// Calls `POST /api/v1/auth/device/code` with optional `client_id` and `scope`.
    pub async fn request_device_code(
        &self,
        client_id: Option<&str>,
        scope: Option<&str>,
    ) -> SyncResult<DeviceCodeResponse> {
        let url = format!("{}/api/v1/auth/device/code", self.base_url);

        let mut body = serde_json::Map::new();
        if let Some(cid) = client_id {
            body.insert("client_id".to_string(), serde_json::json!(cid));
        }
        if let Some(s) = scope {
            body.insert("scope".to_string(), serde_json::json!(s));
        }

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(SyncError::from)?;

        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|e| SyncError::Serialization(e.to_string()))?;

        if status >= 400 {
            return Err(SyncError::PlatformError { status, body: text });
        }

        // Parse the platform envelope: { success: true, data: { ... } }
        let envelope: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| SyncError::Serialization(e.to_string()))?;

        let data = envelope
            .get("data")
            .ok_or_else(|| {
                SyncError::Serialization("Missing 'data' field in device code response".to_string())
            })?;

        serde_json::from_value::<DeviceCodeResponse>(data.clone())
            .map_err(|e| SyncError::Serialization(e.to_string()))
    }

    /// Poll for a device token.
    ///
    /// Calls `POST /api/v1/auth/device/token` with the `device_code`.
    ///
    /// Returns `Ok(DeviceTokenResponse)` on success.
    /// Returns `Err(DeviceFlowError)` for expected polling errors — callers
    /// should match on the error to decide whether to continue polling.
    pub async fn poll_device_token(
        &self,
        device_code: &str,
        client_id: Option<&str>,
    ) -> Result<DeviceTokenResponse, DeviceFlowError> {
        let url = format!("{}/api/v1/auth/device/token", self.base_url);

        let mut body = serde_json::Map::new();
        body.insert("device_code".to_string(), serde_json::json!(device_code));
        if let Some(cid) = client_id {
            body.insert("client_id".to_string(), serde_json::json!(cid));
        }

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| DeviceFlowError::Other(format!("HTTP error: {e}")))?;

        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .unwrap_or_else(|e| format!("Failed to read response body: {e}"));

        if status == 400 {
            // Parse the error envelope: { success: false, error: { code, details: { error } } }
            if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(error_code) = envelope
                    .get("error")
                    .and_then(|e| e.get("code"))
                    .or_else(|| {
                        envelope
                            .get("error")
                            .and_then(|e| e.get("details"))
                            .and_then(|d| d.get("error"))
                    })
                    .and_then(|v| v.as_str())
                {
                    return Err(match error_code {
                        "authorization_pending" => DeviceFlowError::AuthorizationPending,
                        "slow_down" => DeviceFlowError::SlowDown,
                        "expired_token" => DeviceFlowError::ExpiredToken,
                        "access_denied" => DeviceFlowError::AccessDenied,
                        other => DeviceFlowError::Other(other.to_string()),
                    });
                }
            }
            return Err(DeviceFlowError::Other(format!(
                "Bad request (400): {text}"
            )));
        }

        if status >= 400 {
            return Err(DeviceFlowError::Other(format!(
                "Platform error ({}): {text}",
                status
            )));
        }

        // Parse the success envelope: { success: true, data: { ... } }
        let envelope: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({ "data": &text }));

        let data = envelope.get("data").cloned().unwrap_or(envelope);

        serde_json::from_value::<DeviceTokenResponse>(data).map_err(|e| {
            DeviceFlowError::Other(format!("Failed to parse token response: {e}"))
        })
    }

    /// Get the base URL (for testing).
    #[cfg(test)]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn client_creation_requires_base_url() {
        let result = DeviceFlowClient::new("");
        assert!(matches!(result, Err(SyncError::SyncNotConfigured { .. })));
    }

    #[test]
    fn client_creation_succeeds() {
        let result = DeviceFlowClient::new("https://api.example.com");
        assert!(result.is_ok());
        let client = result.expect("ok");
        assert_eq!(client.base_url(), "https://api.example.com");
    }

    #[test]
    fn client_normalizes_trailing_slash() {
        let result = DeviceFlowClient::new("https://api.example.com/");
        let client = result.expect("ok");
        assert_eq!(client.base_url(), "https://api.example.com");
    }

    #[tokio::test]
    async fn request_device_code_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/device/code"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": {
                    "device_code": "dc_test_abc123",
                    "user_code": "ABCD-EFGH",
                    "verification_uri": "https://auth.nexus42.io/device",
                    "verification_uri_complete": "https://auth.nexus42.io/device?user_code=ABCD-EFGH",
                    "expires_in": 900,
                    "interval": 5
                }
            })))
            .mount(&mock_server)
            .await;

        let client = DeviceFlowClient::new(&mock_server.uri()).expect("create");
        let resp = client
            .request_device_code(None, None)
            .await
            .expect("request_device_code");

        assert_eq!(resp.device_code, "dc_test_abc123");
        assert_eq!(resp.user_code, "ABCD-EFGH");
        assert_eq!(resp.verification_uri, "https://auth.nexus42.io/device");
        assert_eq!(
            resp.verification_uri_complete,
            Some("https://auth.nexus42.io/device?user_code=ABCD-EFGH".to_string())
        );
        assert_eq!(resp.expires_in, 900);
        assert_eq!(resp.interval, 5);
    }

    #[tokio::test]
    async fn request_device_code_server_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/device/code"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": "internal server error"
            })))
            .mount(&mock_server)
            .await;

        let client = DeviceFlowClient::new(&mock_server.uri()).expect("create");
        let result = client.request_device_code(None, None).await;
        assert!(matches!(
            result,
            Err(SyncError::PlatformError { status: 500, .. })
        ));
    }

    #[tokio::test]
    async fn poll_device_token_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/device/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": {
                    "access_token": "eyJhbGciOiJIUzI1NiJ9.test",
                    "token_type": "Bearer",
                    "expires_in": 3600
                }
            })))
            .mount(&mock_server)
            .await;

        let client = DeviceFlowClient::new(&mock_server.uri()).expect("create");
        let resp = client
            .poll_device_token("dc_test_abc123", None)
            .await
            .expect("poll_device_token");

        assert_eq!(resp.access_token, "eyJhbGciOiJIUzI1NiJ9.test");
        assert_eq!(resp.token_type, "Bearer");
        assert_eq!(resp.expires_in, 3600);
    }

    #[tokio::test]
    async fn poll_device_token_authorization_pending() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/device/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "success": false,
                "error": {
                    "code": "authorization_pending",
                    "message": "User has not approved yet",
                    "details": {
                        "error": "authorization_pending"
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = DeviceFlowClient::new(&mock_server.uri()).expect("create");
        let result = client.poll_device_token("dc_test", None).await;
        assert_eq!(result.unwrap_err(), DeviceFlowError::AuthorizationPending);
    }

    #[tokio::test]
    async fn poll_device_token_slow_down() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/device/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "success": false,
                "error": {
                    "code": "slow_down",
                    "message": "Polling too fast",
                    "details": {
                        "error": "slow_down"
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = DeviceFlowClient::new(&mock_server.uri()).expect("create");
        let result = client.poll_device_token("dc_test", None).await;
        assert_eq!(result.unwrap_err(), DeviceFlowError::SlowDown);
    }

    #[tokio::test]
    async fn poll_device_token_expired_token() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/device/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "success": false,
                "error": {
                    "code": "expired_token",
                    "message": "Device code expired",
                    "details": {
                        "error": "expired_token"
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = DeviceFlowClient::new(&mock_server.uri()).expect("create");
        let result = client.poll_device_token("dc_test", None).await;
        assert_eq!(result.unwrap_err(), DeviceFlowError::ExpiredToken);
    }

    #[tokio::test]
    async fn poll_device_token_access_denied() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/device/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "success": false,
                "error": {
                    "code": "access_denied",
                    "message": "User denied authorization",
                    "details": {
                        "error": "access_denied"
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = DeviceFlowClient::new(&mock_server.uri()).expect("create");
        let result = client.poll_device_token("dc_test", None).await;
        assert_eq!(result.unwrap_err(), DeviceFlowError::AccessDenied);
    }

    #[tokio::test]
    async fn poll_device_token_server_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/device/token"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": "internal server error"
            })))
            .mount(&mock_server)
            .await;

        let client = DeviceFlowClient::new(&mock_server.uri()).expect("create");
        let result = client.poll_device_token("dc_test", None).await;
        match result.unwrap_err() {
            DeviceFlowError::Other(msg) => {
                assert!(msg.contains("500"));
            }
            other => panic!("Expected DeviceFlowError::Other, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn device_code_response_deserialization_without_complete_uri() {
        let json = serde_json::json!({
            "device_code": "dc_123",
            "user_code": "ABCD",
            "verification_uri": "https://auth.example.com",
            "expires_in": 600,
            "interval": 3
        });
        let resp: DeviceCodeResponse = serde_json::from_value(json).expect("parse");
        assert_eq!(resp.device_code, "dc_123");
        assert_eq!(resp.user_code, "ABCD");
        assert!(resp.verification_uri_complete.is_none());
    }

    #[tokio::test]
    async fn device_token_response_deserialization() {
        let json = serde_json::json!({
            "access_token": "tok_abc",
            "token_type": "Bearer",
            "expires_in": 1800
        });
        let resp: DeviceTokenResponse = serde_json::from_value(json).expect("parse");
        assert_eq!(resp.access_token, "tok_abc");
        assert_eq!(resp.token_type, "Bearer");
        assert_eq!(resp.expires_in, 1800);
    }
}
