//! Workspace session persistence (DF-31 full, V1.56 P0).
//!
//! DB-backed session store for `workspace.open` / `workspace.commit`.
//! Sessions are persisted in `SQLite`, survive daemon restarts, and expire per TTL.

use crate::LocalDbError;
use sqlx::SqlitePool;

/// A row in the `workspace_sessions` table.
#[derive(Debug, Clone)]
pub struct WorkspaceSessionRow {
    pub session_id: String,
    pub workspace_root: String,
    pub relative_path: String,
    pub existed: bool,
    /// JSON object of `{ relative_path: sha256_hex }` for tracked files.
    pub file_hashes_json: String,
    pub created_at: String,
    pub expires_at: String,
    pub consumed: bool,
}

/// Parameters for creating a new workspace session.
#[derive(Debug, Clone)]
pub struct CreateSessionParams {
    pub session_id: String,
    pub workspace_root: String,
    pub relative_path: String,
    pub existed: bool,
    /// JSON object of `{ relative_path: sha256_hex }` for tracked files.
    pub file_hashes_json: String,
    /// TTL in seconds from creation time.
    pub ttl_secs: i64,
}

/// Result of consuming a session.
#[derive(Debug, Clone)]
pub enum ConsumeResult {
    /// Session was consumed successfully.
    Consumed(WorkspaceSessionRow),
    /// Session not found.
    NotFound,
    /// Session was already consumed (stale).
    AlreadyConsumed,
    /// Session has expired.
    Expired,
}

/// Create a new workspace session in the database.
///
/// # Errors
///
/// Returns `LocalDbError` on database failure.
pub async fn create_session(
    pool: &SqlitePool,
    params: &CreateSessionParams,
) -> Result<WorkspaceSessionRow, LocalDbError> {
    let existed_int = i32::from(params.existed);
    // Store timestamps in RFC 3339 format for consistency with chrono.
    // SAFETY: compile-time checked — schema defined in migration 202606220002_workspace_sessions.sql
    sqlx::query!(
        "INSERT INTO workspace_sessions \
         (session_id, workspace_root, relative_path, existed, file_hashes_json, created_at, expires_at, consumed) \
         VALUES (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ? || ' seconds'), 0)",
        params.session_id,
        params.workspace_root,
        params.relative_path,
        existed_int,
        params.file_hashes_json,
        params.ttl_secs,
    )
    .execute(pool)
    .await?;

    get_session(pool, &params.session_id)
        .await?
        .ok_or_else(|| LocalDbError::Sqlx(sqlx::Error::RowNotFound))
}

/// Get a session by ID.
///
/// Returns `None` if the session does not exist.
///
/// # Errors
///
/// Returns `LocalDbError` on database failure.
pub async fn get_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<WorkspaceSessionRow>, LocalDbError> {
    // SAFETY: compile-time checked — reads all columns from workspace_sessions table.
    let row = sqlx::query!(
        "SELECT session_id, workspace_root, relative_path, existed, file_hashes_json, \
         created_at, expires_at, consumed \
         FROM workspace_sessions WHERE session_id = ?",
        session_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        // session_id is TEXT PRIMARY KEY NOT NULL — guaranteed non-null by schema.
        // Explicit unwrap is safe; clippy::missing_panics_doc is waived because
        // this is a schema invariant, not a runtime condition.
        #[allow(clippy::missing_panics_doc)]
        let session_id = r.session_id.unwrap_or_else(|| unreachable!("session_id is PRIMARY KEY NOT NULL"));
        WorkspaceSessionRow {
            session_id,
            workspace_root: r.workspace_root,
            relative_path: r.relative_path,
            existed: r.existed != 0,
            file_hashes_json: r.file_hashes_json,
            created_at: r.created_at,
            expires_at: r.expires_at,
            consumed: r.consumed != 0,
        }
    }))
}

/// Consume a session atomically — marks it as consumed if it is still active and unexpired.
///
/// This is the key OCC primitive: validates that the session exists, is not consumed,
/// and is not expired, then marks it consumed in a single atomic operation.
///
/// # Errors
///
/// Returns `LocalDbError` on database failure. Returns variant-specific `ConsumeResult`
/// for logical failures (not found, already consumed, expired).
pub async fn consume_session(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<ConsumeResult, LocalDbError> {
    // First, get the session to check its state
    let Some(session) = get_session(pool, session_id).await? else {
        return Ok(ConsumeResult::NotFound);
    };

    if session.consumed {
        return Ok(ConsumeResult::AlreadyConsumed);
    }

    // Check expiry using SQLite datetime comparison with RFC 3339 format.
    // Both expires_at and strftime output are in RFC 3339 format.
    let active_count = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM workspace_sessions \
         WHERE session_id = ? AND consumed = 0 AND expires_at > strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        session_id
    )
    .fetch_one(pool)
    .await?;

    if active_count == 0 {
        // Re-read to determine the reason
        let Some(session) = get_session(pool, session_id).await? else {
            return Ok(ConsumeResult::NotFound);
        };
        if session.consumed {
            return Ok(ConsumeResult::AlreadyConsumed);
        }
        return Ok(ConsumeResult::Expired);
    }

    // Atomically mark consumed — only succeeds if still unconsumed and unexpired.
    // Uses strftime for RFC 3339 format consistency with the stored expires_at value.
    // SAFETY: compile-time checked — UPDATE with timestamp comparison.
    let result = sqlx::query!(
        "UPDATE workspace_sessions SET consumed = 1 \
         WHERE session_id = ? AND consumed = 0 AND expires_at > strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        session_id
    )
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        // Another process consumed it between our read and write, or it expired.
        // Re-read to determine the reason.
        let Some(session) = get_session(pool, session_id).await? else {
            return Ok(ConsumeResult::NotFound);
        };
        if session.consumed {
            return Ok(ConsumeResult::AlreadyConsumed);
        }
        return Ok(ConsumeResult::Expired);
    }

    // Re-read to get the updated row
    let consumed_session = get_session(pool, session_id)
        .await?
        .ok_or_else(|| LocalDbError::Sqlx(sqlx::Error::RowNotFound))?;
    Ok(ConsumeResult::Consumed(consumed_session))
}

/// Clean up expired sessions.
///
/// # Errors
///
/// Returns `LocalDbError` on database failure.
pub async fn cleanup_expired_sessions(pool: &SqlitePool) -> Result<u64, LocalDbError> {
    // SAFETY: compile-time checked — DELETE with RFC 3339 timestamp comparison.
    let result = sqlx::query!(
        "DELETE FROM workspace_sessions WHERE expires_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')"
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Count active (unconsumed + unexpired) sessions.
///
/// # Errors
///
/// Returns `LocalDbError` on database failure.
pub async fn count_active_sessions(pool: &SqlitePool) -> Result<i64, LocalDbError> {
    // SAFETY: compile-time checked — COUNT with RFC 3339 timestamp comparison.
    let row = sqlx::query!(
        "SELECT COUNT(*) as count FROM workspace_sessions \
         WHERE consumed = 0 AND expires_at > strftime('%Y-%m-%dT%H:%M:%SZ', 'now')"
    )
    .fetch_one(pool)
    .await?;
    Ok(row.count)
}
