//! Nexus PublishHistoryRequest
//!
//! Request body for POST /v1/publish/history — paginated publish history for a manuscript.
//!
//! @schema_version 1
//! @source publish-history-request.schema.json

use serde::{Deserialize, Serialize};

/// Request body for POST /v1/publish/history — paginated publish history for a manuscript.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PublishHistoryRequest {
    pub schema_version: u32,
    pub world_id: String,
    pub manuscript_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}
