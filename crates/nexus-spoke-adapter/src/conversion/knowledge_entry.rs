//! `WorldKbEntry ↔ spoke KnowledgeEntry` conversion seam + lifecycle delegation.
//!
//! This is the **sole conversion seam** between the nexus domain aggregate
//! [`WorldKbEntry`] and the spoke standard wire type [`SpokeKnowledgeEntry`]
//! (spec `spoke-adapter-architecture.md` §7.1), and the home of the nexus
//! lifecycle methods that delegate status-transition validity to
//! spoke-operations (spec §1.5 / §7).
//!
//! Both [`WorldKbEntry`] and [`SpokeKnowledgeEntry`] are foreign to this crate,
//! so the orphan rule forbids `impl From<...>` here. The seam is therefore
//! expressed as the free functions [`world_kb_to_spoke`] /
//! [`spoke_to_world_kb`], and the lifecycle delegation lives on the local
//! [`WorldKbEntrySpokeExt`] trait. See the parent [`conversion`](super) module
//! doc for the dependency-direction rationale.
//!
//! # Body alignment (spoke 0.4.0)
//!
//! spoke's closed `KnowledgeEntryBody` declares all 5 typed body fields
//! (`summary`, `tags`, `state`, `attributes`, `computable`). spoke's
//! `list_body_attributes` / `find_body_attribute` helpers are read accessors
//! over `body.attributes`, not shape mappers between spoke's
//! `Vec<BodyAttribute>` typed array and nexus's flat JSON `attributes`
//! representation; [`spoke_attrs_to_nexus`] / [`nexus_attr_to_spoke`] perform
//! that structural conversion and stay regardless of spoke version until nexus
//! adopts spoke's typed body shape natively (roadmap). Both directions map all
//! 5 fields (see field-level doc comments on the helpers).
//!
//! Unknown `extensions.nexus` keys ride as [`WorldKbEntry::extensions_nexus_extras`]
//! and are preserved verbatim across the seam (spec §2.2).

use std::collections::HashMap;
use std::num::NonZeroU64;

use nexus_contracts::{BlockType, KeyBlockStatus};
use nexus_knowledge::world_kb::errors::KbError;
use nexus_knowledge::world_kb::knowledge_entry::{
    ConflictCheckResult, MembershipPermissionCheck, WorldKbBody, WorldKbEntry,
};
use nexus_knowledge::world_kb::source_anchor::SourceAnchor;
use serde_json::{Map, Value};
use spoke_schemas::knowledge_entry::{
    KnowledgeEntry as SpokeKnowledgeEntry, KnowledgeEntryBody as SpokeKnowledgeEntryBody,
    KnowledgeEntryBodyAttributesItem, KnowledgeEntryBodyAttributesItemTraitType,
    KnowledgeEntryBodyAttributesItemValue, KnowledgeEntryCanonicalName,
    SourceAnchor as SpokeSourceAnchor,
};

use crate::extensions::{
    get_created_from_command_id, get_nexus_extras, get_provenance, get_world_id,
    set_created_from_command_id, set_nexus_extras, set_provenance, set_world_id, take_nexus_body,
};
// Test-only: simulates the persist-path carrier stash in `build_spoke_upsert_
// request` (production code sets the carrier in nexus-daemon-runtime, not here).
#[cfg(test)]
use crate::extensions::set_nexus_body;
use crate::ops::{assert_revision, transition_status};
use crate::{SpokeReject, SpokeResult};

// ── Free-function conversion seam (orphan-rule compliant) ────────────────

/// Forward: nexus domain [`WorldKbEntry`] → spoke standard
/// [`SpokeKnowledgeEntry`] (spec §7.1 sole conversion seam).
///
/// Borrows the domain entry (callers no longer need to `.clone()` the whole
/// struct before converting — the previous `From<WorldKbEntry>` consumed it).
/// Behavior is byte-identical to the former `From` impl; only the owned fields
/// are cloned internally.
///
/// # Panics
///
/// Panics if `canonical_name` fails the spoke newtype's regex validation or
/// `schema_version` is `0`. Both are nexus-validated invariants
/// (`validate_canonical_name` runs at construction; `schema_version` defaults
/// to `1` in [`WorldKbEntry::new`]), so a panic here indicates a wire-shape
/// drift, not a runtime input error.
#[must_use]
pub fn world_kb_to_spoke(entry: &WorldKbEntry) -> SpokeKnowledgeEntry {
    // Body: map all 5 typed body fields (spoke 0.4.0 closed body). Each nexus
    // field maps onto its spoke counterpart; computable bool→map and attributes
    // object→Vec<BodyAttribute> are shape conversions (see module doc).
    // spoke-operations does not consume body content, but the round-trip now
    // preserves it for any caller that reads the spoke type.
    let body = entry.body.as_ref();
    let state_map = body
        .and_then(|b| b.state.clone())
        .and_then(|v| match v {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default();
    // computable: nexus Option<bool> → spoke Map<String, Value>.
    // Some(true) → marker {"_computable": true}; None/Some(false) → empty.
    let computable_map = match body.and_then(|b| b.computable) {
        Some(true) => {
            let mut m = Map::new();
            m.insert("_computable".to_string(), Value::Bool(true));
            m
        }
        _ => Map::new(),
    };
    let summary = body.and_then(|b| b.summary.clone());
    let tags = body.and_then(|b| b.tags.clone()).unwrap_or_default();
    // attributes: nexus flat JSON object {trait_type: value, ...} → ERC721
    // Vec<BodyAttribute>. Null/array/object values have no spoke slot and are
    // dropped; the trait_type newtype is regex-validated by spoke.
    let attributes = body
        .and_then(|b| b.attributes.as_ref())
        .and_then(Value::as_object)
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| nexus_attr_to_spoke(k, v))
                .collect()
        })
        .unwrap_or_default();
    let spoke_body = SpokeKnowledgeEntryBody {
        attributes,
        computable: computable_map,
        state: state_map,
        summary,
        tags,
    };

    let mut spoke = SpokeKnowledgeEntry {
        body: spoke_body,
        canonical_name: KnowledgeEntryCanonicalName::try_from(entry.canonical_name.as_str())
            .expect("canonical_name is non-empty (validated)"),
        created_at: chrono::DateTime::parse_from_rfc3339(&entry.created_at)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        entry_id: entry.entry_id.clone(),
        entry_type: block_type_to_entry_type(entry.block_type),
        extensions: HashMap::new(),
        modules: HashMap::new(),
        revision: entry.revision,
        schema_version: NonZeroU64::new(u64::from(entry.schema_version))
            .expect("schema_version >= 1"),
        source_anchor: entry.source_anchor.as_ref().map(nexus_anchor_to_spoke),
        status: entry.status.clone(),
        updated_at: entry.updated_at.as_ref().and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        }),
    };

    // Identity fields → extensions.nexus (via adapter accessors; preserves
    // unknown keys per spec §2.2).
    set_world_id(&mut spoke, entry.world_id.clone());
    set_created_from_command_id(&mut spoke, entry.created_from_command_id.clone());
    set_provenance(
        &mut spoke,
        entry.source_work_id.clone(),
        entry.source_chapter,
        entry.source_provenance_kind.clone(),
    );
    // Unknown extensions.nexus keys → carried verbatim onto the spoke type.
    if let Some(Value::Object(extras)) = &entry.extensions_nexus_extras {
        set_nexus_extras(&mut spoke, extras);
    }
    spoke
}

/// Reverse: spoke standard [`SpokeKnowledgeEntry`] → nexus domain
/// [`WorldKbEntry`] (spec §7.1 sole conversion seam).
///
/// Consumes the spoke entry: the reverse conversion must [`take_nexus_body`]
/// (a `&mut` carrier extraction) and destructure the spoke body, so it cannot
/// borrow. Behavior is byte-identical to the former
/// `From<SpokeKnowledgeEntry> for WorldKbEntry` impl.
pub fn spoke_to_world_kb(entry: SpokeKnowledgeEntry) -> WorldKbEntry {
    let mut s = entry;
    // Extract the lossless body carrier FIRST — a reserved nexus key — so it
    // never leaks into product-local extras ([`get_nexus_extras`]) or the
    // persisted `extensions` column. Spoke's typed BodyAttributeValue drops
    // null/array/object attribute values; this carrier (stashed on the
    // persist-path upsert request in `build_spoke_upsert_request`) is the
    // authoritative, lossless nexus body that the persist writes (V1.143
    // Greptile P1 — body fidelity).
    let lossless_body_value = take_nexus_body(&mut s);
    // Extract borrowed accessor data into owned values FIRST, so subsequent
    // field moves out of `s` are not blocked by outstanding borrows.
    let world_id = get_world_id(&s).unwrap_or_default().to_string();
    let created_from_command_id = get_created_from_command_id(&s).map(String::from);
    let (source_work_id, source_chapter, source_provenance_kind) = get_provenance(&s);
    let source_work_id = source_work_id.map(String::from);
    let source_provenance_kind = source_provenance_kind.map(String::from);
    // Unknown extensions.nexus keys → carried verbatim onto the domain type
    // (owned Map; borrow ends before `s.body` is moved).
    let extensions_nexus_extras = get_nexus_extras(&s).map(Value::Object);
    let entry_type = s.entry_type.clone();
    let canonical_name = s.canonical_name.to_string();
    let schema_version = u32::try_from(s.schema_version.get()).unwrap_or(1);

    // Reverse body (fallback): spoke closed body → nexus body. All 5 fields
    // map back; body is `None` only when every field is empty/None. Used only
    // when no lossless carrier is present (e.g. spoke entries that did not
    // originate from a `WorldKbEntry` forward conversion).
    let SpokeKnowledgeEntryBody {
        attributes,
        computable,
        state,
        summary,
        tags,
    } = s.body;
    let has_computable = !computable.is_empty();
    let has_state = !state.is_empty();
    let has_tags = !tags.is_empty();
    let attributes_opt = spoke_attrs_to_nexus(&attributes);
    let spoke_derived_body = if summary.is_none()
        && !has_tags
        && !has_state
        && !has_computable
        && attributes_opt.is_none()
    {
        None
    } else {
        Some(WorldKbBody {
            summary,
            attributes: attributes_opt,
            tags: has_tags.then_some(tags),
            state: has_state.then_some(Value::Object(state)),
            computable: has_computable.then_some(true),
        })
    };
    // Prefer the lossless carrier; fall back to the spoke-typed-body
    // reconstruction when no carrier was stashed.
    let body = lossless_body_value
        .and_then(|v| serde_json::from_value::<WorldKbBody>(v).ok())
        .or(spoke_derived_body);

    WorldKbEntry {
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
        extensions_nexus_extras,
    }
}

// ── Private attribute / anchor shape helpers ─────────────────────────────

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

/// Forward attribute shape: nexus JSON object member → spoke `BodyAttribute`.
///
/// Maps `Value::String`/`Number`/`Bool` to the corresponding
/// `KnowledgeEntryBodyAttributesItemValue` variant; `Null`/array/object values
/// have no spoke slot and are dropped (returns `None`). `display_type` and
/// `max_value` have no nexus carrier and are left `None`. Returns `None` if the
/// `trait_type` fails the spoke newtype's regex validation.
fn nexus_attr_to_spoke(
    trait_type: &str,
    value: &Value,
) -> Option<KnowledgeEntryBodyAttributesItem> {
    let spoke_value = match value {
        Value::String(s) => KnowledgeEntryBodyAttributesItemValue::Variant0(s.clone()),
        Value::Number(n) => KnowledgeEntryBodyAttributesItemValue::Variant1(n.as_f64()?),
        Value::Bool(b) => KnowledgeEntryBodyAttributesItemValue::Variant2(*b),
        // Null / array / object — no spoke BodyAttributeValue slot.
        _ => return None,
    };
    let trait_type = KnowledgeEntryBodyAttributesItemTraitType::try_from(trait_type).ok()?;
    Some(KnowledgeEntryBodyAttributesItem {
        display_type: None,
        max_value: None,
        trait_type,
        value: spoke_value,
    })
}

/// Reverse attribute shape: spoke `Vec<BodyAttribute>` → nexus JSON object.
///
/// Each item's `trait_type` becomes a key; `value` maps back to the matching
/// JSON variant. `display_type`/`max_value` are dropped (no nexus slot). Returns
/// `None` for an empty slice. Duplicate `trait_type`s are last-wins (nexus's
/// object model has unique keys; spoke's Vec model permits duplicates).
fn spoke_attrs_to_nexus(attrs: &[KnowledgeEntryBodyAttributesItem]) -> Option<Value> {
    if attrs.is_empty() {
        return None;
    }
    let mut map = Map::new();
    for attr in attrs {
        let key = attr.trait_type.to_string();
        let val = match &attr.value {
            KnowledgeEntryBodyAttributesItemValue::Variant0(s) => Value::String(s.clone()),
            KnowledgeEntryBodyAttributesItemValue::Variant1(f) => {
                serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number)
            }
            KnowledgeEntryBodyAttributesItemValue::Variant2(b) => Value::Bool(*b),
        };
        map.insert(key, val);
    }
    (!map.is_empty()).then_some(Value::Object(map))
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

// ── Lifecycle delegation (local trait on a foreign type) ─────────────────

/// Nexus lifecycle methods on [`WorldKbEntry`] that delegate status-transition
/// validity to spoke-operations.
///
/// This trait lives in the spoke adapter (not `nexus-knowledge`) because every
/// method here routes through spoke-operations via the conversion seam —
/// housing it here is what removes the former `nexus-knowledge →
/// nexus-spoke-adapter` dependency edge (spec §8 dep-graph reversal). The
/// domain permission / conflict / traceability gates remain nexus logic; only
/// the transition cross-product + revision assertion are delegated to spoke
/// (spec §1.5 / §7).
///
/// Callers must `use nexus_spoke_adapter::conversion::WorldKbEntrySpokeExt;`
/// to invoke these methods (Rust method-resolution requires the trait in
/// scope).
pub trait WorldKbEntrySpokeExt {
    /// Transition provisional → confirmed.
    ///
    /// Gate requirements (consistency-rules-v1.md §3.2):
    /// 1. Initiator must have `can_confirm_canon` permission on the world
    /// 2. `base_revision` / revision must match server current (no version mismatch)
    /// 3. All required fields present and schema-valid
    /// 4. `source_anchor` must satisfy minimum traceability requirements
    /// 5. No unresolved hard conflicts
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if any gate fails.
    fn confirm(
        &mut self,
        membership: &MembershipPermissionCheck,
        base_revision: u64,
        conflict_check: &ConflictCheckResult,
        visible_manifests: &[&str],
    ) -> Result<(), KbError>;

    /// Deprecate this `WorldKbEntry` (mark as superseded).
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    fn deprecate(&mut self, replacement_kb_id: Option<&str>) -> Result<(), KbError>;

    /// Merge this `WorldKbEntry` into another.
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    fn merge_into(&mut self, target_kb_id: &str) -> Result<(), KbError>;

    /// Soft-delete this `WorldKbEntry`.
    ///
    /// # Errors
    /// Returns `Err(KbError::...)` if validation fails.
    fn delete(&mut self) -> Result<(), KbError>;
}

impl WorldKbEntrySpokeExt for WorldKbEntry {
    fn confirm(
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
        // spoke-operations. Map the reject back to nexus's `RevisionMismatch`
        // to preserve observable behavior.
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

        // Gate 4: Source anchor traceability (consistency-rules-v1.md §3.2).
        // When a source_anchor is present, all its story_summary_refs must
        // point to visible manifests in the same world.
        if let Some(ref anchor) = self.source_anchor {
            anchor
                .validate_refs(&self.world_id, visible_manifests)
                .map_err(|e| KbError::ValidationError(format!("{e}")))?;
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
        // spoke-operations. `transition_status` enforces the cross-product
        // transition table (spec §1.5). The revision bump follows the nexus
        // convention (transition_status does not bump revision; that is the
        // promote-specific `apply_promote` path, not used here because the
        // spoke codegen inlines a distinct `PromoteRequest.candidate` type
        // that is not the data `KnowledgeEntry` — see T3 report).
        apply_spoke_status_transition(self, KeyBlockStatus::Confirmed.as_str())?;
        self.revision = Some(current_rev + 1);
        Ok(())
    }

    fn deprecate(&mut self, _replacement_kb_id: Option<&str>) -> Result<(), KbError> {
        if self.status == KeyBlockStatus::Deprecated.as_str() {
            return Err(KbError::AlreadyInState("deprecated".to_string()));
        }
        apply_spoke_status_transition(self, KeyBlockStatus::Deprecated.as_str())
    }

    fn merge_into(&mut self, _target_kb_id: &str) -> Result<(), KbError> {
        if self.status == KeyBlockStatus::Merged.as_str() {
            return Err(KbError::AlreadyInState("merged".to_string()));
        }
        apply_spoke_status_transition(self, KeyBlockStatus::Merged.as_str())
    }

    fn delete(&mut self) -> Result<(), KbError> {
        if self.status == KeyBlockStatus::Deleted.as_str() {
            return Err(KbError::AlreadyInState("deleted".to_string()));
        }
        apply_spoke_status_transition(self, KeyBlockStatus::Deleted.as_str())
    }
}

/// Route a status transition through spoke-operations. Converts the entry to
/// the spoke type, delegates the transition validity check + canonical status
/// to `transition_status`, then applies only the status back (body / identity
/// are preserved on the domain type; the spoke round-trip is body-faithful as
/// of the V1.139 spoke-0.4.0 alignment, but we still do not write the spoke
/// result back wholesale — only the status field, to keep the nexus
/// timestamp/revision convention authoritative). `updated_at` follows the
/// nexus timestamp convention.
fn apply_spoke_status_transition(entry: &mut WorldKbEntry, to: &str) -> Result<(), KbError> {
    let spoke = world_kb_to_spoke(entry);
    let result = map_spoke_reject(transition_status(&spoke, to))?;
    entry.status = result.status;
    entry.updated_at = Some(chrono::Utc::now().to_rfc3339());
    Ok(())
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

    // ── Lifecycle routing (delegates to spoke-operations) ────────────────

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

    // ── V1.139 spoke 0.4.0 full body alignment (conversion seam round-trip) ─
    //
    // Proves the conversion seam (`world_kb_to_spoke` / `spoke_to_world_kb`)
    // round-trips ALL 5 typed body fields plus unknown `extensions.nexus`
    // extras in both directions. Each field that previously dropped
    // (summary / attributes / tags / computable) now survives.

    #[test]
    fn spoke_seam_roundtrips_all_five_body_fields() {
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Hero");
        kb.body = Some(WorldKbBody {
            summary: Some("Protagonist; reluctant cartographer.".to_string()),
            attributes: Some(serde_json::json!({
                "role": "protagonist",
                "age": 28,
                "is_alive": true,
            })),
            tags: Some(vec!["pov".to_string(), "combat".to_string()]),
            state: Some(serde_json::json!({"character": {"current_hp": 80}})),
            computable: Some(true),
        });

        // Forward → spoke, reverse → nexus.
        let spoke = world_kb_to_spoke(&kb);
        let roundtripped = spoke_to_world_kb(spoke);
        let body = roundtripped
            .body
            .as_ref()
            .expect("body survives the spoke round-trip");

        // summary: 1:1
        assert_eq!(
            body.summary.as_deref(),
            Some("Protagonist; reluctant cartographer.")
        );
        // tags: non-empty Vec → Some
        assert_eq!(
            body.tags.as_deref(),
            Some(["pov".to_string(), "combat".to_string()].as_slice())
        );
        // state: JSON object ↔ Map<String, Value>
        assert_eq!(body.state.as_ref().unwrap()["character"]["current_hp"], 80);
        // computable: Some(true) → marker map → Some(true)
        assert_eq!(body.computable, Some(true));
        // attributes: {k:v} → Vec<BodyAttribute> → {k:v}. Numbers are modeled
        // as f64 in spoke's BodyAttributeValue, so an integer nexus value
        // round-trips as a float (28 → 28.0); the numeric value is preserved.
        let attrs = body
            .attributes
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .expect("attributes round-trip to a JSON object");
        assert_eq!(attrs["role"], "protagonist");
        assert_eq!(attrs["age"].as_f64(), Some(28.0));
        assert_eq!(attrs["is_alive"], true);
    }

    #[test]
    fn spoke_seam_drops_null_array_object_attribute_values() {
        // nexus attributes is a free-form JSON object; spoke's BodyAttributeValue
        // only models string/number/boolean. Null/array/object members have no
        // spoke slot and are dropped on the general forward conversion
        // (documented loss of the spoke typed body). The persist path
        // (`build_spoke_upsert_request` → orchestrator → `put_update`) carries
        // the full body losslessly via the `_nexus_body` carrier instead — see
        // `spoke_seam_carrier_preserves_full_body_on_reverse`.
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Item, "Backpack");
        kb.body = Some(WorldKbBody {
            attributes: Some(serde_json::json!({
                "weight": 5,
                "named": null,
                "contents": ["sword", "potion"],
            })),
            ..Default::default()
        });
        let spoke = world_kb_to_spoke(&kb);
        let roundtripped = spoke_to_world_kb(spoke);
        let attrs = roundtripped
            .body
            .unwrap()
            .attributes
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        // General seam (no carrier): only the number survived; null/array dropped.
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs["weight"].as_f64(), Some(5.0));
    }

    #[test]
    fn spoke_seam_carrier_preserves_full_body_on_reverse() {
        // The persist path stashes the full nexus body in a reserved
        // `extensions.nexus._nexus_body` carrier (set in `build_spoke_upsert_
        // request`) so `put_update`'s reverse conversion recovers it losslessly
        // instead of the spoke-truncated body (V1.143 Greptile P1 — body
        // fidelity). This proves the recovery: a spoke entry carrying the full
        // body round-trips null/array/object attribute values that the spoke
        // typed body alone cannot represent.
        let full_body = WorldKbBody {
            summary: Some("Backpack summary".to_string()),
            attributes: Some(serde_json::json!({
                "weight": 5,
                "named": null,
                "contents": ["sword", "potion"],
                "metadata": {"rarity": "common"},
            })),
            tags: Some(vec!["gear".to_string()]),
            ..Default::default()
        };
        let mut spoke =
            world_kb_to_spoke(&WorldKbEntry::new("wld_test", BlockType::Item, "Backpack"));
        // Simulate the persist-path carrier stash (build_spoke_upsert_request).
        let body_value = serde_json::to_value(&full_body).unwrap_or_default();
        set_nexus_body(&mut spoke, Some(&body_value));

        let roundtripped = spoke_to_world_kb(spoke);
        let body = roundtripped.body.expect("carrier recovers the full body");
        assert_eq!(body.summary.as_deref(), Some("Backpack summary"));
        assert_eq!(body.tags.as_deref(), Some(["gear".to_string()].as_slice()));
        let attrs = body
            .attributes
            .as_ref()
            .and_then(Value::as_object)
            .expect("attributes");
        assert_eq!(attrs.len(), 4);
        assert_eq!(attrs["weight"].as_f64(), Some(5.0));
        assert_eq!(attrs["named"], Value::Null);
        assert_eq!(attrs["contents"], serde_json::json!(["sword", "potion"]));
        assert_eq!(attrs["metadata"], serde_json::json!({"rarity": "common"}));
    }

    #[test]
    fn spoke_seam_body_none_when_all_fields_empty() {
        // An entry with no body content round-trips to body = None.
        let kb = WorldKbEntry::new("wld_test", BlockType::Scene, "Empty");
        let spoke = world_kb_to_spoke(&kb);
        let roundtripped = spoke_to_world_kb(spoke);
        assert!(roundtripped.body.is_none(), "no body fields → body is None");
    }

    #[test]
    fn spoke_seam_summary_only_roundtrips_without_state_or_computable() {
        // Previously the reverse direction set body=None unless state/computable
        // were non-empty, dropping a summary-only body. Now summary alone survives.
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Scene, "Forest");
        kb.body = Some(WorldKbBody {
            summary: Some("A dark forest".to_string()),
            ..Default::default()
        });
        let spoke = world_kb_to_spoke(&kb);
        let roundtripped = spoke_to_world_kb(spoke);
        assert_eq!(
            roundtripped.body.unwrap().summary.as_deref(),
            Some("A dark forest")
        );
    }

    #[test]
    fn spoke_seam_extensions_nexus_extras_roundtrip() {
        // Unknown extensions.nexus keys (outside the 5 typed fields) ride on
        // WorldKbEntry.extensions_nexus_extras and survive the spoke seam both
        // ways (spec §2.2 round-trip rule 2).
        let mut kb = WorldKbEntry::new("wld_test", BlockType::Character, "Hero");
        kb.extensions_nexus_extras =
            Some(serde_json::json!({"custom_label": "villain-arc", "priority": 7}));

        let spoke = world_kb_to_spoke(&kb);
        // The unknown keys land under extensions.nexus on the spoke type.
        let roundtripped = spoke_to_world_kb(spoke);
        let extras = roundtripped
            .extensions_nexus_extras
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .expect("unknown extensions.nexus keys survive the seam");
        assert_eq!(extras["custom_label"], "villain-arc");
        assert_eq!(extras["priority"], 7);
    }
}
