//! Nexus SocialGraphRelationshipResponse
//!
//! Response envelope for social graph mutation endpoints (platform plan 17).
//!
//! @schema_version 1
//! @source social-graph-relationship-response.schema.json

use serde::{Deserialize, Serialize};

/// Response envelope for social graph mutation endpoints (platform plan 17).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SocialGraphRelationshipResponse {
    pub schema_version: u32,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub following: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorited: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
