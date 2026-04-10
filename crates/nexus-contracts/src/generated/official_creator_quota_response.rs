//! OfficialCreatorQuotaResponseV1
//!
//! GET /official-creator/quota 200 response body. SSOT: v1-spec schema/entitlements-wire-v1.md §4.
//!
//! @schema_version 1
//! @source official-creator-quota-response.schema.json

use serde::{Deserialize, Serialize};

/// GET /official-creator/quota 200 response body. SSOT: v1-spec schema/entitlements-wire-v1.md §4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OfficialCreatorQuotaResponse {
    pub schema_version: u32,
    pub user_id: String,
    pub quota_period_start: String,
    pub quota_period_end: String,
    pub official_runs_consumed: u64,
    pub official_runs_limit: u64,
    pub official_runs_remaining: u64,
    pub max_concurrent_official_jobs: u64,
}
