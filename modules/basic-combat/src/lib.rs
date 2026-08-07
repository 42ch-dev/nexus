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
//!
//! # Key-block JSON shape
//!
//! Since V1.139, `ComputeInput.key_blocks` carries **spoke `KnowledgeEntry`
//! JSON** (id field `entry_id`, type field `entry_type`, `body.attributes` as
//! an ERC721-style array of `{trait_type, value}` items). This module reads
//! that canonical shape with fallbacks to the legacy domain `KeyBlock` shape
//! (`key_block_id` / `block_type` / flat `body.attributes.<trait>` object), so
//! the module-level fixtures in `crates/nexus-wasm-host/tests/basic_combat.rs`
//! (which hand-build domain-shaped input) keep working unchanged.

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

/// Emit a wire-valid `evt_*` timeline id (alphanumeric suffix only).
fn wire_timeline_event_id(attacker_id: &str, defender_id: &str) -> String {
    let a: String = attacker_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let d: String = defender_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    format!("evt_combat{a}{d}")
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

    let attacker_id = entry_id_of(attacker)
        .unwrap_or("attacker")
        .to_string();
    let defender_id = entry_id_of(defender)
        .unwrap_or("defender")
        .to_string();

    let atk = read_attr_int(attacker, "base_atk")
        .or_else(|| read_int(attacker, &["body", "base_atk"]))
        .unwrap_or(0);
    let def = read_attr_int(defender, "base_def")
        .or_else(|| read_int(defender, &["body", "base_def"]))
        .unwrap_or(0);
    // current_hp: nested by block_type (compass Q5) -> body.state.character.current_hp
    let hp_before = read_int(defender, &["body", "state", "character", "current_hp"])
        .or_else(|| read_int(defender, &["body", "state", "current_hp"]))
        .or_else(|| read_attr_int(defender, "max_hp"))
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
        "timeline_event_id": wire_timeline_event_id(&attacker_id, &defender_id),
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

    let find = |id: &str| key_blocks.iter().find(|kb| entry_id_of(kb) == Some(id));

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
    let mut chars = key_blocks.iter().filter(|kb| is_character(kb));
    let attacker = chars.next().ok_or(())?;
    let defender = chars.next().ok_or(())?;
    Ok((attacker, defender))
}

/// Resolve a key block's identity: spoke `entry_id` (canonical since V1.139)
/// with legacy domain `key_block_id` fallback.
fn entry_id_of(kb: &Value) -> Option<&str> {
    kb.get("entry_id")
        .and_then(Value::as_str)
        .or_else(|| kb.get("key_block_id").and_then(Value::as_str))
}

/// Whether a key block is a character: spoke `entry_type` (canonical since
/// V1.139) with legacy domain `block_type` fallback.
fn is_character(kb: &Value) -> bool {
    kb.get("entry_type")
        .and_then(Value::as_str)
        .or_else(|| kb.get("block_type").and_then(Value::as_str))
        == Some("character")
}

/// Read an integer attribute from a key block. Supports both the legacy
/// domain flat-object form (`body.attributes.base_atk`) and the canonical
/// spoke ERC721-array form (`body.attributes[].trait_type`/`value`).
///
/// Spoke attribute values round-trip as JSON floats (`20.0` — the spoke
/// `BodyAttributeValue` number variant is an f64, and the
/// `nexus-spoke-adapter` key-block conversion emits the flat-object form
/// from those float values), so BOTH branches accept integer and float
/// values.
fn read_attr_int(kb: &Value, trait_name: &str) -> Option<i64> {
    if let Some(v) = read_int(kb, &["body", "attributes", trait_name]) {
        return Some(v);
    }
    // Flat-object form carrying spoke round-tripped float values (`20.0`):
    // `as_i64` misses f64-backed numbers, so fall back to the float read.
    if let Some(v) = read_int_f64(kb, &["body", "attributes", trait_name]) {
        return Some(v);
    }
    let attrs = kb.get("body")?.get("attributes")?.as_array()?;
    attrs.iter().find_map(|item| {
        if item.get("trait_type").and_then(Value::as_str) == Some(trait_name) {
            item.get("value")
                .and_then(Value::as_i64)
                .or_else(|| item.get("value").and_then(Value::as_f64).map(|f| f as i64))
        } else {
            None
        }
    })
}

/// Read a nested integer along a JSON path; returns `None` on any miss.
fn read_int(value: &Value, path: &[&str]) -> Option<i64> {
    let mut cur = value;
    for seg in path {
        cur = cur.get(*seg)?;
    }
    cur.as_i64()
}

/// Read a nested number along a JSON path as an integer, accepting
/// f64-backed JSON numbers (`20.0`) that `as_i64` misses.
fn read_int_f64(value: &Value, path: &[&str]) -> Option<i64> {
    let mut cur = value;
    for seg in path {
        cur = cur.get(*seg)?;
    }
    cur.as_f64().map(|f| f as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `nexus-spoke-adapter` key-block conversion emits spoke attribute
    /// values as f64-backed JSON numbers in the FLAT form
    /// (`body.attributes.base_atk` = `20.0` — the spoke `BodyAttributeValue`
    /// number variant is an f64). `read_attr_int` must accept them on the
    /// flat path exactly like the array path already did (L2 review
    /// Minor-1: the flat branch previously read floats as 0 → damage 0).
    #[test]
    fn flat_object_attributes_accept_f64_values() {
        let kb = serde_json::json!({
            "entry_type": "character",
            "body": {
                "attributes": { "max_hp": 100.0, "base_atk": 20.0, "base_def": 5.0 },
            },
        });
        assert_eq!(read_attr_int(&kb, "base_atk"), Some(20));
        assert_eq!(read_attr_int(&kb, "max_hp"), Some(100));
        assert_eq!(read_attr_int(&kb, "base_def"), Some(5));
    }

    /// Integer-valued flat attributes keep working (legacy domain form).
    #[test]
    fn flat_object_attributes_accept_integer_values() {
        let kb = serde_json::json!({
            "entry_type": "character",
            "body": {
                "attributes": { "max_hp": 100, "base_atk": 20, "base_def": 5 },
            },
        });
        assert_eq!(read_attr_int(&kb, "base_atk"), Some(20));
    }

    /// The canonical ERC721-array form still accepts both int and float
    /// values (unchanged behavior — regression pin).
    #[test]
    fn array_attributes_accept_int_and_float_values() {
        let kb = serde_json::json!({
            "entry_type": "character",
            "body": {
                "attributes": [
                    { "trait_type": "base_atk", "value": 20 },
                    { "trait_type": "base_def", "value": 5.0 },
                ],
            },
        });
        assert_eq!(read_attr_int(&kb, "base_atk"), Some(20));
        assert_eq!(read_attr_int(&kb, "base_def"), Some(5));
    }
}
