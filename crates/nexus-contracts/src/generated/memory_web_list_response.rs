//! Nexus MemoryWebListResponse
//!
//! Paginated list response for memory web read APIs (platform plan 18). Items are read projections; full MemoryItem sync may use domain bundle types separately.
//!
//! @schema_version 1
//! @source memory-web-list-response.schema.json

use serde::{Deserialize, Serialize};

/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MemoryWebListResponseItem {
    pub memory_item_id: String,
    pub creator_id: String,
    pub world_id: String,
    pub memory_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_kind: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
/// Paginated list response for memory web read APIs (platform plan 18). Items are read projections; full MemoryItem sync may use domain bundle types separately.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct MemoryWebListResponse {
    pub schema_version: u32,
    pub items: Vec<MemoryWebListResponseItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
