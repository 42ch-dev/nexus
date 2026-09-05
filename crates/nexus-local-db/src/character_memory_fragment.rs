//! Character memory fragment storage (v1.184 P3 Task 1).
//!
//! Character counterpart to [`crate::memory_fragment`]: keyword-indexed
//! fragments on the dedicated `character_memory_fragments` table. Rows key
//! `character_id` only — authorization derives the owner from `characters`.
//! A non-null `actor_world_binding_id` marks binding-local provenance;
//! shared Character memory is `NULL`.
//!
//! `revision` backs the OCC guard on explicit local→shared promotion, which
//! clears provenance in place (preserving `fragment_id`) and invalidates
//! only the two affected narrative cache scopes.

use sqlx::SqlitePool;

use crate::actor_world_binding::{
    require_active_owned_provenance_pool, require_valid_provenance_tx,
};
use crate::cas::cas_check_with_version_column;
use crate::character::{require_active_owned_character, require_owned_character_pool};
use crate::error::{ActorContractConflict, LocalDbError};
use crate::MAX_CHARACTER_MEMORY_LIST_LIMIT;

/// Character memory fragment row (storage shape; not a wire DTO).
#[derive(Debug, Clone)]
pub struct CharacterMemoryFragmentRecord {
    /// Unique identifier for this fragment.
    pub fragment_id: String,
    /// ACP session ID that generated this fragment.
    pub session_id: String,
    /// Character ID for bearer ownership.
    pub character_id: String,
    /// Binding-local provenance; `None` = shared Character scope.
    pub actor_world_binding_id: Option<String>,
    /// Keywords extracted from digest (stored as JSON array).
    pub keywords: String,
    /// Short summary of the fragment.
    pub summary: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Optional TTL (e.g., "30d", "90d").
    pub ttl: Option<String>,
    /// OCC version backing revision-checked local→shared promotion.
    pub revision: i64,
}

/// Inputs for minting a new Character fragment.
#[derive(Debug, Clone)]
pub struct NewCharacterMemoryFragment {
    pub fragment_id: String,
    pub session_id: String,
    pub character_id: String,
    pub actor_world_binding_id: Option<String>,
    pub keywords: String,
    pub summary: String,
    pub created_at: String,
    pub ttl: Option<String>,
}

#[allow(clippy::too_many_arguments)] // raw column values — the full row shape
const fn record_from_row(
    fragment_id: String,
    session_id: String,
    character_id: String,
    actor_world_binding_id: Option<String>,
    keywords: String,
    summary: String,
    created_at: String,
    ttl: Option<String>,
    revision: i64,
) -> CharacterMemoryFragmentRecord {
    CharacterMemoryFragmentRecord {
        fragment_id,
        session_id,
        character_id,
        actor_world_binding_id,
        keywords,
        summary,
        created_at,
        ttl,
        revision,
    }
}

/// Create a Character memory fragment.
///
/// Runs in a write-serialized transaction: validates Character ownership and
/// binding provenance first. New fragments always start at `revision = 0`.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` for foreign Characters or invalid
/// bindings; `LocalDbError` on constraint or database failure.
pub async fn create_character_fragment(
    pool: &SqlitePool,
    owner_creator_id: &str,
    fragment: &NewCharacterMemoryFragment,
) -> Result<(), LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    let result = async {
        require_active_owned_character(&mut tx, owner_creator_id, &fragment.character_id).await?;
        require_valid_provenance_tx(
            &mut tx,
            owner_creator_id,
            &fragment.character_id,
            fragment.actor_world_binding_id.as_deref(),
        )
        .await?;
        sqlx::query!(
            "INSERT INTO character_memory_fragments
             (fragment_id, session_id, character_id, actor_world_binding_id, keywords, summary, created_at, ttl, revision)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)",
            fragment.fragment_id,
            fragment.session_id,
            fragment.character_id,
            fragment.actor_world_binding_id,
            fragment.keywords,
            fragment.summary,
            fragment.created_at,
            fragment.ttl
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

/// Insert a Character memory fragment inside a caller-owned transaction.
///
/// Runs the same active-ownership and provenance validation as
/// [`create_character_fragment`], but executes inside the caller's
/// transaction (PR #240 finding 2) so the review pipeline can commit the
/// fragment insert and the pending-row deletion atomically.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` for foreign Characters or invalid
/// bindings; `LocalDbError` on constraint or database failure.
pub async fn create_character_fragment_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    owner_creator_id: &str,
    fragment: &NewCharacterMemoryFragment,
) -> Result<(), LocalDbError> {
    require_active_owned_character(&mut *tx, owner_creator_id, &fragment.character_id).await?;
    require_valid_provenance_tx(
        &mut *tx,
        owner_creator_id,
        &fragment.character_id,
        fragment.actor_world_binding_id.as_deref(),
    )
    .await?;
    sqlx::query!(
        "INSERT INTO character_memory_fragments
         (fragment_id, session_id, character_id, actor_world_binding_id, keywords, summary, created_at, ttl, revision)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)",
        fragment.fragment_id,
        fragment.session_id,
        fragment.character_id,
        fragment.actor_world_binding_id,
        fragment.keywords,
        fragment.summary,
        fragment.created_at,
        fragment.ttl
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Get a Character fragment by ID, scoped to the owning Character.
///
/// Returns None if the fragment does not exist within this Character.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character is missing or
/// owned by another Creator; `LocalDbError` on database failure.
pub async fn get_character_fragment(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: Option<&str>,
    fragment_id: &str,
) -> Result<Option<CharacterMemoryFragmentRecord>, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    if let Some(binding_id) = binding_id {
        require_active_owned_provenance_pool(pool, owner_creator_id, character_id, binding_id)
            .await?;
    }
    load_fragment(pool, character_id, binding_id, fragment_id).await
}

async fn load_fragment(
    pool: &SqlitePool,
    character_id: &str,
    binding_id: Option<&str>,
    fragment_id: &str,
) -> Result<Option<CharacterMemoryFragmentRecord>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT fragment_id as "fragment_id!", session_id as "session_id!",
                  character_id as "character_id!", actor_world_binding_id,
                  keywords as "keywords!", summary as "summary!",
                  created_at as "created_at!", ttl,
                  revision as "revision!"
           FROM character_memory_fragments
           WHERE fragment_id = ? AND character_id = ? AND actor_world_binding_id IS ?"#,
        fragment_id,
        character_id,
        binding_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| {
        record_from_row(
            r.fragment_id,
            r.session_id,
            r.character_id,
            r.actor_world_binding_id,
            r.keywords,
            r.summary,
            r.created_at,
            r.ttl,
            r.revision,
        )
    }))
}

/// List a bounded page of Character fragments in a single scope.
///
/// `binding_id = None` reads the shared Character scope (`actor_world_binding_id
/// IS NULL`); `Some(b)` reads only that binding-local scope. Binding-local
/// reads require the exact active binding for the Character.
///
/// Results are ordered `created_at DESC, fragment_id DESC` (a total order, so
/// offset pagination is deterministic); `offset` skips that many rows of the
/// ordered scope.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character (or, for a
/// binding-scoped read, the binding) is missing, foreign, or inactive;
/// `LocalDbError` on database failure.
pub async fn list_character_fragments(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<CharacterMemoryFragmentRecord>, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    if let Some(binding_id) = binding_id {
        require_active_owned_provenance_pool(pool, owner_creator_id, character_id, binding_id)
            .await?;
    }
    let limit = limit.clamp(1, MAX_CHARACTER_MEMORY_LIST_LIMIT);
    let offset = offset.max(0);
    if let Some(binding_id) = binding_id {
        let rows = sqlx::query!(
            r#"SELECT fragment_id as "fragment_id!", session_id as "session_id!",
                      character_id as "character_id!", actor_world_binding_id,
                      keywords as "keywords!", summary as "summary!",
                      created_at as "created_at!", ttl,
                      revision as "revision!"
               FROM character_memory_fragments
               WHERE character_id = ? AND actor_world_binding_id = ?
               ORDER BY created_at DESC, fragment_id DESC LIMIT ? OFFSET ?"#,
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
                    r.fragment_id,
                    r.session_id,
                    r.character_id,
                    r.actor_world_binding_id,
                    r.keywords,
                    r.summary,
                    r.created_at,
                    r.ttl,
                    r.revision,
                )
            })
            .collect())
    } else {
        let rows = sqlx::query!(
            r#"SELECT fragment_id as "fragment_id!", session_id as "session_id!",
                      character_id as "character_id!", actor_world_binding_id,
                      keywords as "keywords!", summary as "summary!",
                      created_at as "created_at!", ttl,
                      revision as "revision!"
               FROM character_memory_fragments
               WHERE character_id = ? AND actor_world_binding_id IS NULL
               ORDER BY created_at DESC, fragment_id DESC LIMIT ? OFFSET ?"#,
            character_id,
            limit,
            offset
        )
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                record_from_row(
                    r.fragment_id,
                    r.session_id,
                    r.character_id,
                    r.actor_world_binding_id,
                    r.keywords,
                    r.summary,
                    r.created_at,
                    r.ttl,
                    r.revision,
                )
            })
            .collect())
    }
}

/// Delete a Character fragment by ID, scoped to the owning Character.
///
/// Returns true if a fragment was deleted, false if it didn't exist.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character is missing or
/// owned by another Creator; `LocalDbError` on database failure.
pub async fn delete_character_fragment(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    fragment_id: &str,
) -> Result<bool, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    let result = sqlx::query!(
        "DELETE FROM character_memory_fragments WHERE fragment_id = ? AND character_id = ?",
        fragment_id,
        character_id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Promote a binding-local Character fragment to shared Character memory.
///
/// One atomic write-serialized transaction:
/// 1. Requires an active owned Character.
/// 2. Loads the fragment scoped to that Character; missing/foreign → not found.
/// 3. Rejects an already-shared fragment with [`ActorContractConflict::CharacterFragmentAlreadyShared`].
/// 4. Rejects a stale `expected_revision` with [`LocalDbError::VersionMismatch`].
/// 5. Clears `actor_world_binding_id` in place (fragment id preserved) and
///    bumps `revision` behind a revision-guarded `UPDATE`.
/// 6. Invalidates the two affected narrative cache scopes — the shared scope
///    and the fragment's prior binding scope — by deleting their cache rows.
///
/// No implicit promotion occurs on read, run, or binding removal.
///
/// # Errors
///
/// Returns `LocalDbError` on not-found, already-shared conflict, version
/// mismatch, or database failure.
pub async fn promote_character_fragment_to_shared(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    fragment_id: &str,
    expected_revision: i64,
) -> Result<CharacterMemoryFragmentRecord, LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    let result = async {
        require_active_owned_character(&mut tx, owner_creator_id, character_id).await?;
        let current = load_fragment_tx(&mut tx, character_id, fragment_id).await?;
        let Some(current) = current else {
            return Err(LocalDbError::ActorNotFound {
                resource: "character_memory_fragment",
                id: fragment_id.to_string(),
            });
        };
        let Some(old_binding) = current.actor_world_binding_id.as_deref() else {
            return Err(LocalDbError::ActorContractConflict {
                code: ActorContractConflict::CharacterFragmentAlreadyShared,
            });
        };
        // Revalidate the source binding inside the write transaction before
        // clearing its provenance: it must still be active, belong to this
        // Character, and target an owned active World. An invalid source
        // binding rejects with zero mutation rather than leaking the row into
        // shared Character memory.
        require_valid_provenance_tx(&mut tx, owner_creator_id, character_id, Some(old_binding))
            .await?;
        if current.revision != expected_revision {
            return Err(LocalDbError::VersionMismatch {
                table: "character_memory_fragments".to_string(),
                id: fragment_id.to_string(),
                expected: expected_revision,
                actual: Some(current.revision),
            });
        }

        let new_revision = expected_revision + 1;
        let updated = sqlx::query!(
            "UPDATE character_memory_fragments
             SET actor_world_binding_id = NULL, revision = ?
             WHERE fragment_id = ? AND character_id = ? AND revision = ?
               AND actor_world_binding_id IS NOT NULL",
            new_revision,
            fragment_id,
            character_id,
            expected_revision
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        cas_check_with_version_column(
            &mut *tx,
            updated,
            "character_memory_fragments",
            "fragment_id",
            fragment_id,
            "revision",
            expected_revision,
        )
        .await?;

        // Invalidate exactly the affected cache scopes: shared + old binding.
        sqlx::query!(
            "DELETE FROM character_soul_narratives
             WHERE character_id = ? AND (actor_world_binding_id IS NULL OR actor_world_binding_id = ?)",
            character_id,
            old_binding
        )
        .execute(&mut *tx)
        .await?;

        let promoted = load_fragment_tx(&mut tx, character_id, fragment_id).await?;
        promoted.ok_or_else(|| LocalDbError::ActorNotFound {
            resource: "character_memory_fragment",
            id: fragment_id.to_string(),
        })
    }
    .await;
    match result {
        Ok(record) => {
            tx.commit().await?;
            Ok(record)
        }
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}

async fn load_fragment_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    character_id: &str,
    fragment_id: &str,
) -> Result<Option<CharacterMemoryFragmentRecord>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT fragment_id as "fragment_id!", session_id as "session_id!",
                  character_id as "character_id!", actor_world_binding_id,
                  keywords as "keywords!", summary as "summary!",
                  created_at as "created_at!", ttl,
                  revision as "revision!"
           FROM character_memory_fragments
           WHERE fragment_id = ? AND character_id = ?"#,
        fragment_id,
        character_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|r| {
        record_from_row(
            r.fragment_id,
            r.session_id,
            r.character_id,
            r.actor_world_binding_id,
            r.keywords,
            r.summary,
            r.created_at,
            r.ttl,
            r.revision,
        )
    }))
}
