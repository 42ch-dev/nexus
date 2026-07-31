//! State delta merge semantics — shared between the preset path
//! (`narrative.compute`) and the direct Control Room lane (V1.147 P0).
//!
//! Implements incremental `add`/`sub`/`set` on nested state paths
//! (dot-notation, e.g. `character.current_hp`) within a `WorldKbEntry`'s
//! `body.state.<block_type_key>.<rest>`.
//!
//! # TX-aware variant
//!
//! `apply_state_delta_pool` uses a pool-backed `SqliteKbStore` (existing
//! pattern). `apply_state_delta_in_tx` operates inside a caller-owned
//! `sqlx::Transaction` using raw queries — needed for the Accept handler's
//! single-transaction atomic boundary.

use crate::capability::CapabilityError;
use nexus_contracts::generated::daemon_api::compute::compute_output::ComputeOutputStateDeltaItem as ComputeOutputStateDelta;
use nexus_knowledge::world_kb::knowledge_entry::{WorldKbBody, WorldKbEntry};
use nexus_knowledge::world_kb::KbStore;
use serde_json::{json, Value};
use sqlx::SqlitePool;

/// Valid `state_delta.op` variants recognized by merge operations.
pub const VALID_OPS: &[&str] = &["add", "sub", "set"];

/// Map the generated enum to a wire string.
#[must_use]
pub const fn state_delta_op_wire(
    op: nexus_contracts::generated::daemon_api::compute::compute_output::ComputeOutputStateDeltaItemOp,
) -> &'static str {
    use nexus_contracts::generated::daemon_api::compute::compute_output::ComputeOutputStateDeltaItemOp;
    match op {
        ComputeOutputStateDeltaItemOp::Add => "add",
        ComputeOutputStateDeltaItemOp::Sub => "sub",
        ComputeOutputStateDeltaItemOp::Set => "set",
    }
}

/// Apply a list of `ComputeOutputStateDelta` entries to the world's
/// `WorldKbEntry` bodies (pool-backed).  Used by `narrative.compute`.
///
/// Returns the number of state deltas successfully applied.
///
/// # Errors
///
/// Returns `CapabilityError::InputInvalid` for unknown ops, missing target
/// entries, path validation failures, or type mismatches.
/// Returns `CapabilityError::Internal` on DB write failures.
pub async fn apply_state_delta_pool(
    pool: &SqlitePool,
    deltas: &[ComputeOutputStateDelta],
) -> Result<usize, CapabilityError> {
    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
    let mut applied = 0usize;

    for delta in deltas {
        let op_wire = state_delta_op_wire(delta.op);
        if !VALID_OPS.contains(&op_wire) {
            return Err(CapabilityError::InputInvalid(format!(
                "unknown state_delta op '{op_wire}' (expected one of: {})",
                VALID_OPS.join(", ")
            )));
        }

        let target_id = delta.target_key_block_id.as_deref().unwrap_or("");
        if target_id.is_empty() {
            return Err(CapabilityError::InputInvalid(
                "state_delta entry missing target_key_block_id".to_string(),
            ));
        }

        let mut kb = kb_store.get_knowledge_entry(target_id).await.map_err(|e| {
            CapabilityError::InputInvalid(format!(
                "state_delta target '{target_id}' not found: {e}"
            ))
        })?;

        apply_single_delta(&mut kb, delta)?;

        kb_store
            .update_knowledge_entry(kb)
            .await
            .map_err(|e| CapabilityError::Internal(format!("kb update state: {e}")))?;

        applied += 1;
    }

    Ok(applied)
}

/// Apply a list of `ComputeOutputStateDelta` entries inside a caller-owned
/// `SQLite` transaction.  Used by the Accept handler (V1.147 P0 direct lane).
///
/// Only updates `kb_key_blocks.body_json` and `updated_at` — skips the
/// full `SqliteKbStore::update_knowledge_entry` validation (canonical name,
/// uniqueness, extensions) because the delta only mutates `body.state`.
///
/// Returns the number of state deltas successfully applied.
///
/// # Errors
///
/// Returns `CapabilityError::InputInvalid` for unknown ops, missing target
/// entries, or path validation failures.
/// Returns `CapabilityError::Internal` on DB read/write failures.
pub async fn apply_state_delta_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    deltas: &[ComputeOutputStateDelta],
) -> Result<usize, CapabilityError> {
    use chrono::Utc;
    use nexus_contracts::BlockType;

    let mut applied = 0usize;

    for delta in deltas {
        let op_wire = state_delta_op_wire(delta.op);
        if !VALID_OPS.contains(&op_wire) {
            return Err(CapabilityError::InputInvalid(format!(
                "unknown state_delta op '{op_wire}' (expected one of: {})",
                VALID_OPS.join(", ")
            )));
        }

        let target_id = delta.target_key_block_id.as_deref().unwrap_or("");
        if target_id.is_empty() {
            return Err(CapabilityError::InputInvalid(
                "state_delta entry missing target_key_block_id".to_string(),
            ));
        }

        // Read current block_type and body_json from the TX.
        // SAFETY: runtime query — nexus-orchestration does not have sqlx
        // offline mode configured for these queries. Tables are vetted
        // (kb_key_blocks columns from kb_store migration).
        let row =
            sqlx::query("SELECT block_type, body_json FROM kb_key_blocks WHERE key_block_id = ?")
                .bind(target_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(|e| CapabilityError::Internal(format!("kb read for delta: {e}")))?;

        let (block_type_str, body_json_str): (String, Option<String>) = match row {
            Some(r) => {
                use sqlx::Row;
                (r.get(0), r.get(1))
            }
            None => {
                return Err(CapabilityError::InputInvalid(format!(
                    "state_delta target '{target_id}' not found"
                )));
            }
        };

        let block_type: BlockType = serde_json::from_str(&format!("\"{block_type_str}\""))
            .map_err(|e| {
                CapabilityError::InputInvalid(format!(
                    "state_delta target '{target_id}': invalid block_type '{block_type_str}': {e}"
                ))
            })?;

        let mut body: WorldKbBody = body_json_str
            .as_deref()
            .map(|s| serde_json::from_str(s).unwrap_or_default())
            .unwrap_or_default();

        let mut state = body
            .state
            .take()
            .unwrap_or_else(|| Value::Object(serde_json::Map::default()));

        // Validate path: first segment is the block_type state key.
        let path_segments: Vec<&str> = delta.path.split('.').collect();
        if path_segments.is_empty() {
            return Err(CapabilityError::InputInvalid(
                "state_delta path must be non-empty (e.g. 'character.current_hp')".to_string(),
            ));
        }

        let expected_state_key =
            nexus_knowledge::world_kb::block_type_state_key(block_type).unwrap_or("unknown");
        let state_key = path_segments[0];
        if expected_state_key != "unknown" && state_key != expected_state_key {
            return Err(CapabilityError::InputInvalid(format!(
                "state_delta path key '{state_key}' does not match block_type '{block_type_str}' expected key '{expected_state_key}'",
            )));
        }

        let rest_path: Vec<&str> = path_segments[1..].to_vec();
        apply_json_delta(&mut state, state_key, &rest_path, op_wire, &delta.value)?;

        body.state = Some(state);

        let updated_body_json = serde_json::to_string(&body).map_err(|e| {
            CapabilityError::Internal(format!("serialize updated body for '{target_id}': {e}"))
        })?;

        let updated_at = Utc::now().to_rfc3339();
        // SAFETY: runtime query — see note above. Table is kb_key_blocks (vetted).
        let affected = sqlx::query(
            "UPDATE kb_key_blocks SET body_json = ?, updated_at = ? WHERE key_block_id = ?",
        )
        .bind(&updated_body_json)
        .bind(&updated_at)
        .bind(target_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            CapabilityError::Internal(format!("kb update state for '{target_id}': {e}"))
        })?;

        if affected.rows_affected() == 0 {
            return Err(CapabilityError::InputInvalid(format!(
                "state_delta target '{target_id}' disappeared during transaction"
            )));
        }

        applied += 1;
    }

    Ok(applied)
}

/// Apply a single delta to an in-memory `WorldKbEntry`.
/// Used by the pool-backed variant (which already has the full entry loaded).
fn apply_single_delta(
    kb: &mut WorldKbEntry,
    delta: &ComputeOutputStateDelta,
) -> Result<(), CapabilityError> {
    let op_wire = state_delta_op_wire(delta.op);

    let mut body = kb.body.take().unwrap_or_default();
    let mut state = body
        .state
        .take()
        .unwrap_or_else(|| Value::Object(serde_json::Map::default()));

    let path_segments: Vec<&str> = delta.path.split('.').collect();
    if path_segments.is_empty() {
        return Err(CapabilityError::InputInvalid(
            "state_delta path must be non-empty (e.g. 'character.current_hp')".to_string(),
        ));
    }

    let expected_state_key =
        nexus_knowledge::world_kb::block_type_state_key(kb.block_type).unwrap_or("unknown");
    let state_key = path_segments[0];
    if expected_state_key != "unknown" && state_key != expected_state_key {
        return Err(CapabilityError::InputInvalid(format!(
            "state_delta path key '{state_key}' does not match block_type '{:?}' expected key '{expected_state_key}'",
            kb.block_type
        )));
    }

    let rest_path: Vec<&str> = path_segments[1..].to_vec();
    apply_json_delta(&mut state, state_key, &rest_path, op_wire, &delta.value)?;

    body.state = Some(state);
    kb.body = Some(body);

    Ok(())
}

/// Apply a single value change at a JSON path inside the state object.
///
/// `state_key` is the top-level key inside the state map (e.g. `"character"`).
/// `rest_path` is the remaining path segments inside the `state_key` object.
///
/// # Errors
///
/// Returns `CapabilityError::InputInvalid` when the state is not a JSON object,
/// the state key is missing, nested segments are missing for `add`/`sub` ops,
/// or a target field is not numeric for `add`/`sub`.
pub fn apply_json_delta(
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
            "state key '{state_key}' not found in target WorldKbEntry state"
        ))
    })?;

    let inner_obj = inner.as_object_mut().ok_or_else(|| {
        CapabilityError::InputInvalid(format!("'state.{state_key}' must be a JSON object"))
    })?;

    let target_key = rest_path.last().copied().ok_or_else(|| {
        CapabilityError::InputInvalid("empty field path after state key".to_string())
    })?;

    let new_val = value.as_ref().unwrap_or(&Value::Null);

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
        apply_op_to_field(current, target_key, op, new_val)?;
    } else {
        apply_op_to_field(inner_obj, target_key, op, new_val)?;
    }

    Ok(())
}

/// Apply a single operation to a field in the state map.
///
/// Game state values (HP, ATK, DEF) are well within `i64`/`f64` safe
/// ranges; the precision-loss warnings from the casts below are
/// theoretical, not practical.
///
/// # Errors
///
/// Returns `CapabilityError::InputInvalid` when the field is not numeric for
/// `add`/`sub` ops, or when the op is unknown.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub fn apply_op_to_field(
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
    fn delta_add_on_non_numeric_errors() {
        let mut state = json!({"character": {"name": "Hero"}});
        let err = apply_json_delta(&mut state, "character", &["name"], "add", &Some(json!(1)))
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
        assert_eq!(state["character"]["current_hp"], 100);
        assert!(state["character"]["current_hp"].is_i64());
    }
}
