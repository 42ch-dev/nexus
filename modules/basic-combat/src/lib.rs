//! `basic-combat` — sample Nexus compute module.
//!
//! Implements the V1 compute envelope ABI (compass Q9): a **stateless pure
//! function** that resolves one attack between two characters with simple
//! `ATK − DEF` arithmetic. Triple role: integration test, ABI validation, and
//! reference implementation for module authors (see `modules/README.md`).
//!
//! Targets `wasm32-unknown-unknown` (no WASI). `std` is available on this
//! target for `Vec`/`String`/`format!`/serde; only I/O, threads, and the wall
//! clock are absent. Exports:
//!
//! | Export | Signature | Purpose |
//! | --- | --- | --- |
//! | `alloc` | `(len: u32) -> u32` | Allocate `len` bytes in linear memory for the host. |
//! | `init`  | `() -> ()` | One-shot setup (no-op here). |
//! | `compute` | `(in_ptr, in_len, out_ptr, out_cap: u32) -> i64` | Read `ComputeInput`, write `ComputeOutput`. |
//!
//! Combatants are read from the inline `key_blocks` snapshot (the canonical
//! path — the host always bundles the relevant blocks per the schema). The
//! optional `invocation.attacker_id` / `invocation.defender_id` select the two
//! combatants; otherwise the first two character blocks are used.

use std::format;
use std::ptr;
use std::string::ToString;
use std::vec::Vec;

use serde_json::{json, Value};

// Global allocator for wasm32-unknown-unknown (std provides none on this
// target). dlmalloc grows linear memory on demand; the host's memory cap bounds
// it.
#[global_allocator]
static A: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

// ===========================================================================
// ABI exports
// ===========================================================================

/// Allocate `len` bytes in linear memory and return the pointer. The host uses
/// this to place the input JSON and reserve an output buffer inside our memory.
///
/// Memory is intentionally leaked: each `compute()` call runs in a fresh
/// per-invocation instance (compass Q6), so the instance is discarded right
/// after the call — no long-lived leak.
#[no_mangle]
pub extern "C" fn alloc(len: u32) -> u32 {
    let mut buf: Vec<u8> = Vec::with_capacity(len as usize);
    let ptr_val = buf.as_mut_ptr() as u32;
    std::mem::forget(buf); // leak; host will read/write these bytes
    ptr_val
}

/// One-shot initialization. No-op for basic-combat.
#[no_mangle]
pub extern "C" fn init() {}

/// Run a single combat resolution.
///
/// Reads a `ComputeInput` JSON envelope from `[in_ptr, in_ptr+in_len)`, computes
/// the attack, and writes a 4-part `ComputeOutput` JSON envelope into
/// `[out_ptr, out_ptr+written)`. Returns `written`, or a negative sentinel on
/// failure (`-1` = malformed input / missing combatants, `-2` = output buffer
/// too small).
#[no_mangle]
pub extern "C" fn compute(in_ptr: u32, in_len: u32, out_ptr: u32, out_cap: u32) -> i64 {
    match resolve(in_ptr, in_len) {
        Ok(out_bytes) => {
            if out_bytes.len() > out_cap as usize {
                return -2;
            }
            // SAFETY: `out_ptr` points into our own linear memory, reserved by
            // the host via `alloc`. The ranges are non-overlapping (input and
            // output buffers are separate allocations).
            unsafe {
                ptr::copy_nonoverlapping(out_bytes.as_ptr(), out_ptr as *mut u8, out_bytes.len());
            }
            out_bytes.len() as i64
        }
        Err(()) => -1,
    }
}

// ===========================================================================
// Combat logic
// ===========================================================================

fn resolve(in_ptr: u32, in_len: u32) -> Result<Vec<u8>, ()> {
    let input = read_input(in_ptr, in_len)?;
    let output = run_combat(&input)?;
    serde_json::to_vec(&output).map_err(|_| ())
}

/// Read and parse the `ComputeInput` envelope from linear memory.
fn read_input(in_ptr: u32, in_len: u32) -> Result<Value, ()> {
    if in_len == 0 {
        return Err(());
    }
    // SAFETY: the host wrote exactly `in_len` bytes starting at `in_ptr` (an
    // address previously returned by our own `alloc`).
    let slice = unsafe { std::slice::from_raw_parts(in_ptr as *const u8, in_len as usize) };
    serde_json::from_slice(slice).map_err(|_| ())
}

/// Resolve a single attack between two characters and build the output envelope.
fn run_combat(input: &Value) -> Result<Value, ()> {
    let key_blocks = input
        .get("key_blocks")
        .and_then(Value::as_array)
        .ok_or(())?;
    if key_blocks.is_empty() {
        return Err(());
    }

    let (attacker, defender) = select_combatants(input, key_blocks)?;

    let attacker_id = attacker
        .get("key_block_id")
        .and_then(Value::as_str)
        .unwrap_or("attacker")
        .to_string();
    let defender_id = defender
        .get("key_block_id")
        .and_then(Value::as_str)
        .unwrap_or("defender")
        .to_string();

    let atk = read_int(attacker, &["body", "attributes", "base_atk"])
        .or_else(|| read_int(attacker, &["body", "base_atk"]))
        .unwrap_or(0);
    let def = read_int(defender, &["body", "attributes", "base_def"])
        .or_else(|| read_int(defender, &["body", "base_def"]))
        .unwrap_or(0);
    // current_hp: nested by block_type (compass Q5) -> body.state.character.current_hp
    let hp_before = read_int(defender, &["body", "state", "character", "current_hp"])
        .or_else(|| read_int(defender, &["body", "state", "current_hp"]))
        .or_else(|| read_int(defender, &["body", "attributes", "max_hp"]))
        .unwrap_or(0);

    let damage = (atk - def).max(0);
    let hp_after = (hp_before - damage).max(0);

    let world_id = input
        .get("world_ref")
        .and_then(|w| w.get("world_id"))
        .and_then(Value::as_str)
        .unwrap_or("world")
        .to_string();
    let branch_id = input
        .get("world_ref")
        .and_then(|w| w.get("branch_id"))
        .and_then(Value::as_str)
        .unwrap_or("root")
        .to_string();

    // --- state_delta -----------------------------------------------------
    let mut state_delta = Vec::new();
    state_delta.push(json!({
        "op": "sub",
        "path": "character.current_hp",
        "target_key_block_id": defender_id,
        "value": damage,
    }));
    if hp_after == 0 {
        state_delta.push(json!({
            "op": "set",
            "path": "character.is_alive",
            "target_key_block_id": defender_id,
            "value": false,
        }));
    }

    // --- timeline_events -------------------------------------------------
    let summary = format!(
        "{attacker} struck {defender} for {damage} ({hp_before} -> {hp_after} hp)",
        attacker = attacker_id,
        defender = defender_id,
        damage = damage,
        hp_before = hp_before,
        hp_after = hp_after,
    );
    let timeline_events = vec![json!({
        "schema_version": 1,
        "timeline_event_id": format!("tl-basic-combat-{attacker_id}-{defender_id}"),
        "world_id": world_id,
        "branch_id": branch_id,
        "event_type": "state_update",
        "status": "canon",
        "sequence_no": 1,
        "title": "Combat resolved",
        "summary": summary,
        "affected_key_block_ids": [attacker_id.clone(), defender_id.clone()],
        // Modules cannot read a wall clock on wasm32-unknown-unknown; the host
        // owns authoritative timestamps when it applies the event.
        "created_at": "1970-01-01T00:00:00Z",
    })];

    // --- battle_report ---------------------------------------------------
    let battle_report = json!({
        "kind": "combat",
        "attacker_id": attacker_id,
        "defender_id": defender_id,
        "damage": damage,
        "defender_hp_before": hp_before,
        "defender_hp_after": hp_after,
        "resolution": "atk_minus_def",
    });

    Ok(json!({
        "schema_version": 1,
        "state_delta": state_delta,
        "timeline_events": timeline_events,
        "new_key_blocks": [],
        "battle_report": battle_report,
    }))
}

/// Pick the attacker and defender KeyBlocks.
///
/// Honors `invocation.attacker_id` / `invocation.defender_id` when present;
/// otherwise falls back to the first two character-typed blocks.
fn select_combatants<'a>(
    input: &'a Value,
    key_blocks: &'a [Value],
) -> Result<(&'a Value, &'a Value), ()> {
    let inv = input.get("invocation");
    let want_attacker = inv
        .and_then(|i| i.get("attacker_id"))
        .and_then(Value::as_str);
    let want_defender = inv
        .and_then(|i| i.get("defender_id"))
        .and_then(Value::as_str);

    let find = |id: &str| {
        key_blocks
            .iter()
            .find(|kb| kb.get("key_block_id").and_then(Value::as_str) == Some(id))
    };

    if let (Some(a), Some(d)) = (want_attacker, want_defender) {
        if a == d {
            return Err(());
        }
        return match (find(a), find(d)) {
            (Some(att), Some(def)) => Ok((att, def)),
            _ => Err(()),
        };
    }

    // Fallback: first two character blocks.
    let mut chars = key_blocks
        .iter()
        .filter(|kb| kb.get("block_type").and_then(Value::as_str) == Some("character"));
    let attacker = chars.next().ok_or(())?;
    let defender = chars.next().ok_or(())?;
    Ok((attacker, defender))
}

/// Read a nested integer along a JSON path; returns `None` on any miss.
fn read_int(value: &Value, path: &[&str]) -> Option<i64> {
    let mut cur = value;
    for seg in path {
        cur = cur.get(*seg)?;
    }
    cur.as_i64()
}
