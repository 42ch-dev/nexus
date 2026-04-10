//! Nexus ExploreCreatorCard
//!
//! Public creator projection for Explore / creator-profile read APIs (platform plan 16 / W3 slice). Field tiers follow v1-spec visibility; omit sensitive fields at the edge.
//!
//! @schema_version 1
//! @source explore-creator-card.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common_types::{Visibility};

/// Public creator projection for Explore / creator-profile read APIs (platform plan 16 / W3 slice). Field tiers follow v1-spec visibility; omit sensitive fields at the edge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExploreCreatorCard {
    pub schema_version: u32,
    pub creator_id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follower_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
}
