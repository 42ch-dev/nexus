//! Work chapters CRUD operations (V1.36 novel-workflow-profile §4.1, V1.42 §4.5.4).
//!
//! Manages the `work_chapters` table — per-chapter state SSOT for
//! `work_profile: novel` Works. Seeded by the `novel-project-init` preset
//! scaffold capability.
//!
//! V1.42 P1: PK migrated from `(work_id, chapter)` to `(work_id, volume, chapter)`.
//! Existing rows backfilled with `volume = 1`. Single-volume Works (V1.41 and earlier)
//! behave identically — all queries default to volume 1 when not specified.

use sqlx::{Row, SqlitePool};

use crate::error::LocalDbError;

/// Work chapter record — mirrors DB row.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkChapterRecord {
    /// Owning Work ID.
    pub work_id: String,
    /// Chapter number (`1..total_planned_chapters` within the volume).
    pub chapter: i32,
    /// Volume number (`NOT NULL DEFAULT 1` in DB; backfilled for pre-V1.42 rows).
    /// V1.42: always `Some(1)` for single-volume Works; `Some(N)` for multi-volume.
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
/// per novel-writing/workflow-profile.md §5.4.5).
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
        // (work_id, volume, chapter) preserves existing rows).
        // V1.42: explicit volume = 1 (PK now includes volume).
        sqlx::query(
            "INSERT OR IGNORE INTO work_chapters
             (work_id, chapter, volume, slug, planned_word_count, actual_word_count,
              status, outline_path, body_path, created_at, updated_at)
             VALUES (?, ?, 1, ?, 4000, NULL, 'not_started', ?, ?, ?, ?)",
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
        // V1.42: explicit volume = 1 (PK now includes volume).
        sqlx::query(
            "INSERT OR IGNORE INTO work_chapters
             (work_id, chapter, volume, slug, planned_word_count, actual_word_count,
              status, outline_path, body_path, created_at, updated_at)
             VALUES (?, ?, 1, ?, 4000, NULL, 'not_started', ?, ?, ?, ?)",
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

/// List all chapter rows for a Work, ordered by volume then chapter number.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_chapters(
    pool: &SqlitePool,
    work_id: &str,
) -> Result<Vec<WorkChapterRecord>, LocalDbError> {
    // SAFETY: SELECT against work_chapters — runtime query.
    // V1.42: ORDER BY volume, chapter for multi-volume Works.
    let rows = sqlx::query(
        "SELECT work_id, chapter, volume, slug, planned_word_count, actual_word_count,
                status, outline_path, body_path, created_at, updated_at
         FROM work_chapters WHERE work_id = ?
         ORDER BY volume, chapter",
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
    /// Volume number (V1.42: `NOT NULL DEFAULT 1`; pass `Some(1)` for single-volume).
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

/// Get a single chapter row by `work_id`, `volume`, and chapter number.
///
/// V1.42: PK is now `(work_id, volume, chapter)`. Pass `volume = 1` for
/// single-volume Works (backward-compatible default).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_chapter(
    pool: &SqlitePool,
    work_id: &str,
    chapter: i32,
    volume: i32,
) -> Result<Option<WorkChapterRecord>, LocalDbError> {
    // SAFETY: SELECT against work_chapters — runtime query.
    let row = sqlx::query(
        "SELECT work_id, chapter, volume, slug, planned_word_count, actual_word_count,
                status, outline_path, body_path, created_at, updated_at
         FROM work_chapters WHERE work_id = ? AND volume = ? AND chapter = ?",
    )
    .bind(work_id)
    .bind(volume)
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
/// V1.42: PK is now `(work_id, volume, chapter)`. Pass `volume = 1` for
/// single-volume Works.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn update_status(
    pool: &SqlitePool,
    work_id: &str,
    chapter: i32,
    volume: i32,
    new_status: &str,
    actual_word_count: Option<u32>,
    now: &str,
) -> Result<(), LocalDbError> {
    // SAFETY: UPDATE against work_chapters — runtime query.
    sqlx::query(
        "UPDATE work_chapters SET status = ?, actual_word_count = ?, updated_at = ?
         WHERE work_id = ? AND volume = ? AND chapter = ?",
    )
    .bind(new_status)
    .bind(actual_word_count.map(|v| i32::try_from(v).unwrap_or(i32::MAX)))
    .bind(now)
    .bind(work_id)
    .bind(volume)
    .bind(chapter)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update a chapter's `outline_path` and `body_path`.
///
/// V1.42: PK is now `(work_id, volume, chapter)`. Pass `volume = 1` for
/// single-volume Works.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn update_paths(
    pool: &SqlitePool,
    work_id: &str,
    chapter: i32,
    volume: i32,
    outline_path: Option<&str>,
    body_path: Option<&str>,
    now: &str,
) -> Result<(), LocalDbError> {
    // SAFETY: UPDATE against work_chapters — runtime query.
    sqlx::query(
        "UPDATE work_chapters SET outline_path = ?, body_path = ?, updated_at = ?
         WHERE work_id = ? AND volume = ? AND chapter = ?",
    )
    .bind(outline_path)
    .bind(body_path)
    .bind(now)
    .bind(work_id)
    .bind(volume)
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
    /// Number of chapter files re-synced to the DB status (DB-as-SSOT).
    pub resynced: u32,
    /// Number of chapter rows preserved unchanged.
    pub preserved: u32,
}

/// A single write decision produced by [`compute_reconcile_diff`] (V1.49 P3,
/// R-V148P4-W3).
///
/// Each variant maps 1:1 to a mutation performed by [`apply_reconcile_diff`].
/// Splitting reconcile into a read-only "compute" phase and a write-only
/// "apply" phase lets the daemon hold the runtime lock **only** for the fast
/// write phase, while the slow filesystem walk + per-chapter DB reads run
/// unlocked. The [`ReconcileReport`] counters are derivable from a
/// [`ReconcileDiff`] without re-reading state (see [`ReconcileDiff::to_report`]),
/// so the dry-run preview and the mutating path produce identical counters.
#[derive(Debug, Clone)]
pub enum ReconcileOp {
    /// Insert a new chapter row, optionally applying a non-default status
    /// and/or word count mirrored from the file frontmatter.
    CreateChapter {
        /// Chapter number parsed from the filename.
        chapter: i32,
        /// Volume parsed from frontmatter (default 1).
        volume: i32,
        /// `Works/<work_ref>/Stories/<filename>` body path.
        body_path: String,
        /// Frontmatter `status` if present and not the `not_started` default.
        status: Option<String>,
        /// Frontmatter `word_count` if present (already converted to `u32`).
        word_count: Option<u32>,
    },
    /// Resync the chapter file's frontmatter `status` to the DB source of
    /// truth (DB-as-SSOT; the file was ahead/conflicting).
    ResyncFileStatus {
        /// Absolute path to the chapter `.md` file.
        path: std::path::PathBuf,
        /// DB status to write into the file frontmatter.
        db_status: String,
    },
    /// Mirror the file's `word_count` into the DB row (status unchanged).
    UpdateWordCount {
        chapter: i32,
        volume: i32,
        /// Current DB status (preserved on the word-count-only update).
        db_status: String,
        word_count: u32,
    },
}

/// The read-only result of [`compute_reconcile_diff`]: an ordered list of
/// [`ReconcileOp`]s plus the count of existing rows that need no write.
///
/// Carrying the decisions (not the raw file/DB state) keeps [`apply_reconcile_diff`]
/// cheap and free of re-reads, which is what makes the lock window short.
#[derive(Debug, Clone, Default)]
pub struct ReconcileDiff {
    /// Ordered write decisions; applying them in order reproduces the legacy
    /// single-pass behavior.
    pub ops: Vec<ReconcileOp>,
    /// Existing rows whose status and `word_count` already agree (no write).
    pub preserved: u32,
}

impl ReconcileDiff {
    /// Derive the [`ReconcileReport`] counters from this diff without touching
    /// the DB or filesystem. Used by both the dry-run preview path and the
    /// post-apply summary so the two paths can never disagree.
    #[must_use]
    pub fn to_report(&self) -> ReconcileReport {
        let mut created = 0u32;
        let mut resynced = 0u32;
        let mut updated = 0u32;
        for op in &self.ops {
            match op {
                ReconcileOp::CreateChapter { .. } => created += 1,
                ReconcileOp::ResyncFileStatus { .. } => resynced += 1,
                ReconcileOp::UpdateWordCount { .. } => updated += 1,
            }
        }
        ReconcileReport {
            created,
            updated,
            resynced,
            preserved: self.preserved,
        }
    }
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
    let canonical = stories_dir
        .canonicalize()
        .map_err(|e| LocalDbError::IoWithPath {
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

/// Rewrite the `status` key in a chapter file's YAML frontmatter to match the
/// DB source of truth, preserving the body content.
///
/// If the file lacks a well-formed frontmatter block (opening `---` without a
/// closing `---`), it is left unchanged.
///
/// The write is performed atomically: content is written to a sibling temp
/// file, flushed, and then renamed over the target. This prevents a partial
/// write from corrupting the chapter file if the process crashes or the
/// filesystem loses power between the write and the rename.
fn sync_frontmatter_status(path: &std::path::Path, status: &str) -> Result<(), LocalDbError> {
    let content = std::fs::read_to_string(path).map_err(|e| LocalDbError::IoWithPath {
        path: path.to_string_lossy().to_string(),
        source: e,
    })?;

    // Only process files that start with a frontmatter delimiter.
    if !content.trim_start().starts_with("---") {
        return Ok(());
    }

    let mut output = Vec::new();
    let mut in_frontmatter = false;
    let mut found_first = false;
    let mut status_set = false;
    let mut body_started = false;

    for line in content.lines() {
        if !body_started && line.trim() == "---" {
            if !found_first {
                found_first = true;
                in_frontmatter = true;
                output.push(line.to_string());
                continue;
            }
            // Closing delimiter: ensure DB status is written before body starts.
            if !status_set {
                output.push(format!("status: {status}"));
                status_set = true;
            }
            output.push(line.to_string());
            in_frontmatter = false;
            body_started = true;
            continue;
        }

        if in_frontmatter {
            if line.trim_start().starts_with("status:") {
                output.push(format!("status: {status}"));
                status_set = true;
            } else {
                output.push(line.to_string());
            }
        } else {
            output.push(line.to_string());
        }
    }

    // If frontmatter was never closed or never opened, leave the file alone.
    if !found_first || in_frontmatter {
        return Ok(());
    }

    let mut new_content = output.join("\n");
    if content.ends_with('\n') && !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    // Atomic write: temp file next to target, flush, then rename.
    // On any error we try to clean up the temp file; the original error is
    // still returned so callers see the actual failure mode.
    let tmp_extension = format!(
        "md.tmp.{}.{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    let temp_path = path.with_extension(&tmp_extension);
    let write_result = std::fs::write(&temp_path, &new_content)
        .and_then(|()| std::fs::File::open(&temp_path)?.sync_data())
        .and_then(|()| std::fs::rename(&temp_path, path));

    if let Err(e) = write_result {
        // Best-effort cleanup; ignore failures because we're already returning
        // the underlying I/O error.
        let _ = std::fs::remove_file(&temp_path);
        return Err(LocalDbError::IoWithPath {
            path: path.to_string_lossy().to_string(),
            source: e,
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
///
/// # Dry-run (V1.49 P2, R-V148P4-W2)
///
/// When `dry_run` is `true`, the function walks the filesystem and the DB to
/// compute the **same** `ReconcileReport` it would return when mutating, but
/// performs **no** writes: no chapter rows are inserted/updated, no chapter
/// file frontmatter is rewritten. Counters (`created` / `updated` / `resynced`
/// / `preserved`) reflect what the mutating path would do. Callers that only
/// want a preview should pass `dry_run = true`; the daemon reconcile handler
/// additionally skips the runtime-lock acquire on this path (overlay §8.2).
///
/// # Implementation (V1.49 P3, R-V148P4-W3)
///
/// This is now a thin wrapper over [`compute_reconcile_diff`] (read-only) and
/// [`apply_reconcile_diff`] (write-only). The daemon handler calls those two
/// directly so the runtime lock is held **only** for the fast write phase.
pub async fn reconcile_from_filesystem(
    pool: &SqlitePool,
    work_id: &str,
    work_ref: &str,
    workspace_root: &std::path::Path,
    now: &str,
    dry_run: bool,
) -> Result<ReconcileReport, LocalDbError> {
    // SAFETY: game-bible profile gate (V1.54 P1).
    // Chapter reconciliation reads `Works/<work_ref>/Stories/` which is
    // novel-specific. Non-novel profiles (game-bible, essay) do not have
    // chapter files and should not run this code path. Return a clear
    // error rather than silently operating on a missing directory.
    {
        // SAFETY: SELECT work_profile — runtime query for profile gate.
        let profile: Option<String> =
            sqlx::query_scalar("SELECT work_profile FROM works WHERE work_id = ?")
                .bind(work_id)
                .fetch_optional(pool)
                .await?
                .flatten();
        // SAFETY: game-bible profile gate (V1.54 P1).
        // Block reconcile for explicitly non-novel profiles (game_bible, essay).
        // Legacy Works (work_profile IS NULL) are treated as novel for backwards
        // compatibility — they were created before the profile system existed.
        // R-V154P1-S002: warn!-level audit trace when reconcile is blocked by
        // profile gate, so operators can see why reconcile is not running.
        if profile.as_deref().is_some() && !crate::is_novel_profile(profile.as_deref()) {
            tracing::warn!(
                target: "reconcile",
                work_id,
                work_ref,
                work_profile = ?profile.as_deref(),
                "chapter reconciliation blocked: work_profile is not novel; \
                 only novel Works have chapter files under Stories/"
            );
            return Err(LocalDbError::Io(format!(
                "chapter reconciliation is not supported for work_profile '{}' (work_id: {work_id}); \
                 only novel Works have chapter files under Stories/",
                profile.as_deref().unwrap_or("null")
            )));
        }
    }

    let diff = compute_reconcile_diff(pool, work_id, work_ref, workspace_root).await?;
    if dry_run {
        return Ok(diff.to_report());
    }
    apply_reconcile_diff(pool, work_id, now, &diff).await
}

/// Read-only phase of reconcile (V1.49 P3, R-V148P4-W3).
///
/// Walks `Works/<work_ref>/Stories/`, reads each `.md` file's frontmatter and
/// the matching `work_chapters` row, and produces a [`ReconcileDiff`] — the
/// ordered list of [`ReconcileOp`]s that [`apply_reconcile_diff`] will execute.
/// Performs **no** writes (no `INSERT`/`UPDATE`, no file frontmatter rewrite),
/// so it is safe to run **without** the runtime lock. The slow part of
/// reconcile (filesystem walk + per-chapter DB reads) lives here precisely so
/// it can run unlocked.
///
/// The trade-off is documented in the plan: under concurrent reconcile +
/// mutate from the same client, the diff may be stale by the time
/// [`apply_reconcile_diff`] re-acquires the lock. This is accepted for the
/// local-first single-writer daemon model (one active creator, schedule
/// operations serialized through the daemon).
///
/// # Errors
///
/// Returns `LocalDbError` if the filesystem walk or a DB read fails.
/// Returns `LocalDbError::PathEscape` if `work_ref` would cause the resolved
/// path to escape the `Works/<work_ref>/` subtree.
pub async fn compute_reconcile_diff(
    pool: &SqlitePool,
    work_id: &str,
    work_ref: &str,
    workspace_root: &std::path::Path,
) -> Result<ReconcileDiff, LocalDbError> {
    let stories_dir = workspace_root.join("Works").join(work_ref).join("Stories");

    // Defense in depth: canonicalize and verify the resolved path stays
    // within the expected Works/<work_ref>/ subtree.  This prevents path
    // traversal even if a caller forgets to validate `work_ref`.
    verify_stories_dir_in_workspace(&stories_dir, workspace_root, work_ref)?;

    let mut diff = ReconcileDiff::default();
    if !stories_dir.is_dir() {
        return Ok(diff);
    }

    let entries = std::fs::read_dir(&stories_dir).map_err(|e| LocalDbError::IoWithPath {
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

        // Parse frontmatter for status, word_count, and volume.
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let fm = parse_frontmatter(&content);
        let fm_status = fm.get("status").cloned();
        let fm_word_count: Option<i32> = fm.get("word_count").and_then(|v| v.parse().ok());
        let fm_word_count_u32: Option<u32> = fm_word_count.map(|v| u32::try_from(v).unwrap_or(0));
        // R-V142P1-F-003: parse volume from frontmatter; default to 1 for
        // single-volume works or files without the field.
        // R-V143P0-fix: reject negative/zero volume — default to 1 with a warn.
        let raw_volume: i32 = fm.get("volume").and_then(|v| v.parse().ok()).unwrap_or(1);
        let fm_volume: i32 = if raw_volume >= 1 {
            raw_volume
        } else {
            tracing::warn!(
                path = %path.display(),
                volume = raw_volume,
                "chapter frontmatter has invalid volume (< 1); defaulting to 1"
            );
            1
        };

        // Check if row exists (volume-aware: use frontmatter volume).
        let existing = get_chapter(pool, work_id, ch_num, fm_volume).await?;

        match existing {
            None => {
                // New chapter — record a CreateChapter op.
                let body_path = format!("Works/{work_ref}/Stories/{fname}");
                diff.ops.push(ReconcileOp::CreateChapter {
                    chapter: ch_num,
                    volume: fm_volume,
                    body_path,
                    status: fm_status,
                    word_count: fm_word_count_u32,
                });
            }
            Some(row) => {
                let db_status = row.status.clone();

                // §4.5.3: `work_chapters` is the queryable SSOT for status. If
                // the file frontmatter disagrees, the file is re-synced so
                // prompt-visible state catches up without inventing an extra
                // chapter transition.
                let status_conflicts = fm_status.as_ref().is_some_and(|s| s != &db_status);
                if status_conflicts {
                    diff.ops.push(ReconcileOp::ResyncFileStatus {
                        path: path.clone(),
                        db_status: db_status.clone(),
                    });
                }

                // Mirror word_count from file to DB when present and different.
                // This is not a status transition; it only updates the cached
                // actual_word_count.
                let needs_word_count_update =
                    fm_word_count.is_some_and(|wc| row.actual_word_count != Some(wc));
                if needs_word_count_update {
                    let wc = fm_word_count_u32.unwrap_or(0);
                    diff.ops.push(ReconcileOp::UpdateWordCount {
                        chapter: ch_num,
                        volume: fm_volume,
                        db_status,
                        word_count: wc,
                    });
                }

                if !status_conflicts && !needs_word_count_update {
                    diff.preserved += 1;
                }
            }
        }
    }

    Ok(diff)
}

/// Write phase of reconcile (V1.49 P3, R-V148P4-W3).
///
/// Executes each [`ReconcileOp`] in `diff` in order, reproducing the legacy
/// single-pass behavior. This is the only phase that mutates the DB or the
/// chapter files, so the daemon holds the runtime lock **only** across this
/// call. Returns a [`ReconcileReport`] derived from the diff (identical to
/// what [`ReconcileDiff::to_report`] produces), so the report never depends on
/// a post-write re-read.
///
/// # Errors
///
/// Returns `LocalDbError` if any database or I/O write fails.
pub async fn apply_reconcile_diff(
    pool: &SqlitePool,
    work_id: &str,
    now: &str,
    diff: &ReconcileDiff,
) -> Result<ReconcileReport, LocalDbError> {
    for op in &diff.ops {
        match op {
            ReconcileOp::CreateChapter {
                chapter,
                volume,
                body_path,
                status,
                word_count,
            } => {
                // insert_chapter defaults to 'not_started'.
                insert_chapter(
                    pool,
                    &InsertChapterParams {
                        work_id,
                        chapter: *chapter,
                        volume: Some(*volume),
                        slug: None,
                        planned_word_count: 4000,
                        outline_path: None,
                        body_path: Some(body_path),
                        now,
                    },
                )
                .await?;
                // Apply frontmatter status + word_count if present.
                if status.as_deref() != Some("not_started") || word_count.is_some() {
                    update_status(
                        pool,
                        work_id,
                        *chapter,
                        *volume,
                        status.as_deref().unwrap_or("not_started"),
                        *word_count,
                        now,
                    )
                    .await?;
                }
            }
            ReconcileOp::ResyncFileStatus { path, db_status } => {
                sync_frontmatter_status(path, db_status)?;
            }
            ReconcileOp::UpdateWordCount {
                chapter,
                volume,
                db_status,
                word_count,
            } => {
                update_status(
                    pool,
                    work_id,
                    *chapter,
                    *volume,
                    db_status,
                    Some(*word_count),
                    now,
                )
                .await?;
            }
        }
    }

    Ok(diff.to_report())
}

/// Select the next chapter to work on per novel-workflow-profile §4.5.2.
///
/// Returns the **lowest chapter number** whose status is in the active set
/// `{not_started, outlined, draft}`. This preserves serial chapter order:
/// an earlier draft/outlined chapter is always resumed before advancing to
/// a later `not_started` chapter (spec §4.5.2 resume/outlined semantics).
///
/// Returns `Ok(None)` when all chapters are finalized/published (novel-completion).
///
/// # Concurrency (R-V138P0-01)
///
/// Nexus is **local-first single-user**: there is exactly one writer for any
/// given `work_id` at a time (one CLI / one creator). Under that invariant,
/// the single-statement `SELECT MIN(chapter)` query below is race-free: no
/// other transaction can advance a chapter between the read and the caller's
/// subsequent status update.
///
/// If a future change introduces concurrent writers for the same Work (for
/// example, a daemon-side auto-advancer running alongside a `creator run
/// continue`), this function must be paired with an atomic claim helper
/// (e.g. `UPDATE work_chapters SET status='draft' WHERE work_id=? AND
/// chapter=(SELECT MIN(...)) AND status='not_started' RETURNING chapter`) to
/// prevent two writers from claiming the same chapter. Until then, this
/// pattern is intentional and acceptable.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn next_chapter(pool: &SqlitePool, work_id: &str) -> Result<Option<i32>, LocalDbError> {
    // Single query: lowest chapter among all active statuses.
    // Excludes finalized/published so completed chapters are never selected.
    // SAFETY: SELECT against work_chapters — runtime query.
    let row = sqlx::query(
        "SELECT MIN(chapter) AS chapter FROM work_chapters \
         WHERE work_id = ? AND status IN ('not_started', 'outlined', 'draft')",
    )
    .bind(work_id)
    .fetch_optional(pool)
    .await?;

    row.map_or_else(
        || Ok(None),
        |r| {
            let ch: Option<i32> = r.get("chapter");
            Ok(ch)
        },
    )
}

/// Volume-aware next chapter selection per novel-workflow-profile §4.5.2 + §4.5.4.
///
/// Returns the **lowest (volume, chapter) pair** whose status is in the active set
/// `{not_started, outlined, draft}`. For single-volume Works this is equivalent
/// to [`next_chapter`]. For multi-volume Works, this crosses volume boundaries:
/// after all chapters in volume N are finalized, it picks the first active chapter
/// in volume N+1.
///
/// Returns `Ok(None)` when all chapters across all volumes are finalized (novel-completion).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn next_chapter_volume_aware(
    pool: &SqlitePool,
    work_id: &str,
) -> Result<Option<(i32, i32)>, LocalDbError> {
    // SAFETY: SELECT against work_chapters — runtime query.
    let row = sqlx::query(
        "SELECT volume, chapter FROM work_chapters \
         WHERE work_id = ? AND status IN ('not_started', 'outlined', 'draft') \
         ORDER BY volume ASC, chapter ASC LIMIT 1",
    )
    .bind(work_id)
    .fetch_optional(pool)
    .await?;

    row.map_or_else(
        || Ok(None),
        |r| {
            let vol: i32 = r.get("volume");
            let ch: i32 = r.get("chapter");
            Ok(Some((vol, ch)))
        },
    )
}

// ── V1.50 T-A P3: auto-chronology finish-detection DAOs ──────────────────

/// The highest volume number currently seeded for a Work (V1.50 §3 step 1).
///
/// Auto-chronology treats `max(volume)` over `work_chapters` as the "current
/// volume" (there is no `current_volume` column on `works`). Returns `Ok(None)`
/// when the Work has no chapter rows — the caller treats that as not-yet-
/// eligible (no volume to advance from).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn current_volume(pool: &SqlitePool, work_id: &str) -> Result<Option<i32>, LocalDbError> {
    let row = sqlx::query_scalar!(
        r#"SELECT MAX(volume) as "volume: i32" FROM work_chapters WHERE work_id = ?"#,
        work_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.flatten())
}

/// Check whether every chapter in a given volume is `finalized` (V1.50 §3
/// step 2 — finish detection).
///
/// Returns `false` if the volume has no chapter rows or any row is not in
/// `finalized` status. Returns `true` only when the volume has ≥1 row and all
/// are `finalized`.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn is_volume_fully_finalized(
    pool: &SqlitePool,
    work_id: &str,
    volume: i32,
) -> Result<bool, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT
            COUNT(*) as "total_rows!",
            COALESCE(SUM(CASE WHEN status = 'finalized' THEN 1 ELSE 0 END), 0) as "finalized_rows!"
        FROM work_chapters WHERE work_id = ? AND volume = ?"#,
        work_id,
        volume,
    )
    .fetch_one(pool)
    .await?;

    let total: i64 = row.total_rows;
    let finalized: i64 = row.finalized_rows;
    Ok(total > 0 && total == finalized)
}

/// Seed chapter rows for a single volume inside an existing transaction
/// (V1.50 §4.2 — auto-chronology advance).
///
/// Inserts one `work_chapters` row per chapter (`1..=chapter_count`) for the
/// given `volume`, mirroring the path convention of
/// [`seed_chapters_multi_volume_tx`]. `INSERT OR IGNORE` keeps the seed
/// idempotent on PK conflict `(work_id, volume, chapter)`. The caller decides
/// whether to commit or roll back the transaction (atomic with the outline
/// creation per spec §4).
///
/// # Errors
///
/// Returns `LocalDbError` if any insert fails.
pub async fn seed_volume_chapters_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    work_id: &str,
    work_ref: &str,
    volume: i32,
    chapter_count: i32,
    now: &str,
) -> Result<(), LocalDbError> {
    for ch in 1..=chapter_count {
        let ch_nn = format!("ch{ch:02}");
        let outline_path =
            format!("Works/{work_ref}/Outlines/chapters/v{volume:02}-{ch_nn}-outline.md");
        let slug = format!("v{volume:02}-{ch_nn}");
        let body_path = format!("Works/{work_ref}/Stories/v{volume:02}-{ch_nn}-{slug}.md");

        // INSERT OR IGNORE — idempotent seeding on PK conflict (work_id, chapter).
        // Note: (work_id, chapter) is the PK; volume+chapter uniquely place the row.
        sqlx::query!(
            "INSERT OR IGNORE INTO work_chapters
             (work_id, chapter, volume, slug, planned_word_count, actual_word_count,
              status, outline_path, body_path, created_at, updated_at)
             VALUES (?, ?, ?, ?, 4000, NULL, 'not_started', ?, ?, ?, ?)",
            work_id,
            ch,
            volume,
            slug,
            outline_path,
            body_path,
            now,
            now,
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Seed chapter rows for a multi-volume Work (V1.42 §4.5.4).
///
/// For each volume in `volumes`, inserts `chapters_per_volume` rows. On PK
/// conflict `(work_id, volume, chapter)`, existing rows are not overwritten
/// (idempotent).
///
/// # Errors
///
/// Returns `LocalDbError` if any insert fails.
pub async fn seed_chapters_multi_volume(
    pool: &SqlitePool,
    work_id: &str,
    work_ref: &str,
    total_volumes: i32,
    chapters_per_volume: i32,
    now: &str,
) -> Result<(), LocalDbError> {
    let mut tx = pool.begin().await?;

    for vol in 1..=total_volumes {
        for ch in 1..=chapters_per_volume {
            let ch_nn = format!("ch{ch:02}");
            let outline_path =
                format!("Works/{work_ref}/Outlines/chapters/v{vol:02}-{ch_nn}-outline.md");
            let slug = format!("v{vol:02}-{ch_nn}");
            let body_path = format!("Works/{work_ref}/Stories/v{vol:02}-{ch_nn}-{slug}.md");

            // SAFETY: INSERT OR IGNORE — idempotent seeding on PK conflict.
            sqlx::query(
                "INSERT OR IGNORE INTO work_chapters
                 (work_id, chapter, volume, slug, planned_word_count, actual_word_count,
                  status, outline_path, body_path, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 4000, NULL, 'not_started', ?, ?, ?, ?)",
            )
            .bind(work_id)
            .bind(ch)
            .bind(vol)
            .bind(&slug)
            .bind(&outline_path)
            .bind(&body_path)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

/// Seed multi-volume chapter rows inside an existing transaction (V1.42 T3).
///
/// Same logic as [`seed_chapters_multi_volume`] but uses a caller-provided
/// transaction so the seed can be atomic with `patch_work_tx`.
///
/// # Errors
///
/// Returns `LocalDbError` if any insert fails. The caller decides whether
/// to commit or roll back the transaction.
pub async fn seed_chapters_multi_volume_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    work_id: &str,
    work_ref: &str,
    total_volumes: i32,
    chapters_per_volume: i32,
    now: &str,
) -> Result<(), LocalDbError> {
    for vol in 1..=total_volumes {
        for ch in 1..=chapters_per_volume {
            let ch_nn = format!("ch{ch:02}");
            let outline_path =
                format!("Works/{work_ref}/Outlines/chapters/v{vol:02}-{ch_nn}-outline.md");
            let slug = format!("v{vol:02}-{ch_nn}");
            let body_path = format!("Works/{work_ref}/Stories/v{vol:02}-{ch_nn}-{slug}.md");

            // SAFETY: INSERT OR IGNORE — idempotent seeding on PK conflict.
            sqlx::query(
                "INSERT OR IGNORE INTO work_chapters
                 (work_id, chapter, volume, slug, planned_word_count, actual_word_count,
                  status, outline_path, body_path, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 4000, NULL, 'not_started', ?, ?, ?, ?)",
            )
            .bind(work_id)
            .bind(ch)
            .bind(vol)
            .bind(&slug)
            .bind(&outline_path)
            .bind(&body_path)
            .bind(now)
            .bind(now)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

/// Check whether a Work is completed per novel-workflow-profile §6.1.
///
/// For **novel-profile** Works (`work_profile == 'novel'`): runs the volume-aware
/// §6.1 check — all chapter rows across **all volumes** must be `finalized`,
/// total row count must match `total_planned_chapters`, and `intake_status`
/// must be `'complete'`. V1.44 P2 (F-002): replaces the flat
/// `current_chapter >= total` predicate which was fragile for multi-volume Works
/// where chapter numbers reset per volume.
///
/// For **game-bible** Works (`work_profile == 'game_bible'` — V1.54 P1): returns
/// `false` immediately. Completion detection is deferred to V1.55+ where the
/// daemon will evaluate `section_status` frontmatter across Design files.
/// In V1.54, completion is manual via `creator works complete`.
///
/// For **other non-novel** Works (V1.36 backwards compat): returns `true` immediately
/// if `works.status == 'completed'` — the early exit preserves legacy behaviour.
///
/// Returns `false` if:
/// - `total_planned_chapters` is NULL or 0
/// - Chapter row count does not match `total_planned_chapters`
/// - Any chapter row is not in `finalized` status
/// - `intake_status` is not `'complete'`
/// - The work or chapters don't exist
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn is_work_completed(pool: &SqlitePool, work_id: &str) -> Result<bool, LocalDbError> {
    // SAFETY: SELECT against works — runtime query.
    let row = sqlx::query(
        "SELECT status, work_profile, total_planned_chapters, intake_status \
         FROM works WHERE work_id = ?",
    )
    .bind(work_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(false);
    };

    let status: String = row.get("status");
    let work_profile: Option<String> = row.get("work_profile");

    // SAFETY: game-bible profile gate (V1.54 P1; V1.55 P2 completion).
    // V1.54: game-bible completion detection was deferred. V1.55 P2 enables
    // section-level evaluation via [is_game_bible_design_complete].
    // The novel chapter-completion logic must never apply to game-bible Works.
    // R-V154P1-S002: info!-level trace on this guard path so operators can
    // audit when the novel-completion logic is bypassed per-profile.
    if crate::is_game_bible_profile(work_profile.as_deref()) {
        tracing::info!(
            target: "completion",
            work_id,
            "game-bible profile: bypassing novel chapter-completion; \
             design-section evaluation handled by is_game_bible_design_complete"
        );
        // V1.55 P2: game-bible completion is now evaluated by the caller
        // (e.g., get_work handler) via is_game_bible_design_complete,
        // which inspects Design/*.md frontmatter.
        return Ok(false);
    }

    // SAFETY: script profile gate (V1.60 P1).
    // The novel chapter-completion logic must never apply to script Works.
    // Script completion is evaluated via [is_script_complete] which inspects
    // Scripts/*.md and Beats/*.md frontmatter in the workspace filesystem.
    if crate::is_script_profile(work_profile.as_deref()) {
        tracing::info!(
            target: "completion",
            work_id,
            "script profile: bypassing novel chapter-completion; \
             script-section evaluation handled by is_script_complete"
        );
        return Ok(false);
    }

    // SAFETY: essay profile gate (V1.63 P0).
    // The novel chapter-completion logic must never apply to essay Works.
    // Essay completion is evaluated via [is_essay_complete] which inspects
    // Drafts/draft.md frontmatter in the workspace filesystem.
    if crate::is_essay_profile(work_profile.as_deref()) {
        tracing::info!(
            target: "completion",
            work_id,
            "essay profile: bypassing novel chapter-completion; \
             essay-draft evaluation handled by is_essay_complete"
        );
        return Ok(false);
    }

    // Non-novel Works: keep the legacy early exit (V1.36 backwards compat).
    // Novel-profile Works: always fall through to the full §6.1 check.
    if status == "completed" && !crate::is_novel_profile(work_profile.as_deref()) {
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

    // V1.44 P2 (F-002): Volume-aware completion check.
    // Instead of the flat `current_chapter >= total` + `list_chapters` comparison
    // (which breaks when chapter numbers reset across volumes), use a single
    // DB query that counts total rows and finalized rows across ALL volumes.
    // SAFETY: SELECT COUNT against work_chapters — runtime query.
    let count_row = sqlx::query(
        "SELECT \
             COUNT(*) AS total_rows, \
             SUM(CASE WHEN status = 'finalized' THEN 1 ELSE 0 END) AS finalized_rows \
         FROM work_chapters WHERE work_id = ?",
    )
    .bind(work_id)
    .fetch_one(pool)
    .await?;

    let total_rows: i64 = count_row.get("total_rows");
    let finalized_rows: i64 = count_row.get("finalized_rows");

    // §6.1: row count must match total_planned_chapters and ALL must be finalized.
    let expected = i64::from(total);
    Ok(total_rows == expected && finalized_rows == expected)
}

/// V1.55 P2 — Game-bible design section completion check.
///
/// Evaluates `section_status` frontmatter across all critical `Design/*.md` files
/// in `Works/<work_ref>/Design/`. A game-bible Work is design-complete when:
///
/// 1. Every critical section (`overview.md`, `pillars.md`, `mechanics.md`) has
///    `section_status: accepted` in its YAML frontmatter.
/// 2. The Work's `intake_status == 'complete'`.
///
/// Returns `Ok(false)` when:
/// - Any critical section is missing from the filesystem
/// - Any critical section's `section_status` is not `accepted`
/// - `intake_status` is not `'complete'`
///
/// This function reads files from the workspace filesystem. It is designed to
/// be called from the daemon handler layer (which has access to `workspace_dir`)
/// rather than from the pure-DB `is_work_completed` function.
///
/// # Errors
///
/// Returns `LocalDbError::Io` if the Design directory or a critical file
/// cannot be read.
///
/// # Tracing (R-V154P1-S002)
///
/// Logging level intent (V1.55 P2 fix-wave W-1):
/// - `info!` at meaningful gate transitions (intake not complete; all critical
///   sections accepted) — operators need these decisions without verbose logging.
/// - `debug!` for per-section evaluations — fine-grained detail for debugging
///   but too noisy at `info!` level on the `get_work` hot path.
/// - `warn!` for unexpected states (e.g. missing rows).
pub async fn is_game_bible_design_complete(
    pool: &SqlitePool,
    work_id: &str,
    workspace_dir: &std::path::Path,
) -> Result<bool, LocalDbError> {
    // Critical Design sections per game-bible-profile.md §8.
    const CRITICAL_SECTIONS: &[(&str, &str)] = &[
        ("overview.md", "overview"),
        ("pillars.md", "pillars"),
        ("mechanics.md", "mechanics"),
    ];

    // Resolve work_ref from the works table.
    // SAFETY: SELECT work_ref, intake_status — runtime query.
    let row = sqlx::query("SELECT work_ref, intake_status FROM works WHERE work_id = ?")
        .bind(work_id)
        .fetch_optional(pool)
        .await?;

    let Some(row) = row else {
        tracing::warn!(work_id, "is_game_bible_design_complete: work not found");
        return Ok(false);
    };

    let intake_status: String = row.get("intake_status");
    if intake_status != "complete" {
        tracing::info!(
            target: "completion",
            work_id,
            intake_status,
            "game-bible: intake_status is not complete; design not complete"
        );
        return Ok(false);
    }

    let work_ref: Option<String> = row.get("work_ref");
    let Some(ref work_ref) = work_ref else {
        tracing::debug!(work_id, "is_game_bible_design_complete: work_ref is NULL");
        return Ok(false);
    };

    let design_dir = workspace_dir.join("Works").join(work_ref).join("Design");

    if !design_dir.is_dir() {
        tracing::debug!(
            target = "completion",
            work_id,
            work_ref,
            design_dir = %design_dir.display(),
            "game-bible: Design/ directory missing; design not complete"
        );
        return Ok(false);
    }

    // V1.55 P2 fix-wave W-2: use tokio::fs for async file I/O on the
    // get_work hot path (previously std::fs::read_to_string).
    for (filename, label) in CRITICAL_SECTIONS {
        let path = design_dir.join(filename);
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(
                    target = "completion",
                    work_id,
                    work_ref,
                    section = label,
                    error = %e,
                    "game-bible: critical section file unreadable"
                );
                return Ok(false);
            }
        };
        let fm = parse_frontmatter(&content);
        let status = fm.get("section_status").map_or("draft", String::as_str);
        // Per-section evaluation at debug level (not info — hot path).
        tracing::debug!(
            target = "completion",
            work_id,
            work_ref,
            section = label,
            section_status = status,
            "game-bible: critical section evaluated"
        );
        if status != "accepted" {
            // First non-accepted section is a meaningful gate: keep at info.
            tracing::info!(
                target = "completion",
                work_id,
                work_ref,
                section = label,
                section_status = status,
                "game-bible: critical section not yet accepted; design not complete"
            );
            return Ok(false);
        }
    }

    tracing::info!(
        target = "completion",
        work_id,
        work_ref,
        "game-bible: all critical Design sections accepted + intake complete; design complete"
    );
    Ok(true)
}

/// V1.60 P1 — Script section completion check.
///
/// Evaluates `section_status` frontmatter across all critical script files
/// in `Works/<work_ref>/Scripts/` and `Works/<work_ref>/Beats/`. A script
/// Work is complete when:
///
/// 1. Both critical sections (`Scripts/script.md`, `Beats/beat-sheet.md`)
///    have `section_status: accepted` in their YAML frontmatter.
/// 2. The Work's `intake_status == 'complete'`.
///
/// Returns `Ok(false)` when:
/// - Any critical section is missing from the filesystem
/// - Any critical section's `section_status` is not `accepted`
/// - `intake_status` is not `'complete'`
///
/// This function reads files from the workspace filesystem. It is designed to
/// be called from the daemon handler layer (which has access to `workspace_dir`)
/// rather than from the pure-DB `is_work_completed` function.
///
/// # Errors
///
/// Returns `LocalDbError::Io` if a critical file cannot be read.
pub async fn is_script_complete(
    pool: &SqlitePool,
    work_id: &str,
    workspace_dir: &std::path::Path,
) -> Result<bool, LocalDbError> {
    // Critical script sections per script-profile.md §8.
    const CRITICAL_SECTIONS: &[(&str, &str, &str)] = &[
        ("Scripts/script.md", "script", "Scripts/"),
        ("Beats/beat-sheet.md", "beat-sheet", "Beats/"),
    ];

    // Resolve work_ref from the works table.
    // SAFETY: SELECT work_ref, intake_status — runtime query.
    let row = sqlx::query("SELECT work_ref, intake_status FROM works WHERE work_id = ?")
        .bind(work_id)
        .fetch_optional(pool)
        .await?;

    let Some(row) = row else {
        tracing::warn!(work_id, "is_script_complete: work not found");
        return Ok(false);
    };

    let intake_status: String = row.get("intake_status");
    if intake_status != "complete" {
        tracing::info!(
            target: "completion",
            work_id,
            intake_status,
            "script: intake_status is not complete; script not complete"
        );
        return Ok(false);
    }

    let work_ref: Option<String> = row.get("work_ref");
    let Some(ref work_ref) = work_ref else {
        tracing::debug!(work_id, "is_script_complete: work_ref is NULL");
        return Ok(false);
    };

    for (rel_path, label, _parent_dir) in CRITICAL_SECTIONS {
        let path = workspace_dir.join("Works").join(work_ref).join(rel_path);
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(
                    target = "completion",
                    work_id,
                    work_ref,
                    section = label,
                    error = %e,
                    "script: critical section file unreadable"
                );
                return Ok(false);
            }
        };
        let fm = parse_frontmatter(&content);
        let status = fm.get("section_status").map_or("draft", String::as_str);
        // Per-section evaluation at debug level (not info — hot path).
        tracing::debug!(
            target = "completion",
            work_id,
            work_ref,
            section = label,
            section_status = status,
            "script: critical section evaluated"
        );
        if status != "accepted" {
            // First non-accepted section is a meaningful gate: keep at info.
            tracing::info!(
                target = "completion",
                work_id,
                work_ref,
                section = label,
                section_status = status,
                "script: critical section not yet accepted; script not complete"
            );
            return Ok(false);
        }
    }

    tracing::info!(
        target = "completion",
        work_id,
        work_ref,
        "script: all critical script sections accepted + intake complete; script complete"
    );
    Ok(true)
}

/// V1.63 P0 — Essay completion check.
///
/// An essay Work is complete when:
/// 1. `works.intake_status == 'complete'`
/// 2. `Drafts/draft.md` frontmatter `status == finalized`
///
/// Returns `Ok(false)` when:
/// - The Work or `work_ref` is missing
/// - `intake_status` is not `'complete'`
/// - `Drafts/draft.md` is missing or its frontmatter `status` is not `finalized`
///
/// This function reads files from the workspace filesystem. It is designed to
/// be called from the daemon handler layer (which has access to `workspace_dir`)
/// rather than from the pure-DB `is_work_completed` function.
///
/// # Errors
///
/// Returns `LocalDbError::Io` if the draft file cannot be read.
///
/// # Tracing
///
/// - `info!` at meaningful gate transitions (intake not complete; draft finalized).
/// - `debug!` for per-section evaluations.
/// - `warn!` for unexpected states (e.g. missing rows).
pub async fn is_essay_complete(
    pool: &SqlitePool,
    work_id: &str,
    workspace_dir: &std::path::Path,
) -> Result<bool, LocalDbError> {
    // Resolve work_ref and intake_status from the works table.
    // SAFETY: SELECT work_ref, intake_status — runtime query.
    let row = sqlx::query("SELECT work_ref, intake_status FROM works WHERE work_id = ?")
        .bind(work_id)
        .fetch_optional(pool)
        .await?;

    let Some(row) = row else {
        tracing::warn!(work_id, "is_essay_complete: work not found");
        return Ok(false);
    };

    let intake_status: String = row.get("intake_status");
    if intake_status != "complete" {
        tracing::info!(
            target: "completion",
            work_id,
            intake_status,
            "essay: intake_status is not complete; essay not complete"
        );
        return Ok(false);
    }

    let work_ref: Option<String> = row.get("work_ref");
    let Some(ref work_ref) = work_ref else {
        tracing::debug!(work_id, "is_essay_complete: work_ref is NULL");
        return Ok(false);
    };

    // Read Drafts/draft.md and check frontmatter status.
    let draft_path = workspace_dir
        .join("Works")
        .join(work_ref)
        .join("Drafts")
        .join("draft.md");
    let content = match tokio::fs::read_to_string(&draft_path).await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(
                target = "completion",
                work_id,
                work_ref,
                error = %e,
                "essay: Drafts/draft.md unreadable; essay not complete"
            );
            return Ok(false);
        }
    };

    let fm = parse_frontmatter(&content);
    let status = fm.get("status").map_or("draft", String::as_str);

    tracing::debug!(
        target = "completion",
        work_id,
        work_ref,
        draft_status = status,
        "essay: draft evaluated"
    );

    if status != "finalized" {
        tracing::info!(
            target = "completion",
            work_id,
            work_ref,
            draft_status = status,
            "essay: draft not yet finalized; essay not complete"
        );
        return Ok(false);
    }

    tracing::info!(
        target = "completion",
        work_id,
        work_ref,
        "essay: draft finalized + intake complete; essay complete"
    );
    Ok(true)
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
                volume: Some(1),
                slug: Some("introduction"),
                planned_word_count: 5000,
                outline_path: Some("Works/my-novel/Outlines/chapters/ch01-outline.md"),
                body_path: Some("Works/my-novel/Stories/ch01-introduction.md"),
                now: "2026-06-07T10:00:00Z",
            },
        )
        .await
        .unwrap();

        let row = get_chapter(&pool, "wrk_crud_001", 1, 1)
            .await
            .unwrap()
            .expect("chapter should exist");
        assert_eq!(row.chapter, 1);
        assert_eq!(row.volume, Some(1));
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
            1,
            "draft",
            None,
            "2026-06-07T11:00:00Z",
        )
        .await
        .unwrap();

        let row = get_chapter(&pool, "wrk_crud_002", 1, 1)
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
            1,
            "finalized",
            Some(4200),
            "2026-06-07T12:00:00Z",
        )
        .await
        .unwrap();

        let row = get_chapter(&pool, "wrk_crud_002", 1, 1)
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
                1,
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
                1,
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
            1,
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
    // V1.39 P5 (R-V138P0-05): is_work_completed must return false when
    // total_planned_chapters is NULL (Works that never seeded a plan, e.g.
    // intake_status='pending'). Explicit defense-in-depth test — previously
    // covered only by §6.1 control flow, not by a dedicated assertion.
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_is_work_completed_false_when_total_planned_chapters_null() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_comp_null").await;

        // insert_test_work leaves total_planned_chapters as NULL by default
        // (intake_status='pending', no chapter plan seeded). §6.1 requires
        // total > 0 — NULL must NOT be treated as "complete".
        assert!(
            !is_work_completed(&pool, "wrk_comp_null").await.unwrap(),
            "total_planned_chapters=NULL → MUST NOT be completed (R-V138P0-05)"
        );

        // Even after intake completes and current_chapter is bumped, NULL
        // total_planned_chapters must still gate completion to false.
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query(
            "UPDATE works SET intake_status = 'complete', current_chapter = 5 \
             WHERE work_id = ?",
        )
        .bind("wrk_comp_null")
        .execute(&pool)
        .await
        .unwrap();

        assert!(
            !is_work_completed(&pool, "wrk_comp_null").await.unwrap(),
            "total_planned_chapters=NULL with intake=complete → still NOT completed"
        );
    }

    // -----------------------------------------------------------------------
    // V1.39 P5 (R-V138P0-05 companion): total_planned_chapters = 0 must also
    // gate to false (degenerate value — no chapter plan).
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_is_work_completed_false_when_total_planned_chapters_zero() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_comp_zero").await;

        // SAFETY: UPDATE against works — runtime query.
        sqlx::query(
            "UPDATE works SET total_planned_chapters = 0, intake_status = 'complete', \
             current_chapter = 0 WHERE work_id = ?",
        )
        .bind("wrk_comp_zero")
        .execute(&pool)
        .await
        .unwrap();

        assert!(
            !is_work_completed(&pool, "wrk_comp_zero").await.unwrap(),
            "total_planned_chapters=0 → MUST NOT be completed"
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
            false,
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
    // V1.49 P3 (R-V148P4-W3): compute phase is read-only; apply is the only
    // phase that mutates. This is the DAO-layer evidence that the runtime-lock
    // window can safely exclude the slow filesystem walk.
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_compute_is_read_only_then_apply_writes() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_recon_split").await;

        let stories_dir = dir.path().join("Works").join("my-novel").join("Stories");
        std::fs::create_dir_all(&stories_dir).unwrap();
        let ch1 = stories_dir.join("ch01-intro.md");
        std::fs::write(
            &ch1,
            "---\ntitle: Intro\nchapter: 1\nstatus: draft\nword_count: 1500\n---\nBody",
        )
        .unwrap();

        // 1. Compute (read-only): produces a CreateChapter op but writes nothing.
        let diff = compute_reconcile_diff(&pool, "wrk_recon_split", "my-novel", dir.path())
            .await
            .unwrap();
        assert_eq!(
            diff.to_report().created,
            1,
            "diff should describe one create"
        );
        assert_eq!(diff.ops.len(), 1);
        assert!(
            list_chapters(&pool, "wrk_recon_split")
                .await
                .unwrap()
                .is_empty(),
            "compute_reconcile_diff must not insert chapter rows"
        );
        let after_compute = std::fs::read_to_string(&ch1).unwrap();
        assert!(
            after_compute.contains("status: draft"),
            "compute_reconcile_diff must not rewrite chapter files: {after_compute}"
        );

        // 2. Apply (write): the diff now materializes the row.
        let report = apply_reconcile_diff(&pool, "wrk_recon_split", "2026-06-17T00:00:00Z", &diff)
            .await
            .unwrap();
        assert_eq!(report.created, 1);
        let rows = list_chapters(&pool, "wrk_recon_split").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].chapter, 1);
        assert_eq!(rows[0].status, "draft");
        assert_eq!(rows[0].actual_word_count, Some(1500));
    }

    // -----------------------------------------------------------------------
    // Reconcile: existing row + file status conflict — DB status wins (§4.5.3)
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

        // Create workspace with a file whose frontmatter claims a different
        // status than the DB row. Per §4.5.3 the DB status must win; the file
        // frontmatter is re-synced to the DB status.
        let stories_dir = dir.path().join("Works").join("my-novel").join("Stories");
        std::fs::create_dir_all(&stories_dir).unwrap();

        let ch1_path = stories_dir.join("ch01-intro.md");
        std::fs::write(
            &ch1_path,
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
            false,
        )
        .await
        .unwrap();

        // ch01: DB status stays `not_started` (SSOT), but word_count is mirrored
        // and the file frontmatter is re-synced.
        assert_eq!(
            report.updated, 1,
            "word_count should be mirrored from file to DB"
        );
        assert_eq!(
            report.resynced, 1,
            "status-conflict file should be re-synced to DB status"
        );
        assert_eq!(report.preserved, 2, "ch02 and ch03 are unchanged");
        assert_eq!(report.created, 0);

        // Verify ch01 DB status is preserved
        let ch1 = get_chapter(&pool, "wrk_recon_002", 1, 1)
            .await
            .unwrap()
            .expect("ch1");
        assert_eq!(
            ch1.status, "not_started",
            "DB status must win over filesystem frontmatter (§4.5.3)"
        );
        assert_eq!(
            ch1.actual_word_count,
            Some(3200),
            "word_count should be mirrored"
        );

        // Verify file frontmatter was re-synced to DB status.
        let ch1_file = std::fs::read_to_string(&ch1_path).unwrap();
        assert!(
            ch1_file.contains("status: not_started"),
            "reconcile should re-sync file frontmatter to DB status per §4.5.3"
        );
        assert!(
            ch1_file.contains("Content"),
            "reconcile must preserve chapter body content"
        );

        // Re-run reconcile — should be fully idempotent now that file matches DB.
        let report2 = reconcile_from_filesystem(
            &pool,
            "wrk_recon_002",
            "my-novel",
            dir.path(),
            "2026-06-07T12:00:00Z",
            false,
        )
        .await
        .unwrap();
        assert_eq!(report2.updated, 0);
        assert_eq!(report2.resynced, 0);
        assert_eq!(report2.preserved, 3);
    }

    // -----------------------------------------------------------------------
    // V1.48 P4-fix1 (W-1 qc2): sync_frontmatter_status writes atomically.
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_sync_frontmatter_status_writes_via_temp_file() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_recon_atomic").await;

        // Pre-seed one chapter as `draft` in DB.
        seed_chapters(
            &pool,
            "wrk_recon_atomic",
            "my-novel",
            1,
            "2026-06-07T10:00:00Z",
        )
        .await
        .unwrap();
        update_status(
            &pool,
            "wrk_recon_atomic",
            1,
            1,
            "draft",
            None,
            "2026-06-07T11:00:00Z",
        )
        .await
        .unwrap();

        // Create a file whose frontmatter claims a different status.
        let stories_dir = dir.path().join("Works").join("my-novel").join("Stories");
        std::fs::create_dir_all(&stories_dir).unwrap();
        let ch1_path = stories_dir.join("ch01-intro.md");
        std::fs::write(
            &ch1_path,
            "---\ntitle: Intro\nchapter: 1\nstatus: finalized\n---\nContent",
        )
        .unwrap();

        let report = reconcile_from_filesystem(
            &pool,
            "wrk_recon_atomic",
            "my-novel",
            dir.path(),
            "2026-06-07T12:00:00Z",
            false,
        )
        .await
        .unwrap();

        // The file was re-synced; no DB update because word_count is absent.
        assert_eq!(
            report.resynced, 1,
            "status conflict should count as resynced"
        );
        assert_eq!(report.updated, 0);
        assert_eq!(report.preserved, 0);
        assert_eq!(report.created, 0);

        // Final file content matches DB status and body is preserved.
        let ch1_file = std::fs::read_to_string(&ch1_path).unwrap();
        assert!(
            ch1_file.contains("status: draft"),
            "frontmatter should be re-synced to DB status"
        );
        assert!(
            ch1_file.contains("Content"),
            "body content must be preserved"
        );

        // No temp file should be left behind.
        let leftover_tmp: Vec<_> = std::fs::read_dir(&stories_dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.contains(".tmp."))
            })
            .collect();
        assert!(
            leftover_tmp.is_empty(),
            "atomic write should not leave temp files behind: {leftover_tmp:?}"
        );
    }

    // -----------------------------------------------------------------------
    // R-V142P1-F-003: Volume-aware reconcile
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_reconcile_volume_aware_from_frontmatter() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_recon_vol").await;

        // Create workspace with multi-volume chapter files.
        let stories_dir = dir.path().join("Works").join("my-novel").join("Stories");
        std::fs::create_dir_all(&stories_dir).unwrap();

        // Volume 1 chapter (no volume field → defaults to 1)
        std::fs::write(
            stories_dir.join("ch01-intro.md"),
            "---\ntitle: Intro\nchapter: 1\nstatus: draft\n---\nContent",
        )
        .unwrap();

        // Volume 2 chapter (explicit volume: 2 in frontmatter)
        std::fs::write(
            stories_dir.join("ch01-v2-opening.md"),
            "---\ntitle: Opening\nchapter: 1\nvolume: 2\nstatus: not_started\n---\nContent V2",
        )
        .unwrap();

        let report = reconcile_from_filesystem(
            &pool,
            "wrk_recon_vol",
            "my-novel",
            dir.path(),
            "2026-06-12T10:00:00Z",
            false,
        )
        .await
        .unwrap();

        assert_eq!(report.created, 2);

        // Verify volume 1 chapter 1
        let ch_v1 = get_chapter(&pool, "wrk_recon_vol", 1, 1)
            .await
            .unwrap()
            .expect("v1 ch1");
        assert_eq!(ch_v1.status, "draft");
        assert_eq!(ch_v1.volume, Some(1));

        // Verify volume 2 chapter 1
        let ch_v2 = get_chapter(&pool, "wrk_recon_vol", 1, 2)
            .await
            .unwrap()
            .expect("v2 ch1");
        assert_eq!(ch_v2.status, "not_started");
        assert_eq!(ch_v2.volume, Some(2));
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

    // T10.2: next_chapter resumes lowest active chapter (draft beats later not_started)
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
            1,
            "draft",
            None,
            "2026-06-08T12:00:00Z",
        )
        .await
        .unwrap();

        // ch2 is draft (lower chapter) — must resume it, not skip to ch3
        let next = next_chapter(&pool, "wrk_sel_002").await.unwrap();
        assert_eq!(
            next,
            Some(2),
            "should resume ch2 (draft, lowest active chapter) over ch3 (not_started)"
        );

        // Now finalize ch2 too — ch3 (not_started) is the only active chapter left
        update_status(
            &pool,
            "wrk_sel_002",
            2,
            1,
            "finalized",
            Some(4000),
            "2026-06-08T13:00:00Z",
        )
        .await
        .unwrap();

        let next2 = next_chapter(&pool, "wrk_sel_002").await.unwrap();
        assert_eq!(
            next2,
            Some(3),
            "should select ch3 (not_started) after ch2 finalized"
        );
    }

    // T10.3: Outlined chapter is not skipped in favor of later chapters
    #[tokio::test]
    async fn test_next_chapter_outlined_not_skipped() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_sel_003").await;
        setup_work_for_selection(&pool, "wrk_sel_003", 3).await;
        seed_chapters(&pool, "wrk_sel_003", "my-novel", 3, "2026-06-08T10:00:00Z")
            .await
            .unwrap();

        // ch1=outlined, ch2=not_started — lowest active chapter wins (ch1)
        update_status(
            &pool,
            "wrk_sel_003",
            1,
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
            Some(1),
            "ch1 (outlined) should be selected over ch2 (not_started) — lowest active chapter wins"
        );

        // Now finalize ch1 — ch2 (not_started) becomes the next active chapter
        update_status(
            &pool,
            "wrk_sel_003",
            1,
            1,
            "finalized",
            Some(4000),
            "2026-06-08T12:00:00Z",
        )
        .await
        .unwrap();

        let next2 = next_chapter(&pool, "wrk_sel_003").await.unwrap();
        assert_eq!(
            next2,
            Some(2),
            "ch2 (not_started) should be selected after ch1 finalized"
        );
    }

    // T10.8: Finalized chapters are excluded from selection
    #[tokio::test]
    async fn test_next_chapter_skips_finalized() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_sel_004").await;
        setup_work_for_selection(&pool, "wrk_sel_004", 3).await;
        seed_chapters(&pool, "wrk_sel_004", "my-novel", 3, "2026-06-08T10:00:00Z")
            .await
            .unwrap();

        // ch1=finalized, ch2=outlined, ch3=draft
        update_status(
            &pool,
            "wrk_sel_004",
            1,
            1,
            "finalized",
            Some(4000),
            "2026-06-08T11:00:00Z",
        )
        .await
        .unwrap();
        update_status(
            &pool,
            "wrk_sel_004",
            2,
            1,
            "outlined",
            None,
            "2026-06-08T12:00:00Z",
        )
        .await
        .unwrap();
        update_status(
            &pool,
            "wrk_sel_004",
            3,
            1,
            "draft",
            None,
            "2026-06-08T13:00:00Z",
        )
        .await
        .unwrap();

        let next = next_chapter(&pool, "wrk_sel_004").await.unwrap();
        assert_eq!(
            next,
            Some(2),
            "should select ch2 (outlined, lowest active) — ch1 finalized excluded"
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
                1,
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
                1,
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
            1,
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

    // F-002: is_work_completed novel-profile early-exit narrowing
    #[tokio::test]
    async fn test_completion_novel_profile_needs_full_check() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_comp_novel_001").await;

        // Set work_profile='novel', all §6.1 conditions met, but works.status='draft'
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query(
            "UPDATE works SET work_profile = 'novel', total_planned_chapters = 2, \
             current_chapter = 2, intake_status = 'complete' WHERE work_id = ?",
        )
        .bind("wrk_comp_novel_001")
        .execute(&pool)
        .await
        .unwrap();

        seed_chapters(
            &pool,
            "wrk_comp_novel_001",
            "my-novel",
            2,
            "2026-06-08T10:00:00Z",
        )
        .await
        .unwrap();

        // Finalize all chapters
        for ch in 1..=2 {
            update_status(
                &pool,
                "wrk_comp_novel_001",
                ch,
                1,
                "finalized",
                Some(4000),
                "2026-06-08T12:00:00Z",
            )
            .await
            .unwrap();
        }

        // works.status is still 'draft' but §6.1 conditions are met → completed
        assert!(
            is_work_completed(&pool, "wrk_comp_novel_001").await.unwrap(),
            "novel-profile Work: all §6.1 conditions met → completed even when works.status='draft'"
        );

        // Now set works.status='completed' — still returns true
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query("UPDATE works SET status = 'completed' WHERE work_id = ?")
            .bind("wrk_comp_novel_001")
            .execute(&pool)
            .await
            .unwrap();

        assert!(
            is_work_completed(&pool, "wrk_comp_novel_001")
                .await
                .unwrap(),
            "novel-profile Work: completed status + all §6.1 met → still completed"
        );

        // Now introduce an unfinalized row — should return false despite works.status='completed'
        update_status(
            &pool,
            "wrk_comp_novel_001",
            1,
            1,
            "draft",
            None,
            "2026-06-08T13:00:00Z",
        )
        .await
        .unwrap();

        assert!(
            !is_work_completed(&pool, "wrk_comp_novel_001").await.unwrap(),
            "novel-profile Work: works.status='completed' but ch1 is draft → NOT completed (§6.1 authoritative)"
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

    // F-003: game_bible profile completion gate (V1.54 P1 T8).
    // is_work_completed must return Ok(false) for game-bible Works
    // regardless of works.status — completion detection is deferred to V1.55+.
    #[tokio::test]
    async fn test_is_work_completed_game_bible_returns_false() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_game_bible_001").await;

        // Set work_profile to game_bible and works.status to 'completed'
        // SAFETY: UPDATE against works — runtime query for profile gate test.
        sqlx::query(
            "UPDATE works SET work_profile = 'game_bible', status = 'completed', \
             intake_status = 'complete', total_planned_chapters = 1, current_chapter = 1 \
             WHERE work_id = ?",
        )
        .bind("wrk_game_bible_001")
        .execute(&pool)
        .await
        .unwrap();

        // Even with status='completed', game-bible Works should return false —
        // the game-bible profile gate prevents the novel completion logic from
        // applying to game-bible Works.
        assert!(
            !is_work_completed(&pool, "wrk_game_bible_001")
                .await
                .unwrap(),
            "game-bible profile: is_work_completed must return false \
             (novel completion bypassed; design completion via is_game_bible_design_complete)"
        );
    }

    // V1.55 P2: is_game_bible_design_complete tests
    #[tokio::test]
    async fn test_is_game_bible_design_complete_all_accepted() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_gb_comp_001").await;

        // SAFETY: UPDATE — runtime query.
        sqlx::query(
            "UPDATE works SET work_profile = 'game_bible', work_ref = 'my-game', \
             intake_status = 'complete' WHERE work_id = ?",
        )
        .bind("wrk_gb_comp_001")
        .execute(&pool)
        .await
        .unwrap();

        let design_dir = dir.path().join("Works").join("my-game").join("Design");
        std::fs::create_dir_all(&design_dir).unwrap();
        for filename in ["overview.md", "pillars.md", "mechanics.md"] {
            std::fs::write(
                design_dir.join(filename),
                "---\nsection_status: accepted\nsection_weight: critical\n---\n\n# Content\n",
            )
            .unwrap();
        }

        assert!(
            is_game_bible_design_complete(&pool, "wrk_gb_comp_001", dir.path())
                .await
                .unwrap(),
            "all critical sections accepted → design complete"
        );
    }

    #[tokio::test]
    async fn test_is_game_bible_design_complete_one_draft() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_gb_comp_002").await;

        sqlx::query(
            "UPDATE works SET work_profile = 'game_bible', work_ref = 'my-game-2', \
             intake_status = 'complete' WHERE work_id = ?",
        )
        .bind("wrk_gb_comp_002")
        .execute(&pool)
        .await
        .unwrap();

        let design_dir = dir.path().join("Works").join("my-game-2").join("Design");
        std::fs::create_dir_all(&design_dir).unwrap();
        for (filename, status) in [
            ("overview.md", "accepted"),
            ("pillars.md", "draft"),
            ("mechanics.md", "accepted"),
        ] {
            std::fs::write(
                design_dir.join(filename),
                format!(
                    "---\nsection_status: {status}\nsection_weight: critical\n---\n\n# Content\n"
                ),
            )
            .unwrap();
        }

        assert!(
            !is_game_bible_design_complete(&pool, "wrk_gb_comp_002", dir.path())
                .await
                .unwrap(),
            "pillars is draft → design NOT complete"
        );
    }

    #[tokio::test]
    async fn test_is_game_bible_design_complete_missing_files() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_gb_comp_003").await;

        sqlx::query(
            "UPDATE works SET work_profile = 'game_bible', work_ref = 'my-game-3', \
             intake_status = 'complete' WHERE work_id = ?",
        )
        .bind("wrk_gb_comp_003")
        .execute(&pool)
        .await
        .unwrap();

        // Design dir empty — no critical files
        let design_dir = dir.path().join("Works").join("my-game-3").join("Design");
        std::fs::create_dir_all(&design_dir).unwrap();

        assert!(
            !is_game_bible_design_complete(&pool, "wrk_gb_comp_003", dir.path())
                .await
                .unwrap(),
            "missing critical files → design NOT complete"
        );
    }

    #[tokio::test]
    async fn test_is_game_bible_design_complete_intake_pending() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_gb_comp_004").await;

        // intake_status is pending
        sqlx::query(
            "UPDATE works SET work_profile = 'game_bible', work_ref = 'my-game-4', \
             intake_status = 'pending' WHERE work_id = ?",
        )
        .bind("wrk_gb_comp_004")
        .execute(&pool)
        .await
        .unwrap();

        let design_dir = dir.path().join("Works").join("my-game-4").join("Design");
        std::fs::create_dir_all(&design_dir).unwrap();
        for filename in ["overview.md", "pillars.md", "mechanics.md"] {
            std::fs::write(
                design_dir.join(filename),
                "---\nsection_status: accepted\nsection_weight: critical\n---\n\n# Content\n",
            )
            .unwrap();
        }

        assert!(
            !is_game_bible_design_complete(&pool, "wrk_gb_comp_004", dir.path())
                .await
                .unwrap(),
            "intake is pending → design NOT complete"
        );
    }

    // V1.60 P1: is_script_complete tests

    #[tokio::test]
    async fn test_is_script_complete_all_accepted() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_scr_comp_001").await;

        // SAFETY: UPDATE — runtime query.
        sqlx::query(
            "UPDATE works SET work_profile = 'script', work_ref = 'my-script', \
             intake_status = 'complete' WHERE work_id = ?",
        )
        .bind("wrk_scr_comp_001")
        .execute(&pool)
        .await
        .unwrap();

        let scripts_dir = dir.path().join("Works").join("my-script").join("Scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        let beats_dir = dir.path().join("Works").join("my-script").join("Beats");
        std::fs::create_dir_all(&beats_dir).unwrap();

        std::fs::write(
            scripts_dir.join("script.md"),
            "---\nsection_status: accepted\nsection_weight: critical\n---\n\n# Script\n",
        )
        .unwrap();
        std::fs::write(
            beats_dir.join("beat-sheet.md"),
            "---\nsection_status: accepted\nsection_weight: critical\n---\n\n# Beat Sheet\n",
        )
        .unwrap();

        assert!(
            is_script_complete(&pool, "wrk_scr_comp_001", dir.path())
                .await
                .unwrap(),
            "all critical script sections accepted → script complete"
        );
    }

    #[tokio::test]
    async fn test_is_script_complete_one_draft() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_scr_comp_002").await;

        sqlx::query(
            "UPDATE works SET work_profile = 'script', work_ref = 'my-script-2', \
             intake_status = 'complete' WHERE work_id = ?",
        )
        .bind("wrk_scr_comp_002")
        .execute(&pool)
        .await
        .unwrap();

        let scripts_dir = dir.path().join("Works").join("my-script-2").join("Scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        let beats_dir = dir.path().join("Works").join("my-script-2").join("Beats");
        std::fs::create_dir_all(&beats_dir).unwrap();

        std::fs::write(
            scripts_dir.join("script.md"),
            "---\nsection_status: accepted\nsection_weight: critical\n---\n\n# Script\n",
        )
        .unwrap();
        std::fs::write(
            beats_dir.join("beat-sheet.md"),
            "---\nsection_status: draft\nsection_weight: critical\n---\n\n# Beat Sheet\n",
        )
        .unwrap();

        assert!(
            !is_script_complete(&pool, "wrk_scr_comp_002", dir.path())
                .await
                .unwrap(),
            "beat-sheet is draft → script NOT complete"
        );
    }

    #[tokio::test]
    async fn test_is_script_complete_missing_files() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_scr_comp_003").await;

        sqlx::query(
            "UPDATE works SET work_profile = 'script', work_ref = 'my-script-3', \
             intake_status = 'complete' WHERE work_id = ?",
        )
        .bind("wrk_scr_comp_003")
        .execute(&pool)
        .await
        .unwrap();

        // Scripts/ dir exists but no script.md
        let scripts_dir = dir.path().join("Works").join("my-script-3").join("Scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        // Beats/ dir empty — no beat-sheet.md
        let beats_dir = dir.path().join("Works").join("my-script-3").join("Beats");
        std::fs::create_dir_all(&beats_dir).unwrap();

        assert!(
            !is_script_complete(&pool, "wrk_scr_comp_003", dir.path())
                .await
                .unwrap(),
            "missing critical files → script NOT complete"
        );
    }

    #[tokio::test]
    async fn test_is_script_complete_intake_pending() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_scr_comp_004").await;

        // intake_status is pending
        sqlx::query(
            "UPDATE works SET work_profile = 'script', work_ref = 'my-script-4', \
             intake_status = 'pending' WHERE work_id = ?",
        )
        .bind("wrk_scr_comp_004")
        .execute(&pool)
        .await
        .unwrap();

        let scripts_dir = dir.path().join("Works").join("my-script-4").join("Scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        let beats_dir = dir.path().join("Works").join("my-script-4").join("Beats");
        std::fs::create_dir_all(&beats_dir).unwrap();

        std::fs::write(
            scripts_dir.join("script.md"),
            "---\nsection_status: accepted\nsection_weight: critical\n---\n\n# Script\n",
        )
        .unwrap();
        std::fs::write(
            beats_dir.join("beat-sheet.md"),
            "---\nsection_status: accepted\nsection_weight: critical\n---\n\n# Beat Sheet\n",
        )
        .unwrap();

        assert!(
            !is_script_complete(&pool, "wrk_scr_comp_004", dir.path())
                .await
                .unwrap(),
            "intake is pending → script NOT complete"
        );
    }

    // V1.60 P1: script profile gate in is_work_completed
    #[tokio::test]
    async fn test_is_work_completed_script_returns_false() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_script_001").await;

        // Set work_profile to script and works.status to 'completed'
        // SAFETY: UPDATE against works — runtime query for profile gate test.
        sqlx::query(
            "UPDATE works SET work_profile = 'script', status = 'completed', \
             intake_status = 'complete', total_planned_chapters = 1, current_chapter = 1 \
             WHERE work_id = ?",
        )
        .bind("wrk_script_001")
        .execute(&pool)
        .await
        .unwrap();

        // Even with status='completed', script Works should return false —
        // the script profile gate prevents the novel completion logic from
        // applying to script Works.
        assert!(
            !is_work_completed(&pool, "wrk_script_001").await.unwrap(),
            "script profile: is_work_completed must return false \
             (novel completion bypassed; script section completion via is_script_complete)"
        );
    }

    // V1.60 P1: is_script_profile helper test
    #[test]
    fn test_is_script_profile() {
        assert!(crate::is_script_profile(Some("script")));
        assert!(!crate::is_script_profile(Some("novel")));
        assert!(!crate::is_script_profile(Some("game_bible")));
        assert!(!crate::is_script_profile(Some("essay")));
        assert!(!crate::is_script_profile(None));
    }

    // R-V143P0-fix: negative/zero volume frontmatter must default to 1.
    #[tokio::test]
    async fn test_reconcile_volume_rejects_negative() {
        let (pool, dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_neg_vol").await;

        let stories_dir = dir.path().join("Works").join("my-novel").join("Stories");
        std::fs::create_dir_all(&stories_dir).unwrap();

        // Chapter with volume: -1 in frontmatter.
        std::fs::write(
            stories_dir.join("ch01-negative.md"),
            "---\ntitle: Bad Vol\nchapter: 1\nvolume: -1\nstatus: draft\n---\nContent",
        )
        .unwrap();

        let report = reconcile_from_filesystem(
            &pool,
            "wrk_neg_vol",
            "my-novel",
            dir.path(),
            "2026-06-12T10:00:00Z",
            false,
        )
        .await
        .unwrap();

        // Row should be created with volume=1 (defaulted), not -1.
        assert_eq!(report.created, 1);
        let chapters = list_chapters(&pool, "wrk_neg_vol").await.unwrap();
        assert_eq!(chapters.len(), 1);
        assert_eq!(
            chapters[0].volume,
            Some(1),
            "negative volume must default to 1"
        );
    }

    // =======================================================================
    // V1.44 P2 (F-002): Multi-volume completion regression tests
    // =======================================================================

    /// AC1: 2-volume Work (2×3 = 6 chapters) completes only when ALL volume
    /// rows are finalized. Previously, the flat `current_chapter >= total`
    /// check would fail because `current_chapter` resets per volume.
    #[tokio::test]
    async fn test_is_work_completed_multi_volume_all_finalized() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_mv_comp_001").await;

        // Set up as a 2-volume novel with 6 total chapters
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query(
            "UPDATE works SET total_planned_chapters = 6, intake_status = 'complete' \
             WHERE work_id = ?",
        )
        .bind("wrk_mv_comp_001")
        .execute(&pool)
        .await
        .unwrap();

        // Seed 2 volumes × 3 chapters using multi-volume seeder
        seed_chapters_multi_volume(
            &pool,
            "wrk_mv_comp_001",
            "my-novel",
            2,
            3,
            "2026-06-13T10:00:00Z",
        )
        .await
        .unwrap();

        // Finalize ALL 6 chapters across both volumes
        for vol in 1..=2 {
            for ch in 1..=3 {
                update_status(
                    &pool,
                    "wrk_mv_comp_001",
                    ch,
                    vol,
                    "finalized",
                    Some(4000),
                    "2026-06-13T12:00:00Z",
                )
                .await
                .unwrap();
            }
        }

        assert!(
            is_work_completed(&pool, "wrk_mv_comp_001").await.unwrap(),
            "2-volume Work with all 6 chapters finalized should be completed (F-002)"
        );
    }

    /// AC1 negative: 2-volume Work where vol 1 is finalized but vol 2 has a
    /// draft chapter — must NOT be completed.
    #[tokio::test]
    async fn test_is_work_completed_multi_volume_partial_vol2() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_mv_comp_002").await;

        // SAFETY: UPDATE against works — runtime query.
        sqlx::query(
            "UPDATE works SET total_planned_chapters = 6, intake_status = 'complete' \
             WHERE work_id = ?",
        )
        .bind("wrk_mv_comp_002")
        .execute(&pool)
        .await
        .unwrap();

        seed_chapters_multi_volume(
            &pool,
            "wrk_mv_comp_002",
            "my-novel",
            2,
            3,
            "2026-06-13T10:00:00Z",
        )
        .await
        .unwrap();

        // Finalize all vol 1 chapters
        for ch in 1..=3 {
            update_status(
                &pool,
                "wrk_mv_comp_002",
                ch,
                1,
                "finalized",
                Some(4000),
                "2026-06-13T12:00:00Z",
            )
            .await
            .unwrap();
        }

        // Vol 2: finalize ch1+ch2 but leave ch3 as draft
        for ch in 1..=2 {
            update_status(
                &pool,
                "wrk_mv_comp_002",
                ch,
                2,
                "finalized",
                Some(4000),
                "2026-06-13T12:00:00Z",
            )
            .await
            .unwrap();
        }
        update_status(
            &pool,
            "wrk_mv_comp_002",
            3,
            2,
            "draft",
            None,
            "2026-06-13T11:00:00Z",
        )
        .await
        .unwrap();

        assert!(
            !is_work_completed(&pool, "wrk_mv_comp_002").await.unwrap(),
            "2-volume Work with vol2 ch3 still draft → should NOT be completed"
        );
    }

    /// F-002 edge: row count mismatch (only vol 1 seeded, total says 6).
    #[tokio::test]
    async fn test_is_work_completed_multi_volume_missing_vol2_rows() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_mv_comp_003").await;

        // SAFETY: UPDATE against works — runtime query.
        sqlx::query(
            "UPDATE works SET total_planned_chapters = 6, intake_status = 'complete' \
             WHERE work_id = ?",
        )
        .bind("wrk_mv_comp_003")
        .execute(&pool)
        .await
        .unwrap();

        // Only seed vol 1 (3 rows, total says 6)
        seed_chapters_multi_volume(
            &pool,
            "wrk_mv_comp_003",
            "my-novel",
            1,
            3,
            "2026-06-13T10:00:00Z",
        )
        .await
        .unwrap();

        // Finalize all vol 1 chapters
        for ch in 1..=3 {
            update_status(
                &pool,
                "wrk_mv_comp_003",
                ch,
                1,
                "finalized",
                Some(4000),
                "2026-06-13T12:00:00Z",
            )
            .await
            .unwrap();
        }

        assert!(
            !is_work_completed(&pool, "wrk_mv_comp_003").await.unwrap(),
            "total=6 but only 3 rows seeded → should NOT be completed (row count mismatch)"
        );
    }

    // =======================================================================
    // §4.5.7 acceptance tests #1–#3 (V1.47 P2)
    // novel-workflow-profile §4.5.7: canonical acceptance tests for the
    // multi-chapter roadmap. Tests #4 (reconcile) and #5 (resume) are
    // deferred to V1.48.
    // =======================================================================

    /// §4.5.7 #1 — Chapter selection: a 3-chapter Work with rows at varied
    /// statuses; assert `next_chapter(work_id)` returns the lowest eligible row
    /// per §4.5.2.
    ///
    /// Exercises the full §4.5.2 selection algorithm in one flow:
    /// not_started → outlined → draft → finalized exclusion. Verifies that the
    /// lowest-numbered active chapter wins at each step.
    #[tokio::test]
    async fn spec_4_5_7_chapter_selection_returns_lowest_eligible() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_457_001").await;
        setup_work_for_selection(&pool, "wrk_457_001", 3).await;
        seed_chapters(&pool, "wrk_457_001", "my-novel", 3, "2026-06-15T10:00:00Z")
            .await
            .unwrap();

        // Step 1: all not_started → ch1 (lowest)
        let next = next_chapter(&pool, "wrk_457_001").await.unwrap();
        assert_eq!(
            next,
            Some(1),
            "all not_started → ch1 (lowest not_started per §4.5.2)"
        );

        // Step 2: ch1=outlined, ch2=draft → ch1 (outlined is active, lowest)
        update_status(
            &pool,
            "wrk_457_001",
            1,
            1,
            "outlined",
            None,
            "2026-06-15T11:00:00Z",
        )
        .await
        .unwrap();
        update_status(
            &pool,
            "wrk_457_001",
            2,
            1,
            "draft",
            None,
            "2026-06-15T11:00:00Z",
        )
        .await
        .unwrap();
        let next = next_chapter(&pool, "wrk_457_001").await.unwrap();
        assert_eq!(
            next,
            Some(1),
            "ch1=outlined beats ch2=draft (lowest active chapter per §4.5.2)"
        );

        // Step 3: ch1 finalized → ch2 (draft, now lowest active)
        update_status(
            &pool,
            "wrk_457_001",
            1,
            1,
            "finalized",
            Some(4000),
            "2026-06-15T12:00:00Z",
        )
        .await
        .unwrap();
        let next = next_chapter(&pool, "wrk_457_001").await.unwrap();
        assert_eq!(
            next,
            Some(2),
            "after ch1 finalized → ch2 (draft, lowest active)"
        );

        // Step 4: ch2 finalized → ch3 (not_started, only remaining)
        update_status(
            &pool,
            "wrk_457_001",
            2,
            1,
            "finalized",
            Some(4000),
            "2026-06-15T13:00:00Z",
        )
        .await
        .unwrap();
        let next = next_chapter(&pool, "wrk_457_001").await.unwrap();
        assert_eq!(
            next,
            Some(3),
            "after ch1+ch2 finalized → ch3 (not_started, only remaining)"
        );

        // Step 5: ch3 finalized → None (novel-completion per §4.5.2)
        update_status(
            &pool,
            "wrk_457_001",
            3,
            1,
            "finalized",
            Some(4000),
            "2026-06-15T14:00:00Z",
        )
        .await
        .unwrap();
        let next = next_chapter(&pool, "wrk_457_001").await.unwrap();
        assert_eq!(
            next, None,
            "all chapters finalized → None (novel-completion per §4.5.2)"
        );
    }

    /// §4.5.7 #3 — Novel completion: completion fires only when every row is
    /// `finalized`, `current_chapter >= total_planned_chapters`, and
    /// `intake_status == complete` (§6.1).
    ///
    /// Asserts ALL three conditions in a single flow: positive case fires
    /// completion, then each condition individually violated returns false.
    #[tokio::test]
    async fn spec_4_5_7_completion_requires_all_section_6_1_conditions() {
        let (pool, _dir) = fresh_pool().await;
        insert_test_work(&pool, "wrk_457_002").await;
        setup_work_for_selection(&pool, "wrk_457_002", 2).await;
        // setup_work_for_selection sets intake_status='complete',
        // total_planned_chapters=2, current_chapter=0.
        seed_chapters(&pool, "wrk_457_002", "my-novel", 2, "2026-06-15T10:00:00Z")
            .await
            .unwrap();

        // --- Condition: all rows finalized (initially false — ch1+ch2 not_started)
        assert!(
            !is_work_completed(&pool, "wrk_457_002").await.unwrap(),
            "§6.1: not all finalized → NOT complete"
        );

        // Finalize ch1 only
        update_status(
            &pool,
            "wrk_457_002",
            1,
            1,
            "finalized",
            Some(4000),
            "2026-06-15T11:00:00Z",
        )
        .await
        .unwrap();
        assert!(
            !is_work_completed(&pool, "wrk_457_002").await.unwrap(),
            "§6.1: ch2 still not_started → NOT complete"
        );

        // Finalize ch2 — now all rows finalized
        update_status(
            &pool,
            "wrk_457_002",
            2,
            1,
            "finalized",
            Some(4000),
            "2026-06-15T12:00:00Z",
        )
        .await
        .unwrap();

        // --- Condition: current_chapter >= total_planned_chapters
        // current_chapter is still 0 (the test helper doesn't advance it).
        // §6.1 requires current_chapter >= total. The DB-layer check uses row
        // count + finalized count (V1.44 P2 volume-aware), so current_chapter
        // is not directly checked here — but the §4.5.2 invariant requires it
        // to be set on finalize. Set it to match:
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query("UPDATE works SET current_chapter = 2 WHERE work_id = ?")
            .bind("wrk_457_002")
            .execute(&pool)
            .await
            .unwrap();

        // --- All §6.1 conditions met → complete
        assert!(
            is_work_completed(&pool, "wrk_457_002").await.unwrap(),
            "§6.1: all finalized + current_chapter=2 + intake=complete → complete"
        );

        // --- Violate condition: intake_status != 'complete'
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query("UPDATE works SET intake_status = 'pending' WHERE work_id = ?")
            .bind("wrk_457_002")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            !is_work_completed(&pool, "wrk_457_002").await.unwrap(),
            "§6.1: intake_status='pending' → NOT complete even if all finalized"
        );

        // Restore intake
        // SAFETY: UPDATE against works — runtime query.
        sqlx::query("UPDATE works SET intake_status = 'complete' WHERE work_id = ?")
            .bind("wrk_457_002")
            .execute(&pool)
            .await
            .unwrap();

        // --- Violate condition: one row unfinalized
        update_status(
            &pool,
            "wrk_457_002",
            1,
            1,
            "draft",
            None,
            "2026-06-15T13:00:00Z",
        )
        .await
        .unwrap();
        assert!(
            !is_work_completed(&pool, "wrk_457_002").await.unwrap(),
            "§6.1: ch1 reverted to draft → NOT complete despite current_chapter=2"
        );
    }
}
