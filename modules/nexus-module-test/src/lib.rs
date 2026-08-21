//! `nexus-module-test` — ABI-conformance mini-host for Nexus compute modules
//! (V1.170 P0, AR-10).
//!
//! A standalone, publishable test harness that module crates add as a
//! host-target **dev-dependency**. It compiles a module's `.wasm` with
//! wasmtime and runs one stateless compute invocation through the real V1 ABI:
//!
//! - exports table: `memory` + `alloc` + `compute` (the `init` export is
//!   optional and called when the manifest's `init_export` is exported);
//! - the two whitelisted `nexus::` host imports (`kb_read`, `narrative_query`)
//!   served from the invocation snapshot, with the `-1`/`-2` sentinel
//!   convention (`nexus-wasm-host` `host.rs` L23–31);
//! - calling convention: allocate input + output buffers through the module's
//!   own `alloc`, write the input JSON, call `init` then `compute`, map
//!   `>= 0` → read the output, `-1`/`-2` → typed errors ([`MiniHostError`]).
//!
//! ## Honest boundary
//!
//! The mini-host validates **ABI conformance only** — exactly the AR-10 scope.
//! It does **not** enforce fuel, memory-cap, or wall-time limits; those are
//! `nexus-wasm-host` runtime duties (`sandbox.rs`). A module that passes here
//! may still be rejected by the real host for sandbox reasons, and a hostile
//! module can hang or exhaust memory in this process. Run untrusted modules
//! only in the real host.
//!
//! ## Types
//!
//! [`run`] takes the SDK's typed envelope ([`ComputeInput`], [`ComputeOutput`])
//! and manifest ([`ModuleManifest`]) so module crates — which compile against
//! the SDK — pass their own types directly. AR-1 lists `wasmtime` +
//! `serde`/`serde_json` for this crate; the SDK path dep is required by the
//! AR-10 signature and is host-target only (it never lands in a wasm artifact).
//!
//! ## Fixture conformance
//!
//! `fixtures/combat-input.json` is the canonical input fixture (SSOT, copied
//! from `crates/nexus-wasm-host/tests/basic_combat.rs`; the
//! `check-module-sdk-drift.sh` gate asserts value-identity with the test
//! file's inline JSON). The `fixture_conformance` integration test parses it
//! and runs it through the mini-host. For full real-artifact conformance set
//! `NEXUS_MODULE_TEST_WASM` and `NEXUS_MODULE_TEST_MANIFEST` to a compiled
//! module pair (e.g. `basic-combat`); the test then asserts the complete
//! 4-part output contract (the AR-12 `module-dx` CI leg does exactly this).

use std::collections::HashMap;
use std::fmt;

use nexus_module_sdk::{ComputeInput, ComputeOutput, HostFunction, ModuleManifest};
use wasmtime::{Caller, Engine, Extern, Instance, Linker, Memory, Module, Store, TypedFunc};

/// Sentinel returned by host functions when the lookup yields nothing
/// (mirrors `nexus-wasm-host::host::RET_NOT_FOUND`).
const RET_NOT_FOUND: i64 = -1;
/// Sentinel returned when the caller's output buffer is too small (mirrors
/// `nexus-wasm-host::host::RET_OVERFLOW`).
const RET_OVERFLOW: i64 = -2;
/// Output buffer reserved in module memory (mirrors the real host's 1 MiB
/// reservation in `compute.rs` `OUTPUT_BUFFER_BYTES`).
const OUTPUT_BUFFER_BYTES: u32 = 1 << 20;

/// Errors produced by one mini-host invocation (AR-10 typed error mapping).
#[derive(Debug)]
pub enum MiniHostError {
    /// The wasm bytes failed to compile/validate or the instance failed to
    /// instantiate — including a module importing a `nexus::*` function the
    /// manifest did not whitelist (the whitelist is enforced by linker
    /// registration, exactly like the real host).
    Instantiation(String),
    /// A required export (`memory`, `alloc`, or the manifest's
    /// `compute_export`) is missing.
    MissingExport(String),
    /// The module's `compute` returned `-1` — the module-level failure
    /// sentinel (AR-5 mapping).
    ModuleFailed,
    /// The module's `compute` returned `-2` — output buffer too small.
    OutputTooSmall,
    /// The module's `compute` returned an unknown negative sentinel.
    UnknownSentinel(i64),
    /// The module trapped during execution.
    Trap(String),
    /// The SDK-side envelope failed to (de)serialize (cannot normally happen
    /// for the typed types).
    Serialization(String),
    /// The output JSON could not be deserialized as a [`ComputeOutput`].
    InvalidOutput(String),
}

impl fmt::Display for MiniHostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Instantiation(e) => write!(f, "module instantiation failed: {e}"),
            Self::MissingExport(name) => write!(f, "module missing required export `{name}`"),
            Self::ModuleFailed => write!(f, "module `compute` returned -1 (module error)"),
            Self::OutputTooSmall => write!(
                f,
                "module `compute` returned -2 (output buffer too small) or wrote past the buffer"
            ),
            Self::UnknownSentinel(n) => {
                write!(f, "module `compute` returned unknown negative sentinel {n}")
            }
            Self::Trap(e) => write!(f, "module trapped: {e}"),
            Self::Serialization(e) => write!(f, "envelope (de)serialization failed: {e}"),
            Self::InvalidOutput(e) => write!(f, "module output is not a valid ComputeOutput: {e}"),
        }
    }
}

impl std::error::Error for MiniHostError {}

/// Read-only snapshot served to host functions during one invocation.
///
/// Mirrors `nexus-wasm-host::HostContext`: `key_blocks` are indexed by the
/// spoke `entry_id` field (O(1) `kb_read`), and `narrative_query` returns the
/// invocation's `narrative_state` verbatim. Stateless — one instance per
/// [`run`] call.
struct InvocationState {
    key_blocks: HashMap<String, serde_json::Value>,
    narrative_state: serde_json::Value,
}

impl InvocationState {
    fn from_input(input: &ComputeInput) -> Self {
        let mut key_blocks = HashMap::new();
        for kb in &input.key_blocks {
            if let Ok(json) = serde_json::to_value(kb) {
                // Index `entry_id` first with `key_block_id` fallback (I7):
                // the canonical fixture (and the real host's basic_combat
                // fixture) use `key_block_id`, while spoke-produced blocks
                // use `entry_id` — same accessor semantics as the SDK's
                // `entry_id_of` (AR-3).
                let id = kb
                    .get("entry_id")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| kb.get("key_block_id").and_then(serde_json::Value::as_str));
                if let Some(id) = id {
                    key_blocks.insert(id.to_string(), json);
                }
            }
        }
        Self {
            key_blocks,
            narrative_state: input.narrative_state.clone(),
        }
    }

    fn kb_read(&self, id: &str) -> Option<&serde_json::Value> {
        self.key_blocks.get(id)
    }

    fn narrative_query(&self, _query: &serde_json::Value) -> serde_json::Value {
        self.narrative_state.clone()
    }
}

/// Run one stateless compute invocation against a module's compiled bytes.
///
/// # Errors
///
/// See [`MiniHostError`]: instantiation/export failures, module traps,
/// the `-1`/`-2` sentinels, or an unparseable output envelope.
pub fn run(
    wasm_bytes: &[u8],
    manifest: &ModuleManifest,
    input: &ComputeInput,
) -> Result<ComputeOutput, MiniHostError> {
    let engine = Engine::default();
    let module = Module::new(&engine, wasm_bytes)
        .map_err(|e| MiniHostError::Instantiation(e.to_string()))?;

    let input_bytes = serde_json::to_vec(input)
        .map_err(|e| MiniHostError::Serialization(format!("input envelope: {e}")))?;

    let mut store = Store::new(&engine, InvocationState::from_input(input));
    let mut linker = Linker::<InvocationState>::new(&engine);
    register_host_imports(&mut linker, manifest)?;
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|e| MiniHostError::Instantiation(e.to_string()))?;

    // Optional one-shot `init` (the manifest's `init_export`; a declared-but-
    // not-exported name is skipped, exactly like the real host).
    if let Some(init) = optional_export::<(), ()>(&mut store, &instance, &manifest.init_export) {
        init.call(&mut store, ()).map_err(map_call_error)?;
    }

    // The host places input and reserves output inside the module's own
    // linear memory through the module's allocator.
    let alloc = required_export::<u32, u32>(&mut store, &instance, "alloc")?;
    let in_len = u32::try_from(input_bytes.len()).unwrap_or(u32::MAX);
    let in_ptr = alloc.call(&mut store, in_len).map_err(map_call_error)?;
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| MiniHostError::MissingExport("memory".to_string()))?;
    memory
        .write(&mut store, in_ptr as usize, &input_bytes)
        .map_err(|e| MiniHostError::Trap(format!("memory write: {e}")))?;

    let out_cap = OUTPUT_BUFFER_BYTES;
    let out_ptr = alloc.call(&mut store, out_cap).map_err(map_call_error)?;

    let compute = required_export::<(u32, u32, u32, u32), i64>(
        &mut store,
        &instance,
        &manifest.compute_export,
    )?;
    let written = compute
        .call(&mut store, (in_ptr, in_len, out_ptr, out_cap))
        .map_err(map_call_error)?;

    if written < 0 {
        return Err(match written {
            -1 => MiniHostError::ModuleFailed,
            -2 => MiniHostError::OutputTooSmall,
            other => MiniHostError::UnknownSentinel(other),
        });
    }
    let written = usize::try_from(written).map_err(|_| MiniHostError::OutputTooSmall)?;
    if written > out_cap as usize {
        return Err(MiniHostError::OutputTooSmall);
    }

    let mut out_bytes = vec![0u8; written];
    memory
        .read(&store, out_ptr as usize, &mut out_bytes)
        .map_err(|e| MiniHostError::Trap(format!("memory read: {e}")))?;
    serde_json::from_slice(&out_bytes)
        .map_err(|e| MiniHostError::InvalidOutput(format!("{e} (module wrote {written} bytes)")))
}

/// Register the whitelisted host imports on a [`Linker`].
///
/// Only the functions listed in `manifest.host_functions` are linked. A module
/// that imports a non-registered `nexus::*` function fails instantiation —
/// the explicit enforcement of the whitelist (identical to the real host).
fn register_host_imports(
    linker: &mut Linker<InvocationState>,
    manifest: &ModuleManifest,
) -> Result<(), MiniHostError> {
    if manifest.host_functions.contains(&HostFunction::KbRead) {
        linker
            .func_wrap(
                "nexus",
                "kb_read",
                |mut caller: Caller<'_, InvocationState>,
                 id_ptr: u32,
                 id_len: u32,
                 out_ptr: u32,
                 out_cap: u32|
                 -> wasmtime::Result<i64> {
                    let Some(mem) = current_memory(&mut caller) else {
                        return Ok(RET_NOT_FOUND);
                    };
                    let Some(id_bytes) = read_bytes(&caller, &mem, id_ptr, id_len) else {
                        return Ok(RET_NOT_FOUND);
                    };
                    let Ok(id) = std::str::from_utf8(&id_bytes) else {
                        return Ok(RET_NOT_FOUND);
                    };
                    let Some(value) = caller.data().kb_read(id) else {
                        return Ok(RET_NOT_FOUND);
                    };
                    let Ok(json) = serde_json::to_vec(value) else {
                        return Ok(RET_NOT_FOUND);
                    };
                    write_or_overflow(&mut caller, &mem, out_ptr, out_cap, &json)
                },
            )
            .map_err(|e| MiniHostError::Instantiation(format!("kb_read linkage: {e}")))?;
    }

    if manifest
        .host_functions
        .contains(&HostFunction::NarrativeQuery)
    {
        linker
            .func_wrap(
                "nexus",
                "narrative_query",
                |mut caller: Caller<'_, InvocationState>,
                 q_ptr: u32,
                 q_len: u32,
                 out_ptr: u32,
                 out_cap: u32|
                 -> wasmtime::Result<i64> {
                    let Some(mem) = current_memory(&mut caller) else {
                        return Ok(RET_NOT_FOUND);
                    };
                    let Some(q_bytes) = read_bytes(&caller, &mem, q_ptr, q_len) else {
                        return Ok(RET_NOT_FOUND);
                    };
                    let query: serde_json::Value =
                        serde_json::from_slice(&q_bytes).unwrap_or_default();
                    let Ok(json) = serde_json::to_vec(&caller.data().narrative_query(&query))
                    else {
                        return Ok(RET_NOT_FOUND);
                    };
                    write_or_overflow(&mut caller, &mem, out_ptr, out_cap, &json)
                },
            )
            .map_err(|e| MiniHostError::Instantiation(format!("narrative_query linkage: {e}")))?;
    }

    Ok(())
}

/// Fetch the calling instance's exported linear `memory`.
fn current_memory<T>(caller: &mut Caller<'_, T>) -> Option<Memory> {
    caller.get_export("memory").and_then(Extern::into_memory)
}

/// Read `len` bytes from `[ptr, ptr+len)` out of the instance memory.
fn read_bytes<T>(caller: &Caller<'_, T>, mem: &Memory, ptr: u32, len: u32) -> Option<Vec<u8>> {
    let len = usize::try_from(len).ok()?;
    let ptr = usize::try_from(ptr).ok()?;
    let mut buf = vec![0u8; len];
    mem.read(caller, ptr, &mut buf).ok()?;
    Some(buf)
}

/// Write `bytes` to `[ptr, ptr+bytes.len())`, respecting `cap`. Returns the
/// number of bytes written or the overflow sentinel.
fn write_or_overflow<T>(
    caller: &mut Caller<'_, T>,
    mem: &Memory,
    ptr: u32,
    cap: u32,
    bytes: &[u8],
) -> wasmtime::Result<i64> {
    if bytes.len() > usize::try_from(cap).unwrap_or(0) {
        return Ok(RET_OVERFLOW);
    }
    mem.write(caller, ptr as usize, bytes)?;
    Ok(i64::try_from(bytes.len()).unwrap_or(RET_OVERFLOW))
}

/// Look up an optional export (used for `init` — a missing export is not an
/// error).
fn optional_export<Params, Returns>(
    store: &mut Store<InvocationState>,
    instance: &Instance,
    name: &str,
) -> Option<TypedFunc<Params, Returns>>
where
    Params: wasmtime::WasmParams,
    Returns: wasmtime::WasmResults,
{
    instance.get_typed_func(store, name).ok()
}

/// Look up a required export, returning [`MiniHostError::MissingExport`] when
/// absent.
fn required_export<Params, Returns>(
    store: &mut Store<InvocationState>,
    instance: &Instance,
    name: &str,
) -> Result<TypedFunc<Params, Returns>, MiniHostError>
where
    Params: wasmtime::WasmParams,
    Returns: wasmtime::WasmResults,
{
    instance
        .get_typed_func(store, name)
        .map_err(|_| MiniHostError::MissingExport(name.to_string()))
}

/// Map a wasmtime call error to a [`MiniHostError::Trap`].
fn map_call_error(e: wasmtime::Error) -> MiniHostError {
    MiniHostError::Trap(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_module_sdk::WorldRef;
    use serde_json::json;

    /// Minimal valid manifest for a probe module.
    fn manifest() -> ModuleManifest {
        ModuleManifest {
            module_id: "probe".to_string(),
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

    fn input(key_blocks: Vec<serde_json::Value>) -> ComputeInput {
        ComputeInput {
            schema_version: 1,
            world_ref: WorldRef {
                world_id: Some("wld_test".to_string()),
                branch_id: Some("root".to_string()),
                timeline_head_event_id: Some("evt_0".to_string()),
            },
            key_blocks,
            narrative_state: json!({"current_chapter": "ch-1"}),
            invocation: json!({}),
        }
    }

    /// Bump-allocating module skeleton with optional imports, `init`, and a
    /// `compute` body.
    fn module_wat(imports: &str, init: &str, compute_body: &str) -> String {
        format!(
            r#"(module
  {imports}
  (memory (export "memory") 32)
  (global $heap (mut i32) (i32.const 1024))
  (func (export "alloc") (param $len i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (i32.add (local.get $len) (i32.const 7))))
    (local.get $ptr))
  (func (export "init"){init})
  (func (export "compute") (param $in_ptr i32) (param $in_len i32) (param $out_ptr i32) (param $out_cap i32) (result i64)
    {compute_body})
)"#
        )
    }

    /// A module whose `compute` returns a fixed (negative) sentinel.
    fn sentinel_module_wat(sentinel: i64) -> String {
        module_wat("", "", &format!("(i64.const {sentinel})"))
    }

    /// A module that echoes a static envelope into the output buffer.
    fn echo_module_wat(envelope: &str) -> String {
        // The envelope is embedded as a wat string literal — escape quotes.
        let escaped = envelope.replace('\\', "\\\\").replace('"', "\\\"");
        format!(
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
        )
    }

    /// Drive `compute` directly and return the raw i64, bypassing envelope
    /// parsing (host-import probing).
    fn raw_compute(
        wasm: &[u8],
        manifest: &ModuleManifest,
        input: &ComputeInput,
    ) -> Result<i64, MiniHostError> {
        let engine = Engine::default();
        let module =
            Module::new(&engine, wasm).map_err(|e| MiniHostError::Instantiation(e.to_string()))?;
        let mut store = Store::new(&engine, InvocationState::from_input(input));
        let mut linker = Linker::<InvocationState>::new(&engine);
        register_host_imports(&mut linker, manifest)?;
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| MiniHostError::Instantiation(e.to_string()))?;
        let compute = required_export::<(u32, u32, u32, u32), i64>(
            &mut store,
            &instance,
            &manifest.compute_export,
        )?;
        compute
            .call(&mut store, (0, 0, 0, 0))
            .map_err(map_call_error)
    }

    #[test]
    fn run_round_trips_output_envelope() {
        let envelope = r#"{"schema_version":1,"state_delta":[],"timeline_events":[],"new_key_blocks":[],"battle_report":{"kind":"probe"}}"#;
        let wasm = wat::parse_str(echo_module_wat(envelope)).expect("wat parses");

        let output = run(&wasm, &manifest(), &input(vec![])).expect("run succeeds");
        assert_eq!(output.schema_version, 1);
        assert!(output.state_delta.is_empty());
        assert!(output.timeline_events.is_empty());
        assert!(output.new_key_blocks.is_empty());
        assert_eq!(output.battle_report["kind"], "probe");
    }

    #[test]
    fn run_maps_compute_negative_sentinels() {
        let manifest = manifest();
        for (sentinel, expected) in [
            (-1, MiniHostError::ModuleFailed),
            (-2, MiniHostError::OutputTooSmall),
            (-7, MiniHostError::UnknownSentinel(-7)),
        ] {
            let wasm = wat::parse_str(sentinel_module_wat(sentinel)).expect("wat parses");
            let err = run(&wasm, &manifest, &input(vec![])).expect_err("negative sentinel");
            assert!(
                std::mem::discriminant(&err) == std::mem::discriminant(&expected),
                "{err}"
            );
        }
    }

    #[test]
    fn run_requires_alloc_export() {
        let wat = r#"(module
  (memory (export "memory") 1)
  (func (export "compute") (param i32 i32 i32 i32) (result i64) (i64.const 0)))"#;
        let wasm = wat::parse_str(wat).expect("wat parses");
        let err = run(&wasm, &manifest(), &input(vec![])).expect_err("missing alloc must fail");
        assert!(matches!(err, MiniHostError::MissingExport(name) if name == "alloc"));
    }

    #[test]
    fn unwhitelisted_nexus_import_fails_instantiation() {
        // The ABI whitelist is enforced by linker registration: a module
        // importing a function the manifest did not whitelist fails
        // instantiation even when it is not one of the real host functions.
        let wat = r#"(module
  (import "nexus" "not_a_real_function" (func $f (param i32)))
  (memory (export "memory") 1)
  (func (export "alloc") (param $len i32) (result i32) (i32.const 0))
  (func (export "compute") (param i32 i32 i32 i32) (result i64) (i64.const 0)))"#;
        let wasm = wat::parse_str(wat).expect("wat parses");
        let err = run(&wasm, &manifest(), &input(vec![])).expect_err("must fail");
        assert!(matches!(err, MiniHostError::Instantiation(_)), "{err}");
    }

    #[test]
    fn init_declared_but_absent_is_skipped() {
        // Declared `init_export` with no matching export → skipped (optional
        // export), and compute still runs (returns -1 → ModuleFailed).
        let wat = r#"(module
  (memory (export "memory") 32)
  (global $heap (mut i32) (i32.const 1024))
  (func (export "alloc") (param $len i32) (result i32)
    (local $ptr i32)
    (local.set $ptr (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $len)))
    (local.get $ptr))
  (func (export "compute") (param i32 i32 i32 i32) (result i64) (i64.const -1)))"#;
        let wasm = wat::parse_str(wat).expect("wat parses");
        assert!(matches!(
            run(&wasm, &manifest(), &input(vec![])),
            Err(MiniHostError::ModuleFailed)
        ));
    }

    #[test]
    fn init_trap_fails_the_run() {
        let wat = r#"(module
  (memory (export "memory") 1)
  (func (export "alloc") (param $len i32) (result i32) (i32.const 0))
  (func (export "init") (unreachable))
  (func (export "compute") (param i32 i32 i32 i32) (result i64) (i64.const 0)))"#;
        let wasm = wat::parse_str(wat).expect("wat parses");
        let err = run(&wasm, &manifest(), &input(vec![])).expect_err("init trap must fail");
        assert!(matches!(err, MiniHostError::Trap(_)), "{err}");
    }

    // ── host-import probing (whitelisted imports, -1/-2 sentinels) ────────

    /// Module whose `compute` calls `kb_read` with a static id and copies the
    /// host response to a fixed probe address, returning the host's return
    /// value verbatim. `probe_addr` must not collide with the out buffer.
    fn kb_read_module(id: &str, out_cap: u32) -> String {
        format!(
            r#"(module
  (import "nexus" "kb_read" (func $kb_read (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 32)
  (data (i32.const 0x30000) "{id}")
  (func (export "alloc") (param $len i32) (result i32) (i32.const 0))
  (func (export "init"))
  (func (export "compute") (param i32 i32 i32 i32) (result i64)
    (call $kb_read (i32.const 0x30000) (i32.const {len}) (i32.const 0x31000) (i32.const {outcap}))))
"#,
            id = id,
            len = id.len(),
            outcap = out_cap
        )
    }

    /// Read `len` bytes at `addr` from the instance memory after a compute
    /// call (probe helper).
    fn read_probe(
        wasm: &[u8],
        manifest: &ModuleManifest,
        input: &ComputeInput,
        addr: usize,
    ) -> (i64, Vec<u8>) {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm).expect("module compiles");
        let mut store = Store::new(&engine, InvocationState::from_input(input));
        let mut linker = Linker::<InvocationState>::new(&engine);
        register_host_imports(&mut linker, manifest).expect("linker ok");
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("instantiates");
        let compute =
            required_export::<(u32, u32, u32, u32), i64>(&mut store, &instance, "compute")
                .expect("compute export");
        let written = compute.call(&mut store, (0, 0, 0, 0)).expect("call ok");
        let mem = instance.get_memory(&mut store, "memory").expect("memory");
        let mut buf = vec![0u8; usize::try_from(written).unwrap_or(0)];
        if written > 0 {
            mem.read(&store, addr, &mut buf).expect("read ok");
        }
        (written, buf)
    }

    #[test]
    fn kb_read_serves_snapshot_block() {
        let mut manifest = manifest();
        manifest.host_functions = vec![HostFunction::KbRead];
        let input = input(vec![json!({
            "entry_id": "kb_atk",
            "canonical_name": "Striker",
            "body": {"attributes": {"base_atk": 20}},
        })]);
        let wasm = wat::parse_str(kb_read_module("kb_atk", 4096)).expect("wat parses");
        let (written, buf) = read_probe(&wasm, &manifest, &input, 0x31000);
        assert!(written > 0, "kb_read must write bytes, got {written}");
        let text = String::from_utf8_lossy(&buf);
        assert!(text.contains("kb_atk"), "{text}");
        assert!(text.contains("Striker"), "{text}");
    }

    #[test]
    fn kb_read_indexes_key_block_id_fallback() {
        // I7: the canonical fixture (fixtures/combat-input.json) indexes its
        // blocks by `key_block_id`, not `entry_id` — the mini-host must serve
        // `kb_read("kb_atk")` against it (fixture parity with the real host).
        let fixture: ComputeInput =
            serde_json::from_str(include_str!("../fixtures/combat-input.json"))
                .expect("canonical fixture parses as ComputeInput");
        let mut manifest = manifest();
        manifest.host_functions = vec![HostFunction::KbRead];
        let wasm = wat::parse_str(kb_read_module("kb_atk", 4096)).expect("wat parses");
        let (written, buf) = read_probe(&wasm, &manifest, &fixture, 0x31000);
        assert!(written > 0, "kb_read must write bytes, got {written}");
        let text = String::from_utf8_lossy(&buf);
        assert!(text.contains("kb_atk"), "{text}");
        assert!(text.contains("Striker"), "{text}");
    }

    #[test]
    fn kb_read_unknown_id_returns_not_found() {
        let mut manifest = manifest();
        manifest.host_functions = vec![HostFunction::KbRead];
        let wasm = wat::parse_str(kb_read_module("nope", 4096)).expect("wat parses");
        let written = raw_compute(&wasm, &manifest, &input(vec![])).expect("compute ok");
        assert_eq!(written, RET_NOT_FOUND);
    }

    #[test]
    fn kb_read_overflow_returns_minus_two() {
        let mut manifest = manifest();
        manifest.host_functions = vec![HostFunction::KbRead];
        let input = input(vec![json!({"entry_id": "kb_atk", "body": {"x": 1}})]);
        let wasm = wat::parse_str(kb_read_module("kb_atk", 4)).expect("wat parses");
        let written = raw_compute(&wasm, &manifest, &input).expect("compute ok");
        assert_eq!(written, RET_OVERFLOW);
    }

    #[test]
    fn narrative_query_returns_narrative_state() {
        let wat = r#"(module
  (import "nexus" "narrative_query" (func $nq (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 32)
  (func (export "alloc") (param $len i32) (result i32) (i32.const 0))
  (func (export "init"))
  (func (export "compute") (param i32 i32 i32 i32) (result i64)
    (call $nq (i32.const 0) (i32.const 0) (i32.const 0x31000) (i32.const 4096))))
"#;
        let mut manifest = manifest();
        manifest.host_functions = vec![HostFunction::NarrativeQuery];
        let input = input(vec![]);
        let wasm = wat::parse_str(wat).expect("wat parses");
        let (written, buf) = read_probe(&wasm, &manifest, &input, 0x31000);
        assert!(
            written > 0,
            "narrative_query must write bytes, got {written}"
        );
        let text = String::from_utf8_lossy(&buf);
        assert!(
            text.contains("ch-1"),
            "narrative state must be served: {text}"
        );
    }

    #[test]
    fn manifest_whitelist_gates_host_imports() {
        // A module calling kb_read while the manifest whitelists nothing →
        // the import is not linked → instantiation fails (whitelist
        // enforcement, like the real host).
        let wat = r#"(module
  (import "nexus" "kb_read" (func $kb_read (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 1)
  (func (export "alloc") (param $len i32) (result i32) (i32.const 0))
  (func (export "compute") (param i32 i32 i32 i32) (result i64) (i64.const 0)))"#;
        let wasm = wat::parse_str(wat).expect("wat parses");
        let err = run(&wasm, &manifest(), &input(vec![])).expect_err("must fail");
        assert!(matches!(err, MiniHostError::Instantiation(_)), "{err}");
    }
}
