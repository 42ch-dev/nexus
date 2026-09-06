//! `ActorWorldBinding` storage and the authoritative last-binding removal transaction.

use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::begin_immediate;
use crate::character::{
    check_expected_revision, map_actor_constraint, require_active_owned_character_tx,
    require_owned_active_world, require_owned_character_pool, require_owned_world,
    require_owned_world_pool, FieldPatch,
};
use crate::error::{ActorContractConflict, LocalDbError};

/// Persisted binding row (storage shape; not a wire DTO).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorWorldBindingRecord {
    pub binding_id: String,
    pub character_id: String,
    pub world_id: String,
    pub status: String,
    pub world_sheet_entry_id: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Inputs for adding an active binding to an owned Character.
#[derive(Debug, Clone, Copy)]
pub struct CreateBindingParams<'a> {
    pub owner_creator_id: &'a str,
    pub character_id: &'a str,
    pub world_id: &'a str,
    pub world_sheet_entry_id: Option<&'a str>,
}

/// Mint an `awb_` + 32 lowercase hex binding id.
#[must_use]
pub fn mint_binding_id() -> String {
    format!("awb_{}", Uuid::new_v4().simple())
}

const fn record_from_query(
    binding_id: String,
    character_id: String,
    world_id: String,
    status: String,
    world_sheet_entry_id: Option<String>,
    revision: i64,
    created_at: String,
    updated_at: String,
) -> ActorWorldBindingRecord {
    ActorWorldBindingRecord {
        binding_id,
        character_id,
        world_id,
        status,
        world_sheet_entry_id,
        revision,
        created_at,
        updated_at,
    }
}

pub(crate) async fn validate_world_sheet_tx(
    tx: &mut Transaction<'_, Sqlite>,
    world_id: &str,
    world_sheet_entry_id: Option<&str>,
) -> Result<(), LocalDbError> {
    let Some(sheet_id) = world_sheet_entry_id else {
        return Ok(());
    };
    if sheet_id.len() > 128 {
        return Err(LocalDbError::ActorContractConflict {
            code: ActorContractConflict::InvalidWorldSheet,
        });
    }
    if !sheet_id.starts_with("kb_") {
        return Err(LocalDbError::ActorContractConflict {
            code: ActorContractConflict::InvalidWorldSheet,
        });
    }
    let ok = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM kb_key_blocks
            WHERE key_block_id = ?
              AND world_id = ?
              AND owner_kind = 'world'
              AND block_type = 'character'
              AND status NOT IN ('deleted', 'merged', 'deprecated')
              AND creator_only = 0
         ) as "ok!: i64""#,
        sheet_id,
        world_id
    )
    .fetch_one(&mut **tx)
    .await?;
    if ok == 0 {
        return Err(LocalDbError::ActorContractConflict {
            code: ActorContractConflict::InvalidWorldSheet,
        });
    }
    Ok(())
}

pub(crate) async fn insert_binding_tx(
    tx: &mut Transaction<'_, Sqlite>,
    params: CreateBindingParams<'_>,
    now: &str,
) -> Result<ActorWorldBindingRecord, LocalDbError> {
    validate_world_sheet_tx(tx, params.world_id, params.world_sheet_entry_id).await?;
    let binding_id = mint_binding_id();
    let character_id = params.character_id;
    let world_id = params.world_id;
    let world_sheet_entry_id = params.world_sheet_entry_id;
    let insert = sqlx::query!(
        r#"INSERT INTO actor_world_bindings
           (binding_id, character_id, world_id, status, world_sheet_entry_id, created_at, updated_at)
           VALUES (?, ?, ?, 'active', ?, ?, ?)"#,
        binding_id,
        character_id,
        world_id,
        world_sheet_entry_id,
        now,
        now
    )
    .execute(&mut **tx)
    .await;
    map_actor_constraint(insert)?;
    load_binding_tx(tx, &binding_id)
        .await?
        .ok_or_else(|| LocalDbError::ActorNotFound {
            resource: "actor_world_binding",
            id: binding_id,
        })
}

async fn load_binding_tx(
    tx: &mut Transaction<'_, Sqlite>,
    binding_id: &str,
) -> Result<Option<ActorWorldBindingRecord>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT binding_id as "binding_id!",
                  character_id as "character_id!",
                  world_id as "world_id!",
                  status as "status!",
                  world_sheet_entry_id,
                  revision as "revision!",
                  created_at as "created_at!",
                  updated_at as "updated_at!"
           FROM actor_world_bindings WHERE binding_id = ?"#,
        binding_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|r| {
        record_from_query(
            r.binding_id,
            r.character_id,
            r.world_id,
            r.status,
            r.world_sheet_entry_id,
            r.revision,
            r.created_at,
            r.updated_at,
        )
    }))
}

/// Require an active binding that belongs to `character_id` (tx variant).
///
/// Cross-Character or non-active bindings are indistinguishable from missing.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the binding is missing, belongs
/// to another Character, or is not active; `LocalDbError` on database failure.
pub(crate) async fn require_active_character_binding_tx(
    tx: &mut Transaction<'_, Sqlite>,
    character_id: &str,
    binding_id: &str,
) -> Result<ActorWorldBindingRecord, LocalDbError> {
    let binding = load_binding_tx(tx, binding_id).await?;
    match binding {
        Some(b) if b.character_id == character_id && b.status == "active" => Ok(b),
        _ => Err(LocalDbError::ActorNotFound {
            resource: "actor_world_binding",
            id: binding_id.to_string(),
        }),
    }
}

/// Validate optional binding provenance inside an open write transaction.
///
/// A non-null binding must be active, belong to the same Character, and
/// target a World owned by `owner_creator_id`. Shared-scope (`None`) writes
/// skip binding validation entirely. Used by every Character memory write
/// path so binding-local rows cannot be created against a foreign Character,
/// a non-active binding, or a World owned by someone else.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` for a missing, foreign, or inactive
/// binding; `LocalDbError` on owner/world or database failure.
pub(crate) async fn require_valid_provenance_tx(
    tx: &mut Transaction<'_, Sqlite>,
    owner_creator_id: &str,
    character_id: &str,
    actor_world_binding_id: Option<&str>,
) -> Result<(), LocalDbError> {
    let Some(binding_id) = actor_world_binding_id else {
        return Ok(());
    };
    let binding = require_active_character_binding_tx(tx, character_id, binding_id).await?;
    crate::character::require_owned_active_world(tx, owner_creator_id, &binding.world_id).await?;
    Ok(())
}

async fn load_binding_pool(
    pool: &SqlitePool,
    binding_id: &str,
) -> Result<Option<ActorWorldBindingRecord>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT binding_id as "binding_id!",
                  character_id as "character_id!",
                  world_id as "world_id!",
                  status as "status!",
                  world_sheet_entry_id,
                  revision as "revision!",
                  created_at as "created_at!",
                  updated_at as "updated_at!"
           FROM actor_world_bindings WHERE binding_id = ?"#,
        binding_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| {
        record_from_query(
            r.binding_id,
            r.character_id,
            r.world_id,
            r.status,
            r.world_sheet_entry_id,
            r.revision,
            r.created_at,
            r.updated_at,
        )
    }))
}

/// Retained-data read provenance: owned Character plus the stored binding
/// tuple (`binding_id` belongs to `character_id`) plus a World that exists
/// and is owned by `owner_creator_id`.
///
/// Retained reads never require liveness: the Character, binding, or World
/// may be archived, but a missing binding, a binding belonging to another
/// Character, or a missing/foreign World is still rejected as not-found
/// (fail-closed, indistinguishable from missing).
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character is missing or
/// foreign, the binding is missing or belongs to another Character, or the
/// World is missing or owned by another Creator; `LocalDbError` on database
/// failure.
pub(crate) async fn require_owned_binding_provenance_pool(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: &str,
) -> Result<(), LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    let binding = load_binding_pool(pool, binding_id).await?;
    let Some(binding) = binding.filter(|b| b.character_id == character_id) else {
        return Err(LocalDbError::ActorNotFound {
            resource: "actor_world_binding",
            id: binding_id.to_string(),
        });
    };
    crate::character::require_owned_world_pool(pool, owner_creator_id, &binding.world_id).await?;
    Ok(())
}

/// Add a second (or later) active binding for an owned active Character.
///
/// # Errors
///
/// Returns `LocalDbError` on ownership, `WorldSheet`, duplicate, or SQL failure.
pub async fn add_actor_world_binding(
    pool: &SqlitePool,
    params: CreateBindingParams<'_>,
) -> Result<ActorWorldBindingRecord, LocalDbError> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = begin_immediate(pool).await?;
    let result = async {
        require_active_owned_character_tx(&mut tx, params.owner_creator_id, params.character_id)
            .await?;
        require_owned_world(&mut tx, params.owner_creator_id, params.world_id).await?;
        insert_binding_tx(&mut tx, params, &now).await
    }
    .await;
    match result {
        Ok(row) => {
            tx.commit().await?;
            Ok(row)
        }
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}

/// Ownership-scoped binding list for a Character.
///
/// # Errors
///
/// Returns `LocalDbError` on database failure or when the Character is not owned.
pub async fn list_bindings_for_character(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ActorWorldBindingRecord>, LocalDbError> {
    let owned = sqlx::query_scalar!(
        r#"SELECT owner_creator_id as "owner_creator_id!" FROM characters WHERE character_id = ?"#,
        character_id
    )
    .fetch_optional(pool)
    .await?;
    match owned {
        Some(stored) if stored == owner_creator_id => {}
        Some(_) | None => {
            return Err(LocalDbError::ActorNotFound {
                resource: "character",
                id: character_id.to_string(),
            });
        }
    }
    let rows = sqlx::query!(
        r#"SELECT binding_id as "binding_id!",
                  character_id as "character_id!",
                  world_id as "world_id!",
                  status as "status!",
                  world_sheet_entry_id,
                  revision as "revision!",
                  created_at as "created_at!",
                  updated_at as "updated_at!"
           FROM actor_world_bindings
           WHERE character_id = ?
           ORDER BY created_at ASC, binding_id ASC
           LIMIT ? OFFSET ?"#,
        character_id,
        limit,
        offset
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            record_from_query(
                r.binding_id,
                r.character_id,
                r.world_id,
                r.status,
                r.world_sheet_entry_id,
                r.revision,
                r.created_at,
                r.updated_at,
            )
        })
        .collect())
}

/// Count every binding for a World inside an open write transaction.
///
/// Includes inactive rows because `ON DELETE RESTRICT` applies to all statuses.
///
/// # Errors
///
/// Returns `LocalDbError` on database failure.
pub async fn count_bindings_for_world_tx(
    tx: &mut Transaction<'_, Sqlite>,
    world_id: &str,
) -> Result<i64, LocalDbError> {
    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "count!: i64" FROM actor_world_bindings WHERE world_id = ?"#,
        world_id
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(count)
}

fn binding_revision_conflict() -> LocalDbError {
    LocalDbError::ActorContractConflict {
        code: ActorContractConflict::BindingRevisionConflict,
    }
}

fn resolved_world_sheet_patch(
    current: &Option<String>,
    patch: FieldPatch<&str>,
) -> Option<String> {
    match patch {
        FieldPatch::Keep => current.clone(),
        FieldPatch::Clear => None,
        FieldPatch::Set(id) => Some(id.to_string()),
    }
}

fn binding_sheet_patch_is_no_op(
    current: &Option<String>,
    patch: FieldPatch<&str>,
) -> bool {
    resolved_world_sheet_patch(current, patch) == *current
}

/// Owner-scoped binding detail with path tuple validation.
///
/// Retained reads tolerate archived Character/World status; missing, foreign,
/// or tuple-mismatched bindings are indistinguishable (`None`).
///
/// # Errors
///
/// Returns `LocalDbError` on database failure.
pub async fn get_actor_world_binding(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: &str,
) -> Result<Option<ActorWorldBindingRecord>, LocalDbError> {
    let owned = sqlx::query_scalar!(
        r#"SELECT owner_creator_id as "owner_creator_id!" FROM characters WHERE character_id = ?"#,
        character_id
    )
    .fetch_optional(pool)
    .await?;
    if !matches!(owned.as_deref(), Some(stored) if stored == owner_creator_id) {
        return Ok(None);
    }
    let binding = load_binding_pool(pool, binding_id).await?;
    let Some(binding) = binding.filter(|b| b.character_id == character_id) else {
        return Ok(None);
    };
    match require_owned_world_pool(pool, owner_creator_id, &binding.world_id).await {
        Ok(()) => Ok(Some(binding)),
        Err(LocalDbError::ActorNotFound { .. }) => Ok(None),
        Err(err) => Err(err),
    }
}

/// Patch the optional WorldSheet link on an owned active binding.
///
/// Only `world_sheet_entry_id` is mutable. Null clears; omission (`Keep`) retains;
/// material changes bump binding revision once inside `BEGIN IMMEDIATE`.
///
/// # Errors
///
/// Returns `LocalDbError` on ownership, activity, CAS, or WorldSheet validation failure.
pub async fn update_actor_world_binding(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: &str,
    expected_revision: i64,
    world_sheet_entry_id: FieldPatch<&str>,
) -> Result<ActorWorldBindingRecord, LocalDbError> {
    check_expected_revision(expected_revision)?;
    let mut tx = begin_immediate(pool).await?;
    let result = update_actor_world_binding_tx(
        &mut tx,
        owner_creator_id,
        character_id,
        binding_id,
        expected_revision,
        world_sheet_entry_id,
    )
    .await;
    match result {
        Ok(row) => {
            tx.commit().await?;
            Ok(row)
        }
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}

async fn update_actor_world_binding_tx(
    tx: &mut Transaction<'_, Sqlite>,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: &str,
    expected_revision: i64,
    world_sheet_entry_id: FieldPatch<&str>,
) -> Result<ActorWorldBindingRecord, LocalDbError> {
    require_active_owned_character_tx(tx, owner_creator_id, character_id).await?;
    let binding = require_active_character_binding_tx(tx, character_id, binding_id).await?;
    require_owned_active_world(tx, owner_creator_id, &binding.world_id).await?;

    if binding.revision != expected_revision {
        return Err(binding_revision_conflict());
    }
    if binding_sheet_patch_is_no_op(&binding.world_sheet_entry_id, world_sheet_entry_id) {
        return Ok(binding);
    }

    let new_sheet =
        resolved_world_sheet_patch(&binding.world_sheet_entry_id, world_sheet_entry_id);
    validate_world_sheet_tx(tx, &binding.world_id, new_sheet.as_deref()).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let new_revision = expected_revision + 1;
    let updated = map_actor_constraint(
        sqlx::query!(
            r#"UPDATE actor_world_bindings
               SET world_sheet_entry_id = ?, updated_at = ?, revision = ?
               WHERE binding_id = ? AND character_id = ? AND status = 'active' AND revision = ?"#,
            new_sheet,
            now,
            new_revision,
            binding_id,
            character_id,
            expected_revision
        )
        .execute(&mut **tx)
        .await,
    )?
    .rows_affected();
    if updated == 0 {
        return Err(binding_revision_conflict());
    }
    load_binding_tx(tx, binding_id)
        .await?
        .ok_or_else(|| LocalDbError::ActorNotFound {
            resource: "actor_world_binding",
            id: binding_id.to_string(),
        })
}

/// Authoritative binding removal. Last active binding is a zero-mutation 409.
///
/// Decision order: resolve active binding + ownership → count active bindings
/// → reject `count <= 1` → reject binding-owned `KnowledgeEntry` rows → reject
/// binding-local Character memory (`binding_has_local_memory`) → delete
/// exactly the target row. Every reject is zero-mutation.
///
/// # Errors
///
/// Returns `LocalDbError` on not-found, last-binding conflict, or SQL failure.
pub async fn remove_binding(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: &str,
) -> Result<(), LocalDbError> {
    let mut tx = begin_immediate(pool).await?;
    let result = remove_binding_tx(&mut tx, owner_creator_id, character_id, binding_id).await;
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

async fn remove_binding_tx(
    tx: &mut Transaction<'_, Sqlite>,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: &str,
) -> Result<(), LocalDbError> {
    require_active_owned_character_tx(tx, owner_creator_id, character_id).await?;
    let binding = require_active_character_binding_tx(tx, character_id, binding_id).await?;
    require_owned_active_world(tx, owner_creator_id, &binding.world_id).await?;

    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "count!: i64" FROM actor_world_bindings WHERE character_id = ? AND status = 'active'"#,
        character_id
    )
    .fetch_one(&mut **tx)
    .await?;
    if count <= 1 {
        return Err(LocalDbError::ActorContractConflict {
            code: ActorContractConflict::LastActiveBinding,
        });
    }

    let owned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_key_blocks WHERE owner_kind = 'actor_world_binding' AND actor_world_binding_id = ?",
    )
    .bind(binding_id)
    .fetch_one(&mut **tx)
    .await?;
    if owned > 0 {
        return Err(LocalDbError::ActorContractConflict {
            code: ActorContractConflict::BindingHasOwnedKnowledge,
        });
    }

    // v1.184 P3: binding-local Character memory is the final dependency gate.
    // Pending rows, fragments, or narrative cache rows carrying this binding's
    // provenance block removal with a stable zero-mutation 409.
    let local_memory = sqlx::query_scalar!(
        r#"SELECT (SELECT COUNT(*) FROM character_memory_pending_review WHERE actor_world_binding_id = ?) +
                  (SELECT COUNT(*) FROM character_memory_fragments WHERE actor_world_binding_id = ?) +
                  (SELECT COUNT(*) FROM character_soul_narratives WHERE actor_world_binding_id = ?)
                  as "count!: i64""#,
        binding_id,
        binding_id,
        binding_id
    )
    .fetch_one(&mut **tx)
    .await?;
    if local_memory > 0 {
        return Err(LocalDbError::ActorContractConflict {
            code: ActorContractConflict::BindingHasLocalMemory,
        });
    }

    let deleted = sqlx::query!(
        r#"DELETE FROM actor_world_bindings WHERE binding_id = ? AND character_id = ? AND status = 'active'"#,
        binding_id,
        character_id
    )
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if deleted == 0 {
        return Err(LocalDbError::ActorNotFound {
            resource: "actor_world_binding",
            id: binding_id.to_string(),
        });
    }
    Ok(())
}
