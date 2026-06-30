//! `Nexus` `ReviewRequest`
//!
//! `Request` body for `POST` /v1/local/memory/review. `Triggers` the review/summarization pipeline for the active creator's entire pending queue. `creator_id` must match the active creator (config.toml), otherwise 403.
//!
//! `@schema_version` 1
//! `@source` review-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/memory/review. `Triggers` the review/summarization pipeline for the active creator's entire pending queue. `creator_id` must match the active creator (config.toml), otherwise 403.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ReviewRequest {
    pub creator_id: String,
}
