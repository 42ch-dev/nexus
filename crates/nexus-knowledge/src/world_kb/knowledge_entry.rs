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
    /// Dynamic runtime state for computable `KeyBlocks` (V1.61, compass Q4/Q5).
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

        // Gate 2: Version match — delegate the standard revision invariant to
        // spoke-operations (via the adapter). Map the reject back to nexus's
        // `RevisionMismatch` to preserve observable behavior.
        let current_rev = self.revision.unwrap_or(0);
        if matches!(
            assert_revision(base_revision, current_rev),
            SpokeResult::Reject(_)
        ) {
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

        // Transition: route the provisional→confirmed transition through
        // spoke-operations (via the adapter). `transition_status` enforces the
        // cross-product transition table (spec §1.5). The revision bump follows
        // the nexus convention (transition_status does not bump revision; that is
        // the promote-specific `apply_promote` path, not used here because the
        // spoke codegen inlines a distinct `PromoteRequest.candidate` type that
        // is not the data `KnowledgeEntry` — see T3 report).
        self.apply_spoke_status_transition(KeyBlockStatus::Confirmed.as_str())?;
        self.revision = Some(current_rev + 1);
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    /// Deprecate this `WorldKbEntry` (mark as superseded).
    pub fn deprecate(&mut self, _replacement_kb_id: Option<&str>) -> Result<(), KbError> {
        if self.status == KeyBlockStatus::Deprecated.as_str() {
            return Err(KbError::AlreadyInState("deprecated".to_string()));
        }
        self.apply_spoke_status_transition(KeyBlockStatus::Deprecated.as_str())
    }

    /// Merge this `WorldKbEntry` into another.
    pub fn merge_into(&mut self, _target_kb_id: &str) -> Result<(), KbError> {
        if self.status == KeyBlockStatus::Merged.as_str() {
            return Err(KbError::AlreadyInState("merged".to_string()));
        }
        self.apply_spoke_status_transition(KeyBlockStatus::Merged.as_str())
    }
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    /// Soft-delete this `WorldKbEntry`.
    pub fn delete(&mut self) -> Result<(), KbError> {
        if self.status == KeyBlockStatus::Deleted.as_str() {
            return Err(KbError::AlreadyInState("deleted".to_string()));
        }
        self.apply_spoke_status_transition(KeyBlockStatus::Deleted.as_str())
    }

    /// Route a status transition through spoke-operations (via the adapter).
    /// Converts `self` to the spoke type, delegates the transition validity
    /// check + canonical status to `transition_status`, then applies only the
    /// status back to `self` (body / identity are preserved on the domain type;
    /// the spoke round-trip drops body content per the T2 conversion seam, so we
    /// do not write the spoke result back wholesale). `updated_at` follows the
    /// nexus timestamp convention.
    fn apply_spoke_status_transition(&mut self, to: &str) -> Result<(), KbError> {
        let spoke: SpokeKnowledgeEntry = self.clone().into();
        let result = map_spoke_reject(transition_status(&spoke, to))?;
        self.status = result.status;
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
    pub fn set_body(&mut self, body: WorldKbBody) -> Result<(), KbError> {
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

// ── Wire boundary: WorldKbEntry ↔ spoke KnowledgeEntry (conversion seam) ─
//
// V1.139 P1 T2: the legacy `nexus_contracts::KeyBlock` contract type was
// deleted in P0 (`key-block.schema.json` removed). The spoke standard type
// `spoke_schemas::KnowledgeEntry` is now the wire boundary. `WorldKbEntry` is
// the nexus domain aggregate; these `From` impls are the **sole conversion
// seam** (spec `spoke-adapter-architecture.md` §7.1 — the adapter constructs
// the spoke type before calling spoke-operations).
//
// Body fidelity: spoke's `KnowledgeEntryBody` exposes only `computable`/`state`
// typed maps; nexus's `summary`/`attributes`/`tags` have no typed spoke slot
// today (typify drops `additionalProperties`). The forward direction maps what
// spoke carries (state); the rest is preserved on the nexus domain type and is
// NOT relied upon by spoke-operations. When spoke later declares body fields,
// extend ONLY these two impls — validation/consumers are unaffected.

use std::collections::HashMap;
use std::num::NonZeroU64;

use nexus_spoke_adapter::extensions::{
    get_created_from_command_id, get_provenance, get_world_id, set_created_from_command_id,
    set_provenance, set_world_id,
};
// V1.139 P1 T3 — lifecycle invariants are delegated to spoke-operations via the
// adapter (spec §1.5 / §7). nexus-knowledge never calls spoke-operations directly.
use nexus_spoke_adapter::ops::{assert_revision, transition_status};
use nexus_spoke_adapter::{SpokeReject, SpokeResult};
use serde_json::{Map, Value};
use spoke_schemas::knowledge_entry::{
    KnowledgeEntry as SpokeKnowledgeEntry, KnowledgeEntryBody as SpokeKnowledgeEntryBody,
    KnowledgeEntryCanonicalName, SourceAnchor as SpokeSourceAnchor,
};

/// Convert a nexus `BlockType` to spoke's open-string `entry_type`.
fn block_type_to_entry_type(bt: BlockType) -> String {
    serde_json::to_value(bt)
        .ok()
        .and_then(|v| v.as_str().map(std::string::ToString::to_string))
        .unwrap_or_else(|| "character".to_string())
}

/// Parse spoke `entry_type` back to nexus `BlockType` (unknown values → default).
fn entry_type_to_block_type(s: &str) -> BlockType {
    serde_json::from_value(Value::String(s.to_string())).unwrap_or_default()
}

impl From<WorldKbEntry> for SpokeKnowledgeEntry {
    fn from(d: WorldKbEntry) -> Self {
        // Body: carry only what spoke's typed body models (state). nexus
        // summary/attributes/tags stay on the domain type; spoke-operations
        // does not consume them. Spoke 0.2.0 declared `attributes`/`summary`/
        // `tags` as typed L2 body fields (closed `KnowledgeEntryBody`); they
        // are emitted empty here — the typed-body ↔ nexus-body alignment is
        // deferred to the next iteration per the V1.139 spoke-0.2.0 bump.
        let state_map = d
            .body
            .as_ref()
            .and_then(|b| b.state.clone())
            .and_then(|v| match v {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default();
        let spoke_body = SpokeKnowledgeEntryBody {
            attributes: Vec::new(),
            computable: Map::new(),
            state: state_map,
            summary: None,
            tags: Vec::new(),
        };

        let mut entry = Self {
            body: spoke_body,
            canonical_name: KnowledgeEntryCanonicalName::try_from(d.canonical_name.as_str())
                .expect("canonical_name is non-empty (validated)"),
            created_at: chrono::DateTime::parse_from_rfc3339(&d.created_at)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc)),
            entry_id: d.entry_id,
            entry_type: block_type_to_entry_type(d.block_type),
            extensions: HashMap::new(),
            revision: d.revision,
            schema_version: NonZeroU64::new(u64::from(d.schema_version))
                .expect("schema_version >= 1"),
            source_anchor: d.source_anchor.as_ref().map(nexus_anchor_to_spoke),
            status: d.status,
            updated_at: d.updated_at.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            }),
        };

        // Identity fields → extensions.nexus (via adapter accessors; preserves
        // unknown keys per spec §2.2).
        set_world_id(&mut entry, d.world_id);
        set_created_from_command_id(&mut entry, d.created_from_command_id);
        set_provenance(
            &mut entry,
            d.source_work_id,
            d.source_chapter,
            d.source_provenance_kind,
        );
        entry
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<SpokeKnowledgeEntry> for WorldKbEntry {
    fn from(s: SpokeKnowledgeEntry) -> Self {
        // Extract borrowed accessor data into owned values FIRST, so subsequent
        // field moves out of `s` are not blocked by outstanding borrows.
        let world_id = get_world_id(&s).unwrap_or_default().to_string();
        let created_from_command_id = get_created_from_command_id(&s).map(String::from);
        let (source_work_id, source_chapter, source_provenance_kind) = get_provenance(&s);
        let source_work_id = source_work_id.map(String::from);
        let source_provenance_kind = source_provenance_kind.map(String::from);
        let entry_type = s.entry_type.clone();
        let canonical_name = s.canonical_name.to_string();
        let schema_version = u32::try_from(s.schema_version.get()).unwrap_or(1);

        // Reverse body: spoke typed body → nexus state. summary/attributes/tags
        // cannot be recovered (forward direction dropped them); they default to None.
        let SpokeKnowledgeEntryBody {
            computable, state, ..
        } = s.body;
        let body = if state.is_empty() && computable.is_empty() {
            None
        } else {
            Some(WorldKbBody {
                summary: None,
                attributes: None,
                tags: None,
                state: if state.is_empty() {
                    None
                } else {
                    Some(Value::Object(state))
                },
                computable: None,
            })
        };

        Self {
            schema_version,
            entry_id: s.entry_id,
            world_id,
            block_type: entry_type_to_block_type(&entry_type),
            canonical_name,
            status: s.status,
            revision: s.revision,
            body,
            source_anchor: s.source_anchor.as_ref().map(spoke_anchor_to_nexus),
            created_from_command_id,
            created_at: s
                .created_at
                .map_or_else(|| chrono::Utc::now().to_rfc3339(), |dt| dt.to_rfc3339()),
            updated_at: s.updated_at.map(|dt| dt.to_rfc3339()),
            source_work_id,
            source_chapter,
            source_provenance_kind,
        }
    }
}

/// Forward: encode the nexus `SourceAnchor` as JSON inside the spoke anchor's
/// `source_id`. Round-trippable placeholder mapping — refines when the spoke
/// anchor model aligns with nexus provenance (T3).
fn nexus_anchor_to_spoke(a: &SourceAnchor) -> SpokeSourceAnchor {
    SpokeSourceAnchor {
        schema_version: NonZeroU64::new(1).expect("1 is non-zero"),
        source_id: serde_json::to_string(a).unwrap_or_else(|_| "{}".to_string()),
        extensions: HashMap::new(),
        label: None,
        mime_type: None,
        span: None,
    }
}

/// Reverse of [`nexus_anchor_to_spoke`].
fn spoke_anchor_to_nexus(a: &SpokeSourceAnchor) -> SourceAnchor {
    serde_json::from_str(&a.source_id).unwrap_or(SourceAnchor {
        story_summary_refs: None,
        excerpt: None,
        summary: None,
    })
}

/// Map a spoke-operations `SpokeResult` into a nexus `Result`, folding a
/// [`SpokeReject`] into `KbError::ValidationError` carrying the spoke code +
/// message. Used by the lifecycle routing (`confirm` / status transitions).
fn map_spoke_reject<T>(r: SpokeResult<T>) -> Result<T, KbError> {
    match r {
        SpokeResult::Ok(v) => Ok(v),
        SpokeResult::Reject(SpokeReject { code, message, .. }) => Err(KbError::ValidationError(
            format!("{}: {}", code.as_str(), message),
        )),
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
        let kb = WorldKbEntry::new("wld_test123", BlockType::Character, "Test Character");
        assert_eq!(kb.status, "provisional");
        assert_eq!(kb.revision, None);
        assert_eq!(kb.schema_version, 1);
        assert_eq!(kb.world_id, "wld_test123");
        assert!(kb.entry_id.starts_with("kb_"));
    }

    #[test]
    fn test_confirm_with_permission() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Hero");
        kb.confirm(&owner_membership(), 0, &no_conflicts(), &[])
            .unwrap();
        assert_eq!(kb.status, "confirmed");
        assert_eq!(kb.revision, Some(1));
    }

    #[test]
    fn test_confirm_without_permission() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Hero");
        let result = kb.confirm(&collaborator_membership(), 0, &no_conflicts(), &[]);
        assert!(matches!(result, Err(KbError::PermissionDenied(_))));
    }

    #[test]
    fn test_confirm_with_conflict() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Hero");
        let conflict = ConflictCheckResult::hard_conflict("conflicting KB entry");
        let result = kb.confirm(&owner_membership(), 0, &conflict, &[]);
        assert!(matches!(result, Err(KbError::UnresolvedConflict(_))));
    }

    #[test]
    fn test_confirm_with_revision_mismatch() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Event, "Battle");
        // kb.revision is None (i.e., 0 internally), but base_revision is 1
        let result = kb.confirm(&owner_membership(), 1, &no_conflicts(), &[]);
        assert!(matches!(result, Err(KbError::RevisionMismatch { .. })));
    }

    #[test]
    fn test_modify_confirmed_body_rejected() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Scene, "Forest");
        kb.confirm(&owner_membership(), 0, &no_conflicts(), &[])
            .unwrap();
        let result = kb.set_body(WorldKbBody {
            summary: Some("new summary".to_string()),
            attributes: None,
            tags: None,
            ..Default::default()
        });
        assert!(matches!(result, Err(KbError::ImmutableConfirmedState)));
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
    fn test_deprecate_keyblock() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Item, "Old Sword");
        kb.confirm(&owner_membership(), 0, &no_conflicts(), &[])
            .unwrap();
        kb.deprecate(Some("kb_new_sword")).unwrap();
        assert_eq!(kb.status, "deprecated");
    }

    #[test]
    fn test_merge_keyblock() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Hero v1");
        kb.confirm(&owner_membership(), 0, &no_conflicts(), &[])
            .unwrap();
        kb.merge_into("kb_hero_v2").unwrap();
        assert_eq!(kb.status, "merged");
    }

    #[test]
    fn test_delete_keyblock() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Temp");
        kb.delete().unwrap();
        assert_eq!(kb.status, "deleted");
    }

    #[test]
    fn test_is_confirmed() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Hero");
        assert!(!kb.is_confirmed());
        kb.confirm(&owner_membership(), 0, &no_conflicts(), &[])
            .unwrap();
        assert!(kb.is_confirmed());
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

    /// C-1: `confirm()` must enforce Gate 4 — `source_anchor` traceability.
    /// When `source_anchor` references a non-visible manifest, `confirm()` should fail.
    #[test]
    fn test_confirm_without_valid_source_anchor_fails() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Hero");
        // Set source_anchor pointing to a non-visible manifest
        let anchor = SourceAnchor::new("stm_hidden", "sum_1", None);
        kb.set_source_anchor(anchor).unwrap();

        // visible_manifests does NOT include stm_hidden → should fail Gate 4
        let visible_manifests: &[&str] = &["stm_visible1"];
        let result = kb.confirm(&owner_membership(), 0, &no_conflicts(), visible_manifests);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), KbError::ValidationError(_)));
    }

    /// C-1: `confirm()` succeeds when `source_anchor` references visible manifests.
    #[test]
    fn test_confirm_with_valid_source_anchor_succeeds() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Hero");
        let anchor = SourceAnchor::new("stm_visible1", "sum_1", None);
        kb.set_source_anchor(anchor).unwrap();

        let visible_manifests: &[&str] = &["stm_visible1"];
        let result = kb.confirm(&owner_membership(), 0, &no_conflicts(), visible_manifests);
        assert!(result.is_ok());
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
