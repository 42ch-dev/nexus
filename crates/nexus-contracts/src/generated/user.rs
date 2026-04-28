//! Nexus User Entity
//!
//! End-user account for authentication and platform identity. Aligned with data-model-v1.md §5.1.
//!
//! @schema_version 1
//! @source user.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common_types::{AccountStatus, SubscriptionTier};

/// End-user account for authentication and platform identity. Aligned with data-model-v1.md §5.1.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct User {
    pub schema_version: u32,
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub account_status: AccountStatus,
    pub subscription_tier: SubscriptionTier,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
