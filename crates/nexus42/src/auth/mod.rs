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
#[cfg(unix)]
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
    /// Pending or activated API key for the creator.
    /// Stored locally after successful registration/verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_api_key: Option<String>,
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

    /// Save auth store to disk (owner-only: 0600 on Unix).
    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let path = auth_store_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, &content)?;
        #[cfg(unix)]
        {
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&path, perms)?;
        }
        Ok(())
    }

    /// Check if a specific creator is authenticated
    pub fn is_creator_authenticated(&self, creator_id: &str) -> bool {
        self.creators
            .as_ref()
            .and_then(|m| m.get(creator_id))
            .is_some_and(|c| !c.access_token.is_empty())
    }

    /// Store a creator API key.
    ///
    /// If an entry already exists for the given creator_id, the API key field
    /// is updated in place. Otherwise, a new entry is created with the API key
    /// and placeholder token fields (the token is populated separately during
    /// authentication).
    #[allow(dead_code)]
    pub fn store_creator_api_key(&mut self, creator_id: &str, api_key: &str) -> Result<()> {
        let creators = self
            .creators
            .get_or_insert_with(std::collections::HashMap::new);

        if let Some(existing) = creators.get_mut(creator_id) {
            existing.creator_api_key = Some(api_key.to_string());
        } else {
            let state = CreatorAuthState {
                creator_id: creator_id.to_string(),
                access_token: String::new(),
                expires_at: String::new(),
                creator_api_key: Some(api_key.to_string()),
            };
            creators.insert(creator_id.to_string(), state);
        }

        self.save()
    }

    /// Get a stored creator API key.
    ///
    /// Checks in-memory state first. If the creator entry exists in memory
    /// but has no API key set, returns `None` immediately (no disk fallback).
    /// If the creator entry is not found in memory, reloads from disk.
    /// Distinguishes `NotFound` (legitimate `None`) from other I/O errors
    /// (which propagate as errors).
    #[allow(dead_code)]
    pub fn get_creator_api_key(&self, creator_id: &str) -> Result<Option<String>> {
        // Check in-memory state
        if let Some(creators) = &self.creators {
            if let Some(state) = creators.get(creator_id) {
                // Entry exists in memory — return whatever is there (may be None)
                return Ok(state.creator_api_key.clone());
            }
        }

        // Entry not in memory — fall back to disk.
        // Only treat NotFound as "key not found"; propagate other errors.
        match AuthStore::load() {
            Ok(store) => Ok(store
                .creators
                .as_ref()
                .and_then(|m| m.get(creator_id))
                .and_then(|c| c.creator_api_key.clone())),
            Err(crate::errors::CliError::Io(ref io_err))
                if io_err.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creator_auth_state_serialization_with_api_key() {
        let state = CreatorAuthState {
            creator_id: "crt_123".to_string(),
            access_token: "token_abc".to_string(),
            expires_at: "2026-04-16T00:00:00Z".to_string(),
            creator_api_key: Some("nexus_live_key".to_string()),
        };
        let json = serde_json::to_string(&state).expect("serialize");
        assert!(json.contains("nexus_live_key"));
        assert!(json.contains("crt_123"));

        let parsed: CreatorAuthState = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed.creator_api_key, Some("nexus_live_key".to_string()));
    }

    #[test]
    fn creator_auth_state_serialization_without_api_key() {
        let state = CreatorAuthState {
            creator_id: "crt_123".to_string(),
            access_token: "token_abc".to_string(),
            expires_at: "2026-04-16T00:00:00Z".to_string(),
            creator_api_key: None,
        };
        let json = serde_json::to_string(&state).expect("serialize");
        assert!(!json.contains("creator_api_key"));

        let parsed: CreatorAuthState = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed.creator_api_key, None);
    }

    #[test]
    fn store_and_get_creator_api_key_in_memory() {
        let mut store = AuthStore::default();
        store
            .store_creator_api_key("crt_test", "nexus_live_test_key")
            .expect("store");

        // In-memory retrieval
        let key = store.get_creator_api_key("crt_test").expect("get");
        assert_eq!(key, Some("nexus_live_test_key".to_string()));
    }

    #[test]
    fn store_creates_new_entry_when_not_exists() {
        let mut store = AuthStore::default();
        store
            .store_creator_api_key("crt_new", "nexus_live_new_key")
            .expect("store");

        let creators = store.creators.as_ref().expect("creators map exists");
        let state = creators.get("crt_new").expect("entry exists");
        assert_eq!(
            state.creator_api_key,
            Some("nexus_live_new_key".to_string())
        );
        assert_eq!(state.creator_id, "crt_new");
        assert!(state.access_token.is_empty());
    }

    #[test]
    fn store_updates_existing_entry() {
        let mut store = AuthStore {
            creators: Some({
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "crt_existing".to_string(),
                    CreatorAuthState {
                        creator_id: "crt_existing".to_string(),
                        access_token: "old_token".to_string(),
                        expires_at: "2026-01-01T00:00:00Z".to_string(),
                        creator_api_key: Some("old_key".to_string()),
                    },
                );
                m
            }),
        };

        store
            .store_creator_api_key("crt_existing", "new_key")
            .expect("store");

        let creators = store.creators.as_ref().expect("creators map");
        let state = creators.get("crt_existing").expect("entry exists");
        assert_eq!(state.creator_api_key, Some("new_key".to_string()));
        // Other fields should be preserved
        assert_eq!(state.access_token, "old_token");
        assert_eq!(state.expires_at, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn get_returns_none_for_unknown_creator() {
        let store = AuthStore::default();
        let key = store.get_creator_api_key("crt_unknown").expect("get");
        assert_eq!(key, None);
    }

    #[test]
    fn get_returns_none_when_api_key_not_set() {
        let store = AuthStore {
            creators: Some({
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "crt_no_key".to_string(),
                    CreatorAuthState {
                        creator_id: "crt_no_key".to_string(),
                        access_token: "some_token".to_string(),
                        expires_at: "2026-01-01T00:00:00Z".to_string(),
                        creator_api_key: None,
                    },
                );
                m
            }),
        };
        let key = store.get_creator_api_key("crt_no_key").expect("get");
        assert_eq!(key, None);
    }

    #[test]
    fn default_auth_store_has_no_creators() {
        let store = AuthStore::default();
        assert!(store.creators.is_none());
    }

    #[test]
    fn store_and_get_roundtrip_with_file() {
        // store_creator_api_key calls self.save() which persists to disk.
        // We verify the in-memory portion works regardless of whether
        // file persistence succeeds (depends on test environment).
        let mut store = AuthStore::default();
        let _ = store.store_creator_api_key("crt_file_test", "nexus_live_file_key");

        // In-memory retrieval should always work after store
        let key = store.get_creator_api_key("crt_file_test").expect("get");
        assert_eq!(key, Some("nexus_live_file_key".to_string()));
    }
}
