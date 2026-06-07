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

/// Parameters for inserting a single chapter row.
#[derive(Debug)]
pub struct InsertChapterParams<'a> {
    /// Owning Work ID.
    pub work_id: &'a str,
    /// Chapter number.
    pub chapter: i32,
    /// Volume number (nullable; V1.36 single-volume leaves NULL).
    pub volume: Option<i32>,
    /// Filename slug.
    pub slug: Option<&'a str>,
    /// Planned word count (default 4000).
    pub planned_word_count: i32,
    /// Relative path to outline file.
    pub outline_path: Option<&'a str>,
    /// Relative path to chapter body file.
    pub body_path: Option<&'a str>,
    /// ISO 8601 timestamp.
    pub now: &'a str,
}

/// Insert a single chapter row.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails (e.g. PK conflict).
pub async fn insert_chapter(
    pool: &SqlitePool,
    params: &InsertChapterParams<'_>,
) -> Result<(), LocalDbError> {
    // SAFETY: INSERT against work_chapters — runtime query.
    sqlx::query(
        "INSERT INTO work_chapters
         (work_id, chapter, volume, slug, planned_word_count, actual_word_count,
          status, outline_path, body_path, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, NULL, 'not_started', ?, ?, ?, ?)",
    )
    .bind(params.work_id)
    .bind(params.chapter)
    .bind(params.volume)
    .bind(params.slug)
    .bind(params.planned_word_count)
    .bind(params.outline_path)
    .bind(params.body_path)
    .bind(params.now)
    .bind(params.now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get a single chapter row by `work_id` and chapter number.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_chapter(
    pool: &SqlitePool,
    work_id: &str,
    chapter: i32,
) -> Result<Option<WorkChapterRecord>, LocalDbError> {
    // SAFETY: SELECT against work_chapters — runtime query.
    let row = sqlx::query(
        "SELECT work_id, chapter, volume, slug, planned_word_count, actual_word_count,
                status, outline_path, body_path, created_at, updated_at
         FROM work_chapters WHERE work_id = ? AND chapter = ?",
    )
    .bind(work_id)
    .bind(chapter)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| WorkChapterRecord {
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
    }))
}

/// Update a chapter's status and optionally its actual word count.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn update_status(
    pool: &SqlitePool,
    work_id: &str,
    chapter: i32,
    new_status: &str,
    actual_word_count: Option<u32>,
    now: &str,
) -> Result<(), LocalDbError> {
    // SAFETY: UPDATE against work_chapters — runtime query.
    sqlx::query(
        "UPDATE work_chapters SET status = ?, actual_word_count = ?, updated_at = ?
         WHERE work_id = ? AND chapter = ?",
    )
    .bind(new_status)
    .bind(actual_word_count.map(|v| i32::try_from(v).unwrap_or(i32::MAX)))
    .bind(now)
    .bind(work_id)
    .bind(chapter)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update a chapter's `outline_path` and `body_path`.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn update_paths(
    pool: &SqlitePool,
    work_id: &str,
    chapter: i32,
    outline_path: Option<&str>,
    body_path: Option<&str>,
    now: &str,
) -> Result<(), LocalDbError> {
    // SAFETY: UPDATE against work_chapters — runtime query.
    sqlx::query(
        "UPDATE work_chapters SET outline_path = ?, body_path = ?, updated_at = ?
         WHERE work_id = ? AND chapter = ?",
    )
    .bind(outline_path)
    .bind(body_path)
    .bind(now)
    .bind(work_id)
    .bind(chapter)
    .execute(pool)
    .await?;
    Ok(())
}

/// Report from reconciling `work_chapters` with the filesystem.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReconcileReport {
    /// Number of new chapter rows created.
    pub created: u32,
    /// Number of existing chapter rows updated.
    pub updated: u32,
    /// Number of chapter rows preserved unchanged.
    pub preserved: u32,
}

/// Reconcile `work_chapters` rows from the filesystem.
///
/// Walks `Works/<work_ref>/Stories/`, parses each `.md` file's frontmatter
/// for chapter number and status, and rebuilds/updates `work_chapters` rows.
/// For new files, inserts a row. For existing rows, updates status if
/// frontmatter has changed. Preserves rows that match.
///
/// # Errors
///
/// Returns `LocalDbError` if any database operation fails.
pub async fn reconcile_from_filesystem(
    pool: &SqlitePool,
    work_id: &str,
    work_ref: &str,
    workspace_root: &std::path::Path,
    now: &str,
) -> Result<ReconcileReport, LocalDbError> {
    let stories_dir = workspace_root.join("Works").join(work_ref).join("Stories");

    if !stories_dir.is_dir() {
        return Ok(ReconcileReport {
            created: 0,
            updated: 0,
            preserved: 0,
        });
    }

    let mut created: u32 = 0;
    let mut updated: u32 = 0;
    let mut preserved: u32 = 0;

    let entries = std::fs::read_dir(&stories_dir).map_err(|e| LocalDbError::Io {
        path: stories_dir.to_string_lossy().to_string(),
        source: e,
    })?;

    for entry in entries.flatten() {
        let path: std::path::PathBuf = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let fname = path
            .file_name()
            .map(|n: &std::ffi::OsStr| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if fname.starts_with('.') || fname == "README.md" {
            continue;
        }

        // Parse chapter number from filename
        let Some(ch_num) = parse_chapter_from_filename(&fname) else {
            continue;
        };

        // Parse frontmatter for status and word_count
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let fm = parse_frontmatter(&content);
        let fm_status = fm.get("status").cloned();
        let fm_word_count: Option<i32> = fm.get("word_count").and_then(|v| v.parse().ok());

        // Check if row exists
        let existing = get_chapter(pool, work_id, ch_num).await?;

        match existing {
            None => {
                // New chapter — insert
                let body_path = format!("Works/{work_ref}/Stories/{fname}");
                insert_chapter(
                    pool,
                    &InsertChapterParams {
                        work_id,
                        chapter: ch_num,
                        volume: None,
                        slug: None,
                        planned_word_count: 4000,
                        outline_path: None,
                        body_path: Some(&body_path),
                        now,
                    },
                )
                .await?;
                created += 1;
            }
            Some(row) => {
                // Check if status changed
                let db_status = row.status.clone();
                let needs_update = fm_status.as_ref().is_some_and(|s| s != &db_status)
                    || fm_word_count.is_some_and(|wc| row.actual_word_count != Some(wc));

                if needs_update {
                    update_status(
                        pool,
                        work_id,
                        ch_num,
                        fm_status.as_deref().unwrap_or(&db_status),
                        fm_word_count.map(|v| u32::try_from(v).unwrap_or(0)),
                        now,
                    )
                    .await?;
                    updated += 1;
                } else {
                    preserved += 1;
                }
            }
        }
    }

    Ok(ReconcileReport {
        created,
        updated,
        preserved,
    })
}

/// Check whether a Work is completed per novel-workflow-profile §6.1.
///
/// Returns `true` if:
/// - `works.status == 'completed'` (early exit), OR
/// - `work_chapters` has ≥1 row AND all rows have `status == 'finalized'`
///   AND the row count equals `works.total_planned_chapters`.
///
/// Returns `false` if:
/// - `total_planned_chapters` is NULL or 0
/// - Any chapter is not in `finalized` status
/// - The work or chapters don't exist
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn is_work_completed(pool: &SqlitePool, work_id: &str) -> Result<bool, LocalDbError> {
    // SAFETY: SELECT against works — runtime query.
    let row = sqlx::query("SELECT status, total_planned_chapters FROM works WHERE work_id = ?")
        .bind(work_id)
        .fetch_optional(pool)
        .await?;

    let Some(row) = row else {
        return Ok(false);
    };

    let status: String = row.get("status");
    if status == "completed" {
        return Ok(true);
    }

    let total: Option<i32> = row.get("total_planned_chapters");
    let total = match total {
        Some(t) if t > 0 => t,
        _ => return Ok(false),
    };

    let chapters = list_chapters(pool, work_id).await?;
    if chapters.len() != usize::try_from(total).unwrap_or(0) {
        return Ok(false);
    }

    Ok(chapters.iter().all(|c| c.status == "finalized"))
}

/// Parse chapter number from a filename like `ch01-introduction.md`.
fn parse_chapter_from_filename(filename: &str) -> Option<i32> {
    let name = filename.strip_suffix(".md")?;
    let digits = name.strip_prefix("ch")?;
    let num_str = digits.split('-').next()?;
    num_str.parse().ok()
}

/// Minimal YAML frontmatter parser (key: value pairs only).
/// Returns a `HashMap` of key-value pairs from between `---` delimiters.
fn parse_frontmatter(content: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut in_frontmatter = false;
    let mut found_first = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if !found_first {
                found_first = true;
                in_frontmatter = true;
                continue;
            }
            // Second --- ends frontmatter
            break;
        }
        if in_frontmatter {
            if let Some((key, value)) = trimmed.split_once(':') {
                let key = key.trim().to_string();
                let value = value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                map.insert(key, value);
            }
        }
    }

    map
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

    // -----------------------------------------------------------------------
    // CRUD: insert_chapter + get_chapter round-trip
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_insert_and_get_chapter() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_crud_001").await;

        insert_chapter(
            &pool,
            &InsertChapterParams {
                work_id: "wrk_crud_001",
                chapter: 1,
                volume: None,
                slug: Some("introduction"),
                planned_word_count: 5000,
                outline_path: Some("Works/my-novel/Outlines/chapters/ch01-outline.md"),
                body_path: Some("Works/my-novel/Stories/ch01-introduction.md"),
                now: "2026-06-07T10:00:00Z",
            },
        )
        .await
        .unwrap();

        let row = get_chapter(&pool, "wrk_crud_001", 1)
            .await
            .unwrap()
            .expect("chapter should exist");
        assert_eq!(row.chapter, 1);
        assert_eq!(row.slug.as_deref(), Some("introduction"));
        assert_eq!(row.planned_word_count, 5000);
        assert_eq!(row.status, "not_started");
        assert!(row.actual_word_count.is_none());
    }

    // -----------------------------------------------------------------------
    // CRUD: update_status flips state
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_update_status() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_crud_002").await;

        seed_chapters(&pool, "wrk_crud_002", "my-novel", 2, "2026-06-07T10:00:00Z")
            .await
            .unwrap();

        update_status(
            &pool,
            "wrk_crud_002",
            1,
            "draft",
            None,
            "2026-06-07T11:00:00Z",
        )
        .await
        .unwrap();

        let row = get_chapter(&pool, "wrk_crud_002", 1)
            .await
            .unwrap()
            .expect("chapter 1");
        assert_eq!(row.status, "draft");
        assert!(row.actual_word_count.is_none());

        // Update to finalized with word count
        update_status(
            &pool,
            "wrk_crud_002",
            1,
            "finalized",
            Some(4200),
            "2026-06-07T12:00:00Z",
        )
        .await
        .unwrap();

        let row = get_chapter(&pool, "wrk_crud_002", 1)
            .await
            .unwrap()
            .expect("chapter 1");
        assert_eq!(row.status, "finalized");
        assert_eq!(row.actual_word_count, Some(4200));
    }

    // -----------------------------------------------------------------------
    // is_work_completed: true when all chapters finalized
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_is_work_completed_all_finalized() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_comp_001").await;

        // Set total_planned_chapters on the work
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query("UPDATE works SET total_planned_chapters = 3 WHERE work_id = ?")
            .bind("wrk_comp_001")
            .execute(&pool)
            .await
            .unwrap();

        seed_chapters(&pool, "wrk_comp_001", "my-novel", 3, "2026-06-07T10:00:00Z")
            .await
            .unwrap();

        // Finalize all 3 chapters
        for ch in 1..=3 {
            update_status(
                &pool,
                "wrk_comp_001",
                ch,
                "finalized",
                Some(4000),
                "2026-06-07T12:00:00Z",
            )
            .await
            .unwrap();
        }

        assert!(
            is_work_completed(&pool, "wrk_comp_001").await.unwrap(),
            "3/3 chapters finalized → should be completed"
        );
    }

    // -----------------------------------------------------------------------
    // is_work_completed: false when 1 chapter still draft
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_is_work_completed_one_draft() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_comp_002").await;

        // SAFETY: UPDATE against works — runtime query.
        sqlx::query("UPDATE works SET total_planned_chapters = 3 WHERE work_id = ?")
            .bind("wrk_comp_002")
            .execute(&pool)
            .await
            .unwrap();

        seed_chapters(&pool, "wrk_comp_002", "my-novel", 3, "2026-06-07T10:00:00Z")
            .await
            .unwrap();

        // Finalize ch1, ch2; leave ch3 as draft
        for ch in 1..=2 {
            update_status(
                &pool,
                "wrk_comp_002",
                ch,
                "finalized",
                Some(4000),
                "2026-06-07T12:00:00Z",
            )
            .await
            .unwrap();
        }
        update_status(
            &pool,
            "wrk_comp_002",
            3,
            "draft",
            None,
            "2026-06-07T11:00:00Z",
        )
        .await
        .unwrap();

        assert!(
            !is_work_completed(&pool, "wrk_comp_002").await.unwrap(),
            "2/3 finalized, 1 draft → should NOT be completed"
        );
    }

    // -----------------------------------------------------------------------
    // Reconcile: 3-chapter workspace
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_reconcile_from_filesystem() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_recon_001").await;

        // Create 3-chapter workspace
        let stories_dir = dir.path().join("Works").join("my-novel").join("Stories");
        std::fs::create_dir_all(&stories_dir).unwrap();

        std::fs::write(
            stories_dir.join("ch01-intro.md"),
            "---\ntitle: Intro\nchapter: 1\nstatus: draft\n---\nContent 1",
        )
        .unwrap();
        std::fs::write(
            stories_dir.join("ch02-body.md"),
            "---\ntitle: Body\nchapter: 2\nstatus: not_started\n---\nContent 2",
        )
        .unwrap();
        std::fs::write(
            stories_dir.join("ch03-end.md"),
            "---\ntitle: End\nchapter: 3\nstatus: not_started\n---\nContent 3",
        )
        .unwrap();

        let report = reconcile_from_filesystem(
            &pool,
            "wrk_recon_001",
            "my-novel",
            dir.path(),
            "2026-06-07T10:00:00Z",
        )
        .await
        .unwrap();

        assert_eq!(report.created, 3);
        assert_eq!(report.updated, 0);
        assert_eq!(report.preserved, 0);

        // Verify rows
        let chapters = list_chapters(&pool, "wrk_recon_001").await.unwrap();
        assert_eq!(chapters.len(), 3);
        assert_eq!(chapters[0].chapter, 1);
        assert_eq!(chapters[1].chapter, 2);
        assert_eq!(chapters[2].chapter, 3);
    }

    // -----------------------------------------------------------------------
    // Reconcile: update existing + idempotent
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_reconcile_update_and_idempotent() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_recon_002").await;

        // Pre-seed 3 chapters
        seed_chapters(
            &pool,
            "wrk_recon_002",
            "my-novel",
            3,
            "2026-06-07T10:00:00Z",
        )
        .await
        .unwrap();

        // Create workspace with 1 changed frontmatter
        let stories_dir = dir.path().join("Works").join("my-novel").join("Stories");
        std::fs::create_dir_all(&stories_dir).unwrap();

        std::fs::write(
            stories_dir.join("ch01-intro.md"),
            "---\ntitle: Intro\nchapter: 1\nstatus: draft\nword_count: 3200\n---\nContent",
        )
        .unwrap();
        std::fs::write(
            stories_dir.join("ch02-body.md"),
            "---\ntitle: Body\nchapter: 2\nstatus: not_started\n---\nContent",
        )
        .unwrap();
        std::fs::write(
            stories_dir.join("ch03-end.md"),
            "---\ntitle: End\nchapter: 3\nstatus: not_started\n---\nContent",
        )
        .unwrap();

        let report = reconcile_from_filesystem(
            &pool,
            "wrk_recon_002",
            "my-novel",
            dir.path(),
            "2026-06-07T11:00:00Z",
        )
        .await
        .unwrap();

        // ch01 changed status: not_started → draft + word_count set
        assert_eq!(report.updated, 1);
        assert_eq!(report.preserved, 2);
        assert_eq!(report.created, 0);

        // Verify ch01 updated
        let ch1 = get_chapter(&pool, "wrk_recon_002", 1)
            .await
            .unwrap()
            .expect("ch1");
        assert_eq!(ch1.status, "draft");
        assert_eq!(ch1.actual_word_count, Some(3200));

        // Re-run reconcile — should be fully idempotent
        let report2 = reconcile_from_filesystem(
            &pool,
            "wrk_recon_002",
            "my-novel",
            dir.path(),
            "2026-06-07T12:00:00Z",
        )
        .await
        .unwrap();
        assert_eq!(report2.updated, 0);
        assert_eq!(report2.preserved, 3);
    }
}
