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
    /// the spoke round-trip is body-faithful as of the V1.139 spoke-0.4.0
    /// alignment, but we still do not write the spoke result back wholesale —
    /// only the status field, to keep the nexus timestamp/revision convention
    /// authoritative). `updated_at` follows the nexus timestamp convention.
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
// Body alignment (spoke 0.4.0): spoke's closed `KnowledgeEntryBody` declares
// all 5 typed body fields (`summary`, `tags`, `state`, `attributes`,
// `computable`). spoke's `list_body_attributes` / `find_body_attribute`
// helpers are read accessors over `body.attributes`, not shape mappers between
// spoke's `Vec<BodyAttribute>` typed array and nexus's flat JSON `attributes`
// representation; the `spoke_attrs_to_nexus` / `nexus_attr_to_spoke` shims
// below perform that structural conversion and stay regardless of spoke
// version until nexus adopts spoke's typed body shape natively (roadmap).
// Both directions map all 5:
//   • `summary`  — 1:1 `Option<String>`.
//   • `tags`     — nexus `Option<Vec<String>>` ↔ spoke `Vec<String>` (empty ≡ None).
//   • `state`    — nexus JSON object ↔ spoke `Map<String, Value>` (1:1).
//   • `attributes` — nexus flat JSON object `{trait_type: value, ...}` ↔ spoke
//     `Vec<BodyAttribute>` (ERC721-style). Shape conversion is lossy for
//     `display_type`/`max_value` (no nexus slot) and duplicate `trait_type`
//     (nexus object is last-wins); the core `trait_type`+`value` round-trips.
//   • `computable` — nexus `Option<bool>` ↔ spoke marker `Map<String, Value>`
//     (`{"_computable": true}` when `Some(true)`, empty ≡ `None`/`Some(false)`).
// Unknown `extensions.nexus` keys ride as `WorldKbEntry::extensions_nexus_extras`
// and are preserved verbatim across the seam (spec §2.2).

use std::collections::HashMap;
use std::num::NonZeroU64;

use nexus_spoke_adapter::extensions::{
    get_created_from_command_id, get_nexus_extras, get_provenance, get_world_id,
    set_created_from_command_id, set_nexus_extras, set_provenance, set_world_id, take_nexus_body,
};
// Test-only: simulates the persist-path carrier stash in `build_spoke_upsert_
// request` (production code sets the carrier in nexus-daemon-runtime, not here).
#[cfg(test)]
use nexus_spoke_adapter::extensions::set_nexus_body;
// V1.139 P1 T3 — lifecycle invariants are delegated to spoke-operations via the
// adapter (spec §1.5 / §7). nexus-knowledge never calls spoke-operations directly.
use nexus_spoke_adapter::ops::{assert_revision, transition_status};
use nexus_spoke_adapter::{SpokeReject, SpokeResult};
use serde_json::{Map, Value};
use spoke_schemas::knowledge_entry::{
    KnowledgeEntry as SpokeKnowledgeEntry, KnowledgeEntryBody as SpokeKnowledgeEntryBody,
    KnowledgeEntryBodyAttributesItem, KnowledgeEntryBodyAttributesItemTraitType,
    KnowledgeEntryBodyAttributesItemValue, KnowledgeEntryCanonicalName,
    SourceAnchor as SpokeSourceAnchor,
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

impl From<WorldKbEntry> for SpokeKnowledgeEntry {
    fn from(d: WorldKbEntry) -> Self {
        // Body: map all 5 typed body fields (spoke 0.4.0 closed body). Each
        // nexus field maps onto its spoke counterpart; computable bool→map and
        // attributes object→Vec<BodyAttribute> are shape conversions (see seam
        // doc above). spoke-operations does not consume body content, but the
        // round-trip now preserves it for any caller that reads the spoke type.
        let body = d.body.as_ref();
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
        // Vec<BodyAttribute>. Null/array/object values have no spoke slot and
        // are dropped; the trait_type newtype is regex-validated by spoke.
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
        // Unknown extensions.nexus keys → carried verbatim onto the spoke type.
        if let Some(Value::Object(extras)) = &d.extensions_nexus_extras {
            set_nexus_extras(&mut entry, extras);
        }
        entry
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<SpokeKnowledgeEntry> for WorldKbEntry {
    fn from(mut s: SpokeKnowledgeEntry) -> Self {
        // Extract the lossless body carrier FIRST — a reserved nexus key — so
        // it never leaks into product-local extras ([`get_nexus_extras`]) or the
        // persisted `extensions` column. Spoke's typed BodyAttributeValue
        // drops null/array/object attribute values; this carrier (stashed on
        // the persist-path upsert request in `build_spoke_upsert_request`) is
        // the authoritative, lossless nexus body that the persist writes
        // (V1.143 Greptile P1 — body fidelity).
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
        // map back; body is `None` only when every field is empty/None. Used
        // only when no lossless carrier is present (e.g. spoke entries that did
        // not originate from a `WorldKbEntry` forward conversion).
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
            extensions_nexus_extras,
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

    // ── V1.139 spoke 0.4.0 full body alignment (Greptile P1+P2) ───────
    //
    // Proves the conversion seam (`WorldKbEntry ↔ spoke KnowledgeEntry`)
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
        let spoke: SpokeKnowledgeEntry = kb.into();
        let roundtripped: WorldKbEntry = spoke.into();
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
        let spoke: SpokeKnowledgeEntry = kb.into();
        let roundtripped: WorldKbEntry = spoke.into();
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
        let mut spoke: SpokeKnowledgeEntry =
            WorldKbEntry::new("wld_test", BlockType::Item, "Backpack").into();
        // Simulate the persist-path carrier stash (build_spoke_upsert_request).
        let body_value = serde_json::to_value(&full_body).unwrap_or_default();
        set_nexus_body(&mut spoke, Some(&body_value));

        let roundtripped: WorldKbEntry = spoke.into();
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
        let spoke: SpokeKnowledgeEntry = kb.into();
        let roundtripped: WorldKbEntry = spoke.into();
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
        let spoke: SpokeKnowledgeEntry = kb.into();
        let roundtripped: WorldKbEntry = spoke.into();
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

        let spoke: SpokeKnowledgeEntry = kb.into();
        // The unknown keys land under extensions.nexus on the spoke type.
        let roundtripped: WorldKbEntry = spoke.into();
        let extras = roundtripped
            .extensions_nexus_extras
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .expect("unknown extensions.nexus keys survive the seam");
        assert_eq!(extras["custom_label"], "villain-arc");
        assert_eq!(extras["priority"], 7);
    }
}
