//! Dual-Subject Authentication Module
//!
//! Supports both User authentication (device flow OAuth) and Creator API key management.

pub mod user_auth;
pub mod creator_auth;

use crate::config::auth_store_path;
use crate::errors::{CliError, Result};
use serde::{Deserialize, Serialize};

/// Auth store — persisted to `$HOME/.nexus42/auth.json`
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthStore {
    /// User authentication state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserAuthState>,

    /// Creator authentication states (keyed by creator_id)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creators: Option<std::collections::HashMap<String, CreatorAuthState>>,
}

/// User authentication state (from device flow OAuth)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAuthState {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: String,
    pub expires_at: String, // ISO 8601
}

/// Creator authentication state (short-lived token cached from platform)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatorAuthState {
    pub creator_id: String,
    pub access_token: String,
    pub expires_at: String, // ISO 8601
}

impl AuthStore {
    /// Load auth store from disk
    pub fn load() -> Result<Self> {
        let path = auth_store_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save auth store to disk
    pub fn save(&self) -> Result<()> {
        let path = auth_store_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Check if user is authenticated
    pub fn is_user_authenticated(&self) -> bool {
        self.user.as_ref().map_or(false, |u| !u.access_token.is_empty())
    }

    /// Check if a specific creator is authenticated
    pub fn is_creator_authenticated(&self, creator_id: &str) -> bool {
        self.creators
            .as_ref()
            .and_then(|m| m.get(creator_id))
            .map_or(false, |c| !c.access_token.is_empty())
    }

    /// Get user access token
    pub fn user_token(&self) -> Result<&str> {
        self.user
            .as_ref()
            .map(|u| u.access_token.as_str())
            .ok_or(CliError::AuthenticationRequired)
    }
}
