//! `MindState` when-axis storage (V1.164 P2 / l5-mind) — the `mind_states`
//! table.
//!
//! **Derivative storage, NOT a second authority (PD-3 / TL-3):** the
//! authoritative mental layer lives on the holder `KnowledgeEntry`'s
//! `modules.mental` / `modules.belief` (persisted via
//! `kb_key_blocks.modules_json`). This table stores temporal
//! snapshots/deltas — `occurred_at` is the when-axis placement.
//!
//! The write path is gated by spoke `validate_mind_state` (spoke-operations
//! 0.10.0, l5-mind capability): required fields (`schema_version`,
//! `mind_state_id`, `holder_entry_id`, `extensions`), the closed envelope
//! (no unknown properties), and `snapshot` / `deltas` / `source_anchor` /
//! timestamp types are enforced. A wire-shape rejection is surfaced as
//! [`LocalDbError::ValidationError`] with the spoke reject code/message and
//! nothing is persisted.
//!
//! Column naming mirrors the spoke `mind-state.schema.json` keys: wire
//! `snapshot` / `deltas` / `source_anchor` / `extensions` map to the
//! `*_json` columns; `schema_version` is stored as a plain `i64` (the wire
//! requires a non-zero unsigned integer).

use crate::LocalDbError;
use serde_json::Value;
use spoke_operations::{validate_mind_state, SpokeResult};
use sqlx::SqlitePool;

/// Row type matching the `mind_states` DDL
/// (`20260814000002_create_mind_states.sql`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MindStateRow {
    /// PK — spoke `MindState` `mind_state_id` (wire-validated non-empty string).
    pub mind_state_id: String,
    /// Spoke `MindState` `schema_version` (wire-validated integer >= 1).
    pub schema_version: i64,
    /// Holder `kb_key_blocks.key_block_id` — FK, `ON DELETE CASCADE`
    /// (the Rust `entry_id` field maps to this DDL column — TL-3).
    pub holder_entry_id: String,
    /// Optional canonical name for the holder at this snapshot.
    pub canonical_name: Option<String>,
    /// When-axis placement (validated date-time string when present).
    pub occurred_at: Option<String>,
    /// Optional sort key for ordering snapshots within a holder.
    pub sort_key: Option<String>,
    /// Serialized `snapshot` object (when present).
    pub snapshot_json: Option<String>,
    /// Serialized `deltas` array (when present).
    pub deltas_json: Option<String>,
    /// Serialized `source_anchor` object (when present).
    pub source_anchor_json: Option<String>,
    /// Row creation timestamp (wire `created_at` when present, else insert time).
    pub created_at: String,
    /// Row last-update timestamp (wire `updated_at` when present, else insert time).
    pub updated_at: String,
    /// Serialized `extensions` object (required by the wire validator).
    pub extensions_json: Option<String>,
}

/// Insert a `MindState` when-axis record from its wire envelope.
///
/// The envelope is gated by spoke `validate_mind_state` BEFORE anything
/// reaches the database — a rejection (required field missing, unknown
/// property, malformed `snapshot` / `deltas` / `source_anchor` / timestamp)
/// is surfaced as [`LocalDbError::ValidationError`] and no row is written.
///
/// Column mapping mirrors the spoke `mind-state.schema.json` keys: `snapshot`
/// / `deltas` / `source_anchor` / `extensions` are serialized into the
/// `*_json` columns; `created_at` / `updated_at` fall back to the current
/// UTC time when the envelope omits them.
///
/// # Errors
///
/// Returns [`LocalDbError::ValidationError`] when `validate_mind_state`
/// rejects the envelope, [`LocalDbError::Sqlx`] on database failure —
/// including a duplicate `mind_state_id` primary key and an unknown
/// `holder_entry_id` foreign key.
pub async fn insert_mind_state(pool: &SqlitePool, value: &Value) -> Result<(), LocalDbError> {
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
        .unwrap_or_default()
        .to_string();
    let holder_entry_id = state
        .get("holder_entry_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let canonical_name = state
        .get("canonical_name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let occurred_at = state
        .get("occurred_at")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let sort_key = state
        .get("sort_key")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let created_at = state
        .get("created_at")
        .and_then(Value::as_str)
        .map_or_else(|| chrono::Utc::now().to_rfc3339(), str::to_owned);
    let updated_at = state
        .get("updated_at")
        .and_then(Value::as_str)
        .map_or_else(|| chrono::Utc::now().to_rfc3339(), str::to_owned);
    let snapshot_json = state.get("snapshot").map(Value::to_string);
    let deltas_json = state.get("deltas").map(Value::to_string);
    let source_anchor_json = state.get("source_anchor").map(Value::to_string);
    let extensions_json = state.get("extensions").map(Value::to_string);

    sqlx::query!(
        "INSERT INTO mind_states (
            mind_state_id, schema_version, holder_entry_id, canonical_name,
            occurred_at, sort_key, snapshot_json, deltas_json, source_anchor_json,
            created_at, updated_at, extensions_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        mind_state_id,
        schema_version,
        holder_entry_id,
        canonical_name,
        occurred_at,
        sort_key,
        snapshot_json,
        deltas_json,
        source_anchor_json,
        created_at,
        updated_at,
        extensions_json,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch a single `MindState` row by `mind_state_id`.
///
/// Returns `Ok(None)` when no row matches.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn get_mind_state(
    pool: &SqlitePool,
    mind_state_id: &str,
) -> Result<Option<MindStateRow>, LocalDbError> {
    // `mind_state_id as "mind_state_id!"` — SQLite describes a TEXT PRIMARY
    // KEY as nullable (sqlx 0.8.6), the non-null coercion matches the field.
    let row = sqlx::query_as!(
        MindStateRow,
        "SELECT mind_state_id as \"mind_state_id!\", schema_version, holder_entry_id, \
                canonical_name, occurred_at, sort_key, snapshot_json, deltas_json, \
                source_anchor_json, created_at, updated_at, extensions_json \
         FROM mind_states \
         WHERE mind_state_id = ?",
        mind_state_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// List `MindState` rows for a holder, when-axis order.
///
/// Ordering is `occurred_at ASC, sort_key ASC` (chronological — the
/// `idx_mind_states_holder_occurred` index direction; `SQLite` places NULL
/// `occurred_at` first within a holder). Empty holder → `Ok(vec![])`.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn list_mind_states_by_holder(
    pool: &SqlitePool,
    holder_entry_id: &str,
) -> Result<Vec<MindStateRow>, LocalDbError> {
    let rows = sqlx::query_as!(
        MindStateRow,
        "SELECT mind_state_id as \"mind_state_id!\", schema_version, holder_entry_id, \
                canonical_name, occurred_at, sort_key, snapshot_json, deltas_json, \
                source_anchor_json, created_at, updated_at, extensions_json \
         FROM mind_states \
         WHERE holder_entry_id = ? \
         ORDER BY occurred_at ASC, sort_key ASC",
        holder_entry_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Delete a `MindState` row by `mind_state_id`.
///
/// Returns `true` when a row was deleted, `false` when no row matched.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn delete_mind_state(
    pool: &SqlitePool,
    mind_state_id: &str,
) -> Result<bool, LocalDbError> {
    let result = sqlx::query!(
        "DELETE FROM mind_states WHERE mind_state_id = ?",
        mind_state_id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
