//! Novel pool entries DAO (DF-61 selection pool).
//!
//! Manages the `novel_pool_entries` table — creator-scoped selection pool
//! tracking active/queued/completed Work references.
//!
//! Spec: novel-work-pool.md §2, local-db-schema.md §4.1.5.
//!
//! # Instrumented mutation paths (V1.46 P4 audit)
//!
//! The following `pub fn` mutate the `novel_pool_entries` table and are
//! instrumented with `tracing::info!`:
//!
//! - [`promote_to_active`]
//! - [`archive_pool_entry`]
//! - [`mark_pool_entry_completed`]
//! - [`mark_pool_entry_completed_for_work`]
//!
//! Read-only functions (`list_pool_entries`, `count_pool_entries`,
//! `get_pool_entry`, `get_pool_entry_by_work`, `get_active_pool_entry`) are
//! intentionally not traced.

use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::error::LocalDbError;

/// Column list for all SELECT queries on `novel_pool_entries`.
pub const POOL_ENTRY_COLUMNS: &str = "\
    entry_id, creator_id, work_id, status, promoted_at, note, title, updated_at";

/// Pool entry record — mirrors DB row.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PoolEntry {
    /// Unique identifier (`npe_` prefix per spec; `pool_` used by P0 for compat).
    pub entry_id: String,
    /// Owning creator.
    pub creator_id: String,
    /// Bound Work ID (nullable until scaffolded).
    pub work_id: Option<String>,
    /// Pool status: `active` | `queued` | `completed` | `archived`.
    pub status: String,
    /// When the entry was last promoted to `active`.
    pub promoted_at: String,
    /// Optional note.
    pub note: Option<String>,
    /// Display title (from Work metadata or user input).
    pub title: String,
    /// Last update timestamp (ISO-8601).
    pub updated_at: String,
}

/// Map a sqlx row to [`PoolEntry`].
#[must_use]
pub fn row_to_pool_entry(r: &sqlx::sqlite::SqliteRow) -> PoolEntry {
    PoolEntry {
        entry_id: r.get("entry_id"),
        creator_id: r.get("creator_id"),
        work_id: r.get("work_id"),
        status: r.get("status"),
        promoted_at: r.get("promoted_at"),
        note: r.get("note"),
        title: r.get("title"),
        updated_at: r.get("updated_at"),
    }
}

/// List pool entries for a creator, optionally filtered by status.
///
/// `limit` defaults to 200 when `None`; capped at 1000.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_pool_entries(
    pool: &SqlitePool,
    creator_id: &str,
    status_filter: Option<&str>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<PoolEntry>, LocalDbError> {
    let effective_limit = limit.unwrap_or(200).min(1000);
    let effective_offset = offset.unwrap_or(0);

    let sql = if status_filter.is_some() {
        format!(
            "SELECT {POOL_ENTRY_COLUMNS} FROM novel_pool_entries \
             WHERE creator_id = ? AND status = ? \
             ORDER BY updated_at DESC \
             LIMIT ? OFFSET ?"
        )
    } else {
        format!(
            "SELECT {POOL_ENTRY_COLUMNS} FROM novel_pool_entries \
             WHERE creator_id = ? \
             ORDER BY updated_at DESC \
             LIMIT ? OFFSET ?"
        )
    };

    let mut query = sqlx::query(&sql).bind(creator_id);
    if let Some(s) = status_filter {
        query = query.bind(s);
    }
    query = query.bind(effective_limit).bind(effective_offset);

    let rows = query.fetch_all(pool).await?;
    Ok(rows.iter().map(row_to_pool_entry).collect())
}

/// Count pool entries for a creator, optionally filtered by status.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn count_pool_entries(
    pool: &SqlitePool,
    creator_id: &str,
    status_filter: Option<&str>,
) -> Result<u32, LocalDbError> {
    let sql = if status_filter.is_some() {
        "SELECT COUNT(*) FROM novel_pool_entries WHERE creator_id = ? AND status = ?"
    } else {
        "SELECT COUNT(*) FROM novel_pool_entries WHERE creator_id = ?"
    };

    let mut query = sqlx::query(sql).bind(creator_id);
    if let Some(s) = status_filter {
        query = query.bind(s);
    }

    let count: i64 = query.fetch_one(pool).await?.get(0);
    Ok(u32::try_from(count).unwrap_or(0))
}

/// Promote a pool entry to `active`, demoting any prior `active` entry to `queued`.
///
/// Transactional: the demote + promote happen in a single transaction so the
/// one-active-per-creator invariant is never violated.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or the entry is not found.
pub async fn promote_to_active(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<PoolEntry, LocalDbError> {
    tracing::info!(
        operation = "pool_promote_to_active",
        creator_id = %creator_id,
        work_id = %work_id,
        "pool mutation"
    );
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx: Transaction<'_, Sqlite> = pool.begin().await?;

    // Step 1: Demote prior active → queued
    // SAFETY: dynamic SQL — compile-time macro not applicable.
    sqlx::query(
        "UPDATE novel_pool_entries SET status = 'queued', updated_at = ? \
         WHERE creator_id = ? AND status = 'active'",
    )
    .bind(&now)
    .bind(creator_id)
    .execute(&mut *tx)
    .await?;

    // Step 2: Upsert target → active
    // SAFETY: dynamic SQL for upsert — compile-time macro not applicable.
    let entry_id = format!("npe_{}", uuid::Uuid::new_v4());

    // Get the title from the works table for the work_id if available
    let work_title: Option<String> =
        sqlx::query_scalar("SELECT title FROM works WHERE work_id = ? AND creator_id = ?")
            .bind(work_id)
            .bind(creator_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();

    let title = work_title.unwrap_or_default();

    sqlx::query(
        "INSERT INTO novel_pool_entries (entry_id, creator_id, work_id, status, promoted_at, note, title, updated_at) \
         VALUES (?, ?, ?, 'active', ?, NULL, ?, ?) \
         ON CONFLICT(creator_id, work_id) DO UPDATE SET \
           status = 'active', promoted_at = excluded.promoted_at, note = NULL, \
           title = excluded.title, updated_at = excluded.updated_at",
    )
    .bind(&entry_id)
    .bind(creator_id)
    .bind(work_id)
    .bind(&now)
    .bind(&title)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Fetch the upserted entry
    get_pool_entry_by_work(pool, creator_id, work_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey {
            key: format!("novel_pool_entries/{work_id}"),
        })
}

/// Archive a pool entry (set status to `archived`).
///
/// Restricted to the owning `creator_id` — rows belonging to other
/// creators are silently unaffected (0 rows updated → `MissingVersionKey`).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or the entry is not found
/// (or does not belong to the given `creator_id`).
pub async fn archive_pool_entry(
    pool: &SqlitePool,
    entry_id: &str,
    creator_id: &str,
) -> Result<PoolEntry, LocalDbError> {
    tracing::info!(
        operation = "pool_archive",
        entry_id = %entry_id,
        creator_id = %creator_id,
        "pool mutation"
    );
    let now = chrono::Utc::now().to_rfc3339();

    // SAFETY: dynamic SQL — compile-time macro not applicable.
    let result = sqlx::query(
        "UPDATE novel_pool_entries SET status = 'archived', updated_at = ? \
         WHERE entry_id = ? AND creator_id = ?",
    )
    .bind(&now)
    .bind(entry_id)
    .bind(creator_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(LocalDbError::MissingVersionKey {
            key: format!("novel_pool_entries/{entry_id} (creator {creator_id})"),
        });
    }

    get_pool_entry(pool, entry_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey {
            key: format!("novel_pool_entries/{entry_id}"),
        })
}

/// Mark a pool entry as `completed`.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or the entry is not found.
pub async fn mark_pool_entry_completed(
    pool: &SqlitePool,
    entry_id: &str,
) -> Result<(), LocalDbError> {
    tracing::info!(
        operation = "pool_mark_completed",
        entry_id = %entry_id,
        "pool mutation"
    );
    let now = chrono::Utc::now().to_rfc3339();

    // SAFETY: dynamic SQL — compile-time macro not applicable.
    sqlx::query(
        "UPDATE novel_pool_entries SET status = 'completed', updated_at = ? WHERE entry_id = ?",
    )
    .bind(&now)
    .bind(entry_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark a pool entry as `completed` by `creator_id` + `work_id` (for completion hook).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn mark_pool_entry_completed_for_work(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<(), LocalDbError> {
    tracing::info!(
        operation = "pool_mark_completed_for_work",
        creator_id = %creator_id,
        work_id = %work_id,
        "pool mutation"
    );
    let now = chrono::Utc::now().to_rfc3339();

    // SAFETY: dynamic SQL — compile-time macro not applicable.
    sqlx::query(
        "UPDATE novel_pool_entries SET status = 'completed', updated_at = ? \
         WHERE creator_id = ? AND work_id = ? AND status != 'completed'",
    )
    .bind(&now)
    .bind(creator_id)
    .bind(work_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get a single pool entry by `entry_id`.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_pool_entry(
    pool: &SqlitePool,
    entry_id: &str,
) -> Result<Option<PoolEntry>, LocalDbError> {
    let row = sqlx::query(&format!(
        "SELECT {POOL_ENTRY_COLUMNS} FROM novel_pool_entries WHERE entry_id = ?"
    ))
    .bind(entry_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.as_ref().map(row_to_pool_entry))
}

/// Get a pool entry by `creator_id` + `work_id`.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_pool_entry_by_work(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<Option<PoolEntry>, LocalDbError> {
    let row = sqlx::query(&format!(
        "SELECT {POOL_ENTRY_COLUMNS} FROM novel_pool_entries WHERE creator_id = ? AND work_id = ?"
    ))
    .bind(creator_id)
    .bind(work_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.as_ref().map(row_to_pool_entry))
}

/// Get the active pool entry for a creator.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_active_pool_entry(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<Option<PoolEntry>, LocalDbError> {
    let row = sqlx::query(&format!(
        "SELECT {POOL_ENTRY_COLUMNS} FROM novel_pool_entries WHERE creator_id = ? AND status = 'active'"
    ))
    .bind(creator_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.as_ref().map(row_to_pool_entry))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::open_pool(&db_path).await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    fn seed_work(
        pool: &SqlitePool,
        work_id: &str,
        creator_id: &str,
    ) -> tokio::task::JoinHandle<()> {
        let pool = pool.clone();
        let work_id = work_id.to_string();
        let creator_id = creator_id.to_string();
        tokio::spawn(async move {
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT OR IGNORE INTO works (work_id, creator_id, workspace_slug, status, title, \
                 long_term_goal, initial_idea, intake_status, inspiration_log, primary_preset_id, \
                 schedule_ids, created_at, updated_at, current_stage, stage_status, current_chapter, \
                 auto_chain_enabled, auto_chain_interrupted, auto_review_master_on_timeout) \
                 VALUES (?, ?, 'default', 'active', ?, 'goal', 'idea', 'pending', '[]', \
                 'novel-writing', '[]', ?, ?, 'intake', 'pending', 0, 1, 0, 0)",
            )
            .bind(&work_id)
            .bind(&creator_id)
            .bind(format!("Work {work_id}"))
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        })
    }

    #[tokio::test]
    async fn test_list_pool_entries_returns_all_statuses() {
        let (pool, _dir) = fresh_pool().await;
        seed_work(&pool, "wrk_001", "ctr_test").await.unwrap();
        seed_work(&pool, "wrk_002", "ctr_test").await.unwrap();

        let now = chrono::Utc::now().to_rfc3339();
        // Insert entries directly
        for (eid, wid, status) in [
            ("npe_1", "wrk_001", "active"),
            ("npe_2", "wrk_002", "queued"),
        ] {
            sqlx::query(
                "INSERT INTO novel_pool_entries (entry_id, creator_id, work_id, status, promoted_at, note, title, updated_at) \
                 VALUES (?, 'ctr_test', ?, ?, ?, NULL, ?, ?)",
            )
            .bind(eid)
            .bind(wid)
            .bind(status)
            .bind(&now)
            .bind(format!("Title {eid}"))
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        }

        let entries = list_pool_entries(&pool, "ctr_test", None, None, None)
            .await
            .unwrap();
        assert_eq!(entries.len(), 2);
        // Active comes first
        assert_eq!(entries[0].status, "active");
        assert_eq!(entries[1].status, "queued");
    }

    #[tokio::test]
    async fn test_promote_demotes_prior_active() {
        let (pool, _dir) = fresh_pool().await;
        seed_work(&pool, "wrk_001", "ctr_test").await.unwrap();
        seed_work(&pool, "wrk_002", "ctr_test").await.unwrap();

        // Promote first work
        let entry1 = promote_to_active(&pool, "ctr_test", "wrk_001")
            .await
            .unwrap();
        assert_eq!(entry1.status, "active");

        // Promote second work — should demote first
        let entry2 = promote_to_active(&pool, "ctr_test", "wrk_002")
            .await
            .unwrap();
        assert_eq!(entry2.status, "active");

        let entries = list_pool_entries(&pool, "ctr_test", None, None, None)
            .await
            .unwrap();
        let active: Vec<_> = entries.iter().filter(|e| e.status == "active").collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].work_id, Some("wrk_002".to_string()));
    }

    #[tokio::test]
    async fn test_archive_marks_archived() {
        let (pool, _dir) = fresh_pool().await;
        seed_work(&pool, "wrk_001", "ctr_test").await.unwrap();

        let entry = promote_to_active(&pool, "ctr_test", "wrk_001")
            .await
            .unwrap();
        let archived = archive_pool_entry(&pool, &entry.entry_id, "ctr_test")
            .await
            .unwrap();
        assert_eq!(archived.status, "archived");
    }

    #[tokio::test]
    async fn test_mark_pool_entry_completed_for_work() {
        let (pool, _dir) = fresh_pool().await;
        seed_work(&pool, "wrk_001", "ctr_test").await.unwrap();

        promote_to_active(&pool, "ctr_test", "wrk_001")
            .await
            .unwrap();
        mark_pool_entry_completed_for_work(&pool, "ctr_test", "wrk_001")
            .await
            .unwrap();

        let entry = get_pool_entry_by_work(&pool, "ctr_test", "wrk_001")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(entry.status, "completed");
    }
}
