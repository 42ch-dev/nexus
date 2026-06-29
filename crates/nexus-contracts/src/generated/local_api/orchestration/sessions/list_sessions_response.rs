//! `Nexus` `ListOrchestrationSessionsResponse`
//!
//! `Response` for `GET` /v1/local/orchestration/sessions (cursor-based pagination, `F`-`P3`). `The` array field is `items`; the legacy `sessions` key was removed in `@42ch/nexus-contracts` 0.6.0.
//!
//! `@schema_version` 2
//! `@source` list-sessions-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::kb::pagination_info::PaginationInfo;
use crate::generated::local_api::orchestration::sessions::session_summary::SessionSummary;

/// `Response` for `GET` /v1/local/orchestration/sessions (cursor-based pagination, `F`-`P3`). `The` array field is `items`; the legacy `sessions` key was removed in `@42ch/nexus-contracts` 0.6.0.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ListSessionsResponse {
    pub items: Vec<SessionSummary>,
    pub pagination: PaginationInfo,
}
