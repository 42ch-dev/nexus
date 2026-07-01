//! `Nexus` `CreatePendingReviewResponse`
//!
//! `Response` body for `POST` /v1/local/memory/pending-review. `Echoes` the request `pending_id`; `success` is always `true` (uses `INSERT` `OR` `IGNORE` so duplicate retries also return success).
//!
//! `@schema_version` 1
//! `@source` create-pending-review-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` body for `POST` /v1/local/memory/pending-review. `Echoes` the request `pending_id`; `success` is always `true` (uses `INSERT` `OR` `IGNORE` so duplicate retries also return success).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CreatePendingReviewResponse {
    pub success: bool,
    pub pending_id: String,
}
