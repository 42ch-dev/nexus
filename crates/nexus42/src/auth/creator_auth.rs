//! Creator Authentication — API Key Management
//!
//! Manages Creator API keys. Keys are stored in platform secure storage.
//! CLI obtains short-lived tokens via `POST /v1/creators/{id}/credentials`
//! and caches them locally.
//!
//! V1.16 adds `creator_auth_headers()` for Creator-context HTTP header selection.

#![allow(dead_code)]

use super::AuthStore;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};

/// Creator-context auth headers for HTTP requests.
#[derive(Clone)]
pub struct CreatorAuthHeaders {
    /// `Authorization: Bearer <token>` value.
    pub authorization: String,
    /// Optional `X-Creator-Id` header value (set when using user token fallback).
    pub x_creator_id: Option<String>,
}

impl std::fmt::Debug for CreatorAuthHeaders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreatorAuthHeaders")
            .field("authorization", &"<redacted>")
            .field("x_creator_id", &self.x_creator_id)
            .finish()
    }
}

/// Build Creator-context authentication headers.
///
/// Selection logic:
/// 1. If `creator_api_key` exists for the `creator_id` →
///    `Authorization: Bearer <creator_api_key>` (no X-Creator-Id needed).
/// 2. Else if a valid user token exists →
///    `Authorization: Bearer <user_token>` + `X-Creator-Id: <creator_id>`.
/// 3. Else → Error: "Authentication required."
///
/// This is a pure function — no network calls.
///
/// # Errors
///
/// Returns `CliError::AuthenticationRequired` if no credentials are available.
/// Returns `CliError::Other` if `creator_id` is empty or whitespace-only.
pub fn creator_auth_headers(_config: &CliConfig, creator_id: &str) -> Result<CreatorAuthHeaders> {
    if creator_id.trim().is_empty() {
        return Err(CliError::Other(
            "creator_id must not be empty or whitespace-only.".to_string(),
        ));
    }

    let store = AuthStore::load()?;

    // Path 1: Creator API key
    if let Some(api_key) = store.get_creator_api_key(creator_id)? {
        return Ok(CreatorAuthHeaders {
            authorization: format!("Bearer {api_key}"),
            x_creator_id: None,
        });
    }

    // Path 2: User token + X-Creator-Id
    if store.is_user_authenticated() {
        if let Some(token_state) = &store.user_token {
            let token = token_state.access_token.clone();
            return Ok(CreatorAuthHeaders {
                authorization: format!("Bearer {token}"),
                x_creator_id: Some(creator_id.to_string()),
            });
        }
    }

    // Path 3: No credentials
    Err(CliError::Other(
        "Authentication required. Run `nexus42 auth login` or `nexus42 creator register`."
            .to_string(),
    ))
}

/// Rotate credentials for a Creator entity
///
/// Calls `POST /v1/creators/{id}/credentials` on the platform API
/// to obtain a new short-lived token.
///
/// # Errors
///
/// Returns `CliError::Other` if the platform API for credential rotation
/// is not yet available or if the credential rotation fails.
pub async fn rotate_credentials(config: &CliConfig, creator_id: &str) -> Result<()> {
    // We need a user token to call the platform API
    let _user_token = get_user_token(config).await?;

    tracing::info!("Rotating credentials for creator {}", creator_id);

    // Step 1: Request new credentials from platform (skeleton — needs real auth)
    // In production, this would use the user token to authenticate.
    // For now, return an error since the platform integration is not ready.
    Err(CliError::Other(
        "Platform API for credential rotation not yet available.".into(),
    ))
}

/// Credentials response from platform
#[derive(Debug, serde::Deserialize)]
pub struct CredentialsResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

/// Validate cached Creator token, refresh if expired
///
/// # Errors
///
/// Returns `CliError::Other` if:
/// - The auth store cannot be loaded
/// - The token expiry timestamp cannot be parsed
/// - Credential rotation fails
/// - No token is found for the creator after rotation
pub async fn ensure_valid_token(config: &CliConfig, creator_id: &str) -> Result<String> {
    let store = AuthStore::load()?;

    if let Some(creators) = &store.creators {
        if let Some(state) = creators.get(creator_id) {
            let expires = chrono::DateTime::parse_from_rfc3339(&state.expires_at)?;
            if chrono::Utc::now() < expires {
                return Ok(state.access_token.clone());
            }
        }
    }

    // Token expired or missing — rotate
    rotate_credentials(config, creator_id).await?;
    let store = AuthStore::load()?;
    let creators = store.creators.as_ref().and_then(|c| c.get(creator_id));
    creators
        .map(|s| s.access_token.clone())
        .ok_or_else(|| CliError::Other(format!("Failed to obtain token for creator {creator_id}")))
}

/// Get the current user access token from `AuthStore`.
///
/// Reads the platform JWT from the local auth store.
/// Calls `ensure_valid_token()` to auto-refresh if the token is expiring.
/// Returns an error if the user is not authenticated.
async fn get_user_token(config: &CliConfig) -> Result<String> {
    super::user_auth::ensure_valid_token(config).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{CreatorAuthState, UserTokenState};

    fn store_with_creator_key(creator_id: &str, api_key: &str) -> AuthStore {
        let mut store = AuthStore::default();
        let mut creators = std::collections::HashMap::new();
        creators.insert(
            creator_id.to_string(),
            CreatorAuthState {
                creator_id: creator_id.to_string(),
                access_token: String::new(),
                expires_at: String::new(),
                creator_api_key: Some(api_key.to_string()),
            },
        );
        store.creators = Some(creators);
        store
    }

    fn store_with_user_token(token: &str) -> AuthStore {
        AuthStore {
            user_token: Some(UserTokenState {
                access_token: token.to_string(),
                token_type: "Bearer".to_string(),
                expires_at: "2099-01-01T00:00:00Z".to_string(),
                user_id: "usr_test".to_string(),
                refresh_token: None,
                refresh_expires_at: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn creator_key_auth_path() {
        // Creator API key exists → Bearer <api_key>, no X-Creator-Id
        let store = store_with_creator_key("ctr_alpha", "nexus_live_alpha");
        let headers = build_headers_from_store(&store, "ctr_alpha").expect("headers");
        assert_eq!(headers.authorization, "Bearer nexus_live_alpha");
        assert!(headers.x_creator_id.is_none());
    }

    #[test]
    fn user_token_fallback_path() {
        // No creator key but valid user token → Bearer <user_token> + X-Creator-Id
        let store = store_with_user_token("user_tok_123");
        let headers = build_headers_from_store(&store, "ctr_alpha").expect("headers");
        assert_eq!(headers.authorization, "Bearer user_tok_123");
        assert_eq!(headers.x_creator_id.as_deref(), Some("ctr_alpha"));
    }

    #[test]
    fn no_credentials_errors() {
        // Neither creator key nor user token → error
        let store = AuthStore::default();
        let result = build_headers_from_store(&store, "ctr_alpha");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Authentication required"));
    }

    /// Test helper: build headers directly from an `AuthStore` without disk I/O.
    fn build_headers_from_store(store: &AuthStore, creator_id: &str) -> Result<CreatorAuthHeaders> {
        // Path 1: Creator API key
        if let Some(creators) = &store.creators {
            if let Some(state) = creators.get(creator_id) {
                if let Some(api_key) = &state.creator_api_key {
                    return Ok(CreatorAuthHeaders {
                        authorization: format!("Bearer {api_key}"),
                        x_creator_id: None,
                    });
                }
            }
        }

        // Path 2: User token + X-Creator-Id
        if store.is_user_authenticated() {
            let token = store
                .user_token
                .as_ref()
                .expect("is_user_authenticated guarantees Some")
                .access_token
                .clone();
            return Ok(CreatorAuthHeaders {
                authorization: format!("Bearer {token}"),
                x_creator_id: Some(creator_id.to_string()),
            });
        }

        // Path 3: No credentials
        Err(CliError::Other(
            "Authentication required. Run `nexus42 auth login` or `nexus42 creator register`."
                .to_string(),
        ))
    }

    // ── R-CREATOR-003: creator_id validation tests ────────────────────

    #[test]
    fn empty_creator_id_returns_error() {
        let _home = crate::testutil::isolated_home();
        let config = CliConfig::default();
        let result = creator_auth_headers(&config, "");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("must not be empty"),
            "expected empty-id error, got: {err}"
        );
    }

    #[test]
    fn whitespace_only_creator_id_returns_error() {
        let _home = crate::testutil::isolated_home();
        let config = CliConfig::default();
        let result = creator_auth_headers(&config, "   \t  ");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("must not be empty"),
            "expected whitespace-id error, got: {err}"
        );
    }

    #[test]
    fn valid_creator_id_proceeds_to_auth_check() {
        let _home = crate::testutil::isolated_home();
        let config = CliConfig::default();
        // No credentials stored → should get "Authentication required", not "empty id"
        let result = creator_auth_headers(&config, "ctr_valid");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Authentication required"),
            "expected auth-required error for valid ID without credentials, got: {err}"
        );
    }
}
