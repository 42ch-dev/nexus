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
use nexus_local_db::works::{self, WorkPatch, WorkRecord};
use sqlx::SqlitePool;

use crate::stage_gates::{self, WorkFields};

/// Result of an `on_schedule_complete` evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainAction {
    /// No further action needed (auto-chain disabled, work complete, or not an FL-E driver).
    NoAction,
    /// Advance to the next FL-E stage for the current chapter.
    AdvanceStage { work_id: String, next_stage: String },
    /// Start the produce stage for the next chapter (chapter outer loop).
    NextChapter { work_id: String, next_chapter: i32 },
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
        }
    } else {
        // All chapters finalized
        ChainAction::WorkComplete {
            work_id: work.work_id.clone(),
        }
    }
}

/// Build the schedule request for an auto-chain step (stage advance or next chapter).
///
/// Constructs a correctly-shaped `AddScheduleRequest` using the shared
/// [`stage_gates::build_schedule_for_stage`] facade.
#[allow(clippy::missing_panics_doc)] // panic only on invalid stage names, which we validate
pub fn build_auto_chain_schedule(
    stage: &str,
    creator_id: &str,
    work: &WorkRecord,
    chapter: Option<i32>,
) -> Option<AddScheduleRequest> {
    let work_ref = work.work_ref.clone();
    let chapter_label = chapter.map(stage_gates::chapter_label);

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

    let patch = WorkPatch {
        status: Some("completed".to_string()),
        current_stage: Some("persist".to_string()),
        stage_status: Some("complete".to_string()),
        driver_schedule_id: Some(None), // clear driver
        auto_chain_interrupted: Some(false),
        ..Default::default()
    };

    works::patch_work(pool, creator_id, work_id, &patch, &now)
        .await
        .map_err(AutoChainError::from)
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
    work: &WorkRecord,
) -> Result<String, AutoChainError> {
    let schedule_req =
        build_auto_chain_schedule(stage, creator_id, work, chapter).ok_or_else(|| {
            AutoChainError::InvalidState(format!("no schedule mapping for stage '{stage}'"))
        })?;

    // Fix A: Single source of truth for ACH schedule ID format.
    let schedule_id = format!("ACH{}", chrono::Utc::now().format("%Y%m%d%H%M%S%3f"));
    let now_ts = chrono::Utc::now().timestamp();

    // SAFETY: dynamic SQL — auto-chain schedule insert with derived params.
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id)
           VALUES (?, ?, ?, 1, 'pending', 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(&schedule_id)
    .bind(creator_id)
    .bind(&schedule_req.preset_id)
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

    tracing::info!(
        work_id = %work_id,
        schedule_id = %schedule_id,
        stage = %stage,
        chapter = chapter.unwrap_or(0),
        "auto-chain: enqueued next step"
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
    let rows = sqlx::query(&format!(
        "SELECT {0} FROM works w
         WHERE w.auto_chain_enabled = 1
           AND w.driver_schedule_id IS NOT NULL
           AND w.auto_chain_interrupted = 0
           AND w.status != 'completed'
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
        let req = build_auto_chain_schedule("produce", "ctr_test", &work, Some(2))
            .expect("produce should have a preset");
        assert_eq!(req.preset_id, "novel-writing");
        let input = req.input.expect("input should be set");
        assert_eq!(input["chapter"], 2);
        assert_eq!(input["work_id"], "wrk_test");
    }

    #[test]
    fn build_auto_chain_schedule_research() {
        let work = work_at("research", "active", 0, 5);
        let req = build_auto_chain_schedule("research", "ctr_test", &work, None)
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

        let sid =
            enqueue_auto_chain_schedule(&pool, "ctr_test", "wrk_test", "research", None, &work)
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
}
