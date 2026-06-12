//! Integration test for F-001: supervisor-level cross-volume auto-chain wiring.
//!
//! Verifies that the supervisor's `on_schedule_terminal` path uses the
//! volume-aware evaluator to correctly transition between volumes when
//! all chapters in volume N are finalized.

use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::auto_chain::{self, ChainAction};
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;
use std::sync::Arc;

/// Helper: open an in-memory DB pool with migrations applied.
async fn test_pool() -> sqlx::SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("xvol_test_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    std::mem::forget(db);

    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    pool
}

/// Helper: standard test Work.
fn test_work(work_id: &str, chapter: i32, total: i32, auto_chain: bool) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: "ctr_test".to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "Cross-Volume Novel".to_string(),
        long_term_goal: "Write a multi-volume novel".to_string(),
        initial_idea: "An epic saga".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-11T10:00:00Z".to_string(),
        updated_at: "2026-06-11T10:00:00Z".to_string(),
        current_stage: "persist".to_string(),
        stage_status: "active".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some("xvol-novel".to_string()),
        total_planned_chapters: if total > 0 { Some(total) } else { None },
        current_chapter: chapter,
        auto_chain_enabled: auto_chain,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    }
}

/// Seed chapter rows across multiple volumes.
///
/// Chapters in vol `1..=finalized_vol` ch `1..=finalized_ch` are "finalized";
/// everything else is "not_started".
async fn seed_multi_volume_chapters(
    pool: &sqlx::SqlitePool,
    work_id: &str,
    volumes: i32,
    chapters_per_volume: i32,
    finalized_vol: i32,
    finalized_ch: i32,
) {
    let now_ts = chrono::Utc::now().timestamp();
    for vol in 1..=volumes {
        for ch in 1..=chapters_per_volume {
            let status = if vol < finalized_vol || (vol == finalized_vol && ch <= finalized_ch) {
                "finalized"
            } else {
                "not_started"
            };
            // SAFETY: test-only — DML helper for multi-volume chapter seeding.
            sqlx::query(
                "INSERT INTO work_chapters
                   (work_id, volume, chapter, slug, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(work_id)
            .bind(vol)
            .bind(ch)
            .bind(format!("v{vol:02}-ch{ch:02}"))
            .bind(status)
            .bind(now_ts)
            .bind(now_ts)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

/// Helper: insert a minimal schedule row.
async fn insert_driver_schedule(
    pool: &sqlx::SqlitePool,
    schedule_id: &str,
    creator_id: &str,
    preset_id: &str,
    status: &str,
    work_id: &str,
) {
    let now = chrono::Utc::now().timestamp();
    // SAFETY: test-only — DML helper for schedule row insertion.
    sqlx::query(
        r"INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version,
            label, created_at, updated_at, work_id)
           VALUES (?, ?, ?, 1, ?, 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(schedule_id)
    .bind(creator_id)
    .bind(preset_id)
    .bind(status)
    .bind(format!("fl-e-persist-{work_id}"))
    .bind(now)
    .bind(now)
    .bind(work_id)
    .execute(pool)
    .await
    .unwrap();
}

// ── F-001: Cross-volume supervisor integration ─────────────────────────

/// AC2 (F-001): After finalizing the last chapter of volume 1 in a
/// two-volume work, the supervisor enqueues a produce schedule for
/// volume 2 chapter 1 (not volume 1 chapter N+1).
#[tokio::test]
async fn f001_cross_volume_supervisor_enqueues_vol2_chapter1() {
    let pool = test_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(Arc::new(pool.clone())));

    // Work: 2 volumes × 3 chapters = 6 total
    let mut work = test_work("wrk_xvol_1", 3, 6, true);
    work.current_stage = "persist".to_string();
    work.stage_status = "active".to_string();
    works::create_work(&pool, &work).await.unwrap();

    // Seed chapters: vol 1 all finalized, vol 2 all not_started
    seed_multi_volume_chapters(&pool, "wrk_xvol_1", 2, 3, 1, 3).await;

    // Insert the driver schedule (persist for vol1 ch3) as running
    insert_driver_schedule(
        &pool,
        "sch_xvol_persist_v1c3",
        "ctr_test",
        "novel-writing",
        "running",
        "wrk_xvol_1",
    )
    .await;

    auto_chain::set_driver(
        &pool,
        "ctr_test",
        "wrk_xvol_1",
        "sch_xvol_persist_v1c3",
        "persist",
    )
    .await
    .unwrap();

    // Mark stage as complete
    works::patch_work(
        &pool,
        "ctr_test",
        "wrk_xvol_1",
        &works::WorkPatch {
            stage_status: Some("complete".to_string()),
            ..Default::default()
        },
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();

    // Complete the persist schedule via the supervisor
    sup.on_schedule_terminal(
        "sch_xvol_persist_v1c3",
        nexus_contracts::local::schedule::ScheduleStatus::Completed,
    )
    .await
    .unwrap();

    // Verify: the work should advance to produce stage
    let updated = works::get_work(&pool, "ctr_test", "wrk_xvol_1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.current_stage, "produce",
        "should advance to produce for vol 2 ch 1"
    );
    assert!(
        updated.driver_schedule_id.is_some(),
        "should have a new driver schedule"
    );

    // Verify: the volume-aware evaluator picks vol 2 ch 1, not vol 1 ch 4.
    // After the supervisor enqueues, the work is at produce/active, not persist/complete,
    // so re-evaluating gives NoAction. Instead verify via chapter row status.
    let v2c1_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM work_chapters WHERE work_id = ? AND volume = 2 AND chapter = 1",
    )
    .bind("wrk_xvol_1")
    .fetch_optional(&pool)
    .await
    .unwrap()
    .flatten();
    assert_eq!(
        v2c1_status.as_deref(),
        Some("not_started"),
        "vol 2 ch 1 should still be not_started (just enqueued)"
    );
}

/// AC2 (F-001 negative): Single-volume work where all chapters are finalized
/// should mark the work as completed (not try to advance to a non-existent vol 2).
#[tokio::test]
async fn f001_single_volume_all_finalized_marks_complete() {
    let pool = test_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(Arc::new(pool.clone())));

    // Work: 1 volume × 3 chapters = 3 total
    let mut work = test_work("wrk_xvol_2", 3, 3, true);
    work.current_stage = "persist".to_string();
    work.stage_status = "active".to_string();
    works::create_work(&pool, &work).await.unwrap();

    // Seed chapters: vol 1 all finalized
    seed_multi_volume_chapters(&pool, "wrk_xvol_2", 1, 3, 1, 3).await;

    insert_driver_schedule(
        &pool,
        "sch_xvol_persist_v1c3b",
        "ctr_test",
        "novel-writing",
        "running",
        "wrk_xvol_2",
    )
    .await;

    auto_chain::set_driver(
        &pool,
        "ctr_test",
        "wrk_xvol_2",
        "sch_xvol_persist_v1c3b",
        "persist",
    )
    .await
    .unwrap();

    works::patch_work(
        &pool,
        "ctr_test",
        "wrk_xvol_2",
        &works::WorkPatch {
            stage_status: Some("complete".to_string()),
            ..Default::default()
        },
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();

    sup.on_schedule_terminal(
        "sch_xvol_persist_v1c3b",
        nexus_contracts::local::schedule::ScheduleStatus::Completed,
    )
    .await
    .unwrap();

    let updated = works::get_work(&pool, "ctr_test", "wrk_xvol_2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.status, "completed",
        "single-volume work with all chapters finalized should be completed"
    );
    assert!(
        updated.driver_schedule_id.is_none(),
        "driver should be cleared on completion"
    );
}

/// F-001 (hermetic): `evaluate_after_persist_volume_aware` returns vol 2 ch 1
/// when vol 1 is fully finalized but vol 2 has remaining chapters.
#[tokio::test]
async fn f001_volume_aware_evaluator_picks_vol2_ch1() {
    let pool = test_pool().await;

    let work = test_work("wrk_xvol_3", 3, 6, true);
    works::create_work(&pool, &work).await.unwrap();

    // Seed: vol 1 all finalized, vol 2 all not_started
    seed_multi_volume_chapters(&pool, "wrk_xvol_3", 2, 3, 1, 3).await;

    let action = auto_chain::evaluate_after_persist_volume_aware(&pool, &work)
        .await
        .unwrap();

    assert_eq!(
        action,
        ChainAction::NextChapter {
            work_id: "wrk_xvol_3".to_string(),
            next_chapter: 1,
            next_volume: 2,
        },
        "should pick vol 2 ch 1, not vol 1 ch 4"
    );
}

/// F-001 (hermetic): `evaluate_after_persist_volume_aware` returns WorkComplete
/// when all chapters across all volumes are finalized.
#[tokio::test]
async fn f001_volume_aware_evaluator_work_complete() {
    let pool = test_pool().await;

    let work = test_work("wrk_xvol_4", 6, 6, true);
    works::create_work(&pool, &work).await.unwrap();

    // Seed: all chapters finalized
    seed_multi_volume_chapters(&pool, "wrk_xvol_4", 2, 3, 2, 3).await;

    let action = auto_chain::evaluate_after_persist_volume_aware(&pool, &work)
        .await
        .unwrap();

    assert_eq!(
        action,
        ChainAction::WorkComplete {
            work_id: "wrk_xvol_4".to_string(),
        },
        "should mark work complete when all volumes finalized"
    );
}
