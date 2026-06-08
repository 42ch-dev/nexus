//! Integration tests for the auto-chain engine (V1.39 §5, AC1–AC6).
//!
//! Covers:
//! - AC1: Full auto-chain stage flow (intake → research → produce → review → persist)
//! - AC2: Chapter outer loop (persist ch.N → produce ch.N+1)
//! - AC3: Work completion after last chapter
//! - AC4: Boot recovery (find_resumable_works)
//! - AC6: --no-auto-chain still writes checkpoint fields

use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::auto_chain::{self, ChainAction};
use sqlx::SqlitePool;

/// Helper: create a test Work record in the database.
async fn seed_work(pool: &SqlitePool, work: &WorkRecord) {
    works::create_work(pool, work).await.unwrap();
}

/// Helper: standard test Work with configurable chapter/chapter count.
fn test_work(work_id: &str, chapter: i32, total: i32, auto_chain: bool) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: "ctr_test".to_string(),
        workspace_slug: "ws".to_string(),
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
        current_stage: "produce".to_string(),
        stage_status: "active".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some("test-novel".to_string()),
        total_planned_chapters: if total > 0 { Some(total) } else { None },
        current_chapter: chapter,
        auto_chain_enabled: auto_chain,
        // NOTE: driver_schedule_id is always NULL in create_work; set via set_driver.
        driver_schedule_id: None,
        auto_chain_interrupted: false,
    }
}

/// Helper: seed a work AND set a driver schedule on it.
async fn seed_work_with_driver(pool: &SqlitePool, work: &WorkRecord, driver_id: &str, stage: &str) {
    seed_work(pool, work).await;
    auto_chain::set_driver(pool, "ctr_test", &work.work_id, driver_id, stage)
        .await
        .unwrap();
}

/// Helper: open an in-memory DB pool with migrations applied.
async fn test_pool() -> SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("auto_chain_test_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    // Keep the file alive for the test lifetime
    std::mem::forget(db);

    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    pool
}

// ── AC1: Full auto-chain stage flow ───────────────────────────────────

#[tokio::test]
async fn ac1_intake_to_research_chain_action() {
    let work = test_work("wrk_ac1", 0, 3, true);
    let mut w = work.clone();
    w.current_stage = "intake".to_string();
    w.stage_status = "complete".to_string();

    let action = auto_chain::evaluate_next_step(&w);
    assert_eq!(
        action,
        ChainAction::AdvanceStage {
            work_id: "wrk_ac1".to_string(),
            next_stage: "research".to_string(),
        }
    );
}

#[tokio::test]
async fn ac1_full_stage_chain_intake_to_persist() {
    // Verify the complete stage chain: intake → research → produce → review → persist
    let stages = ["intake", "research", "produce", "review"];
    let expected_next = ["research", "produce", "review", "persist"];

    for (stage, expected) in stages.iter().zip(expected_next.iter()) {
        let mut work = test_work("wrk_chain", 1, 3, true);
        work.current_stage = stage.to_string();
        work.stage_status = "complete".to_string();

        let action = auto_chain::evaluate_next_step(&work);
        assert_eq!(
            action,
            ChainAction::AdvanceStage {
                work_id: "wrk_chain".to_string(),
                next_stage: expected.to_string(),
            },
            "stage {stage} → {expected} failed"
        );
    }
}

// ── AC2: Chapter outer loop ───────────────────────────────────────────

#[tokio::test]
async fn ac2_persist_chapter1_starts_chapter2() {
    let mut work = test_work("wrk_ac2", 1, 3, true);
    work.current_stage = "persist".to_string();
    work.stage_status = "complete".to_string();

    let action = auto_chain::evaluate_next_step(&work);
    assert_eq!(
        action,
        ChainAction::NextChapter {
            work_id: "wrk_ac2".to_string(),
            next_chapter: 2,
        }
    );
}

#[tokio::test]
async fn ac2_persist_penultimate_chapter_starts_last() {
    let mut work = test_work("wrk_ac2b", 4, 5, true);
    work.current_stage = "persist".to_string();
    work.stage_status = "complete".to_string();

    let action = auto_chain::evaluate_next_step(&work);
    assert_eq!(
        action,
        ChainAction::NextChapter {
            work_id: "wrk_ac2b".to_string(),
            next_chapter: 5,
        }
    );
}

// ── AC3: Work completion after last chapter ────────────────────────────

#[tokio::test]
async fn ac3_persist_last_chapter_marks_complete() {
    let mut work = test_work("wrk_ac3", 3, 3, true);
    work.current_stage = "persist".to_string();
    work.stage_status = "complete".to_string();

    let action = auto_chain::evaluate_next_step(&work);
    assert_eq!(
        action,
        ChainAction::WorkComplete {
            work_id: "wrk_ac3".to_string(),
        }
    );
}

#[tokio::test]
async fn ac3_mark_work_completed_in_db() {
    let pool = test_pool().await;
    let work = test_work("wrk_ac3_db", 3, 3, true);
    seed_work_with_driver(&pool, &work, "sch_driver_003", "persist").await;

    let updated = auto_chain::mark_work_completed(&pool, "ctr_test", "wrk_ac3_db")
        .await
        .unwrap();

    assert_eq!(updated.status, "completed");
    assert_eq!(updated.stage_status, "complete");
    assert!(updated.driver_schedule_id.is_none());
}

// ── AC4: Boot recovery ────────────────────────────────────────────────

#[tokio::test]
async fn ac4_find_resumable_works_finds_interrupted() {
    let pool = test_pool().await;

    // Work with auto-chain enabled and driver pointing to a non-existent schedule
    let work = test_work("wrk_ac4a", 2, 5, true);
    seed_work_with_driver(&pool, &work, "sch_nonexistent", "produce").await;

    // Work that is completed — should NOT be resumable
    let work2 = test_work("wrk_ac4b", 5, 5, true);
    seed_work_with_driver(&pool, &work2, "sch_done", "persist").await;
    works::patch_work(
        &pool,
        "ctr_test",
        "wrk_ac4b",
        &works::WorkPatch {
            status: Some("completed".to_string()),
            ..Default::default()
        },
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();

    // Work with auto_chain_interrupted — should NOT be resumable
    let work3 = test_work("wrk_ac4c", 1, 3, true);
    seed_work_with_driver(&pool, &work3, "sch_interrupted", "produce").await;
    works::patch_work(
        &pool,
        "ctr_test",
        "wrk_ac4c",
        &works::WorkPatch {
            auto_chain_interrupted: Some(true),
            ..Default::default()
        },
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();

    let resumable = auto_chain::find_resumable_works(&pool).await.unwrap();
    assert_eq!(resumable.len(), 1);
    assert_eq!(resumable[0].work_id, "wrk_ac4a");
}

#[tokio::test]
async fn ac4_find_resumable_works_empty_when_none() {
    let pool = test_pool().await;

    // No works at all
    let resumable = auto_chain::find_resumable_works(&pool).await.unwrap();
    assert!(resumable.is_empty());
}

// ── AC6: --no-auto-chain still writes checkpoint fields ───────────────

#[tokio::test]
async fn ac6_auto_chain_disabled_no_action() {
    let work = test_work("wrk_ac6", 1, 3, false);
    let action = auto_chain::evaluate_next_step(&work);
    assert_eq!(action, ChainAction::NoAction);
}

#[tokio::test]
async fn ac6_checkpoint_fields_persisted_in_db() {
    let pool = test_pool().await;
    let work = test_work("wrk_ac6_db", 1, 3, false);
    seed_work_with_driver(&pool, &work, "sch_driver_006", "produce").await;

    // Now disable auto-chain
    works::patch_work(
        &pool,
        "ctr_test",
        "wrk_ac6_db",
        &works::WorkPatch {
            auto_chain_enabled: Some(false),
            ..Default::default()
        },
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();

    let loaded = works::get_work(&pool, "ctr_test", "wrk_ac6_db")
        .await
        .unwrap()
        .expect("work should exist");

    assert!(
        !loaded.auto_chain_enabled,
        "auto_chain_enabled should be false"
    );
    assert_eq!(
        loaded.driver_schedule_id,
        Some("sch_driver_006".to_string()),
        "driver_schedule_id should still be written"
    );
    assert!(
        !loaded.auto_chain_interrupted,
        "auto_chain_interrupted should be false"
    );
}

// ── DB helpers: set_driver / clear_driver / update_checkpoint ─────────

#[tokio::test]
async fn set_driver_updates_work() {
    let pool = test_pool().await;
    let work = test_work("wrk_driver", 1, 3, true);
    seed_work(&pool, &work).await;

    auto_chain::set_driver(&pool, "ctr_test", "wrk_driver", "sch_new_123", "research")
        .await
        .unwrap();

    let loaded = works::get_work(&pool, "ctr_test", "wrk_driver")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.driver_schedule_id, Some("sch_new_123".to_string()));
    assert_eq!(loaded.current_stage, "research");
    assert_eq!(loaded.stage_status, "active");
}

#[tokio::test]
async fn clear_driver_clears_schedule_id() {
    let pool = test_pool().await;
    let work = test_work("wrk_clear", 1, 3, true);
    seed_work_with_driver(&pool, &work, "sch_to_clear", "produce").await;

    auto_chain::clear_driver(&pool, "ctr_test", "wrk_clear")
        .await
        .unwrap();

    let loaded = works::get_work(&pool, "ctr_test", "wrk_clear")
        .await
        .unwrap()
        .unwrap();
    assert!(loaded.driver_schedule_id.is_none());
}

#[tokio::test]
async fn find_work_for_driver_returns_matching_work() {
    let pool = test_pool().await;
    let work = test_work("wrk_find", 1, 3, true);
    seed_work_with_driver(&pool, &work, "sch_driver_001", "produce").await;

    let found = auto_chain::find_work_for_driver(&pool, "sch_driver_001")
        .await
        .unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().work_id, "wrk_find");
}

#[tokio::test]
async fn find_work_for_driver_returns_none_for_unknown() {
    let pool = test_pool().await;
    let work = test_work("wrk_find2", 1, 3, true);
    seed_work(&pool, &work).await;

    let found = auto_chain::find_work_for_driver(&pool, "sch_unknown")
        .await
        .unwrap();
    assert!(found.is_none());
}

// ── Fix 1: Supervisor terminal hook for auto-chain (AC1 end-to-end) ───

use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;
use std::sync::Arc;

/// Helper: create a supervisor backed by the test pool.
fn test_supervisor(pool: SqlitePool) -> Arc<ScheduleSupervisor> {
    Arc::new(ScheduleSupervisor::new(Arc::new(pool)))
}

/// Helper: insert a minimal schedule row directly (bypasses insert_pending validation).
async fn insert_driver_schedule(
    pool: &SqlitePool,
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
    .bind(format!("fl-e-intake-{work_id}"))
    .bind(now)
    .bind(now)
    .bind(work_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Helper: check if a schedule exists with the given status.
async fn schedule_status(pool: &SqlitePool, schedule_id: &str) -> Option<String> {
    // SAFETY: test-only — scalar lookup for schedule status.
    sqlx::query_scalar::<_, String>(
        "SELECT status FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(schedule_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn fix1_terminal_completed_enqueues_next_stage() {
    let pool = test_pool().await;
    let sup = test_supervisor(pool.clone());

    // Create a Work at intake/complete with auto-chain enabled
    let mut work = test_work("wrk_fix1a", 0, 3, true);
    work.current_stage = "intake".to_string();
    work.stage_status = "active".to_string(); // start active, will be advanced via patch
    seed_work(&pool, &work).await;

    // Insert the driver schedule as running
    insert_driver_schedule(&pool, "sch_intake_001", "ctr_test", "novel-writing", "running", "wrk_fix1a").await;

    // Set driver — this also sets stage_status to "active"
    auto_chain::set_driver(&pool, "ctr_test", "wrk_fix1a", "sch_intake_001", "intake")
        .await
        .unwrap();

    // Mark stage as complete (simulating what the schedule runner does when it finishes)
    works::patch_work(
        &pool,
        "ctr_test",
        "wrk_fix1a",
        &works::WorkPatch {
            stage_status: Some("complete".to_string()),
            ..Default::default()
        },
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();

    // Complete the intake schedule via the supervisor terminal handler
    sup.on_schedule_terminal("sch_intake_001", nexus_contracts::local::schedule::ScheduleStatus::Completed)
        .await
        .unwrap();

    // Verify the work now has a new driver schedule and is at research stage
    let updated = works::get_work(&pool, "ctr_test", "wrk_fix1a")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.current_stage, "research", "should have advanced to research");
    assert!(updated.driver_schedule_id.is_some(), "should have a new driver schedule");

    // Verify the old schedule is completed
    assert_eq!(
        schedule_status(&pool, "sch_intake_001").await,
        Some("completed".to_string()),
        "intake schedule should be completed"
    );

    // Verify a new schedule was created and is pending or running
    let new_driver = updated.driver_schedule_id.unwrap();
    let new_status = schedule_status(&pool, &new_driver).await;
    assert!(
        new_status.is_some(),
        "new driver schedule should exist: {new_driver}"
    );
    assert!(
        matches!(new_status.as_deref(), Some("pending" | "running")),
        "new driver should be pending or running, got: {new_status:?}"
    );
}

#[tokio::test]
async fn fix1_terminal_failed_does_not_enqueue_next() {
    let pool = test_pool().await;
    let sup = test_supervisor(pool.clone());

    let mut work = test_work("wrk_fix1b", 0, 3, true);
    work.current_stage = "intake".to_string();
    work.stage_status = "active".to_string();
    seed_work(&pool, &work).await;

    insert_driver_schedule(&pool, "sch_fail_001", "ctr_test", "novel-writing", "running", "wrk_fix1b").await;

    auto_chain::set_driver(&pool, "ctr_test", "wrk_fix1b", "sch_fail_001", "intake")
        .await
        .unwrap();

    // Fail the schedule — should NOT trigger auto-chain
    sup.on_schedule_terminal("sch_fail_001", nexus_contracts::local::schedule::ScheduleStatus::Failed)
        .await
        .unwrap();

    // Work should still be at intake (no advancement on failure)
    let updated = works::get_work(&pool, "ctr_test", "wrk_fix1b")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.current_stage, "intake", "should NOT advance on failure");
}

#[tokio::test]
async fn fix1_chapter_loop_after_persist() {
    let pool = test_pool().await;
    let sup = test_supervisor(pool.clone());

    // Work at persist/active, chapter 1 of 3
    let mut work = test_work("wrk_fix1c", 1, 3, true);
    work.current_stage = "persist".to_string();
    work.stage_status = "active".to_string();
    seed_work(&pool, &work).await;

    insert_driver_schedule(&pool, "sch_persist_001", "ctr_test", "novel-writing", "running", "wrk_fix1c").await;

    auto_chain::set_driver(&pool, "ctr_test", "wrk_fix1c", "sch_persist_001", "persist")
        .await
        .unwrap();

    // Mark stage as complete
    works::patch_work(
        &pool,
        "ctr_test",
        "wrk_fix1c",
        &works::WorkPatch {
            stage_status: Some("complete".to_string()),
            ..Default::default()
        },
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();

    sup.on_schedule_terminal("sch_persist_001", nexus_contracts::local::schedule::ScheduleStatus::Completed)
        .await
        .unwrap();

    let updated = works::get_work(&pool, "ctr_test", "wrk_fix1c")
        .await
        .unwrap()
        .unwrap();
    // Should advance to produce for chapter 2
    assert_eq!(updated.current_stage, "produce", "should advance to produce for chapter 2");
    assert!(updated.driver_schedule_id.is_some(), "should have a new driver");
}

#[tokio::test]
async fn fix1_last_chapter_marks_work_complete() {
    let pool = test_pool().await;
    let sup = test_supervisor(pool.clone());

    // Work at persist/active, chapter 3 of 3 (last chapter)
    let mut work = test_work("wrk_fix1d", 3, 3, true);
    work.current_stage = "persist".to_string();
    work.stage_status = "active".to_string();
    seed_work(&pool, &work).await;

    insert_driver_schedule(&pool, "sch_last_persist", "ctr_test", "novel-writing", "running", "wrk_fix1d").await;

    auto_chain::set_driver(&pool, "ctr_test", "wrk_fix1d", "sch_last_persist", "persist")
        .await
        .unwrap();

    // Mark stage as complete
    works::patch_work(
        &pool,
        "ctr_test",
        "wrk_fix1d",
        &works::WorkPatch {
            stage_status: Some("complete".to_string()),
            ..Default::default()
        },
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();

    sup.on_schedule_terminal("sch_last_persist", nexus_contracts::local::schedule::ScheduleStatus::Completed)
        .await
        .unwrap();

    let updated = works::get_work(&pool, "ctr_test", "wrk_fix1d")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, "completed", "work should be marked completed");
    assert!(updated.driver_schedule_id.is_none(), "driver should be cleared");
}
