//! `KeyBlock` aggregate — structured knowledge unit in a world timeline.
//!
//! `KeyBlock` is the primary knowledge container in Nexus. Each KB has a lifecycle
//! from provisional → confirmed (with possible deprecation/merge/deletion).
//! See data-model-v1.md §5.5, consistency-rules-v1.md §3.2.

use crate::errors::KbError;
use crate::source_anchor::SourceAnchor;
use nexus_contracts::BlockType;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// `KeyBlock` status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyBlockStatus {
    Provisional,
    Confirmed,
    Deprecated,
    Merged,
    Deleted,
}

impl KeyBlockStatus {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Provisional => "provisional",
            Self::Confirmed => "confirmed",
            Self::Deprecated => "deprecated",
            Self::Merged => "merged",
            Self::Deleted => "deleted",
        }
    }
}

/// `KeyBlock` body content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyBlockBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Result of a conflict check for confirm gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictCheckResult {
    pub has_hard_conflicts: bool,
    pub conflict_description: Option<String>,
}

impl ConflictCheckResult {
    #[must_use]
    pub const fn no_conflicts() -> Self {
        Self {
            has_hard_conflicts: false,
            conflict_description: None,
        }
    }

    pub fn hard_conflict(description: &str) -> Self {
        Self {
            has_hard_conflicts: true,
            conflict_description: Some(description.to_string()),
        }
    }
}

/// A simplified world membership reference for permission checks.
/// Full `WorldMembership` is in `world_membership` module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipPermissionCheck {
    pub can_confirm_canon: bool,
    pub can_sync_kb: bool,
}

/// `KeyBlock` aggregate — a structured knowledge unit in a world timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyBlock {
    pub schema_version: u32,
    pub key_block_id: String,
    pub world_id: String,
    pub block_type: BlockType,
    pub canonical_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<KeyBlockBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor: Option<SourceAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_from_command_id: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl KeyBlock {
    /// Create a new provisional `KeyBlock`.
    /// Precondition: caller must have `WorldMembership` with `can_sync_kb=true`.
    #[must_use]
    pub fn new(world_id: &str, block_type: BlockType, canonical_name: &str) -> Self {
        let key_block_id = format!("kb_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        Self {
            schema_version: 1,
            key_block_id,
            world_id: world_id.to_string(),
            block_type,
            canonical_name: canonical_name.to_string(),
            status: KeyBlockStatus::Provisional.as_str().to_string(),
            revision: None,
            body: None,
            source_anchor: None,
            created_from_command_id: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: None,
        }
    }

    /// Transition provisional → confirmed.
    ///
    /// Gate requirements (consistency-rules-v1.md §3.2):
    /// 1. Initiator must have `can_confirm_canon` permission on the world
    /// 2. `base_revision` / revision must match server current (no version mismatch)
    /// 3. All required fields present and schema-valid
    /// 4. `source_anchor` must satisfy minimum traceability requirements
    /// 5. No unresolved hard conflicts
    pub fn confirm(
        &mut self,
        membership: &MembershipPermissionCheck,
        base_revision: u64,
        conflict_check: &ConflictCheckResult,
        visible_manifests: &[&str],
    ) -> Result<(), KbError> {
        // Gate 1: Permission check
        if !membership.can_confirm_canon {
            return Err(KbError::PermissionDenied(
                "can_confirm_canon permission required".to_string(),
            ));
        }

        // Gate 2: Version match
        let current_rev = self.revision.unwrap_or(0);
        if current_rev != base_revision {
            return Err(KbError::RevisionMismatch {
                expected: base_revision,
                actual: current_rev,
            });
        }

        // Gate 3: Required fields present (canonical_name must be non-empty)
        if self.canonical_name.trim().is_empty() {
            return Err(KbError::ValidationError(
                "canonical_name is required".to_string(),
            ));
        }

        // Gate 4: Source anchor traceability (consistency-rules-v1.md §3.2)
        // When a source_anchor is present, all its story_summary_refs must
        // point to visible manifests in the same world.
        if let Some(ref anchor) = self.source_anchor {
            anchor
                .validate_refs(&self.world_id, visible_manifests)
                .map_err(|e| KbError::ValidationError(format!("{}", e)))?;
        }

        // Gate 5: No unresolved conflicts
        if conflict_check.has_hard_conflicts {
            return Err(KbError::UnresolvedConflict(
                conflict_check
                    .conflict_description
                    .clone()
                    .unwrap_or_else(|| "unresolved hard conflict".to_string()),
            ));
        }

        // Transition
        self.status = KeyBlockStatus::Confirmed.as_str().to_string();
        self.revision = Some(current_rev + 1);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    /// Deprecate this `KeyBlock` (mark as superseded).
    pub fn deprecate(&mut self, _replacement_kb_id: Option<&str>) -> Result<(), KbError> {
        if self.status == KeyBlockStatus::Deprecated.as_str() {
            return Err(KbError::AlreadyInState("deprecated".to_string()));
        }
        self.status = KeyBlockStatus::Deprecated.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }

    /// Merge this `KeyBlock` into another.
    pub fn merge_into(&mut self, _target_kb_id: &str) -> Result<(), KbError> {
        if self.status == KeyBlockStatus::Merged.as_str() {
            return Err(KbError::AlreadyInState("merged".to_string()));
        }
        self.status = KeyBlockStatus::Merged.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    /// Soft-delete this `KeyBlock`.
    pub fn delete(&mut self) -> Result<(), KbError> {
        if self.status == KeyBlockStatus::Deleted.as_str() {
            return Err(KbError::AlreadyInState("deleted".to_string()));
        }
        self.status = KeyBlockStatus::Deleted.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    /// Check if this KB is in confirmed state.
    #[must_use]
    pub fn is_confirmed(&self) -> bool {
        self.status == KeyBlockStatus::Confirmed.as_str()
    }

    /// Check if body modifications are allowed.
    /// Only provisional KBs allow body updates; confirmed KBs require fork/append.
    #[must_use]
    pub fn can_modify_body(&self) -> bool {
        self.status == KeyBlockStatus::Provisional.as_str()
    }

    /// Validate `source_anchor` traceability.
    /// Per G6: `source_anchor` must reference visible story manifests in same world.
    pub fn validate_source_anchor(
        &self,
        world_id: &str,
        visible_manifests: &[&str],
    ) -> Result<(), KbError> {
        if let Some(ref anchor) = self.source_anchor {
            // Check refs point to visible manifests in same world
            anchor.validate_refs(world_id, visible_manifests)?;
        }
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    /// Set body content (only allowed for provisional KBs).
    pub fn set_body(&mut self, body: KeyBlockBody) -> Result<(), KbError> {
        if !self.can_modify_body() {
            return Err(KbError::ImmutableConfirmedState);
        }
        self.body = Some(body);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    /// Set source anchor (only allowed for provisional KBs).
    pub fn set_source_anchor(&mut self, anchor: SourceAnchor) -> Result<(), KbError> {
        if !self.can_modify_body() {
            return Err(KbError::ImmutableConfirmedState);
        }
        self.source_anchor = Some(anchor);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::KeyBlock> for KeyBlock {
    fn from(c: nexus_contracts::KeyBlock) -> Self {
        Self {
            schema_version: c.schema_version,
            key_block_id: c.key_block_id,
            world_id: c.world_id,
            block_type: c.block_type,
            canonical_name: c.canonical_name,
            status: c.status.as_str().to_string(),
            revision: c.revision,
            body: c.body.map(|v| {
                serde_json::from_value(v).unwrap_or(KeyBlockBody {
                    summary: None,
                    attributes: None,
                    tags: None,
                })
            }),
            source_anchor: c.source_anchor.map(SourceAnchor::from),
            created_from_command_id: c.created_from_command_id,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<KeyBlock> for nexus_contracts::KeyBlock {
    fn from(d: KeyBlock) -> Self {
        Self {
            schema_version: d.schema_version,
            key_block_id: d.key_block_id,
            world_id: d.world_id,
            block_type: d.block_type,
            canonical_name: d.canonical_name,
            status: nexus_contracts::KeyBlockStatus::from_str(&d.status).unwrap(),
            revision: d.revision,
            body: d.body.map(|b| serde_json::to_value(b).unwrap_or_default()),
            source_anchor: d.source_anchor.map(nexus_contracts::SourceAnchor::from),
            created_from_command_id: d.created_from_command_id,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner_membership() -> MembershipPermissionCheck {
        MembershipPermissionCheck {
            can_confirm_canon: true,
            can_sync_kb: true,
        }
    }

    fn collaborator_membership() -> MembershipPermissionCheck {
        MembershipPermissionCheck {
            can_confirm_canon: false,
            can_sync_kb: true,
        }
    }

    fn no_conflicts() -> ConflictCheckResult {
        ConflictCheckResult::no_conflicts()
    }

    #[test]
    fn test_create_provisional_keyblock() {
        let kb = KeyBlock::new("wld_test123", BlockType::Character, "Test Character");
        assert_eq!(kb.status, "provisional");
        assert_eq!(kb.revision, None);
        assert_eq!(kb.schema_version, 1);
        assert_eq!(kb.world_id, "wld_test123");
        assert!(kb.key_block_id.starts_with("kb_"));
    }

    #[test]
    fn test_confirm_with_permission() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Character, "Hero");
        kb.confirm(&owner_membership(), 0, &no_conflicts(), &[])
            .unwrap();
        assert_eq!(kb.status, "confirmed");
        assert_eq!(kb.revision, Some(1));
    }

    #[test]
    fn test_confirm_without_permission() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Character, "Hero");
        let result = kb.confirm(&collaborator_membership(), 0, &no_conflicts(), &[]);
        assert!(matches!(result, Err(KbError::PermissionDenied(_))));
    }

    #[test]
    fn test_confirm_with_conflict() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Character, "Hero");
        let conflict = ConflictCheckResult::hard_conflict("conflicting KB entry");
        let result = kb.confirm(&owner_membership(), 0, &conflict, &[]);
        assert!(matches!(result, Err(KbError::UnresolvedConflict(_))));
    }

    #[test]
    fn test_confirm_with_revision_mismatch() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Event, "Battle");
        // kb.revision is None (i.e., 0 internally), but base_revision is 1
        let result = kb.confirm(&owner_membership(), 1, &no_conflicts(), &[]);
        assert!(matches!(result, Err(KbError::RevisionMismatch { .. })));
    }

    #[test]
    fn test_modify_confirmed_body_rejected() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Scene, "Forest");
        kb.confirm(&owner_membership(), 0, &no_conflicts(), &[])
            .unwrap();
        let result = kb.set_body(KeyBlockBody {
            summary: Some("new summary".to_string()),
            attributes: None,
            tags: None,
        });
        assert!(matches!(result, Err(KbError::ImmutableConfirmedState)));
    }

    #[test]
    fn test_modify_provisional_body_allowed() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Scene, "Forest");
        kb.set_body(KeyBlockBody {
            summary: Some("A dark forest".to_string()),
            attributes: None,
            tags: Some(vec!["location".to_string()]),
        })
        .unwrap();
        assert!(kb.body.is_some());
        assert_eq!(
            kb.body.as_ref().unwrap().summary.as_deref(),
            Some("A dark forest")
        );
    }

    #[test]
    fn test_all_block_types_serialize() {
        let types = vec![
            BlockType::Character,
            BlockType::Ability,
            BlockType::Scene,
            BlockType::Organization,
            BlockType::Item,
            BlockType::Conflict,
            BlockType::InfoPoint,
            BlockType::Event,
        ];

        for bt in &types {
            let kb = KeyBlock::new("wld_test", *bt, "Test");
            let json = serde_json::to_string(&kb).unwrap();
            let deserialized: KeyBlock = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.block_type, *bt);
        }
    }

    #[test]
    fn test_deprecate_keyblock() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Item, "Old Sword");
        kb.confirm(&owner_membership(), 0, &no_conflicts(), &[])
            .unwrap();
        kb.deprecate(Some("kb_new_sword")).unwrap();
        assert_eq!(kb.status, "deprecated");
    }

    #[test]
    fn test_merge_keyblock() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Character, "Hero v1");
        kb.confirm(&owner_membership(), 0, &no_conflicts(), &[])
            .unwrap();
        kb.merge_into("kb_hero_v2").unwrap();
        assert_eq!(kb.status, "merged");
    }

    #[test]
    fn test_delete_keyblock() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Character, "Temp");
        kb.delete().unwrap();
        assert_eq!(kb.status, "deleted");
    }

    #[test]
    fn test_is_confirmed() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Character, "Hero");
        assert!(!kb.is_confirmed());
        kb.confirm(&owner_membership(), 0, &no_conflicts(), &[])
            .unwrap();
        assert!(kb.is_confirmed());
    }

    #[test]
    fn test_source_anchor_traceability() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Character, "Hero");
        let anchor = SourceAnchor::new("stm_visible1", "sum_1", Some("chapter_summary"));
        kb.set_source_anchor(anchor).unwrap();
        assert!(kb
            .validate_source_anchor("wld_test", &["stm_visible1"])
            .is_ok());
    }

    #[test]
    fn test_source_anchor_invalid_ref() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Character, "Hero");
        let anchor = SourceAnchor::new("stm_hidden", "sum_1", None);
        kb.set_source_anchor(anchor).unwrap();
        assert!(kb
            .validate_source_anchor("wld_test", &["stm_visible1"])
            .is_err());
    }

    /// C-1: confirm() must enforce Gate 4 — source_anchor traceability.
    /// When source_anchor references a non-visible manifest, confirm() should fail.
    #[test]
    fn test_confirm_without_valid_source_anchor_fails() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Character, "Hero");
        // Set source_anchor pointing to a non-visible manifest
        let anchor = SourceAnchor::new("stm_hidden", "sum_1", None);
        kb.set_source_anchor(anchor).unwrap();

        // visible_manifests does NOT include stm_hidden → should fail Gate 4
        let visible_manifests: &[&str] = &["stm_visible1"];
        let result = kb.confirm(&owner_membership(), 0, &no_conflicts(), visible_manifests);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), KbError::ValidationError(_)));
    }

    /// C-1: confirm() succeeds when source_anchor references visible manifests.
    #[test]
    fn test_confirm_with_valid_source_anchor_succeeds() {
        let mut kb = KeyBlock::new("wld_test", BlockType::Character, "Hero");
        let anchor = SourceAnchor::new("stm_visible1", "sum_1", None);
        kb.set_source_anchor(anchor).unwrap();

        let visible_manifests: &[&str] = &["stm_visible1"];
        let result = kb.confirm(&owner_membership(), 0, &no_conflicts(), visible_manifests);
        assert!(result.is_ok());
    }
}
