//! World aggregate — a narrative universe maintained by creators.
//!
//! World is the top-level container for all story content, key blocks,
//! timeline events, and world memberships.
//! See data-model-v1.md §5.3.

use crate::errors::DomainError;
use crate::fork_branch::ForkBranch;
use nexus_contracts::{TimePolicy, Visibility};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// World status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorldStatus {
    Active,
    Archived,
    Paused,
}

impl WorldStatus {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Paused => "paused",
        }
    }
}

/// World aggregate — a narrative universe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct World {
    pub schema_version: u32,
    pub world_id: String,
    pub owner_creator_id: String,
    pub title: String,
    pub slug: String,
    pub status: String,
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

impl World {
    /// Create a new world.
    #[must_use]
    pub fn new(
        world_id: &str,
        owner_creator_id: &str,
        title: &str,
        slug: &str,
        visibility: Visibility,
        time_policy: TimePolicy,
    ) -> Self {
        Self {
            schema_version: 1,
            world_id: world_id.to_string(),
            owner_creator_id: owner_creator_id.to_string(),
            title: title.to_string(),
            slug: slug.to_string(),
            status: WorldStatus::Active.as_str().to_string(),
            visibility,
            time_policy,
            canon_revision: Some(0),
            current_timeline_head_id: None,
            current_time_pointer: None,
            root_fork_branch_id: None,
            world_rules: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: None,
        }
    }

    /// Update time pointer (world progression).
    pub fn advance_time_pointer(&mut self, event_id: &str) -> Result<(), DomainError> {
        if self.status != WorldStatus::Active.as_str() {
            return Err(DomainError::InvalidState {
                expected: "active".to_string(),
                actual: self.status.clone(),
            });
        }
        self.current_time_pointer = Some(event_id.to_string());
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    /// Fork this world.
    /// Creates a new child world and `ForkBranch` record.
    pub fn fork(
        &self,
        creator_id: &str,
        forked_from_event_id: &str,
    ) -> Result<(World, ForkBranch), DomainError> {
        if self.status != WorldStatus::Active.as_str() {
            return Err(DomainError::InvalidState {
                expected: "active".to_string(),
                actual: self.status.clone(),
            });
        }

        let child_world_id = format!("wld_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        let parent_branch = self.root_fork_branch_id.as_deref().unwrap_or("fbk_root");

        let fork = ForkBranch::fork_from(
            &child_world_id,
            &self.world_id,
            parent_branch,
            forked_from_event_id,
            creator_id,
        );

        let mut child_world = World::new(
            &child_world_id,
            creator_id,
            &format!("{} (fork)", self.title),
            &format!("{}-fork", self.slug),
            Visibility::Private,
            self.time_policy,
        );
        child_world.root_fork_branch_id = Some(fork.fork_branch_id.clone());

        Ok((child_world, fork))
    }
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    /// Update visibility.
    pub fn set_visibility(&mut self, visibility: Visibility) -> Result<(), DomainError> {
        self.visibility = visibility;
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::World> for World {
    fn from(c: nexus_contracts::World) -> Self {
        Self {
            schema_version: c.schema_version,
            world_id: c.world_id,
            owner_creator_id: c.owner_creator_id,
            title: c.title,
            slug: c.slug,
            status: c.status.as_str().to_string(),
            visibility: c.visibility,
            time_policy: c.time_policy,
            canon_revision: c.canon_revision,
            current_timeline_head_id: c.current_timeline_head_id,
            current_time_pointer: c.current_time_pointer,
            root_fork_branch_id: c.root_fork_branch_id,
            world_rules: c.world_rules,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<World> for nexus_contracts::World {
    fn from(d: World) -> Self {
        Self {
            schema_version: d.schema_version,
            world_id: d.world_id,
            owner_creator_id: d.owner_creator_id,
            title: d.title,
            slug: d.slug,
            status: nexus_contracts::WorldStatus::from_str(&d.status).unwrap(),
            visibility: d.visibility,
            time_policy: d.time_policy,
            canon_revision: d.canon_revision,
            current_timeline_head_id: d.current_timeline_head_id,
            current_time_pointer: d.current_time_pointer,
            root_fork_branch_id: d.root_fork_branch_id,
            world_rules: d.world_rules,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_world() {
        let world = World::new(
            "wld_test",
            "ctr_owner",
            "My World",
            "my-world",
            Visibility::Private,
            TimePolicy::Manual,
        );
        assert_eq!(world.status, "active");
        assert_eq!(world.visibility, Visibility::Private);
        assert_eq!(world.canon_revision, Some(0));
        assert!(world.root_fork_branch_id.is_none());
    }

    #[test]
    fn test_advance_time_pointer() {
        let mut world = World::new(
            "wld_test",
            "ctr_owner",
            "My World",
            "my-world",
            Visibility::Private,
            TimePolicy::Manual,
        );
        world.advance_time_pointer("evt_123").unwrap();
        assert_eq!(world.current_time_pointer.as_deref(), Some("evt_123"));
    }

    #[test]
    fn test_fork_world() {
        let world = World::new(
            "wld_parent",
            "ctr_owner",
            "Parent World",
            "parent-world",
            Visibility::Public,
            TimePolicy::OwnerDriven,
        );
        let (child_world, fork) = world.fork("ctr_forker", "evt_100").unwrap();
        assert_eq!(fork.world_id, child_world.world_id);
        assert_eq!(fork.parent_world_id, "wld_parent");
        assert_eq!(child_world.visibility, Visibility::Private);
        assert!(child_world.root_fork_branch_id.is_some());
    }

    #[test]
    fn test_set_visibility() {
        let mut world = World::new(
            "wld_test",
            "ctr_owner",
            "My World",
            "my-world",
            Visibility::Private,
            TimePolicy::Manual,
        );
        world.set_visibility(Visibility::Public).unwrap();
        assert_eq!(world.visibility, Visibility::Public);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let world = World::new(
            "wld_test",
            "ctr_owner",
            "My World",
            "my-world",
            Visibility::Private,
            TimePolicy::Manual,
        );
        let json = serde_json::to_string(&world).unwrap();
        let deserialized: World = serde_json::from_str(&json).unwrap();
        assert_eq!(world, deserialized);
    }
}
