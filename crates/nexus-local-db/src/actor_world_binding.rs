//! ActorWorldBinding storage and the authoritative last-binding removal transaction.

use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::character::{
    map_actor_constraint, require_active_owned_character, require_owned_character,
    require_owned_world,
};
use crate::error::{ActorContractConflict, LocalDbError};
use crate::begin_immediate;

/// Persisted binding row (storage shape; not a wire DTO).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorWorldBindingRecord {
    pub binding_id: String,
    pub character_id: String,
    pub world_id: String,
    pub status: String,
    pub world_sheet_entry_id: Option<String>,
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

fn record_from_query(
    binding_id: String,
    character_id: String,
    world_id: String,
    status: String,
    world_sheet_entry_id: Option<String>,
    created_at: String,
    updated_at: String,
) -> ActorWorldBindingRecord {
    ActorWorldBindingRecord {
        binding_id,
        character_id,
        world_id,
        status,
        world_sheet_entry_id,
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
    let ok = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM kb_key_blocks
            WHERE key_block_id = ?
              AND world_id = ?
              AND block_type = 'character'
              AND status != 'deleted'
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
            r.created_at,
            r.updated_at,
        )
    }))
}

/// Add a second (or later) active binding for an owned active Character.
///
/// # Errors
///
/// Returns `LocalDbError` on ownership, WorldSheet, duplicate, or SQL failure.
pub async fn add_actor_world_binding(
    pool: &SqlitePool,
    params: CreateBindingParams<'_>,
) -> Result<ActorWorldBindingRecord, LocalDbError> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = begin_immediate(pool).await?;
    let result = async {
        require_active_owned_character(&mut tx, params.owner_creator_id, params.character_id)
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
) -> Result<Vec<ActorWorldBindingRecord>, LocalDbError> {
    let mut tx = begin_immediate(pool).await?;
    require_owned_character(&mut tx, owner_creator_id, character_id).await?;
    let rows = sqlx::query!(
        r#"SELECT binding_id as "binding_id!",
                  character_id as "character_id!",
                  world_id as "world_id!",
                  status as "status!",
                  world_sheet_entry_id,
                  created_at as "created_at!",
                  updated_at as "updated_at!"
           FROM actor_world_bindings WHERE character_id = ? ORDER BY created_at ASC, binding_id ASC"#,
        character_id
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            record_from_query(
                r.binding_id,
                r.character_id,
                r.world_id,
                r.status,
                r.world_sheet_entry_id,
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
pub async fn count_active_bindings_for_world_tx(
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

/// Authoritative binding removal. Last active binding is a zero-mutation 409.
///
/// Decision order: resolve active binding + ownership → count active bindings
/// → reject `count <= 1` → delete exactly the target row.
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
    require_owned_character(tx, owner_creator_id, character_id).await?;
    let binding = load_binding_tx(tx, binding_id).await?;
    let Some(binding) = binding else {
        return Err(LocalDbError::ActorNotFound {
            resource: "actor_world_binding",
            id: binding_id.to_string(),
        });
    };
    if binding.character_id != character_id || binding.status != "active" {
        return Err(LocalDbError::ActorNotFound {
            resource: "actor_world_binding",
            id: binding_id.to_string(),
        });
    }
    require_owned_world(tx, owner_creator_id, &binding.world_id).await?;

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
