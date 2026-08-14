//! `MindState` write-path gate — the spec-conformant home of the spoke
//! `validate_mind_state` wire-shape check (V1.164 P2 layering fix).
//!
//! Layering (spec §8 dep-graph reversal): `nexus-local-db` is pure storage
//! with no `spoke-operations` dependency; this adapter module is the **sole**
//! consumer of `spoke_operations::validate_mind_state` for the `mind_states`
//! table (entity-scope-model.md:219 — nexus-spoke-adapter is the only crate
//! that directly depends on spoke-operations). The gate validates the wire
//! envelope, maps the validated fields onto the store's raw column
//! parameters, and delegates persistence to
//! [`nexus_local_db::mind_state_store`] — which stays spoke-free.
//!
//! A wire-shape rejection surfaces as [`LocalDbError::ValidationError`]
//! carrying the spoke reject code/message; nothing reaches the database.

use nexus_local_db::mind_state_store;
use nexus_local_db::LocalDbError;
use serde_json::Value;
use spoke_operations::{validate_mind_state, SpokeResult};
use sqlx::SqlitePool;

/// Validate a `MindState` wire envelope against spoke `validate_mind_state`
/// and persist it when accepted.
///
/// # Flow
///
/// 1. `spoke_operations::validate_mind_state(value)` — required fields
///    (`schema_version`, `mind_state_id`, `holder_entry_id`, `extensions`),
///    the closed envelope (no unknown properties), and `snapshot` /
///    `deltas` / `source_anchor` / timestamp types. A `SpokeReject` aborts
///    with [`LocalDbError::ValidationError`] before any DB write.
/// 2. Map the validated envelope fields to the store's raw column
///    parameters: wire `snapshot` / `deltas` / `source_anchor` /
///    `extensions` serialize into the `*_json` columns; `schema_version`
///    is passed as a plain `i64`.
/// 3. [`mind_state_store::insert_mind_state`] — the store stamps
///    `created_at` / `updated_at` itself (RFC 3339).
///
/// # Errors
///
/// Returns [`LocalDbError::ValidationError`] when the spoke validator
/// rejects the envelope, [`LocalDbError::Sqlx`] on database failure —
/// including a duplicate `mind_state_id` primary key and an unknown
/// `holder_entry_id` foreign key.
pub async fn validate_and_store_mind_state(
    pool: &SqlitePool,
    value: &Value,
) -> Result<(), LocalDbError> {
    match validate_mind_state(value) {
        SpokeResult::Ok(()) => {}
        SpokeResult::Reject(reject) => {
            return Err(LocalDbError::ValidationError(format!(
                "mind_state rejected [{}]: {}",
                reject.code, reject.message
            )));
        }
    }

    // The gate above guarantees a non-null plain object; keep this branch
    // fail-closed (defensive) instead of panicking on an impossible shape.
    let Some(state) = value.as_object() else {
        return Err(LocalDbError::ValidationError(
            "mind_state must be a non-null plain object".to_string(),
        ));
    };

    // `schema_version` was validated as an integer >= 1; the f64 → i64 cast
    // is exact for whole numbers in the validated range.
    #[allow(clippy::cast_possible_truncation)]
    let schema_version = state
        .get("schema_version")
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as i64;
    let mind_state_id = state
        .get("mind_state_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let holder_entry_id = state
        .get("holder_entry_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let canonical_name = state.get("canonical_name").and_then(Value::as_str);
    let occurred_at = state.get("occurred_at").and_then(Value::as_str);
    let sort_key = state.get("sort_key").and_then(Value::as_str);
    let snapshot_json = state.get("snapshot").map(Value::to_string);
    let deltas_json = state.get("deltas").map(Value::to_string);
    let source_anchor_json = state.get("source_anchor").map(Value::to_string);
    let extensions_json = state.get("extensions").map(Value::to_string);

    mind_state_store::insert_mind_state(
        pool,
        mind_state_id,
        schema_version,
        holder_entry_id,
        canonical_name,
        occurred_at,
        sort_key,
        snapshot_json.as_deref(),
        deltas_json.as_deref(),
        source_anchor_json.as_deref(),
        extensions_json.as_deref(),
    )
    .await
}
