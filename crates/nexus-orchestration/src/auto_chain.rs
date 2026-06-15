//! Auto-chain engine — `on_complete` handler for FL-E stage advancement and
//! chapter outer loop (V1.39 §5.4–5.6).
//!
//! When a schedule reaches a terminal state, this module determines the next
//! step for auto-chain-enabled Works:
//!
//! 1. **Stage advance**: after intake/research/review completes → enqueue next stage
//! 2. **Chapter loop**: after persist for chapter N → enqueue produce for chapter N+1
//!    (if chapters remain)
//! 3. **Work completion**: after persist for the last chapter → mark Work completed
//!
//! Checkpoint fields on the Work record track the continuation state:
//! - `auto_chain_enabled`: whether auto-chain is active (default true)
//! - `driver_schedule_id`: the currently-running FL-E driver schedule
//! - `auto_chain_interrupted`: set when driver is interrupted externally

use nexus_contracts::local::orchestration::{stage_index, FL_E_STAGES};
use nexus_contracts::local::schedule::http::AddScheduleRequest;
use nexus_local_db::findings::{self, ReviewVerdictFinding};
use nexus_local_db::novel_pool_entries;
use nexus_local_db::works::{self, WorkPatch, WorkRecord};
use sqlx::SqlitePool;

use crate::completion_lock::{self, CompletionLock};

/// R-V139P0-W-B: per-process monotonic counter for ACH schedule ID collision resistance.
static ACH_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// R-V147P0-05 (hotfix H-1): per-process monotonic counter for `RVM` schedule
/// ID collision resistance. The previous `enqueue_review_master_schedule` minted
/// `RVM<ts_ms>` which collided when the stale-findings watcher (or repeated
/// sweeps within one tick) enqueued two opt-in Works in the same millisecond.
/// Mirrors the `ACH_COUNTER` fix (R-V139P0-W-B) for the same class of bug.
static RVM_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

use crate::stage_gates::{self, WorkFields};

/// Result of an `on_schedule_complete` evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainAction {
    /// No further action needed (auto-chain disabled, work complete, or not an FL-E driver).
    NoAction,
    /// Advance to the next FL-E stage for the current chapter.
    AdvanceStage { work_id: String, next_stage: String },
    /// Start the produce stage for the next chapter (chapter outer loop).
    /// V1.42: includes volume for cross-volume chaining.
    NextChapter {
        work_id: String,
        next_chapter: i32,
        next_volume: i32,
    },
    /// The Work is complete — all chapters finalized.
    WorkComplete { work_id: String },
}

/// Error type for auto-chain operations.
#[derive(Debug, thiserror::Error)]
pub enum AutoChainError {
    /// Database operation failed.
    #[error("database error: {0}")]
    Database(#[from] nexus_local_db::LocalDbError),
    /// Work record not found.
    #[error("work not found: {0}")]
    WorkNotFound(String),
    /// Invalid state for auto-chain operation.
    #[error("invalid state: {0}")]
    InvalidState(String),
}

/// Look up the Work record associated with a completed schedule.
///
/// Matches by `driver_schedule_id` on the works table. Returns `None` if no
/// Work has this schedule as its driver (e.g., non-FL-E schedules).
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the database query fails.
pub async fn find_work_for_driver(
    pool: &SqlitePool,
    schedule_id: &str,
) -> Result<Option<WorkRecord>, AutoChainError> {
    // SAFETY: dynamic SQL — driver_schedule_id lookup is a simple equality filter.
    let row = sqlx::query(&format!(
        "SELECT {} FROM works WHERE driver_schedule_id = ? LIMIT 1",
        works::WORKS_COLUMNS
    ))
    .bind(schedule_id)
    .fetch_optional(pool)
    .await
    .map_err(nexus_local_db::LocalDbError::from)?;

    Ok(row.as_ref().map(works::row_to_work_record))
}

/// V1.47 P0 — Review-stage findings producer hook.
///
/// This is the **single code path** (spec §5.5.6 "Trigger paths — both
/// required") that persists ≥1 finding row when a `novel-chapter-review`
/// schedule reaches terminal status. It is called from the supervisor's
/// `on_schedule_terminal(Completed)` for **both** the auto-chain driver
/// schedule and on-demand `creator run novel-chapter-review <work_id>`
/// schedules, satisfying acceptance criteria #1 and #2 of the V1.47 P0 plan.
///
/// # Behavior
///
/// 1. Loads the schedule row to read `preset_id`, `work_id`, `creator_id`.
/// 2. Returns `Ok(0)` early when the preset is not `novel-chapter-review`
///    (no-op for non-review schedules).
/// 3. Loads the Work record for `chapter` context (`work.current_chapter`).
/// 4. Synthesizes ≥1 `ReviewVerdictFinding` and persists it via
///    [`findings::create_finding_from_review`]. The synthesized finding uses
///    safe defaults (`kind=craft`, `severity=info`, `target_executor=none`)
///    because the LLM judge output is not directly observable at the
///    supervisor layer in P0 (the agent writes a richer report under
///    `Works/<work_ref>/Logs/review/` as a side-effect; a follow-up slice
///    can parse that artifact for richer findings).
///
/// Spec §8.4 invariant: finding creation MUST NOT fork or cancel the active
/// FL-E driver schedule. This function performs only a DB INSERT and does
/// not touch `driver_schedule_id`.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the schedule/Work lookup or the
/// finding INSERT fails. Errors are logged by the caller and do **not**
/// block the supervisor terminal transition.
pub async fn persist_review_findings_for_schedule(
    pool: &SqlitePool,
    schedule_id: &str,
) -> Result<usize, AutoChainError> {
    const REVIEW_PRESET_ID: &str = "novel-chapter-review";

    // SAFETY: dynamic SQL — single-row schedule lookup by PK. `work_id` is
    // nullable (added in 202606080002_creator_schedules_work_id.sql), so we
    // cannot use `as "work_id!"` (NOT NULL assertion). `creator_id` and
    // `preset_id` are NOT NULL per the original 20260419 migration.
    let row = sqlx::query(
        "SELECT preset_id, work_id, creator_id
         FROM creator_schedules
         WHERE schedule_id = ?",
    )
    .bind(schedule_id)
    .fetch_optional(pool)
    .await
    .map_err(nexus_local_db::LocalDbError::from)?;

    let Some(row) = row else {
        // Schedule row missing — nothing to do (caller already updated status).
        tracing::debug!(
            schedule_id,
            "review-findings: schedule row not found; skipping"
        );
        return Ok(0);
    };

    // SAFETY: dynamic-SQL row → typed fields via ColumnIndex + TypeCheck.
    // Columns are positional/named; sqlx runtime decode is fine for this
    // nullable schema.
    let preset_id: String = sqlx::Row::try_get(&row, "preset_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode preset_id: {e}")))?;
    let creator_id: String = sqlx::Row::try_get(&row, "creator_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode creator_id: {e}")))?;
    let work_id: Option<String> = sqlx::Row::try_get(&row, "work_id")
        .map_err(|e| AutoChainError::InvalidState(format!("decode work_id: {e}")))?;

    if preset_id != REVIEW_PRESET_ID {
        // Not a review schedule — no-op.
        return Ok(0);
    }

    let Some(work_id) = work_id else {
        tracing::warn!(
            schedule_id,
            "review-findings: schedule has NULL work_id; skipping"
        );
        return Ok(0);
    };

    // Work row may be missing for malformed schedules; log + return.
    let work = match works::get_work(pool, &creator_id, &work_id).await {
        Ok(Some(w)) => w,
        Ok(None) => {
            tracing::warn!(
                schedule_id,
                work_id = %work_id,
                "review-findings: work not found; skipping"
            );
            return Ok(0);
        }
        Err(e) => return Err(AutoChainError::from(e)),
    };

    // Derive chapter context from Work's current_chapter (V1.38 §4.5.2).
    // `current_chapter` is 0 until first finalize; treat 0 as Work-level.
    let chapter: Option<i64> = if work.current_chapter > 0 {
        Some(i64::from(work.current_chapter))
    } else {
        None
    };

    // Synthesize the minimum viable review finding per spec §8.2.
    let chapter_ctx = chapter.map_or_else(|| "work-level".to_string(), |c| format!("chapter {c}"));
    let title = format!("Review pass completed ({chapter_ctx})");
    let work_ref_or_id = work.work_ref.as_deref().unwrap_or(&work_id);
    let description = format!(
        "Automated review pass for Work '{}' ({}) — {chapter_ctx}.\n\
         The full review report is written under Works/{}/Logs/review/.\n\
         \n\
         (V1.47 P0: synthesized finding — the LLM review output is not parsed\n\
         at the supervisor layer in this slice. A follow-up will parse the\n\
         structured review artifact for richer kind/severity/rule_suggestion.)",
        work.title, work_ref_or_id, work_ref_or_id,
    );

    let verdict = ReviewVerdictFinding {
        work_id: work_id.clone(),
        chapter,
        // Safe defaults per spec §8.2 + §2.1; the synthesized finding is
        // intentionally non-disruptive (`info` severity, `none` executor).
        severity: "info".to_string(),
        title,
        description,
        target_executor: "none".to_string(),
        creator_id,
        kind: "craft".to_string(),
        // Optional — no rule suggestion in the synthesized path.
        rule_suggestion: None,
        // V1.47 P0 fix (qc1 W-2): pass the originating schedule_id so the
        // INSERT is idempotent — a second terminal transition for the same
        // review schedule is a no-op (partial unique index
        // `findings_unique_review_per_chapter`).
        source_schedule_id: Some(schedule_id.to_string()),
    };

    match findings::create_finding_from_review(pool, &verdict).await {
        Ok(finding_id) => {
            tracing::info!(
                schedule_id,
                work_id = %work_id,
                finding_id = %finding_id,
                "review-findings: persisted finding for review pass"
            );
            Ok(1)
        }
        Err(e) => {
            // R-V139P1-W-6: log + propagate so the caller can record the
            // failure without blocking the terminal transition.
            tracing::warn!(
                schedule_id,
                work_id = %work_id,
                error = %e,
                "review-findings: failed to persist finding"
            );
            Err(AutoChainError::from(e))
        }
    }
}

/// Determine the next chain action after a schedule completes.
///
/// This is the core decision function of the auto-chain engine. It evaluates:
/// 1. Whether auto-chain is enabled for this Work
/// 2. The current FL-E stage and chapter state
/// 3. Whether more chapters remain
///
/// Returns the appropriate `ChainAction` to execute.
#[must_use]
pub fn evaluate_next_step(work: &WorkRecord) -> ChainAction {
    // If auto-chain is disabled, no automatic advancement
    if !work.auto_chain_enabled {
        return ChainAction::NoAction;
    }

    // If the auto-chain was interrupted, don't resume automatically
    if work.auto_chain_interrupted {
        return ChainAction::NoAction;
    }

    // If work is already completed, nothing to do
    if work.status == "completed" {
        return ChainAction::NoAction;
    }

    let current_stage = work.current_stage.as_str();
    let current_idx = stage_index(current_stage).unwrap_or(0);

    // After persist (last FL-E stage): check for chapter loop or work completion
    if current_stage == "persist" && work.stage_status == "complete" {
        return evaluate_after_persist(work);
    }

    // After any other stage completes: advance to the next stage
    if work.stage_status == "complete" && current_idx < FL_E_STAGES.len() - 1 {
        let next_idx = current_idx + 1;
        if let Some(&next_stage) = FL_E_STAGES.get(next_idx) {
            return ChainAction::AdvanceStage {
                work_id: work.work_id.clone(),
                next_stage: next_stage.to_string(),
            };
        }
    }

    // Intake stage with status "skipped" — advance to research
    if current_stage == "intake" && work.stage_status == "skipped" {
        return ChainAction::AdvanceStage {
            work_id: work.work_id.clone(),
            next_stage: "research".to_string(),
        };
    }

    ChainAction::NoAction
}

/// Evaluate what happens after the persist stage completes.
///
/// This handles the chapter outer loop:
/// - If more chapters remain → start produce for chapter N+1
/// - If all chapters done → mark work as completed
///
/// For single-volume Works (the common case), uses the flat `current_chapter`
/// comparison. For multi-volume Works, callers should use
/// [`evaluate_after_persist_volume_aware`] instead.
fn evaluate_after_persist(work: &WorkRecord) -> ChainAction {
    let total_chapters = work.total_planned_chapters.unwrap_or(0);
    let current_chapter = work.current_chapter;

    if total_chapters <= 0 {
        // No chapter tracking — single-pass work, mark complete
        return ChainAction::WorkComplete {
            work_id: work.work_id.clone(),
        };
    }

    // Check if there are more chapters to process
    // current_chapter is the latest finalized chapter number
    if current_chapter < total_chapters {
        let next_chapter = current_chapter + 1;
        ChainAction::NextChapter {
            work_id: work.work_id.clone(),
            next_chapter,
            next_volume: 1, // V1.42: single-volume path defaults to 1
        }
    } else {
        // All chapters finalized
        ChainAction::WorkComplete {
            work_id: work.work_id.clone(),
        }
    }
}

/// V1.42 volume-aware version of [`evaluate_after_persist`].
///
/// Queries the DB for the next non-finalized chapter across all volumes.
/// Falls back to the flat `evaluate_after_persist` logic if the volume-aware
/// query returns `None` (e.g. all chapters finalized).
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the DB query fails.
pub async fn evaluate_after_persist_volume_aware(
    pool: &SqlitePool,
    work: &WorkRecord,
) -> Result<ChainAction, AutoChainError> {
    let total_chapters = work.total_planned_chapters.unwrap_or(0);

    if total_chapters <= 0 {
        return Ok(ChainAction::WorkComplete {
            work_id: work.work_id.clone(),
        });
    }

    // Try volume-aware next chapter selection
    let next =
        nexus_local_db::work_chapters::next_chapter_volume_aware(pool, &work.work_id).await?;

    match next {
        Some((volume, chapter)) => Ok(ChainAction::NextChapter {
            work_id: work.work_id.clone(),
            next_chapter: chapter,
            next_volume: volume,
        }),
        None => Ok(ChainAction::WorkComplete {
            work_id: work.work_id.clone(),
        }),
    }
}

/// Build the schedule request for an auto-chain step (stage advance or next chapter).
///
/// Constructs a correctly-shaped `AddScheduleRequest` using the shared
/// [`stage_gates::build_schedule_for_stage`] facade.
///
/// V1.44 P2 (F-004): `volume` is threaded through to `WorkFields` so the
/// `novel-writing` preset input includes a `volume` template var for
/// cross-volume context preservation.
#[allow(clippy::missing_panics_doc)] // panic only on invalid stage names, which we validate
pub fn build_auto_chain_schedule(
    stage: &str,
    creator_id: &str,
    work: &WorkRecord,
    chapter: Option<i32>,
    volume: Option<i32>,
) -> Option<AddScheduleRequest> {
    let work_ref = work.work_ref.clone();
    let chapter_label = chapter.map(stage_gates::chapter_label);

    // Fix W-2: when the stage is produce (following research), include the
    // research artifacts directory in the preset input so produce can see
    // research-derived material (AC2, AC3).
    let research_artifacts_dir = if stage == "produce" {
        work.driver_schedule_id
            .as_ref()
            .map(|sid| format!(".nexus42/references/{sid}/"))
    } else {
        None
    };

    let fields = WorkFields {
        work_id: work.work_id.clone(),
        fl_e_stage: stage.to_string(),
        creative_brief: work.creative_brief.clone().unwrap_or_default(),
        inspiration_log: work.inspiration_log.clone(),
        work_ref,
        chapter,
        chapter_label,
        outline_path: None,
        body_path: None,
        slug: None,
        research_artifacts_dir,
        workspace_dir: None,
        world_kb_block: None,
        world_id: work.world_id.clone(),
        volume,
    };

    stage_gates::build_schedule_for_stage(stage, creator_id, &fields)
}

/// Update the Work checkpoint after an auto-chain step is enqueued.
///
/// Sets the new `driver_schedule_id` and resets `auto_chain_interrupted`.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the patch fails or the work is not found.
pub async fn update_checkpoint(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    new_stage: &str,
    driver_schedule_id: Option<&str>,
    chapter: Option<i32>,
) -> Result<WorkRecord, AutoChainError> {
    let now = chrono::Utc::now().to_rfc3339();

    let patch = WorkPatch {
        current_stage: Some(new_stage.to_string()),
        stage_status: Some("active".to_string()),
        driver_schedule_id: driver_schedule_id.map(|s| Some(s.to_string())),
        auto_chain_interrupted: Some(false),
        ..Default::default()
    };

    if let Some(ch) = chapter {
        // For chapter loop, the current_chapter update happens at finalize time
        // (via novel_chapter_transition). We don't advance it here.
        // But we need to set the stage to "produce" for the new chapter.
        let _ = ch; // chapter is used for the schedule input, not the patch
    }

    works::patch_work(pool, creator_id, work_id, &patch, &now)
        .await
        .map_err(AutoChainError::from)
}

/// Mark a Work as completed.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the patch fails or the work is not found.
pub async fn mark_work_completed(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<WorkRecord, AutoChainError> {
    let now = chrono::Utc::now().to_rfc3339();

    // Step 1: DB patch — status + novel_completion_status + completion_locked_at
    let patch = WorkPatch {
        status: Some("completed".to_string()),
        current_stage: Some("persist".to_string()),
        stage_status: Some("complete".to_string()),
        driver_schedule_id: Some(None), // clear driver
        auto_chain_interrupted: Some(false),
        novel_completion_status: Some(Some("finalize_complete".to_string())),
        completion_locked_at: Some(Some(now.clone())),
        ..Default::default()
    };

    let updated = works::patch_work(pool, creator_id, work_id, &patch, &now)
        .await
        .map_err(AutoChainError::from)?;

    // Step 1.5: Update pool entry to `completed` (DF-61 §5.4).
    // The pool row may not exist if the Work was created outside the
    // selection pool (e.g., `creator run start`).
    match novel_pool_entries::mark_pool_entry_completed_for_work(pool, creator_id, work_id).await {
        Ok(()) => {}
        Err(e) => {
            // Pool update failed — clear completion_locked_at so the
            // supervisor retries on the next tick (qc2 W-03, qc3 F-003).
            tracing::error!(
                target: "novel.completion",
                work_id = %work_id,
                creator_id = %creator_id,
                error = %e,
                "mark_work_completed: pool entry update FAILED — \
                 clearing completion_locked_at for supervisor retry"
            );
            let clear_lock = WorkPatch {
                completion_locked_at: Some(None),
                ..Default::default()
            };
            let retry_now = chrono::Utc::now().to_rfc3339();
            if let Err(clear_err) =
                works::patch_work(pool, creator_id, work_id, &clear_lock, &retry_now).await
            {
                tracing::error!(
                    target: "novel.completion",
                    work_id = %work_id,
                    error = %clear_err,
                    "mark_work_completed: failed to clear completion_locked_at after pool update failure"
                );
            }
        }
    }

    // Step 2: Write completion-lock file (best-effort; non-blocking for Work completion)
    if let Some(ref _work_ref) = updated.work_ref {
        let lock = CompletionLock {
            schema_version: 1,
            work_id: work_id.to_string(),
            locked_at: now.clone(),
            reason: "completion".to_string(),
        };
        // We don't have workspace_dir here — the caller (supervisor) should
        // write the lock file after calling this function if they have the path.
        // For now, we log an info-level note. The actual file I/O is done by
        // the supervisor or CLI layer that has access to the workspace dir.
        tracing::info!(
            target: "novel.completion",
            work_id = %work_id,
            creator_id = %creator_id,
            completion_locked_at = %now,
            work_ref = ?updated.work_ref,
            "mark_work_completed: DB columns set; completion-lock file \
             should be written by caller"
        );
        let _ = lock; // used by caller
    }

    Ok(updated)
}

/// Write the completion-lock file for a completed Work (DF-60 §3).
///
/// Call this after `mark_work_completed` succeeds, providing the workspace
/// directory and the Work record (for `work_ref`). This is separated from
/// `mark_work_completed` because the supervisor does not have access to the
/// workspace directory — the daemon layer calls this function.
///
/// # Errors
///
/// Returns `std::io::Error` if the file cannot be written.
pub fn write_completion_lock_for_work(
    workspace_dir: &std::path::Path,
    work: &WorkRecord,
    locked_at: &str,
) -> Result<(), std::io::Error> {
    let work_ref = work.work_ref.as_deref().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "work {} has no work_ref; cannot write completion-lock",
                work.work_id
            ),
        )
    })?;

    let lock = CompletionLock {
        schema_version: 1,
        work_id: work.work_id.clone(),
        locked_at: locked_at.to_string(),
        reason: "completion".to_string(),
    };

    completion_lock::write_completion_lock(workspace_dir, work_ref, &lock)
}

/// Clear the `driver_schedule_id` on a Work (e.g., when schedule completes or is cancelled).
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the patch fails or the work is not found.
pub async fn clear_driver(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<(), AutoChainError> {
    let now = chrono::Utc::now().to_rfc3339();

    let patch = WorkPatch {
        driver_schedule_id: Some(None),
        ..Default::default()
    };

    works::patch_work(pool, creator_id, work_id, &patch, &now)
        .await
        .map_err(AutoChainError::from)?;

    Ok(())
}

/// Set the `driver_schedule_id` on a Work and mark the stage as active.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the patch fails or the work is not found.
pub async fn set_driver(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    schedule_id: &str,
    stage: &str,
) -> Result<(), AutoChainError> {
    let now = chrono::Utc::now().to_rfc3339();

    let patch = WorkPatch {
        current_stage: Some(stage.to_string()),
        stage_status: Some("active".to_string()),
        driver_schedule_id: Some(Some(schedule_id.to_string())),
        auto_chain_interrupted: Some(false),
        ..Default::default()
    };

    works::patch_work(pool, creator_id, work_id, &patch, &now)
        .await
        .map_err(AutoChainError::from)?;

    Ok(())
}

// Fix A (W-A): Shared enqueue logic — single source of truth for ACH schedule
// ID minting, pending INSERT, and set_driver. Used by both the supervisor
// terminal hook and the boot recovery path to eliminate duplication.
/// Enqueue a new auto-chain schedule and update the Work checkpoint.
///
/// This is the single shared path for:
/// 1. Supervisor `on_schedule_terminal` → `enqueue_auto_chain_step`
/// 2. Boot `resume_auto_chain_work`
///
/// It owns: (a) schedule ID generation (`ACH{timestamp}`), (b) pending schedule
/// INSERT into `creator_schedules`, (c) `set_driver` call on the Work.
///
/// # Errors
///
/// Returns `AutoChainError::InvalidState` if no schedule mapping exists for the
/// given stage. Returns `AutoChainError::Database` if any DB operation fails.
pub async fn enqueue_auto_chain_schedule(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    stage: &str,
    chapter: Option<i32>,
    volume: Option<i32>,
    work: &WorkRecord,
) -> Result<String, AutoChainError> {
    let schedule_req = build_auto_chain_schedule(stage, creator_id, work, chapter, volume)
        .ok_or_else(|| {
            AutoChainError::InvalidState(format!("no schedule mapping for stage '{stage}'"))
        })?;

    // Fix A: Single source of truth for ACH schedule ID format.
    // R-V139P0-W-B: append per-process monotonic counter for collision resistance.
    // Pure-timestamp IDs could collide under millisecond-granule concurrent enqueue;
    // the counter provides unique suffix without adding a new crate dependency.

    let counter = ACH_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let schedule_id = format!(
        "ACH{}{:06x}",
        chrono::Utc::now().format("%Y%m%d%H%M%S%3f"),
        counter & 0x00FF_FFFF
    );
    let now_ts = chrono::Utc::now().timestamp();

    // SAFETY: dynamic SQL — auto-chain schedule insert with derived params.
    // R-V139P5-S4: read preset_version from the manifest mapping instead of
    // hard-coding 1. Keep in sync with embedded-presets/*/preset.yaml `version:`.
    let preset_version = preset_version_for_id(&schedule_req.preset_id);
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id)
           VALUES (?, ?, ?, ?, 'pending', 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(&schedule_id)
    .bind(creator_id)
    .bind(&schedule_req.preset_id)
    .bind(preset_version)
    .bind(&schedule_req.label)
    .bind(now_ts)
    .bind(now_ts)
    .bind(work_id)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "auto-chain: failed to insert schedule");
        AutoChainError::Database(nexus_local_db::LocalDbError::from(e))
    })?;

    // Update the Work checkpoint to point at the new driver schedule.
    set_driver(pool, creator_id, work_id, &schedule_id, stage).await?;

    // V1.42 P0 (T3): Acquire runtime lock for this schedule.
    // Holder format: `daemon:schedule:<schedule_id>`.
    let holder = nexus_local_db::runtime_lock::schedule_holder(&schedule_id);
    let ttl = nexus_local_db::runtime_lock::ttl_from_env();
    match nexus_local_db::acquire_runtime_lock(
        pool, creator_id, work_id, &holder, ttl, true, // force_stale=true for daemon
    )
    .await
    {
        Ok(nexus_local_db::AcquireResult::Acquired { .. }) => {}
        Ok(nexus_local_db::AcquireResult::Locked {
            holder: existing, ..
        }) => {
            tracing::warn!(
                work_id = %work_id,
                schedule_id = %schedule_id,
                existing_holder = %existing,
                "runtime_lock: could not acquire for auto-chain (locked by another process)"
            );
            // Continue — auto-chain will skip if Work is locked at next tick.
        }
        Err(e) => {
            tracing::warn!(
                work_id = %work_id,
                schedule_id = %schedule_id,
                error = %e,
                "runtime_lock: failed to acquire for auto-chain"
            );
            // Non-fatal — the schedule was already enqueued.
        }
    }

    tracing::info!(
        work_id = %work_id,
        schedule_id = %schedule_id,
        stage = %stage,
        chapter = chapter.unwrap_or(0),
        "auto-chain: enqueued next step"
    );

    Ok(schedule_id)
}

/// R-V139P5-S4: Map `preset_id` to its embedded manifest version.
///
/// Must be kept in sync with `embedded-presets/*/preset.yaml` `version:` field.
/// Returns 1 as fallback for unknown preset IDs.
///
/// R-V139P5-W-4: version policy — bump the version number in both this mapping
/// AND the corresponding `preset.yaml` whenever the state machine undergoes a
/// breaking change (state additions/removals, transition edge changes, prompt
/// template modifications that alter the output contract). Non-breaking changes
/// (comments, optional fields) may keep the same version. The version is stored
/// in `creator_schedules` at enqueue time and used by the loader for compat checks.
fn preset_version_for_id(preset_id: &str) -> i64 {
    match preset_id {
        "novel-writing" => 7,
        "research" | "novel-review-master" => 2,
        "kb-extract" => 3,
        // V1.47: `novel-chapter-review` replaces `reflection-loop` (renamed
        // per compass §0.1 #6). Bumped to version 1 (was already 1 as
        // `reflection-loop`); the state-machine contract is intentionally new
        // (load_chapter → review → done) but ships at v1 because no prior
        // consumer depends on the old `reflection-loop` version.
        // All other presets default to version 1
        _ => 1,
    }
}

/// Enqueue a `novel-review-master` preset run for a Work whose findings have
/// passed the master-decision SLA (V1.39 P4 T4).
///
/// This is the auto-enqueue half of the stale-findings watcher. It is
/// **only** called by the daemon's stale-findings sweep when the Work has
/// `auto_review_master_on_timeout = true`. The flag default is `false`, so
/// no schedule is created without explicit opt-in.
///
/// Unlike [`enqueue_auto_chain_schedule`], this does not touch the Work's
/// `driver_schedule_id` — `novel-review-master` is an out-of-band review
/// preset and the Work's FL-E driver is unrelated.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the schedule INSERT fails.
pub async fn enqueue_review_master_schedule(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
) -> Result<String, AutoChainError> {
    // R-V147P0-05 (hotfix H-1): append a per-process monotonic counter suffix
    // (mirrors `ACH_COUNTER` / R-V139P0-W-B) so two enqueues in the same
    // millisecond produce distinct PKs. Without this, the
    // `master_decision_timeout::repeated_sweeps_remain_stable` test flakes
    // when both sweeps land in the same `%Y%m%d%H%M%S%3f` granule.
    let counter = RVM_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let schedule_id = format!(
        "RVM{}{:06x}",
        chrono::Utc::now().format("%Y%m%d%H%M%S%3f"),
        counter & 0x00FF_FFFF
    );
    let now_ts = chrono::Utc::now().timestamp();
    let label = format!("auto-review-master: {work_id}");
    let preset_version = preset_version_for_id("novel-review-master");

    // SAFETY: dynamic SQL — review-master schedule insert with derived params.
    // Matches the `enqueue_auto_chain_schedule` pattern (runtime sqlx is the
    // established convention in this crate; see auto_chain.rs:354-355).
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id)
           VALUES (?, ?, 'novel-review-master', ?, 'pending', 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(&schedule_id)
    .bind(creator_id)
    .bind(preset_version)
    .bind(&label)
    .bind(now_ts)
    .bind(now_ts)
    .bind(work_id)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, work_id, "stale-findings: failed to insert review-master schedule");
        AutoChainError::Database(nexus_local_db::LocalDbError::from(e))
    })?;

    tracing::info!(
        work_id,
        schedule_id = %schedule_id,
        "stale-findings: enqueued novel-review-master (opt-in)"
    );

    Ok(schedule_id)
}

/// Find auto-chain-enabled Works that have a `driver_schedule_id` but whose
/// schedule is no longer running (interrupted during daemon restart).
///
/// Returns works where `auto_chain_enabled = true` and `driver_schedule_id IS NOT NULL`
/// and `auto_chain_interrupted = false` and the schedule status is not 'running'.
///
/// # Errors
///
/// Returns `AutoChainError::Database` if the database query fails.
pub async fn find_resumable_works(pool: &SqlitePool) -> Result<Vec<WorkRecord>, AutoChainError> {
    // SAFETY: dynamic SQL — complex multi-table join for boot recovery.
    // V1.42 P0: skip Works with a foreign runtime_lock_holder.
    let rows = sqlx::query(&format!(
        "SELECT {0} FROM works w
         WHERE w.auto_chain_enabled = 1
           AND w.driver_schedule_id IS NOT NULL
           AND w.auto_chain_interrupted = 0
           AND w.status != 'completed'
           AND w.runtime_lock_holder IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM creator_schedules cs
               WHERE cs.schedule_id = w.driver_schedule_id
                 AND cs.status = 'running'
           )",
        works::WORKS_COLUMNS
    ))
    .fetch_all(pool)
    .await
    .map_err(nexus_local_db::LocalDbError::from)?;

    Ok(rows.iter().map(works::row_to_work_record).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_at(stage: &str, status: &str, chapter: i32, total: i32) -> WorkRecord {
        WorkRecord {
            work_id: "wrk_test".to_string(),
            creator_id: "ctr_test".to_string(),
            workspace_slug: "default".to_string(),
            status: "active".to_string(),
            title: "Test Novel".to_string(),
            long_term_goal: "Write a novel".to_string(),
            initial_idea: "A sci-fi thriller".to_string(),
            creative_brief: None,
            intake_status: "complete".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: "2026-06-09T10:00:00Z".to_string(),
            updated_at: "2026-06-09T10:00:00Z".to_string(),
            current_stage: stage.to_string(),
            stage_status: status.to_string(),
            work_profile: Some("novel".to_string()),
            work_ref: Some("test-novel".to_string()),
            total_planned_chapters: if total > 0 { Some(total) } else { None },
            current_chapter: chapter,
            auto_chain_enabled: true,
            driver_schedule_id: Some("sch_driver_001".to_string()),
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        }
    }

    // ── evaluate_next_step tests ──────────────────────────────────────

    #[test]
    fn intake_complete_advances_to_research() {
        let work = work_at("intake", "complete", 0, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "research".to_string(),
            }
        );
    }

    #[test]
    fn research_complete_advances_to_produce() {
        let work = work_at("research", "complete", 0, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "produce".to_string(),
            }
        );
    }

    #[test]
    fn produce_complete_advances_to_review() {
        let work = work_at("produce", "complete", 1, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "review".to_string(),
            }
        );
    }

    #[test]
    fn review_complete_advances_to_persist() {
        let work = work_at("review", "complete", 1, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "persist".to_string(),
            }
        );
    }

    #[test]
    fn persist_complete_chapter1_of_3_starts_next_chapter() {
        let work = work_at("persist", "complete", 1, 3);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::NextChapter {
                work_id: "wrk_test".to_string(),
                next_chapter: 2,
                next_volume: 1,
            }
        );
    }

    #[test]
    fn persist_complete_last_chapter_marks_work_complete() {
        let work = work_at("persist", "complete", 3, 3);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::WorkComplete {
                work_id: "wrk_test".to_string(),
            }
        );
    }

    #[test]
    fn no_chapters_marks_work_complete_after_persist() {
        let work = work_at("persist", "complete", 0, 0);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::WorkComplete {
                work_id: "wrk_test".to_string(),
            }
        );
    }

    #[test]
    fn auto_chain_disabled_no_action() {
        let mut work = work_at("research", "complete", 0, 10);
        work.auto_chain_enabled = false;
        let action = evaluate_next_step(&work);
        assert_eq!(action, ChainAction::NoAction);
    }

    #[test]
    fn auto_chain_interrupted_no_action() {
        let mut work = work_at("research", "complete", 0, 10);
        work.auto_chain_interrupted = true;
        let action = evaluate_next_step(&work);
        assert_eq!(action, ChainAction::NoAction);
    }

    #[test]
    fn work_already_completed_no_action() {
        let mut work = work_at("persist", "complete", 10, 10);
        work.status = "completed".to_string();
        let action = evaluate_next_step(&work);
        assert_eq!(action, ChainAction::NoAction);
    }

    #[test]
    fn stage_active_no_action() {
        let work = work_at("research", "active", 0, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(action, ChainAction::NoAction);
    }

    #[test]
    fn intake_skipped_advances_to_research() {
        let mut work = work_at("intake", "skipped", 0, 10);
        work.intake_status = "skipped".to_string();
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "research".to_string(),
            }
        );
    }

    #[test]
    fn build_auto_chain_schedule_produce_includes_chapter() {
        let work = work_at("produce", "active", 2, 5);
        let req = build_auto_chain_schedule("produce", "ctr_test", &work, Some(2), None)
            .expect("produce should have a preset");
        assert_eq!(req.preset_id, "novel-writing");
        let input = req.input.expect("input should be set");
        assert_eq!(input["chapter"], 2);
        assert_eq!(input["work_id"], "wrk_test");
    }

    #[test]
    fn build_auto_chain_schedule_research() {
        let work = work_at("research", "active", 0, 5);
        let req = build_auto_chain_schedule("research", "ctr_test", &work, None, None)
            .expect("research should have a preset");
        assert_eq!(req.preset_id, "research");
    }

    #[test]
    fn persist_complete_chapter5_of_10_starts_chapter6() {
        let work = work_at("persist", "complete", 5, 10);
        let action = evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::NextChapter {
                work_id: "wrk_test".to_string(),
                next_chapter: 6,
                next_volume: 1,
            }
        );
    }

    // ── Fix A (W-A): enqueue_auto_chain_schedule shared helper ─────────

    #[tokio::test]
    async fn enqueue_helper_success_path() {
        let db = tempfile::Builder::new()
            .prefix("auto_chain_helper_")
            .suffix(".db")
            .tempfile()
            .unwrap();
        let db_path = db.path().to_path_buf();
        std::mem::forget(db);

        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let work = work_at("intake", "complete", 0, 3);
        nexus_local_db::works::create_work(&pool, &work)
            .await
            .unwrap();

        let sid = enqueue_auto_chain_schedule(
            &pool, "ctr_test", "wrk_test", "research", None, None, &work,
        )
        .await
        .unwrap();

        // Verify schedule ID format
        assert!(
            sid.starts_with("ACH"),
            "schedule ID should start with ACH: {sid}"
        );

        // Verify schedule was inserted as pending
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM creator_schedules WHERE schedule_id = ?")
                .bind(&sid)
                .fetch_optional(&pool)
                .await
                .unwrap()
                .flatten();
        assert_eq!(
            status.as_deref(),
            Some("pending"),
            "schedule should be pending"
        );

        // Verify driver_schedule_id was set on the work
        let updated = nexus_local_db::works::get_work(&pool, "ctr_test", "wrk_test")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.driver_schedule_id, Some(sid));
        assert_eq!(updated.current_stage, "research");
    }

    #[tokio::test]
    async fn enqueue_helper_error_path_no_mapping() {
        let db = tempfile::Builder::new()
            .prefix("auto_chain_helper_err_")
            .suffix(".db")
            .tempfile()
            .unwrap();
        let db_path = db.path().to_path_buf();
        std::mem::forget(db);

        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let mut work = work_at("intake", "complete", 0, 3);
        work.primary_preset_id = "nonexistent-preset".to_string();
        nexus_local_db::works::create_work(&pool, &work)
            .await
            .unwrap();

        let result = enqueue_auto_chain_schedule(
            &pool,
            "ctr_test",
            "wrk_test",
            "unknown_stage_xyz",
            None,
            None,
            &work,
        )
        .await;

        assert!(result.is_err(), "should fail for unknown stage");
        let err = result.unwrap_err();
        assert!(
            matches!(err, AutoChainError::InvalidState(_)),
            "should be InvalidState: {err:?}"
        );
    }

    // ── R-V147P0-05 (hotfix H-1): RVM schedule_id PK collision regression ────

    /// Regression for R-V147P0-05: two `enqueue_review_master_schedule` calls
    /// landing in the same `%Y%m%d%H%M%S%3f` millisecond granule MUST produce
    /// distinct `schedule_id` PKs. Before the fix, the second INSERT collided
    /// on the PK and surfaced as a flake in
    /// `master_decision_timeout::repeated_sweeps_remain_stable`.
    ///
    /// The per-process `RVM_COUNTER` provides the unique suffix without adding
    /// a new crate dependency (mirrors the `ACH_COUNTER` fix, R-V139P0-W-B).
    #[tokio::test]
    async fn rvm_schedule_ids_are_unique_within_same_millisecond() {
        let db = tempfile::Builder::new()
            .prefix("rvm_pk_collision_")
            .suffix(".db")
            .tempfile()
            .unwrap();
        let db_path = db.path().to_path_buf();
        std::mem::forget(db);

        let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
        nexus_local_db::run_migrations(&pool).await.unwrap();

        let work = work_at("review", "active", 1, 3);
        nexus_local_db::works::create_work(&pool, &work)
            .await
            .unwrap();

        // Fire two enqueues back-to-back. Even if both land in the same ms
        // granule, the counter suffix must keep the PKs distinct.
        let sid_a = enqueue_review_master_schedule(&pool, "ctr_test", "wrk_test")
            .await
            .expect("first RVM enqueue must succeed");
        let sid_b = enqueue_review_master_schedule(&pool, "ctr_test", "wrk_test")
            .await
            .expect("second RVM enqueue must succeed even in the same ms");

        assert!(
            sid_a != sid_b,
            "RVM schedule ids must be distinct; got sid_a={sid_a} sid_b={sid_b}"
        );
        assert!(
            sid_a.starts_with("RVM") && sid_b.starts_with("RVM"),
            "both ids must keep the RVM prefix: sid_a={sid_a} sid_b={sid_b}"
        );

        // Both rows must be present in the table (no PK collision).
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM creator_schedules \
             WHERE preset_id = 'novel-review-master' AND work_id = ?",
        )
        .bind("wrk_test")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            n, 2,
            "both RVM schedules must be persisted without PK collision; got count={n}"
        );
    }

    // ── V1.39 P0.5 (T6): research-stage wiring integration tests ───────

    /// AC1: research preset schedule has `fl_e_stage` = "research" and includes
    /// `creative_brief` + `inspiration_log` in the seed (same surface produce reads).
    #[test]
    fn research_schedule_seed_includes_context_for_produce() {
        let work = work_at("research", "active", 0, 5);
        let req = build_auto_chain_schedule("research", "ctr_test", &work, None, None)
            .expect("research should have a preset");
        assert_eq!(req.preset_id, "research");

        let seed: serde_json::Value =
            serde_json::from_str(&req.seed.expect("seed must be set")).unwrap();

        // fl_e_stage annotation
        assert_eq!(seed["fl_e_stage"], "research");
        // creative_brief and inspiration_log are the shared surface
        // that produce reads — research can enrich these.
        assert!(seed["creative_brief"].is_string());
        assert!(seed["inspiration_log"].is_string());
        assert_eq!(seed["work_id"], "wrk_test");
    }

    /// AC1: produce stage seed also carries `creative_brief` and `inspiration_log`,
    /// confirming the shared context surface between research and produce.
    #[test]
    fn produce_schedule_seed_carries_research_enrichable_fields() {
        let work = work_at("produce", "active", 1, 5);
        let req = build_auto_chain_schedule("produce", "ctr_test", &work, Some(1), None)
            .expect("produce should have a preset");
        assert_eq!(req.preset_id, "novel-writing");

        let input = req.input.expect("input must be set");
        assert_eq!(input["fl_e_stage"], "produce");
        // These are the same fields research enriches, confirming
        // the downstream produce stage can see research-derived material.
        assert!(input.get("creative_brief").is_some());
        assert!(input.get("inspiration_log").is_some());
    }

    /// Fix W-2: produce stage input includes `research_artifacts_dir` when
    /// the work has a `driver_schedule_id` (the research schedule that just
    /// completed). This enables AC2 and AC3 (produce sees research output).
    #[test]
    fn produce_schedule_includes_research_artifacts_dir() {
        let mut work = work_at("produce", "active", 1, 5);
        // Simulate: driver_schedule_id is the research schedule that just completed
        work.driver_schedule_id = Some("ACH20260609120000000".to_string());
        let req = build_auto_chain_schedule("produce", "ctr_test", &work, Some(1), None)
            .expect("produce should have a preset");

        let input = req.input.expect("input must be set");
        let rad = input
            .get("research_artifacts_dir")
            .expect("Fix W-2: produce input must include research_artifacts_dir");
        assert!(
            rad.as_str().unwrap().contains("ACH20260609120000000"),
            "research_artifacts_dir should contain the driver schedule ID: {rad}"
        );
        assert!(
            rad.as_str().unwrap().starts_with(".nexus42/references/"),
            "research_artifacts_dir should use .nexus42/references/ prefix: {rad}"
        );
    }

    /// Fix W-2 (negative): research stage does NOT include `research_artifacts_dir`.
    #[test]
    fn research_schedule_does_not_include_research_artifacts_dir() {
        let mut work = work_at("research", "active", 0, 5);
        work.driver_schedule_id = Some("SCH_prev_research".to_string());
        let req = build_auto_chain_schedule("research", "ctr_test", &work, None, None)
            .expect("research should have a preset");

        let input = req.input.expect("input must be set");
        assert!(
            input.get("research_artifacts_dir").is_none(),
            "research stage should NOT include research_artifacts_dir"
        );
    }

    /// AC2: full chain intake→research→produce advances correctly
    /// (verifies `evaluate_next_step` for the research-middle position).
    #[test]
    fn full_chain_intake_research_produce_advances() {
        // intake complete → advance to research
        let work = work_at("intake", "complete", 0, 3);
        assert_eq!(
            evaluate_next_step(&work),
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "research".to_string(),
            }
        );

        // research complete → advance to produce
        let work = work_at("research", "complete", 0, 3);
        assert_eq!(
            evaluate_next_step(&work),
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "produce".to_string(),
            }
        );

        // produce complete (ch1 of 3) → advance to review (not NextChapter)
        let work = work_at("produce", "complete", 1, 3);
        assert_eq!(
            evaluate_next_step(&work),
            ChainAction::AdvanceStage {
                work_id: "wrk_test".to_string(),
                next_stage: "review".to_string(),
            }
        );
    }

    /// QC1 W-2: assert `preset_version_for_id` stays in sync with
    /// embedded preset.yaml version fields.
    #[test]
    fn preset_version_mapping_matches_yaml() {
        use crate::preset::EMBEDDED_PRESETS;

        let known_ids = [
            "novel-writing",
            "research",
            "novel-review-master",
            "kb-extract",
        ];

        for preset_id in &known_ids {
            let mapping_version = preset_version_for_id(preset_id);

            // Find the embedded preset
            let yaml_path = format!("{preset_id}/preset.yaml");
            let yaml_bytes = EMBEDDED_PRESETS.get_file(&yaml_path).unwrap_or_else(|| {
                panic!("preset.yaml missing for '{preset_id}' at '{yaml_path}'")
            });
            let yaml_str = std::str::from_utf8(yaml_bytes.contents())
                .unwrap_or_else(|e| panic!("preset.yaml for '{preset_id}' is not UTF-8: {e}"));

            // Extract version: field from YAML
            let yaml_version = yaml_str
                .lines()
                .find_map(|line| {
                    let trimmed = line.trim();
                    trimmed.strip_prefix("version:").map(|v| {
                        v.split_whitespace()
                            .next()
                            .unwrap()
                            .trim()
                            .parse::<i64>()
                            .unwrap_or_else(|_| {
                                panic!(
                                    "non-integer version in preset.yaml for '{preset_id}': '{v}'"
                                )
                            })
                    })
                })
                .unwrap_or_else(|| panic!("no 'version:' field in preset.yaml for '{preset_id}'"));

            assert_eq!(
                mapping_version, yaml_version,
                "preset_version_for_id('{preset_id}') = {mapping_version}, but preset.yaml version = {yaml_version}. \
                 Update the match arm in preset_version_for_id() to match."
            );
        }
    }
}
