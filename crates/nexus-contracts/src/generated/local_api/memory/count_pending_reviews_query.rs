//! `Nexus` `CountPendingReviewsQuery`
//!
//! `Query` parameters for `GET` /v1/local/memory/pending-review/count.
//!
//! `@schema_version` 1
//! `@source` count-pending-reviews-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/local/memory/pending-review/count.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CountPendingReviewsQuery {
    pub creator_id: String,
}
