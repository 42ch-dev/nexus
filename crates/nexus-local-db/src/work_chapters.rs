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

/// Seed chapter rows inside an existing transaction (V1.37 R-V136P1-02).
///
/// Same logic as [`seed_chapters`] but uses a caller-provided transaction
/// so the seed can be atomic with a subsequent `patch_work_tx` call.
///
/// # Errors
///
/// Returns `LocalDbError` if any insert fails. The caller decides whether
/// to commit or roll back the transaction.
pub async fn seed_chapters_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    work_id: &str,
    work_ref: &str,
    total_chapters: i32,
    now: &str,
) -> Result<(), LocalDbError> {
    for ch in 1..=total_chapters {
        let ch_nn = format!("ch{ch:02}");
        let outline_path = format!("Works/{work_ref}/Outlines/chapters/{ch_nn}-outline.md");
        let slug = ch_nn.clone();
        let body_path = format!("Works/{work_ref}/Stories/{ch_nn}-{slug}.md");

        // SAFETY: INSERT OR IGNORE against work_chapters — runtime query
        // (same rationale as seed_chapters).
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
        .execute(&mut **tx)
        .await?;
    }
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

/// Verify that `stories_dir` does not escape the expected
/// `Works/<work_ref>/` subtree via path traversal. Returns `Ok(())` if the
/// path is safe, `Err(LocalDbError::PathEscape)` otherwise.
#[allow(clippy::missing_errors_doc)]
fn verify_stories_dir_in_workspace(
    stories_dir: &std::path::Path,
    workspace_root: &std::path::Path,
    work_ref: &str,
) -> Result<(), LocalDbError> {
    if !stories_dir.exists() {
        return Ok(());
    }
    let canonical = stories_dir.canonicalize().map_err(|e| LocalDbError::Io {
        path: stories_dir.to_string_lossy().to_string(),
        source: e,
    })?;
    let expected_prefix = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf())
        .join("Works")
        .join(work_ref);
    // Check the parent (Stories → Work root) so symlinks under Stories/ are
    // still allowed but the Stories dir itself must live inside the Work root.
    let stories_parent = canonical.parent().unwrap_or(&canonical);
    let prefix_str = expected_prefix.to_string_lossy();
    let parent_str = stories_parent.to_string_lossy();
    if !parent_str.starts_with(prefix_str.as_ref()) {
        return Err(LocalDbError::PathEscape {
            path: parent_str.to_string(),
            prefix: prefix_str.to_string(),
        });
    }
    Ok(())
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
/// Returns `LocalDbError` if any database or I/O operation fails.
/// Returns `LocalDbError::PathEscape` if `work_ref` would cause the resolved
/// path to escape the `Works/<work_ref>/` subtree.
pub async fn reconcile_from_filesystem(
    pool: &SqlitePool,
    work_id: &str,
    work_ref: &str,
    workspace_root: &std::path::Path,
    now: &str,
) -> Result<ReconcileReport, LocalDbError> {
    let stories_dir = workspace_root.join("Works").join(work_ref).join("Stories");

    // Defense in depth: canonicalize and verify the resolved path stays
    // within the expected Works/<work_ref>/ subtree.  This prevents path
    // traversal even if a caller forgets to validate `work_ref`.
    verify_stories_dir_in_workspace(&stories_dir, workspace_root, work_ref)?;

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

/// Select the next chapter to work on per novel-workflow-profile §4.5.2.
///
/// Selection order:
/// 1. Lowest `not_started` chapter.
/// 2. If none: lowest `outlined` chapter (outline exists, drafting not started).
/// 3. If none: lowest `draft` chapter (resume in-progress draft).
/// 4. If none: `Ok(None)` — the Work is at novel-completion.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn next_chapter(pool: &SqlitePool, work_id: &str) -> Result<Option<i32>, LocalDbError> {
    // Try not_started first (lowest chapter number).
    // SAFETY: SELECT against work_chapters — runtime query.
    let row = sqlx::query(
        "SELECT chapter FROM work_chapters \
         WHERE work_id = ? AND status = 'not_started' \
         ORDER BY chapter ASC LIMIT 1",
    )
    .bind(work_id)
    .fetch_optional(pool)
    .await?;

    if let Some(r) = row {
        let ch: i32 = r.get("chapter");
        return Ok(Some(ch));
    }

    // Try outlined next (outline exists but drafting hasn't started).
    let row = sqlx::query(
        "SELECT chapter FROM work_chapters \
         WHERE work_id = ? AND status = 'outlined' \
         ORDER BY chapter ASC LIMIT 1",
    )
    .bind(work_id)
    .fetch_optional(pool)
    .await?;

    if let Some(r) = row {
        let ch: i32 = r.get("chapter");
        return Ok(Some(ch));
    }

    // Try draft (resume in-progress chapter).
    let row = sqlx::query(
        "SELECT chapter FROM work_chapters \
         WHERE work_id = ? AND status = 'draft' \
         ORDER BY chapter ASC LIMIT 1",
    )
    .bind(work_id)
    .fetch_optional(pool)
    .await?;

    if let Some(r) = row {
        let ch: i32 = r.get("chapter");
        return Ok(Some(ch));
    }

    // No eligible chapter — novel is complete or has no chapters.
    Ok(None)
}

/// Check whether a Work is completed per novel-workflow-profile §6.1.
///
/// Returns `true` if:
/// - `works.status == 'completed'` (early exit), OR
/// - `work_chapters` has ≥1 row AND all rows have `status == 'finalized'`
///   AND the row count equals `works.total_planned_chapters`
///   AND `works.current_chapter >= works.total_planned_chapters`
///   AND `works.intake_status == 'complete'`.
///
/// Returns `false` if:
/// - `total_planned_chapters` is NULL or 0
/// - Any chapter is not in `finalized` status
/// - `current_chapter < total_planned_chapters`
/// - `intake_status` is not `'complete'`
/// - The work or chapters don't exist
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn is_work_completed(pool: &SqlitePool, work_id: &str) -> Result<bool, LocalDbError> {
    // SAFETY: SELECT against works — runtime query.
    let row = sqlx::query(
        "SELECT status, total_planned_chapters, current_chapter, intake_status \
         FROM works WHERE work_id = ?",
    )
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

    // §6.1: intake_status must be 'complete'.
    let intake: String = row.get("intake_status");
    if intake != "complete" {
        return Ok(false);
    }

    // §6.1: current_chapter >= total_planned_chapters.
    let current: Option<i32> = row.get("current_chapter");
    let current = match current {
        Some(c) if c >= 0 => c,
        _ => 0,
    };
    if current < total {
        return Ok(false);
    }

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
    // is_work_completed: true when all chapters finalized, current_chapter >= total, intake complete
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_is_work_completed_all_finalized() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_comp_001").await;

        // V1.38 P0 (T7): completion requires total_planned_chapters, current_chapter,
        // intake_status, and all rows finalized per §6.1.
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query(
            "UPDATE works SET total_planned_chapters = 3, current_chapter = 3, \
             intake_status = 'complete' WHERE work_id = ?",
        )
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
            "3/3 chapters finalized + current_chapter=3 + intake=complete → should be completed"
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

    // =======================================================================
    // V1.38 P0 (T10): Multi-chapter selection and completion hermetic tests
    // =======================================================================

    /// Helper: set up a work with total_planned_chapters and intake_status.
    async fn setup_work_for_selection(pool: &SqlitePool, work_id: &str, total: i32) {
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query(
            "UPDATE works SET total_planned_chapters = ?, current_chapter = 0, \
             intake_status = 'complete' WHERE work_id = ?",
        )
        .bind(total)
        .bind(work_id)
        .execute(pool)
        .await
        .unwrap();
    }

    // T10.1: next_chapter selection order (3-chapter Work with mixed statuses)
    #[tokio::test]
    async fn test_next_chapter_selects_lowest_not_started() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_sel_001").await;
        setup_work_for_selection(&pool, "wrk_sel_001", 3).await;
        seed_chapters(&pool, "wrk_sel_001", "my-novel", 3, "2026-06-08T10:00:00Z")
            .await
            .unwrap();

        // ch1=finalized, ch2=not_started, ch3=not_started
        update_status(
            &pool,
            "wrk_sel_001",
            1,
            "finalized",
            Some(4000),
            "2026-06-08T11:00:00Z",
        )
        .await
        .unwrap();

        let next = next_chapter(&pool, "wrk_sel_001").await.unwrap();
        assert_eq!(next, Some(2), "should select ch2 (lowest not_started)");
    }

    // T10.2: next_chapter draft resume (one draft row, no not_started)
    #[tokio::test]
    async fn test_next_chapter_resumes_draft() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_sel_002").await;
        setup_work_for_selection(&pool, "wrk_sel_002", 3).await;
        seed_chapters(&pool, "wrk_sel_002", "my-novel", 3, "2026-06-08T10:00:00Z")
            .await
            .unwrap();

        // ch1=finalized, ch2=draft, ch3=not_started
        update_status(
            &pool,
            "wrk_sel_002",
            1,
            "finalized",
            Some(4000),
            "2026-06-08T11:00:00Z",
        )
        .await
        .unwrap();
        update_status(
            &pool,
            "wrk_sel_002",
            2,
            "draft",
            None,
            "2026-06-08T12:00:00Z",
        )
        .await
        .unwrap();

        // ch3 is not_started but ch2 is draft and earlier — not_started wins
        let next = next_chapter(&pool, "wrk_sel_002").await.unwrap();
        assert_eq!(
            next,
            Some(3),
            "should select ch3 (not_started) over ch2 (draft)"
        );

        // Now make ch3 finalized too — only ch2 draft remains
        update_status(
            &pool,
            "wrk_sel_002",
            3,
            "finalized",
            Some(4000),
            "2026-06-08T13:00:00Z",
        )
        .await
        .unwrap();

        let next2 = next_chapter(&pool, "wrk_sel_002").await.unwrap();
        assert_eq!(
            next2,
            Some(2),
            "should resume ch2 (draft, no not_started left)"
        );
    }

    // T10.3: Outlined handling (outlined is not skipped)
    #[tokio::test]
    async fn test_next_chapter_outlined_not_skipped() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_sel_003").await;
        setup_work_for_selection(&pool, "wrk_sel_003", 3).await;
        seed_chapters(&pool, "wrk_sel_003", "my-novel", 3, "2026-06-08T10:00:00Z")
            .await
            .unwrap();

        // ch1=outlined, ch2=not_started — outlined is selected over not_started per T5?
        // No: not_started wins per §4.5.2. outlined is tried AFTER not_started.
        update_status(
            &pool,
            "wrk_sel_003",
            1,
            "outlined",
            None,
            "2026-06-08T11:00:00Z",
        )
        .await
        .unwrap();

        let next = next_chapter(&pool, "wrk_sel_003").await.unwrap();
        assert_eq!(
            next,
            Some(2),
            "ch2 (not_started) should be selected over ch1 (outlined)"
        );

        // Now finalize ch2, ch3 — only ch1 (outlined) remains
        update_status(
            &pool,
            "wrk_sel_003",
            2,
            "finalized",
            Some(4000),
            "2026-06-08T12:00:00Z",
        )
        .await
        .unwrap();
        update_status(
            &pool,
            "wrk_sel_003",
            3,
            "finalized",
            Some(4000),
            "2026-06-08T13:00:00Z",
        )
        .await
        .unwrap();

        let next2 = next_chapter(&pool, "wrk_sel_003").await.unwrap();
        assert_eq!(
            next2,
            Some(1),
            "outlined ch1 should be selected when no not_started/draft remain"
        );
    }

    // T10.4: Completion stops when all rows finalized + intake complete + current_chapter OK
    #[tokio::test]
    async fn test_completion_all_finalized() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_comp_010").await;
        setup_work_for_selection(&pool, "wrk_comp_010", 2).await;

        // SAFETY: UPDATE current_chapter — runtime query.
        sqlx::query("UPDATE works SET current_chapter = 2 WHERE work_id = ?")
            .bind("wrk_comp_010")
            .execute(&pool)
            .await
            .unwrap();

        seed_chapters(&pool, "wrk_comp_010", "my-novel", 2, "2026-06-08T10:00:00Z")
            .await
            .unwrap();

        for ch in 1..=2 {
            update_status(
                &pool,
                "wrk_comp_010",
                ch,
                "finalized",
                Some(4000),
                "2026-06-08T12:00:00Z",
            )
            .await
            .unwrap();
        }

        assert!(
            is_work_completed(&pool, "wrk_comp_010").await.unwrap(),
            "all finalized + current_chapter=2 + intake=complete → completed"
        );

        // next_chapter should return None (no eligible chapters)
        let next = next_chapter(&pool, "wrk_comp_010").await.unwrap();
        assert_eq!(next, None, "no next chapter when all finalized");
    }

    // T10.5: Completion does NOT fire when one row is still draft
    #[tokio::test]
    async fn test_completion_blocked_by_draft() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_comp_011").await;
        setup_work_for_selection(&pool, "wrk_comp_011", 3).await;

        // SAFETY: UPDATE current_chapter — runtime query.
        sqlx::query("UPDATE works SET current_chapter = 2 WHERE work_id = ?")
            .bind("wrk_comp_011")
            .execute(&pool)
            .await
            .unwrap();

        seed_chapters(&pool, "wrk_comp_011", "my-novel", 3, "2026-06-08T10:00:00Z")
            .await
            .unwrap();

        // ch1, ch2 finalized; ch3 draft
        for ch in 1..=2 {
            update_status(
                &pool,
                "wrk_comp_011",
                ch,
                "finalized",
                Some(4000),
                "2026-06-08T12:00:00Z",
            )
            .await
            .unwrap();
        }
        update_status(
            &pool,
            "wrk_comp_011",
            3,
            "draft",
            None,
            "2026-06-08T13:00:00Z",
        )
        .await
        .unwrap();

        assert!(
            !is_work_completed(&pool, "wrk_comp_011").await.unwrap(),
            "ch3 still draft → should NOT be completed"
        );
    }

    // T10.6: Completion blocked when intake_status != 'complete'
    #[tokio::test]
    async fn test_completion_blocked_by_intake() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_comp_012").await;

        // Set total + current_chapter but NOT intake_status (stays 'pending')
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query(
            "UPDATE works SET total_planned_chapters = 1, current_chapter = 1 WHERE work_id = ?",
        )
        .bind("wrk_comp_012")
        .execute(&pool)
        .await
        .unwrap();

        seed_chapters(&pool, "wrk_comp_012", "my-novel", 1, "2026-06-08T10:00:00Z")
            .await
            .unwrap();
        update_status(
            &pool,
            "wrk_comp_012",
            1,
            "finalized",
            Some(4000),
            "2026-06-08T12:00:00Z",
        )
        .await
        .unwrap();

        assert!(
            !is_work_completed(&pool, "wrk_comp_012").await.unwrap(),
            "intake_status='pending' → should NOT be completed even if all finalized"
        );
    }

    // T10.7: One-chapter compatibility (V1.36 path still works)
    #[tokio::test]
    async fn test_one_chapter_v136_compatible() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_compat_001").await;

        // V1.36 style: single chapter, intake complete
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query(
            "UPDATE works SET total_planned_chapters = 1, intake_status = 'complete' WHERE work_id = ?",
        )
        .bind("wrk_compat_001")
        .execute(&pool)
        .await
        .unwrap();

        seed_chapters(
            &pool,
            "wrk_compat_001",
            "my-novel",
            1,
            "2026-06-08T10:00:00Z",
        )
        .await
        .unwrap();

        // Before finalizing: next_chapter should return ch1
        let next = next_chapter(&pool, "wrk_compat_001").await.unwrap();
        assert_eq!(next, Some(1), "single-chapter Work should select ch1");

        // Finalize ch1 with current_chapter=1
        update_status(
            &pool,
            "wrk_compat_001",
            1,
            "finalized",
            Some(4000),
            "2026-06-08T12:00:00Z",
        )
        .await
        .unwrap();
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query("UPDATE works SET current_chapter = 1 WHERE work_id = ?")
            .bind("wrk_compat_001")
            .execute(&pool)
            .await
            .unwrap();

        assert!(
            is_work_completed(&pool, "wrk_compat_001").await.unwrap(),
            "single-chapter V1.36 path should complete"
        );

        let next2 = next_chapter(&pool, "wrk_compat_001").await.unwrap();
        assert_eq!(
            next2, None,
            "no next chapter after single-chapter completion"
        );
    }
}
