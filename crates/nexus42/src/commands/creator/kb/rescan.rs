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
use nexus_kb::key_block::KeyBlockBody;
use nexus_kb::validation::ValidationMode;
use nexus_kb::{diff_and_apply, KbStore};
use nexus_local_db::kb_extract_job::{
    delete_pending_for_chapter, list_for_chapter, upsert_pending_candidate, UpsertOutcome,
};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_orchestration::quality_loop::{extract_candidates_from_text, KbCandidate};
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
                report.candidates_inserted.push(candidate.canonical_name_guess.clone());
            }
            UpsertOutcome::Updated(_) => {
                report.candidates_updated.push(candidate.canonical_name_guess.clone());
            }
            UpsertOutcome::Unchanged(_) => report.candidates_unchanged += 1,
        }
    }

    // Stale pending candidates (sourced from this chapter, no longer present).
    let new_names: std::collections::HashSet<&str> =
        candidates.iter().map(|c| c.canonical_name_guess.as_str()).collect();
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
    report.kb_updated = diff.updated.iter().map(|u| u.canonical_name.clone()).collect();
    report.kb_removed_advisory = diff.removed;
    Ok(())
}

/// Parse `<work_ref>/<chapter>` into `(work_ref, chapter)`.
fn parse_target(target: &str) -> Result<(String, i32)> {
    let (work_ref, chapter_str) = target
        .rsplit_once('/')
        .ok_or_else(|| {
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

/// Resolve `<work_ref>` → Work for the active creator.
///
/// Matches `work_ref`, `story_ref`, or `work_id` (most flexible for authors).
async fn resolve_work(
    pool: &SqlitePool,
    creator_id: &str,
    work_ref: &str,
) -> Result<Option<ResolvedWork>> {
    // SAFETY: SELECT against the known works table schema; runtime query
    // (consistent with works.rs using runtime queries for this table).
    let row: Option<(String, Option<String>)> =
        sqlx::query_as(
            "SELECT work_id, world_id FROM works \
             WHERE creator_id = ? AND (work_ref = ? OR story_ref = ? OR work_id = ?) LIMIT 1",
        )
        .bind(creator_id)
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
    let chapter_row =
        nexus_local_db::work_chapters::get_chapter(pool, work_id, chapter, 1)
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
        E::Validation(ve) => {
            CliError::Other(format!("ValidationError refreshing KB rows: {ve}"))
        }
        E::ValidationLegacy(msg) => {
            CliError::Other(format!("ValidationError refreshing KB rows: {msg}"))
        }
        other => CliError::Other(format!("Failed to refresh KB rows: {other}")),
    }
}

/// Print the rescan report (human-readable by default, JSON with `--json`).
fn print_report(report: &RescanReport, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(report).unwrap_or_else(|e| {
            format!("{{\"error\":\"failed to serialize report: {e}\"}}")
        }));
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
