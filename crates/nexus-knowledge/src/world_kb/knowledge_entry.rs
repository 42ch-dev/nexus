//! `WorldKbEntry` aggregate — structured knowledge unit in a world timeline.
//!
//! `WorldKbEntry` is the primary knowledge container in Nexus. Each KB has a lifecycle
//! from provisional → confirmed (with possible deprecation/merge/deletion).
//! See data-model-v1.md §5.5, consistency-rules-v1.md §3.2.

use crate::world_kb::errors::KbError;
use crate::world_kb::source_anchor::SourceAnchor;
use nexus_contracts::BlockType;
use nexus_contracts::KeyBlockStatus;
use serde::{Deserialize, Serialize};

/// `WorldKbEntry` body content.
///
/// # V1.61 Structured Compute Layer
///
/// The [`state`] and [`computable`] fields are additive (optional) for the WASM
/// compute pipeline (compass Q4). Existing `WorldKbEntry`s without them remain valid.
///
/// ## Computable `BlockType` set
///
/// The canonical set of `BlockType`s that participate in compute is:
/// `Character`, `Item`, `Faction`, `Ability`, `Species`. Per-module
/// attribute/state shapes are declared by each compute module's
/// `manifest.json` `schemas` block (V1.62 — see `modules/README.md` and
/// `.mstar/specs/compute-module-abi.md`). `environment` is NOT
/// a valid `BlockType` enum variant and is not included.
///
/// When [`computable`](Self::computable) is `Some(true)`, the body SHOULD
/// carry static `attributes` (immutable compute params) and MAY carry
/// dynamic `state` (mutable runtime data, nested by `block_type` per
/// compass Q5: `state.character.current_hp`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorldKbBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Dynamic runtime state for computable `KnowledgeEntries` (V1.61, compass Q4/Q5).
    /// Nested by `block_type` to avoid field-name collisions across module
    /// types (e.g. `state.character.current_hp`). Per-module state shapes
    /// are declared in each compute module's `manifest.json` (V1.62).
    /// Only meaningful when [`computable`](Self::computable) is `Some(true)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<serde_json::Value>,
    /// Marks this `WorldKbEntry` as participating in WASM compute (V1.61, compass Q4).
    /// When `Some(true)`, `state` holds mutable runtime state and `attributes`
    /// hold immutable compute params. Stored inside `body_json` (no DB column)
    /// for additive, migration-free rollout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computable: Option<bool>,
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

/// `WorldKbEntry` aggregate — a structured knowledge unit in a world timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldKbEntry {
    pub schema_version: u32,
    pub entry_id: String,
    pub world_id: String,
    pub block_type: BlockType,
    pub canonical_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<WorldKbBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor: Option<SourceAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_from_command_id: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    // V1.52 T-A P2: Work→WorldKbEntry provenance (entity-scope-model.md §5.5.7)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_work_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_chapter: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_provenance_kind: Option<String>,
    /// Unknown keys carried under `extensions.nexus` on the spoke boundary —
    /// everything outside the 5 typed identity fields (`world_id`,
    /// `created_from_command_id`, `source_work_id`, `source_chapter`,
    /// `source_provenance_kind`). Preserved verbatim across the `SQLite`
    /// read-modify-write cycle and the spoke conversion seam (spec §2.2
    /// round-trip rule 2). `None` when no unknown keys are present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions_nexus_extras: Option<serde_json::Value>,
    /// Per-entry functional-dialect modules (`modules.*`).
    ///
    /// Carried as a JSON object (`{"activation": {...}, "pack": {...}}`).
    /// Preserved verbatim across the `SQLite` read-modify-write cycle and the
    /// spoke conversion seam. `None` when no modules data is present.
    /// V1.146 P4 T1 — additive field; legacy entries have `modules = None`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub modules: Option<serde_json::Value>,
}

impl WorldKbEntry {
    /// Create a new provisional `WorldKbEntry`.
    /// Precondition: caller must have `WorldMembership` with `can_sync_kb=true`.
    #[must_use]
    pub fn new(world_id: &str, block_type: BlockType, canonical_name: &str) -> Self {
        let entry_id = format!("kb_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        Self {
            schema_version: 1,
            entry_id,
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
            source_work_id: None,
            source_chapter: None,
            source_provenance_kind: None,
            extensions_nexus_extras: None,
            modules: None,
        }
    }

    // NOTE (V1.145 P1a): the lifecycle methods that delegate status transitions
    // to spoke-operations (`confirm` / `deprecate` / `merge_into` / `delete`)
    // moved to `nexus_spoke_adapter::conversion::WorldKbEntrySpokeExt`. They
    // could not stay here because they call spoke-operations through the
    // adapter, and keeping them would preserve the `nexus-knowledge →
    // nexus-spoke-adapter` dependency edge (spec §8 dep-graph reversal). The
    // `WorldKbEntry ↔ spoke KnowledgeEntry` conversion seam likewise moved to
    // `nexus_spoke_adapter::conversion` (free functions `world_kb_to_spoke` /
    // `spoke_to_world_kb` — orphan rule forbids the `From` impls here). This
    // crate now owns only the pure domain aggregate + its non-spoke methods.

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

    /// Set body content (only allowed for provisional KBs).
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    pub fn set_body(&mut self, body: WorldKbBody) -> Result<(), KbError> {
        if !self.can_modify_body() {
            return Err(KbError::ImmutableConfirmedState);
        }
        self.body = Some(body);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }

    /// Set source anchor (only allowed for provisional KBs).
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    pub fn set_source_anchor(&mut self, anchor: SourceAnchor) -> Result<(), KbError> {
        if !self.can_modify_body() {
            return Err(KbError::ImmutableConfirmedState);
        }
        self.source_anchor = Some(anchor);
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_provisional_keyblock() {
        let kb = WorldKbEntry::new("wld_test123", BlockType::Character, "Test Character");
        assert_eq!(kb.status, "provisional");
        assert_eq!(kb.revision, None);
        assert_eq!(kb.schema_version, 1);
        assert_eq!(kb.world_id, "wld_test123");
        assert!(kb.entry_id.starts_with("kb_"));
    }

    #[test]
    fn test_modify_provisional_body_allowed() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Scene, "Forest");
        kb.set_body(WorldKbBody {
            summary: Some("A dark forest".to_string()),
            attributes: None,
            tags: Some(vec!["location".to_string()]),
            ..Default::default()
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
            let kb = WorldKbEntry::new("wld_test", *bt, "Test");
            let json = serde_json::to_string(&kb).unwrap();
            let deserialized: WorldKbEntry = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.block_type, *bt);
        }
    }

    #[test]
    fn test_source_anchor_traceability() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Hero");
        let anchor = SourceAnchor::new("stm_visible1", "sum_1", Some("chapter_summary"));
        kb.set_source_anchor(anchor).unwrap();
        assert!(kb
            .validate_source_anchor("wld_test", &["stm_visible1"])
            .is_ok());
    }

    #[test]
    fn test_source_anchor_invalid_ref() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Hero");
        let anchor = SourceAnchor::new("stm_hidden", "sum_1", None);
        kb.set_source_anchor(anchor).unwrap();
        assert!(kb
            .validate_source_anchor("wld_test", &["stm_visible1"])
            .is_err());
    }

    // ── State roundtrip (V1.61 P1) ─────────────────────────────────

    #[test]
    fn test_state_roundtrip_serialize_deserialize_preserves_state() {
        let body = WorldKbBody {
            summary: Some("Hero character".to_string()),
            attributes: Some(serde_json::json!({"max_hp": 100, "base_atk": 30})),
            tags: Some(vec!["combat".to_string()]),
            computable: Some(true),
            state: Some(serde_json::json!({
                "character": {
                    "current_hp": 80,
                    "status_effects": ["poisoned"],
                    "position": "front_line",
                    "is_alive": true
                }
            })),
        };

        let json = serde_json::to_string(&body).unwrap();
        let deserialized: WorldKbBody = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.computable, Some(true));
        assert_eq!(
            deserialized.state.as_ref().unwrap()["character"]["current_hp"],
            80
        );
        assert_eq!(
            deserialized.state.as_ref().unwrap()["character"]["status_effects"][0],
            "poisoned"
        );
    }

    #[test]
    fn test_state_roundtrip_without_state_and_computable() {
        // Legacy WorldKbEntry without state/computable should roundtrip correctly
        let body = WorldKbBody {
            summary: Some("Old block".to_string()),
            attributes: None,
            tags: None,
            ..Default::default()
        };

        let json = serde_json::to_string(&body).unwrap();
        let deserialized: WorldKbBody = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.summary.as_deref(), Some("Old block"));
        assert_eq!(deserialized.state, None);
        assert_eq!(deserialized.computable, None);
    }

    #[test]
    fn test_state_roundtrip_empty_state_object() {
        let body = WorldKbBody {
            summary: Some("minimal".to_string()),
            attributes: Some(serde_json::json!({"max_hp": 50})),
            tags: None,
            computable: Some(true),
            state: Some(serde_json::json!({"character": {}})),
        };

        let json = serde_json::to_string(&body).unwrap();
        let deserialized: WorldKbBody = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.computable, Some(true));
        assert!(deserialized
            .state
            .as_ref()
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("character"));
    }
}
