//! Character pending-review storage (v1.184 P3 Task 1; v1.185 P3 run capture).
//!
//! Character counterpart to [`crate::pending_review`]: session-end capture
//! queue rows on the dedicated `character_memory_pending_review` table.
//! Rows key `character_id` only — authorization derives the owner from
//! `characters`. A non-null `actor_world_binding_id` marks binding-local
//! (one World life) provenance; the write path validates that the binding is
//! active, belongs to the same Character, and targets an owned active World
//! before any row is written.

use sqlx::{SqlitePool, Transaction};

use crate::actor_world_binding::require_valid_provenance_tx;
use crate::character::{require_active_owned_character_tx, require_owned_character_pool};
use crate::error::{ActorContractConflict, LocalDbError};
use crate::MAX_CHARACTER_MEMORY_LIST_LIMIT;

/// Reserved pending-id prefix for server-owned run capture rows.
pub const RUN_PENDING_ID_PREFIX: &str = "run_";

/// Character pending review record — mirrors DB row.
#[derive(Debug, Clone)]
pub struct CharacterPendingReviewRecord {
    pub pending_id: String,
    pub session_id: String,
    pub character_id: String,
    pub actor_world_binding_id: Option<String>,
    pub task_kind: String,
    pub raw_digest: String,
    pub created_at: String,
    pub source_operation_id: Option<String>,
}

/// Borrowed input for explicit run capture (§11.6).
#[derive(Debug, Clone, Copy)]
pub struct RunCaptureInput<'a> {
    pub operation_id: &'a str,
    pub session_id: &'a str,
    pub character_id: &'a str,
    pub binding_id: &'a str,
    pub pending_id: &'a str,
    pub raw_digest: &'a str,
    pub captured_at: &'a str,
    pub lifecycle_epoch: i64,
}

/// Immutable run-capture receipt row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCaptureReceipt {
    pub operation_id: String,
    pub session_id: String,
    pub character_id: String,
    pub binding_id: String,
    pub lifecycle_epoch: i64,
    pub pending_id: String,
    pub captured_at: String,
}

#[allow(clippy::too_many_arguments)] // row mapper mirrors SQL projection
const fn record_from_row(
    pending_id: String,
    session_id: String,
    character_id: String,
    actor_world_binding_id: Option<String>,
    task_kind: String,
    raw_digest: String,
    created_at: String,
    source_operation_id: Option<String>,
) -> CharacterPendingReviewRecord {
    CharacterPendingReviewRecord {
        pending_id,
        session_id,
        character_id,
        actor_world_binding_id,
        task_kind,
        raw_digest,
        created_at,
        source_operation_id,
    }
}

const fn receipt_from_row(
    operation_id: String,
    session_id: String,
    character_id: String,
    binding_id: String,
    lifecycle_epoch: i64,
    pending_id: String,
    captured_at: String,
) -> RunCaptureReceipt {
    RunCaptureReceipt {
        operation_id,
        session_id,
        character_id,
        binding_id,
        lifecycle_epoch,
        pending_id,
        captured_at,
    }
}

fn receipt_matches_input(receipt: &RunCaptureReceipt, input: &RunCaptureInput<'_>) -> bool {
    receipt.session_id == input.session_id
        && receipt.character_id == input.character_id
        && receipt.binding_id == input.binding_id
        && receipt.lifecycle_epoch == input.lifecycle_epoch
        && receipt.pending_id == input.pending_id
        && receipt.captured_at == input.captured_at
}

async fn fetch_run_capture_receipt(
    executor: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    operation_id: &str,
) -> Result<Option<RunCaptureReceipt>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT operation_id as "operation_id!", session_id as "session_id!",
                  character_id as "character_id!", binding_id as "binding_id!",
                  lifecycle_epoch as "lifecycle_epoch!", pending_id as "pending_id!",
                  captured_at as "captured_at!"
           FROM character_run_captures WHERE operation_id = ?"#,
        operation_id
    )
    .fetch_optional(executor)
    .await?;
    Ok(row.map(|r| {
        receipt_from_row(
            r.operation_id,
            r.session_id,
            r.character_id,
            r.binding_id,
            r.lifecycle_epoch,
            r.pending_id,
            r.captured_at,
        )
    }))
}

const fn run_capture_provenance_conflict() -> LocalDbError {
    LocalDbError::ActorContractConflict {
        code: ActorContractConflict::RunCaptureProvenanceConflict,
    }
}

const fn run_capture_scope_changed() -> LocalDbError {
    LocalDbError::ActorContractConflict {
        code: ActorContractConflict::RunCaptureScopeChanged,
    }
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db_err) => db_err.code().as_deref() == Some("2067"),
        _ => false,
    }
}

/// Create a Character pending review record (idempotent on retry).
///
/// # Errors
///
/// Returns `LocalDbError` on ownership, provenance, or database failure.
pub async fn create_character_pending_review(
    pool: &SqlitePool,
    owner_creator_id: &str,
    record: &CharacterPendingReviewRecord,
) -> Result<(), LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    let result = async {
        require_active_owned_character_tx(&mut tx, owner_creator_id, &record.character_id).await?;
        require_valid_provenance_tx(
            &mut tx,
            owner_creator_id,
            &record.character_id,
            record.actor_world_binding_id.as_deref(),
        )
        .await?;
        sqlx::query!(
            "INSERT OR IGNORE INTO character_memory_pending_review
             (pending_id, session_id, character_id, actor_world_binding_id, task_kind,
              raw_digest, created_at, source_operation_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, NULL)",
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

/// Atomically record one run capture receipt and enqueue its pending row.
///
/// # Errors
///
/// Returns `LocalDbError` on ownership, provenance, idempotency conflict, or database failure.
pub async fn capture_character_run(
    pool: &SqlitePool,
    owner_creator_id: &str,
    input: RunCaptureInput<'_>,
) -> Result<RunCaptureReceipt, LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    let result = capture_character_run_in_tx(&mut tx, owner_creator_id, input).await;
    match result {
        Ok(receipt) => {
            tx.commit().await?;
            Ok(receipt)
        }
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}

async fn capture_character_run_in_tx(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    owner_creator_id: &str,
    input: RunCaptureInput<'_>,
) -> Result<RunCaptureReceipt, LocalDbError> {
    if let Some(existing) = fetch_run_capture_receipt(&mut **tx, input.operation_id).await? {
        return if receipt_matches_input(&existing, &input) {
            Ok(existing)
        } else {
            Err(run_capture_provenance_conflict())
        };
    }

    let character =
        require_active_owned_character_tx(tx, owner_creator_id, input.character_id).await?;
    if character.lifecycle_epoch != input.lifecycle_epoch {
        return Err(run_capture_scope_changed());
    }
    require_valid_provenance_tx(
        tx,
        owner_creator_id,
        input.character_id,
        Some(input.binding_id),
    )
    .await?;

    let insert_receipt = sqlx::query!(
        "INSERT INTO character_run_captures
         (operation_id, session_id, character_id, binding_id, lifecycle_epoch, pending_id, captured_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        input.operation_id,
        input.session_id,
        input.character_id,
        input.binding_id,
        input.lifecycle_epoch,
        input.pending_id,
        input.captured_at
    )
    .execute(&mut **tx)
    .await;

    if let Err(err) = insert_receipt {
        if is_unique_violation(&err) {
            if let Some(existing) = fetch_run_capture_receipt(&mut **tx, input.operation_id).await?
            {
                return if receipt_matches_input(&existing, &input) {
                    Ok(existing)
                } else {
                    Err(run_capture_provenance_conflict())
                };
            }
        }
        return Err(LocalDbError::from(err));
    }

    sqlx::query!(
        "INSERT INTO character_memory_pending_review
         (pending_id, session_id, character_id, actor_world_binding_id, task_kind,
          raw_digest, created_at, source_operation_id)
         VALUES (?, ?, ?, ?, 'unknown', ?, ?, ?)",
        input.pending_id,
        input.session_id,
        input.character_id,
        input.binding_id,
        input.raw_digest,
        input.captured_at,
        input.operation_id
    )
    .execute(&mut **tx)
    .await?;

    Ok(receipt_from_row(
        input.operation_id.to_string(),
        input.session_id.to_string(),
        input.character_id.to_string(),
        input.binding_id.to_string(),
        input.lifecycle_epoch,
        input.pending_id.to_string(),
        input.captured_at.to_string(),
    ))
}

/// # Errors
///
/// Returns `LocalDbError` on ownership, provenance, or database failure.
pub async fn get_character_pending_review(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: Option<&str>,
    pending_id: &str,
) -> Result<Option<CharacterPendingReviewRecord>, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    if let Some(binding_id) = binding_id {
        crate::actor_world_binding::require_owned_binding_provenance_pool(
            pool,
            owner_creator_id,
            character_id,
            binding_id,
        )
        .await?;
    }
    let row = sqlx::query!(
        r#"SELECT pending_id as "pending_id!", session_id as "session_id!",
                  character_id as "character_id!", actor_world_binding_id,
                  task_kind as "task_kind!", raw_digest as "raw_digest!",
                  created_at as "created_at!", source_operation_id
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
            r.source_operation_id,
        )
    }))
}

/// # Errors
///
/// Returns `LocalDbError` on ownership, provenance, or database failure.
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
        crate::actor_world_binding::require_owned_binding_provenance_pool(
            pool,
            owner_creator_id,
            character_id,
            binding_id,
        )
        .await?;
    }
    let limit = limit.clamp(1, MAX_CHARACTER_MEMORY_LIST_LIMIT);
    let offset = offset.max(0);
    let rows = sqlx::query!(
        r#"SELECT pending_id as "pending_id!", session_id as "session_id!",
                  character_id as "character_id!", actor_world_binding_id,
                  task_kind as "task_kind!", raw_digest as "raw_digest!",
                  created_at as "created_at!", source_operation_id
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
                r.source_operation_id,
            )
        })
        .collect())
}

/// # Errors
///
/// Returns `LocalDbError` on ownership or database failure.
pub async fn delete_character_pending_review(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    pending_id: &str,
) -> Result<bool, LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    let result = async {
        require_active_owned_character_tx(&mut tx, owner_creator_id, character_id).await?;
        let deleted = sqlx::query!(
            "DELETE FROM character_memory_pending_review WHERE pending_id = ? AND character_id = ?",
            pending_id,
            character_id
        )
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;
        Ok(deleted)
    }
    .await;
    match result {
        Ok(deleted) => {
            tx.commit().await?;
            Ok(deleted)
        }
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}

/// # Errors
///
/// Returns `LocalDbError` on ownership or database failure.
pub async fn delete_character_pending_review_in_tx(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    owner_creator_id: &str,
    character_id: &str,
    pending_id: &str,
) -> Result<bool, LocalDbError> {
    require_active_owned_character_tx(tx, owner_creator_id, character_id).await?;
    let result = sqlx::query!(
        "DELETE FROM character_memory_pending_review WHERE pending_id = ? AND character_id = ?",
        pending_id,
        character_id
    )
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// # Errors
///
/// Returns `LocalDbError` on ownership, provenance, or database failure.
///
/// # Panics
///
/// Panics if the SQL `COUNT` result cannot fit in `usize` (should not occur).
pub async fn count_character_pending_reviews(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: Option<&str>,
) -> Result<usize, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    if let Some(binding_id) = binding_id {
        crate::actor_world_binding::require_owned_binding_provenance_pool(
            pool,
            owner_creator_id,
            character_id,
            binding_id,
        )
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
