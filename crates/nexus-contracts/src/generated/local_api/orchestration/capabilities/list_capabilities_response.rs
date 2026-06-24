//! `Nexus` `ListCapabilitiesResponse`
//!
//! `Response` for `GET` /v1/local/orchestration/capabilities.
//!
//! `@schema_version` 1
//! `@source` list-capabilities-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::orchestration::capabilities::capability_info::CapabilityInfo;

/// `Response` for `GET` /v1/local/orchestration/capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListCapabilitiesResponse {
    pub capabilities: Vec<CapabilityInfo>,
}
