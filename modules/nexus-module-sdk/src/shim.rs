//! ABI export shims (AR-2): `alloc`, `init`, `compute`.
//!
//! These are the bodies behind the exports [`nexus_entry!`](crate::nexus_entry)
//! generates. Module authors never call them directly.

use crate::error::{write_output, ModuleError};
use crate::types::ComputeInput;
use crate::NexusModule;

/// Allocate `len` bytes in linear memory and return the pointer. The host uses
/// this to place the input JSON and reserve an output buffer inside our memory.
///
/// Memory is intentionally leaked: each `compute()` call runs in a fresh
/// per-invocation instance (compass Q6), so the instance is discarded right
/// after the call — no long-lived leak.
pub fn alloc(len: u32) -> u32 {
    let mut buf: Vec<u8> = Vec::with_capacity(len as usize);
    let ptr_val = buf.as_mut_ptr() as u32;
    std::mem::forget(buf); // leak; host will read/write these bytes
    ptr_val
}

/// One-shot initialization. No-op for V1 modules (the
/// [`NexusModule::init`] default is a no-op too); the export exists so the
/// host's `init_export` contract is satisfied.
pub fn init() {}

/// Run a single compute invocation.
///
/// Reads a `ComputeInput` JSON envelope from `[in_ptr, in_ptr+in_len)`,
/// delegates to `module`, and writes the `ComputeOutput` JSON envelope into
/// `[out_ptr, out_ptr+written)`. Returns `written`, or a negative sentinel on
/// failure (`-1` = malformed input / serialization failure / host error,
/// `-2` = output buffer too small).
pub fn compute<N: NexusModule>(
    in_ptr: u32,
    in_len: u32,
    out_ptr: u32,
    out_cap: u32,
    module: N,
) -> i64 {
    match compute_inner(in_ptr, in_len, out_ptr, out_cap, &module) {
        Ok(written) => written as i64,
        Err(e) => e.to_compute_return(),
    }
}

fn compute_inner<N: NexusModule>(
    in_ptr: u32,
    in_len: u32,
    out_ptr: u32,
    out_cap: u32,
    module: &N,
) -> Result<usize, ModuleError> {
    if in_len == 0 {
        return Err(ModuleError::InputMalformed);
    }
    // SAFETY: the host wrote exactly `in_len` bytes starting at `in_ptr` (an
    // address previously returned by our own `alloc`).
    let slice = unsafe { std::slice::from_raw_parts(in_ptr as *const u8, in_len as usize) };
    let bytes = run(slice, module)?;
    write_output(out_ptr, out_cap, &bytes).map_err(|_| ModuleError::OutputTooSmall)?;
    Ok(bytes.len())
}

/// Pure invocation pipeline: deserialize → delegate → serialize.
///
/// Kept separate from the pointer marshalling so the SDK's host-target unit
/// tests can exercise the full pipeline without linear-memory addresses
/// (u32 pointers are only meaningful on wasm32; a host pointer truncated to
/// u32 is invalid memory).
fn run<N: NexusModule>(input_bytes: &[u8], module: &N) -> Result<Vec<u8>, ModuleError> {
    if input_bytes.is_empty() {
        return Err(ModuleError::InputMalformed);
    }
    let input: ComputeInput =
        serde_json::from_slice(input_bytes).map_err(|_| ModuleError::InputMalformed)?;
    let output = module.compute(input)?;
    serde_json::to_vec(&output).map_err(|_| ModuleError::SerializeFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_module(input: ComputeInput) -> Result<crate::ComputeOutput, ModuleError> {
        Ok(crate::ComputeOutput {
            schema_version: input.schema_version,
            state_delta: vec![],
            timeline_events: vec![],
            new_key_blocks: vec![],
            battle_report: json!({ "kind": "test" }),
        })
    }

    fn input_bytes() -> Vec<u8> {
        serde_json::to_vec(&json!({
            "schema_version": 1,
            "world_ref": {"world_id": "w", "branch_id": "root", "timeline_head_event_id": "evt_0"},
            "key_blocks": []
        }))
        .unwrap()
    }

    #[test]
    fn run_round_trips_envelope() {
        let out = run(&input_bytes(), &test_module).expect("run succeeds");
        let output: crate::ComputeOutput = serde_json::from_slice(&out).unwrap();
        assert_eq!(output.schema_version, 1);
        assert_eq!(output.battle_report["kind"], "test");
    }

    #[test]
    fn run_rejects_empty_input() {
        let err = run(b"", &test_module).expect_err("empty input must fail");
        assert_eq!(err, ModuleError::InputMalformed);
    }

    #[test]
    fn run_rejects_malformed_input() {
        let err = run(b"not json", &test_module).expect_err("parse failure must fail");
        assert_eq!(err, ModuleError::InputMalformed);
    }

    /// The pointer-based `compute` entry must reject an empty input without
    /// touching memory (host-safe check of the sentinel path; the full
    /// round-trip through linear memory is exercised by the real-host
    /// integration tests on wasm32).
    #[test]
    fn compute_rejects_empty_input_without_touching_memory() {
        let ret = compute(0, 0, 0, 0, test_module);
        assert_eq!(ret, -1, "empty input must map to InputMalformed (-1)");
    }
}
