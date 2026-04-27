//! Nexus ExploreHit
//!
//! Single browse/search result row for Explore read APIs (platform contract; plan 16 slice).
//!
//! @schema_version 1
//! @source explore-hit.schema.json

use crate::generated::common_types::Visibility;
use serde::{Deserialize, Serialize};

/// Single browse/search result row for Explore read APIs (platform contract; plan 16 slice).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExploreHit {
    pub hit_type: String,
    pub entity_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
}
