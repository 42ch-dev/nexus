//! `Nexus` `PendingReviewInfo`
//!
//! `A` single pending-review row in list/get responses. `Mirrors` the `memory_pending_review` table projection 1:1. `task_kind` and `created_at` are always present here (defaults are applied server-side on insert), unlike the create-request where they are optional. `world_id` is nullable.
//!
//! `@schema_version` 1
//! `@source` pending-review-info.schema.json

use serde::{Deserialize, Serialize};

/// `A` single pending-review row in list/get responses. `Mirrors` the `memory_pending_review` table projection 1:1. `task_kind` and `created_at` are always present here (defaults are applied server-side on insert), unlike the create-request where they are optional. `world_id` is nullable.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PendingReviewInfo {
    pub pending_id: String,
    pub session_id: String,
    pub creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    pub task_kind: String,
    pub raw_digest: String,
    pub created_at: String,
}
