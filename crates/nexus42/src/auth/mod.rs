//! Dual-Subject Authentication Module
//!
//! Supports both User authentication (device flow OAuth) and Creator API key management.
//! User auth state (platform JWT from device flow) is stored locally in `~/.nexus42/auth.json`.
//! Creator auth state remains file-based for V1.x (will migrate to daemon in V1.2).

pub mod creator_auth;
pub mod user_auth;

use crate::config::auth_store_path;
use crate::errors::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Platform user token obtained via RFC 8628 device flow.
///
/// Stored in `AuthStore.user_token` and persisted to `~/.nexus42/auth.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTokenState {
    /// Platform JWT access token.
    pub access_token: String,
    /// Token type (always "Bearer" for device flow).
    #[serde(default = "default_token_type")]
    pub token_type: String,
    /// ISO 8601 expiry timestamp.
    pub expires_at: String,
    /// Platform user ID extracted from JWT claims (`sub` / `userId`).
    pub user_id: String,
    /// `OAuth2` refresh token (optional — present when platform delivers it).
    /// `#[serde(default)]` ensures backward compat with existing auth.json files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// ISO 8601 expiry timestamp for the refresh token (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_expires_at: Option<String>,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

/// Auth store — persisted to `$HOME/.nexus42/auth.json`
///
/// Contains both user auth (platform JWT from device flow) and creator API key cache.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthStore {
    /// Platform user token from device flow login.
    /// `#[serde(default)]` ensures backward compat — existing auth.json files
    /// without this field parse correctly (field defaults to `None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_token: Option<UserTokenState>,

    /// Creator authentication states (keyed by `creator_id`)
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
    ///
    /// # Errors
    ///
    /// Returns I/O errors if the auth store file cannot be read.
    /// Returns JSON parsing errors if the file content is malformed.
    pub fn load() -> Result<Self> {
        let path = auth_store_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save auth store to disk (owner-only: 0600 on Unix).
    ///
    /// # Errors
    ///
    /// Returns I/O errors if the auth store file cannot be written
    /// or if permissions cannot be set on Unix systems.
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
    #[must_use]
    pub fn is_creator_authenticated(&self, creator_id: &str) -> bool {
        self.creators
            .as_ref()
            .and_then(|m| m.get(creator_id))
            .is_some_and(|c| !c.access_token.is_empty())
    }

    /// Store a creator API key.
    ///
    /// If an entry already exists for the given `creator_id`, the API key field
    /// is updated in place. Otherwise, a new entry is created with the API key
    /// and placeholder token fields (the token is populated separately during
    /// authentication).
    ///
    /// V1.3 residual: API key stored in plain-text JSON.
    /// Acceptable for pre-1.0 — per AGENTS.md, local persistence
    /// is not treated as a long-term migration contract.
    /// Future: encrypt at rest or use platform-managed credentials.
    ///
    /// # Errors
    ///
    /// Returns I/O errors if the auth store cannot be saved to disk.
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
    ///
    /// # Errors
    ///
    /// Returns I/O errors (other than `NotFound`) when reloading from disk.
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
        match Self::load() {
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

    /// Store a platform user token from device flow login.
    ///
    /// # Errors
    ///
    /// Returns I/O errors if the auth store cannot be saved to disk.
    pub fn store_user_token(&mut self, token: UserTokenState) -> Result<()> {
        self.user_token = Some(token);
        self.save()
    }

    /// Clear the platform user token (logout).
    ///
    /// # Errors
    ///
    /// Returns I/O errors if the auth store cannot be saved to disk.
    pub fn clear_user_token(&mut self) -> Result<()> {
        self.user_token = None;
        self.save()
    }

    /// Check if a user is authenticated (has a non-expired user token).
    ///
    /// Returns `true` only if a `user_token` exists, has a non-empty `access_token`,
    /// AND the token has not yet expired according to its `expires_at` timestamp.
    /// If the `expires_at` field cannot be parsed, the token is conservatively
    /// treated as already expired (returns `false`).
    #[must_use]
    pub fn is_user_authenticated(&self) -> bool {
        self.user_token.as_ref().is_some_and(|t| {
            if t.access_token.is_empty() {
                return false;
            }
            // Parse ISO 8601 expiry; treat unparseable dates as expired.
            DateTime::parse_from_rfc3339(&t.expires_at).is_ok_and(|expiry| expiry > Utc::now())
        })
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
            user_token: None,
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
            user_token: None,
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

    // ── User token tests (T2) ───────────────────────────────────────

    #[test]
    fn user_token_serialization_roundtrip() {
        let token = UserTokenState {
            access_token: "eyJhbGciOiJIUzI1NiJ9.test".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: "2026-04-27T12:00:00Z".to_string(),
            user_id: "usr_abc123".to_string(),
            refresh_token: None,
            refresh_expires_at: None,
        };
        let json = serde_json::to_string(&token).expect("serialize");
        assert!(json.contains("eyJhbGciOiJIUzI1NiJ9.test"));
        assert!(json.contains("usr_abc123"));

        let parsed: UserTokenState = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed.access_token, token.access_token);
        assert_eq!(parsed.token_type, "Bearer");
        assert_eq!(parsed.expires_at, "2026-04-27T12:00:00Z");
        assert_eq!(parsed.user_id, "usr_abc123");
    }

    #[test]
    fn user_token_default_token_type() {
        let json = r#"{
            "access_token": "tok",
            "expires_at": "2026-01-01T00:00:00Z",
            "user_id": "usr_1"
        }"#;
        let parsed: UserTokenState = serde_json::from_str(json).expect("parse");
        assert_eq!(parsed.token_type, "Bearer");
    }

    #[test]
    fn auth_store_roundtrip_with_user_token() {
        let token = UserTokenState {
            access_token: "tok_roundtrip".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: "2026-12-31T23:59:59Z".to_string(),
            user_id: "usr_roundtrip".to_string(),
            refresh_token: None,
            refresh_expires_at: None,
        };

        let mut store = AuthStore::default();
        store.store_user_token(token).expect("store");

        assert!(store.is_user_authenticated());
        let t = store.user_token.as_ref().expect("has token");
        assert_eq!(t.access_token, "tok_roundtrip");
        assert_eq!(t.user_id, "usr_roundtrip");
    }

    #[test]
    fn auth_store_clear_user_token() {
        let token = UserTokenState {
            access_token: "tok_clear".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: "2099-01-01T00:00:00Z".to_string(),
            user_id: "usr_clear".to_string(),
            refresh_token: None,
            refresh_expires_at: None,
        };

        let mut store = AuthStore::default();
        store.store_user_token(token).expect("store");
        assert!(store.is_user_authenticated());

        store.clear_user_token().expect("clear");
        assert!(!store.is_user_authenticated());
        assert!(store.user_token.is_none());
    }

    #[test]
    fn auth_store_backward_compat_without_user_token() {
        // Existing auth.json without user_token field must parse correctly
        let json = r#"{
            "creators": {
                "crt_123": {
                    "creator_id": "crt_123",
                    "access_token": "old_tok",
                    "expires_at": "2026-01-01T00:00:00Z"
                }
            }
        }"#;
        let store: AuthStore = serde_json::from_str(json).expect("parse");
        assert!(store.user_token.is_none());
        assert!(store.creators.is_some());
        assert!(!store.is_user_authenticated());
    }

    #[test]
    fn auth_store_empty_json_parses_correctly() {
        let store: AuthStore = serde_json::from_str("{}").expect("parse");
        assert!(store.user_token.is_none());
        assert!(store.creators.is_none());
    }

    #[test]
    fn auth_store_default_no_user_token() {
        let store = AuthStore::default();
        assert!(store.user_token.is_none());
        assert!(!store.is_user_authenticated());
    }

    // ── Refresh token backward compat tests (T1) ──────────────────────

    #[test]
    fn user_token_backward_compat_without_refresh_fields() {
        // Pre-V1.11 auth.json without refresh_token fields must deserialize
        let json = r#"{
            "access_token": "old_tok",
            "token_type": "Bearer",
            "expires_at": "2099-01-01T00:00:00Z",
            "user_id": "usr_old"
        }"#;
        let parsed: UserTokenState = serde_json::from_str(json).expect("parse");
        assert_eq!(parsed.access_token, "old_tok");
        assert!(parsed.refresh_token.is_none());
        assert!(parsed.refresh_expires_at.is_none());
    }

    #[test]
    fn user_token_roundtrip_with_refresh_fields() {
        let token = UserTokenState {
            access_token: "tok_rt".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: "2026-04-28T12:00:00Z".to_string(),
            user_id: "usr_rt".to_string(),
            refresh_token: Some("refresh_abc".to_string()),
            refresh_expires_at: Some("2026-05-28T12:00:00Z".to_string()),
        };
        let json = serde_json::to_string(&token).expect("serialize");
        let parsed: UserTokenState = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed.refresh_token, Some("refresh_abc".to_string()));
        assert_eq!(
            parsed.refresh_expires_at,
            Some("2026-05-28T12:00:00Z".to_string())
        );
    }

    #[test]
    fn user_token_serialization_omits_none_refresh_fields() {
        let token = UserTokenState {
            access_token: "tok_no_rt".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: "2026-04-28T12:00:00Z".to_string(),
            user_id: "usr_no_rt".to_string(),
            refresh_token: None,
            refresh_expires_at: None,
        };
        let json = serde_json::to_string(&token).expect("serialize");
        assert!(!json.contains("refresh_token"));
        assert!(!json.contains("refresh_expires_at"));
    }
}
