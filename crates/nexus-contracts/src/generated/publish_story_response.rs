//! Nexus PublishStoryResponse
//!
//! Response body for POST /v1/publish/story.
//!
//! @schema_version 1
//! @source publish-story-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common_types::{PublishStoryOutcome};

/// Response body for POST /v1/publish/story.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PublishStoryResponse {
    pub schema_version: u32,
    pub outcome: PublishStoryOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}
