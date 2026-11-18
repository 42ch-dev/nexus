//! ABI-conformance target for the packed `nexus-module-test` archive: runs a tiny ABI-valid module
//! through the mini-host and rejects a module whose `compute` writes an output envelope that is
//! not a `ComputeOutput`.
//!
//! This is a dev-dependency test target on purpose: `nexus-module-test` is declared under
//! `[dev-dependencies]`, so the mini-host resolving from its archive here is exactly the usage a
//! module crate gets.
//!
//! Honest boundary: the mini-host validates ABI conformance only. It enforces no fuel, memory or
//! wall-time budget, so nothing here is evidence about the production sandbox — those limits
//! belong to `nexus-wasm-host`.

use nexus_module_sdk::{ComputeInput, ModuleManifest, WorldRef};
use nexus_module_test::{run, MiniHostError};
use serde_json::json;

/// WAT convention shared with the crate's own conformance test: the envelope lives in a data
/// segment, `compute` copies it to the host's output buffer and returns its length.
fn module_writing(envelope: &str) -> Vec<u8> {
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

fn probe_input() -> ComputeInput {
    ComputeInput {
        schema_version: 1,
        world_ref: WorldRef {
            world_id: Some("wld_probe".to_string()),
            branch_id: Some("root".to_string()),
            timeline_head_event_id: None,
        },
        key_blocks: vec![json!({"key_block_id": "kb_probe", "block_type": "character"})],
        narrative_state: json!({}),
        invocation: json!({"probe": true}),
    }
}

#[test]
fn a_tiny_abi_module_runs_through_the_dev_dependency() {
    let manifest = probe_manifest("echo");
    manifest.validate().expect("the probe manifest validates");

    // A conformant module: the 4-part envelope comes back through the typed API.
    let conformant = module_writing(
        r#"{"schema_version":1,"state_delta":[],"timeline_events":[],"new_key_blocks":[],"battle_report":{"kind":"probe"}}"#,
    );
    let output = run(&conformant, &manifest, &probe_input()).expect("a conformant module runs");
    assert_eq!(output.schema_version, 1);
    assert!(output.state_delta.is_empty());
    assert!(output.timeline_events.is_empty());
    assert!(output.new_key_blocks.is_empty());
    assert_eq!(output.battle_report["kind"], "probe");

    println!(
        "nexus-module-test ran a tiny ABI-valid module and rejected a malformed output envelope"
    );
}

#[test]
fn a_malformed_output_envelope_is_rejected() {
    let manifest = probe_manifest("malformed");

    // A malformed envelope is rejected by the output-contract check, not by the guest: the module
    // compiles, runs, and returns bytes that are not a `ComputeOutput`.
    let malformed = module_writing(r#"{"schema_version":1,"state_delta":"not-a-list"}"#);
    let error = run(&malformed, &manifest, &probe_input())
        .expect_err("a malformed output envelope must be rejected");
    assert!(
        matches!(error, MiniHostError::InvalidOutput(_)),
        "expected InvalidOutput, got {error:?}"
    );
}
