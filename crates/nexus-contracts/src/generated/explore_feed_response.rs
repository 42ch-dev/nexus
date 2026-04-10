//! Nexus ExploreFeedResponse
//!
//! Paginated Explore results for browse and search responses (POST /v1/explore/browse | /v1/explore/search).
//!
//! @schema_version 1
//! @source explore-feed-response.schema.json

use serde::{Deserialize, Serialize};

/// Paginated Explore results for browse and search responses (POST /v1/explore/browse | /v1/explore/search).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExploreFeedResponse {
    pub schema_version: u32,
    pub entries: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
