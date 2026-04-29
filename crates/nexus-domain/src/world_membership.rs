//! `WorldMembership` aggregate — Creator-World relationship with roles and permissions.
//!
//! Tracks which creators belong to which worlds and what they can do.
//! See data-model-v1.md §5.4.

use serde::{Deserialize, Serialize};
use std::str::FromStr;
use strum::Display;

/// Membership role enum - matches v1-spec §5.4, §7
/// Values: owner, maintainer, collaborator, `official_creator`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum MembershipRole {
    Owner,
    Maintainer,
    Collaborator,
    OfficialCreator,
}

/// Membership status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MembershipStatus {
    Active,
    Invited,
    Suspended,
    Removed,
}

impl MembershipStatus {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Invited => "invited",
            Self::Suspended => "suspended",
            Self::Removed => "removed",
        }
    }
}

/// World membership permissions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct MembershipPermissions {
    pub can_sync_kb: bool,
    pub can_publish: bool,
    pub can_fork: bool,
    pub can_invite_official_creator: bool,
    pub can_confirm_canon: bool,
}

impl MembershipPermissions {
    #[must_use]
    pub const fn owner_permissions() -> Self {
        Self {
            can_sync_kb: true,
            can_publish: true,
            can_fork: true,
            can_invite_official_creator: true,
            can_confirm_canon: true,
        }
    }

    pub const fn maintainer_permissions() -> Self {
        Self {
            can_sync_kb: true,
            can_publish: true,
            can_fork: true,
            can_invite_official_creator: true,
            can_confirm_canon: true,
        }
    }

    pub const fn collaborator_permissions() -> Self {
        Self {
            can_sync_kb: true,
            can_publish: false,
            can_fork: true,
            can_invite_official_creator: false,
            can_confirm_canon: false,
        }
    }

    pub const fn official_creator_permissions() -> Self {
        Self {
            can_sync_kb: false,
            can_publish: false,
            can_fork: true,
            can_invite_official_creator: false,
            can_confirm_canon: true,
        }
    }
}

/// `WorldMembership` aggregate — Creator-World relationship.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldMembership {
    pub schema_version: u32,
    pub membership_id: String,
    pub world_id: String,
    pub creator_id: String,
    pub role: String,
    pub membership_status: String,
    pub joined_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<MembershipPermissions>,
}

impl WorldMembership {
    /// Create a new world membership.
    #[must_use]
    pub fn new(world_id: &str, creator_id: &str, role: MembershipRole) -> Self {
        let membership_id = format!("wmb_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        let permissions = match role {
            MembershipRole::Owner => Some(MembershipPermissions::owner_permissions()),
            MembershipRole::Maintainer => Some(MembershipPermissions::maintainer_permissions()),
            MembershipRole::Collaborator => Some(MembershipPermissions::collaborator_permissions()),
            MembershipRole::OfficialCreator => {
                Some(MembershipPermissions::official_creator_permissions())
            }
        };
        Self {
            schema_version: 1,
            membership_id,
            world_id: world_id.to_string(),
            creator_id: creator_id.to_string(),
            role: role.to_string(),
            membership_status: MembershipStatus::Active.as_str().to_string(),
            joined_at: chrono::Utc::now().to_rfc3339(),
            permissions,
        }
    }

    /// Check if membership is active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.membership_status == MembershipStatus::Active.as_str()
    }

    /// Get `can_confirm_canon` permission.
    pub fn can_confirm_canon(&self) -> bool {
        self.permissions
            .as_ref()
            .is_some_and(|p| p.can_confirm_canon)
    }

    /// Get `can_sync_kb` permission.
    pub fn can_sync_kb(&self) -> bool {
        self.permissions.as_ref().is_some_and(|p| p.can_sync_kb)
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::WorldMembership> for WorldMembership {
    fn from(c: nexus_contracts::WorldMembership) -> Self {
        Self {
            schema_version: c.schema_version,
            membership_id: c.membership_id,
            world_id: c.world_id,
            creator_id: c.creator_id,
            role: c.role.as_str().to_string(),
            membership_status: c.membership_status.as_str().to_string(),
            joined_at: c.joined_at,
            permissions: c
                .permissions
                .map(|v| serde_json::from_value(v).unwrap_or_default()),
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<WorldMembership> for nexus_contracts::WorldMembership {
    fn from(d: WorldMembership) -> Self {
        Self {
            schema_version: d.schema_version,
            membership_id: d.membership_id,
            world_id: d.world_id,
            creator_id: d.creator_id,
            role: nexus_contracts::MembershipRole::from_str(&d.role).unwrap(),
            membership_status: nexus_contracts::MembershipStatus::from_str(&d.membership_status)
                .unwrap(),
            joined_at: d.joined_at,
            permissions: d
                .permissions
                .map(|p| serde_json::to_value(p).unwrap_or_default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_owner_membership() {
        let m = WorldMembership::new("wld_test", "ctr_owner", MembershipRole::Owner);
        assert_eq!(m.role, "owner");
        assert!(m.is_active());
        assert!(m.can_confirm_canon());
        assert!(m.can_sync_kb());
    }

    #[test]
    fn test_create_maintainer_membership() {
        let m = WorldMembership::new("wld_test", "ctr_maintainer", MembershipRole::Maintainer);
        assert_eq!(m.role, "maintainer");
        assert!(m.is_active());
        assert!(m.can_confirm_canon());
        assert!(m.can_sync_kb());
    }

    #[test]
    fn test_create_collaborator_membership() {
        let m = WorldMembership::new("wld_test", "ctr_collab", MembershipRole::Collaborator);
        assert!(!m.can_confirm_canon());
        assert!(m.can_sync_kb());
    }

    #[test]
    fn test_create_official_creator_membership() {
        let m = WorldMembership::new("wld_test", "ctr_official", MembershipRole::OfficialCreator);
        assert_eq!(m.role, "official_creator");
        assert!(m.is_active());
        assert!(m.can_confirm_canon());
        assert!(!m.can_sync_kb());
    }

    #[test]
    fn test_serialize_roundtrip() {
        let m = WorldMembership::new("wld_test", "ctr_owner", MembershipRole::Owner);
        let json = serde_json::to_string(&m).unwrap();
        let deserialized: WorldMembership = serde_json::from_str(&json).unwrap();
        assert_eq!(m, deserialized);
    }
}

#[cfg(test)]
mod enum_alignment_tests {
    use super::*;

    #[test]
    fn membership_role_matches_spec() {
        let roles = [
            MembershipRole::Owner,
            MembershipRole::Maintainer,
            MembershipRole::Collaborator,
            MembershipRole::OfficialCreator,
        ];
        assert_eq!(roles.len(), 4);
    }

    #[test]
    fn official_creator_role_exists() {
        let role = MembershipRole::OfficialCreator;
        assert_eq!(role.to_string(), "official_creator");
    }

    #[test]
    fn maintainer_role_exists() {
        let role = MembershipRole::Maintainer;
        assert_eq!(role.to_string(), "maintainer");
    }
}

#[cfg(test)]
mod permission_tests {
    use super::*;

    #[test]
    fn maintainer_can_confirm_and_sync() {
        let role = MembershipRole::Maintainer;
        let m = WorldMembership::new("wld_test", "ctr_maintainer", role);
        assert!(m.can_confirm_canon());
        assert!(m.can_sync_kb());
    }

    #[test]
    fn official_creator_can_confirm_but_not_sync() {
        let role = MembershipRole::OfficialCreator;
        let m = WorldMembership::new("wld_test", "ctr_official", role);
        assert!(m.can_confirm_canon());
        assert!(!m.can_sync_kb());
    }

    #[test]
    fn collaborator_can_sync_but_not_confirm() {
        let role = MembershipRole::Collaborator;
        let m = WorldMembership::new("wld_test", "ctr_collab", role);
        assert!(!m.can_confirm_canon());
        assert!(m.can_sync_kb());
    }
}
