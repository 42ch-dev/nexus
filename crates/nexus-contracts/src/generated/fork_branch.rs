//! `Nexus` `ForkBranch`
//!
//! `ForkBranch` - describes a world branch forked from a parent world at a specific event. `Aligned` with data-model-v1.md §5.7.
//!
//! `@schema_version` 1
//! `@source` fork-branch.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common_types::{ForkBranchStatus, VerificationStatus};

/// `ForkBranch` - describes a world branch forked from a parent world at a specific event. `Aligned` with data-model-v1.md §5.7.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ForkBranch {
    pub schema_version: u32,
    pub fork_branch_id: String,
    pub world_id: String,
    pub parent_world_id: String,
    pub parent_branch_id: String,
    pub forked_from_event_id: String,
    pub status: ForkBranchStatus,
    pub verification_status: VerificationStatus,
    pub created_by_creator_id: String,
    pub created_at: String,
}
