//! `MindState` when-axis storage (V1.164 P2 / l5-mind) — the `mind_states`
//! table.
//!
//! **Derivative storage, NOT a second authority (PD-3 / TL-3):** the
//! authoritative mental layer lives on the holder `KnowledgeEntry`'s
//! `modules.mental` / `modules.belief` (persisted via
//! `kb_key_blocks.modules_json`). This table stores temporal
//! snapshots/deltas — `occurred_at` is the when-axis placement.
//!
//! **Pure storage (V1.164 P2 layering fix):** this module is spoke-free —
//! it persists raw column values and performs no wire-shape validation.
//! The spoke `validate_mind_state` gate lives at the adapter boundary
//! (`nexus-spoke-adapter::adapter::mind_state::validate_and_store_mind_state`
//! — the sole `spoke-operations` consumer, spec §8 dep-graph reversal /
//! entity-scope-model.md:219).
//!
//! Column naming mirrors the spoke `mind-state.schema.json` keys: wire
//! `snapshot` / `deltas` / `source_anchor` / `extensions` map to the
//! `*_json` columns; `schema_version` is stored as a plain `i64` (the wire
//! requires a non-zero unsigned integer). `created_at` / `updated_at` are
//! stamped by the store at insert time (RFC 3339).

use crate::LocalDbError;
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
    /// Row creation timestamp (store-stamped at insert).
    pub created_at: String,
    /// Row last-update timestamp (store-stamped at insert).
    pub updated_at: String,
    /// Serialized `extensions` object (required by the wire validator).
    pub extensions_json: Option<String>,
}

/// Insert a `MindState` when-axis record from raw column values.
///
/// Pure storage: no validation is performed here — the caller (the
/// adapter-boundary gate `validate_and_store_mind_state`) is responsible
/// for the spoke wire-shape check. `created_at` / `updated_at` are stamped
/// with the current UTC time (RFC 3339).
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure — including a
/// duplicate `mind_state_id` primary key and an unknown
/// `holder_entry_id` foreign key.
#[allow(clippy::too_many_arguments)] // raw column values — the full row shape
pub async fn insert_mind_state(
    pool: &SqlitePool,
    mind_state_id: &str,
    schema_version: i64,
    holder_entry_id: &str,
    canonical_name: Option<&str>,
    occurred_at: Option<&str>,
    sort_key: Option<&str>,
    snapshot_json: Option<&str>,
    deltas_json: Option<&str>,
    source_anchor_json: Option<&str>,
    extensions_json: Option<&str>,
) -> Result<(), LocalDbError> {
    let created_at = chrono::Utc::now().to_rfc3339();
    let updated_at = created_at.clone();

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
