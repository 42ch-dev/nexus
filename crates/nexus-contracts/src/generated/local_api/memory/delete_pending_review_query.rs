//! `Nexus` `DeletePendingReviewQuery`
//!
//! `Query` parameters for `DELETE` /v1/local/memory/pending-review/{id}. `The` `{id}` path parameter is the pending review's `pending_id` (not modeled here); `creator_id` gates ownership.
//!
//! `@schema_version` 1
//! `@source` delete-pending-review-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `DELETE` /v1/local/memory/pending-review/{id}. `The` `{id}` path parameter is the pending review's `pending_id` (not modeled here); `creator_id` gates ownership.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DeletePendingReviewQuery {
    pub creator_id: String,
}
