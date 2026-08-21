//! Typed compute envelope skeleton (AR-3).
//!
//! Field-level split, verified against
//! `schemas/daemon-api/compute/compute-{input,output}.schema.json`: the
//! low-churn envelope fields are typed; the high-churn parts (spoke
//! `KnowledgeEntry` shapes, module-declared invocation/battle_report bodies)
//! pass through as `serde_json::Value`.
//!
//! No `#[serde(deny_unknown_fields)]` on any type: the additive versioning
//! policy (ABI §9.1) means the host may add envelope fields under ABI 1; the
//! SDK ignores unknowns. The mirror-gap drift check (AR-7) is what catches a
//! wire addition the SDK has not yet mirrored.

use serde::{Deserialize, Serialize};

/// World and timeline locator for one invocation.
///
/// Every field is optional on the wire: the compute-input schema declares
/// `world_ref` required but its inner properties are not, and the host's
/// generated `ComputeInputWorldRef` mirrors that with `Option<String>` fields
/// (the killing-blow fixture in `basic_combat.rs` sends `{"world_id": …}`
/// only). Absent fields deserialize to `None`; modules apply their own
/// fallbacks (e.g. `branch_id` → `"root"`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldRef {
    #[serde(default)]
    pub world_id: Option<String>,
    #[serde(default)]
    pub branch_id: Option<String>,
    #[serde(default)]
    pub timeline_head_event_id: Option<String>,
}

/// Standard input envelope passed into a WASM compute module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComputeInput {
    pub schema_version: u32,
    pub world_ref: WorldRef,
    /// Snapshot of KnowledgeEntry records relevant to this invocation
    /// (opaque objects — spoke `KnowledgeEntry` JSON, V1.139 fallback).
    pub key_blocks: Vec<serde_json::Value>,
    /// Narrative position context (module-declared shape). Optional on the
    /// wire; absent → `Null`.
    #[serde(default)]
    pub narrative_state: serde_json::Value,
    /// Module-defined input parameters (freeform object). Optional on the
    /// wire; absent → `Null`.
    #[serde(default)]
    pub invocation: serde_json::Value,
}

/// A single state delta operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateDeltaOp {
    pub op: DeltaOp,
    /// Dotted state path within the target KnowledgeEntry body
    /// (e.g. `character.current_hp`).
    pub path: String,
    /// KnowledgeEntry `entry_id` the delta applies to. Omitted → the host
    /// applies the delta to the entry implied by the capability context.
    pub target_key_block_id: Option<String>,
    /// Value for `set`, or numeric delta for `add`/`sub`.
    pub value: Option<serde_json::Value>,
}

/// Delta operation: add (increment numeric), sub (decrement numeric), set
/// (replace value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeltaOp {
    Add,
    Sub,
    Set,
}

/// Standard 4-part output envelope returned by a WASM compute module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComputeOutput {
    pub schema_version: u32,
    /// Ordered list of +/-/set state operations to apply.
    pub state_delta: Vec<StateDeltaOp>,
    /// Timeline events to append (opaque —
    /// `schemas/domain/timeline-event.schema.json`).
    pub timeline_events: Vec<serde_json::Value>,
    /// New KnowledgeEntry records the module creates (opaque objects).
    pub new_key_blocks: Vec<serde_json::Value>,
    /// Module-declared freeform report (kept open via
    /// `additionalProperties: true`).
    pub battle_report: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The canonical envelope sample (ABI §4, mirroring the compute-input
    /// schema contract) must deserialize with typed-field survival.
    #[test]
    fn compute_input_round_trip_preserves_typed_fields() {
        let raw = r#"{
            "schema_version": 1,
            "world_ref": {"world_id": "wld_combat", "branch_id": "root", "timeline_head_event_id": "evt_0"},
            "key_blocks": [
                {"key_block_id": "kb_atk", "block_type": "character", "body": {"attributes": {"base_atk": 20}}}
            ],
            "narrative_state": {"current_chapter": "ch-1"},
            "invocation": {"attacker_id": "kb_atk"}
        }"#;
        let input: ComputeInput = serde_json::from_str(raw).unwrap();
        assert_eq!(input.schema_version, 1);
        assert_eq!(input.world_ref.world_id.as_deref(), Some("wld_combat"));
        assert_eq!(input.world_ref.branch_id.as_deref(), Some("root"));
        assert_eq!(
            input.world_ref.timeline_head_event_id.as_deref(),
            Some("evt_0")
        );
        assert_eq!(input.key_blocks.len(), 1);
        assert_eq!(input.key_blocks[0]["key_block_id"], "kb_atk");
        assert_eq!(input.narrative_state["current_chapter"], "ch-1");
        assert_eq!(input.invocation["attacker_id"], "kb_atk");

        let reserialized = serde_json::to_value(&input).unwrap();
        assert_eq!(reserialized["schema_version"], 1);
        assert_eq!(reserialized["world_ref"]["world_id"], "wld_combat");
        assert_eq!(reserialized["key_blocks"][0]["key_block_id"], "kb_atk");
    }

    /// The additive versioning policy: unknown envelope fields are ignored,
    /// never rejected (ABI §9.1).
    #[test]
    fn compute_input_ignores_unknown_fields() {
        let raw = r#"{
            "schema_version": 1,
            "world_ref": {"world_id": "w", "branch_id": "root", "timeline_head_event_id": "evt_0"},
            "key_blocks": [],
            "future_field": {"anything": true}
        }"#;
        let input: ComputeInput = serde_json::from_str(raw).unwrap();
        assert_eq!(input.schema_version, 1);
    }

    /// `narrative_state` / `invocation` are optional on the wire (schema
    /// `required` lists only schema_version/world_ref/key_blocks).
    #[test]
    fn compute_input_optional_fields_default_to_null() {
        let raw = r#"{
            "schema_version": 1,
            "world_ref": {"world_id": "w", "branch_id": "root", "timeline_head_event_id": "evt_0"},
            "key_blocks": []
        }"#;
        let input: ComputeInput = serde_json::from_str(raw).unwrap();
        assert!(input.narrative_state.is_null());
        assert!(input.invocation.is_null());
    }

    /// `world_ref` inner fields are all optional on the wire (the schema
    /// requires `world_ref` itself, not its properties; the host's generated
    /// type mirrors that). The real-host killing-blow fixture sends only
    /// `world_id` — the SDK must accept it (AR-11 byte-compat bar).
    #[test]
    fn compute_input_world_ref_fields_are_optional() {
        let raw = r#"{
            "schema_version": 1,
            "world_ref": {"world_id": "wld_kill"},
            "key_blocks": []
        }"#;
        let input: ComputeInput = serde_json::from_str(raw).unwrap();
        assert_eq!(input.world_ref.world_id.as_deref(), Some("wld_kill"));
        assert_eq!(input.world_ref.branch_id, None);
        assert_eq!(input.world_ref.timeline_head_event_id, None);
    }

    /// The canonical output sample (ABI §5) must round-trip with the delta-op
    /// enum surviving serde.
    #[test]
    fn compute_output_round_trip_preserves_delta_ops() {
        let raw = r#"{
            "schema_version": 1,
            "state_delta": [
                {"op": "sub", "path": "character.current_hp", "target_key_block_id": "kb_def", "value": 15},
                {"op": "set", "path": "character.is_alive", "target_key_block_id": "kb_def", "value": false}
            ],
            "timeline_events": [{"event_type": "state_update", "title": "Combat resolved"}],
            "new_key_blocks": [],
            "battle_report": {"kind": "combat", "damage": 15}
        }"#;
        let output: ComputeOutput = serde_json::from_str(raw).unwrap();
        assert_eq!(output.schema_version, 1);
        assert_eq!(output.state_delta.len(), 2);
        assert_eq!(output.state_delta[0].op, DeltaOp::Sub);
        assert_eq!(output.state_delta[0].path, "character.current_hp");
        assert_eq!(
            output.state_delta[0].target_key_block_id.as_deref(),
            Some("kb_def")
        );
        assert_eq!(output.state_delta[0].value, Some(json!(15)));
        assert_eq!(output.state_delta[1].op, DeltaOp::Set);
        assert_eq!(output.timeline_events.len(), 1);
        assert_eq!(output.battle_report["kind"], "combat");

        let reserialized = serde_json::to_value(&output).unwrap();
        assert_eq!(reserialized["state_delta"][0]["op"], "sub");
        assert_eq!(reserialized["state_delta"][1]["op"], "set");
    }

    /// Delta op serde names are the wire values `add`/`sub`/`set`.
    #[test]
    fn delta_op_serde_names() {
        assert_eq!(serde_json::to_string(&DeltaOp::Add).unwrap(), "\"add\"");
        assert_eq!(serde_json::to_string(&DeltaOp::Sub).unwrap(), "\"sub\"");
        assert_eq!(serde_json::to_string(&DeltaOp::Set).unwrap(), "\"set\"");
        assert_eq!(
            serde_json::from_str::<DeltaOp>("\"add\"").unwrap(),
            DeltaOp::Add
        );
        assert_eq!(
            serde_json::from_str::<DeltaOp>("\"sub\"").unwrap(),
            DeltaOp::Sub
        );
        assert_eq!(
            serde_json::from_str::<DeltaOp>("\"set\"").unwrap(),
            DeltaOp::Set
        );
    }
}
