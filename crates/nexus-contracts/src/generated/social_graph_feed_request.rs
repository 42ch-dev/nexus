//! Nexus SocialGraphFeedRequest
//!
//! Request body for personalized social / activity feed listing (platform plan 17).
//!
//! @schema_version 1
//! @source social-graph-feed-request.schema.json

use serde::{Deserialize, Serialize};

/// Request body for personalized social / activity feed listing (platform plan 17).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SocialGraphFeedRequest {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}
