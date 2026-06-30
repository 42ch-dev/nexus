//! `Nexus` `CountPendingReviewsResponse`
//!
//! `Response` body for `GET` /v1/local/memory/pending-review/count. `count` is the number of pending-review rows for the creator.
//!
//! `@schema_version` 1
//! `@source` count-pending-reviews-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` body for `GET` /v1/local/memory/pending-review/count. `count` is the number of pending-review rows for the creator.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CountPendingReviewsResponse {
    pub count: i64,
}
