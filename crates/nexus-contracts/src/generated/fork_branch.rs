//! Nexus ForkBranch
//!
//! ForkBranch - describes a world branch forked from a parent world at a specific event. Aligned with data-model-v1.md §5.7.
//!
//! @schema_version 1
//! @source fork-branch.schema.json

use serde::{Deserialize, Serialize};

/// Nexus ForkBranch
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ForkBranch {
    pub schema_version: u32,
    pub fork_branch_id: String,
    pub world_id: String,
    pub parent_world_id: String,
    pub parent_branch_id: String,
    pub forked_from_event_id: String,
    pub status: String,
    pub verification_status: String,
    pub created_by_creator_id: String,
    pub created_at: String,
}
