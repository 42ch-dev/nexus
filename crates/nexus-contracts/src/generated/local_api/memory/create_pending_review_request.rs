//! `Nexus` `CreatePendingReviewRequest`
//!
//! `Request` body for `POST` /v1/local/memory/pending-review. `Called` by the `CLI` at session-end capture. `Optional` fields (`world_id`, `task_kind`, `created_at`) default at runtime (`task_kind` → "unknown", `created_at` → current `RFC` 3339 timestamp); validation limits (`pending_id`/`session_id`/`world_id` ≤ 128, `raw_digest` ≤ 64KB, `task_kind` ≤ 64) stay handler-owned and are intentionally `NOT` encoded here (no behavior redesign).
//!
//! `@schema_version` 1
//! `@source` create-pending-review-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/memory/pending-review. `Called` by the `CLI` at session-end capture. `Optional` fields (`world_id`, `task_kind`, `created_at`) default at runtime (`task_kind` → "unknown", `created_at` → current `RFC` 3339 timestamp); validation limits (`pending_id`/`session_id`/`world_id` ≤ 128, `raw_digest` ≤ 64KB, `task_kind` ≤ 64) stay handler-owned and are intentionally `NOT` encoded here (no behavior redesign).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CreatePendingReviewRequest {
    pub pending_id: String,
    pub session_id: String,
    pub creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_kind: Option<String>,
    pub raw_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}
