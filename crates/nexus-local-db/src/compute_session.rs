//! Compute session persistence (V1.146 P2 T2).
//!
//! Bridges spoke's stateless `ProjectRequest`/`ComputeRequest` pair to nexus's
//! stateless WASM host: `project()` stages computable state; `compute()` reads
//! staged state, merges dynamic computable updates, builds `ComputeInput`,
//! fires the WASM engine, and optionally settles `state_delta` back into the
//! target entry. This module provides the minimal CRUD primitives the adapter
//! consumes.

use crate::LocalDbError;
use sqlx::SqlitePool;

/// A row in the `compute_sessions` table.
#[derive(Debug, Clone)]
pub struct ComputeSessionRow {
    pub session_id: String,
    pub entry_id: String,
    /// `state_json` is the serialized `ComputableFieldMap` (JSON object).
    /// NULL when no static state has been staged.
    pub state_json: Option<String>,
    pub created_at: String,
}

/// Insert a new compute session row.
///
/// # Errors
/// Returns `LocalDbError` on database failure or duplicate `session_id`.
pub async fn insert_compute_session(
    pool: &SqlitePool,
    session_id: &str,
    entry_id: &str,
    state_json: &str,
) -> Result<ComputeSessionRow, LocalDbError> {
    let created_at = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "INSERT INTO compute_sessions (session_id, entry_id, state_json, created_at) \
         VALUES (?, ?, ?, ?)",
        session_id,
        entry_id,
        state_json,
        created_at,
    )
    .execute(pool)
    .await?;
    Ok(ComputeSessionRow {
        session_id: session_id.to_string(),
        entry_id: entry_id.to_string(),
        state_json: Some(state_json.to_string()),
        created_at,
    })
}

/// Look up a compute session by `session_id`.
///
/// Returns `Ok(None)` when the session does not exist.
///
/// # Errors
/// Returns `LocalDbError` on database failure.
pub async fn get_compute_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<ComputeSessionRow>, LocalDbError> {
    let row = sqlx::query!(
        "SELECT session_id, entry_id, state_json, created_at \
         FROM compute_sessions WHERE session_id = ?",
        session_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ComputeSessionRow {
        session_id: r.session_id.expect("session_id is PRIMARY KEY"),
        entry_id: r.entry_id,
        state_json: r.state_json,
        created_at: r.created_at,
    }))
}

/// Update the `state_json` column of an existing compute session.
///
/// # Errors
/// Returns `LocalDbError` on database failure.
pub async fn update_compute_session_state(
    pool: &SqlitePool,
    session_id: &str,
    state_json: &str,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        "UPDATE compute_sessions SET state_json = ? WHERE session_id = ?",
        state_json,
        session_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a compute session by `session_id`. Safe to call on a non-existent
/// session (no-op).
///
/// # Errors
/// Returns `LocalDbError` on database failure.
pub async fn delete_compute_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<(), LocalDbError> {
    sqlx::query!("DELETE FROM compute_sessions WHERE session_id = ?", session_id)
        .execute(pool)
        .await?;
    Ok(())
}
