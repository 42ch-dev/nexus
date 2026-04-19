//! Nexus ExploreAiAnswerResponse
//!
//! Response for Explore AI Q&A with optional citations envelope (platform plan 19).
//!
//! @schema_version 1
//! @source explore-ai-answer-response.schema.json

use serde::{Deserialize, Serialize};

/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExploreAiAnswerResponseCitation {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
}
/// Response for Explore AI Q&A with optional citations envelope (platform plan 19).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExploreAiAnswerResponse {
    pub schema_version: u32,
    pub answer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<ExploreAiAnswerResponseCitation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}
