//! MeEntitlementsResponseV1
//!
//! GET /me/entitlements 200 response body. SSOT: v1-spec schema/entitlements-wire-v1.md §3.
//!
//! @schema_version 1
//! @source me-entitlements-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common_types::{AccountStatus, SubscriptionTier};

/// GET /me/entitlements 200 response body. SSOT: v1-spec schema/entitlements-wire-v1.md §3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MeEntitlementsResponse {
    pub schema_version: u32,
    pub user_id: String,
    pub subscription_tier: SubscriptionTier,
    pub account_status: AccountStatus,
    pub official_creator: serde_json::Value,
}
