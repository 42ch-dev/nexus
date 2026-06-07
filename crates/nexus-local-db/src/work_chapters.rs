//! Work chapters CRUD operations (V1.36 novel-workflow-profile §4.1).
//!
//! Manages the `work_chapters` table — per-chapter state SSOT for
//! `work_profile: novel` Works. Seeded by the `novel-project-init` preset
//! scaffold capability.

use sqlx::{Row, SqlitePool};

use crate::error::LocalDbError;

/// Work chapter record — mirrors DB row.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkChapterRecord {
    /// Owning Work ID.
    pub work_id: String,
    /// Chapter number (`1..total_planned_chapters`).
    pub chapter: i32,
    /// Volume number (nullable; V1.36 single-volume leaves NULL).
    pub volume: Option<i32>,
    /// Filename slug (e.g. "the-third-layer").
    pub slug: Option<String>,
    /// Planned word count (default 4000).
    pub planned_word_count: i32,
    /// Actual word count (set on transition to finalized).
    pub actual_word_count: Option<i32>,
    /// Chapter status.
    pub status: String,
    /// Relative path to outline file.
    pub outline_path: Option<String>,
    /// Relative path to chapter body file.
    pub body_path: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last update timestamp (ISO 8601).
    pub updated_at: String,
}

/// Seed N chapter rows for a Work (idempotent; preserves existing).
///
/// For `chapter IN 1..total`, inserts one row per chapter. On PK conflict
/// `(work_id, chapter)`, existing rows are **not** overwritten (idempotency
/// per novel-workflow-profile.md §5.4.5).
///
/// All inserts run in a single transaction; rolls back on any failure.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn seed_chapters(
    pool: &SqlitePool,
    work_id: &str,
    work_ref: &str,
    total_chapters: i32,
    now: &str,
) -> Result<(), LocalDbError> {
    let mut tx = pool.begin().await?;

    for ch in 1..=total_chapters {
        let ch_nn = format!("ch{ch:02}");
        let outline_path = format!("Works/{work_ref}/Outlines/chapters/{ch_nn}-outline.md");
        let slug = ch_nn.clone();
        let body_path = format!("Works/{work_ref}/Stories/{ch_nn}-{slug}.md");

        // SAFETY: INSERT OR IGNORE against work_chapters — runtime query because
        // the table was added in the same migration cycle and sqlx prepare hasn't
        // run yet. Uses INSERT OR IGNORE for idempotent seeding (PK conflict on
        // (work_id, chapter) preserves existing rows).
        sqlx::query(
            "INSERT OR IGNORE INTO work_chapters
             (work_id, chapter, volume, slug, planned_word_count, actual_word_count,
              status, outline_path, body_path, created_at, updated_at)
             VALUES (?, ?, NULL, ?, 4000, NULL, 'not_started', ?, ?, ?, ?)",
        )
        .bind(work_id)
        .bind(ch)
        .bind(&slug)
        .bind(&outline_path)
        .bind(&body_path)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// List all chapter rows for a Work, ordered by chapter number.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_chapters(
    pool: &SqlitePool,
    work_id: &str,
) -> Result<Vec<WorkChapterRecord>, LocalDbError> {
    // SAFETY: SELECT against work_chapters — runtime query.
    let rows = sqlx::query(
        "SELECT work_id, chapter, volume, slug, planned_word_count, actual_word_count,
                status, outline_path, body_path, created_at, updated_at
         FROM work_chapters WHERE work_id = ?
         ORDER BY chapter",
    )
    .bind(work_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| WorkChapterRecord {
            work_id: r.get("work_id"),
            chapter: r.get("chapter"),
            volume: r.get("volume"),
            slug: r.get("slug"),
            planned_word_count: r.get("planned_word_count"),
            actual_word_count: r.get("actual_word_count"),
            status: r.get("status"),
            outline_path: r.get("outline_path"),
            body_path: r.get("body_path"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

/// Count chapter rows for a Work.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn count_chapters(pool: &SqlitePool, work_id: &str) -> Result<u32, LocalDbError> {
    // SAFETY: SELECT COUNT against work_chapters — runtime query.
    let row = sqlx::query("SELECT COUNT(*) AS cnt FROM work_chapters WHERE work_id = ?")
        .bind(work_id)
        .fetch_one(pool)
        .await?;
    let count: i64 = row.get("cnt");
    Ok(u32::try_from(count).unwrap_or(0))
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

    async fn insert_test_work(pool: &SqlitePool, work_id: &str) {
        // SAFETY: INSERT against works — runtime query.
        sqlx::query(
            "INSERT INTO works (work_id, creator_id, workspace_slug, status, title,
             long_term_goal, initial_idea, intake_status, inspiration_log, primary_preset_id,
             schedule_ids, created_at, updated_at)
             VALUES (?, 'ctr_test', 'default', 'draft', 'Test', 'Goal', 'Idea', 'pending',
             '[]', 'novel-writing', '[]', ?, ?)",
        )
        .bind(work_id)
        .bind("2026-06-07T10:00:00Z")
        .bind("2026-06-07T10:00:00Z")
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_seed_chapters_creates_rows() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_seed_001").await;

        seed_chapters(&pool, "wrk_seed_001", "my-novel", 5, "2026-06-07T10:00:00Z")
            .await
            .unwrap();

        let chapters = list_chapters(&pool, "wrk_seed_001").await.unwrap();
        assert_eq!(chapters.len(), 5);
        assert_eq!(chapters[0].chapter, 1);
        assert_eq!(chapters[4].chapter, 5);
        assert_eq!(chapters[0].status, "not_started");
        assert!(chapters[0].outline_path.is_some());
        assert!(chapters[0].body_path.is_some());
    }

    #[tokio::test]
    async fn test_seed_chapters_idempotent() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_seed_002").await;

        seed_chapters(&pool, "wrk_seed_002", "my-novel", 3, "2026-06-07T10:00:00Z")
            .await
            .unwrap();

        // Re-seed — should not duplicate
        seed_chapters(&pool, "wrk_seed_002", "my-novel", 3, "2026-06-07T11:00:00Z")
            .await
            .unwrap();

        let count = count_chapters(&pool, "wrk_seed_002").await.unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_seed_chapters_paths_formatted_correctly() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_seed_003").await;

        seed_chapters(
            &pool,
            "wrk_seed_003",
            "cozy-mystery",
            2,
            "2026-06-07T10:00:00Z",
        )
        .await
        .unwrap();

        let chapters = list_chapters(&pool, "wrk_seed_003").await.unwrap();
        assert_eq!(
            chapters[0].outline_path.as_deref(),
            Some("Works/cozy-mystery/Outlines/chapters/ch01-outline.md")
        );
        assert_eq!(
            chapters[0].body_path.as_deref(),
            Some("Works/cozy-mystery/Stories/ch01-ch01.md")
        );
        assert_eq!(
            chapters[1].outline_path.as_deref(),
            Some("Works/cozy-mystery/Outlines/chapters/ch02-outline.md")
        );
    }

    #[tokio::test]
    async fn test_count_chapters_empty() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_seed_004").await;

        let count = count_chapters(&pool, "wrk_seed_004").await.unwrap();
        assert_eq!(count, 0);
    }
}
