//! `Nexus` `ExploreAiAnswerRequest`
//!
//! `Request` body for `Explore` `AI` grounded `Q`&`A` over world / corpus context (platform plan 19). `Boundary` with context assembly: this is platform-side retrieval + generation; wire shape only.
//!
//! `@schema_version` 1
//! `@source` explore-ai-answer-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `Explore` `AI` grounded `Q`&`A` over world / corpus context (platform plan 19). `Boundary` with context assembly: this is platform-side retrieval + generation; wire shape only.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExploreAiAnswerRequest {
    pub schema_version: u32,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_citations: Option<u64>,
}
