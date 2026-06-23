//! `Nexus` `SocialGraphRelationshipRequest`
//!
//! `Request` body for social graph mutations: follow / unfollow / favorite / unfavorite (platform plan 17).
//!
//! `@schema_version` 1
//! `@source` social-graph-relationship-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for social graph mutations: follow / unfollow / favorite / unfavorite (platform plan 17).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SocialGraphRelationshipRequest {
    pub schema_version: u32,
    pub action: String,
    pub target_creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
}
