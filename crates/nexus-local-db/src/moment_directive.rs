//! Moment Directive storage (V1.150 P1, DF-75) — persistent, scoped author
//! instruction for the MCA `moment.directive` slot (spec
//! `fl-l-w5-prompt-control-plane.md` §3).
//!
//! This is a **persistent scoped directive** (at most one `active` row per
//! `(creator_id, scope_kind, scope_id)`), NOT a queue — it deliberately does
//! NOT mirror the `creator_prompt_injections` queue lifecycle (guide
//! `mca-section-audit.md` Q8). It reuses the same migration + repository
//! pattern (`prompt_injection.rs`), but the lifecycle is:
//!
//! `active` → (`ttl_remaining` hits 0 | scene clear | manual `clear` |
//! `--replace`) → `expired` (soft-delete: `status='expired'`, `expires_at`
//! set; rows retained for DF-76 inspection).
//!
//! # Scope resolution (spec §3.2)
//!
//! The repository exposes per-scope lookups (`get_active_for_work` /
//! `get_active_for_world`). The Work-wins / World-override composition lives
//! at the composition root (the `DirectiveStore` adapter in `nexus42`) so
//! the Work→World binding can be verified against the `works` table — this
//! module stays a pure storage layer.
//!
//! # Lifecycle bookkeeping columns
//!
//! `last_focused_event_id` / `last_chapter_no` persist the cross-assemble
//! state that drives the TTL + scene-change signals (spec §3.3, guide Q7):
//! a `MomentRequest.event_id` change between injecting assembles is the
//! scene-change proxy; a `works.current_chapter` advance is the chapter-
//! advance signal for novel Works.
//!
//! # Never on the spoke wire (AC-I3)
//!
//! This table is product-local prompt control. Nothing here is a
//! `KnowledgeEntry`, a `modules.*` object, or an `AssemblePacket`
//! `placement[]` / `activation_trace[]` entry.

use sqlx::SqlitePool;

use crate::error::LocalDbError;

/// Scope kinds for a Moment Directive (`scope_kind` column values).
pub mod scope_kind {
    /// Work-scoped directive (`scope_id` = work id).
    pub const WORK: &str = "work";
    /// World-scoped override (`scope_id` = world id).
    pub const WORLD: &str = "world";
}

/// Moment Directive row — mirrors the `moment_directives` DB row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MomentDirectiveRow {
    /// Unique directive id (application-generated).
    pub directive_id: String,
    /// Owning creator.
    pub creator_id: String,
    /// `work` | `world`.
    pub scope_kind: String,
    /// Work id (`scope_kind` = `work`) or world id (`scope_kind` = `world`).
    pub scope_id: String,
    /// Author instruction text (non-empty after trim).
    pub body: String,
    /// `head` | `mid` | `tail` — placement within the directive region.
    pub insert_depth: String,
    /// `generations` | `chapters`.
    pub ttl_kind: String,
    /// Remaining TTL count (decremented in place; 0 ⇒ expired).
    pub ttl_remaining: i64,
    /// Clear when the focused moment anchor changes between assembles.
    pub clear_on_scene_change: bool,
    /// `active` | `expired` (soft-delete).
    pub status: String,
    /// Last focused `MomentRequest.event_id` seen at an injecting assemble
    /// (scene-change signal; `NULL` until the first injection).
    pub last_focused_event_id: Option<String>,
    /// Last observed `works.current_chapter` at an injecting assemble
    /// (chapter-advance signal for novel Works; `NULL` until the first
    /// injection / for non-novel Works).
    pub last_chapter_no: Option<i64>,
    /// Unix epoch millis when created.
    pub created_at: i64,
    /// Unix epoch millis of the last lifecycle write.
    pub updated_at: i64,
    /// Unix epoch millis when soft-deleted (TTL-0 / scene-clear / manual clear).
    pub expires_at: Option<i64>,
    /// New directive id when `--replace` superseded this row.
    pub replaced_by: Option<String>,
}

/// Parameters for creating a new active Moment Directive.
#[derive(Debug, Clone)]
pub struct NewMomentDirective<'a> {
    /// Unique directive id (caller-generated).
    pub directive_id: &'a str,
    /// Owning creator.
    pub creator_id: &'a str,
    /// `scope_kind::WORK` | `scope_kind::WORLD`.
    pub scope_kind: &'a str,
    /// Work id or world id matching `scope_kind`.
    pub scope_id: &'a str,
    /// Author instruction text (must be non-empty after trim — validated by
    /// the CLI; the DB only enforces non-NULL).
    pub body: &'a str,
    /// `head` | `mid` | `tail`.
    pub insert_depth: &'a str,
    /// `generations` | `chapters`.
    pub ttl_kind: &'a str,
    /// Starting TTL count (must be ≥ 1 — validated by the CLI).
    pub ttl_remaining: i64,
    /// Clear on scene change (default false).
    pub clear_on_scene_change: bool,
    /// Unix epoch millis for `created_at` / `updated_at`.
    pub now: i64,
}

/// Insert a new `active` directive for a scope.
///
/// The unique partial index `moment_directives_one_active_per_scope` rejects
/// a second active row for the same `(creator_id, scope_kind, scope_id)` —
/// callers surface that as "an active directive exists; use `--replace`"
/// (spec §3.1: no silent overwrite).
///
/// # Errors
///
/// Returns `LocalDbError::Sqlx` on database failure (including the unique
/// partial-index violation when a directive is already active in the scope).
pub async fn set_active(
    pool: &SqlitePool,
    new: &NewMomentDirective<'_>,
) -> Result<MomentDirectiveRow, LocalDbError> {
    insert_active_row(pool, new).await
}

/// Shared INSERT behind `set_active` / `replace_active` — executor-generic so
/// the replace path can insert inside its transaction.
async fn insert_active_row<'e, E>(
    executor: E,
    new: &NewMomentDirective<'_>,
) -> Result<MomentDirectiveRow, LocalDbError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query!(
        "INSERT INTO moment_directives
            (directive_id, creator_id, scope_kind, scope_id, body, insert_depth,
             ttl_kind, ttl_remaining, clear_on_scene_change, status,
             created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, ?)",
        new.directive_id,
        new.creator_id,
        new.scope_kind,
        new.scope_id,
        new.body,
        new.insert_depth,
        new.ttl_kind,
        new.ttl_remaining,
        new.clear_on_scene_change,
        new.now,
        new.now,
    )
    .execute(executor)
    .await?;
    Ok(MomentDirectiveRow {
        directive_id: new.directive_id.to_string(),
        creator_id: new.creator_id.to_string(),
        scope_kind: new.scope_kind.to_string(),
        scope_id: new.scope_id.to_string(),
        body: new.body.to_string(),
        insert_depth: new.insert_depth.to_string(),
        ttl_kind: new.ttl_kind.to_string(),
        ttl_remaining: new.ttl_remaining,
        clear_on_scene_change: new.clear_on_scene_change,
        status: "active".to_string(),
        last_focused_event_id: None,
        last_chapter_no: None,
        created_at: new.now,
        updated_at: new.now,
        expires_at: None,
        replaced_by: None,
    })
}

/// Supersede an existing active directive in the scope with a new one
/// (`--replace`, spec §3.1 / §3.3).
///
/// Runs in a transaction: the old active row (if any) is soft-deleted with
/// `replaced_by = new directive id`, then the new row is inserted. The
/// resulting scope always has exactly one active directive.
///
/// # Errors
///
/// Returns `LocalDbError` if the transaction fails.
pub async fn replace_active(
    pool: &SqlitePool,
    new: &NewMomentDirective<'_>,
) -> Result<MomentDirectiveRow, LocalDbError> {
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "UPDATE moment_directives
         SET status = 'expired', expires_at = ?, replaced_by = ?, updated_at = ?
         WHERE creator_id = ? AND scope_kind = ? AND scope_id = ? AND status = 'active'",
        new.now,
        new.directive_id,
        new.now,
        new.creator_id,
        new.scope_kind,
        new.scope_id,
    )
    .execute(&mut *tx)
    .await?;
    let row = insert_active_row(&mut *tx, new).await?;
    tx.commit().await?;
    Ok(row)
}

/// Fetch the active directive for a Work scope.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_active_for_work(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<Option<MomentDirectiveRow>, LocalDbError> {
    get_active_by_scope(pool, creator_id, scope_kind::WORK, work_id).await
}

/// Fetch the active directive for a World scope (the World override).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_active_for_world(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> Result<Option<MomentDirectiveRow>, LocalDbError> {
    get_active_by_scope(pool, creator_id, scope_kind::WORLD, world_id).await
}

/// Fetch a directive row by id (any status — used by the lifecycle path).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_by_id(
    pool: &SqlitePool,
    directive_id: &str,
) -> Result<Option<MomentDirectiveRow>, LocalDbError> {
    let row = sqlx::query_as!(
        MomentDirectiveRow,
        // `directive_id!` — SQLite describes a TEXT PRIMARY KEY as nullable,
        // the macro needs the non-null coercion for the `String` field;
        // `clear_on_scene_change: bool` — the column is INTEGER, the struct
        // field is `bool` (explicit decode override).
        "SELECT directive_id as \"directive_id!\",
                creator_id, scope_kind, scope_id, body, insert_depth,
                ttl_kind, ttl_remaining, clear_on_scene_change as \"clear_on_scene_change: bool\",
                status,
                last_focused_event_id, last_chapter_no, created_at, updated_at,
                expires_at, replaced_by
         FROM moment_directives
         WHERE directive_id = ?",
        directive_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Manual clear — soft-delete the active directive in a scope immediately
/// (spec §3.3, "Manual clear"; no TTL wait).
///
/// Returns `true` when an active row was soft-deleted.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn clear(
    pool: &SqlitePool,
    creator_id: &str,
    scope_kind: &str,
    scope_id: &str,
    now: i64,
) -> Result<bool, LocalDbError> {
    let result = sqlx::query!(
        "UPDATE moment_directives
         SET status = 'expired', expires_at = ?, updated_at = ?
         WHERE creator_id = ? AND scope_kind = ? AND scope_id = ? AND status = 'active'",
        now,
        now,
        creator_id,
        scope_kind,
        scope_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Decrement `ttl_remaining` by 1 for an active directive (spec §3.3 TTL
/// expiry); when it reaches 0 the row is soft-deleted (`status='expired'`,
/// `expires_at` set).
///
/// Returns the updated row, or `None` when the directive is not active (or
/// unknown).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn decrement_ttl(
    pool: &SqlitePool,
    directive_id: &str,
    now: i64,
) -> Result<Option<MomentDirectiveRow>, LocalDbError> {
    // Single round-trip `UPDATE … RETURNING` (QC3-S002): the updated row comes
    // back in the same statement as the write — no `get_by_id` read-back race,
    // and no spurious "failed" log when the read-back would fail after a
    // successful write.
    let row = sqlx::query_as!(
        MomentDirectiveRow,
        // Column annotations as in `get_by_id` (SQLite TEXT-PK nullability
        // quirk + INTEGER→bool decode override).
        "UPDATE moment_directives
         SET ttl_remaining = MAX(ttl_remaining - 1, 0),
             status = CASE WHEN ttl_remaining <= 1 THEN 'expired' ELSE status END,
             expires_at = CASE WHEN ttl_remaining <= 1 THEN ? ELSE expires_at END,
             updated_at = ?
         WHERE directive_id = ? AND status = 'active'
         RETURNING directive_id as \"directive_id!\",
                   creator_id, scope_kind, scope_id, body, insert_depth,
                   ttl_kind, ttl_remaining, clear_on_scene_change as \"clear_on_scene_change: bool\",
                   status,
                   last_focused_event_id, last_chapter_no, created_at, updated_at,
                   expires_at, replaced_by",
        now,
        now,
        directive_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Scene clear — soft-delete an active directive because the focused moment
/// anchor changed between injecting assembles (`clear_on_scene_change`,
/// spec §3.3).
///
/// Returns the soft-deleted row, or `None` when the directive is not active
/// (or unknown).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn clear_on_scene_change(
    pool: &SqlitePool,
    directive_id: &str,
    now: i64,
) -> Result<Option<MomentDirectiveRow>, LocalDbError> {
    // Single round-trip `UPDATE … RETURNING` (QC3-S002) — see `decrement_ttl`.
    let row = sqlx::query_as!(
        MomentDirectiveRow,
        // Column annotations as in `get_by_id` (SQLite TEXT-PK nullability
        // quirk + INTEGER→bool decode override).
        "UPDATE moment_directives
         SET status = 'expired', expires_at = ?, updated_at = ?
         WHERE directive_id = ? AND status = 'active'
         RETURNING directive_id as \"directive_id!\",
                   creator_id, scope_kind, scope_id, body, insert_depth,
                   ttl_kind, ttl_remaining, clear_on_scene_change as \"clear_on_scene_change: bool\",
                   status,
                   last_focused_event_id, last_chapter_no, created_at, updated_at,
                   expires_at, replaced_by",
        now,
        now,
        directive_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Record the cross-assemble lifecycle anchors on an active directive
/// (spec §3.3): the `event_id` / chapter observed at this injecting assemble.
///
/// Returns `true` when an active row was updated.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn update_lifecycle_anchor(
    pool: &SqlitePool,
    directive_id: &str,
    last_focused_event_id: Option<&str>,
    last_chapter_no: Option<i64>,
    now: i64,
) -> Result<bool, LocalDbError> {
    let result = sqlx::query!(
        "UPDATE moment_directives
         SET last_focused_event_id = ?, last_chapter_no = ?, updated_at = ?
         WHERE directive_id = ? AND status = 'active'",
        last_focused_event_id,
        last_chapter_no,
        now,
        directive_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

async fn get_active_by_scope(
    pool: &SqlitePool,
    creator_id: &str,
    scope_kind: &str,
    scope_id: &str,
) -> Result<Option<MomentDirectiveRow>, LocalDbError> {
    let row = sqlx::query_as!(
        MomentDirectiveRow,
        // Column annotations as in `get_by_id` (SQLite TEXT-PK nullability
        // quirk + INTEGER→bool decode override).
        "SELECT directive_id as \"directive_id!\",
                creator_id, scope_kind, scope_id, body, insert_depth,
                ttl_kind, ttl_remaining, clear_on_scene_change as \"clear_on_scene_change: bool\",
                status,
                last_focused_event_id, last_chapter_no, created_at, updated_at,
                expires_at, replaced_by
         FROM moment_directives
         WHERE creator_id = ? AND scope_kind = ? AND scope_id = ? AND status = 'active'",
        creator_id,
        scope_kind,
        scope_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_pool, run_migrations};

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    fn new_directive<'a>(
        directive_id: &'a str,
        scope_kind: &'a str,
        scope_id: &'a str,
        ttl_remaining: i64,
        now: i64,
    ) -> NewMomentDirective<'a> {
        NewMomentDirective {
            directive_id,
            creator_id: "ctr_test",
            scope_kind,
            scope_id,
            body: "Keep the prose terse.",
            insert_depth: "mid",
            ttl_kind: "generations",
            ttl_remaining,
            clear_on_scene_change: false,
            now,
        }
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn set_and_get_round_trip() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();
        set_active(
            &pool,
            &new_directive("dir_1", scope_kind::WORK, "wrk_1", 3, now),
        )
        .await
        .unwrap();

        let row = get_active_for_work(&pool, "ctr_test", "wrk_1")
            .await
            .unwrap()
            .expect("active directive");
        assert_eq!(row.directive_id, "dir_1");
        assert_eq!(row.body, "Keep the prose terse.");
        assert_eq!(row.insert_depth, "mid");
        assert_eq!(row.ttl_remaining, 3);
        assert_eq!(row.status, "active");
        assert!(!row.clear_on_scene_change);
        assert!(row.expires_at.is_none());
        assert!(row.replaced_by.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn second_active_in_scope_rejected_by_partial_index() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();
        set_active(
            &pool,
            &new_directive("dir_1", scope_kind::WORK, "wrk_1", 3, now),
        )
        .await
        .unwrap();

        let err = set_active(
            &pool,
            &new_directive("dir_2", scope_kind::WORK, "wrk_1", 3, now),
        )
        .await
        .unwrap_err();
        assert!(
            format!("{err}").contains("UNIQUE constraint failed"),
            "at-most-one-active-per-scope must be DB-enforced, got: {err}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn same_directive_ok_in_different_scopes() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();
        set_active(
            &pool,
            &new_directive("dir_w", scope_kind::WORK, "wrk_1", 3, now),
        )
        .await
        .unwrap();
        set_active(
            &pool,
            &new_directive("dir_x", scope_kind::WORLD, "wld_1", 3, now),
        )
        .await
        .unwrap();
        set_active(
            &pool,
            &new_directive("dir_y", scope_kind::WORK, "wrk_2", 3, now),
        )
        .await
        .unwrap();

        assert!(get_active_for_work(&pool, "ctr_test", "wrk_1")
            .await
            .unwrap()
            .is_some());
        assert!(get_active_for_work(&pool, "ctr_test", "wrk_2")
            .await
            .unwrap()
            .is_some());
        assert!(get_active_for_world(&pool, "ctr_test", "wld_1")
            .await
            .unwrap()
            .is_some());
        // A different creator never sees the directives.
        assert!(get_active_for_work(&pool, "ctr_other", "wrk_1")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn replace_active_soft_deletes_old_with_replaced_by() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();
        set_active(
            &pool,
            &new_directive("dir_old", scope_kind::WORK, "wrk_1", 3, now),
        )
        .await
        .unwrap();

        let fresh = new_directive("dir_new", scope_kind::WORK, "wrk_1", 5, now + 1);
        let inserted = replace_active(&pool, &fresh).await.unwrap();
        assert_eq!(inserted.directive_id, "dir_new");
        assert_eq!(inserted.ttl_remaining, 5);

        let active = get_active_for_work(&pool, "ctr_test", "wrk_1")
            .await
            .unwrap()
            .expect("new directive active");
        assert_eq!(active.directive_id, "dir_new");

        let old = get_by_id(&pool, "dir_old")
            .await
            .unwrap()
            .expect("old row retained");
        assert_eq!(old.status, "expired");
        assert_eq!(old.replaced_by.as_deref(), Some("dir_new"));
        assert!(old.expires_at.is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn decrement_ttl_expires_at_zero() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();
        set_active(
            &pool,
            &new_directive("dir_1", scope_kind::WORK, "wrk_1", 1, now),
        )
        .await
        .unwrap();

        let after = decrement_ttl(&pool, "dir_1", now + 1)
            .await
            .unwrap()
            .expect("row updated");
        assert_eq!(after.ttl_remaining, 0);
        assert_eq!(after.status, "expired");
        assert!(after.expires_at.is_some());
        // Expired rows no longer inject.
        assert!(get_active_for_work(&pool, "ctr_test", "wrk_1")
            .await
            .unwrap()
            .is_none());
        // Idempotent: further decrements are no-ops.
        assert!(decrement_ttl(&pool, "dir_1", now + 2)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn decrement_ttl_stays_active_until_zero() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();
        set_active(
            &pool,
            &new_directive("dir_1", scope_kind::WORK, "wrk_1", 2, now),
        )
        .await
        .unwrap();

        let after = decrement_ttl(&pool, "dir_1", now + 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.ttl_remaining, 1);
        assert_eq!(after.status, "active");
        assert!(after.expires_at.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn clear_soft_deletes_active_row() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();
        set_active(
            &pool,
            &new_directive("dir_1", scope_kind::WORK, "wrk_1", 3, now),
        )
        .await
        .unwrap();

        let cleared = clear(&pool, "ctr_test", scope_kind::WORK, "wrk_1", now + 1)
            .await
            .unwrap();
        assert!(cleared, "an active row was cleared");

        assert!(get_active_for_work(&pool, "ctr_test", "wrk_1")
            .await
            .unwrap()
            .is_none());
        let row = get_by_id(&pool, "dir_1").await.unwrap().expect("retained");
        assert_eq!(row.status, "expired");
        assert!(row.expires_at.is_some());

        // Clearing an already-empty scope reports false.
        let again = clear(&pool, "ctr_test", scope_kind::WORK, "wrk_1", now + 2)
            .await
            .unwrap();
        assert!(!again);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scene_clear_soft_deletes_and_is_idempotent() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();
        set_active(
            &pool,
            &new_directive("dir_1", scope_kind::WORLD, "wld_1", 5, now),
        )
        .await
        .unwrap();

        let row = clear_on_scene_change(&pool, "dir_1", now + 1)
            .await
            .unwrap()
            .expect("soft-deleted");
        assert_eq!(row.status, "expired");
        assert!(row.expires_at.is_some());
        assert!(get_active_for_world(&pool, "ctr_test", "wld_1")
            .await
            .unwrap()
            .is_none());
        assert!(clear_on_scene_change(&pool, "dir_1", now + 2)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lifecycle_anchor_updates_and_round_trips() {
        let (pool, _dir) = fresh_pool().await;
        let now = now_ms();
        set_active(
            &pool,
            &new_directive("dir_1", scope_kind::WORK, "wrk_1", 3, now),
        )
        .await
        .unwrap();

        let updated = update_lifecycle_anchor(&pool, "dir_1", Some("evt_a"), Some(4), now + 1)
            .await
            .unwrap();
        assert!(updated);
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(row.last_focused_event_id.as_deref(), Some("evt_a"));
        assert_eq!(row.last_chapter_no, Some(4));

        // NULL anchors clear the stored state.
        let cleared_anchor = update_lifecycle_anchor(&pool, "dir_1", None, None, now + 2)
            .await
            .unwrap();
        assert!(cleared_anchor);
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert!(row.last_focused_event_id.is_none());
        assert!(row.last_chapter_no.is_none());
    }
}
