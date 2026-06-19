//! `creator kb rescan` — refreshable KB scan (V1.50 T-B P2).
//!
//! Re-syncs `kb_extract_jobs` candidates and confirmed `KeyBlock` rows from a
//! chapter's current text. Plan: `2026-06-18-v1.50-kb-refreshable-scan`;
//! spec: `entity-scope-model.md` §5.5; compass §0.1 decision 7.
//!
//! # Flow
//!
//! 1. Parse `<work_ref>/<chapter>` → resolve the Work (by `work_ref` /
//!    `story_ref` / `work_id`) and its `world_id`.
//! 2. Author gate: world owner must match the active creator, else `403`
//!    `WORLD_KB_FORBIDDEN` (same code path as T-B P0/P1).
//! 3. Read the chapter's current prose from disk.
//! 4. Run the review-time heuristic
//!    (`nexus_orchestration::quality_loop::extract_candidates_from_text`).
//! 5. Idempotently upsert pending `kb_extract_jobs` candidates (keyed on
//!    `(creator, world, canonical_name)` per the V1.50 P1 DB uniqueness) and
//!    remove stale pending candidates sourced from this chapter.
//! 6. Refresh confirmed `KeyBlock` bodies via `nexus_kb::diff_and_apply` so KB
//!    rows reflect the current text (inserts/deletes stay with adopt/edit/delete
//!    per §5.5).
//!
//! R-V150KBED-02 self-corrects here: the rescan resolves `workspace_id` fresh
//! from `narrative_gateway` (the column was informational-only in T-B P1).
//!
//! `--dry-run` computes the candidate preview + `kb_key_blocks` diff without
//! writing.

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use nexus_kb::key_block::{KeyBlock, KeyBlockBody};
use nexus_kb::validation::ValidationMode;
use nexus_kb::{compute_kb_diff, diff_and_apply, KbStore};
use nexus_local_db::kb_extract_job::{
    delete_pending_for_chapter, list_for_chapter, upsert_pending_candidate, UpsertOutcome,
};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_orchestration::quality_loop::{
    aggregate_candidates_by_canonical_name, extract_candidates_from_text, AggregatedCandidate,
    KbCandidate,
};
use sqlx::SqlitePool;

/// Re-export the world-KB forbidden code so cross-author attempts share the
/// stable error string with `creator world kb` (T-B P0/P1).
pub use crate::commands::creator::world::kb::WORLD_KB_FORBIDDEN_CODE;

/// Default `blockType` guess used by the heuristic (mirrors
/// `quality_loop::DEFAULT_BLOCK_TYPE_GUESS`, kept private there).
const DEFAULT_BLOCK_TYPE_GUESS: &str = "character";

/// Report produced by a rescan (serialized for `--json`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct RescanReport {
    pub work_ref: String,
    pub chapter: i32,
    pub world_id: String,
    pub dry_run: bool,
    /// Newly-inserted pending candidate names.
    pub candidates_inserted: Vec<String>,
    /// Pending candidate names whose payload was refreshed.
    pub candidates_updated: Vec<String>,
    /// Stale pending candidates removed (name vanished from the chapter).
    pub candidates_removed: Vec<String>,
    /// Pending candidates whose payload was unchanged.
    pub candidates_unchanged: usize,
    /// Names extracted but with no active `KeyBlock` (advisory; adopt to promote).
    pub kb_inserted_advisory: Vec<String>,
    /// Active `KeyBlock`s whose body was refreshed.
    pub kb_updated: Vec<String>,
    /// Active `KeyBlock`s whose name vanished from extraction (advisory).
    pub kb_removed_advisory: Vec<String>,
}

impl RescanReport {
    /// Returns `true` when the rescan changed (or would change) nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.candidates_inserted.is_empty()
            && self.candidates_updated.is_empty()
            && self.candidates_removed.is_empty()
            && self.kb_updated.is_empty()
    }
}

/// Cross-chapter reuse entry for the work-scoped rescan report.
///
/// One per aggregate. Surfaced in `--dry-run` output so the author can see, per
/// canonical entity, which chapters referenced it and whether an active
/// `KeyBlock` already exists (entity-scope-model §5.5.1).
#[derive(Debug, Clone, serde::Serialize)]
pub struct CrossChapterReuse {
    /// Canonical entity name (first-seen case).
    pub canonical_name: String,
    /// Chapters that referenced this entity, ascending + deduped.
    pub source_chapters: Vec<i32>,
    /// `true` when an active `KeyBlock` already exists for this name in the
    /// work's world (advisory "no new candidate needed").
    pub existing_kb_row: bool,
}

/// Report produced by a work-scoped (`--work`) rescan (serialized for `--json`).
///
/// Separate from [`RescanReport`] (chapter-scoped) so the V1.50 shape is
/// unchanged (AC2 non-breaking extension).
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkRescanReport {
    pub work_ref: String,
    pub world_id: String,
    pub dry_run: bool,
    /// Chapters actually scanned (had a readable body file), ascending.
    pub chapters_scanned: Vec<i32>,
    /// Newly-inserted pending candidate names (one per aggregate with no
    /// existing pending/confirmed row).
    pub candidates_inserted: Vec<String>,
    /// Pending candidate names whose payload / `source_chapter_id` was refreshed.
    pub candidates_updated: Vec<String>,
    /// Pending candidates removed (name vanished from ALL chapters).
    pub candidates_removed: Vec<String>,
    /// Pending candidates whose payload was unchanged.
    pub candidates_unchanged: usize,
    /// Cross-chapter reuse summary, one entry per aggregate.
    pub cross_chapter_reuse: Vec<CrossChapterReuse>,
    /// Names extracted but with no active `KeyBlock` (advisory; adopt to promote).
    pub kb_inserted_advisory: Vec<String>,
    /// Active `KeyBlock`s whose body was refreshed.
    pub kb_updated: Vec<String>,
    /// Active `KeyBlock`s whose name vanished from extraction (advisory).
    pub kb_removed_advisory: Vec<String>,
}

impl WorkRescanReport {
    /// Returns `true` when the work rescan changed (or would change) nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.candidates_inserted.is_empty()
            && self.candidates_updated.is_empty()
            && self.candidates_removed.is_empty()
            && self.kb_updated.is_empty()
    }
}

/// `creator kb rescan <work_ref>/<chapter>` CLI entrypoint.
///
/// Resolves the active creator, workspace pool, and workspace dir from
/// `CliConfig`, then delegates to [`kb_rescan_hermetic`].
///
/// # Errors
///
/// Returns [`CliError::CreatorNotSelected`] if no active creator is set, and
/// other [`CliError`] variants for DB init or hermetic-logic failures.
// CLI entry-point runs on a single-threaded tokio runtime — Send not required.
#[allow(clippy::future_not_send)]
pub async fn kb_rescan(config: &CliConfig, target: &str, dry_run: bool, json: bool) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?
        .to_string();
    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;
    let workspace_dir = crate::config::find_workspace_root();
    let report = kb_rescan_hermetic(
        &pool,
        &creator_id,
        workspace_dir.as_deref(),
        target,
        dry_run,
    )
    .await?;
    print_report(&report, json);
    Ok(())
}

/// Hermetic rescan logic (testable against a fresh temp DB + temp workspace).
///
/// Takes an explicit pool, creator, and optional workspace dir so integration
/// tests can drive it without `$HOME` or a daemon. `workspace_dir` being
/// `None` surfaces a clean error (the heuristic needs chapter prose on disk).
///
/// # Errors
///
/// Returns [`CliError::Other`] for a malformed `<work_ref>/<chapter>` target,
/// a missing work/chapter/body file, a work without a bound `world_id`, or DB
/// / heuristic / KB-refresh failures. Returns [`CliError::Api`] with status
/// `403` (`WORLD_KB_FORBIDDEN`) when the active creator does not own the work's
/// world (AC4).
// CLI helper — single-threaded tokio; Send not required.
#[allow(clippy::future_not_send)]
pub async fn kb_rescan_hermetic(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_dir: Option<&std::path::Path>,
    target: &str,
    dry_run: bool,
) -> Result<RescanReport> {
    let (work_ref, chapter) = parse_target(target)?;
    if chapter < 1 {
        return Err(CliError::Other(format!(
            "Chapter number must be >= 1 (got {chapter})"
        )));
    }

    let work = resolve_work(pool, creator_id, &work_ref)
        .await?
        .ok_or_else(|| {
            CliError::Other(format!(
                "Work '{work_ref}' not found for creator '{creator_id}'. \
                 Usage: creator kb rescan <work_ref>/<chapter>"
            ))
        })?;
    let world_id = work.world_id.ok_or_else(|| {
        CliError::Other(format!(
            "Work '{work_ref}' has no world_id; cannot rescan KB. \
             Bind the work to a world first."
        ))
    })?;

    // Author gate: world owner must match the active creator (AC4).
    require_world_owner(pool, &world_id, creator_id).await?;

    let ws_dir = workspace_dir.ok_or_else(|| {
        CliError::Other(
            "No workspace root bound (cannot read chapter prose). \
             Run from a workspace directory or bind a workspace."
                .to_string(),
        )
    })?;

    // Read the chapter's current prose.
    let prose = read_chapter_prose(pool, &work.work_id, chapter, ws_dir).await?;

    // Fresh workspace_id (R-V150KBED-02 self-corrects here).
    let workspace_id = resolve_workspace_id(pool, creator_id).await;

    // Heuristic extraction (review-time, no LLM).
    let candidates = extract_candidates_from_text(&prose);

    // Diff baseline: pending candidates currently sourced from this chapter.
    let old_for_chapter = list_for_chapter(pool, &work.work_id, i64::from(chapter))
        .await
        .map_err(|e| CliError::Other(format!("Failed to list chapter candidates: {e}")))?;

    let mut report = RescanReport {
        work_ref,
        chapter,
        world_id: world_id.clone(),
        dry_run,
        candidates_inserted: Vec::new(),
        candidates_updated: Vec::new(),
        candidates_removed: Vec::new(),
        candidates_unchanged: 0,
        kb_inserted_advisory: Vec::new(),
        kb_updated: Vec::new(),
        kb_removed_advisory: Vec::new(),
    };

    sync_candidates(
        &mut report,
        pool,
        creator_id,
        &workspace_id,
        &work.work_id,
        &world_id,
        chapter,
        &candidates,
        &old_for_chapter,
        dry_run,
    )
    .await?;

    sync_kb_rows(&mut report, pool, &world_id, &candidates, dry_run).await?;

    Ok(report)
}

// ═══════════════════════════════════════════════════════════════════════
// V1.51 T-A P1 — Work-scoped (`--work`) cross-chapter reconciliation
// (closes R-V150KBED-08; spec: world-kb-runtime-architecture.md §5.5.1)
// ═══════════════════════════════════════════════════════════════════════

/// `creator kb rescan --work <work_ref>` CLI entrypoint.
///
/// Resolves the active creator, workspace pool, and workspace dir from
/// `CliConfig`, then delegates to [`kb_rescan_work_hermetic`].
///
/// # Errors
///
/// Returns [`CliError::CreatorNotSelected`] if no active creator is set, and
/// other [`CliError`] variants for DB init or hermetic-logic failures.
// CLI entry-point runs on a single-threaded tokio runtime — Send not required.
#[allow(clippy::future_not_send)]
pub async fn kb_rescan_work(
    config: &CliConfig,
    work_ref: &str,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?
        .to_string();
    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;
    let workspace_dir = crate::config::find_workspace_root();
    let report = kb_rescan_work_hermetic(
        &pool,
        &creator_id,
        workspace_dir.as_deref(),
        work_ref,
        dry_run,
    )
    .await?;
    print_work_report(&report, json);
    Ok(())
}

/// Hermetic work-scoped rescan logic (testable against a fresh temp DB + temp
/// workspace).
///
/// Iterates all chapters in `Works/<work_ref>/Stories/` (via the
/// `work_chapters` table), runs the review-time heuristic per chapter,
/// aggregates candidates by `canonical_name` across chapters, and reconciles
/// so a recurring entity collapses to a single `pending` candidate carrying
/// cross-chapter provenance. The non-dry path acquires the T-B P0 advisory
/// lock `Works/<work_ref>/.lock` before the DB upsert; the dry path is
/// read-only and acquires no lock.
///
/// # Errors
///
/// Returns [`CliError::Other`] for a missing work / worldless work / chapter
/// read failure, [`CliError::Api`] `403` (`WORLD_KB_FORBIDDEN`) on cross-author
/// attempts, [`CliError::Locked`] on advisory-lock contention (exit 75), and
/// [`CliError::LockIo`] on lock I/O failure (exit 78).
// CLI helper — single-threaded tokio; Send not required.
#[allow(clippy::future_not_send)]
pub async fn kb_rescan_work_hermetic(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_dir: Option<&std::path::Path>,
    work_ref: &str,
    dry_run: bool,
) -> Result<WorkRescanReport> {
    let work = resolve_work(pool, creator_id, work_ref)
        .await?
        .ok_or_else(|| {
            CliError::Other(format!(
                "Work '{work_ref}' not found for creator '{creator_id}'. \
                 Usage: creator kb rescan --work <work_ref>"
            ))
        })?;
    let world_id = work.world_id.clone().ok_or_else(|| {
        CliError::Other(format!(
            "Work '{work_ref}' has no world_id; cannot rescan KB. \
             Bind the work to a world first."
        ))
    })?;

    // Author gate: world owner must match the active creator (AC4 / §5.5.3).
    require_world_owner(pool, &world_id, creator_id).await?;

    let ws_dir = workspace_dir.ok_or_else(|| {
        CliError::Other(
            "No workspace root bound (cannot read chapter prose). \
             Run from a workspace directory or bind a workspace."
                .to_string(),
        )
    })?;

    // Enumerate chapters and extract per-chapter candidates (heuristic).
    let (chapters_scanned, per_chapter) = extract_per_chapter(pool, &work.work_id, ws_dir).await?;

    // Pure cross-chapter aggregation (group by canonical_name).
    let aggregates = aggregate_candidates_by_canonical_name(&per_chapter);

    // Existing active KeyBlock names (for the reuse summary + KB refresh).
    let store = SqliteKbStore::with_validation_mode(pool.clone(), ValidationMode::Novel);
    let old_kb_rows = store
        .list_by_world(&world_id)
        .await
        .map_err(|e| CliError::Other(format!("Failed to list world KeyBlocks: {e}")))?;
    // Mirrors the `kb_key_blocks` partial unique index
    // `WHERE status NOT IN ('deleted', 'merged', 'deprecated')` (nexus-kb
    // extract_sync); replicated here because that helper is crate-private.
    let active_kb_names: std::collections::HashSet<String> = old_kb_rows
        .iter()
        .filter(|kb| !matches!(kb.status.as_str(), "deleted" | "merged" | "deprecated"))
        .map(|kb| kb.canonical_name.to_ascii_lowercase())
        .collect();

    // Build the cross-chapter reuse summary (always; surfaces in both dry + non-dry).
    let cross_chapter_reuse: Vec<CrossChapterReuse> = aggregates
        .iter()
        .map(|a| CrossChapterReuse {
            canonical_name: a.canonical_name.clone(),
            source_chapters: a.source_chapters.clone(),
            existing_kb_row: active_kb_names.contains(&a.canonical_name.to_ascii_lowercase()),
        })
        .collect();

    // Existing pending/confirmed candidates for this work (for stale cleanup +
    // upsert classification). Loaded once across the whole work.
    let existing_for_work =
        nexus_local_db::kb_extract_job::list_pending_for_world(pool, &world_id, None)
            .await
            .map_err(|e| CliError::Other(format!("Failed to list work candidates: {e}")))?;

    let mut report = WorkRescanReport {
        work_ref: work_ref.to_string(),
        world_id: world_id.clone(),
        dry_run,
        chapters_scanned,
        candidates_inserted: Vec::new(),
        candidates_updated: Vec::new(),
        candidates_removed: Vec::new(),
        candidates_unchanged: 0,
        cross_chapter_reuse,
        kb_inserted_advisory: Vec::new(),
        kb_updated: Vec::new(),
        kb_removed_advisory: Vec::new(),
    };

    // Non-dry path: acquire advisory lock before any DB upsert (T-B P0).
    // Dry path is read-only → no lock. Guard dropped at end of scope.
    let work_dir = ws_dir.join("Works").join(work_ref);
    let _file_lock = if dry_run || !work_dir.exists() {
        None
    } else {
        Some(acquire_work_lock(&work_dir)?)
    };

    // Upsert one row per aggregate (DB uniqueness collapses same-name rows).
    sync_work_candidates(
        &mut report,
        pool,
        creator_id,
        &work.work_id,
        &world_id,
        &aggregates,
        &existing_for_work,
        dry_run,
    )
    .await?;

    // Refresh confirmed KeyBlock bodies (same §5.5 invariant as chapter-scope).
    // Non-dry path applies via diff_and_apply; dry path computes only.
    sync_work_kb_rows(
        &mut report,
        &store,
        &world_id,
        &old_kb_rows,
        &aggregates,
        dry_run,
    )
    .await?;

    Ok(report)
}

/// Enumerate a work's chapters, read each chapter body from disk, and run the
/// heuristic extractor. Returns `(chapters_scanned, per_chapter candidates)`,
/// ordered by chapter number. Chapters without a `body_path` are skipped.
// CLI helper — single-threaded tokio; Send not required.
#[allow(clippy::future_not_send)]
async fn extract_per_chapter(
    pool: &SqlitePool,
    work_id: &str,
    ws_dir: &std::path::Path,
) -> Result<(Vec<i32>, Vec<(i32, Vec<KbCandidate>)>)> {
    let chapters = nexus_local_db::work_chapters::list_chapters(pool, work_id)
        .await
        .map_err(|e| CliError::Other(format!("Failed to list chapters: {e}")))?;
    let mut per_chapter: Vec<(i32, Vec<KbCandidate>)> = Vec::new();
    let mut chapters_scanned: Vec<i32> = Vec::new();
    for ch in &chapters {
        let Some(body_rel) = ch.body_path.as_deref() else {
            continue;
        };
        let body_path = ws_dir.join(body_rel);
        let prose = std::fs::read_to_string(&body_path).map_err(|e| {
            CliError::Other(format!(
                "Failed to read chapter {} body {}: {e}",
                ch.chapter,
                body_path.display()
            ))
        })?;
        let candidates = extract_candidates_from_text(&prose);
        chapters_scanned.push(ch.chapter);
        per_chapter.push((ch.chapter, candidates));
    }
    chapters_scanned.sort_unstable();
    Ok((chapters_scanned, per_chapter))
}

/// Acquire the T-B P0 advisory file lock for a work-scoped rescan, mapping
/// `FileLockError` → the dual exit-code contract (Locked → 75, Io → 78).
fn acquire_work_lock(
    work_dir: &std::path::Path,
) -> Result<nexus_local_db::file_lock::FileLockGuard> {
    match nexus_local_db::file_lock::try_acquire(work_dir, "cli:kb-rescan-work") {
        Ok(guard) => Ok(guard),
        Err(nexus_local_db::file_lock::FileLockError::Locked(locked)) => Err(CliError::Locked {
            holder_pid: locked.holder_pid,
            holder_name: locked.holder_name,
            stale: locked.stale,
        }),
        Err(nexus_local_db::file_lock::FileLockError::Io(e)) => Err(CliError::LockIo(e)),
    }
}

/// Upsert one row per cross-chapter aggregate + remove stale pending
/// candidates whose name vanished from ALL chapters.
// CLI helper — single-threaded tokio; Send not required. Argument count
// mirrors the chapter-scoped `sync_candidates` DAO-shaped upsert signature;
// grouping into a struct would add boilerplate for a single-use private helper.
#[allow(clippy::future_not_send)]
#[allow(clippy::too_many_arguments)]
async fn sync_work_candidates(
    report: &mut WorkRescanReport,
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    world_id: &str,
    aggregates: &[nexus_orchestration::quality_loop::AggregatedCandidate],
    existing_for_work: &[nexus_local_db::kb_extract_job::KbExtractPromotion],
    dry_run: bool,
) -> Result<()> {
    let workspace_id = resolve_workspace_id(pool, creator_id).await;

    for agg in aggregates {
        let block_type = if agg.block_type.is_empty() {
            DEFAULT_BLOCK_TYPE_GUESS
        } else {
            agg.block_type.as_str()
        };
        // TODO(T-B P1): swap this non-versioned upsert_pending_candidate call
        // for the versioned CAS path (kb_extract_jobs.version column +
        // cas_update helper) once T-B P1 ships. The advisory lock acquired
        // above is the cross-process guard; CAS will be the per-row optimistic
        // guard. Single call-site swap — see world-kb-runtime-architecture.md
        // §5.5.1 "T-B P1 CAS hook".
        let outcome = if dry_run {
            preview_work_candidate_outcome(existing_for_work, agg)
        } else {
            upsert_pending_candidate(
                pool,
                creator_id,
                &workspace_id,
                world_id,
                Some(work_id),
                agg.source_chapters.first().map(|c| i64::from(*c)),
                block_type,
                &agg.canonical_name,
                &agg.proposed_payload,
            )
            .await
            .map_err(|e| CliError::Other(format!("Failed to upsert candidate: {e}")))?
        };
        match outcome {
            UpsertOutcome::Inserted(_) => {
                report.candidates_inserted.push(agg.canonical_name.clone());
            }
            UpsertOutcome::Updated(_) => {
                report.candidates_updated.push(agg.canonical_name.clone());
            }
            UpsertOutcome::Unchanged(_) => report.candidates_unchanged += 1,
        }
    }

    // Stale cleanup across the WHOLE work: pending candidates whose name no
    // longer appears in any chapter's aggregate.
    let new_names: std::collections::HashSet<String> = aggregates
        .iter()
        .map(|a| a.canonical_name.to_ascii_lowercase())
        .collect();
    for old in existing_for_work {
        if old.promotion_status != "pending" {
            continue;
        }
        let Some(old_name) = old.canonical_name_guess.as_deref() else {
            continue;
        };
        if new_names.contains(&old_name.to_ascii_lowercase()) {
            continue;
        }
        let removed = if dry_run {
            true
        } else {
            // Delete the stale pending row sourced from this work (any chapter).
            delete_pending_for_chapter_work(pool, work_id, old_name)
                .await
                .map_err(|e| CliError::Other(format!("Failed to delete stale candidate: {e}")))?
        };
        if removed {
            report.candidates_removed.push(old_name.to_string());
        }
    }
    Ok(())
}

/// Preview a work-scoped upsert outcome without writing (dry-run).
fn preview_work_candidate_outcome(
    existing_for_work: &[nexus_local_db::kb_extract_job::KbExtractPromotion],
    agg: &nexus_orchestration::quality_loop::AggregatedCandidate,
) -> UpsertOutcome {
    let existing = existing_for_work.iter().find(|row| {
        row.canonical_name_guess.as_deref() == Some(agg.canonical_name.as_str())
            && matches!(row.promotion_status.as_str(), "pending" | "confirmed")
    });
    let Some(row) = existing else {
        return UpsertOutcome::Inserted(String::new());
    };
    if row.promotion_status == "confirmed" {
        return UpsertOutcome::Unchanged(String::new());
    }
    let same_payload = row.proposed_payload.as_deref() == Some(agg.proposed_payload.as_str());
    let same_chapter = row.source_chapter_id == agg.source_chapters.first().copied().map(i64::from);
    if same_payload && same_chapter {
        UpsertOutcome::Unchanged(String::new())
    } else {
        UpsertOutcome::Updated(String::new())
    }
}

/// Delete a stale `pending` candidate sourced from anywhere in the work
/// (cross-chapter cleanup). Unlike the chapter-scoped
/// [`delete_pending_for_chapter`], this matches on `work_id` +
/// `canonical_name` regardless of `source_chapter_id`.
///
/// # Errors
///
/// Returns `sqlx::Error` on database failure.
async fn delete_pending_for_chapter_work(
    pool: &SqlitePool,
    work_id: &str,
    canonical_name_guess: &str,
) -> std::result::Result<bool, sqlx::Error> {
    // SAFETY: static DELETE with bind params; scoped to pending rows only.
    let result = sqlx::query(
        "DELETE FROM kb_extract_jobs \
         WHERE work_id = ? AND canonical_name_guess = ? \
         AND promotion_status = 'pending'",
    )
    .bind(work_id)
    .bind(canonical_name_guess)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Refresh confirmed `KeyBlock` bodies from the cross-chapter aggregates
/// (same §5.5 invariant as the chapter-scoped path; inserts/deletes stay with
/// adopt/edit/delete). Pure half (`compute_kb_diff`) in dry mode; non-dry
/// applies via [`nexus_kb::diff_and_apply`].
// CLI helper — single-threaded tokio; Send not required.
#[allow(clippy::future_not_send)]
async fn sync_work_kb_rows(
    report: &mut WorkRescanReport,
    store: &SqliteKbStore,
    world_id: &str,
    old_kb_rows: &[KeyBlock],
    aggregates: &[AggregatedCandidate],
    dry_run: bool,
) -> Result<()> {
    // Parse each aggregate's payload into a KeyBlockBody for the delta.
    let mut new_bodies: Vec<(String, KeyBlockBody)> = Vec::with_capacity(aggregates.len());
    for agg in aggregates {
        let body: KeyBlockBody = serde_json::from_str(&agg.proposed_payload).map_err(|e| {
            CliError::Other(format!(
                "Aggregate produced invalid proposed_payload for '{}': {e}",
                agg.canonical_name
            ))
        })?;
        new_bodies.push((agg.canonical_name.clone(), body));
    }

    let diff = if dry_run {
        compute_kb_diff(old_kb_rows, &new_bodies)
    } else {
        diff_and_apply(store, world_id, old_kb_rows, &new_bodies)
            .await
            .map_err(map_kb_sync_error)?
    };
    report.kb_inserted_advisory = diff.inserted;
    report.kb_updated = diff
        .updated
        .iter()
        .map(|u| u.canonical_name.clone())
        .collect();
    report.kb_removed_advisory = diff.removed;
    Ok(())
}

/// Print the work-scoped rescan report (human-readable by default, JSON with `--json`).
fn print_work_report(report: &WorkRescanReport, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).unwrap_or_else(|e| {
                format!("{{\"error\":\"failed to serialize report: {e}\"}}")
            })
        );
        return;
    }

    let mode = if report.dry_run { "DRY RUN — " } else { "" };
    println!(
        "{mode}Rescan --work {}: world {} ({} chapters: {})",
        report.work_ref,
        report.world_id,
        report.chapters_scanned.len(),
        report
            .chapters_scanned
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );

    // Cross-chapter reuse summary (AC3).
    if !report.cross_chapter_reuse.is_empty() {
        println!("  Cross-chapter reuse:");
        for r in &report.cross_chapter_reuse {
            let chs = r
                .source_chapters
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let kb_note = if r.existing_kb_row {
                "existing KB row found → no new candidate"
            } else {
                "no existing KB row → new pending candidate"
            };
            println!(
                "    Entity '{}' referenced in chapters {}; {}",
                r.canonical_name, chs, kb_note
            );
        }
    }

    println!(
        "  Candidates: {} inserted, {} updated, {} removed, {} unchanged",
        report.candidates_inserted.len(),
        report.candidates_updated.len(),
        report.candidates_removed.len(),
        report.candidates_unchanged
    );
    for n in &report.candidates_inserted {
        println!("    + {n}");
    }
    for n in &report.candidates_updated {
        println!("    ~ {n}");
    }
    for n in &report.candidates_removed {
        println!("    - {n}");
    }

    println!(
        "  KB rows: {} refreshed, {} new (adopt to promote), {} vanished (review via edit/delete)",
        report.kb_updated.len(),
        report.kb_inserted_advisory.len(),
        report.kb_removed_advisory.len()
    );

    if report.is_empty() {
        println!("  (no changes)");
    }
}

/// Upsert freshly-extracted candidates and remove stale pending ones.
///
/// Fills `report.candidates_*`. In `dry_run` mode the classification is a
/// read-only preview against the chapter's existing rows; otherwise the DB
/// upsert + delete are applied.
// CLI helper — single-threaded tokio; Send not required. Argument count
// mirrors the DAO-shaped upsert signature; grouping into a struct would add
// boilerplate for a single-use private helper.
#[allow(clippy::future_not_send)]
#[allow(clippy::too_many_arguments)]
async fn sync_candidates(
    report: &mut RescanReport,
    pool: &SqlitePool,
    creator_id: &str,
    workspace_id: &str,
    work_id: &str,
    world_id: &str,
    chapter: i32,
    candidates: &[KbCandidate],
    old_for_chapter: &[nexus_local_db::kb_extract_job::KbExtractPromotion],
    dry_run: bool,
) -> Result<()> {
    let chapter_i64 = i64::from(chapter);

    for candidate in candidates {
        let outcome = if dry_run {
            preview_candidate_outcome(old_for_chapter, candidate, chapter)
        } else {
            upsert_pending_candidate(
                pool,
                creator_id,
                workspace_id,
                world_id,
                Some(work_id),
                Some(chapter_i64),
                DEFAULT_BLOCK_TYPE_GUESS,
                &candidate.canonical_name_guess,
                &candidate.proposed_payload,
            )
            .await
            .map_err(|e| CliError::Other(format!("Failed to upsert candidate: {e}")))?
        };
        match outcome {
            UpsertOutcome::Inserted(_) => {
                report
                    .candidates_inserted
                    .push(candidate.canonical_name_guess.clone());
            }
            UpsertOutcome::Updated(_) => {
                report
                    .candidates_updated
                    .push(candidate.canonical_name_guess.clone());
            }
            UpsertOutcome::Unchanged(_) => report.candidates_unchanged += 1,
        }
    }

    // Stale pending candidates (sourced from this chapter, no longer present).
    let new_names: std::collections::HashSet<&str> = candidates
        .iter()
        .map(|c| c.canonical_name_guess.as_str())
        .collect();
    for old in old_for_chapter {
        if old.promotion_status != "pending" {
            continue;
        }
        let Some(old_name) = &old.canonical_name_guess else {
            continue;
        };
        if new_names.contains(old_name.as_str()) {
            continue;
        }
        let removed = if dry_run {
            true
        } else {
            delete_pending_for_chapter(pool, work_id, chapter_i64, old_name)
                .await
                .map_err(|e| CliError::Other(format!("Failed to delete stale candidate: {e}")))?
        };
        if removed {
            report.candidates_removed.push(old_name.clone());
        }
    }
    Ok(())
}

/// Refresh confirmed `KeyBlock` bodies via the nexus-kb delta.
///
/// Fills `report.kb_*`. In `dry_run` mode the diff is computed (pure) but not
/// applied; otherwise [`nexus_kb::diff_and_apply`] refreshes matching rows.
// CLI helper — single-threaded tokio; Send not required.
#[allow(clippy::future_not_send)]
async fn sync_kb_rows(
    report: &mut RescanReport,
    pool: &SqlitePool,
    world_id: &str,
    candidates: &[KbCandidate],
    dry_run: bool,
) -> Result<()> {
    let new_bodies = parse_candidate_bodies(candidates)?;
    let store = SqliteKbStore::with_validation_mode(pool.clone(), ValidationMode::Novel);
    let old_kb_rows = store
        .list_by_world(world_id)
        .await
        .map_err(|e| CliError::Other(format!("Failed to list world KeyBlocks: {e}")))?;

    let diff = if dry_run {
        nexus_kb::compute_kb_diff(&old_kb_rows, &new_bodies)
    } else {
        diff_and_apply(&store, world_id, &old_kb_rows, &new_bodies)
            .await
            .map_err(map_kb_sync_error)?
    };
    report.kb_inserted_advisory = diff.inserted;
    report.kb_updated = diff
        .updated
        .iter()
        .map(|u| u.canonical_name.clone())
        .collect();
    report.kb_removed_advisory = diff.removed;
    Ok(())
}

/// Parse `<work_ref>/<chapter>` into `(work_ref, chapter)`.
fn parse_target(target: &str) -> Result<(String, i32)> {
    let (work_ref, chapter_str) = target.rsplit_once('/').ok_or_else(|| {
        CliError::Other(format!(
            "Invalid target '{target}'. \
                 Expected <work_ref>/<chapter> (e.g. my-novel/05)."
        ))
    })?;
    if work_ref.is_empty() {
        return Err(CliError::Other(format!(
            "Invalid target '{target}': work_ref is empty."
        )));
    }
    let chapter: i32 = chapter_str.parse().map_err(|_| {
        CliError::Other(format!(
            "Invalid target '{target}': chapter '{chapter_str}' is not a number."
        ))
    })?;
    Ok((work_ref.to_string(), chapter))
}

/// Minimal Work row needed for a rescan.
struct ResolvedWork {
    work_id: String,
    world_id: Option<String>,
}

/// Resolve `<work_ref>` → Work.
///
/// Matches `work_ref`, `story_ref`, or `work_id` globally (the author typed a
/// valid ref). Authz is enforced later by [`require_world_owner`] on **world**
/// ownership (entity-scope-model §5.5.3 / T-B P0/P1), not at work resolution —
/// so a cross-author attempt resolves the work and then surfaces `403` at the
/// world gate rather than a misleading "not found".
async fn resolve_work(
    pool: &SqlitePool,
    _creator_id: &str,
    work_ref: &str,
) -> Result<Option<ResolvedWork>> {
    // SAFETY: SELECT against the known works table schema; runtime query
    // (consistent with works.rs using runtime queries for this table).
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT work_id, world_id FROM works \
             WHERE work_ref = ? OR story_ref = ? OR work_id = ? LIMIT 1",
    )
    .bind(work_ref)
    .bind(work_ref)
    .bind(work_ref)
    .fetch_optional(pool)
    .await
    .map_err(|e| CliError::Other(format!("Failed to resolve work: {e}")))?;
    Ok(row.map(|(work_id, world_id)| ResolvedWork { work_id, world_id }))
}

/// Read the chapter body prose from the workspace filesystem.
async fn read_chapter_prose(
    pool: &SqlitePool,
    work_id: &str,
    chapter: i32,
    ws_dir: &std::path::Path,
) -> Result<String> {
    let chapter_row = nexus_local_db::work_chapters::get_chapter(pool, work_id, chapter, 1)
        .await
        .map_err(|e| CliError::Other(format!("Failed to load chapter row: {e}")))?
        .ok_or_else(|| {
            CliError::Other(format!(
                "Chapter {chapter} for work '{work_id}' is not seeded. \
                     Expected a work_chapters row for volume 1."
            ))
        })?;
    let body_path_rel = chapter_row.body_path.ok_or_else(|| {
        CliError::Other(format!(
            "Chapter {chapter} has no body_path; nothing to scan."
        ))
    })?;
    let body_path = ws_dir.join(body_path_rel);
    std::fs::read_to_string(&body_path).map_err(|e| {
        CliError::Other(format!(
            "Failed to read chapter body {}: {e}",
            body_path.display()
        ))
    })
}

/// Author identity gate. Reads `narrative_worlds.owner_creator_id` and
/// requires it to match `creator_id`. Returns `403 WORLD_KB_FORBIDDEN` on
/// mismatch — same code path as `creator world kb` (T-B P0/P1).
async fn require_world_owner(pool: &SqlitePool, world_id: &str, creator_id: &str) -> Result<()> {
    // SAFETY: SELECT against the known narrative_worlds table schema.
    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?")
            .bind(world_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| CliError::Other(format!("Failed to query world owner: {e}")))?
            .flatten();
    match owner {
        None => Err(CliError::Other(format!(
            "World '{world_id}' not found. \
             List worlds with: nexus42 creator world list"
        ))),
        Some(owner_id) if owner_id == creator_id => Ok(()),
        Some(owner_id) => Err(CliError::Api {
            status: 403,
            message: format!(
                "{WORLD_KB_FORBIDDEN_CODE}: active creator '{creator_id}' does not own \
                 world '{world_id}' (owner: '{owner_id}'). \
                 Cross-author KB rescan is not permitted."
            ),
        }),
    }
}

/// Best-effort fresh `workspace_id` for the upserted candidate rows.
///
/// R-V150KBED-02: `narrative_gateway.workspace_id` may be stale; the column is
/// informational only. The rescan resolves it fresh here so newly-upserted
/// rows carry the current value. Falls back to `creator_id` when no workspace
/// is registered (the extraction logic keys off `world_id` + `work_id`).
async fn resolve_workspace_id(pool: &SqlitePool, creator_id: &str) -> String {
    // SAFETY: static scalar lookup against narrative_gateway.
    let ws: Option<String> = sqlx::query_scalar(
        "SELECT workspace_id FROM narrative_gateway WHERE creator_id = ? LIMIT 1",
    )
    .bind(creator_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    ws.unwrap_or_else(|| creator_id.to_string())
}

/// Preview an upsert outcome without writing (dry-run).
///
/// Classifies a candidate against the chapter's existing pending rows. This is
/// a faithful preview on a clean per-chapter basis; the real (non-dry) path
/// uses the DB upsert which also reconciles cross-chapter reuse.
fn preview_candidate_outcome(
    old_for_chapter: &[nexus_local_db::kb_extract_job::KbExtractPromotion],
    candidate: &KbCandidate,
    chapter: i32,
) -> UpsertOutcome {
    let existing = old_for_chapter.iter().find(|row| {
        row.canonical_name_guess.as_deref() == Some(&candidate.canonical_name_guess)
            && matches!(row.promotion_status.as_str(), "pending" | "confirmed")
    });
    let Some(row) = existing else {
        return UpsertOutcome::Inserted(String::new());
    };
    if row.promotion_status == "confirmed" {
        return UpsertOutcome::Unchanged(String::new());
    }
    let same_payload = row.proposed_payload.as_deref() == Some(&candidate.proposed_payload);
    let same_chapter = row.source_chapter_id == Some(i64::from(chapter));
    if same_payload && same_chapter {
        UpsertOutcome::Unchanged(String::new())
    } else {
        UpsertOutcome::Updated(String::new())
    }
}

/// Parse each candidate's `proposed_payload` JSON into a `KeyBlockBody` for
/// the nexus-kb delta.
fn parse_candidate_bodies(candidates: &[KbCandidate]) -> Result<Vec<(String, KeyBlockBody)>> {
    let mut out = Vec::with_capacity(candidates.len());
    for c in candidates {
        let body: KeyBlockBody = serde_json::from_str(&c.proposed_payload).map_err(|e| {
            CliError::Other(format!(
                "Heuristic produced invalid proposed_payload for '{}': {e}",
                c.canonical_name_guess
            ))
        })?;
        out.push((c.canonical_name_guess.clone(), body));
    }
    Ok(out)
}

/// Map a KB sync store error to a user-facing `CliError`.
fn map_kb_sync_error(e: nexus_kb::KbStoreError) -> CliError {
    use nexus_kb::KbStoreError as E;
    match e {
        E::Validation(ve) => CliError::Other(format!("ValidationError refreshing KB rows: {ve}")),
        E::ValidationLegacy(msg) => {
            CliError::Other(format!("ValidationError refreshing KB rows: {msg}"))
        }
        other => CliError::Other(format!("Failed to refresh KB rows: {other}")),
    }
}

/// Print the rescan report (human-readable by default, JSON with `--json`).
fn print_report(report: &RescanReport, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).unwrap_or_else(|e| {
                format!("{{\"error\":\"failed to serialize report: {e}\"}}")
            })
        );
        return;
    }

    let mode = if report.dry_run { "DRY RUN — " } else { "" };
    println!(
        "{mode}Rescan {} chapter {}: world {}",
        report.work_ref, report.chapter, report.world_id
    );

    println!(
        "  Candidates: {} inserted, {} updated, {} removed, {} unchanged",
        report.candidates_inserted.len(),
        report.candidates_updated.len(),
        report.candidates_removed.len(),
        report.candidates_unchanged
    );
    for n in &report.candidates_inserted {
        println!("    + {n}");
    }
    for n in &report.candidates_updated {
        println!("    ~ {n}");
    }
    for n in &report.candidates_removed {
        println!("    - {n}");
    }

    println!(
        "  KB rows: {} refreshed, {} new (adopt to promote), {} vanished (review via edit/delete)",
        report.kb_updated.len(),
        report.kb_inserted_advisory.len(),
        report.kb_removed_advisory.len()
    );
    for n in &report.kb_updated {
        println!("    ~ {n}");
    }

    if report.is_empty() {
        println!("  (no changes)");
    }
}
