//! Auto-chronology advance engine (V1.50 T-A P3, T2–T6).
//!
//! Spec: `.mstar/knowledge/specs/novel-writing/auto-chronology.md` §3 / §4.
//!
//! ## Role
//!
//! Reusable logic layer for per-Work volume auto-advance on finish. The daemon
//! `nexus_daemon_runtime::auto_chronology` task calls [`run_one_tick`] on a
//! 5-min interval (auto path); the CLI `creator works chronology advance` calls
//! [`advance_manual`] directly (manual override path).
//!
//! ## Finish detection — auto path only (spec §3)
//!
//! For each Work with `auto_chronology = true`:
//! 1. `intake_status == "complete"` — else skip (`IntakeIncomplete`).
//! 2. `runtime_lock_holder IS NULL` — else skip (`RuntimeLocked`).
//! 3. `completion_locked_at IS NULL` — else skip (`CompletionLocked`). This is
//!    also the terminal "last planned volume" guard (spec §3.1): there is no
//!    `total_planned_volumes` column, so a Work with no further volumes is
//!    expected to be completion-locked by the author.
//! 4. `current_volume = max(volume)` exists and is fully finalized — else skip
//!    (`VolumeNotFinalized`).
//! 5. The next-volume outline does **not** already exist — else skip
//!    (`AlreadyAdvanced`, idempotent guard; also the crash-recovery path).
//!
//! ## Manual override (spec §2.2)
//!
//! `advance_manual` bypasses the finish-detection gates (intake / lock /
//! finalization) and creates the requested volume immediately. It still honors
//! the idempotent guard (`AlreadyAdvanced`) so it never clobbers an existing
//! outline — a future `--force` flag could override that (out of scope).
//!
//! ## Advance execution (spec §4)
//!
//! 1. Render `Outlines/volume-<N>-outline.md` from the embedded template.
//! 2. Atomic write: temp file + `sync_all` + rename (spec §4.1).
//! 3. Open `state.db` transaction → (optionally) seed chapter rows for the new
//!    volume → bump `works.updated_at` → commit. Crash mid-tx rolls back; the
//!    next tick sees the existing outline and skips (idempotent).
//! 4. Append the chronology log entry (spec §4.3; best-effort, non-fatal).
//!
//! The auto path seeds **zero** chapters because the outline is a placeholder
//! (spec §4.2 last paragraph); the author fills the outline and runs the seed
//! step. The manual CLI may pass a chapter count to seed immediately.

use std::io::Write;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;
use thiserror::Error;

use nexus_local_db::works::WorkAutoChronologyRow;
use nexus_local_db::{work_chapters, works};

/// Embedded volume-outline template (spec §4.1 / AC §6.5).
const VOLUME_OUTLINE_TMPL: &str =
    include_str!("../embedded-presets/novel-writing/templates/volume-outline.md.tmpl");

/// Errors raised by the auto-chronology advance engine.
#[derive(Debug, Error)]
pub enum AutoChronologyError {
    /// A database read/write failed.
    #[error("auto-chronology database error: {0}")]
    Db(#[from] nexus_local_db::LocalDbError),
    /// An on-disk outline/log write failed.
    #[error("auto-chronology io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Why a Work was skipped during an auto-chronology tick (spec §3).
///
/// `AlreadyAdvanced` is also the crash-recovery signal: if the daemon was
/// interrupted mid-advance (outline written, tx not committed), the next tick
/// observes the existing outline and skips cleanly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// `intake_status != "complete"` (spec §3 step 3).
    IntakeIncomplete,
    /// `runtime_lock_holder IS NOT NULL` (spec §3 step 4 / §3.1).
    RuntimeLocked,
    /// `completion_locked_at IS NOT NULL` — Work fully complete (spec §3 step 4).
    /// Also the terminal "last planned volume" guard (spec §3.1).
    CompletionLocked,
    /// The current volume has chapters that are not all `finalized`, or has no
    /// chapter rows at all (spec §3 step 2).
    VolumeNotFinalized,
    /// The target-volume outline already exists on disk (spec §3.1 idempotent
    /// guard; also the post-crash recovery path).
    AlreadyAdvanced,
}

/// Outcome of a single Work's auto-chronology evaluation.
#[derive(Debug, Clone)]
pub enum AdvanceOutcome {
    /// The Work advanced to `next_volume`: outline created, tx committed.
    Advanced {
        /// The Work that advanced.
        work_id: String,
        /// Volume advanced from (`prev_volume` for the auto path; for manual,
        /// the max seeded volume prior to the requested one).
        prev_volume: i32,
        /// Volume advanced to.
        next_volume: i32,
        /// Chapter rows seeded for the new volume (0 for the placeholder path).
        chapters_seeded: i32,
        /// Who triggered the advance (`"daemon auto_chronology_tick"` or
        /// `"manual cli advance"`).
        trigger: String,
    },
    /// The Work was evaluated but did not advance.
    Skipped {
        /// The Work evaluated.
        work_id: String,
        /// Why it was skipped.
        reason: SkipReason,
    },
}

/// Render the volume-outline template with Work metadata (spec §4.1).
///
/// Pure function over the inputs — no IO. Placeholder tokens use the
/// `{{name}}` convention and are substituted with simple `replace` (the
/// template is small and fixed; a templating engine would be over-engineering).
#[must_use]
pub fn render_volume_outline(
    work_id: &str,
    work_ref: &str,
    title: &str,
    prev_volume: i32,
    next_volume: i32,
    total_planned_chapters: Option<i32>,
    generated_at: &str,
) -> String {
    let total_str = total_planned_chapters.map_or_else(
        || "(unset)".to_string(),
        |n| n.to_string(),
    );
    VOLUME_OUTLINE_TMPL
        .replace("{{work_id}}", work_id)
        .replace("{{work_ref}}", work_ref)
        .replace("{{title}}", title)
        .replace("{{prev_volume}}", &prev_volume.to_string())
        .replace("{{next_volume}}", &next_volume.to_string())
        .replace("{{total_planned_chapters}}", &total_str)
        .replace("{{generated_at}}", generated_at)
}

/// Path to a volume outline file: `Works/<work_ref>/Outlines/volume-<N>-outline.md`.
#[must_use]
pub fn outline_path(workspace_dir: &Path, work_ref: &str, volume: i32) -> PathBuf {
    workspace_dir
        .join("Works")
        .join(work_ref)
        .join("Outlines")
        .join(format!("volume-{volume}-outline.md"))
}

/// Atomically write `content` to `path` (temp + fsync + rename).
///
/// Matches the V1.36 atomicity pattern referenced by spec §4.1. The temp file
/// is written beside the target, `sync_all`'d, then renamed over the target.
/// On any error the temp file is removed (best-effort) and the target is left
/// untouched.
///
/// # Errors
///
/// Returns `std::io::Error` if the parent directory does not exist, the temp
/// write/sync fails, or the rename fails.
pub fn write_outline_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp_path = path.with_extension("md.tmp");
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    })();
    if result.is_err() && tmp_path.exists() {
        // Best-effort cleanup of the orphaned temp file.
        let _ = std::fs::remove_file(&tmp_path);
    }
    result
}

/// Path to the advance log file:
/// `Works/<work_ref>/Logs/chronology/<YYYY-MM-DD>-advance-vol<N>.md`.
#[must_use]
pub fn advance_log_path(
    workspace_dir: &Path,
    work_ref: &str,
    date_utc: &str,
    next_volume: i32,
) -> PathBuf {
    workspace_dir
        .join("Works")
        .join(work_ref)
        .join("Logs")
        .join("chronology")
        .join(format!("{date_utc}-advance-vol{next_volume}.md"))
}

/// Append the chronology advance log entry (spec §4.3).
///
/// Creates the `Logs/chronology/` directory if missing. Best-effort: callers
/// log the IO error and continue (the advance already succeeded in the DB; the
/// log is observational).
///
/// # Errors
///
/// Returns `std::io::Error` if directory creation or the append write fails.
pub fn write_advance_log(
    workspace_dir: &Path,
    work_ref: &str,
    next_volume: i32,
    prev_volume: i32,
    chapters_seeded: i32,
    trigger: &str,
    now_utc: &str,
) -> std::io::Result<()> {
    let date = now_utc_date(now_utc);
    let path = advance_log_path(workspace_dir, work_ref, &date, next_volume);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = format!(
        "# Auto-Chronology Advance — Volume {next_volume}\n\
         \n\
         - At: {now_utc}\n\
         - Trigger: {trigger}\n\
         - Previous volume: {prev_volume} (all finalized, intake complete)\n\
         - New volume: {next_volume}\n\
         - Outline: Works/{work_ref}/Outlines/volume-{next_volume}-outline.md (template-rendered)\n\
         - Chapters seeded: {chapters_seeded}\n"
    );
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(body.as_bytes())?;
    Ok(())
}

/// Load the gating columns for a Work regardless of its `auto_chronology` flag.
///
/// Used by [`advance_manual`] which bypasses the opt-in gate. Returns `None`
/// when the Work does not exist.
async fn load_row_for_manual(
    pool: &SqlitePool,
    work_id: &str,
) -> Result<Option<WorkAutoChronologyRow>, AutoChronologyError> {
    // SAFETY: SELECT against works table — runtime query (auto_chronology column
    // added in the same migration cycle). All inputs are bound parameters.
    let row: Option<WorkAutoChronologyRow> = sqlx::query_as(
        "SELECT work_id, creator_id, work_ref, intake_status, \
                runtime_lock_holder, completion_locked_at \
         FROM works WHERE work_id = ?",
    )
    .bind(work_id)
    .fetch_optional(pool)
    .await
    .map_err(nexus_local_db::LocalDbError::from)?;
    Ok(row)
}

/// The shared advance: idempotent guard → outline render+write → tx → log.
///
/// `chapter_count` seeds that many `not_started` rows for `next_volume` inside
/// the transaction (0 → no rows; placeholder-outline path). `trigger` is
/// recorded in the log. `title`/`total_planned_chapters` feed the template.
async fn perform_advance(
    pool: &SqlitePool,
    workspace_dir: &Path,
    work_id: &str,
    work_ref: &str,
    title: &str,
    total_planned_chapters: Option<i32>,
    prev_volume: i32,
    next_volume: i32,
    chapter_count: i32,
    trigger: &str,
) -> Result<AdvanceOutcome, AutoChronologyError> {
    let now = now_utc();

    // Gate (shared): idempotent guard — do not clobber an existing outline.
    let outline = outline_path(workspace_dir, work_ref, next_volume);
    if outline.exists() {
        tracing::info!(
            work_id,
            next_volume,
            "auto-chronology: outline already exists (idempotent skip)"
        );
        return Ok(AdvanceOutcome::Skipped {
            work_id: work_id.to_string(),
            reason: SkipReason::AlreadyAdvanced,
        });
    }

    // Step 1+2 (spec §4.1): render + atomic outline write.
    let rendered =
        render_volume_outline(work_id, work_ref, title, prev_volume, next_volume, total_planned_chapters, &now);
    if let Some(parent) = outline.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_outline_atomic(&outline, &rendered)?;

    // Step 3 (spec §4.2): transactional chapter seed + updated_at bump.
    let seeded = chapter_count.max(0);
    let mut tx = pool
        .begin()
        .await
        .map_err(nexus_local_db::LocalDbError::from)?;
    if seeded > 0 {
        work_chapters::seed_volume_chapters_tx(
            &mut tx, work_id, work_ref, next_volume, seeded, &now,
        )
        .await?;
    }
    // Bump updated_at to mark the advance timestamp (DB-layer signal that the
    // Work changed, independent of the filesystem outline).
    // SAFETY: UPDATE against works — runtime query.
    sqlx::query("UPDATE works SET updated_at = ? WHERE work_id = ?")
        .bind(&now)
        .bind(work_id)
        .execute(&mut *tx)
        .await
        .map_err(nexus_local_db::LocalDbError::from)?;
    tx.commit()
        .await
        .map_err(nexus_local_db::LocalDbError::from)?;

    // Step 4 (spec §4.3): best-effort log entry.
    if let Err(e) = write_advance_log(
        workspace_dir,
        work_ref,
        next_volume,
        prev_volume,
        seeded,
        trigger,
        &now,
    ) {
        tracing::warn!(
            work_id,
            next_volume,
            error = %e,
            "auto-chronology: advance log write failed (non-fatal; advance succeeded)"
        );
    }

    tracing::info!(
        work_id,
        prev_volume,
        next_volume,
        chapters_seeded = seeded,
        "auto-chronology: advanced volume"
    );

    Ok(AdvanceOutcome::Advanced {
        work_id: work_id.to_string(),
        prev_volume,
        next_volume,
        chapters_seeded: seeded,
        trigger: trigger.to_string(),
    })
}

/// Auto-path evaluation: run finish detection for one scanned Work, advance if
/// eligible (spec §3 / §4). Called by [`run_one_tick`].
async fn advance_auto(
    pool: &SqlitePool,
    workspace_dir: &Path,
    row: &WorkAutoChronologyRow,
) -> Result<AdvanceOutcome, AutoChronologyError> {
    let work_id = &row.work_id;

    // Gate 1: intake complete.
    if row.intake_status != "complete" {
        return Ok(skip(work_id, SkipReason::IntakeIncomplete, "intake incomplete"));
    }
    // Gate 2: no runtime lock.
    if row.runtime_lock_holder.is_some() {
        return Ok(skip(work_id, SkipReason::RuntimeLocked, "work is locked"));
    }
    // Gate 3: not completion-locked (terminal "last planned volume" guard).
    if row.completion_locked_at.is_some() {
        return Ok(skip(work_id, SkipReason::CompletionLocked, "work completion-locked"));
    }
    // Gate 4: current volume fully finalized.
    let Some(prev_volume) = work_chapters::current_volume(pool, work_id).await? else {
        return Ok(skip(work_id, SkipReason::VolumeNotFinalized, "no chapters seeded"));
    };
    if !work_chapters::is_volume_fully_finalized(pool, work_id, prev_volume).await? {
        return Ok(skip(work_id, SkipReason::VolumeNotFinalized, "volume not fully finalized"));
    }

    let next_volume = prev_volume + 1;
    let work_ref = row.work_ref.clone().unwrap_or_else(|| work_id.clone());

    // The auto path seeds zero chapters (placeholder outline, spec §4.2).
    perform_advance(
        pool,
        workspace_dir,
        work_id,
        &work_ref,
        "(untitled)",
        None,
        prev_volume,
        next_volume,
        0,
        "daemon auto_chronology_tick",
    )
    .await
}

/// Manual override: create the requested volume regardless of finish detection
/// (spec §2.2). Still honors the idempotent guard so it never clobbers an
/// existing outline.
///
/// `next_volume` is the explicit target (e.g. CLI `--volume 3`). `chapter_count`
/// seeds that many rows for the new volume inside the transaction (`None` → 0).
///
/// # Errors
///
/// Returns [`AutoChronologyError`] on database or IO failure. A missing Work
/// returns `Ok(Skipped { VolumeNotFinalized })` so the caller surfaces it.
pub async fn advance_manual(
    pool: &SqlitePool,
    workspace_dir: &Path,
    work_id: &str,
    next_volume: i32,
    chapter_count: Option<i32>,
) -> Result<AdvanceOutcome, AutoChronologyError> {
    let Some(row) = load_row_for_manual(pool, work_id).await? else {
        return Ok(AdvanceOutcome::Skipped {
            work_id: work_id.to_string(),
            reason: SkipReason::VolumeNotFinalized,
        });
    };
    let work_ref = row.work_ref.clone().unwrap_or_else(|| work_id.to_string());
    let prev_volume = work_chapters::current_volume(pool, work_id)
        .await?
        .unwrap_or(next_volume.saturating_sub(1).max(0));

    perform_advance(
        pool,
        workspace_dir,
        work_id,
        &work_ref,
        "(untitled)",
        None,
        prev_volume,
        next_volume,
        chapter_count.unwrap_or(0),
        "manual cli advance",
    )
    .await
}

/// Run one auto-chronology tick: scan opted-in Works and advance each eligible
/// one (spec §4.1). Called by the daemon task on its interval.
///
/// Non-fatal: per-Work errors are logged and the tick continues.
pub async fn run_one_tick(pool: &SqlitePool, workspace_dir: &Path) {
    let rows = match works::list_works_with_auto_chronology(pool).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "auto-chronology tick: scan failed");
            return;
        }
    };
    if rows.is_empty() {
        tracing::debug!("auto-chronology tick: no opted-in Works");
        return;
    }
    for row in &rows {
        match advance_auto(pool, workspace_dir, row).await {
            Ok(AdvanceOutcome::Advanced {
                next_volume, chapters_seeded, ..
            }) => tracing::info!(
                work_id = %row.work_id,
                next_volume,
                chapters_seeded,
                "auto-chronology tick: advanced"
            ),
            Ok(AdvanceOutcome::Skipped {
                reason, ..
            }) => tracing::debug!(
                work_id = %row.work_id,
                reason = ?reason,
                "auto-chronology tick: skipped"
            ),
            Err(e) => tracing::error!(
                work_id = %row.work_id,
                error = %e,
                "auto-chronology tick: advance failed (non-fatal)"
            ),
        }
    }
}

/// Build a `Skipped` outcome + emit a `DEBUG`/`INFO` log line per spec §3.1.
fn skip(work_id: &str, reason: SkipReason, note: &str) -> AdvanceOutcome {
    let level = match reason {
        // spec §3.1: these two log at INFO.
        SkipReason::CompletionLocked | SkipReason::AlreadyAdvanced => tracing::Level::INFO,
        // runtime lock + intake + volume finalize log at DEBUG (spec §3.1).
        _ => tracing::Level::DEBUG,
    };
    if level == tracing::Level::INFO {
        tracing::info!(work_id, note, "auto-chronology: skip");
    } else {
        tracing::debug!(work_id, note, "auto-chronology: skip");
    }
    AdvanceOutcome::Skipped {
        work_id: work_id.to_string(),
        reason,
    }
}

/// Current UTC timestamp in RFC 3339 (e.g. `2026-06-18T10:00:00+00:00`).
fn now_utc() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Extract the `YYYY-MM-DD` date from an RFC 3339 / ISO 8601 timestamp.
///
/// Falls back to `unknown-date` if the input does not contain a `-` (should
/// not happen for `chrono::Utc::now().to_rfc3339()` output).
fn now_utc_date(ts: &str) -> String {
    // RFC 3339 dates start with YYYY-MM-DD; take the first 10 chars when they
    // look like a date, else fall back.
    if ts.len() >= 10 && ts.as_bytes()[4] == b'-' {
        ts[..10].to_string()
    } else {
        "unknown-date".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_substitutes_all_placeholders() {
        let out = render_volume_outline(
            "wrk_1",
            "my-novel",
            "My Novel",
            1,
            2,
            Some(30),
            "2026-06-18T10:00:00+00:00",
        );
        assert!(out.contains("Volume 2 Outline — My Novel"));
        assert!(out.contains("Previous volume: 1"));
        assert!(out.contains("--volume 2"));
        // No leftover tokens.
        assert!(!out.contains("{{"));
        assert!(!out.contains("}}"));
    }

    #[test]
    fn render_unset_total_shows_unset() {
        let out = render_volume_outline("wrk_1", "n", "T", 1, 2, None, "2026-06-18T10:00:00Z");
        // total_planned_chapters is unreferenced in the current template body
        // (only in metadata), so just assert no tokens remain.
        assert!(!out.contains("{{"));
    }

    #[test]
    fn write_outline_atomic_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("volume-2-outline.md");
        write_outline_atomic(&target, "hello").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");
        // No leftover temp file.
        assert!(!target.with_extension("md.tmp").exists());
    }

    #[test]
    fn write_outline_atomic_missing_parent_fails_clean() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("missing").join("volume-2-outline.md");
        assert!(write_outline_atomic(&target, "x").is_err());
        // No temp file orphaned at the missing parent.
        assert!(!target.with_extension("md.tmp").exists());
    }

    #[test]
    fn advance_log_path_format() {
        let p = advance_log_path(Path::new("/ws"), "nov", "2026-06-18", 2);
        assert_eq!(
            p,
            PathBuf::from("/ws/Works/nov/Logs/chronology/2026-06-18-advance-vol2.md")
        );
    }

    #[test]
    fn write_advance_log_appends_and_creates_dir() {
        let dir = tempfile::tempdir().unwrap();
        write_advance_log(dir.path(), "nov", 2, 1, 0, "test", "2026-06-18T10:00:00Z").unwrap();
        write_advance_log(dir.path(), "nov", 2, 1, 0, "test", "2026-06-18T11:00:00Z").unwrap();
        let p = advance_log_path(dir.path(), "nov", "2026-06-18", 2);
        let content = std::fs::read_to_string(&p).unwrap();
        // Two appends → two "Auto-Chronology Advance" headers.
        assert_eq!(content.matches("Auto-Chronology Advance").count(), 2);
        assert!(content.contains("Chapters seeded: 0"));
        assert!(content.contains("Trigger: test"));
    }

    #[test]
    fn now_utc_date_extracts_yyyy_mm_dd() {
        assert_eq!(
            now_utc_date("2026-06-18T10:00:00+00:00"),
            "2026-06-18"
        );
    }

    #[test]
    fn now_utc_date_falls_back_for_malformed() {
        assert_eq!(now_utc_date("nope"), "unknown-date");
    }
}
