//! Integration test: load the embedded `basic-combat.wasm`, run one compute
//! invocation with two computable characters, and validate the 4-part
//! `ComputeOutput` envelope. This is the P2 acceptance test (compass Q9:
//! basic-combat's triple role — integration test, ABI validation, reference impl).

use nexus_wasm_host::{
    embedded_module_bytes, embedded_module_manifest, ComputeInput, ComputeOutput, ModuleManifest,
    WasmEngine,
};

/// Build a `ComputeInput` with two characters: attacker `kb-atk` (ATK 20) and
/// defender `kb-def` (DEF 5, current HP 30). Bodies follow the V1.61 KB
/// structured shape (compass Q4/Q5): `attributes` (immutable) + nested
/// `state.character.*` (mutable).
fn combat_input() -> ComputeInput {
    let raw = r#"{
        "schema_version": 1,
        "world_ref": {"world_id": "world-1", "branch_id": "root", "timeline_head_event_id": "ev-0"},
        "key_blocks": [
            {
                "schema_version": 1,
                "key_block_id": "kb-atk",
                "world_id": "world-1",
                "block_type": "character",
                "canonical_name": "Striker",
                "status": "confirmed",
                "body": {
                    "attributes": {"max_hp": 100, "base_atk": 20, "base_def": 3, "speed": 8},
                    "state": {"character": {"current_hp": 100, "is_alive": true}}
                },
                "created_at": "t"
            },
            {
                "schema_version": 1,
                "key_block_id": "kb-def",
                "world_id": "world-1",
                "block_type": "character",
                "canonical_name": "Guardian",
                "status": "confirmed",
                "body": {
                    "attributes": {"max_hp": 50, "base_atk": 10, "base_def": 5, "speed": 4},
                    "state": {"character": {"current_hp": 30, "is_alive": true}}
                },
                "created_at": "t"
            }
        ],
        "narrative_state": {"current_chapter": "ch-1"},
        "invocation": {"attacker_id": "kb-atk", "defender_id": "kb-def"}
    }"#;
    serde_json::from_str(raw).expect("valid ComputeInput")
}

#[test]
fn basic_combat_resolves_attack_into_four_part_output() {
    let engine = WasmEngine::new().expect("engine builds");

    let wasm = embedded_module_bytes("basic-combat").expect("basic-combat.wasm embedded");
    let module = engine.load_module(wasm).expect("module loads");
    let manifest: ModuleManifest =
        serde_json::from_str(embedded_module_manifest("basic-combat").unwrap())
            .expect("manifest parses");

    let output: ComputeOutput = engine
        .compute(&module, &manifest, &combat_input())
        .expect("compute succeeds");

    // 1) battle_report present with the combat discriminator.
    assert_eq!(output.battle_report["kind"], "combat");
    assert_eq!(output.battle_report["attacker_id"], "kb-atk");
    assert_eq!(output.battle_report["defender_id"], "kb-def");
    assert_eq!(output.battle_report["damage"], 15); // 20 ATK − 5 DEF
    assert_eq!(output.battle_report["defender_hp_before"], 30);
    assert_eq!(output.battle_report["defender_hp_after"], 15);

    // 2) state_delta: defender HP reduced by 15 at the nested state path.
    let delta = output
        .state_delta
        .iter()
        .find(|d| d.target_key_block_id.as_deref() == Some("kb-def"))
        .expect("delta targeting defender present");
    assert_eq!(delta.op, "sub");
    assert_eq!(delta.path, "character.current_hp");
    assert_eq!(delta.value, Some(serde_json::json!(15)));

    // 3) timeline_events: one state_update event recording the outcome.
    assert_eq!(output.timeline_events.len(), 1);
    let ev = &output.timeline_events[0];
    assert_eq!(
        ev.event_type,
        nexus_contracts::generated::common::common_types::TimelineEventType::StateUpdate
    );
    assert!(
        ev.summary
            .as_ref()
            .is_some_and(|s| s.contains("15") && s.contains("kb-def")),
        "event summary should mention damage and defender: {:?}",
        ev.summary
    );
    assert_eq!(
        ev.affected_key_block_ids.as_deref(),
        Some(&["kb-atk".to_string(), "kb-def".to_string()][..])
    );

    // 4) new_key_blocks empty for basic combat.
    assert!(output.new_key_blocks.is_empty());

    // The whole envelope must round-trip through serde (already did; sanity-check size).
    let json = serde_json::to_string(&output).unwrap();
    assert!(json.contains("state_delta"));
    assert!(json.contains("battle_report"));
}

/// A second invocation on a fresh instance is fully independent (compass Q6:
/// per-invocation sandbox). Two computes back-to-back produce identical,
/// reproducible results.
#[test]
fn compute_is_reproducible_across_invocations() {
    let engine = WasmEngine::new().unwrap();
    let wasm = embedded_module_bytes("basic-combat").unwrap();
    let module = engine.load_module(wasm).unwrap();
    let manifest: ModuleManifest =
        serde_json::from_str(embedded_module_manifest("basic-combat").unwrap()).unwrap();

    let a = engine.compute(&module, &manifest, &combat_input()).unwrap();
    let b = engine.compute(&module, &manifest, &combat_input()).unwrap();
    assert_eq!(a, b, "stateless compute must be reproducible");
}

/// A killing blow (damage >= current HP) drives HP to 0 and sets `is_alive=false`.
#[test]
fn killing_blow_marks_defender_not_alive() {
    let engine = WasmEngine::new().unwrap();
    let wasm = embedded_module_bytes("basic-combat").unwrap();
    let module = engine.load_module(wasm).unwrap();
    let manifest: ModuleManifest =
        serde_json::from_str(embedded_module_manifest("basic-combat").unwrap()).unwrap();

    let raw = r#"{
        "schema_version": 1,
        "world_ref": {"world_id": "w"},
        "key_blocks": [
            {"schema_version":1,"key_block_id":"a","world_id":"w","block_type":"character","canonical_name":"A","status":"confirmed","body":{"attributes":{"max_hp":100,"base_atk":50,"base_def":10},"state":{"character":{"current_hp":10,"is_alive":true}}},"created_at":"t"},
            {"schema_version":1,"key_block_id":"d","world_id":"w","block_type":"character","canonical_name":"D","status":"confirmed","body":{"attributes":{"max_hp":50,"base_atk":10,"base_def":2},"state":{"character":{"current_hp":10,"is_alive":true}}},"created_at":"t"}
        ],
        "invocation": {"attacker_id": "a", "defender_id": "d"}
    }"#;
    let input: ComputeInput = serde_json::from_str(raw).unwrap();
    let out = engine.compute(&module, &manifest, &input).unwrap();

    assert_eq!(out.battle_report["damage"], 48); // 50 − 2
    assert_eq!(out.battle_report["defender_hp_after"], 0);

    // Two deltas: sub HP to 0, then set is_alive=false.
    let alive_delta = out
        .state_delta
        .iter()
        .find(|d| d.path == "character.is_alive")
        .expect("is_alive delta emitted on kill");
    assert_eq!(alive_delta.op, "set");
    assert_eq!(alive_delta.value, Some(serde_json::json!(false)));
}
