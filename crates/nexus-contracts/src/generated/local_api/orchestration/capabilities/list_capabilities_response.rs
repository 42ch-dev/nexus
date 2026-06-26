//! `Nexus` `ListCapabilitiesResponse`
//!
//! `Response` for `GET` /v1/local/orchestration/capabilities (cursor-based pagination, `F`-`P3`). `The` array field is `items`; the legacy `capabilities` key was removed in `@42ch/nexus-contracts` 0.6.0.
//!
//! `@schema_version` 2
//! `@source` list-capabilities-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::orchestration::capabilities::capability_info::CapabilityInfo;
use crate::generated::local_api::kb::pagination_info::PaginationInfo;

/// `Response` for `GET` /v1/local/orchestration/capabilities (cursor-based pagination, `F`-`P3`). `The` array field is `items`; the legacy `capabilities` key was removed in `@42ch/nexus-contracts` 0.6.0.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListCapabilitiesResponse {
    pub items: Vec<CapabilityInfo>,
    pub pagination: PaginationInfo,
}
