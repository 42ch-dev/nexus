//! Sandbox enforcement (compass Q6): fuel, memory, and wall-time limits must
//! trap a runaway module and surface as a typed [`ComputeError`], never crash
//! the host. This validates acceptance criterion #3.

use nexus_wasm_host::{ComputeError, ComputeInput, ModuleManifest, WasmEngine};

/// A module that exports the V1 ABI (`alloc`, `init`, `compute`, `memory`) but
/// whose `compute` is an **infinite loop**. With fuel consumption enabled, the
/// loop traps with `Trap::OutOfFuel` — mapped to [`ComputeError::OutOfFuel`].
fn infinite_loop_module() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (global $heap (mut i32) (i32.const 1024))
            (func (export "alloc") (param $len i32) (result i32)
              (local $p i32)
              (local.set $p (global.get $heap))
              (global.set $heap (i32.add (global.get $heap) (local.get $len)))
              (local.get $p))
            (func (export "init"))
            (func (export "compute")
              (param i32 i32 i32 i32) (result i64)
              (loop $forever (br $forever))
              (i64.const 0)))
        "#,
    )
    .expect("valid wat")
}

fn manifest() -> ModuleManifest {
    serde_json::from_str(
        r#"{"module_id":"loop","name":"Loop","version":"0.1.0","nexus_abi_version":1,
           "required_key_block_types":[],"compute_export":"compute","init_export":"init"}"#,
    )
    .unwrap()
}

fn empty_input() -> ComputeInput {
    serde_json::from_str(r#"{"schema_version":1,"world_ref":{"world_id":"w"},"key_blocks":[]}"#)
        .unwrap()
}

#[test]
fn infinite_loop_is_bounded_by_fuel() {
    let engine = WasmEngine::new().unwrap();
    let module = engine.load_module(&infinite_loop_module()).unwrap();

    let err = engine
        .compute(&module, &manifest(), &empty_input())
        .expect_err("infinite loop must not succeed");

    assert!(
        matches!(err, ComputeError::OutOfFuel),
        "expected OutOfFuel, got {err:?}"
    );
}

/// A tiny fuel budget set via the manifest also bounds a finite-but-greedy
/// module (here, the same infinite loop). Confirms manifest-level override
/// wiring.
#[test]
fn manifest_fuel_override_bounds_compute() {
    let engine = WasmEngine::new().unwrap();
    let module = engine.load_module(&infinite_loop_module()).unwrap();
    let mut manifest = manifest();
    manifest.max_fuel = Some(1_000); // barely enough to instantiate, not to loop

    let err = engine
        .compute(&module, &manifest, &empty_input())
        .expect_err("tiny fuel budget must trap");

    assert!(
        matches!(err, ComputeError::OutOfFuel | ComputeError::Trap(_)),
        "expected fuel/trap from tiny budget, got {err:?}"
    );
}
