//! ActorWorldBinding storage and the authoritative last-binding removal transaction.

use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::character::{map_actor_constraint, require_owned_character, require_owned_world};
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

fn row_to_binding(r: sqlx::sqlite::SqliteRow) -> ActorWorldBindingRecord {
    ActorWorldBindingRecord {
        binding_id: r.get("binding_id"),
        character_id: r.get("character_id"),
        world_id: r.get("world_id"),
        status: r.get("status"),
        world_sheet_entry_id: r.get("world_sheet_entry_id"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
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
    // SAFETY: WorldSheet cross-table rule (P0 world_id predicate).
    let ok: i64 = sqlx::query_scalar(
        "SELECT EXISTS(\
            SELECT 1 FROM kb_key_blocks \
            WHERE key_block_id = ? \
              AND world_id = ? \
              AND block_type = 'character' \
              AND status != 'deleted'\
         )",
    )
    .bind(sheet_id)
    .bind(world_id)
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
    // SAFETY: INSERT matches actor_world_bindings DDL in 202609050001_actor_identity.sql.
    let insert = sqlx::query(
        "INSERT INTO actor_world_bindings \
         (binding_id, character_id, world_id, status, world_sheet_entry_id, created_at, updated_at) \
         VALUES (?, ?, ?, 'active', ?, ?, ?)",
    )
    .bind(&binding_id)
    .bind(params.character_id)
    .bind(params.world_id)
    .bind(params.world_sheet_entry_id)
    .bind(now)
    .bind(now)
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
    // SAFETY: SELECT matches actor_world_bindings DDL.
    let row = sqlx::query(
        "SELECT binding_id, character_id, world_id, status, world_sheet_entry_id, created_at, updated_at \
         FROM actor_world_bindings WHERE binding_id = ?",
    )
    .bind(binding_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(row_to_binding))
}

/// Add a second (or later) active binding for an owned Character.
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
        require_owned_character(&mut tx, params.owner_creator_id, params.character_id).await?;
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
    // SAFETY: SELECT matches actor_world_bindings DDL.
    let rows = sqlx::query(
        "SELECT binding_id, character_id, world_id, status, world_sheet_entry_id, created_at, updated_at \
         FROM actor_world_bindings WHERE character_id = ? ORDER BY created_at ASC, binding_id ASC",
    )
    .bind(character_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.into_iter().map(row_to_binding).collect())
}

/// Count active bindings for a World inside an open write transaction.
///
/// # Errors
///
/// Returns `LocalDbError` on database failure.
pub async fn count_active_bindings_for_world_tx(
    tx: &mut Transaction<'_, Sqlite>,
    world_id: &str,
) -> Result<i64, LocalDbError> {
    // SAFETY: COUNT against actor_world_bindings.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM actor_world_bindings WHERE world_id = ? AND status = 'active'",
    )
    .bind(world_id)
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

    // SAFETY: cardinality count inside the reserved lock.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ? AND status = 'active'",
    )
    .bind(character_id)
    .fetch_one(&mut **tx)
    .await?;
    if count <= 1 {
        return Err(LocalDbError::ActorContractConflict {
            code: ActorContractConflict::LastActiveBinding,
        });
    }

    // SAFETY: delete exactly the target active row.
    let deleted = sqlx::query(
        "DELETE FROM actor_world_bindings WHERE binding_id = ? AND character_id = ? AND status = 'active'",
    )
    .bind(binding_id)
    .bind(character_id)
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
