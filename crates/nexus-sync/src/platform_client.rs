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

/// Deterministic CLI-visible error bucket for staged platform operations.
///
/// Maps low-level [`SyncError`] variants into a small, stable set of
/// error categories that the CLI can display and test against without
/// leaking internal error details.
///
/// This is the "error shaping" layer for the staged e2e verification
/// harness (DF-14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StagedPlatformError {
    /// Upstream platform request timed out.
    Timeout,
    /// Client configuration is incomplete or invalid.
    Config(String),
    /// Authentication token is missing, empty, or rejected.
    Auth(String),
    /// Platform returned a non-success HTTP response.
    Platform {
        /// HTTP status code.
        status: u16,
        /// Sanitized response body (capped length, secrets redacted).
        body: String,
    },
}

/// Maximum length for displayed error bodies.
const ERROR_BODY_CAP: usize = 200;

/// Sanitize an upstream error body for safe CLI display.
///
/// - Caps the body length to [`ERROR_BODY_CAP`] characters.
/// - Redacts patterns that look like API keys, bearer tokens, or long
///   hex/base64 secrets.
fn sanitize_error_body(body: &str) -> String {
    let mut s = body.to_string();

    // Redact Bearer tokens: "Bearer <token>"
    if let Some(pos) = s.find("Bearer ") {
        let after_bearer = pos + 7; // len("Bearer ")
        let end = s[after_bearer..]
            .find(|c: char| c.is_whitespace() || c == ',' || c == '"' || c == '}')
            .map_or(s.len(), |i| after_bearer + i);
        s.replace_range(pos..end, "Bearer [REDACTED]");
    }

    // Redact sk- prefixed keys (case-insensitive)
    s = redact_prefixed_tokens(&s, "sk-");
    s = redact_prefixed_tokens(&s, "sk_");
    s = redact_prefixed_tokens(&s, "nexus_live_");
    s = redact_prefixed_tokens(&s, "nexus_test_");

    // Redact long hex-like strings (32+ hex chars)
    s = redact_long_pattern(&s, |c: char| c.is_ascii_hexdigit(), 32, "[REDACTED_HEX]");

    // Redact long base64-like strings (40+ alphanumeric+/= chars)
    s = redact_long_pattern(
        &s,
        |c: char| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=',
        40,
        "[REDACTED_B64]",
    );

    // Cap length
    if s.len() > ERROR_BODY_CAP {
        s.truncate(ERROR_BODY_CAP);
        s.push('…');
    }

    s
}

/// Find and replace tokens starting with `prefix` (case-insensitive match).
fn redact_prefixed_tokens(s: &str, prefix: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let prefix_lower = prefix.to_ascii_lowercase();
    let mut result = s.to_string();
    let mut offset = 0;

    while let Some(pos) = lower[offset..].find(&prefix_lower) {
        let abs_pos = offset + pos;
        let after_prefix = abs_pos + prefix.len();
        // Token ends at next whitespace, comma, quote, or brace
        let end = result[after_prefix..]
            .find(|c: char| c.is_whitespace() || c == ',' || c == '"' || c == '}')
            .map_or(result.len(), |i| after_prefix + i);
        result.replace_range(abs_pos..end, "[REDACTED_KEY]");
        // Advance past the replacement to avoid infinite loop
        offset = abs_pos + "[REDACTED_KEY]".len();
        if offset >= result.len() {
            break;
        }
    }

    result
}

/// Redact long runs of characters matching `is_valid` that exceed `min_len`.
fn redact_long_pattern(
    s: &str,
    is_valid: impl Fn(char) -> bool,
    min_len: usize,
    replacement: &str,
) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;

    while i < chars.len() {
        let start = i;
        while i < chars.len() && is_valid(chars[i]) {
            i += 1;
        }
        let run_len = i - start;
        if run_len >= min_len {
            result.push_str(replacement);
        } else {
            for &ch in &chars[start..i] {
                result.push(ch);
            }
        }
        // Append non-matching character(s)
        if i < chars.len() && !is_valid(chars[i]) {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

impl std::fmt::Display for StagedPlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => write!(f, "platform integration failed: timeout"),
            Self::Config(msg) => write!(f, "platform integration failed: config — {msg}"),
            Self::Auth(msg) => write!(f, "platform integration failed: auth — {msg}"),
            Self::Platform { status, body } => {
                write!(f, "platform integration failed: HTTP {status} — {body}")
            }
        }
    }
}

impl std::error::Error for StagedPlatformError {}

/// Classify a [`SyncError`] into a deterministic CLI-visible error bucket.
///
/// The mapping is:
/// - `SyncTimeout` → `Timeout`
/// - `SyncNotConfigured` → `Config`
/// - `AuthTokenInvalid` → `Auth`
/// - `PlatformError` → `Platform`
/// - Everything else → `Platform` with status 0 (uncategorized transport error)
#[must_use]
pub fn classify_platform_error(err: SyncError) -> StagedPlatformError {
    match err {
        SyncError::SyncTimeout { seconds: _ } => StagedPlatformError::Timeout,
        SyncError::SyncNotConfigured(msg) => StagedPlatformError::Config(msg),
        SyncError::AuthTokenInvalid(msg) => StagedPlatformError::Auth(msg),
        SyncError::PlatformError { status, body } => StagedPlatformError::Platform {
            status,
            body: sanitize_error_body(&body),
        },
        SyncError::HttpError(e) => {
            // reqwest timeout errors surface as HttpError with is_timeout()
            if e.is_timeout() {
                StagedPlatformError::Timeout
            } else {
                StagedPlatformError::Platform {
                    status: 0,
                    body: sanitize_error_body(&e.to_string()),
                }
            }
        }
        other => StagedPlatformError::Platform {
            status: 0,
            body: sanitize_error_body(&other.to_string()),
        },
    }
}

/// Request body for creator registration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisterRequest {
    /// Display name for the creator.
    pub display_name: String,
    /// Registration source (e.g. "cli", "`web_agent`").
    pub registration_source: String,
    /// Optional creator handle (4–15 chars, `[a-z0-9-_.]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifyResponse {
    /// Verification result status.
    pub status: VerifyStatus,
    /// The activated API key (only present on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_api_key: Option<String>,
    /// Remaining verification attempts (on `wrong_answer`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_attempts: Option<u32>,
}

/// Verification result status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    device_id: String,
}

impl PlatformClient {
    /// Create a new platform client.
    ///
    /// # Arguments
    /// * `platform_base_url` - Base URL of the platform API (e.g. `https://api.nexus42.invalid`)
    /// * `auth_token` - Bearer token for authentication
    /// * `device_id` - Persistent machine identifier (UUID v4)
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub fn new(platform_base_url: &str, auth_token: &str, device_id: &str) -> SyncResult<Self> {
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
        if device_id.is_empty() {
            return Err(SyncError::SyncNotConfigured(
                "device_id is required".to_string(),
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
            device_id: device_id.to_string(),
        })
    }

    /// Register a new creator on the platform.
    ///
    /// Calls `POST /api/v1/creators/register` with the display name
    /// and registration source.
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
    pub async fn register_creator(
        &self,
        display_name: &str,
        registration_source: &str,
        handle: Option<&str>,
    ) -> SyncResult<RegisterResponse> {
        let url = format!("{}/api/v1/creators/register", self.base_url);
        let body = RegisterRequest {
            display_name: display_name.to_string(),
            registration_source: registration_source.to_string(),
            handle: handle.map(std::string::ToString::to_string),
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
    ///
    /// # Errors
    /// Returns the specific error type if the operation fails.
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
    ) -> RequestBuilder {
        let mut request = self
            .client
            .request(method, url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .header("X-Device-ID", &self.device_id)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");

        if let Some(b) = body {
            request = request.json(b);
        }

        request
    }

    /// Execute an HTTP request.
    #[allow(clippy::future_not_send)]
    async fn execute_request<T: Serialize + ?Sized>(
        &self,
        method: Method,
        url: &str,
        body: Option<&T>,
    ) -> SyncResult<reqwest::Response> {
        let request = self.build_request(method, url, body);
        request.send().await.map_err(SyncError::from)
    }

    /// Get the base URL (for testing).
    #[cfg(test)]
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── StagedPlatformError classification tests ──────────────────

    #[test]
    fn classify_timeout_maps_to_staged_timeout() {
        let err = SyncError::SyncTimeout { seconds: 30 };
        let staged = classify_platform_error(err);
        assert_eq!(staged, StagedPlatformError::Timeout);
    }

    #[test]
    fn classify_not_configured_maps_to_staged_config() {
        let err = SyncError::SyncNotConfigured("platform_base_url is required".to_string());
        let staged = classify_platform_error(err);
        assert_eq!(
            staged,
            StagedPlatformError::Config("platform_base_url is required".to_string())
        );
    }

    #[test]
    fn classify_auth_token_invalid_maps_to_staged_auth() {
        let err = SyncError::AuthTokenInvalid("expired".to_string());
        let staged = classify_platform_error(err);
        assert_eq!(staged, StagedPlatformError::Auth("expired".to_string()));
    }

    #[test]
    fn classify_platform_error_maps_to_staged_platform() {
        let err = SyncError::PlatformError {
            status: 409,
            body: "creator already exists".to_string(),
        };
        let staged = classify_platform_error(err);
        assert_eq!(
            staged,
            StagedPlatformError::Platform {
                status: 409,
                body: "creator already exists".to_string()
            }
        );
    }

    #[test]
    fn sanitize_redacts_bearer_token() {
        let body = r#"{"error": "Bearer sk-proj-abc123def456ghi789jkl012mno345"}"#;
        let sanitized = sanitize_error_body(body);
        assert!(
            !sanitized.contains("sk-proj-abc123"),
            "Bearer token should be redacted: {sanitized}"
        );
        assert!(
            sanitized.contains("[REDACTED]"),
            "Should contain redaction marker: {sanitized}"
        );
    }

    #[test]
    fn sanitize_redacts_sk_prefixed_key() {
        let body = r#"api_key=sk-live-abcdef1234567890abcdef1234567890"#;
        let sanitized = sanitize_error_body(body);
        assert!(
            !sanitized.contains("sk-live-abcdef"),
            "sk- key should be redacted: {sanitized}"
        );
        assert!(
            sanitized.contains("[REDACTED_KEY]"),
            "Should contain redaction marker: {sanitized}"
        );
    }

    #[test]
    fn sanitize_redacts_nexus_live_key() {
        let body = r#"creator_api_key: nexus_live_abc123XYZ789"#;
        let sanitized = sanitize_error_body(body);
        assert!(
            !sanitized.contains("nexus_live_abc"),
            "nexus_live_ key should be redacted: {sanitized}"
        );
        assert!(
            sanitized.contains("[REDACTED_KEY]"),
            "Should contain redaction marker: {sanitized}"
        );
    }

    #[test]
    fn sanitize_redacts_long_hex_string() {
        let body = format!("token={}", "deadbeef".repeat(8)); // 64 hex chars
        let sanitized = sanitize_error_body(&body);
        assert!(
            !sanitized.contains("deadbeef"),
            "long hex should be redacted: {sanitized}"
        );
        assert!(
            sanitized.contains("[REDACTED_HEX]"),
            "Should contain redaction marker: {sanitized}"
        );
    }

    #[test]
    fn sanitize_caps_body_length() {
        let body = "x".repeat(500);
        let sanitized = sanitize_error_body(&body);
        assert!(
            sanitized.len() <= ERROR_BODY_CAP + "…".len(),
            "body should be capped at {}: got {} chars: {sanitized}",
            ERROR_BODY_CAP,
            sanitized.len()
        );
    }

    #[test]
    fn sanitize_preserves_safe_body() {
        let body = "creator already exists";
        let sanitized = sanitize_error_body(body);
        assert_eq!(sanitized, body);
    }

    #[test]
    fn classify_platform_error_sanitizes_raw_body() {
        let err = SyncError::PlatformError {
            status: 500,
            body: format!(
                "error: key=nexus_live_SECRETKEY123, token={}",
                "a".repeat(64)
            ),
        };
        let staged = classify_platform_error(err);
        let display = format!("{staged}");
        assert!(
            !display.contains("nexus_live_SECRETKEY123"),
            "raw key should not appear in display: {display}"
        );
        assert!(
            !display.contains(&"a".repeat(64)),
            "long hex should not appear in display: {display}"
        );
    }

    #[test]
    fn staged_platform_error_display_timeout() {
        let err = StagedPlatformError::Timeout;
        let msg = format!("{err}");
        assert!(msg.contains("timeout"));
        assert!(msg.contains("platform integration failed"));
    }

    #[test]
    fn staged_platform_error_display_platform() {
        let err = StagedPlatformError::Platform {
            status: 500,
            body: "internal error".to_string(),
        };
        let msg = format!("{err}");
        assert!(msg.contains("500"));
        assert!(msg.contains("internal error"));
        assert!(msg.contains("platform integration failed"));
    }

    // ── Struct serialization/deserialization tests ──────────────────

    #[test]
    fn register_request_serialization() {
        let req = RegisterRequest {
            display_name: "Test Creator".to_string(),
            registration_source: "cli".to_string(),
            handle: None,
        };
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains("Test Creator"));
        assert!(json.contains("cli"));
        // When handle is None, the field should be omitted from JSON
        assert!(!json.contains("handle"));
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
        let result = PlatformClient::new("", "some_token", "dev_123");
        assert!(matches!(result, Err(SyncError::SyncNotConfigured { .. })));
    }

    #[test]
    fn client_creation_requires_auth_token() {
        let result = PlatformClient::new("https://api.example.com", "", "dev_123");
        assert!(matches!(result, Err(SyncError::SyncNotConfigured { .. })));
    }

    #[test]
    fn client_creation_requires_device_id() {
        let result = PlatformClient::new("https://api.example.com", "some_token", "");
        assert!(matches!(result, Err(SyncError::SyncNotConfigured { .. })));
    }

    #[test]
    fn client_creation_succeeds() {
        let result = PlatformClient::new("https://api.example.com", "some_token", "dev_123");
        assert!(result.is_ok());
        let client = result.expect("ok");
        assert_eq!(client.base_url(), "https://api.example.com");
    }

    #[test]
    fn client_normalizes_trailing_slash() {
        let result = PlatformClient::new("https://api.example.com/", "some_token", "dev_123");
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

        let client =
            PlatformClient::new(&mock_server.uri(), "test_token", "dev_123").expect("create");
        let resp = client
            .register_creator("Test Creator", "cli", None)
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

        let client =
            PlatformClient::new(&mock_server.uri(), "test_token", "dev_123").expect("create");
        let result = client.register_creator("Test", "cli", None).await;
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

        let client =
            PlatformClient::new(&mock_server.uri(), "test_token", "dev_123").expect("create");
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

        let client =
            PlatformClient::new(&mock_server.uri(), "test_token", "dev_123").expect("create");
        let resp = client
            .verify_creator("nxc_verify_test", "99")
            .await
            .expect("verify");

        assert_eq!(resp.status, VerifyStatus::WrongAnswer);
        assert_eq!(resp.remaining_attempts, Some(2));
    }

    #[tokio::test]
    async fn request_includes_x_device_id_header() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/register"))
            .and(header("X-Device-ID", "dev_test_uuid"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "creator_id": "crt_header_test",
                "display_name": "Header Test",
                "creator_api_key": "nexus_live_key",
                "verification": {
                    "verification_code": "nxc_verify_test",
                    "challenge_text": "Test",
                    "expires_at": "2026-04-16T00:05:00.000Z",
                    "instructions": "Test"
                }
            })))
            .mount(&mock_server)
            .await;

        let client =
            PlatformClient::new(&mock_server.uri(), "test_token", "dev_test_uuid").expect("create");
        let resp = client
            .register_creator("Header Test", "cli", None)
            .await
            .expect("register");

        assert_eq!(resp.creator_id, "crt_header_test");
    }
}
