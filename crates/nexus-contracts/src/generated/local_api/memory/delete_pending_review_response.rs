//! `Nexus` `DeletePendingReviewResponse`
//!
//! `Response` body for `DELETE` /v1/local/memory/pending-review/{id}. `Echoes` the path `pending_id`; `success` is `true` on deletion (a missing or non-owned row surfaces as an error envelope, not `success: false`).
//!
//! `@schema_version` 1
//! `@source` delete-pending-review-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` body for `DELETE` /v1/local/memory/pending-review/{id}. `Echoes` the path `pending_id`; `success` is `true` on deletion (a missing or non-owned row surfaces as an error envelope, not `success: false`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DeletePendingReviewResponse {
    pub success: bool,
    pub pending_id: String,
}
