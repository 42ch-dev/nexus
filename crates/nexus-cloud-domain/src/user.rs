//! User aggregate — end-user account identity.
//!
//! Aligned with data-model-v1.md §5.1.

use crate::errors::CloudDomainError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Account lifecycle status (spec §5.1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Active,
    Suspended,
    Deleted,
}

impl AccountStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Deleted => "deleted",
        }
    }
}

/// Subscription / entitlements tier (spec §5.1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionTier {
    Free,
    Pro,
    Studio,
    Enterprise,
}

impl SubscriptionTier {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Pro => "pro",
            Self::Studio => "studio",
            Self::Enterprise => "enterprise",
        }
    }
}

/// User aggregate — platform identity for auth and sync attribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub schema_version: u32,
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub account_status: String,
    pub subscription_tier: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl User {
    /// New user with active account and free tier.
    #[must_use]
    pub fn register(user_id: &str, username: &str, email: &str, display_name: &str) -> Self {
        Self {
            schema_version: 1,
            user_id: user_id.to_string(),
            username: username.to_string(),
            email: email.to_string(),
            display_name: display_name.to_string(),
            account_status: AccountStatus::Active.as_str().to_string(),
            subscription_tier: SubscriptionTier::Free.as_str().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: None,
        }
    }

    /// Suspend an active account.
    pub fn suspend(&mut self) -> Result<(), CloudDomainError> {
        if self.account_status == AccountStatus::Deleted.as_str() {
            return Err(CloudDomainError::InvalidState {
                expected: "active or suspended".to_string(),
                actual: self.account_status.clone(),
            });
        }
        if self.account_status == AccountStatus::Suspended.as_str() {
            return Err(CloudDomainError::AlreadyInState("suspended".to_string()));
        }
        self.account_status = AccountStatus::Suspended.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(CloudDomainError::...)` if validation fails.
    /// Soft-delete marker (`account_status` = deleted).
    pub fn mark_deleted(&mut self) -> Result<(), CloudDomainError> {
        if self.account_status == AccountStatus::Deleted.as_str() {
            return Err(CloudDomainError::AlreadyInState("deleted".to_string()));
        }
        self.account_status = AccountStatus::Deleted.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(CloudDomainError::...)` if validation fails.
    ///
    /// # Errors
    /// Returns `Err(CloudDomainError::...)` if validation fails.
    /// Change subscription tier (blocked when deleted).
    pub fn set_subscription_tier(
        &mut self,
        tier: SubscriptionTier,
    ) -> Result<(), CloudDomainError> {
        if self.account_status == AccountStatus::Deleted.as_str() {
            return Err(CloudDomainError::InvalidState {
                expected: "active or suspended".to_string(),
                actual: "deleted".to_string(),
            });
        }
        self.subscription_tier = tier.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
}

impl From<nexus_contracts::User> for User {
    fn from(c: nexus_contracts::User) -> Self {
        Self {
            schema_version: c.schema_version,
            user_id: c.user_id,
            username: c.username,
            email: c.email,
            display_name: c.display_name,
            account_status: c.account_status.as_str().to_string(),
            subscription_tier: c.subscription_tier.as_str().to_string(),
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

impl TryFrom<User> for nexus_contracts::User {
    type Error = CloudDomainError;

    fn try_from(d: User) -> Result<Self, Self::Error> {
        let account_status =
            nexus_contracts::AccountStatus::from_str(&d.account_status).map_err(|_| {
                CloudDomainError::ValidationError(format!(
                    "account_status {:?} is not a valid wire enum value",
                    d.account_status
                ))
            })?;
        let subscription_tier = nexus_contracts::SubscriptionTier::from_str(&d.subscription_tier)
            .map_err(|_| {
            CloudDomainError::ValidationError(format!(
                "subscription_tier {:?} is not a valid wire enum value",
                d.subscription_tier
            ))
        })?;
        Ok(Self {
            schema_version: d.schema_version,
            user_id: d.user_id,
            username: d.username,
            email: d.email,
            display_name: d.display_name,
            account_status,
            subscription_tier,
            created_at: d.created_at,
            updated_at: d.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_defaults_free_active() {
        let u = User::register("usr_1", "alice", "a@example.com", "Alice");
        assert_eq!(u.account_status, "active");
        assert_eq!(u.subscription_tier, "free");
    }

    #[test]
    fn suspend_from_active() {
        let mut u = User::register("usr_1", "alice", "a@example.com", "Alice");
        u.suspend().unwrap();
        assert_eq!(u.account_status, "suspended");
        assert!(u.suspend().is_err());
    }

    #[test]
    fn cannot_mutate_subscription_when_deleted() {
        let mut u = User::register("usr_1", "alice", "a@example.com", "Alice");
        u.mark_deleted().unwrap();
        assert!(u.set_subscription_tier(SubscriptionTier::Pro).is_err());
    }

    #[test]
    fn contract_roundtrip() {
        let u = User::register("usr_rt", "bob", "b@example.com", "Bob");
        let c = nexus_contracts::User::try_from(u.clone()).unwrap();
        let back = User::from(c);
        assert_eq!(back.user_id, u.user_id);
        assert_eq!(back.username, u.username);
        assert_eq!(back.email, u.email);
        assert_eq!(back.account_status, u.account_status);
        assert_eq!(back.subscription_tier, u.subscription_tier);
    }
}
