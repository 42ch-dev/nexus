//! `ContextAssemblyV1`
//!
//! `Context` `Assembly` request/response schemas retained for deferred direct platform cloud context assembly and `CLI` local in-process context assembly flows. `In` `V1`.26, only local `CLI` assembly is shipped: assemble-local uses `Stage0`/`TwoStage` in-process assembly, and assemble-moment uses local four-domain `Moment` assembly. `There` is no active daemon context-assemble `Local` `API` endpoint.
//!
//! `@schema_version` 1
//! `@source` context-assembly-v1.schema.json

use serde::{Deserialize, Serialize};

/// `Request` shape for deferred direct platform cloud context assembly. `CLI` may use this shape when platform cloud assembly becomes available; `V1`.26 shipped context assembly is local-only and does not send this request to a daemon context-assemble `Local` `API` endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ContextAssembleRequestV1 {
    pub request_id: String,
    pub workspace_id: String,
    pub creator_id: String,
    pub world_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_memory: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_timeline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_story_summaries: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_block_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_kinds: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_timeline_events: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_story_summaries: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
}
/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ContextAssembleResponseV1KeyBlock {
    pub key_block_id: String,
    pub block_type: String,
    pub name: String,
    pub summary: String,
}
/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ContextAssembleResponseV1TimelineEvent {
    pub event_id: String,
    pub event_type: String,
    pub description: String,
    pub occurred_at: String,
}
/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ContextAssembleResponseV1StorySummary {
    pub story_manifest_id: String,
    pub title: String,
    pub summary_text: String,
    pub manifest_type: String,
}
/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ContextAssembleResponseV1MemoryItem {
    pub memory_id: String,
    pub memory_kind: String,
    pub content: String,
}
/// `Response` shape for deferred direct platform cloud context assembly. `Shipped` `V1`.26 local assembly paths run in-process and do not receive this response from a daemon context-assemble `Local` `API` endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ContextAssembleResponseV1 {
    pub request_id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub world_id: String,
    pub assembled_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_freshness_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_blocks: Option<Vec<ContextAssembleResponseV1KeyBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline_events: Option<Vec<ContextAssembleResponseV1TimelineEvent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_summaries: Option<Vec<ContextAssembleResponseV1StorySummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_items: Option<Vec<ContextAssembleResponseV1MemoryItem>>,
}
