//! User Authentication — Device Flow OAuth
//!
//! Implements OAuth 2.0 Device Authorization Grant for human user login.
//! The actual OAuth flow requires an external IdP; this module provides
//! the CLI interface and local token storage.

#![allow(dead_code)]

use super::{AuthStore, UserAuthState};
use crate::config::CliConfig;
use crate::errors::Result;

/// Device flow response from platform
#[derive(Debug, serde::Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Token response from platform
#[derive(Debug, serde::Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
    /// Platform user ID (prefix: "usr_")
    pub user_id: String,
}

/// Initiate device flow login
pub async fn login(config: &CliConfig) -> Result<()> {
    // NOTE: In production, this calls the platform API with a reqwest::Client.
    // For V1.0 skeleton, we print instructions.
    println!("To authenticate, visit:");
    println!("  {}", config.platform_url);
    println!();
    println!("Enter the code shown there to link this device.");
    println!();
    println!("Waiting for authentication...");

    // Step 2: Poll for token (skeleton — actual implementation awaits platform)
    // In production: loop { poll token endpoint; break on success/error/timeout }
    println!("⚠ Device flow requires platform API. This is a V1.0 skeleton.");
    println!("  Run: nexus42 auth token <access_token> (for development/testing)");

    Ok(())
}

/// Login with a raw access token (development/testing mode)
pub fn login_with_token(
    access_token: String,
    refresh_token: String,
    user_id: String,
) -> Result<()> {
    let mut store = AuthStore::load()?;
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::hours(24);

    store.user = Some(UserAuthState {
        access_token,
        refresh_token,
        user_id: user_id.clone(),
        expires_at: expires_at.to_rfc3339(),
    });
    store.save()?;

    println!("✓ Authenticated successfully.");
    println!("  User: {}", user_id);
    println!("  Expires: {}", expires_at.to_rfc3339());

    Ok(())
}

/// Logout — clear user credentials
pub fn logout() -> Result<()> {
    let mut store = AuthStore::load()?;
    store.user = None;
    store.save()?;

    println!("✓ Logged out successfully.");
    Ok(())
}

/// Show current authentication status
pub fn status() -> Result<()> {
    let store = AuthStore::load()?;

    if let Some(user) = &store.user {
        println!("User Authentication: ✓ Active");
        println!("  User ID: {}", user.user_id);
        println!("  Expires: {}", user.expires_at);
    } else {
        println!("User Authentication: ✗ Not logged in");
    }

    if let Some(creators) = &store.creators {
        if !creators.is_empty() {
            println!();
            println!("Creator Tokens ({}):", creators.len());
            for (id, state) in creators {
                println!("  {} — expires {}", id, state.expires_at);
            }
        }
    }

    Ok(())
}
