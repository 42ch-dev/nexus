//! Creator Authentication — API Key Management
//!
//! Manages Creator API keys. Keys are stored in platform secure storage.
//! CLI obtains short-lived tokens via `POST /v1/creators/{id}/credentials`
//! and caches them locally.

#![allow(dead_code)]

use super::{user_auth, AuthStore};
use crate::api::daemon_client::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};

/// Rotate credentials for a Creator entity
///
/// Calls `POST /v1/creators/{id}/credentials` on the platform API
/// to obtain a new short-lived token.
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
    creators.map(|s| s.access_token.clone()).ok_or_else(|| {
        CliError::Other(format!("Failed to obtain token for creator {}", creator_id))
    })
}

/// Get the current user access token from the daemon.
///
/// Returns an error if the user is not authenticated.
async fn get_user_token(config: &CliConfig) -> Result<String> {
    let client = DaemonClient::from_config(config);
    if !client.health_check().await? {
        return Err(CliError::DaemonNotRunning);
    }
    let _status: user_auth::AuthStatusResponse = client.get("/v1/local/auth/status").await?;
    // The daemon doesn't expose the access_token directly via status.
    // For now, return a placeholder. The actual token flow will be
    // handled differently when platform integration lands.
    Err(CliError::Other(
        "User token retrieval via daemon not yet implemented. Use `nexus42 auth login`.".into(),
    ))
}
