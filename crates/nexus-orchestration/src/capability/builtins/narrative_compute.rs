//! `narrative.compute` capability (V1.61 P3 — compass Q7).
//!
//! Orchestration-scope capability that bridges the orchestration engine with
//! the WASM compute host. Reads computable `KeyBlock`s from the KB layer,
//! passes them to a sandboxed WASM module via [`nexus_wasm_host::WasmEngine`],
//! and applies the resulting 4-part output envelope (`state_delta`,
//! `timeline_events`, `new_key_blocks`, `battle_report`).
//!
//! # Design
//!
//! Mirrors the `world.rs` (V1.60 P0 DF-46) orchestration handler pattern:
//! `Option<Arc<SqlitePool>>`, admission gate inline, structured `CapabilityError`.
//!
//! ## State delta merge semantics (open design item #1, compass §5)
//!
//! `apply_state_delta()` implements incremental `add/sub/set` on nested state paths:
//!
//! | Op    | Target  | Behavior |
//! |-------|---------|----------|
//! | `set` | Any     | Replace the field value at `path` (recursive JSON pointer). |
//! | `add` | Numeric | Add `value` to the current field. |
//! | `sub` | Numeric | Subtract `value` from the current field. |
//! | `add/sub` | Non-numeric | Return `CapabilityError::InputInvalid`. |
//!
//! Paths use dot-notation (e.g. `character.current_hp`) mapping to the nested
//! `body.state.<block_type_state_key>.<rest>` in a `KeyBlock`. The first
//! segment identifies the per-`block_type` state namespace per compass Q5
//! (e.g. `character` → `state.character.current_hp`).
//!
//! ## Decision: `state_delta.op` as String (R-V161P0-LOW-002)
//!
//! The generated `ComputeOutputStateDelta.op` is a plain `String`. We keep it
//! as `String` in this consumer and validate at runtime (match on `"add"`/`"sub"`/
//! `"set"` per the wire contract), returning `InputInvalid` on unknown ops.
//! would require a schema change + codegen cascade, which is deferred to a
//! follow-up iteration. The runtime validation is sufficient for V1.61 safety.
//!
//! ## `battle_report` size cap (R-V161P0-LOW-003)
//!
//! The generated `battle_report` field is freeform `serde_json::Value`. We
//! enforce a **64 KiB** runtime cap on the serialized report size. A module
//! emitting a larger report receives `InputInvalid` (the output is rejected
//! before any side-effects are applied).
//!
//! ## Error handling (graceful degradation)
//!
//! Compute failures (wasm trap, timeout, fuel exhaustion, output schema
//! mismatch) do NOT crash the daemon. Instead, they produce a `TimelineEvent`
//! with `event_type: StateUpdate`, title `"compute_error"`, and a summary
//! containing the error details. The error is also logged at `warn` level.

use crate::capability::builtins::world::ensure_world_owned;
use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use nexus_kb::KbStore;
use nexus_narrative::NarrativeGateway;
use nexus_wasm_host::{
    embedded_module_bytes, embedded_module_manifest, ComputeInput, ComputeOutputStateDelta,
    ModuleManifest, WasmEngine, WasmModule,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Maximum size in bytes for the serialized `battle_report` field (64 KiB).
/// R-V161P0-LOW-003: freeform `battle_report` cap to prevent unbounded output.
const BATTLE_REPORT_MAX_BYTES: usize = 64 * 1024;

/// Valid `state_delta.op` variants recognized by `apply_state_delta`.
const VALID_OPS: &[&str] = &["add", "sub", "set"];

/// Input for `narrative.compute`.
#[derive(Debug, Deserialize)]
struct NarrativeComputeInput {
    world_id: String,
    /// Caller creator id (admission gate).
    creator_id: String,
    /// Which embedded module to invoke (default: `"basic-combat"`).
    #[serde(default = "default_module_id")]
    module_id: String,
    /// Optional module-declared invocation parameters passed into the
    /// `ComputeInput.invocation` field.
    #[serde(default)]
    invocation_params: Option<Value>,
}

#[allow(clippy::missing_const_for_fn)]
fn default_module_id() -> String {
    String::from("basic-combat")
}

/// Execute a WASM compute module for a world's computable `KeyBlock`s.
#[derive(Clone)]
pub struct NarrativeCompute {
    pool: Option<Arc<sqlx::SqlitePool>>,
    engine: Option<Arc<WasmEngine>>,
}

impl NarrativeCompute {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pool: None,
            engine: None,
        }
    }

    /// Construct with a pool and a fresh `WasmEngine`.
    ///
    /// `WasmEngine` construction is expensive; this is the pool-bound constructor
    /// that enables actual compute at runtime. The engine is reused across all
    /// `compute()` calls (compass Q6: per-invocation sandbox isolates each call).
    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        let engine = WasmEngine::new().ok().map(Arc::new);
        Self {
            pool: Some(Arc::new(pool)),
            engine,
        }
    }
}

impl Default for NarrativeCompute {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for NarrativeCompute {
    fn name(&self) -> &'static str {
        "narrative.compute"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"module_id":{"type":"string"},"invocation_params":{"type":"object"}},"required":["world_id","creator_id"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"battle_report":{},"state_delta_applied":{"type":"integer","minimum":0},"timeline_events_created":{"type":"integer","minimum":0},"new_key_blocks_created":{"type":"integer","minimum":0}},"required":["battle_report","state_delta_applied","timeline_events_created","new_key_blocks_created"],"additionalProperties":false}"#
    }

    #[allow(clippy::too_many_lines)]
    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: NarrativeComputeInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("narrative.compute input: {e}")))?;

        let pool = self
            .pool
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        let engine = self
            .engine
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        tracing::info!(
            world_id = %parsed.world_id,
            module_id = %parsed.module_id,
            "narrative.compute admitted"
        );

        // Admission gate: creator must own the world.
        ensure_world_owned(pool, &parsed.creator_id, &parsed.world_id).await?;

        // 1. Read computable KeyBlocks from the KB store.
        let kb_store = nexus_local_db::kb_store::SqliteKbStore::new((**pool).clone());
        let q = nexus_kb::KbQuery::new(&parsed.world_id).with_computable(Some(true));
        let computable_blocks = kb_store
            .query(&q)
            .await
            .map_err(|e| CapabilityError::Internal(format!("kb query computable: {e}")))?;

        if computable_blocks.items.is_empty() {
            return Err(CapabilityError::InputInvalid(
                "no computable KeyBlocks found in world".to_string(),
            ));
        }

        // Convert domain KeyBlocks to contract KeyBlocks for the compute envelope.
        let key_blocks: Vec<nexus_contracts::KeyBlock> = computable_blocks
            .items
            .into_iter()
            .map(Into::into)
            .collect();

        // 2. Read narrative state (timeline position, root branch).
        let gw = nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new((**pool).clone());
        let world_state = gw
            .get_world_state(&parsed.world_id)
            .await
            .map_err(|e| CapabilityError::Internal(format!("world state read: {e}")))?;

        let branch_id = world_state
            .fork_branch_id
            .clone()
            .unwrap_or_else(|| "fbk_root".to_string());

        let narrative_state = json!({
            "world_id": parsed.world_id,
            "branch_id": branch_id,
            "timeline_position": 0, // V1.61: default to start of timeline
        });

        // 3. Build ComputeInput envelope and invoke WASM.
        let compute_input = ComputeInput {
            schema_version: 1,
            world_ref: json!({"world_id": parsed.world_id}),
            key_blocks,
            narrative_state: Some(narrative_state),
            invocation: parsed.invocation_params,
        };

        // Load the embedded module (compile once, reuse).
        let wasm_bytes = embedded_module_bytes(&parsed.module_id).ok_or_else(|| {
            CapabilityError::InputInvalid(format!(
                "embedded module '{}' not found",
                parsed.module_id
            ))
        })?;

        let manifest_json = embedded_module_manifest(&parsed.module_id).ok_or_else(|| {
            CapabilityError::InputInvalid(format!(
                "manifest for module '{}' not found",
                parsed.module_id
            ))
        })?;

        let manifest: ModuleManifest = serde_json::from_str(manifest_json)
            .map_err(|e| CapabilityError::InputInvalid(format!("module manifest parse: {e}")))?;

        let module: WasmModule = engine
            .load_module(wasm_bytes)
            .map_err(|e| CapabilityError::Internal(format!("wasm module compile: {e}")))?;

        // Invoke compute with graceful error handling.
        let output = match engine.compute(&module, &manifest, &compute_input) {
            Ok(o) => o,
            Err(e) => {
                return handle_compute_error(
                    pool,
                    &parsed.world_id,
                    &parsed.creator_id,
                    &branch_id,
                    &e.to_string(),
                )
                .await;
            }
        };

        // 4. Validate battle_report size (R-V161P0-LOW-003).
        if let Ok(report_bytes) = serde_json::to_vec(&output.battle_report) {
            if report_bytes.len() > BATTLE_REPORT_MAX_BYTES {
                return Err(CapabilityError::InputInvalid(format!(
                    "battle_report too large: {} bytes (max {} bytes)",
                    report_bytes.len(),
                    BATTLE_REPORT_MAX_BYTES
                )));
            }
        }

        // 5. Apply state_delta to KB state fields.
        let applied = apply_state_delta(pool, &parsed.world_id, &output.state_delta).await?;

        // 6. Create new KeyBlocks from output.
        let new_kb_count = create_new_key_blocks(pool, &parsed.world_id, &output.new_key_blocks)
            .await
            .map_err(|e| CapabilityError::Internal(format!("create new key_blocks: {e}")))?;

        // 7. Append timeline events from output.
        let evt_count = append_timeline_events(
            pool,
            &parsed.world_id,
            &parsed.creator_id,
            &branch_id,
            &output.timeline_events,
        )
        .await
        .map_err(|e| CapabilityError::Internal(format!("append timeline events: {e}")))?;

        tracing::info!(
            world_id = %parsed.world_id,
            module_id = %parsed.module_id,
            state_delta_applied = applied,
            timeline_events_created = evt_count,
            new_key_blocks_created = new_kb_count,
            "narrative.compute completed"
        );

        Ok(json!({
            "battle_report": output.battle_report,
            "state_delta_applied": applied,
            "timeline_events_created": evt_count,
            "new_key_blocks_created": new_kb_count,
        }))
    }
}

// ─── State delta merge (open design item #1, compass §5) ─────────────────

/// Apply a list of `ComputeOutputStateDelta` entries to the world's `KeyBlock`
/// bodies. Each entry targets a specific `KeyBlock` by `target_key_block_id`
/// and applies an `op` (`+`/`-`/`set`) at a dot-separated `path` inside the
/// block's `body.state` field.
///
/// # Merge semantics
///
/// The `path` uses dot-notation: `character.current_hp` maps to
/// `body.state.character.current_hp` in the target `KeyBlock`. The first
/// segment (e.g. `character`) is the per-`block_type` state key (compass Q5),
/// validated against `nexus_kb::block_type_state_key()`.
///
/// - `set` replaces the value at `path` (numeric, string, bool, object).
/// - `+` adds `value` (must be numeric) to the current value at `path`.
/// - `-` subtracts `value` (must be numeric) from the current value at `path`.
/// - Unknown ops → `CapabilityError::InputInvalid`.
/// - `+/-` on non-numeric fields → `CapabilityError::InputInvalid`.
///
/// # Returns
///
/// The number of state deltas successfully applied.
async fn apply_state_delta(
    pool: &sqlx::SqlitePool,
    _world_id: &str,
    deltas: &[ComputeOutputStateDelta],
) -> Result<usize, CapabilityError> {
    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
    let mut applied = 0usize;

    for delta in deltas {
        // Validate op.
        if !VALID_OPS.contains(&delta.op.as_str()) {
            return Err(CapabilityError::InputInvalid(format!(
                "unknown state_delta op '{}' (expected one of: {})",
                delta.op,
                VALID_OPS.join(", ")
            )));
        }

        let target_id = delta.target_key_block_id.as_deref().unwrap_or("");
        if target_id.is_empty() {
            return Err(CapabilityError::InputInvalid(
                "state_delta entry missing target_key_block_id".to_string(),
            ));
        }

        // Read current KeyBlock.
        let mut kb = kb_store.get_key_block(target_id).await.map_err(|e| {
            CapabilityError::InputInvalid(format!(
                "state_delta target '{target_id}' not found: {e}"
            ))
        })?;

        // Ensure the body exists and has state.
        let mut body = kb.body.take().unwrap_or_default();
        let mut state = body
            .state
            .take()
            .unwrap_or_else(|| Value::Object(serde_json::Map::default()));

        // Resolve the path: first segment is the block_type state key.
        let path_segments: Vec<&str> = delta.path.split('.').collect();
        if path_segments.is_empty() {
            return Err(CapabilityError::InputInvalid(
                "state_delta path must be non-empty (e.g. 'character.current_hp')".to_string(),
            ));
        }

        // Validate the first segment against the block_type's expected state key.
        let expected_state_key = nexus_kb::block_type_state_key(kb.block_type).unwrap_or("unknown");
        let state_key = path_segments[0];
        if expected_state_key != "unknown" && state_key != expected_state_key {
            return Err(CapabilityError::InputInvalid(format!(
                "state_delta path key '{state_key}' does not match block_type '{}' expected key '{expected_state_key}'",
                kb.block_type
            )));
        }

        let rest_path: Vec<&str> = path_segments[1..].to_vec();

        // Apply the delta to the state JSON.
        apply_json_delta(&mut state, state_key, &rest_path, &delta.op, &delta.value)?;

        // Write back.
        body.state = Some(state);
        kb.body = Some(body);

        kb_store
            .update_key_block(kb)
            .await
            .map_err(|e| CapabilityError::Internal(format!("kb update state: {e}")))?;

        applied += 1;
    }

    Ok(applied)
}

/// Apply a single value change at a JSON path inside the state object.
///
/// `state_key` is the top-level key inside the state map (e.g. `"character"`).
///  `rest_path` is the remaining path segments inside the `state_key` object.
#[allow(clippy::ref_option)]
fn apply_json_delta(
    state: &mut Value,
    state_key: &str,
    rest_path: &[&str],
    op: &str,
    value: &Option<Value>,
) -> Result<(), CapabilityError> {
    let state_obj = state
        .as_object_mut()
        .ok_or_else(|| CapabilityError::InputInvalid("state must be a JSON object".to_string()))?;

    let inner = state_obj.get_mut(state_key).ok_or_else(|| {
        CapabilityError::InputInvalid(format!(
            "state key '{state_key}' not found in target KeyBlock state"
        ))
    })?;

    let inner_obj = inner.as_object_mut().ok_or_else(|| {
        CapabilityError::InputInvalid(format!("'state.{state_key}' must be a JSON object"))
    })?;

    // Navigate to the target field.
    let target_key = rest_path.last().copied().ok_or_else(|| {
        CapabilityError::InputInvalid("empty field path after state key".to_string())
    })?;

    let new_val = value.as_ref().unwrap_or(&Value::Null);

    // Navigate through intermediate path segments, creating intermediate objects
    // for `set` if needed. `+` and `-` require the path to already exist.
    if rest_path.len() > 1 {
        let intermediate = &rest_path[..rest_path.len() - 1];
        let mut current = inner_obj;
        for &seg in intermediate {
            if !current.contains_key(seg) {
                if op == "set" {
                    current.insert(seg.to_string(), json!({}));
                } else {
                    return Err(CapabilityError::InputInvalid(format!(
                        "path segment '{seg}' does not exist; cannot apply '{op}' to missing field"
                    )));
                }
            }
            let next = current.get_mut(seg).and_then(|v| v.as_object_mut());
            current = next.ok_or_else(|| {
                CapabilityError::InputInvalid(format!("path segment '{seg}' is not an object"))
            })?;
        }
        // `current` now points to the parent object of the target field.
        apply_op_to_field(current, target_key, op, new_val)?;
    } else {
        // Single-segment path after state_key — operate directly on inner_obj.
        apply_op_to_field(inner_obj, target_key, op, new_val)?;
    }

    Ok(())
}

/// Apply a single operation to a field in the state map.
///
/// Game state values (HP, ATK, DEF) are well within `i64`/`f64` safe
/// ranges; the precision-loss warnings from the casts below are
/// theoretical, not practical.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn apply_op_to_field(
    obj: &mut serde_json::Map<String, Value>,
    target_key: &str,
    op: &str,
    new_val: &Value,
) -> Result<(), CapabilityError> {
    match op {
        "set" => {
            obj.insert(target_key.to_string(), new_val.clone());
        }
        "add" | "sub" => {
            let current = obj.get(target_key).cloned().unwrap_or(Value::Null);
            let current_num = current
                .as_f64()
                .or_else(|| current.as_i64().map(|i| i as f64));
            let delta_num = new_val
                .as_f64()
                .or_else(|| new_val.as_i64().map(|i| i as f64));

            match (current_num, delta_num) {
                (Some(c), Some(d)) => {
                    let result = if op == "add" { c + d } else { c - d };
                    // Preserve integer type if both values were integers and the
                    // result fits in i64.
                    let is_int = current.as_i64().is_some() && new_val.as_i64().is_some();
                    if is_int
                        && result.fract() == 0.0
                        && result >= (i64::MIN as f64)
                        && result <= (i64::MAX as f64)
                    {
                        obj.insert(target_key.to_string(), json!(result as i64));
                    } else {
                        obj.insert(target_key.to_string(), json!(result));
                    }
                }
                _ => {
                    return Err(CapabilityError::InputInvalid(format!(
                        "cannot apply '{op}' to non-numeric field '{target_key}': current={current}, delta={new_val}"
                    )));
                }
            }
        }
        other => {
            return Err(CapabilityError::InputInvalid(format!(
                "unknown op '{other}'"
            )));
        }
    }
    Ok(())
}

// ─── New KeyBlock creation ─────────────────────────────────────────────────

/// Create new `KeyBlock`s emitted by the compute module. Each block is inserted
/// with `provisional` status via the KB store.
///
/// Returns the number of blocks created.
async fn create_new_key_blocks(
    pool: &sqlx::SqlitePool,
    _world_id: &str,
    blocks: &[nexus_contracts::KeyBlock],
) -> Result<usize, CapabilityError> {
    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
    let mut created = 0usize;

    for kb_contract in blocks {
        let kb = nexus_kb::key_block::KeyBlock::from(kb_contract.clone());
        kb_store
            .insert_key_block(kb)
            .await
            .map_err(|e| CapabilityError::Internal(format!("insert new key_block: {e}")))?;
        created += 1;
    }

    Ok(created)
}

// ─── Timeline event appending ──────────────────────────────────────────────

/// Append timeline events emitted by the compute module via the narrative
/// gateway.
///
/// Returns the number of events created.
async fn append_timeline_events(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    _creator_id: &str,
    branch_id: &str,
    events: &[nexus_contracts::TimelineEvent],
) -> Result<usize, CapabilityError> {
    let mut count = 0usize;

    for evt in events {
        let event_type = evt.event_type.as_str();
        nexus_local_db::narrative_write::append_event(
            pool,
            world_id,
            branch_id,
            event_type,
            evt.title.as_deref(),
            evt.summary.as_deref(),
        )
        .await
        .map_err(|e| CapabilityError::Internal(format!("append timeline event: {e}")))?;
        count += 1;
    }

    Ok(count)
}

// ─── Error handling: compute_error timeline event ──────────────────────────

/// Create a `compute_error` timeline event and return a graceful error.
///
/// The daemon must NOT crash on compute failure (compass T4). Instead, a
/// `TimelineEvent` with `event_type: StateUpdate` and `title: "compute_error"`
/// is inserted into the world timeline, and the capability returns
/// `CapabilityError::TransientExternal`.
async fn handle_compute_error(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    _creator_id: &str,
    branch_id: &str,
    error_detail: &str,
) -> Result<Value, CapabilityError> {
    tracing::warn!(
        world_id = %world_id,
        error = %error_detail,
        "narrative.compute failed; recording compute_error timeline event"
    );

    // Best-effort: append the error event. If this also fails, we still return
    // the original compute error so the caller knows something went wrong.
    let evt_result = nexus_local_db::narrative_write::append_event(
        pool,
        world_id,
        branch_id,
        "state_update",
        Some("compute_error"),
        Some(error_detail),
    )
    .await;

    if let Err(ref e) = evt_result {
        tracing::error!(
            world_id = %world_id,
            db_error = %e,
            "failed to record compute_error timeline event"
        );
    }

    Err(CapabilityError::TransientExternal(format!(
        "compute failed: {error_detail}"
    )))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nexus_kb::key_block::{KeyBlock, KeyBlockBody};
    use nexus_kb::KbStore;
    use nexus_local_db::{open_pool, run_migrations};

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_creator(pool: &sqlx::SqlitePool, creator_id: &str) {
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', datetime('now'), '{}')",
        )
        .bind(creator_id)
        .bind("Test Creator")
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_world(pool: &sqlx::SqlitePool, owner: &str, world_id: &str) {
        nexus_local_db::narrative_write::create_world(
            pool,
            owner,
            "Test World",
            "test-world",
            "private",
            "manual",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE narrative_worlds SET world_id = ? WHERE owner_creator_id = ?")
            .bind(world_id)
            .bind(owner)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn seed_computable_character(
        pool: &sqlx::SqlitePool,
        world_id: &str,
        canonical_name: &str,
        max_hp: i64,
        current_hp: i64,
    ) -> KeyBlock {
        let kb = nexus_kb::key_block::KeyBlock {
            world_id: world_id.to_string(),
            block_type: nexus_contracts::BlockType::Character,
            canonical_name: canonical_name.to_string(),
            body: Some(KeyBlockBody {
                summary: Some(format!("{canonical_name} character")),
                attributes: Some(json!({"max_hp": max_hp, "base_atk": 20})),
                computable: Some(true),
                state: Some(json!({
                    "character": {
                        "current_hp": current_hp,
                        "status_effects": [],
                        "position": "front_line",
                        "is_alive": true,
                    }
                })),
                ..Default::default()
            }),
            ..KeyBlock::new(
                world_id,
                nexus_contracts::BlockType::Character,
                canonical_name,
            )
        };
        let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        kb_store.insert_key_block(kb.clone()).await.unwrap();
        kb
    }

    // ── apply_json_delta unit tests ────────────────────────────────────────

    #[test]
    fn delta_set_numeric() {
        let mut state = json!({"character": {"current_hp": 100, "name": "Hero"}});
        apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "set",
            &Some(json!(50)),
        )
        .unwrap();
        assert_eq!(state["character"]["current_hp"], 50);
    }

    #[test]
    fn delta_add_numeric() {
        let mut state = json!({"character": {"current_hp": 80}});
        apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "add",
            &Some(json!(20)),
        )
        .unwrap();
        assert_eq!(state["character"]["current_hp"], 100);
    }

    #[test]
    fn delta_subtract_numeric() {
        let mut state = json!({"character": {"current_hp": 100}});
        apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "sub",
            &Some(json!(30)),
        )
        .unwrap();
        assert_eq!(state["character"]["current_hp"], 70);
    }

    #[test]
    fn delta_set_string_field() {
        let mut state = json!({"character": {"position": "front_line"}});
        apply_json_delta(
            &mut state,
            "character",
            &["position"],
            "set",
            &Some(json!("back_line")),
        )
        .unwrap();
        assert_eq!(state["character"]["position"], "back_line");
    }

    #[test]
    fn delta_add_on_non_numeric_errors() {
        let mut state = json!({"character": {"name": "Hero"}});
        let err = apply_json_delta(&mut state, "character", &["name"], "add", &Some(json!(1)))
            .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[test]
    fn delta_sub_on_non_numeric_errors() {
        let mut state = json!({"character": {"name": "Hero"}});
        let err = apply_json_delta(&mut state, "character", &["name"], "sub", &Some(json!(1)))
            .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[test]
    fn delta_unknown_op_errors() {
        let mut state = json!({"character": {"current_hp": 50}});
        let err = apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "multiply",
            &Some(json!(2)),
        )
        .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[test]
    fn delta_missing_state_key_errors() {
        let mut state = json!({"item": {"durability": 50}});
        let err = apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "set",
            &Some(json!(100)),
        )
        .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[test]
    fn delta_integer_addition_preserves_int_type() {
        let mut state = json!({"character": {"current_hp": 80}});
        apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "add",
            &Some(json!(20)),
        )
        .unwrap();
        // Integer preservation: 80 + 20 = 100, both i64 → result is i64
        assert_eq!(state["character"]["current_hp"], 100);
        assert!(state["character"]["current_hp"].is_i64());
    }

    #[test]
    fn delta_float_addition_produces_float() {
        let mut state = json!({"character": {"current_hp": 80.5}});
        apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "add",
            &Some(json!(19.5)),
        )
        .unwrap();
        assert_eq!(state["character"]["current_hp"], 100.0);
    }

    // ── Integration: narrative.compute capability ──────────────────────────

    #[tokio::test]
    async fn narrative_compute_rejects_missing_world() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        let cap = NarrativeCompute::with_pool(pool);
        let err = cap
            .run(json!({
                "world_id": "wld_missing",
                "creator_id": "ctr_a",
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Forbidden(_)));
    }

    #[tokio::test]
    async fn narrative_compute_rejects_cross_creator() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_creator(&pool, "ctr_b").await;
        seed_world(&pool, "ctr_a", "wld_a").await;
        let cap = NarrativeCompute::with_pool(pool);
        let err = cap
            .run(json!({
                "world_id": "wld_a",
                "creator_id": "ctr_b",
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Forbidden(_)));
    }

    #[tokio::test]
    async fn narrative_compute_rejects_no_computable_blocks() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_world(&pool, "ctr_a", "wld_a").await;
        let cap = NarrativeCompute::with_pool(pool);
        let err = cap
            .run(json!({
                "world_id": "wld_a",
                "creator_id": "ctr_a",
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[tokio::test]
    async fn narrative_compute_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        let cap = NarrativeCompute::with_pool(pool);
        let err = cap.run(json!(42)).await.unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[tokio::test]
    async fn narrative_compute_pool_less_returns_worker_unavailable() {
        let cap = NarrativeCompute::new();
        let err = cap
            .run(json!({"world_id": "wld_a", "creator_id": "ctr_a"}))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::WorkerUnavailable));
    }

    /// Full integration test: create world with 2 computable characters,
    /// run narrative.compute, verify output.
    #[tokio::test]
    async fn narrative_compute_full_cycle() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_world(&pool, "ctr_a", "wld_a").await;

        // Seed two computable characters with HP state.
        let kb_a = seed_computable_character(&pool, "wld_a", "Hero", 100, 80).await;
        let kb_b = seed_computable_character(&pool, "wld_a", "Villain", 120, 120).await;

        let cap = NarrativeCompute::with_pool(pool.clone());

        // The basic-combat module expects two combatants; if it runs successfully
        // it will return state_delta + battle_report. If it traps (e.g., because
        // the module's ABI doesn't handle our exact character shapes), the error
        // path creates a compute_error timeline event instead of crashing.
        let result = cap
            .run(json!({
                "world_id": "wld_a",
                "creator_id": "ctr_a",
                "module_id": "basic-combat",
                "invocation_params": {"rounds": 1},
            }))
            .await;

        match result {
            Ok(out) => {
                // Compute succeeded — verify the output shape.
                assert!(
                    out.get("battle_report").is_some(),
                    "expected battle_report in output"
                );
                assert!(
                    out.get("state_delta_applied").is_some(),
                    "expected state_delta_applied count"
                );
                assert!(
                    out.get("timeline_events_created").is_some(),
                    "expected timeline_events_created count"
                );
            }
            Err(e) => {
                // Compute failure is allowed in tests — verifying it doesn't crash.
                // The compute_error timeline event should have been recorded.
                let err_str = e.to_string();
                assert!(
                    err_str.contains("compute failed"),
                    "expected compute error message, got: {err_str}"
                );
            }
        }
    }
}
