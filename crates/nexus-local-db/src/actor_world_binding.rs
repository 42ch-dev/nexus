//! `ActorWorldBinding` storage and the authoritative last-binding removal transaction.

use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::begin_immediate;
use crate::character::{
    check_expected_revision, map_actor_constraint, require_active_owned_character_tx,
    require_owned_active_world, require_owned_character_pool, require_owned_world_pool, FieldPatch,
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

#[allow(clippy::too_many_arguments)] // row mapper mirrors SQL projection
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
        return Err(invalid_world_sheet());
    }
    if !sheet_id.starts_with("kb_") {
        return Err(invalid_world_sheet());
    }
    let ok: i64 = sqlx::query_scalar(WORLD_SHEET_ELIGIBILITY_EXISTS_SQL)
        .bind(sheet_id)
        .bind(world_id)
        .bind(world_id)
        .fetch_one(&mut **tx)
        .await?;
    if ok == 0 {
        return Err(invalid_world_sheet());
    }
    Ok(())
}

/// `invalid_world_sheet` conflict (durable §3).
pub(crate) const fn invalid_world_sheet() -> LocalDbError {
    LocalDbError::ActorContractConflict {
        code: ActorContractConflict::InvalidWorldSheet,
    }
}

/// The forward (link-time) `WorldSheet` eligibility rule as one `EXISTS`
/// (durable §3): a live, World-owned `character` entry of the same World with
/// no disclosure.
///
/// `disclosure IS NULL` is the native replacement for the retired
/// `creator_only = 0` predicate: a linkable sheet must be shared. `owner_kind =
/// 'world'` plus the `world_id` bind is the same-world clause, `block_type =
/// 'character'` the World-owned-character clause and the status clause the
/// liveness clause.
const WORLD_SHEET_ELIGIBILITY_EXISTS_SQL: &str = "SELECT EXISTS(SELECT 1 FROM kb_key_blocks
            WHERE key_block_id = ?
              AND world_id = ?
              AND owner_kind = 'world'
              AND block_type = 'character'
              AND status NOT IN ('deleted', 'merged', 'deprecated')
              AND disclosure IS NULL)";

/// Reverse-reference guard (durable §3): after a World-owned entry was
/// written, every binding that links it as its `WorldSheet` must still be
/// satisfied by the stored row.
///
/// The forward rule (link time) lives in [`validate_world_sheet_tx`]; this is
/// the same rule evaluated from the binding side, so a governance or status
/// change that would strip a live, World-owned, shared `character` entry of its
/// eligibility refuses with `invalid_world_sheet` instead of silently
/// unlinking the binding. Callers run it inside the same transaction as the
/// material write and roll back on the refusal (zero mutation).
///
/// # Errors
///
/// Returns [`LocalDbError::ActorContractConflict`] `invalid_world_sheet` when
/// any linking binding's sheet no longer satisfies the rule, and
/// `LocalDbError` on database failure.
pub(crate) async fn assert_linked_world_sheets_remain_eligible_tx(
    tx: &mut Transaction<'_, Sqlite>,
    entry_id: &str,
) -> Result<(), LocalDbError> {
    let broken: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM actor_world_bindings b
            WHERE b.world_sheet_entry_id = ?
              AND NOT EXISTS (
                SELECT 1 FROM kb_key_blocks
                WHERE key_block_id = b.world_sheet_entry_id
                  AND world_id = b.world_id
                  AND owner_kind = 'world'
                  AND block_type = 'character'
                  AND status NOT IN ('deleted', 'merged', 'deprecated')
                  AND disclosure IS NULL
              )
         )",
    )
    .bind(entry_id)
    .fetch_one(&mut **tx)
    .await?;
    if broken != 0 {
        return Err(invalid_world_sheet());
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
/// The Character capture also runs it on the *provider's* write transaction,
/// the one that inserts its candidate (compass D15): `remove_binding` takes the
/// same `BEGIN IMMEDIATE` lock, so it cannot delete the binding between this
/// check and the provider write.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` for a missing, foreign, or inactive
/// binding; `LocalDbError` on owner/world or database failure.
pub async fn require_valid_provenance_tx(
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
        require_owned_active_world(&mut tx, params.owner_creator_id, params.world_id).await?;
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

/// Whether an owned Character holds an active binding to an owned World
/// (durable §3 `character-private` audience permission on a World row).
///
/// The probe is transaction-taking on purpose: authoring admission must
/// re-read the stored binding state inside its own transaction, so a
/// bind/unbind racing the write cannot land a private row whose audience was
/// no longer bound when it committed.
///
/// `Ok(false)` covers a missing/inactive binding, a foreign World and a
/// binding whose World is not owned by `owner_creator_id`; the caller owns the
/// refusal shape. The Character's own ownership/activity is a separate,
/// existing admission check — this probe answers only the binding question.
///
/// # Errors
///
/// Returns `LocalDbError` on database failure.
pub(crate) async fn has_active_binding_to_world_tx(
    tx: &mut Transaction<'_, Sqlite>,
    owner_creator_id: &str,
    character_id: &str,
    world_id: &str,
) -> Result<bool, LocalDbError> {
    let exists: i64 = sqlx::query_scalar(ACTIVE_BINDING_TO_WORLD_EXISTS_SQL)
        .bind(character_id)
        .bind(world_id)
        .bind(owner_creator_id)
        .fetch_one(&mut **tx)
        .await?;
    Ok(exists != 0)
}

/// The active-owned-binding `EXISTS` behind [`has_active_binding_to_world_tx`]:
/// the binding is
/// active, targets `world_id`, and that World belongs to `owner_creator_id`.
const ACTIVE_BINDING_TO_WORLD_EXISTS_SQL: &str = "SELECT EXISTS(
        SELECT 1 FROM actor_world_bindings b
        INNER JOIN narrative_worlds w ON w.world_id = b.world_id
        WHERE b.character_id = ?
          AND b.world_id = ?
          AND b.status = 'active'
          AND w.owner_creator_id = ?
     )";

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

const fn binding_revision_conflict() -> LocalDbError {
    LocalDbError::ActorContractConflict {
        code: ActorContractConflict::BindingRevisionConflict,
    }
}

fn resolved_world_sheet_patch(current: Option<&String>, patch: FieldPatch<&str>) -> Option<String> {
    match patch {
        FieldPatch::Keep => current.cloned(),
        FieldPatch::Clear => None,
        FieldPatch::Set(id) => Some(id.to_string()),
    }
}

fn binding_sheet_patch_is_no_op(current: Option<&String>, patch: FieldPatch<&str>) -> bool {
    resolved_world_sheet_patch(current, patch) == current.cloned()
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

/// Patch the optional `WorldSheet` link on an owned active binding.
///
/// Only `world_sheet_entry_id` is mutable. Null clears; omission (`Keep`) retains;
/// material changes bump binding revision once inside `BEGIN IMMEDIATE`.
///
/// # Errors
///
/// Returns `LocalDbError` on ownership, activity, CAS, or `WorldSheet` validation failure.
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
    if binding_sheet_patch_is_no_op(binding.world_sheet_entry_id.as_ref(), world_sheet_entry_id) {
        return Ok(binding);
    }

    let new_sheet =
        resolved_world_sheet_patch(binding.world_sheet_entry_id.as_ref(), world_sheet_entry_id);
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
    commit_binding_removal(
        begin_immediate(pool).await?,
        owner_creator_id,
        character_id,
        binding_id,
    )
    .await
}

async fn commit_binding_removal(
    mut tx: Transaction<'_, Sqlite>,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: &str,
) -> Result<(), LocalDbError> {
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
    // The gate counts what is *served* for this binding, not the historical
    // legacy rows: a legacy row the provider owns under the bridge's
    // deterministic id is the provider's disposition — a dismissed pending
    // review and a promoted fragment no longer hold the binding — while the
    // provider's own binding-placed pending candidates and live records block
    // as before. Ownership is matched on the id alone because a consumed
    // candidate and a tombstoned record carry no body to read the legacy
    // identity from. Narrative cache rows have no provider counterpart yet and
    // keep counting.
    let local_memory = sqlx::query_scalar!(
        r#"SELECT (
                   (SELECT COUNT(*) FROM character_memory_pending_review l
                    WHERE l.actor_world_binding_id = ?
                      AND NOT EXISTS (
                          SELECT 1 FROM memory_candidates c
                          WHERE c.candidate_id =
                              'lgc:character_memory_pending_review:' || l.pending_id)) +
                   (SELECT COUNT(*) FROM memory_candidates c
                    WHERE c.capture_context_id = ? AND c.status = 'pending') +
                   (SELECT COUNT(*) FROM character_memory_fragments l
                    WHERE l.actor_world_binding_id = ?
                      AND NOT EXISTS (
                          SELECT 1 FROM memory_records r
                          WHERE r.record_id =
                              'lgc:character_memory_fragments:' || l.fragment_id)) +
                   (SELECT COUNT(*) FROM memory_records r
                    WHERE r.scope_context_id = ? AND r.tombstone_json IS NULL) +
                   (SELECT COUNT(*) FROM character_soul_narratives
                    WHERE actor_world_binding_id = ?)
                  ) as "count!: i64""#,
        binding_id,
        binding_id,
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

    // D18 (§5.1): a remote binding-local dependency keeps the binding until its
    // own evidence settles. Unknown admissions block even after later retries
    // succeed (the supervisor-selected conservative policy; owner confirmation
    // pending); known live references block until a positive
    // disposal/review/promotion fact closes them. Only terminal facts remain at
    // that point, so they are cleared with the delete.
    let remote = remote_binding_dependency_counts_tx(tx, binding_id).await?;
    if remote.admissions > 0 || remote.live_objects > 0 {
        return Err(LocalDbError::ActorContractConflict {
            code: ActorContractConflict::BindingHasRemoteMemory,
        });
    }
    sqlx::query("DELETE FROM remote_binding_memory_objects WHERE binding_id = ?")
        .bind(binding_id)
        .execute(&mut **tx)
        .await?;

    // The legacy rows the gate proved are no longer served still name this
    // binding in their foreign key (`ON DELETE RESTRICT`), so removal releases
    // that provenance the way the legacy local→shared promotion does: the row
    // stays, the binding it no longer holds goes.
    //
    // PM ruling D14 (compass `## Decisions`, PR #469 final review round 2, B3):
    // this narrow release is legacy-domain foreign-key bookkeeping — the same
    // shape the legacy local→shared promotion uses — NOT memory-content
    // migration, so it is permitted in exactly this scope: only rows the gate
    // above proved the provider already owns, and only the
    // `actor_world_binding_id` column. D6 ("write-idle") governs memory content
    // and the provider cutover; it does not forbid the legacy-domain FK
    // maintenance that has to precede a binding's deletion. The condition is
    // "no **content** column changes": this table's own UPDATE trigger
    // (`bump_character_memory_fragments_revision`) still advances its `revision`
    // audit column on any update, which is the legacy schema's own behaviour and
    // cannot be suppressed without the DDL the ruling forbids. Do not broaden
    // these writes.
    sqlx::query!(
        r"UPDATE character_memory_pending_review SET actor_world_binding_id = NULL
          WHERE actor_world_binding_id = ?
            AND EXISTS (SELECT 1 FROM memory_candidates c
                        WHERE c.candidate_id =
                            'lgc:character_memory_pending_review:' || pending_id)",
        binding_id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        r"UPDATE character_memory_fragments SET actor_world_binding_id = NULL
          WHERE actor_world_binding_id = ?
            AND EXISTS (SELECT 1 FROM memory_records r
                        WHERE r.record_id =
                            'lgc:character_memory_fragments:' || fragment_id)",
        binding_id
    )
    .execute(&mut **tx)
    .await?;

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

// ── Remote binding-memory dependency metadata (D18, binding spec §5.1) ─────
//
// Content-free Nexus bookkeeping for remote binding-local memory: the tables
// carry provider-object identity plus the namespace it was recorded under,
// never memory text, request bodies or credentials.

/// The namespace one remote binding-memory dependency belongs to.
///
/// A fact is only visible to — and only clearable by — the exact provider
/// authority, space, loaded public client identity and Nexus bearer/binding
/// that recorded it, so a stale or foreign namespace can neither read nor
/// erase another namespace's dependency.
pub struct RemoteBindingNamespace<'a> {
    owner_creator_id: &'a str,
    character_id: &'a str,
    binding_id: &'a str,
    authority_host_id: &'a str,
    /// The provider's `NonZeroU64` authority generation as its canonical
    /// decimal text, computed once per namespace: every value in the declared
    /// range is its own identity, so two generations can never alias one row,
    /// and a multi-object receipt reuses the text instead of reformatting the
    /// same generation per object write.
    authority_generation_text: String,
    space_id: &'a str,
    client_identity: &'a str,
}

impl<'a> RemoteBindingNamespace<'a> {
    /// Build the one namespace an operation's dependency writes share.
    ///
    /// The canonical generation text is derived here, from the exact numeric
    /// identity, so no caller can supply an independently formatted or
    /// divergent text for the same generation.
    #[must_use]
    pub fn new(
        owner_creator_id: &'a str,
        character_id: &'a str,
        binding_id: &'a str,
        authority_host_id: &'a str,
        authority_generation: u64,
        space_id: &'a str,
        client_identity: &'a str,
    ) -> Self {
        Self {
            owner_creator_id,
            character_id,
            binding_id,
            authority_host_id,
            authority_generation_text: authority_generation.to_string(),
            space_id,
            client_identity,
        }
    }

    /// The canonical decimal generation text every dependency row binds.
    #[must_use]
    pub fn generation_text(&self) -> &str {
        &self.authority_generation_text
    }
}

/// One known remote provider object a binding depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteBindingObjectKind {
    Candidate,
    Record,
}

impl RemoteBindingObjectKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Record => "record",
        }
    }
}

/// One monotone known-object state (D18): `live → consumed → released`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteBindingObjectState {
    Live,
    Consumed,
    Released,
}

impl RemoteBindingObjectState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Consumed => "consumed",
            Self::Released => "released",
        }
    }
}

/// The binding's open remote dependencies: unknown wire attempts and known
/// live object references. Terminal (consumed/released) facts are evidence,
/// not dependencies, and never appear here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteBindingDependencyCounts {
    pub admissions: i64,
    pub live_objects: i64,
}

/// Record one independently identified remote capture admission (D18).
///
/// The caller commits it in the same writer transaction as the provenance
/// check and before the RPC; only the definite result of this exact wire
/// attempt may retire it.
///
/// # Errors
///
/// Returns [`LocalDbError`] when the row cannot be written.
pub async fn insert_remote_binding_admission_tx(
    tx: &mut Transaction<'_, Sqlite>,
    admission_id: &str,
    namespace: &RemoteBindingNamespace<'_>,
    now: &str,
) -> Result<(), LocalDbError> {
    sqlx::query(
        r"INSERT INTO remote_binding_memory_admissions
          (admission_id, owner_creator_id, character_id, binding_id, authority_host_id,
           authority_generation, space_id, client_identity, created_at)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(admission_id)
    .bind(namespace.owner_creator_id)
    .bind(namespace.character_id)
    .bind(namespace.binding_id)
    .bind(namespace.authority_host_id)
    .bind(namespace.generation_text())
    .bind(namespace.space_id)
    .bind(namespace.client_identity)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Retire exactly the named admission of this namespace.
///
/// Returns `false` when no row of that identity exists (already retired, or a
/// foreign namespace), in which case nothing changed. No later attempt,
/// receipt or counter can retire another admission through this helper.
///
/// # Errors
///
/// Returns [`LocalDbError`] when the delete cannot run.
pub async fn retire_remote_binding_admission_tx(
    tx: &mut Transaction<'_, Sqlite>,
    admission_id: &str,
    namespace: &RemoteBindingNamespace<'_>,
) -> Result<bool, LocalDbError> {
    let retired = sqlx::query(
        r"DELETE FROM remote_binding_memory_admissions
          WHERE admission_id = ? AND owner_creator_id = ? AND character_id = ?
            AND binding_id = ? AND authority_host_id = ? AND authority_generation = ?
            AND space_id = ? AND client_identity = ?",
    )
    .bind(admission_id)
    .bind(namespace.owner_creator_id)
    .bind(namespace.character_id)
    .bind(namespace.binding_id)
    .bind(namespace.authority_host_id)
    .bind(namespace.generation_text())
    .bind(namespace.space_id)
    .bind(namespace.client_identity)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(retired > 0)
}

/// Record one known provider-object fact for this binding (D18).
///
/// The write is monotone (`live → consumed → released`) and conditional on the
/// binding still existing, so a late acknowledgement can neither resurrect a
/// terminal fact nor revive a deleted binding. A fact recorded under one
/// namespace is never rewritten by another.
///
/// # Errors
///
/// Returns [`LocalDbError`] when the write cannot run.
pub async fn record_remote_binding_object_tx(
    tx: &mut Transaction<'_, Sqlite>,
    namespace: &RemoteBindingNamespace<'_>,
    kind: RemoteBindingObjectKind,
    object_id: &str,
    state: RemoteBindingObjectState,
    now: &str,
) -> Result<(), LocalDbError> {
    sqlx::query(
        r"INSERT INTO remote_binding_memory_objects
          (authority_host_id, authority_generation, space_id, client_identity, character_id,
           binding_id, object_kind, object_id, state, created_at, updated_at)
          SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
          WHERE EXISTS (SELECT 1 FROM actor_world_bindings WHERE binding_id = ?)
          ON CONFLICT (authority_host_id, authority_generation, space_id, client_identity,
                       character_id, binding_id, object_kind, object_id)
          DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at
          WHERE (CASE excluded.state WHEN 'live' THEN 0 WHEN 'consumed' THEN 1 ELSE 2 END)
              > (CASE remote_binding_memory_objects.state
                      WHEN 'live' THEN 0 WHEN 'consumed' THEN 1 ELSE 2 END)",
    )
    .bind(namespace.authority_host_id)
    .bind(namespace.generation_text())
    .bind(namespace.space_id)
    .bind(namespace.client_identity)
    .bind(namespace.character_id)
    .bind(namespace.binding_id)
    .bind(kind.as_str())
    .bind(object_id)
    .bind(state.as_str())
    .bind(now)
    .bind(now)
    .bind(namespace.binding_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The binding's open remote dependencies for the removal gate.
///
/// # Errors
///
/// Returns [`LocalDbError`] when the counts cannot be read.
pub async fn remote_binding_dependency_counts_tx(
    tx: &mut Transaction<'_, Sqlite>,
    binding_id: &str,
) -> Result<RemoteBindingDependencyCounts, LocalDbError> {
    let admissions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM remote_binding_memory_admissions WHERE binding_id = ?",
    )
    .bind(binding_id)
    .fetch_one(&mut **tx)
    .await?;
    let live_objects: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM remote_binding_memory_objects \
         WHERE binding_id = ? AND state = 'live'",
    )
    .bind(binding_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(RemoteBindingDependencyCounts {
        admissions,
        live_objects,
    })
}

/// Whether this namespace recorded any D18 fact for one provider object.
///
/// A positive answer is authoritative Nexus origin evidence for that exact
/// object: only this module writes those facts, and the lookup binds the same
/// eight identity columns the write's conflict key uses plus the canonical
/// generation text, so a foreign authority/space/client/bearer cannot answer for
/// another namespace's object. A negative answer proves nothing about where the
/// object came from — callers MUST NOT read it as "provider-native".
///
/// # Errors
///
/// Returns [`LocalDbError`] when the check cannot run.
pub async fn remote_binding_object_is_known(
    pool: &SqlitePool,
    namespace: &RemoteBindingNamespace<'_>,
    kind: RemoteBindingObjectKind,
    object_id: &str,
) -> Result<bool, LocalDbError> {
    let known: i64 = sqlx::query_scalar(
        r"SELECT EXISTS (
              SELECT 1 FROM remote_binding_memory_objects
              WHERE authority_host_id = ? AND authority_generation = ? AND space_id = ?
                AND client_identity = ? AND character_id = ? AND binding_id = ?
                AND object_kind = ? AND object_id = ?
          )",
    )
    .bind(namespace.authority_host_id)
    .bind(namespace.generation_text())
    .bind(namespace.space_id)
    .bind(namespace.client_identity)
    .bind(namespace.character_id)
    .bind(namespace.binding_id)
    .bind(kind.as_str())
    .bind(object_id)
    .fetch_one(pool)
    .await?;
    Ok(known != 0)
}
