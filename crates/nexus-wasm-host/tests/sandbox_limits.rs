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
    serde_json::from_str(
        r#"{"schema_version":1,"world_ref":{"world_id":"wld_empty"},"key_blocks":[]}"#,
    )
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

/// A module whose initial memory already exceeds the invocation's memory cap.
/// Instantiation hits the `StoreLimits` resource limiter and must surface as
/// [`ComputeError::MemoryCapExceeded`], not a generic trap or a host crash.
fn big_memory_module() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (memory (export "memory") 64)
            (global $heap (mut i32) (i32.const 1024))
            (func (export "alloc") (param $len i32) (result i32)
              (local $p i32)
              (local.set $p (global.get $heap))
              (global.set $heap (i32.add (global.get $heap) (local.get $len)))
              (local.get $p))
            (func (export "init"))
            (func (export "compute")
              (param i32 i32 i32 i32) (result i64)
              (i64.const 0)))
        "#,
    )
    .expect("valid wat")
}

#[test]
fn memory_cap_is_enforced_at_instantiation() {
    let engine = WasmEngine::new().unwrap();
    // 64 pages = 4 MiB initial memory; the manifest tightens the cap to 1 MiB.
    let module = engine.load_module(&big_memory_module()).unwrap();
    let mut manifest = manifest();
    manifest.max_memory_mib = Some(1);

    let err = engine
        .compute(&module, &manifest, &empty_input())
        .expect_err("oversized memory must be rejected");

    assert!(
        matches!(err, ComputeError::MemoryCapExceeded),
        "expected MemoryCapExceeded, got {err:?}"
    );
}

/// The wall-time watchdog must trap a runaway module independently of fuel:
/// with the default fuel budget intact but a 1 ms wall-time deadline, the
/// infinite loop must surface as [`ComputeError::WallTimeExceeded`]
/// (epoch interruption), not `OutOfFuel`.
#[test]
fn wall_time_deadline_traps_independently_of_fuel() {
    let engine = WasmEngine::new().unwrap();
    let module = engine.load_module(&infinite_loop_module()).unwrap();
    let mut manifest = manifest();
    manifest.max_wall_time_ms = Some(1);

    let err = engine
        .compute(&module, &manifest, &empty_input())
        .expect_err("runaway module must hit the wall-time deadline");

    assert!(
        matches!(err, ComputeError::WallTimeExceeded),
        "expected WallTimeExceeded, got {err:?}"
    );
}

/// Wasmtime 47+ enables the GC / function-references / exceptions proposals by
/// default; the engine pins them OFF (`engine.rs`) to preserve the v46 module
/// admission set. A module requiring the exceptions proposal (`try_table`)
/// must be rejected at load time.
#[test]
fn rejects_newly_default_enabled_proposals() {
    let engine = WasmEngine::new().unwrap();
    let wasm = wat::parse_str(
        r#"(module
            (tag $e)
            (memory (export "memory") 1)
            (func (export "alloc") (param $len i32) (result i32) (i32.const 0))
            (func (export "compute") (param i32 i32 i32 i32) (result i64)
              (block $b (result)
                (try_table (catch $e $b) (nop)))
              (i64.const 0)))
        "#,
    )
    .expect("valid wat");

    let err = engine
        .load_module(&wasm)
        .expect_err("exceptions-proposal module must be rejected");

    assert!(
        matches!(err, ComputeError::InvalidModule(_)),
        "expected InvalidModule, got {err:?}"
    );
}
