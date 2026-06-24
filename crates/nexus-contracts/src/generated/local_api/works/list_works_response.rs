//! `Nexus` `ListWorksResponse`
//!
//! `Response` for `GET` /v1/local/works (cursor-based pagination, `F`-`P1`). `The` legacy `total` field is removed; array field name `works` is retained (the `works` -> `items` rename is deferred to `F`-`P3`).
//!
//! `@schema_version` 1
//! `@source` list-works-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::kb::pagination_info::PaginationInfo;
use crate::generated::local_api::works::work_summary::WorkSummary;

/// `Response` for `GET` /v1/local/works (cursor-based pagination, `F`-`P1`). `The` legacy `total` field is removed; array field name `works` is retained (the `works` -> `items` rename is deferred to `F`-`P3`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListWorksResponse {
    pub works: Vec<WorkSummary>,
    pub pagination: PaginationInfo,
}
