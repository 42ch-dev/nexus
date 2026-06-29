//! `Nexus` `ListFindingsResponse`
//!
//! `Response` for `GET` /v1/local/works/{`work_id`}/findings (cursor-based pagination, `F`-`P2`). `New` list endpoints use the canonical `items` array key (convention §4); the `pagination` envelope reuses the shared `PaginationInfo`.
//!
//! `@schema_version` 1
//! `@source` list-findings-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::findings::finding_detail_response::FindingDetailResponse;
use crate::generated::local_api::kb::pagination_info::PaginationInfo;

/// `Response` for `GET` /v1/local/works/{`work_id`}/findings (cursor-based pagination, `F`-`P2`). `New` list endpoints use the canonical `items` array key (convention §4); the `pagination` envelope reuses the shared `PaginationInfo`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ListFindingsResponse {
    pub items: Vec<FindingDetailResponse>,
    pub pagination: PaginationInfo,
}
