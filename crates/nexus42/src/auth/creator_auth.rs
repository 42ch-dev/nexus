//! Creator Authentication — API Key Management
//!
//! Manages Creator API keys. Keys are stored in platform secure storage.
//! CLI obtains short-lived tokens via `POST /v1/creators/{id}/credentials`
//! and caches them locally.

#![allow(dead_code)]

use super::{AuthStore, CreatorAuthState};
use crate::config::CliConfig;
use crate::errors::{CliError, Result};

/// Rotate credentials for a Creator entity
///
/// Calls `POST /v1/creators/{id}/credentials` on the platform API
/// to obtain a new short-lived token.
pub async fn rotate_credentials(config: &CliConfig, creator_id: &str) -> Result<()> {
    let store = AuthStore::load()?;

    // We need a user token to call the platform API
    let user_token = store.user_token()?;

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/creators/{}/credentials",
        config.platform_url, creator_id
    );

    tracing::info!("Rotating credentials for creator {}", creator_id);

    // Step 1: Request new credentials from platform
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", user_token))
        .json(&serde_json::json!({"action": "rotate"}))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(CliError::Api {
            status,
            message: body,
        });
    }

    // Step 2: Parse and cache the new token
    let token_resp: CredentialsResponse = resp.json().await?;
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::seconds(token_resp.expires_in as i64);

    let mut store = AuthStore::load()?;
    let creators = store
        .creators
        .get_or_insert_with(std::collections::HashMap::new);
    creators.insert(
        creator_id.to_string(),
        CreatorAuthState {
            creator_id: creator_id.to_string(),
            access_token: token_resp.access_token,
            expires_at: expires_at.to_rfc3339(),
        },
    );
    store.save()?;

    println!("✓ Credentials rotated for creator {}", creator_id);
    println!("  Expires: {}", expires_at.to_rfc3339());

    Ok(())
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
