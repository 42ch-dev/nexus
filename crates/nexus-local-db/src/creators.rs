//! Workspace `creators` row materialization.
//!
//! The `creators` table lives in the **workspace** state db (per-creator +
//! workspace `SQLite` under ADR-014), while local identities live in the
//! global `~/.nexus42/state.db`. `create_world` prechecks the workspace
//! `creators` table for the owner creator, so any flow that mints a creator
//! outside the workspace db (e.g. `creator register --local`) must also
//! materialize the row here or world creation fails its FK precheck.
//!
//! V1.167 P2 T2 introduced this helper for the CLI local-register path.
//! V1.191 P1 T4 makes it the **one** transaction-taking Creator
//! materialization: the core/daemon creators flows delegate here instead of
//! carrying their own upsert SQL, and the holder registry row is committed with
//! the subject (durable §2.1). Removing a materialized Creator is
//! unreferenced-only ([`delete_creator`]).

use std::fmt::Write as _;

use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::error::LocalDbError;
use crate::holders::{ensure_creator_holder_in_tx, require_subject_holder, HolderSubject};

/// Materialize a workspace Creator and its holder registry row in one transaction.
///
/// §2.1: "before a workspace identity becomes usable its local subject and
/// registry row must commit together". This is the one Creator materialization
/// implementation: the daemon creators flow, the CLI local bootstrap, core
/// workspace selection and this crate's own callers all go through it, so every
/// workspace materialization of the same global Creator commits the same
/// derived `hld_…` id.
///
/// Idempotent: re-running updates `display_name`/`cached_at` in place instead of
/// duplicating the row, and re-ensuring an existing holder is a no-op.
///
/// # Errors
///
/// Returns [`LocalDbError`] if the transaction, either upsert, or the holder
/// registration fails (a registry integrity conflict included).
pub async fn ensure_creator_row(
    pool: &SqlitePool,
    creator_id: &str,
    display_name: &str,
) -> Result<(), LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    match ensure_creator_row_in_tx(&mut tx, creator_id, display_name).await {
        Ok(_holder_entry_id) => {
            tx.commit().await?;
            Ok(())
        }
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}

/// Transaction form of [`ensure_creator_row`]: upsert the subject, then its holder.
///
/// UPDATE-else-INSERT the minimal active `creators` row (`display_name`,
/// `cached_at` RFC3339, `status='active'`, `data='{}'`), then ensure its stable
/// holder in the same transaction.
///
/// Returns the Creator's holder id.
///
/// # Errors
///
/// Returns [`LocalDbError`] on SQL failure or a holder registry integrity
/// conflict (collision/mismatch is never reassignment, §2.1).
pub async fn ensure_creator_row_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    creator_id: &str,
    display_name: &str,
) -> Result<String, LocalDbError> {
    let now = chrono::Utc::now().to_rfc3339();
    let updated = sqlx::query!(
        "UPDATE creators SET display_name = ?, cached_at = ? WHERE creator_id = ?",
        display_name,
        now,
        creator_id
    )
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() == 0 {
        sqlx::query!(
            "INSERT INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, ?, 'active', ?, '{}')",
            creator_id,
            display_name,
            now
        )
        .execute(&mut **tx)
        .await?;
    }
    ensure_creator_holder_in_tx(tx, creator_id).await
}

/// Resolve the stable holder of a stored Creator for a normal read (§2.2).
///
/// Fails closed when the registry row is missing or corrupt — a read never
/// provisions one.
///
/// # Errors
///
/// Returns [`LocalDbError::HolderStateInvalid`] when the Creator has no holder
/// registry row here or the row is corrupt, and [`LocalDbError::Sqlx`] on
/// database failure.
pub async fn require_creator_holder(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<String, LocalDbError> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM creators WHERE creator_id = ?")
        .bind(creator_id)
        .fetch_one(pool)
        .await?;
    if exists == 0 {
        return Err(LocalDbError::ActorNotFound {
            resource: "creator",
            id: creator_id.to_string(),
        });
    }
    require_subject_holder(pool, &HolderSubject::Creator(creator_id.to_string())).await
}

/// Unreferenced-only deletion of a stored Creator and its holder (§2.2).
///
/// The holder row and the workspace `creators` row are removed in one
/// transaction, and only when the subject is unreferenced: governance
/// references (rows governed by the Creator's holder), owned Worlds,
/// Characters and descendants, and any retained creator-owned row refuse the
/// deletion with a zero-mutation `actor_in_use` conflict. The Worlds FK cascade
/// is explicitly pre-counted because deleting a referenced Creator would
/// otherwise drop Worlds and their governed rows silently.
///
/// # Errors
///
/// Returns [`LocalDbError::ActorNotFound`] for a missing Creator,
/// [`LocalDbError::HolderStateInvalid`] when its registry row is missing or
/// corrupt, [`LocalDbError::ActorContractConflict`] (`actor_in_use`) when any
/// reference remains, and [`LocalDbError::Sqlx`] on database failure.
pub async fn delete_creator(pool: &SqlitePool, creator_id: &str) -> Result<(), LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    let result = delete_creator_in_tx(&mut tx, creator_id).await;
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

async fn delete_creator_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    creator_id: &str,
) -> Result<(), LocalDbError> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM creators WHERE creator_id = ?")
        .bind(creator_id)
        .fetch_one(&mut **tx)
        .await?;
    if exists == 0 {
        return Err(LocalDbError::ActorNotFound {
            resource: "creator",
            id: creator_id.to_string(),
        });
    }
    let holder_entry_id =
        require_subject_holder(&mut **tx, &HolderSubject::Creator(creator_id.to_string())).await?;
    let in_use = creator_reference_count(tx, creator_id, &holder_entry_id).await?;
    if in_use > 0 {
        return Err(LocalDbError::actor_in_use());
    }
    // Holder first (it references the subject), then the subject — one
    // transaction, so a refusal anywhere leaves both rows untouched.
    sqlx::query("DELETE FROM knowledge_holders WHERE holder_entry_id = ? AND creator_id = ?")
        .bind(&holder_entry_id)
        .bind(creator_id)
        .execute(&mut **tx)
        .await
        .map_err(LocalDbError::actor_reference_refusal)?;
    sqlx::query("DELETE FROM creators WHERE creator_id = ?")
        .bind(creator_id)
        .execute(&mut **tx)
        .await
        .map_err(LocalDbError::actor_reference_refusal)?;
    Ok(())
}

/// Creator-scoped tables whose rows are retained creator-owned data.
///
/// Any row in one of them refuses Creator deletion (durable §2.2: "any
/// governance reference, binding, retained owned row or existing Actor
/// reference guard refuses deletion"); each is counted through its
/// `creator_id` column.
///
/// This list is not prose: the delete guard builds its pre-count from it, and
/// `v1191_holder_lifecycle_creator_guard_covers_every_creator_scoped_table`
/// compares the two lists below against the live schema, so a table added to
/// the workspace schema cannot silently escape the guard.
pub const CREATOR_RETAINED_TABLES: &[&str] = &[
    "creator_prompt_injections",
    "creator_schedules",
    "findings",
    "force_gates_audit",
    "inspiration_items",
    "kb_extract_jobs",
    "memory_fragments",
    "memory_pending_review",
    "memory_soul_narratives",
    "moment_directives",
    "novel_pool_entries",
    "orchestration_sessions",
    "reading_annotations",
    "reading_progress",
    "reference_sources",
    "soul_meta",
    "works",
    "works_idempotency",
];

/// Creator-scoped tables the pre-count does **not** read through `creator_id`.
///
/// Each entry carries the reason it is already handled. Together with
/// [`CREATOR_RETAINED_TABLES`] this classifies every Creator-identifying column
/// in the live schema (§2.2); an unclassified one fails the coverage case.
pub const CREATOR_SCOPE_HANDLED_ELSEWHERE: &[(&str, &str)] = &[
    ("creators", "the subject row being deleted"),
    (
        "narrative_worlds",
        "owned Worlds are pre-counted on owner_creator_id (the FK is ON DELETE CASCADE)",
    ),
    (
        "characters",
        "owned Characters are pre-counted on owner_creator_id",
    ),
    (
        "knowledge_holders",
        "the subject's own registry row, removed by the same transaction",
    ),
    (
        "kb_key_blocks",
        "governed rows are counted as governance references on holder_entry_id",
    ),
    (
        "knowledge_import_quarantine",
        "quarantine atoms are pre-counted on controlling_creator_id",
    ),
    (
        "core_writer_registration",
        "per-connection writer bookkeeping, not retained owned data",
    ),
    (
        "local_identities",
        "global identity store with its own lifecycle (separate database)",
    ),
];

/// Count every row that still references a Creator subject: governance
/// references (rows governed by its holder), owned Worlds/Characters and the
/// quarantine atoms naming it, and the retained creator-owned rows declared in
/// [`CREATOR_RETAINED_TABLES`].
///
/// Only `narrative_worlds` is foreign-key-`CASCADE`d (and `knowledge_holders`
/// `RESTRICT`ed); the rest carry no Creator FK, so an incomplete inventory
/// would silently orphan rows rather than fail. The inventory is therefore
/// declared as data and guarded by a schema coverage case.
async fn creator_reference_count(
    conn: &mut sqlx::SqliteConnection,
    creator_id: &str,
    holder_entry_id: &str,
) -> Result<i64, LocalDbError> {
    let mut sql = String::from(
        "WITH subject(creator_id, holder_entry_id) AS (VALUES (?, ?)) \
         SELECT \
             (SELECT COUNT(*) FROM narrative_worlds r JOIN subject s ON r.owner_creator_id = s.creator_id) \
           + (SELECT COUNT(*) FROM characters r JOIN subject s ON r.owner_creator_id = s.creator_id) \
           + (SELECT COUNT(*) FROM kb_key_blocks r JOIN subject s ON r.holder_entry_id = s.holder_entry_id) \
           + (SELECT COUNT(*) FROM knowledge_import_quarantine r JOIN subject s ON r.controlling_creator_id = s.creator_id)",
    );
    for table in CREATOR_RETAINED_TABLES {
        write!(
            sql,
            " + (SELECT COUNT(*) FROM {table} r JOIN subject s ON r.creator_id = s.creator_id)"
        )
        .expect("writing into a String cannot fail");
    }
    let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(creator_id)
        .bind(holder_entry_id)
        .fetch_one(conn)
        .await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fresh migrated pool in a tempdir (same pattern as `identity::tests`).
    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::open_pool(&db_path).await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn ensure_creator_row_materializes_active_row() {
        let (pool, _dir) = fresh_pool().await;

        ensure_creator_row(&pool, "ctr_localMat", "Local Materializer")
            .await
            .unwrap();

        // SAFETY: one-off test assertion against the known creators DDL
        // (20260417_000001_initial.sql).
        let row = sqlx::query_as::<_, (String, String, String, String, String)>(
            "SELECT creator_id, display_name, status, cached_at, data \
             FROM creators WHERE creator_id = ?",
        )
        .bind("ctr_localMat")
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(row.0, "ctr_localMat");
        assert_eq!(row.1, "Local Materializer");
        assert_eq!(row.2, "active");
        assert_eq!(row.4, "{}");
        // cached_at must be RFC3339 — mirrors the daemon helper's
        // `chrono::Utc::now().to_rfc3339()`.
        chrono::DateTime::parse_from_rfc3339(&row.3).expect("cached_at must be RFC3339");
    }

    #[tokio::test]
    async fn ensure_creator_row_is_idempotent_and_updates_in_place() {
        let (pool, _dir) = fresh_pool().await;

        ensure_creator_row(&pool, "ctr_localIdem", "First Name")
            .await
            .unwrap();
        ensure_creator_row(&pool, "ctr_localIdem", "First Name")
            .await
            .unwrap();

        // SAFETY: one-off test assertion against the known creators DDL.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM creators WHERE creator_id = ?")
            .bind("ctr_localIdem")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "re-running must not duplicate the row");

        // UPDATE branch: a changed display name is applied in place.
        ensure_creator_row(&pool, "ctr_localIdem", "Renamed")
            .await
            .unwrap();

        // SAFETY: one-off test assertion against the known creators DDL.
        let display_name: String =
            sqlx::query_scalar("SELECT display_name FROM creators WHERE creator_id = ?")
                .bind("ctr_localIdem")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(display_name, "Renamed");
    }
}
