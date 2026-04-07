//! Dual-Subject Authentication Module
//!
//! Supports both User authentication (device flow OAuth) and Creator API key management.
//! User auth state is owned by the daemon (SQLite), accessed via HTTP API.
//! Creator auth state remains file-based for V1.x (will migrate to daemon in V1.2).

pub mod creator_auth;
pub mod user_auth;

use crate::config::auth_store_path;
use crate::errors::Result;
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;

/// Auth store — persisted to `$HOME/.nexus42/auth.json`
///
/// NOTE: User auth (OAuth tokens) is now managed by the daemon.
/// This store is retained for creator API key caching.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthStore {
    /// Creator authentication states (keyed by creator_id)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creators: Option<std::collections::HashMap<String, CreatorAuthState>>,
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

    /// Save auth store to disk (owner-only: 0600)
    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let path = auth_store_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, &content)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        Ok(())
    }

    /// Check if a specific creator is authenticated
    pub fn is_creator_authenticated(&self, creator_id: &str) -> bool {
        self.creators
            .as_ref()
            .and_then(|m| m.get(creator_id))
            .is_some_and(|c| !c.access_token.is_empty())
    }
}
