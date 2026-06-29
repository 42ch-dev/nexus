//! `Nexus` `ExploreAiSummaryResponse`
//!
//! `Response` for `Explore` `AI` summarization (platform plan 19).
//!
//! `@schema_version` 1
//! `@source` explore-ai-summary-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `Explore` `AI` summarization (platform plan 19).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExploreAiSummaryResponse {
    pub schema_version: u32,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}
