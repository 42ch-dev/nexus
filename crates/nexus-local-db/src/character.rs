//! Character bearer storage and the atomic create+initial-binding transaction.

use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::actor_world_binding::{
    insert_binding_tx, validate_world_sheet_tx, ActorWorldBindingRecord, CreateBindingParams,
};
use crate::begin_immediate;
use crate::error::{ActorContractConflict, LocalDbError};

const PERSONA_JSON_MAX_BYTES: usize = 16_384;
const IMAGE_URI_MAX_BYTES: usize = 2048;

/// Persisted Character row (storage shape; not a wire DTO).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterRecord {
    pub character_id: String,
    pub owner_creator_id: String,
    pub display_name: String,
    pub status: String,
    pub image_uri: Option<String>,
    pub persona_json: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Inputs for minting a Character together with its first active binding.
#[derive(Debug, Clone, Copy)]
pub struct CreateCharacterParams<'a> {
    pub owner_creator_id: &'a str,
    pub display_name: &'a str,
    pub image_uri: Option<&'a str>,
    pub persona_json: &'a str,
    pub world_id: &'a str,
    pub world_sheet_entry_id: Option<&'a str>,
}

/// Atomic create result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateCharacterResult {
    pub character: CharacterRecord,
    pub binding: ActorWorldBindingRecord,
}

/// Mint a `chr_` + 32 lowercase hex Character id.
#[must_use]
pub fn mint_character_id() -> String {
    format!("chr_{}", Uuid::new_v4().simple())
}

pub(crate) fn normalize_display_name(raw: &str) -> Result<String, LocalDbError> {
    if raw.trim() != raw {
        return Err(LocalDbError::ValidationError(
            "display_name must be trimmed (no leading or trailing whitespace)".into(),
        ));
    }
    let scalars = raw.chars().count();
    if raw.is_empty() || scalars > 120 {
        return Err(LocalDbError::ValidationError(
            "display_name must be trimmed non-empty and at most 120 Unicode scalars".into(),
        ));
    }
    Ok(raw.to_string())
}

pub(crate) fn validate_image_uri(raw: Option<&str>) -> Result<Option<String>, LocalDbError> {
    match raw {
        None => Ok(None),
        Some(uri) if uri.len() > IMAGE_URI_MAX_BYTES => Err(LocalDbError::ValidationError(
            "image_uri must be at most 2048 bytes".into(),
        )),
        Some(uri) => Ok(Some(uri.to_string())),
    }
}

pub(crate) fn validate_persona_json(raw: &str) -> Result<String, LocalDbError> {
    let value: Value = serde_json::from_str(raw).map_err(|e| {
        LocalDbError::ValidationError(format!("persona_json must be a JSON object: {e}"))
    })?;
    if !value.is_object() {
        return Err(LocalDbError::ValidationError(
            "persona_json must be a JSON object".into(),
        ));
    }
    let serialized = value.to_string();
    if serialized.len() > PERSONA_JSON_MAX_BYTES {
        return Err(LocalDbError::ValidationError(
            "persona_json must be at most 16384 bytes".into(),
        ));
    }
    Ok(serialized)
}

pub(crate) async fn require_owned_world(
    tx: &mut Transaction<'_, Sqlite>,
    owner_creator_id: &str,
    world_id: &str,
) -> Result<(), LocalDbError> {
    let owner = sqlx::query_scalar!(
        r#"SELECT owner_creator_id as "owner_creator_id!" FROM narrative_worlds WHERE world_id = ?"#,
        world_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    match owner {
        Some(stored) if stored == owner_creator_id => Ok(()),
        Some(_) | None => Err(LocalDbError::ActorNotFound {
            resource: "world",
            id: world_id.to_string(),
        }),
    }
}

/// Owned **and active** World validation (v1.184 P3 provenance rule).
///
/// Character memory provenance requires a binding to target a World that is
/// both owned by the caller and `status = 'active'`. Archived or paused
/// Worlds reject as not-found, so binding-local memory can never be authored
/// or removed under an inactive World.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the World is missing, owned by
/// another Creator, or not active; `LocalDbError` on database failure.
pub(crate) async fn require_owned_active_world(
    tx: &mut Transaction<'_, Sqlite>,
    owner_creator_id: &str,
    world_id: &str,
) -> Result<(), LocalDbError> {
    let owner = sqlx::query_scalar!(
        r#"SELECT owner_creator_id as "owner_creator_id!"
           FROM narrative_worlds WHERE world_id = ? AND status = 'active'"#,
        world_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    match owner {
        Some(stored) if stored == owner_creator_id => Ok(()),
        Some(_) | None => Err(LocalDbError::ActorNotFound {
            resource: "world",
            id: world_id.to_string(),
        }),
    }
}

/// Pool variant of [`require_owned_active_world`] for read paths.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the World is missing, owned by
/// another Creator, or not active; `LocalDbError` on database failure.
pub(crate) async fn require_owned_active_world_pool(
    pool: &SqlitePool,
    owner_creator_id: &str,
    world_id: &str,
) -> Result<(), LocalDbError> {
    let owner = sqlx::query_scalar!(
        r#"SELECT owner_creator_id as "owner_creator_id!"
           FROM narrative_worlds WHERE world_id = ? AND status = 'active'"#,
        world_id
    )
    .fetch_optional(pool)
    .await?;
    match owner {
        Some(stored) if stored == owner_creator_id => Ok(()),
        Some(_) | None => Err(LocalDbError::ActorNotFound {
            resource: "world",
            id: world_id.to_string(),
        }),
    }
}

async fn load_character(
    tx: &mut Transaction<'_, Sqlite>,
    character_id: &str,
) -> Result<Option<CharacterRecord>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT character_id as "character_id!",
                  owner_creator_id as "owner_creator_id!",
                  display_name as "display_name!",
                  status as "status!",
                  image_uri,
                  persona_json as "persona_json!",
                  created_at as "created_at!",
                  updated_at as "updated_at!"
           FROM characters WHERE character_id = ?"#,
        character_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|r| CharacterRecord {
        character_id: r.character_id,
        owner_creator_id: r.owner_creator_id,
        display_name: r.display_name,
        status: r.status,
        image_uri: r.image_uri,
        persona_json: r.persona_json,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

/// Ownership-scoped Character lookup. Foreign ids are not distinguished from missing.
///
/// # Errors
///
/// Returns `LocalDbError` on database failure.
pub async fn get_character(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
) -> Result<Option<CharacterRecord>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT character_id as "character_id!",
                  owner_creator_id as "owner_creator_id!",
                  display_name as "display_name!",
                  status as "status!",
                  image_uri,
                  persona_json as "persona_json!",
                  created_at as "created_at!",
                  updated_at as "updated_at!"
           FROM characters
           WHERE character_id = ? AND owner_creator_id = ?"#,
        character_id,
        owner_creator_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| CharacterRecord {
        character_id: r.character_id,
        owner_creator_id: r.owner_creator_id,
        display_name: r.display_name,
        status: r.status,
        image_uri: r.image_uri,
        persona_json: r.persona_json,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

/// List Characters owned by `owner_creator_id` with SQL `LIMIT`/`OFFSET`.
///
/// Callers that paginate should pass `limit + 1` so they can detect `has_more`.
///
/// # Errors
///
/// Returns `LocalDbError` on database failure.
pub async fn list_characters(
    pool: &SqlitePool,
    owner_creator_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<CharacterRecord>, LocalDbError> {
    let rows = sqlx::query!(
        r#"SELECT character_id as "character_id!",
                  owner_creator_id as "owner_creator_id!",
                  display_name as "display_name!",
                  status as "status!",
                  image_uri,
                  persona_json as "persona_json!",
                  created_at as "created_at!",
                  updated_at as "updated_at!"
           FROM characters
           WHERE owner_creator_id = ?
           ORDER BY created_at ASC, character_id ASC
           LIMIT ? OFFSET ?"#,
        owner_creator_id,
        limit,
        offset
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| CharacterRecord {
            character_id: r.character_id,
            owner_creator_id: r.owner_creator_id,
            display_name: r.display_name,
            status: r.status,
            image_uri: r.image_uri,
            persona_json: r.persona_json,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect())
}

/// Insert Character + first active binding in one write-serialized transaction.
///
/// # Errors
///
/// Returns `LocalDbError` if validation, ownership, `WorldSheet`, uniqueness, or SQL fails.
/// Any failure rolls back both inserts.
pub async fn create_character_with_initial_binding(
    pool: &SqlitePool,
    params: CreateCharacterParams<'_>,
) -> Result<CreateCharacterResult, LocalDbError> {
    let display_name = normalize_display_name(params.display_name)?;
    let persona_json = validate_persona_json(params.persona_json)?;
    let image_uri = validate_image_uri(params.image_uri)?;
    let character_id = mint_character_id();
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = begin_immediate(pool).await?;
    let result = create_in_tx(
        &mut tx,
        params,
        &character_id,
        &display_name,
        &persona_json,
        image_uri.as_deref(),
        &now,
    )
    .await;
    match result {
        Ok(created) => {
            tx.commit().await?;
            Ok(created)
        }
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}

async fn create_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    params: CreateCharacterParams<'_>,
    character_id: &str,
    display_name: &str,
    persona_json: &str,
    image_uri: Option<&str>,
    now: &str,
) -> Result<CreateCharacterResult, LocalDbError> {
    require_owned_world(tx, params.owner_creator_id, params.world_id).await?;
    validate_world_sheet_tx(tx, params.world_id, params.world_sheet_entry_id).await?;

    let owner_creator_id = params.owner_creator_id;
    let insert = sqlx::query!(
        r#"INSERT INTO characters
           (character_id, owner_creator_id, display_name, status, image_uri, persona_json, created_at, updated_at)
           VALUES (?, ?, ?, 'active', ?, ?, ?, ?)"#,
        character_id,
        owner_creator_id,
        display_name,
        image_uri,
        persona_json,
        now,
        now
    )
    .execute(&mut **tx)
    .await;
    map_actor_constraint(insert)?;

    let binding = insert_binding_tx(
        tx,
        CreateBindingParams {
            owner_creator_id: params.owner_creator_id,
            character_id,
            world_id: params.world_id,
            world_sheet_entry_id: params.world_sheet_entry_id,
        },
        now,
    )
    .await?;

    let character =
        load_character(tx, character_id)
            .await?
            .ok_or_else(|| LocalDbError::ActorNotFound {
                resource: "character",
                id: character_id.to_string(),
            })?;
    Ok(CreateCharacterResult { character, binding })
}

pub(crate) async fn require_owned_character(
    tx: &mut Transaction<'_, Sqlite>,
    owner_creator_id: &str,
    character_id: &str,
) -> Result<CharacterRecord, LocalDbError> {
    let row = load_character(tx, character_id).await?;
    match row {
        Some(c) if c.owner_creator_id == owner_creator_id => Ok(c),
        Some(_) | None => Err(LocalDbError::ActorNotFound {
            resource: "character",
            id: character_id.to_string(),
        }),
    }
}

pub(crate) async fn require_active_owned_character(
    tx: &mut Transaction<'_, Sqlite>,
    owner_creator_id: &str,
    character_id: &str,
) -> Result<CharacterRecord, LocalDbError> {
    let row = require_owned_character(tx, owner_creator_id, character_id).await?;
    if row.status != "active" {
        return Err(LocalDbError::ActorNotFound {
            resource: "character",
            id: character_id.to_string(),
        });
    }
    Ok(row)
}

/// Pool-scoped ownership check for read paths that do not hold a write
/// transaction. Foreign ids are not distinguished from missing.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character is missing or
/// owned by another Creator; `LocalDbError` on database failure.
pub(crate) async fn require_owned_character_pool(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
) -> Result<(), LocalDbError> {
    let owned = sqlx::query_scalar!(
        r#"SELECT owner_creator_id as "owner_creator_id!" FROM characters WHERE character_id = ?"#,
        character_id
    )
    .fetch_optional(pool)
    .await?;
    match owned {
        Some(stored) if stored == owner_creator_id => Ok(()),
        Some(_) | None => Err(LocalDbError::ActorNotFound {
            resource: "character",
            id: character_id.to_string(),
        }),
    }
}

/// Map unique/check failures onto stable actor conflicts when possible.
pub(crate) fn map_actor_constraint<T>(result: Result<T, sqlx::Error>) -> Result<T, LocalDbError> {
    match result {
        Ok(v) => Ok(v),
        Err(err) => {
            if let sqlx::Error::Database(db) = &err {
                if db.is_unique_violation() {
                    let constraint = db.constraint().unwrap_or_default();
                    let message = db.message();
                    if constraint.contains("idx_actor_world_bindings_active_unique")
                        || message.contains("idx_actor_world_bindings_active_unique")
                        || message.contains(
                            "UNIQUE constraint failed: actor_world_bindings.character_id, actor_world_bindings.world_id",
                        )
                    {
                        return Err(LocalDbError::ActorContractConflict {
                            code: ActorContractConflict::DuplicateActiveBinding,
                        });
                    }
                    if constraint.contains("idx_characters_owner_active_display_name")
                        || message.contains("idx_characters_owner_active_display_name")
                        || message.contains(
                            "UNIQUE constraint failed: characters.owner_creator_id, characters.display_name",
                        )
                    {
                        return Err(LocalDbError::ActorContractConflict {
                            code: ActorContractConflict::DuplicateCharacterDisplayName,
                        });
                    }
                }
            }
            Err(LocalDbError::from(err))
        }
    }
}
