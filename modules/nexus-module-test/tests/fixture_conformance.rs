//! Fixture conformance for the mini-host (V1.170 P0, AR-10).
//!
//! `fixtures/combat-input.json` is the canonical input fixture — SSOT copied
//! from `crates/nexus-wasm-host/tests/basic_combat.rs` (the
//! `tooling/check-module-sdk-drift.sh` gate asserts value-identity with the
//! test file's inline JSON). These tests prove the fixture parses into the
//! SDK envelope and runs through the mini-host's real calling convention.
//!
//! For full real-artifact conformance, set `NEXUS_MODULE_TEST_WASM` and
//! `NEXUS_MODULE_TEST_MANIFEST` to a compiled module pair (e.g. a
//! `basic-combat` build: the wasm artifact + its source `manifest.json`).
//! The test then asserts the complete 4-part output contract — the same
//! assertions the real-host `basic_combat.rs` test makes. The AR-12
//! `module-dx` CI leg runs exactly this against the freshly built module.

use nexus_module_sdk::{ComputeInput, DeltaOp, ModuleManifest};
use nexus_module_test::run;
use serde_json::json;

const FIXTURE: &str = include_str!("../fixtures/combat-input.json");

fn fixture_input() -> ComputeInput {
    serde_json::from_str(FIXTURE).expect("canonical fixture must parse as ComputeInput")
}

/// A minimal ABI-conformant module that echoes a fixed output envelope.
fn echo_module() -> Vec<u8> {
    let envelope = r#"{"schema_version":1,"state_delta":[],"timeline_events":[],"new_key_blocks":[],"battle_report":{"kind":"probe"}}"#;
    let escaped = envelope.replace('\\', "\\\\").replace('"', "\\\"");
    let wat = format!(
        r#"(module
  (memory (export "memory") 32)
  (data (i32.const 0x20000) "{escaped}")
  (global $heap (mut i32) (i32.const 1024))
  (func (export "alloc") (param $len i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (i32.add (local.get $len) (i32.const 7))))
    (local.get $ptr))
  (func (export "init"))
  (func (export "compute") (param $in_ptr i32) (param $in_len i32) (param $out_ptr i32) (param $out_cap i32) (result i64)
    (memory.copy (local.get $out_ptr) (i32.const 0x20000) (i32.const {len}))
    (i64.const {len})))
"#,
        len = envelope.len()
    );
    wat::parse_str(&wat).expect("wat parses")
}

fn probe_manifest(module_id: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.to_string(),
        name: "Probe".to_string(),
        version: "0.1.0".to_string(),
        nexus_abi_version: 1,
        required_key_block_types: vec![],
        compute_export: "compute".to_string(),
        init_export: "init".to_string(),
        description: None,
        author: None,
        host_functions: vec![],
        schemas: None,
        battle_report_kind: None,
        max_fuel: None,
        max_memory_mib: None,
        max_wall_time_ms: None,
        wasm_sha256: None,
    }
}

#[test]
fn fixture_parses_as_compute_input() {
    let input = fixture_input();
    assert_eq!(input.schema_version, 1);
    assert_eq!(input.world_ref.world_id.as_deref(), Some("wld_combat"));
    assert_eq!(input.world_ref.branch_id.as_deref(), Some("root"));
    assert_eq!(
        input.world_ref.timeline_head_event_id.as_deref(),
        Some("evt_0")
    );

    assert_eq!(input.key_blocks.len(), 2, "two computable characters");
    let atk = &input.key_blocks[0];
    let def = &input.key_blocks[1];
    assert_eq!(atk["key_block_id"], "kb_atk");
    assert_eq!(atk["block_type"], "character");
    assert_eq!(atk["body"]["attributes"]["base_atk"], 20, "attacker ATK 20");
    assert_eq!(def["key_block_id"], "kb_def");
    assert_eq!(def["body"]["attributes"]["base_def"], 5, "defender DEF 5");
    assert_eq!(
        def["body"]["state"]["character"]["current_hp"], 30,
        "defender HP 30"
    );

    assert_eq!(input.narrative_state["current_chapter"], "ch-1");
    assert_eq!(input.invocation["attacker_id"], "kb_atk");
    assert_eq!(input.invocation["defender_id"], "kb_def");
}

#[test]
fn fixture_runs_through_mini_host() {
    // The canonical fixture drives a full mini-host invocation: the input
    // envelope is serialized into module memory, `init` + `compute` run, and
    // the output envelope is parsed back into the SDK type.
    let wasm = echo_module();
    let output = run(&wasm, &probe_manifest("echo"), &fixture_input()).expect("run succeeds");
    assert_eq!(output.schema_version, 1);
    assert!(output.state_delta.is_empty());
    assert!(output.timeline_events.is_empty());
    assert!(output.new_key_blocks.is_empty());
    assert_eq!(output.battle_report["kind"], "probe");
}

#[test]
fn real_basic_combat_artifact_conformance() {
    // Full conformance against a real compiled module. Enabled via env vars
    // (the AR-12 module-dx CI leg); skipped with a visible note otherwise.
    let (Some(wasm_path), Some(manifest_path)) = (
        std::env::var("NEXUS_MODULE_TEST_WASM").ok(),
        std::env::var("NEXUS_MODULE_TEST_MANIFEST").ok(),
    ) else {
        eprintln!(
            "SKIP real-artifact conformance: set NEXUS_MODULE_TEST_WASM and \
             NEXUS_MODULE_TEST_MANIFEST to a compiled module pair"
        );
        return;
    };

    let wasm = std::fs::read(&wasm_path).expect("read NEXUS_MODULE_TEST_WASM");
    let manifest_json =
        std::fs::read_to_string(&manifest_path).expect("read NEXUS_MODULE_TEST_MANIFEST");
    let manifest: ModuleManifest =
        serde_json::from_str(&manifest_json).expect("manifest parses as ModuleManifest");

    let output = run(&wasm, &manifest, &fixture_input()).expect("real module runs");

    // The 4-part contract, mirrored from the real-host test
    // (crates/nexus-wasm-host/tests/basic_combat.rs).
    let delta = output
        .state_delta
        .iter()
        .find(|d| d.target_key_block_id.as_deref() == Some("kb_def"))
        .expect("delta targeting defender present");
    assert_eq!(delta.op, DeltaOp::Sub);
    assert_eq!(delta.path, "character.current_hp");
    assert_eq!(delta.value, Some(json!(15)));

    assert_eq!(output.timeline_events.len(), 1, "one state_update event");
    assert_eq!(output.timeline_events[0]["event_type"], "state_update");

    assert!(output.new_key_blocks.is_empty(), "no new key blocks");

    assert_eq!(output.battle_report["kind"], "combat");
}
