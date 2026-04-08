//! Nexus World Entity
//!
//! World entity - a narrative universe maintained by creators with timeline evolution. Aligned with data-model-v1.md §5.3.
//!
//! @schema_version 1
//! @source world.schema.json

use crate::generated::common_types::{TimePolicy, Visibility, WorldStatus};
use serde::{Deserialize, Serialize};

/// World entity - a narrative universe maintained by creators with timeline evolution. Aligned with data-model-v1.md §5.3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct World {
    pub schema_version: u32,
    pub world_id: String,
    pub owner_creator_id: String,
    pub title: String,
    pub slug: String,
    pub status: WorldStatus,
    pub visibility: Visibility,
    pub time_policy: TimePolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canon_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_timeline_head_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_time_pointer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_fork_branch_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_rules: Option<serde_json::Value>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
