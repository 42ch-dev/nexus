//! ForkBranch aggregate — world branch forked from a parent world.
//!
//! ForkBranch tracks the lineage of world forks, including verification
//! status and write scope validation. See data-model-v1.md §5.7,
//! consistency-rules-v1.md §3.4.

use crate::errors::DomainError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// ForkBranch status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ForkBranchStatus {
    Active,
    Archived,
}

impl ForkBranchStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }
}

/// ForkBranch verification status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Unverified,
    Requested,
    Verified,
    Rejected,
}

impl VerificationStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Unverified => "unverified",
            Self::Requested => "requested",
            Self::Verified => "verified",
            Self::Rejected => "rejected",
        }
    }
}

/// ForkBranch aggregate — describes a world branch forked from a parent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

impl ForkBranch {
    /// Create a new fork from a parent world at a specific event.
    /// Per consistency-rules-v1.md §3.4: must reference valid parent_world_id,
    /// parent_branch_id, and forked_from_event_id.
    pub fn fork_from(
        world_id: &str,
        parent_world_id: &str,
        parent_branch_id: &str,
        forked_from_event_id: &str,
        creator_id: &str,
    ) -> Self {
        let fork_branch_id = format!("fbk_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        Self {
            schema_version: 1,
            fork_branch_id,
            world_id: world_id.to_string(),
            parent_world_id: parent_world_id.to_string(),
            parent_branch_id: parent_branch_id.to_string(),
            forked_from_event_id: forked_from_event_id.to_string(),
            status: ForkBranchStatus::Active.as_str().to_string(),
            verification_status: VerificationStatus::Unverified.as_str().to_string(),
            created_by_creator_id: creator_id.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Request verification of this fork.
    pub fn request_verification(&mut self) -> Result<(), DomainError> {
        if self.status != ForkBranchStatus::Active.as_str() {
            return Err(DomainError::InvalidState {
                expected: "active".to_string(),
                actual: self.status.clone(),
            });
        }
        if self.verification_status != VerificationStatus::Unverified.as_str() {
            return Err(DomainError::InvalidTransition {
                from: self.verification_status.clone(),
                to: "requested".to_string(),
            });
        }
        self.verification_status = VerificationStatus::Requested.as_str().to_string();
        Ok(())
    }

    /// Verify this fork (admin/policy action).
    pub fn verify(&mut self) -> Result<(), DomainError> {
        if self.verification_status != VerificationStatus::Requested.as_str() {
            return Err(DomainError::InvalidTransition {
                from: self.verification_status.clone(),
                to: "verified".to_string(),
            });
        }
        self.verification_status = VerificationStatus::Verified.as_str().to_string();
        Ok(())
    }

    /// Reject this fork verification.
    pub fn reject(&mut self, _reason: &str) -> Result<(), DomainError> {
        if self.verification_status != VerificationStatus::Requested.as_str() {
            return Err(DomainError::InvalidTransition {
                from: self.verification_status.clone(),
                to: "rejected".to_string(),
            });
        }
        self.verification_status = VerificationStatus::Rejected.as_str().to_string();
        Ok(())
    }

    /// Archive this fork branch.
    pub fn archive(&mut self) -> Result<(), DomainError> {
        if self.status == ForkBranchStatus::Archived.as_str() {
            return Err(DomainError::AlreadyInState("archived".to_string()));
        }
        self.status = ForkBranchStatus::Archived.as_str().to_string();
        Ok(())
    }

    /// Validate that structured writes only go to child world/branch.
    /// Per consistency-rules-v1.md §3.4.
    pub fn validate_write_scope(&self, target_world_id: &str) -> Result<(), DomainError> {
        if self.status != ForkBranchStatus::Active.as_str() {
            return Err(DomainError::InvalidForkWriteScope(
                "fork branch is not active".to_string(),
            ));
        }
        if target_world_id != self.world_id {
            return Err(DomainError::InvalidForkWriteScope(format!(
                "writes must target child world ({}) not parent ({})",
                self.world_id, target_world_id
            )));
        }
        Ok(())
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::ForkBranch> for ForkBranch {
    fn from(c: nexus_contracts::ForkBranch) -> Self {
        Self {
            schema_version: c.schema_version,
            fork_branch_id: c.fork_branch_id,
            world_id: c.world_id,
            parent_world_id: c.parent_world_id,
            parent_branch_id: c.parent_branch_id,
            forked_from_event_id: c.forked_from_event_id,
            status: c.status.as_str().to_string(),
            verification_status: c.verification_status.as_str().to_string(),
            created_by_creator_id: c.created_by_creator_id,
            created_at: c.created_at,
        }
    }
}

impl From<ForkBranch> for nexus_contracts::ForkBranch {
    fn from(d: ForkBranch) -> Self {
        Self {
            schema_version: d.schema_version,
            fork_branch_id: d.fork_branch_id,
            world_id: d.world_id,
            parent_world_id: d.parent_world_id,
            parent_branch_id: d.parent_branch_id,
            forked_from_event_id: d.forked_from_event_id,
            status: nexus_contracts::ForkBranchStatus::from_str(&d.status).unwrap(),
            verification_status: nexus_contracts::VerificationStatus::from_str(
                &d.verification_status,
            )
            .unwrap(),
            created_by_creator_id: d.created_by_creator_id,
            created_at: d.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_fork() {
        let fb = ForkBranch::fork_from(
            "wld_child",
            "wld_parent",
            "fbk_root",
            "evt_123",
            "ctr_creator1",
        );
        assert_eq!(fb.status, "active");
        assert_eq!(fb.verification_status, "unverified");
        assert_eq!(fb.world_id, "wld_child");
        assert_eq!(fb.parent_world_id, "wld_parent");
        assert!(fb.fork_branch_id.starts_with("fbk_"));
    }

    #[test]
    fn test_request_verification() {
        let mut fb = ForkBranch::fork_from(
            "wld_child",
            "wld_parent",
            "fbk_root",
            "evt_123",
            "ctr_creator1",
        );
        fb.request_verification().unwrap();
        assert_eq!(fb.verification_status, "requested");
    }

    #[test]
    fn test_verify_fork() {
        let mut fb = ForkBranch::fork_from(
            "wld_child",
            "wld_parent",
            "fbk_root",
            "evt_123",
            "ctr_creator1",
        );
        fb.request_verification().unwrap();
        fb.verify().unwrap();
        assert_eq!(fb.verification_status, "verified");
    }

    #[test]
    fn test_reject_fork() {
        let mut fb = ForkBranch::fork_from(
            "wld_child",
            "wld_parent",
            "fbk_root",
            "evt_123",
            "ctr_creator1",
        );
        fb.request_verification().unwrap();
        fb.reject("quality too low").unwrap();
        assert_eq!(fb.verification_status, "rejected");
    }

    #[test]
    fn test_verify_without_request() {
        let mut fb = ForkBranch::fork_from(
            "wld_child",
            "wld_parent",
            "fbk_root",
            "evt_123",
            "ctr_creator1",
        );
        assert!(matches!(
            fb.verify(),
            Err(DomainError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn test_write_scope_validation_child() {
        let fb = ForkBranch::fork_from(
            "wld_child",
            "wld_parent",
            "fbk_root",
            "evt_123",
            "ctr_creator1",
        );
        assert!(fb.validate_write_scope("wld_child").is_ok());
    }

    #[test]
    fn test_write_scope_validation_parent() {
        let fb = ForkBranch::fork_from(
            "wld_child",
            "wld_parent",
            "fbk_root",
            "evt_123",
            "ctr_creator1",
        );
        assert!(matches!(
            fb.validate_write_scope("wld_parent"),
            Err(DomainError::InvalidForkWriteScope(_))
        ));
    }

    #[test]
    fn test_archive_fork() {
        let mut fb = ForkBranch::fork_from(
            "wld_child",
            "wld_parent",
            "fbk_root",
            "evt_123",
            "ctr_creator1",
        );
        fb.archive().unwrap();
        assert_eq!(fb.status, "archived");
    }

    #[test]
    fn test_archive_already_archived() {
        let mut fb = ForkBranch::fork_from(
            "wld_child",
            "wld_parent",
            "fbk_root",
            "evt_123",
            "ctr_creator1",
        );
        fb.archive().unwrap();
        assert!(matches!(fb.archive(), Err(DomainError::AlreadyInState(_))));
    }

    #[test]
    fn test_serialize_roundtrip() {
        let fb = ForkBranch::fork_from(
            "wld_child",
            "wld_parent",
            "fbk_root",
            "evt_123",
            "ctr_creator1",
        );
        let json = serde_json::to_string(&fb).unwrap();
        let deserialized: ForkBranch = serde_json::from_str(&json).unwrap();
        assert_eq!(fb, deserialized);
    }
}
