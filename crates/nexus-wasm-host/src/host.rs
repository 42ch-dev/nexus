//! Host function ABI exposed to compute modules (open design item #4).
//!
//! A module may import up to two host functions from the `nexus` module
//! namespace. The host wires exactly the functions the module's
//! [`ModuleManifest`](crate::ModuleManifest) whitelists; importing a function
//! the host did not register fails instantiation (explicit enforcement).
//!
//! ## Memory-buffer ABI
//!
//! Both host functions follow the same convention so a module never has to
//! guess at allocation:
//!
//! ```text
//! nexus::kb_read(id_ptr: u32, id_len: u32, out_ptr: u32, out_cap: u32) -> i64
//! nexus::narrative_query(q_ptr: u32, q_len: u32, out_ptr: u32, out_cap: u32) -> i64
//! ```
//!
//! The module owns its linear memory and passes a buffer it allocated for the
//! result. The host reads the request bytes from `[ptr, ptr+len)`, writes the
//! UTF-8 JSON response into `[out_ptr, out_ptr+written)`, and returns `written`
//! as a non-negative `i64`. On failure it returns a sentinel:
//!
//! | Return | Meaning |
//! | --- | --- |
//! | `>= 0` | Bytes written to `out`. |
//! | `-1`   | Not found / unsupported query. |
//! | `-2`   | `out_cap` too small for the response. |
//!
//! Modules are stateless (compass Q6): the snapshot served by `kb_read` is the
//! exact `key_blocks` array the host bundled into this invocation — there is no
//! cross-call state.

use std::collections::HashMap;

use wasmtime::{Caller, Extern, Linker, Memory, StoreLimits};

use crate::error::Result;
use crate::manifest::{HostFunction, ModuleManifest};

/// Sentinel returned by host functions when the lookup yields nothing.
pub const RET_NOT_FOUND: i64 = -1;
/// Sentinel returned when the caller's output buffer is too small.
pub const RET_OVERFLOW: i64 = -2;

/// Read-only snapshot served to host functions during one invocation.
///
/// Built from the [`ComputeInput`](crate::ComputeInput) envelope: `KeyBlocks` are
/// indexed by ID for O(1) `kb_read`, and the narrative state is passed through
/// to `narrative_query`. Stateless — one instance per `compute()` call.
#[derive(Clone, Debug, Default)]
pub struct HostContext {
    /// `key_block_id` → `KeyBlock` JSON, indexed for `kb_read`.
    key_blocks: HashMap<String, serde_json::Value>,
    /// Narrative context returned by `narrative_query` (V1 pass-through).
    narrative_state: serde_json::Value,
}

/// Per-invocation host state, stored inside the wasmtime `Store`.
///
/// Combines the read-only compute snapshot (served to host functions) with the
/// wasmtime resource limiter. Both are fresh per `compute()` call. Mirrors the
/// documented wasmtime pattern of keeping `StoreLimits` next to the host data.
pub struct InvocationState {
    /// Snapshot served to `kb_read` / `narrative_query`.
    pub ctx: HostContext,
    /// Memory/instance/table caps applied to this invocation.
    pub limits: StoreLimits,
}

/// Builds a [`HostContext`] from a parsed `ComputeInput`.
///
/// The context indexes the bundled `KeyBlocks` by ID so `kb_read` is O(1).
impl HostContext {
    /// Build a host context from a `ComputeInput` envelope.
    #[must_use]
    pub fn from_input(input: &crate::ComputeInput) -> Self {
        let mut blocks = HashMap::with_capacity(input.key_blocks.len());
        for kb in &input.key_blocks {
            // Re-serialize each KeyBlock so the module receives canonical JSON.
            if let Ok(json) = serde_json::to_value(kb) {
                blocks.insert(kb.key_block_id.clone(), json);
            }
        }
        let narrative_state = input.narrative_state.clone().unwrap_or_default();
        Self {
            key_blocks: blocks,
            narrative_state,
        }
    }

    /// Look up a `KeyBlock` by ID. Returns its JSON value, or `None`.
    #[must_use]
    pub fn kb_read(&self, id: &str) -> Option<&serde_json::Value> {
        self.key_blocks.get(id)
    }

    /// Query narrative context. V1 returns the invocation's `narrative_state`
    /// verbatim; a richer query engine lands in a later iteration.
    #[must_use]
    pub fn narrative_query(&self, _query: &serde_json::Value) -> serde_json::Value {
        self.narrative_state.clone()
    }
}

// ---------------------------------------------------------------------------
// Linker wiring
// ---------------------------------------------------------------------------

/// Register the whitelisted host imports on a [`Linker`].
///
/// Only the functions listed in `manifest.host_functions` are linked. A module
/// that imports a non-registered `nexus::*` function fails instantiation — the
/// explicit enforcement of the whitelist.
pub fn register_host_imports(
    linker: &mut Linker<InvocationState>,
    manifest: &ModuleManifest,
) -> Result<()> {
    if manifest.allows(HostFunction::KbRead) {
        linker.func_wrap(
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
                // Read the requested key_block_id.
                let Some(id_bytes) = read_bytes(&caller, &mem, id_ptr, id_len) else {
                    return Ok(RET_NOT_FOUND);
                };
                let Ok(id_str) = std::str::from_utf8(&id_bytes) else {
                    return Ok(RET_NOT_FOUND);
                };
                let Some(value) = caller.data().ctx.kb_read(id_str) else {
                    return Ok(RET_NOT_FOUND);
                };
                let Ok(json) = serde_json::to_vec(value) else {
                    return Ok(RET_NOT_FOUND);
                };
                write_or_overflow(&mut caller, &mem, out_ptr, out_cap, &json)
            },
        )?;
    }

    if manifest.allows(HostFunction::NarrativeQuery) {
        linker.func_wrap(
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
                let query: serde_json::Value = serde_json::from_slice(&q_bytes).unwrap_or_default();
                let Ok(json) = serde_json::to_vec(&caller.data().ctx.narrative_query(&query))
                else {
                    return Ok(RET_NOT_FOUND);
                };
                write_or_overflow(&mut caller, &mem, out_ptr, out_cap, &json)
            },
        )?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComputeInput;

    fn ctx_with_two_blocks() -> HostContext {
        let raw = r#"{
            "schema_version": 1,
            "world_ref": {"world_id": "w1"},
            "key_blocks": [
                {"schema_version":1,"key_block_id":"kb-a","world_id":"w1","block_type":"character","canonical_name":"A","status":"confirmed","created_at":"t"},
                {"schema_version":1,"key_block_id":"kb-b","world_id":"w1","block_type":"character","canonical_name":"B","status":"confirmed","created_at":"t"}
            ]
        }"#;
        let input: ComputeInput = serde_json::from_str(raw).unwrap();
        HostContext::from_input(&input)
    }

    #[test]
    fn kb_read_returns_indexed_block() {
        let ctx = ctx_with_two_blocks();
        let a = ctx.kb_read("kb-a").expect("kb-a present");
        assert_eq!(a["canonical_name"], "A");
        assert!(ctx.kb_read("missing").is_none());
    }

    #[test]
    fn narrative_query_passes_through_state() {
        let mut ctx = ctx_with_two_blocks();
        ctx.narrative_state = serde_json::json!({"current_chapter": "ch3"});
        let out = ctx.narrative_query(&serde_json::Value::Null);
        assert_eq!(out["current_chapter"], "ch3");
    }

    /// End-to-end: a real (tiny) WASM module imports `nexus::kb_read` and calls
    /// it; the host must serve the snapshot and the result must round-trip
    /// through linear memory. This validates the full host-import ABI path
    /// (Linker wiring, Caller memory marshalling, snapshot lookup) — acceptance
    /// criterion #4 ("host functions whitelisted and functional").
    #[test]
    fn host_kb_read_end_to_end_via_wasm() {
        use wasmtime::{Engine, Module, Store};

        let mut cfg = wasmtime::Config::new();
        cfg.consume_fuel(true);
        let engine = Engine::new(&cfg).unwrap();

        // The probe module: places the id "kb-a" at offset 0 and exports a
        // function that calls the imported `kb_read` to fetch that block.
        let probe_wat = r#"(module
            (import "nexus" "kb_read"
              (func $kb_read (param i32 i32 i32 i32) (result i64)))
            (memory (export "memory") 1)
            (data (i32.const 0) "kb-a")
            (func (export "probe_kb_read")
              (param $out_ptr i32) (param $out_cap i32) (result i64)
              (call $kb_read
                (i32.const 0) (i32.const 4)
                (local.get $out_ptr) (local.get $out_cap))))
        "#;
        let module = Module::new(&engine, probe_wat).unwrap();

        let limits = wasmtime::StoreLimitsBuilder::new().build();
        let input: ComputeInput = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "world_ref": {"world_id": "w1"},
                "key_blocks": [
                    {"schema_version":1,"key_block_id":"kb-a","world_id":"w1",
                     "block_type":"character","canonical_name":"Alice","status":"confirmed","created_at":"t"}
                ]
            }"#,
        )
        .unwrap();
        let mut store = Store::new(
            &engine,
            InvocationState {
                ctx: HostContext::from_input(&input),
                limits,
            },
        );
        store.limiter(|s| &mut s.limits);
        store.set_fuel(1_000_000).unwrap();

        let manifest: ModuleManifest =
            serde_json::from_str(r#"{"module_id":"probe","name":"P","version":"0.1.0","nexus_abi_version":1,"required_key_block_types":[],"compute_export":"compute","init_export":"","host_functions":["kb_read"]}"#).unwrap();

        let mut linker: Linker<InvocationState> = Linker::new(&engine);
        register_host_imports(&mut linker, &manifest).unwrap();

        let instance = linker.instantiate(&mut store, &module).unwrap();
        let probe = instance
            .get_typed_func::<(u32, u32), i64>(&mut store, "probe_kb_read")
            .unwrap();

        // out buffer well clear of the "kb-a" data segment at offset 0.
        let out_ptr: u32 = 32;
        let written = probe.call(&mut store, (out_ptr, 60_000)).unwrap();
        assert!(written > 0, "kb_read must find kb-a; got {written}");

        let memory = instance.get_memory(&mut store, "memory").unwrap();
        let len = usize::try_from(written).expect("written is non-negative");
        let mut buf = vec![0u8; len];
        memory.read(&store, out_ptr as usize, &mut buf).unwrap();
        let fetched: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(fetched["key_block_id"], "kb-a");
        assert_eq!(fetched["canonical_name"], "Alice");
    }

    /// A module that imports a non-whitelisted host function must FAIL to
    /// instantiate — that is the explicit enforcement of the whitelist.
    #[test]
    fn non_whitelisted_import_rejected_at_instantiation() {
        use wasmtime::{Engine, Module, Store};

        let mut cfg = wasmtime::Config::new();
        cfg.consume_fuel(true);
        let engine = Engine::new(&cfg).unwrap();
        let module = Module::new(
            &engine,
            r#"(module
                (import "nexus" "kb_read" (func (param i32 i32 i32 i32) (result i64)))
                (memory (export "memory") 1))"#,
        )
        .unwrap();

        let limits = wasmtime::StoreLimitsBuilder::new().build();
        let mut store = Store::new(
            &engine,
            InvocationState {
                ctx: HostContext::default(),
                limits,
            },
        );
        store.limiter(|s| &mut s.limits);

        // Manifest does NOT whitelist kb_read -> import is left undefined.
        let manifest: ModuleManifest = serde_json::from_str(
            r#"{"module_id":"m","name":"M","version":"0.1.0","nexus_abi_version":1,
               "required_key_block_types":[],"compute_export":"compute","init_export":""}"#,
        )
        .unwrap();
        let mut linker: Linker<InvocationState> = Linker::new(&engine);
        register_host_imports(&mut linker, &manifest).unwrap();

        let err = linker.instantiate(&mut store, &module).unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("kb_read") || msg.contains("import"),
            "expected import-not-satisfied error, got: {msg}"
        );
    }
}
