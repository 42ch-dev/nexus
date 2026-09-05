//! Character pending-review storage (v1.184 P3 Task 1).
//!
//! Character counterpart to [`crate::pending_review`]: session-end capture
//! queue rows on the dedicated `character_memory_pending_review` table.
//! Rows key `character_id` only — authorization derives the owner from
//! `characters`. A non-null `actor_world_binding_id` marks binding-local
//! (one World life) provenance; the write path validates that the binding is
//! active, belongs to the same Character, and targets an owned active World
//! before any row is written.

use sqlx::SqlitePool;

use crate::actor_world_binding::{
    require_active_owned_provenance_pool, require_valid_provenance_tx,
};
use crate::character::{require_active_owned_character, require_owned_character_pool};
use crate::error::LocalDbError;
use crate::MAX_CHARACTER_MEMORY_LIST_LIMIT;

/// Character pending review record — mirrors DB row.
#[derive(Debug, Clone)]
pub struct CharacterPendingReviewRecord {
    /// Unique identifier for this pending entry.
    pub pending_id: String,
    /// ACP session ID that triggered the capture.
    pub session_id: String,
    /// Character ID for bearer ownership.
    pub character_id: String,
    /// Binding-local provenance; `None` = shared Character scope.
    pub actor_world_binding_id: Option<String>,
    /// Task kind heuristic (brainstorm, outline, chapter, research, unknown).
    pub task_kind: String,
    /// Raw digest extracted from session.
    pub raw_digest: String,
    /// Creation timestamp.
    pub created_at: String,
}

fn record_from_row(
    pending_id: String,
    session_id: String,
    character_id: String,
    actor_world_binding_id: Option<String>,
    task_kind: String,
    raw_digest: String,
    created_at: String,
) -> CharacterPendingReviewRecord {
    CharacterPendingReviewRecord {
        pending_id,
        session_id,
        character_id,
        actor_world_binding_id,
        task_kind,
        raw_digest,
        created_at,
    }
}


/// Create a Character pending review record (idempotent on retry).
///
/// Runs in a write-serialized transaction: validates Character ownership and
/// binding provenance first, so foreign Characters and invalid bindings
/// reject before any row is written. Insertion uses `INSERT OR IGNORE`, so a
/// duplicate `pending_id` (PK) or `(character_id, session_id)` (unique index)
/// is a no-op success rather than a constraint error — Creator capture parity.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` for foreign Characters or invalid
/// bindings; `LocalDbError` on database failure.
pub async fn create_character_pending_review(
    pool: &SqlitePool,
    owner_creator_id: &str,
    record: &CharacterPendingReviewRecord,
) -> Result<(), LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    let result = async {
        require_active_owned_character(&mut tx, owner_creator_id, &record.character_id).await?;
        require_valid_provenance_tx(
            &mut tx,
            owner_creator_id,
            &record.character_id,
            record.actor_world_binding_id.as_deref(),
        )
        .await?;
        // Creator-parity idempotent capture: `INSERT OR IGNORE` so retrying a
        // capture with the same `pending_id` (PK) or `(character_id,
        // session_id)` (unique index) is a no-op success rather than surfacing
        // a constraint error. Mirrors the Creator `memory_pending_review`
        // handler. No extra row/mutation is produced on a duplicate.
        sqlx::query!(
            "INSERT OR IGNORE INTO character_memory_pending_review
             (pending_id, session_id, character_id, actor_world_binding_id, task_kind, raw_digest, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            record.pending_id,
            record.session_id,
            record.character_id,
            record.actor_world_binding_id,
            record.task_kind,
            record.raw_digest,
            record.created_at
        )
        .execute(&mut *tx)
        .await?;
        Ok(())
    }
    .await;
    match result {
        Ok(()) => {
            tx.commit().await?;
            Ok(())
        }
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}

/// Get a Character pending review by ID, scoped to the owning Character and
/// to one binding scope.
///
/// `binding_id = None` reads the shared Character scope (`actor_world_binding_id
/// IS NULL`); `Some(b)` reads only that binding-local scope and requires the
/// exact active binding with an owned active World. A local row is therefore
/// never observable through another binding, another Character's binding, or
/// the shared scope.
///
/// Returns None if the record does not exist within that scope.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character (or, for a
/// binding-scoped read, the binding/World) is missing, foreign, inactive, or
/// not an active World; `LocalDbError` on database failure.
pub async fn get_character_pending_review(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: Option<&str>,
    pending_id: &str,
) -> Result<Option<CharacterPendingReviewRecord>, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    if let Some(binding_id) = binding_id {
        require_active_owned_provenance_pool(pool, owner_creator_id, character_id, binding_id)
            .await?;
    }
    let row = sqlx::query!(
        r#"SELECT pending_id as "pending_id!", session_id as "session_id!",
                  character_id as "character_id!", actor_world_binding_id,
                  task_kind as "task_kind!", raw_digest as "raw_digest!",
                  created_at as "created_at!"
           FROM character_memory_pending_review
           WHERE pending_id = ? AND character_id = ? AND actor_world_binding_id IS ?"#,
        pending_id,
        character_id,
        binding_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| {
        record_from_row(
            r.pending_id,
            r.session_id,
            r.character_id,
            r.actor_world_binding_id,
            r.task_kind,
            r.raw_digest,
            r.created_at,
        )
    }))
}

/// List a bounded page of pending reviews for a Character binding scope,
/// newest first.
///
/// `binding_id = None` lists the shared Character scope; `Some(b)` lists only
/// that binding-local scope and requires the exact active binding with an
/// owned active World. `limit` is clamped to `1..=MAX_CHARACTER_MEMORY_LIST_LIMIT`.
///
/// Results are ordered `created_at DESC, pending_id DESC` (a total order, so
/// offset pagination is deterministic); `offset` skips that many rows of the
/// ordered scope.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character (or, for a
/// binding-scoped read, the binding/World) is missing, foreign, inactive, or
/// not an active World; `LocalDbError` on database failure.
pub async fn list_character_pending_reviews(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<CharacterPendingReviewRecord>, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    if let Some(binding_id) = binding_id {
        require_active_owned_provenance_pool(pool, owner_creator_id, character_id, binding_id)
            .await?;
    }
    let limit = limit.clamp(1, MAX_CHARACTER_MEMORY_LIST_LIMIT);
    let offset = offset.max(0);
    let rows = sqlx::query!(
        r#"SELECT pending_id as "pending_id!", session_id as "session_id!",
                  character_id as "character_id!", actor_world_binding_id,
                  task_kind as "task_kind!", raw_digest as "raw_digest!",
                  created_at as "created_at!"
           FROM character_memory_pending_review
           WHERE character_id = ? AND actor_world_binding_id IS ?
           ORDER BY created_at DESC, pending_id DESC LIMIT ? OFFSET ?"#,
        character_id,
        binding_id,
        limit,
        offset
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            record_from_row(
                r.pending_id,
                r.session_id,
                r.character_id,
                r.actor_world_binding_id,
                r.task_kind,
                r.raw_digest,
                r.created_at,
            )
        })
        .collect())
}

/// Delete a Character pending review by ID, scoped to the owning Character.
///
/// Returns true if a record was deleted, false if it didn't exist.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character is missing or
/// owned by another Creator; `LocalDbError` on database failure.
pub async fn delete_character_pending_review(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    pending_id: &str,
) -> Result<bool, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    let result = sqlx::query!(
        "DELETE FROM character_memory_pending_review WHERE pending_id = ? AND character_id = ?",
        pending_id,
        character_id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete a Character pending review row inside a caller-owned transaction.
///
/// Ownership is asserted by the review pipeline's bearer context and by the
/// fragment insert in the same transaction, so this variant skips the
/// standalone ownership pre-check (PR #240 finding 2).
///
/// # Errors
///
/// Returns `LocalDbError` on database failure; `Ok(false)` when the row is
/// already gone.
pub async fn delete_character_pending_review_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    character_id: &str,
    pending_id: &str,
) -> Result<bool, LocalDbError> {
    let result = sqlx::query!(
        "DELETE FROM character_memory_pending_review WHERE pending_id = ? AND character_id = ?",
        pending_id,
        character_id
    )
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Count pending reviews for a Character binding scope.
///
/// `binding_id = None` counts the shared Character scope; `Some(b)` counts only
/// that binding-local scope and requires the exact active binding with an
/// owned active World.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character (or, for a
/// binding-scoped read, the binding/World) is missing, foreign, inactive, or
/// not an active World; `LocalDbError` on database failure.
///
/// # Panics
///
/// Panics if the count is negative (database invariant violation).
pub async fn count_character_pending_reviews(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: Option<&str>,
) -> Result<usize, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    if let Some(binding_id) = binding_id {
        require_active_owned_provenance_pool(pool, owner_creator_id, character_id, binding_id)
            .await?;
    }
    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "count!" FROM character_memory_pending_review
           WHERE character_id = ? AND actor_world_binding_id IS ?"#,
        character_id,
        binding_id
    )
    .fetch_one(pool)
    .await?;
    Ok(usize::try_from(count).expect("count is non-negative and fits in usize"))
}
