//! `template-dice` — SDK hello-world demo module (dice tick).
//!
//! The scaffold for new Nexus compute modules (see `modules/README.md`).
//! It demonstrates the full `nexus-module-sdk` authoring surface:
//!
//! - [`nexus_entry!`] generates the three ABI exports (`alloc`, `init`,
//!   `compute`) and wires the global allocator — this crate declares **zero**
//!   `#[no_mangle]` code.
//! - The typed [`ComputeInput`] envelope is read directly; high-churn parts
//!   (`key_blocks`, `invocation`) pass through as `serde_json::Value`.
//! - The [`key_blocks`](nexus_module_sdk::key_blocks) accessors read the
//!   bundled key-block snapshot.
//! - The output envelope carries a `state_delta` on a **computable state
//!   path**, a `timeline_events` entry, and a `battle_report`.
//!
//! Semantics (a "dice tick"):
//!
//! 1. Take the first bundled key block as the tick target.
//! 2. Roll `1..=sides`, where `sides` comes from `invocation.sides` (default
//!    6). The roll is **deterministic**: wasm32 has no wall clock or RNG, so
//!    the roll is derived from a hash of the world id, the target block id,
//!    and the prior tick count — the same input always produces the same
//!    output, which is what keeps the host reproducible.
//! 3. Tick the block's computable state path `<block_type>.dice` (the module
//!    reads the prior tick count from the block state and writes the new
//!    `ticks` + `last_roll` in one `set` op — a single op that always applies,
//!    even on a block whose `dice` state does not exist yet).
//!
//! The `<block_type>` prefix is read from the block itself (`entry_type`,
//! legacy `block_type` fallback), so the module works against any block type
//! the author's world bundles.

use serde_json::{json, Value};

use nexus_module_sdk::key_blocks::{entry_id_of, read_int_f64, timeline_event_id};
use nexus_module_sdk::{
    nexus_entry, ComputeInput, ComputeOutput, DeltaOp, ModuleError, StateDeltaOp,
};

/// Default die sides when `invocation.sides` is absent.
const DEFAULT_SIDES: i64 = 6;

/// Computable state path fragment under the target block's state:
/// `<block_type>.dice`.
const DICE_STATE_KEY: &str = "dice";

/// Resolve the block's type name: spoke `entry_type` (canonical since
/// V1.139) with legacy domain `block_type` fallback.
fn block_type_of(kb: &Value) -> Option<&str> {
    kb.get("entry_type")
        .and_then(Value::as_str)
        .or_else(|| kb.get("block_type").and_then(Value::as_str))
}

/// Deterministic 64-bit FNV-1a over the roll seed (no RNG on wasm32).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Module entry (V1 fn form). [`nexus_entry!`] wires it to the `alloc` /
/// `init` / `compute` exports.
fn dice_tick(input: ComputeInput) -> Result<ComputeOutput, ModuleError> {
    // `required_key_block_types` selects what the host bundles; an empty
    // snapshot means the module cannot tick anything.
    let block = input
        .key_blocks
        .first()
        .ok_or(ModuleError::InputMalformed)?;
    let block_type = block_type_of(block).unwrap_or("unknown");
    let block_id = entry_id_of(block).unwrap_or("block").to_string();

    // Die sides: module-declared invocation param, default 6.
    let sides = input
        .invocation
        .get("sides")
        .and_then(Value::as_i64)
        .filter(|s| *s > 0)
        .unwrap_or(DEFAULT_SIDES);

    // Prior tick count seeds the roll. Canonical spoke shape nests state by
    // block type (`body.state.<block_type>.dice.ticks`); the legacy domain
    // shape keeps it flat (`body.state.dice.ticks`).
    let ticks = read_int_f64(
        block,
        &["body", "state", block_type, DICE_STATE_KEY, "ticks"],
    )
    .or_else(|| read_int_f64(block, &["body", "state", DICE_STATE_KEY, "ticks"]))
    .unwrap_or(0);
    let new_ticks = ticks + 1;

    let world_id = input.world_ref.world_id.as_deref().unwrap_or("world");
    let branch_id = input.world_ref.branch_id.as_deref().unwrap_or("root");
    let seed = format!("{world_id}:{block_id}:{ticks}");
    let roll = (fnv1a64(seed.as_bytes()) % sides as u64) as i64 + 1;

    // One `set` op on the computable state path `<block_type>.dice` — the
    // whole dice state lands atomically and applies even on a first tick
    // (the host rejects a delta into a missing intermediate object, so the
    // module writes the object wholesale instead of a nested field).
    let state_delta = vec![StateDeltaOp {
        op: DeltaOp::Set,
        path: format!("{block_type}.{DICE_STATE_KEY}"),
        target_key_block_id: Some(block_id.clone()),
        value: Some(json!({
            "ticks": new_ticks,
            "last_roll": roll,
        })),
    }];

    // Timeline event mirroring basic-combat's state_update shape; the host
    // stamps authoritative timestamps when it applies the event.
    let timeline_events = vec![json!({
        "schema_version": 1,
        "timeline_event_id": timeline_event_id("dice", &[&block_id]),
        "world_id": world_id,
        "branch_id": branch_id,
        "event_type": "state_update",
        "status": "canon",
        "sequence_no": 1,
        "title": "Dice rolled",
        "summary": format!("d{roll} rolled for {block_id} (tick {new_ticks})"),
        "affected_key_block_ids": [block_id.clone()],
        "created_at": "1970-01-01T00:00:00Z",
    })];

    let battle_report = json!({
        "kind": "dice",
        "block_id": block_id,
        "sides": sides,
        "roll": roll,
        "ticks": new_ticks,
    });

    Ok(ComputeOutput {
        schema_version: input.schema_version,
        state_delta,
        timeline_events,
        new_key_blocks: Vec::new(),
        battle_report,
    })
}

nexus_entry!(dice_tick);

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_module_sdk::WorldRef;

    /// Build a synthetic input: one character block with `dice.ticks` state.
    fn tick_input(world_id: &str, ticks: i64, sides: Option<i64>) -> ComputeInput {
        ComputeInput {
            schema_version: 1,
            world_ref: WorldRef {
                world_id: Some(world_id.to_string()),
                branch_id: None,
                timeline_head_event_id: None,
            },
            key_blocks: vec![json!({
                "entry_id": "kb-dice",
                "entry_type": "character",
                "body": {
                    "state": { "character": { "dice": { "ticks": ticks } } },
                    "attributes": [],
                },
            })],
            narrative_state: json!({}),
            invocation: sides.map_or_else(|| json!({}), |s| json!({ "sides": s })),
        }
    }

    #[test]
    fn rolls_deterministically_and_ticks_the_state_path() {
        let out1 = dice_tick(tick_input("wld_1", 3, None)).expect("tick succeeds");
        let out2 = dice_tick(tick_input("wld_1", 3, None)).expect("tick succeeds");

        // Same input → same roll (deterministic module).
        assert_eq!(out1.battle_report["roll"], out2.battle_report["roll"]);

        // The delta targets the computable state path and carries the new
        // dice state.
        assert_eq!(out1.state_delta.len(), 1);
        assert_eq!(out1.state_delta[0].op, DeltaOp::Set);
        assert_eq!(out1.state_delta[0].path, "character.dice");
        assert_eq!(
            out1.state_delta[0].target_key_block_id.as_deref(),
            Some("kb-dice")
        );
        let dice = out1.state_delta[0].value.as_ref().expect("dice state");
        assert_eq!(dice["ticks"], 4);
        let roll = dice["last_roll"].as_i64().expect("roll is an int");
        assert!((1..=6).contains(&roll));

        // Timeline event + report.
        assert_eq!(out1.timeline_events.len(), 1);
        assert_eq!(out1.battle_report["kind"], "dice");
        assert_eq!(out1.battle_report["roll"], json!(roll));
    }

    #[test]
    fn honors_invocation_sides() {
        let out = dice_tick(tick_input("wld_1", 0, Some(20))).expect("tick succeeds");
        let roll = out.battle_report["roll"].as_i64().expect("roll is an int");
        assert!((1..=20).contains(&roll));
        assert_eq!(out.battle_report["sides"], 20);
    }

    #[test]
    fn rejects_an_empty_key_block_snapshot() {
        let mut input = tick_input("wld_1", 0, None);
        input.key_blocks = Vec::new();
        assert!(matches!(dice_tick(input), Err(ModuleError::InputMalformed)));
    }
}
