//! Nexus ExploreAiSummaryRequest
//!
//! Request body for Explore AI summarization over a world or manuscript (platform plan 19).
//!
//! @schema_version 1
//! @source explore-ai-summary-request.schema.json

use serde::{Deserialize, Serialize};

/// Request body for Explore AI summarization over a world or manuscript (platform plan 19).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExploreAiSummaryRequest {
    pub schema_version: u32,
    pub scope: String,
    pub entity_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<i64>,
}
