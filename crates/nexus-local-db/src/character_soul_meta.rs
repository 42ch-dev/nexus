//! Character SOUL metadata persistence (v1.184 P3 Task 1).
//!
//! Character counterpart to [`crate::soul_meta`]: per-Character SOUL.md
//! metadata on the dedicated `character_soul_meta` table for fast lookups
//! without file I/O. Authorization derives the owner from `characters`.

use sqlx::SqlitePool;

use crate::character::{require_active_owned_character, require_owned_character_pool};
use crate::error::LocalDbError;

/// Character SOUL metadata record.
#[derive(Debug, Clone)]
pub struct CharacterSoulMeta {
    pub character_id: String,
    pub file_path: String,
    pub schema_version: i64,
    pub personality_hash: Option<String>,
    pub experience_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Upsert Character SOUL metadata (insert or update on `character_id`).
///
/// Runs in a write-serialized transaction that requires an active owned
/// Character.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character is missing or
/// owned by another Creator; `LocalDbError` on database failure.
pub async fn upsert_character_soul_meta(
    pool: &SqlitePool,
    owner_creator_id: &str,
    meta: &CharacterSoulMeta,
) -> Result<(), LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    let result = async {
        require_active_owned_character(&mut tx, owner_creator_id, &meta.character_id).await?;
        sqlx::query!(
            "INSERT INTO character_soul_meta
             (character_id, file_path, schema_version, personality_hash, experience_hash, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(character_id) DO UPDATE SET
               file_path = excluded.file_path,
               schema_version = excluded.schema_version,
               personality_hash = excluded.personality_hash,
               experience_hash = excluded.experience_hash,
               updated_at = excluded.updated_at",
            meta.character_id,
            meta.file_path,
            meta.schema_version,
            meta.personality_hash,
            meta.experience_hash,
            meta.created_at,
            meta.updated_at
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

/// Get Character SOUL metadata, scoped to the owning Character.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character is missing or
/// owned by another Creator; `LocalDbError` on database failure.
pub async fn get_character_soul_meta(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
) -> Result<Option<CharacterSoulMeta>, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    let row = sqlx::query!(
        r#"SELECT character_id as "character_id!", file_path as "file_path!",
                  schema_version as "schema_version!", personality_hash, experience_hash,
                  created_at as "created_at!", updated_at as "updated_at!"
           FROM character_soul_meta WHERE character_id = ?"#,
        character_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| CharacterSoulMeta {
        character_id: r.character_id,
        file_path: r.file_path,
        schema_version: r.schema_version,
        personality_hash: r.personality_hash,
        experience_hash: r.experience_hash,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

/// Delete Character SOUL metadata, scoped to the owning Character.
///
/// Returns true if a record was deleted, false if it didn't exist.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character is missing or
/// owned by another Creator; `LocalDbError` on database failure.
pub async fn delete_character_soul_meta(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
) -> Result<bool, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    let result = sqlx::query!(
        "DELETE FROM character_soul_meta WHERE character_id = ?",
        character_id
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
