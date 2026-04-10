//! Nexus PublishHistoryResponse
//!
//! Response body for POST /v1/publish/history.
//!
//! @schema_version 1
//! @source publish-history-response.schema.json

use serde::{Deserialize, Serialize};

/// Response body for POST /v1/publish/history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PublishHistoryResponse {
    pub schema_version: u32,
    pub entries: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
