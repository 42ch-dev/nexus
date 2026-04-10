//! Nexus PublishStoryRequest
//!
//! Request body for POST /v1/publish/story — explicit story/manuscript publish (platform Publish API; plan 14 slice).
//!
//! @schema_version 1
//! @source publish-story-request.schema.json

use serde::{Deserialize, Serialize};

/// Request body for POST /v1/publish/story — explicit story/manuscript publish (platform Publish API; plan 14 slice).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PublishStoryRequest {
    pub schema_version: u32,
    pub world_id: String,
    pub manuscript_id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_manifest_id: Option<serde_json::Value>,
}
