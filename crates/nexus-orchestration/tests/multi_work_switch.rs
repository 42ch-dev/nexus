//! Multi-work lifecycle hermetic tests (DF-60, V1.41 P0 T8).
//!
//! Covers:
//! - TC1: completion ceremony writes lock and updates columns
//! - TC2: other work continues after one is completed
//! - TC3: auto-chain tick skips completion-locked works

#![allow(clippy::unwrap_used)]

use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::auto_chain::{mark_work_completed, ChainAction};
use nexus_orchestration::completion_lock;
use sqlx::SqlitePool;
use tempfile::TempDir;

// ─── Helpers ───────────────────────────────────────────────────────────────

fn test_work(work_id: &str, chapter: i32, total: i32) -> WorkRecord {
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
        created_at: "2026-06-10T10:00:00Z".to_string(),
        updated_at: "2026-06-10T10:00:00Z".to_string(),
        current_stage: "persist".to_string(),
        stage_status: "complete".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(format!("test-{work_id}")),
        total_planned_chapters: Some(total),
        current_chapter: chapter,
        auto_chain_enabled: true,
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

async fn test_pool() -> SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("multi_work_test_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    std::mem::forget(db);

    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    pool
}

async fn seed_work(pool: &SqlitePool, work: &WorkRecord) {
    works::create_work(pool, work).await.unwrap();
}

// ─── TC1: Completion ceremony writes lock + updates pool ────────────────

#[tokio::test]
async fn test_completion_two_step_ceremony_writes_lock_and_updates_pool() {
    let pool = test_pool().await;
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path();

    // Seed a completed-persist work (all chapters finalized)
    let work = test_work("wrk_novel_a", 3, 3);
    seed_work(&pool, &work).await;

    // Step 1: mark_work_completed updates DB columns
    let updated = mark_work_completed(&pool, "ctr_test", "wrk_novel_a")
        .await
        .unwrap();

    assert_eq!(updated.status, "completed");
    assert_eq!(
        updated.novel_completion_status.as_deref(),
        Some("finalize_complete")
    );
    assert!(updated.completion_locked_at.is_some());
    assert!(updated.driver_schedule_id.is_none());

    // Step 2: write_completion_lock_for_work writes the file
    // Create the Works/<work_ref>/ directory first (normally done by daemon)
    let work_dir = workspace_dir.join("Works").join("test-wrk_novel_a");
    std::fs::create_dir_all(&work_dir).unwrap();

    let locked_at = updated.completion_locked_at.as_deref().unwrap();
    nexus_orchestration::auto_chain::write_completion_lock_for_work(
        workspace_dir,
        &updated,
        locked_at,
    )
    .unwrap();

    let lock = completion_lock::read_completion_lock(workspace_dir, "test-wrk_novel_a")
        .unwrap()
        .expect("lock file should exist");
    assert_eq!(lock.work_id, "wrk_novel_a");
    assert_eq!(lock.reason, "completion");

    // Verify DB state: re-read from DB
    let from_db = works::get_work(&pool, "ctr_test", "wrk_novel_a")
        .await
        .unwrap()
        .expect("work should exist");
    assert_eq!(from_db.status, "completed");
    assert!(from_db.completion_locked_at.is_some());
}

// ─── TC2: Other work continues after completion ────────────────────────

#[tokio::test]
#[allow(clippy::similar_names)] // work_a_db / work_b_db are intentionally parallel test fixtures
async fn test_other_work_continues_after_completion() {
    let pool = test_pool().await;

    // Seed two works: novel A (completed) and novel B (still active)
    let work_a = test_work("wrk_novel_a", 3, 3);
    let mut work_b = test_work("wrk_novel_b", 1, 5);
    work_b.current_stage = "produce".to_string();
    work_b.stage_status = "active".to_string();
    seed_work(&pool, &work_a).await;
    seed_work(&pool, &work_b).await;

    // Complete work A
    mark_work_completed(&pool, "ctr_test", "wrk_novel_a")
        .await
        .unwrap();

    // Work B should still be active and unmodified
    let work_b_db = works::get_work(&pool, "ctr_test", "wrk_novel_b")
        .await
        .unwrap()
        .expect("work B should exist");
    assert_eq!(work_b_db.status, "active");
    assert!(work_b_db.completion_locked_at.is_none());

    // Verify work A is completed
    let work_a_db = works::get_work(&pool, "ctr_test", "wrk_novel_a")
        .await
        .unwrap()
        .expect("work A should exist");
    assert_eq!(work_a_db.status, "completed");
    assert!(work_a_db.completion_locked_at.is_some());
}

// ─── TC3: Auto-chain tick skips completion-locked works ─────────────────

#[tokio::test]
async fn test_auto_chain_skips_completion_locked_work() {
    let pool = test_pool().await;

    // Seed a completed work with a driver schedule still set
    let work = test_work("wrk_completed", 3, 3);
    seed_work(&pool, &work).await;

    // Set a driver schedule to simulate an active auto-chain
    nexus_orchestration::auto_chain::set_driver(
        &pool,
        "ctr_test",
        "wrk_completed",
        "sched_driver_1",
        "persist",
    )
    .await
    .unwrap();

    // Complete the work (sets completion_locked_at)
    mark_work_completed(&pool, "ctr_test", "wrk_completed")
        .await
        .unwrap();

    // Evaluate: auto-chain should NOT produce an action for this work
    let record = works::get_work(&pool, "ctr_test", "wrk_completed")
        .await
        .unwrap()
        .expect("work should exist");

    // The supervisor tick would check completion_locked_at before evaluating.
    // Simulate that guard:
    if record.completion_locked_at.is_some() {
        // This is the expected path — the work is skipped.
    } else {
        panic!("completion_locked_at should be set after mark_work_completed");
    }

    // Also verify evaluate_next_step produces NoAction for completed work.
    let action = nexus_orchestration::auto_chain::evaluate_next_step(&record);
    assert!(
        matches!(action, ChainAction::NoAction),
        "completed work should produce NoAction, got: {action:?}"
    );
}
