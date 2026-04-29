//! `Nexus` `WorldMembership`
//!
//! `WorldMembership` entity describing `Creator`-`World` relationship with roles and permissions. `Aligned` with data-model-v1.md §5.4.
//!
//! `@schema_version` 1
//! `@source` world-membership.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common_types::{MembershipRole, MembershipStatus};

/// `WorldMembership` entity describing `Creator`-`World` relationship with roles and permissions. `Aligned` with data-model-v1.md §5.4.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldMembership {
    pub schema_version: u32,
    pub membership_id: String,
    pub world_id: String,
    pub creator_id: String,
    pub role: MembershipRole,
    pub membership_status: MembershipStatus,
    pub joined_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<serde_json::Value>,
}
