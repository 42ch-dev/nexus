//! Nexus SocialGraphFeedResponse
//!
//! Paginated personalized feed for social graph (platform plan 17). Entries are activity rows; shape may evolve per v1-spec.
//!
//! @schema_version 1
//! @source social-graph-feed-response.schema.json

use serde::{Deserialize, Serialize};

/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SocialGraphFeedResponseEntry {
    pub edge_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_creator_id: Option<String>,
    pub verb: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub occurred_at: String,
}
/// Paginated personalized feed for social graph (platform plan 17). Entries are activity rows; shape may evolve per v1-spec.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SocialGraphFeedResponse {
    pub schema_version: u32,
    pub entries: Vec<SocialGraphFeedResponseEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
