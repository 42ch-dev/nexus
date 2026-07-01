//! `Nexus` `ListPendingReviewsResponse`
//!
//! `Response` for `GET` /v1/local/memory/pending-review (cursor-based pagination). `The` `pagination` envelope reuses the shared `PaginationInfo`; `next_cursor` is the `pending_id` of the last item in the page (opaque to clients).
//!
//! `@schema_version` 1
//! `@source` list-pending-reviews-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::kb::pagination_info::PaginationInfo;
use crate::generated::local_api::memory::pending_review_info::PendingReviewInfo;

/// `Response` for `GET` /v1/local/memory/pending-review (cursor-based pagination). `The` `pagination` envelope reuses the shared `PaginationInfo`; `next_cursor` is the `pending_id` of the last item in the page (opaque to clients).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListPendingReviewsResponse {
    pub items: Vec<PendingReviewInfo>,
    pub pagination: PaginationInfo,
}
