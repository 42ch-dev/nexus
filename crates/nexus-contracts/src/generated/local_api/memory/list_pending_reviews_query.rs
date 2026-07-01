//! `Nexus` `ListPendingReviewsQuery`
//!
//! `Query` parameters for `GET` /v1/local/memory/pending-review. `limit` defaults to 50 (clamped 1..=250) when omitted; `cursor` is the opaque `next_cursor` from a previous page (cursor = `pending_id`).
//!
//! `@schema_version` 1
//! `@source` list-pending-reviews-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/local/memory/pending-review. `limit` defaults to 50 (clamped 1..=250) when omitted; `cursor` is the opaque `next_cursor` from a previous page (cursor = `pending_id`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListPendingReviewsQuery {
    pub creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}
