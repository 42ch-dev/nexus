//! Nexus PublishHistoryEntry
//!
//! Single publish history row (platform Publish API).
//!
//! @schema_version 1
//! @source publish-history-entry.schema.json

use crate::generated::common_types::PublishStoryOutcome;
use serde::{Deserialize, Serialize};

/// Single publish history row (platform Publish API).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PublishHistoryEntry {
    pub occurred_at: String,
    pub outcome: PublishStoryOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_manifest_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
