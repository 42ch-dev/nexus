//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Token Lifecycle Manager
//!
//! Manages OAuth token storage, retrieval, and refresh lifecycle.
//! Tokens are stored in the daemon's `SQLite` database (`auth_tokens` table)
//! to provide centralized auth state for both CLI and daemon.

use crate::api::errors::NexusApiError;
use crate::db::pool::DbPool;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Minimum remaining time before expiry that triggers proactive refresh.
pub const REFRESH_THRESHOLD: Duration = Duration::from_secs(300); // 5 minutes

/// Stored auth token record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub user_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl StoredToken {
    /// Check if the token is expired or within the refresh threshold.
    #[must_use]
    pub fn needs_refresh(&self) -> bool {
        let now = Utc::now();
        let threshold = self.expires_at
            - chrono::Duration::from_std(REFRESH_THRESHOLD).unwrap_or(chrono::Duration::zero());
        now >= threshold
    }

    /// Check if the token is fully expired (past `expires_at`).
    #[must_use]
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
}

/// Token manager — handles storage, retrieval, and lifecycle of auth tokens.
pub struct TokenManager {
    db: DbPool,
}

fn db_error(e: impl std::fmt::Display) -> NexusApiError {
    NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: e.to_string(),
    }
}

impl TokenManager {
    /// Create a new `TokenManager` backed by the given connection pool.
    #[must_use]
    pub const fn new(db: DbPool) -> Self {
        Self { db }
    }

    /// Store a new set of tokens, replacing any existing tokens for the user.
    pub async fn store_tokens(
        &self,
        user_id: &str,
        access_token: &str,
        refresh_token: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), NexusApiError> {
        let expires_at_str = expires_at.to_rfc3339();
        let created_at = Utc::now().to_rfc3339();

        let user_id = user_id.to_string();
        let access_token = access_token.to_string();
        let refresh_token = refresh_token.to_string();

        sqlx::query!(
            "INSERT OR REPLACE INTO auth_tokens (user_id, access_token, refresh_token, expires_at, created_at)
             VALUES (?, ?, ?, ?, ?)",
            user_id, access_token, refresh_token, expires_at_str, created_at
        )
        .execute(self.db.pool())
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: format!("Token storage failed: {e}"),
        })?;

        Ok(())
    }

    /// Get the current stored token for the user.
    ///
    /// Returns `Ok(None)` if no token is stored.
    pub async fn get_token(&self) -> Result<Option<StoredToken>, NexusApiError> {
        let row = sqlx::query!(
            r#"SELECT user_id as "user_id!", access_token, refresh_token, expires_at, created_at FROM auth_tokens ORDER BY created_at DESC LIMIT 1"#
        )
        .fetch_optional(self.db.pool())
        .await
        .map_err(db_error)?;

        let Some(row) = row else { return Ok(None) };

        let expires_at = DateTime::parse_from_rfc3339(&row.expires_at)
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: format!("Invalid expires_at: {e}"),
            })?
            .with_timezone(&Utc);

        let created_at = DateTime::parse_from_rfc3339(&row.created_at)
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: format!("Invalid created_at: {e}"),
            })?
            .with_timezone(&Utc);

        Ok(Some(StoredToken {
            user_id: row.user_id,
            access_token: row.access_token,
            refresh_token: row.refresh_token,
            expires_at,
            created_at,
        }))
    }

    /// Get a valid (non-expired) access token.
    ///
    /// If no token is stored or the token is expired, returns `Ok(None)`.
    pub async fn get_valid_token(&self) -> Result<Option<StoredToken>, NexusApiError> {
        let token = self.get_token().await?;
        match token {
            Some(t) if !t.is_expired() => Ok(Some(t)),
            _ => Ok(None),
        }
    }

    /// Clear all stored tokens (logout).
    pub async fn clear_tokens(&self) -> Result<(), NexusApiError> {
        sqlx::query!("DELETE FROM auth_tokens")
            .execute(self.db.pool())
            .await
            .map_err(db_error)?;
        Ok(())
    }

    /// Validate an access token against stored tokens.
    ///
    /// Returns `Ok(true)` if the token matches a stored, non-expired token.
    pub async fn validate_token(&self, token: &str) -> Result<bool, NexusApiError> {
        let token = token.to_string();
        let stored = self.get_valid_token().await?;
        Ok(stored.is_some_and(|t| t.access_token == token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::pool::DbPool;
    use crate::test_utils::create_test_workspace;

    /// Create a test database with schema initialized
    async fn create_test_db() -> (crate::test_utils::TestTempRoot, std::path::PathBuf, DbPool) {
        let (tmp, _nexus_home, db_path) = create_test_workspace().await;
        // Keep `tmp` alive (it owns the temp dir containing the DB file).
        // create_test_workspace already ran migrations & seeded the schema,
        // so we just open a new pool on the same file.
        let pool = DbPool::with_defaults(&db_path)
            .await
            .expect("DbPool::with_defaults should succeed");
        (tmp, db_path, pool)
    }

    #[tokio::test]
    async fn store_and_retrieve_token() {
        let (_tmp, _db_path, pool) = create_test_db().await;
        let mgr = TokenManager::new(pool);

        let expires_at = Utc::now() + chrono::Duration::hours(1);

        mgr.store_tokens("usr_test123", "at_abc", "rt_def", expires_at)
            .await
            .expect("store_tokens should succeed");

        let token = mgr
            .get_token()
            .await
            .expect("get_token should succeed")
            .expect("token should exist");
        assert_eq!(token.user_id, "usr_test123");
        assert_eq!(token.access_token, "at_abc");
        assert_eq!(token.refresh_token, "rt_def");
    }

    #[tokio::test]
    async fn get_token_returns_none_when_empty() {
        let (_tmp, _db_path, pool) = create_test_db().await;
        let mgr = TokenManager::new(pool);

        let token = mgr.get_token().await.expect("get_token should succeed");
        assert!(token.is_none());
    }

    #[tokio::test]
    async fn get_valid_token_returns_none_for_expired_token() {
        let (_tmp, _db_path, pool) = create_test_db().await;
        let mgr = TokenManager::new(pool);

        // Store a token that expired in the past
        let expires_at = Utc::now() - chrono::Duration::hours(1);
        mgr.store_tokens("usr_test", "at_old", "rt_old", expires_at)
            .await
            .expect("store_tokens should succeed for expired token");

        let valid = mgr
            .get_valid_token()
            .await
            .expect("get_valid_token should succeed");
        assert!(valid.is_none());
    }

    #[tokio::test]
    async fn get_valid_token_returns_token_when_not_expired() {
        let (_tmp, _db_path, pool) = create_test_db().await;
        let mgr = TokenManager::new(pool);

        let expires_at = Utc::now() + chrono::Duration::hours(1);
        mgr.store_tokens("usr_test", "at_valid", "rt_valid", expires_at)
            .await
            .expect("store_tokens should succeed");

        let valid = mgr
            .get_valid_token()
            .await
            .expect("get_valid_token should succeed");
        assert!(valid.is_some());
        assert_eq!(
            valid.expect("valid token should exist").access_token,
            "at_valid"
        );
    }

    #[tokio::test]
    async fn clear_tokens_removes_all_tokens() {
        let (_tmp, _db_path, pool) = create_test_db().await;
        let mgr = TokenManager::new(pool);

        let expires_at = Utc::now() + chrono::Duration::hours(1);
        mgr.store_tokens("usr_test", "at_abc", "rt_def", expires_at)
            .await
            .expect("store_tokens should succeed");

        assert!(mgr
            .get_token()
            .await
            .expect("get_token should succeed")
            .is_some());

        mgr.clear_tokens()
            .await
            .expect("clear_tokens should succeed");

        assert!(mgr
            .get_token()
            .await
            .expect("get_token should succeed")
            .is_none());
    }

    #[tokio::test]
    async fn validate_token_returns_true_for_matching_valid_token() {
        let (_tmp, _db_path, pool) = create_test_db().await;
        let mgr = TokenManager::new(pool);

        let expires_at = Utc::now() + chrono::Duration::hours(1);
        mgr.store_tokens("usr_test", "at_abc", "rt_def", expires_at)
            .await
            .expect("store_tokens should succeed");

        assert!(mgr
            .validate_token("at_abc")
            .await
            .expect("validate_token should succeed"));
        assert!(!mgr
            .validate_token("at_wrong")
            .await
            .expect("validate_token should succeed"));
    }

    #[tokio::test]
    async fn validate_token_returns_false_for_expired_token() {
        let (_tmp, _db_path, pool) = create_test_db().await;
        let mgr = TokenManager::new(pool);

        let expires_at = Utc::now() - chrono::Duration::hours(1);
        mgr.store_tokens("usr_test", "at_expired", "rt_old", expires_at)
            .await
            .expect("store_tokens should succeed for expired token");

        assert!(!mgr
            .validate_token("at_expired")
            .await
            .expect("validate_token should succeed"));
    }

    #[tokio::test]
    async fn store_tokens_replaces_existing() {
        let (_tmp, _db_path, pool) = create_test_db().await;
        let mgr = TokenManager::new(pool);

        let expires_at = Utc::now() + chrono::Duration::hours(1);
        mgr.store_tokens("usr_test", "at_old", "rt_old", expires_at)
            .await
            .expect("store_tokens should succeed");

        let expires_at2 = Utc::now() + chrono::Duration::hours(2);
        mgr.store_tokens("usr_test", "at_new", "rt_new", expires_at2)
            .await
            .expect("store_tokens should succeed");

        let token = mgr
            .get_token()
            .await
            .expect("get_token should succeed")
            .expect("token should exist");
        assert_eq!(token.access_token, "at_new");
        assert_eq!(token.refresh_token, "rt_new");
    }

    #[test]
    fn stored_token_needs_refresh_when_within_threshold() {
        let expires_at = Utc::now() + chrono::Duration::minutes(3); // < 5min
        let token = StoredToken {
            user_id: "usr_test".into(),
            access_token: "at".into(),
            refresh_token: "rt".into(),
            expires_at,
            created_at: Utc::now(),
        };
        assert!(token.needs_refresh());
    }

    #[test]
    fn stored_token_does_not_need_refresh_when_far_from_expiry() {
        let expires_at = Utc::now() + chrono::Duration::hours(1);
        let token = StoredToken {
            user_id: "usr_test".into(),
            access_token: "at".into(),
            refresh_token: "rt".into(),
            expires_at,
            created_at: Utc::now(),
        };
        assert!(!token.needs_refresh());
    }

    #[test]
    fn stored_token_is_expired_when_past_expires_at() {
        let expires_at = Utc::now() - chrono::Duration::seconds(1);
        let token = StoredToken {
            user_id: "usr_test".into(),
            access_token: "at".into(),
            refresh_token: "rt".into(),
            expires_at,
            created_at: Utc::now(),
        };
        assert!(token.is_expired());
    }
}
