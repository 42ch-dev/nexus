//! `Nexus` `ListSchedulesResponse`
//!
//! `Response` for `GET` /v1/local/orchestration/schedules (cursor-based pagination, `F`-`P3`). `The` array field is `items`; the legacy `schedules` key was removed in `@42ch/nexus-contracts` 0.6.0.
//!
//! `@schema_version` 2
//! `@source` list-schedules-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::kb::pagination_info::PaginationInfo;
use crate::generated::local_api::schedule::schedule_summary::ScheduleSummary;

/// `Response` for `GET` /v1/local/orchestration/schedules (cursor-based pagination, `F`-`P3`). `The` array field is `items`; the legacy `schedules` key was removed in `@42ch/nexus-contracts` 0.6.0.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListSchedulesResponse {
    pub items: Vec<ScheduleSummary>,
    pub pagination: PaginationInfo,
}
