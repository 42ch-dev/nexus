//! Platform Client
//!
//! HTTP client for non-sync platform interactions — creator registration,
//! verification, and future entitlement/auth operations.
//!
//! Semantically distinct from [`SyncClient`](crate::sync_client) which handles
//! bundle push/pull/outbox operations. The `PlatformClient` shares the same
//! `reqwest` client configuration patterns (timeout, retry, auth headers).
//!
//! # Design Decision
//!
//! Per design doc §5 (Option B): new module rather than extending `SyncClient`
//! to keep registration/verification concerns separate from sync operations.

use reqwest::{Client, Method, RequestBuilder};
use serde::{Deserialize, Serialize};

use crate::errors::{SyncError, SyncResult};

/// Default request timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Request body for creator registration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegisterRequest {
    /// Display name for the creator.
    pub display_name: String,
    /// Registration source (e.g. "cli", "web_agent").
    pub registration_source: String,
}

/// Response from creator registration endpoint.
///
/// Matches the platform API contract (design doc §2):
/// ```json
/// {
///   "creator_id": "...",
///   "display_name": "...",
///   "creator_api_key": "nexus_live_...",
///   "verification": {
///     "verification_code": "nxc_verify_abc123...",
///     "challenge_text": "...",
///     "expires_at": "2026-04-16T00:05:00.000Z",
///     "instructions": "Solve the math problem..."
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegisterResponse {
    /// Unique creator identifier.
    pub creator_id: String,
    /// Display name as registered.
    pub display_name: String,
    /// Pending API key (activated after verification).
    pub creator_api_key: String,
    /// Verification challenge details.
    pub verification: VerificationChallenge,
}

/// Verification challenge returned by the registration endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationChallenge {
    /// Unique code for this verification attempt.
    pub verification_code: String,
    /// Obfuscated math challenge text.
    pub challenge_text: String,
    /// ISO 8601 expiry timestamp.
    pub expires_at: String,
    /// Human-readable instructions.
    pub instructions: String,
}

/// Request body for challenge verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifyRequest {
    /// The verification code from the registration response.
    pub verification_code: String,
    /// The solved answer (numeric string).
    pub answer: String,
}

/// Response from the verification endpoint.
///
/// Matches the platform API contract (design doc §2):
/// ```json
/// {
///   "status": "verified" | "wrong_answer" | "expired" | "locked",
///   "creator_api_key": "nexus_live_...",
///   "remaining_attempts": 2
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifyResponse {
    /// Verification result status.
    pub status: VerifyStatus,
    /// The activated API key (only present on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_api_key: Option<String>,
    /// Remaining verification attempts (on wrong_answer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_attempts: Option<u32>,
}

/// Verification result status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VerifyStatus {
    /// Challenge solved successfully.
    Verified,
    /// Answer was incorrect.
    WrongAnswer,
    /// Challenge has expired.
    Expired,
    /// Account is permanently locked.
    Locked,
}

/// Platform client for non-sync API interactions.
///
/// Handles creator registration and verification flows.
pub struct PlatformClient {
    client: Client,
    base_url: String,
    auth_token: String,
}

impl PlatformClient {
    /// Create a new platform client.
    ///
    /// # Arguments
    /// * `platform_base_url` - Base URL of the platform API (e.g. `https://api.nexus42.invalid`)
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
            .timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()?;

        let base_url = platform_base_url.trim_end_matches('/').to_string();

        Ok(Self {
            client,
            base_url,
            auth_token: auth_token.to_string(),
        })
    }

    /// Register a new creator on the platform.
    ///
    /// Calls `POST /api/v1/creators/register` with the display name
    /// and registration source.
    pub async fn register_creator(
        &self,
        display_name: &str,
        registration_source: &str,
    ) -> SyncResult<RegisterResponse> {
        let url = format!("{}/api/v1/creators/register", self.base_url);
        let body = RegisterRequest {
            display_name: display_name.to_string(),
            registration_source: registration_source.to_string(),
        };

        tracing::info!(display_name = %display_name, "Registering creator on platform");

        let response = self
            .execute_request(Method::POST, &url, Some(&body))
            .await?;

        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|e| SyncError::Serialization(e.to_string()))?;

        if status >= 400 {
            tracing::error!(status = status, "Creator registration failed");
            return Err(SyncError::PlatformError { status, body: text });
        }

        serde_json::from_str::<RegisterResponse>(&text)
            .map_err(|e| SyncError::Serialization(e.to_string()))
    }

    /// Verify a creator registration by submitting the challenge answer.
    ///
    /// Calls `POST /api/v1/creators/verify` with the verification code
    /// and the solved answer.
    pub async fn verify_creator(
        &self,
        verification_code: &str,
        answer: &str,
    ) -> SyncResult<VerifyResponse> {
        let url = format!("{}/api/v1/creators/verify", self.base_url);
        let body = VerifyRequest {
            verification_code: verification_code.to_string(),
            answer: answer.to_string(),
        };

        tracing::info!("Verifying creator challenge");

        let response = self
            .execute_request(Method::POST, &url, Some(&body))
            .await?;

        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|e| SyncError::Serialization(e.to_string()))?;

        if status >= 400 {
            tracing::error!(status = status, "Creator verification failed");
            return Err(SyncError::PlatformError { status, body: text });
        }

        serde_json::from_str::<VerifyResponse>(&text)
            .map_err(|e| SyncError::Serialization(e.to_string()))
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

    /// Execute an HTTP request.
    async fn execute_request<T: Serialize + ?Sized>(
        &self,
        method: Method,
        url: &str,
        body: Option<&T>,
    ) -> SyncResult<reqwest::Response> {
        let request = self.build_request(method, url, body)?;
        request.send().await.map_err(SyncError::from)
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

    // ── Struct serialization/deserialization tests ──────────────────

    #[test]
    fn register_request_serialization() {
        let req = RegisterRequest {
            display_name: "Test Creator".to_string(),
            registration_source: "cli".to_string(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains("Test Creator"));
        assert!(json.contains("cli"));
    }

    #[test]
    fn register_response_deserialization() {
        let json = r#"{
            "creator_id": "crt_abc123",
            "display_name": "Test Creator",
            "creator_api_key": "nexus_live_pending_key",
            "verification": {
                "verification_code": "nxc_verify_abc123",
                "challenge_text": "A basket has thirty five apples...",
                "expires_at": "2026-04-16T00:05:00.000Z",
                "instructions": "Solve the math problem"
            }
        }"#;

        let resp: RegisterResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(resp.creator_id, "crt_abc123");
        assert_eq!(resp.display_name, "Test Creator");
        assert_eq!(resp.creator_api_key, "nexus_live_pending_key");
        assert_eq!(resp.verification.verification_code, "nxc_verify_abc123");
        assert_eq!(resp.verification.expires_at, "2026-04-16T00:05:00.000Z");
    }

    #[test]
    fn verify_request_serialization() {
        let req = VerifyRequest {
            verification_code: "nxc_verify_abc123".to_string(),
            answer: "47".to_string(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains("nxc_verify_abc123"));
        assert!(json.contains("47"));
    }

    #[test]
    fn verify_response_success_deserialization() {
        let json = r#"{
            "status": "verified",
            "creator_api_key": "nexus_live_active_key"
        }"#;

        let resp: VerifyResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(resp.status, VerifyStatus::Verified);
        assert_eq!(
            resp.creator_api_key,
            Some("nexus_live_active_key".to_string())
        );
        assert!(resp.remaining_attempts.is_none());
    }

    #[test]
    fn verify_response_wrong_answer_deserialization() {
        let json = r#"{
            "status": "wrong_answer",
            "remaining_attempts": 2
        }"#;

        let resp: VerifyResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(resp.status, VerifyStatus::WrongAnswer);
        assert!(resp.creator_api_key.is_none());
        assert_eq!(resp.remaining_attempts, Some(2));
    }

    #[test]
    fn verify_response_expired_deserialization() {
        let json = r#"{
            "status": "expired"
        }"#;

        let resp: VerifyResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(resp.status, VerifyStatus::Expired);
    }

    #[test]
    fn verify_response_locked_deserialization() {
        let json = r#"{
            "status": "locked"
        }"#;

        let resp: VerifyResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(resp.status, VerifyStatus::Locked);
    }

    #[test]
    fn verify_status_serde_roundtrip() {
        let statuses = vec![
            VerifyStatus::Verified,
            VerifyStatus::WrongAnswer,
            VerifyStatus::Expired,
            VerifyStatus::Locked,
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).expect("serialize");
            let parsed: VerifyStatus = serde_json::from_str(&json).expect("parse");
            assert_eq!(status, parsed);
        }
    }

    // ── PlatformClient construction tests ──────────────────────────

    #[test]
    fn client_creation_requires_base_url() {
        let result = PlatformClient::new("", "some_token");
        assert!(matches!(result, Err(SyncError::SyncNotConfigured { .. })));
    }

    #[test]
    fn client_creation_requires_auth_token() {
        let result = PlatformClient::new("https://api.example.com", "");
        assert!(matches!(result, Err(SyncError::SyncNotConfigured { .. })));
    }

    #[test]
    fn client_creation_succeeds() {
        let result = PlatformClient::new("https://api.example.com", "some_token");
        assert!(result.is_ok());
        let client = result.expect("ok");
        assert_eq!(client.base_url(), "https://api.example.com");
    }

    #[test]
    fn client_normalizes_trailing_slash() {
        let result = PlatformClient::new("https://api.example.com/", "some_token");
        let client = result.expect("ok");
        assert_eq!(client.base_url(), "https://api.example.com");
    }

    // ── Wiremock integration tests ─────────────────────────────────

    #[tokio::test]
    async fn register_creator_success() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/register"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "creator_id": "crt_test123",
                "display_name": "Test Creator",
                "creator_api_key": "nexus_live_pending_key",
                "verification": {
                    "verification_code": "nxc_verify_test",
                    "challenge_text": "A basket has five apples and someone adds three more",
                    "expires_at": "2026-04-16T00:05:00.000Z",
                    "instructions": "Solve the math problem"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = PlatformClient::new(&mock_server.uri(), "test_token").expect("create");
        let resp = client
            .register_creator("Test Creator", "cli")
            .await
            .expect("register");

        assert_eq!(resp.creator_id, "crt_test123");
        assert_eq!(resp.display_name, "Test Creator");
        assert_eq!(resp.verification.verification_code, "nxc_verify_test");
    }

    #[tokio::test]
    async fn register_creator_server_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/register"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": "internal server error"
            })))
            .mount(&mock_server)
            .await;

        let client = PlatformClient::new(&mock_server.uri(), "test_token").expect("create");
        let result = client.register_creator("Test", "cli").await;
        assert!(matches!(
            result,
            Err(SyncError::PlatformError { status: 500, .. })
        ));
    }

    #[tokio::test]
    async fn verify_creator_success() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "verified",
                "creator_api_key": "nexus_live_active_key"
            })))
            .mount(&mock_server)
            .await;

        let client = PlatformClient::new(&mock_server.uri(), "test_token").expect("create");
        let resp = client
            .verify_creator("nxc_verify_test", "47")
            .await
            .expect("verify");

        assert_eq!(resp.status, VerifyStatus::Verified);
        assert_eq!(
            resp.creator_api_key,
            Some("nexus_live_active_key".to_string())
        );
    }

    #[tokio::test]
    async fn verify_creator_wrong_answer() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "wrong_answer",
                "remaining_attempts": 2
            })))
            .mount(&mock_server)
            .await;

        let client = PlatformClient::new(&mock_server.uri(), "test_token").expect("create");
        let resp = client
            .verify_creator("nxc_verify_test", "99")
            .await
            .expect("verify");

        assert_eq!(resp.status, VerifyStatus::WrongAnswer);
        assert_eq!(resp.remaining_attempts, Some(2));
    }
}
