//! `Nexus` `ListWorksResponse`
//!
//! `Response` for `GET` /v1/local/works (cursor-based pagination, `F`-`P3`). `The` array field is `items`; the legacy `works` key was removed in `@42ch/nexus-contracts` 0.6.0.
//!
//! `@schema_version` 2
//! `@source` list-works-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::kb::pagination_info::PaginationInfo;
use crate::generated::local_api::works::work_summary::WorkSummary;

/// `Response` for `GET` /v1/local/works (cursor-based pagination, `F`-`P3`). `The` array field is `items`; the legacy `works` key was removed in `@42ch/nexus-contracts` 0.6.0.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListWorksResponse {
    pub items: Vec<WorkSummary>,
    pub pagination: PaginationInfo,
}
