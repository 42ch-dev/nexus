//! WorldMembership aggregate — Creator-World relationship with roles and permissions.
//!
//! Tracks which creators belong to which worlds and what they can do.
//! See data-model-v1.md §5.4.

use serde::{Deserialize, Serialize};

/// Membership role enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MembershipRole {
    Owner,
    Admin,
    Curator,
    Collaborator,
    Viewer,
}

impl MembershipRole {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Curator => "curator",
            Self::Collaborator => "collaborator",
            Self::Viewer => "viewer",
        }
    }
}

/// Membership status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MembershipStatus {
    Active,
    Suspended,
    Left,
}

impl MembershipStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Left => "left",
        }
    }
}

/// World membership permissions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MembershipPermissions {
    pub can_sync_kb: bool,
    pub can_publish: bool,
    pub can_fork: bool,
    pub can_invite_official_creator: bool,
    pub can_confirm_canon: bool,
}

impl MembershipPermissions {
    pub fn owner_permissions() -> Self {
        Self {
            can_sync_kb: true,
            can_publish: true,
            can_fork: true,
            can_invite_official_creator: true,
            can_confirm_canon: true,
        }
    }

    pub fn admin_permissions() -> Self {
        Self {
            can_sync_kb: true,
            can_publish: true,
            can_fork: true,
            can_invite_official_creator: true,
            can_confirm_canon: true,
        }
    }

    pub fn collaborator_permissions() -> Self {
        Self {
            can_sync_kb: true,
            can_publish: false,
            can_fork: true,
            can_invite_official_creator: false,
            can_confirm_canon: false,
        }
    }

    pub fn viewer_permissions() -> Self {
        Self {
            can_sync_kb: false,
            can_publish: false,
            can_fork: true,
            can_invite_official_creator: false,
            can_confirm_canon: false,
        }
    }
}

/// WorldMembership aggregate — Creator-World relationship.
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
    pub fn new(world_id: &str, creator_id: &str, role: MembershipRole) -> Self {
        let membership_id = format!("wmb_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        let permissions = match role {
            MembershipRole::Owner => Some(MembershipPermissions::owner_permissions()),
            MembershipRole::Admin => Some(MembershipPermissions::admin_permissions()),
            MembershipRole::Curator => Some(MembershipPermissions::collaborator_permissions()),
            MembershipRole::Collaborator => Some(MembershipPermissions::collaborator_permissions()),
            MembershipRole::Viewer => Some(MembershipPermissions::viewer_permissions()),
        };
        Self {
            schema_version: 1,
            membership_id,
            world_id: world_id.to_string(),
            creator_id: creator_id.to_string(),
            role: role.as_str().to_string(),
            membership_status: MembershipStatus::Active.as_str().to_string(),
            joined_at: chrono::Utc::now().to_rfc3339(),
            permissions,
        }
    }

    /// Check if membership is active.
    pub fn is_active(&self) -> bool {
        self.membership_status == MembershipStatus::Active.as_str()
    }

    /// Get can_confirm_canon permission.
    pub fn can_confirm_canon(&self) -> bool {
        self.permissions
            .as_ref()
            .map(|p| p.can_confirm_canon)
            .unwrap_or(false)
    }

    /// Get can_sync_kb permission.
    pub fn can_sync_kb(&self) -> bool {
        self.permissions
            .as_ref()
            .map(|p| p.can_sync_kb)
            .unwrap_or(false)
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
            role: c.role,
            membership_status: c.membership_status,
            joined_at: c.joined_at,
            permissions: c
                .permissions
                .map(|v| serde_json::from_value(v).unwrap_or_default()),
        }
    }
}

impl From<WorldMembership> for nexus_contracts::WorldMembership {
    fn from(d: WorldMembership) -> Self {
        Self {
            schema_version: d.schema_version,
            membership_id: d.membership_id,
            world_id: d.world_id,
            creator_id: d.creator_id,
            role: d.role,
            membership_status: d.membership_status,
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
    fn test_create_collaborator_membership() {
        let m = WorldMembership::new("wld_test", "ctr_collab", MembershipRole::Collaborator);
        assert!(!m.can_confirm_canon());
        assert!(m.can_sync_kb());
    }

    #[test]
    fn test_create_viewer_membership() {
        let m = WorldMembership::new("wld_test", "ctr_viewer", MembershipRole::Viewer);
        assert!(!m.can_confirm_canon());
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
