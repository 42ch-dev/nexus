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
//! v1.184 P4 adds [`atomic_cas_carrier_modules_and_insert_mind_state_in_tx`]:
//! one SQLite transaction CAS-patches the carrier `modules_json` and inserts
//! a validated derivative `MindState` row (both-or-neither).

use nexus_knowledge::world_kb::is_character_subject_id;
use nexus_local_db::kb_store::cas_update_key_block_modules_in_tx;
use nexus_local_db::mind_state_store;
use nexus_local_db::LocalDbError;
use serde_json::Value;
use spoke_operations::{validate_mind_state, SpokeResult};
use sqlx::{Sqlite, SqlitePool, Transaction};

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
    let columns = validated_mind_state_columns(value)?;
    mind_state_store::insert_mind_state(
        pool,
        columns.mind_state_id,
        columns.schema_version,
        columns.holder_entry_id,
        columns.canonical_name,
        columns.occurred_at,
        columns.sort_key,
        columns.snapshot_json.as_deref(),
        columns.deltas_json.as_deref(),
        columns.source_anchor_json.as_deref(),
        columns.extensions_json.as_deref(),
    )
    .await
}

/// Transaction-aware variant of [`validate_and_store_mind_state`].
pub async fn validate_and_store_mind_state_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    value: &Value,
) -> Result<(), LocalDbError> {
    let columns = validated_mind_state_columns(value)?;
    mind_state_store::insert_mind_state_in_tx(
        tx,
        columns.mind_state_id,
        columns.schema_version,
        columns.holder_entry_id,
        columns.canonical_name,
        columns.occurred_at,
        columns.sort_key,
        columns.snapshot_json.as_deref(),
        columns.deltas_json.as_deref(),
        columns.source_anchor_json.as_deref(),
        columns.extensions_json.as_deref(),
    )
    .await
}

/// Atomic Character ToM carrier write: CAS-patch `modules_json` on the
/// carrier KnowledgeEntry, then insert a spoke-validated derivative
/// `MindState` whose `holder_entry_id` equals the carrier id.
///
/// The CAS predicate revalidates non-deleted status and the admitted
/// Character/binding ownership (`owner_character_id` / `owner_binding_id`)
/// inside this transaction (QC fix round 1, F-004).
///
/// Callers own the outer `sqlx::Transaction` and must `commit()` only after
/// this returns `Ok`. Any CAS, product, validation, or insert error leaves
/// the transaction uncommitted so both writes roll back together.
///
/// # Errors
///
/// Returns [`LocalDbError::ValidationError`] for product invariants or spoke
/// rejections, [`LocalDbError::VersionMismatch`] on stale carrier revision,
/// and [`LocalDbError::Sqlx`] on database failure.
pub async fn atomic_cas_carrier_modules_and_insert_mind_state_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    carrier_entry_id: &str,
    expected_revision: i64,
    modules_json: &str,
    mind_state_wire: &Value,
    owner_character_id: &str,
    owner_binding_id: &str,
) -> Result<u64, LocalDbError> {
    assert_derivative_holder_is_carrier(carrier_entry_id, mind_state_wire)?;

    let new_revision = cas_update_key_block_modules_in_tx(
        tx,
        carrier_entry_id,
        modules_json,
        expected_revision,
        owner_character_id,
        owner_binding_id,
    )
    .await?;

    if let Err(e) = validate_and_store_mind_state_in_tx(tx, mind_state_wire).await {
        return Err(e);
    }

    Ok(new_revision)
}

fn assert_derivative_holder_is_carrier(
    carrier_entry_id: &str,
    mind_state_wire: &Value,
) -> Result<(), LocalDbError> {
    let holder = mind_state_wire
        .get("holder_entry_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if holder != carrier_entry_id {
        return Err(LocalDbError::ValidationError(format!(
            "mind_state holder_entry_id must equal carrier KnowledgeEntry id \
             (expected {carrier_entry_id}, got {holder})"
        )));
    }
    if is_character_subject_id(holder) {
        return Err(LocalDbError::ValidationError(
            "mind_state holder_entry_id must be a KnowledgeEntry carrier id, not a Character subject (chr_*)".into(),
        ));
    }
    Ok(())
}

struct MindStateColumnValues<'a> {
    mind_state_id: &'a str,
    schema_version: i64,
    holder_entry_id: &'a str,
    canonical_name: Option<&'a str>,
    occurred_at: Option<&'a str>,
    sort_key: Option<&'a str>,
    snapshot_json: Option<String>,
    deltas_json: Option<String>,
    source_anchor_json: Option<String>,
    extensions_json: Option<String>,
}

fn validated_mind_state_columns(value: &Value) -> Result<MindStateColumnValues<'_>, LocalDbError> {
    match validate_mind_state(value) {
        SpokeResult::Ok(()) => {}
        SpokeResult::Reject(reject) => {
            return Err(LocalDbError::ValidationError(format!(
                "mind_state rejected [{}]: {}",
                reject.code, reject.message
            )));
        }
    }

    let Some(state) = value.as_object() else {
        return Err(LocalDbError::ValidationError(
            "mind_state must be a non-null plain object".to_string(),
        ));
    };

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

    Ok(MindStateColumnValues {
        mind_state_id,
        schema_version,
        holder_entry_id,
        canonical_name,
        occurred_at,
        sort_key,
        snapshot_json,
        deltas_json,
        source_anchor_json,
        extensions_json,
    })
}
