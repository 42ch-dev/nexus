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

    /// Typed parse of the `modules.mental` nine-field dialect from
    /// `modules_json` (V1.164 P2 T1, l5-mind).
    ///
    /// `None` when the module is absent or not a JSON object — malformed
    /// dialect data reads as absent, same consumer-only discipline as
    /// `modules.activation` (the authoritative shape stays `modules_json`
    /// JSON; the typed view is a read lens, not a second authority).
    #[must_use]
    pub fn parse_mental_fields(&self) -> Option<MentalFieldsRaw> {
        let modules = self.modules.as_ref()?;
        let mental = modules.get("mental")?;
        let obj = mental.as_object()?;
        serde_json::from_value(serde_json::Value::Object(obj.clone())).ok()
    }

    /// Typed parse of the `modules.belief` proposition array from
    /// `modules_json` (V1.164 P2 T1, l5-mind).
    ///
    /// One [`BeliefPropositionRaw`] per array element; rows that are not
    /// JSON objects are skipped. Empty when the module is absent or not an
    /// array. Handbook field names are locked (`holder` / `access` /
    /// `source`) — paper aliases (`actor` / `knowledge_access` /
    /// `mental_source`) are not fields of the parse type (PD-5 / TL-5).
    #[must_use]
    pub fn parse_belief_rows(&self) -> Vec<BeliefPropositionRaw> {
        let Some(rows) = self
            .modules
            .as_ref()
            .and_then(|m| m.get("belief"))
            .and_then(serde_json::Value::as_array)
        else {
            return Vec::new();
        };
        rows.iter()
            .filter_map(|row| serde_json::from_value(row.clone()).ok())
            .collect()
    }
}

// ── Mental-layer dialect raw types (V1.164 P2 T1, l5-mind) ────────────────
//
// Typed serde raw intermediates for the spoke `modules.mental` (nine-field)
// and `modules.belief` (proposition array) dialects, following the
// `ActivationConfigRaw` precedent (`nexus-spoke-adapter/src/adapter/
// activation.rs`): serde types over the handbook shape where the
// authoritative source is `modules_json` JSON — parse what the handbook
// names, leave the rest in the raw JSON. Handbook:
// spoke `domain-profile-mental-state.md`; field-vocabulary copy in
// `.mstar/iterations/v1.164/specs/v1.164-mental-layer-product-locks.md`.
//
// Handbook field names are LOCKED (TL-5 / PD-5): `holder` / `access` /
// `source` — never the OmniToM paper aliases `actor` / `knowledge_access` /
// `mental_source`. These types carry no fields for the aliases, so an
// emitted row never contains them and a paper-alias row does not round-trip
// through the typed form (AC-V1164-7). Unknown keys inside the dialect bag
// stay in `modules_json` and survive the adapter seam verbatim (PD-13) —
// the typed view is a handbook-names lens, not a lossless re-emitter.

/// Typed view of the `modules.mental` nine-field dialect on a holder
/// `KnowledgeEntry`.
///
/// All nine fields are optional — a holder carries the subset the author or
/// engine populated. Each field admits a scalar or a structured value
/// (handbook scalar-vs-nested guidance), so values stay raw JSON in this
/// intermediate type; the authoritative shape is `modules_json` JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MentalFieldsRaw {
    /// Mental identity (self-concept, role, occupation) — not the wire
    /// `entry_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<serde_json::Value>,
    /// Summary / count / `entry_id` refs — never a second copy of the
    /// labeled `modules.belief` array (PD-14).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beliefs: Option<serde_json::Value>,
    /// Current focus of perception or thought.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<serde_json::Value>,
    /// Desired end states.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goals: Option<serde_json::Value>,
    /// Planned courses of action toward goals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intentions: Option<serde_json::Value>,
    /// Affective state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emotions: Option<serde_json::Value>,
    /// Preferences, values, personality traits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispositions: Option<serde_json::Value>,
    /// Rules and customs the entity regards as binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub norms: Option<serde_json::Value>,
    /// Obligations and prohibitions on behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<serde_json::Value>,
}

/// One `modules.belief` proposition record (handbook field table).
///
/// Serves both narrated world facts (`holder: "world"`) and actor beliefs
/// (`holder: <entry_id>`); the `holder` field is the semantic discriminator.
/// All fields optional for forward-compat (raw intermediate — the
/// authoritative shape is `modules_json` JSON). Closed label spaces
/// (handbook exact) are not enforced here: adapters round-trip the record
/// verbatim and an unknown label is an emitter error, not an adapter error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BeliefPropositionRaw {
    /// Belief holder: an `entry_id`, a group id, or the special `world`
    /// marking a narrated fact. Not `actor` (TL-5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<String>,
    /// Minimal content being represented.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposition: Option<String>,
    /// Recursive belief depth: `0` = world-level narrated fact, `1` =
    /// first-order belief about the world, `2`/`3` = deeper nesting as flat
    /// rows (depth cap 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i64>,
    /// Truth Status: `True` / `False` / `Unknown`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truth: Option<String>,
    /// Knowledge Access: `Private` / `Shared` / `Public`. Not
    /// `knowledge_access` (TL-5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<String>,
    /// Representation: `Explicit` / `Implicit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub representation: Option<String>,
    /// Content Type (closed space, slash-containing labels are literal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Mental Source: `Narration` / `Perception` / `Memory` / `Testimony` /
    /// `Inference` / `Imagination` / `Unknown`. Not `mental_source` (TL-5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Context: `Deceptive` / `Temporal` / `Counterfactual` / `Neutral`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
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

    // ── Mental-layer dialect (V1.164 P2 T1, l5-mind) ─────────────────

    /// Fixture entry carrying the handbook `modules.mental` subset +
    /// `modules.belief` rows (worked-example box/basket story).
    fn mental_dialect_entry(entry_id: &str, block_type: BlockType) -> WorldKbEntry {
        let mut kb = WorldKbEntry::new("wld_test", block_type, "Dialect Holder");
        kb.entry_id = entry_id.to_string();
        kb.modules = Some(serde_json::json!({
            "mental": {
                "identity": { "role": "harbor_master" },
                "beliefs": { "ref": "kb_bo_beliefs", "count": 12 },
                "attention": { "target": "kb_tw_dawn_dock", "modality": "visual" },
                "goals": [{ "goal": "clear the dawn berths", "status": "active" }],
                "emotions": [{ "emotion": "alert", "intensity": 0.6 }],
                "norms": ["greet arriving captains"],
                "constraints": ["cannot waive dockside law"]
            },
            "belief": [
                {
                    "holder": "kb_bo",
                    "proposition": "The marble is in the box",
                    "order": 1,
                    "truth": "False",
                    "access": "Private",
                    "representation": "Implicit",
                    "content_type": "Location",
                    "source": "Perception",
                    "context": "Temporal"
                },
                {
                    "holder": "world",
                    "proposition": "The marble is in the basket",
                    "order": 0,
                    "truth": "True",
                    "access": "Public",
                    "representation": "Explicit",
                    "content_type": "Location",
                    "source": "Narration",
                    "context": "Temporal"
                }
            ]
        }));
        kb
    }

    #[test]
    fn test_parse_mental_fields_reads_nine_field_dialect() {
        let kb = mental_dialect_entry("kb_bo", BlockType::Character);
        let mental = kb
            .parse_mental_fields()
            .expect("modules.mental parses to MentalFieldsRaw");
        assert_eq!(
            mental.identity,
            Some(serde_json::json!({ "role": "harbor_master" }))
        );
        assert_eq!(
            mental.beliefs,
            Some(serde_json::json!({ "ref": "kb_bo_beliefs", "count": 12 }))
        );
        assert_eq!(
            mental.goals,
            Some(serde_json::json!([{ "goal": "clear the dawn berths", "status": "active" }]))
        );
        assert_eq!(
            mental.emotions,
            Some(serde_json::json!([{ "emotion": "alert", "intensity": 0.6 }]))
        );
        assert_eq!(mental.intentions, None);
        assert_eq!(mental.dispositions, None);
    }

    #[test]
    fn test_parse_belief_rows_reads_handbook_rows() {
        let kb = mental_dialect_entry("kb_bo", BlockType::Character);
        let rows = kb.parse_belief_rows();
        assert_eq!(rows.len(), 2);
        let actor = &rows[0];
        assert_eq!(actor.holder.as_deref(), Some("kb_bo"));
        assert_eq!(
            actor.proposition.as_deref(),
            Some("The marble is in the box")
        );
        assert_eq!(actor.order, Some(1));
        assert_eq!(actor.truth.as_deref(), Some("False"));
        assert_eq!(actor.access.as_deref(), Some("Private"));
        assert_eq!(actor.source.as_deref(), Some("Perception"));
        let world = &rows[1];
        assert_eq!(world.holder.as_deref(), Some("world"));
        assert_eq!(world.order, Some(0));
        assert_eq!(world.truth.as_deref(), Some("True"));
    }

    #[test]
    fn test_mental_absent_or_non_object_parses_as_none() {
        let bare = WorldKbEntry::new("wld_test", BlockType::Character, "Bare");
        assert!(bare.parse_mental_fields().is_none());
        assert!(bare.parse_belief_rows().is_empty());

        let mut scalar = WorldKbEntry::new("wld_test", BlockType::Character, "Scalar");
        scalar.modules = Some(serde_json::json!({ "mental": "not-an-object" }));
        assert!(scalar.parse_mental_fields().is_none());

        let mut not_array = WorldKbEntry::new("wld_test", BlockType::Character, "NotArray");
        not_array.modules = Some(serde_json::json!({ "belief": { "holder": "world" } }));
        assert!(not_array.parse_belief_rows().is_empty());
    }

    #[test]
    fn test_paper_alias_row_is_not_produced_and_does_not_round_trip() {
        // AC-V1164-7 / TL-5: a row using the OmniToM paper aliases
        // `actor` / `knowledge_access` / `mental_source` is not produced by
        // the parse type and does not round-trip as-is.
        let alias_row = serde_json::json!({
            "actor": "kb_bo",
            "proposition": "The marble is in the box",
            "order": 1,
            "knowledge_access": "Private",
            "mental_source": "Perception"
        });
        let parsed: BeliefPropositionRaw =
            serde_json::from_value(alias_row.clone()).expect("raw type ignores unknown keys");
        // Handbook fields are not populated from paper aliases.
        assert_eq!(parsed.holder, None);
        assert_eq!(parsed.access, None);
        assert_eq!(parsed.source, None);
        // Re-emission never contains the aliases and differs from the input
        // (the typed form is handbook-names only).
        let emitted = serde_json::to_value(&parsed).unwrap();
        assert_ne!(emitted, alias_row);
        assert!(emitted.get("actor").is_none());
        assert!(emitted.get("knowledge_access").is_none());
        assert!(emitted.get("mental_source").is_none());
    }
}
