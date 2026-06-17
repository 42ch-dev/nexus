//! Hermetic test for the daemon-side cron supervisor background task wiring
//! (V1.50 T-A P1).
//!
//! Verifies that `cron_supervisor::run_one_tick` enqueues cron-fired schedules
//! AND admits them via the supervisor tick in a single call (the daemon's
//! periodic integration path).

use std::sync::Arc;

use nexus_daemon_runtime::cron_supervisor;
use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;

async fn test_pool() -> sqlx::SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("daemon_cron_test_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    std::mem::forget(db);
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    pool
}

fn test_work(work_id: &str) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: "ctr_test".to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "Daemon Cron Test".to_string(),
        long_term_goal: "Test".to_string(),
        initial_idea: "Idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-18T10:00:00Z".to_string(),
        updated_at: "2026-06-18T10:00:00Z".to_string(),
        current_stage: "intake".to_string(),
        stage_status: "complete".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(format!("dcron-{work_id}")),
        total_planned_chapters: Some(3),
        current_chapter: 0,
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

/// `run_one_tick` enqueues a matching cron fire AND admits it to Running in
/// one call — the daemon's periodic integration path (AC2 analog).
#[tokio::test]
async fn run_one_tick_enqueues_and_admits() {
    let pool = Arc::new(test_pool().await);
    let work = test_work("wrk_daemon_tick");
    works::create_work(&pool, &work).await.unwrap();
    let blob = serde_json::json!({
        "tz": "UTC",
        "roles": {
            "brainstorm": {"cron": "* * * * *", "enabled": true},
            "write": {"cron": "0 4 * * *", "enabled": false}
        }
    })
    .to_string();
    works::set_schedule_json(&pool, "wrk_daemon_tick", &blob, "2026-06-18T10:00:00Z")
        .await
        .unwrap();

    let supervisor = Arc::new(ScheduleSupervisor::new(pool.clone()));
    cron_supervisor::run_one_tick(&pool, &supervisor).await;

    let status: String = sqlx::query_scalar(
        "SELECT status FROM creator_schedules \
         WHERE work_id = 'wrk_daemon_tick' AND preset_id = 'novel-brainstorm'",
    )
    .fetch_one(&*pool)
    .await
    .unwrap();
    assert_eq!(
        status, "running",
        "run_one_tick should enqueue + admit the cron-fired schedule"
    );
}

/// An idle tick (no cron match) does not enqueue or admit anything.
#[tokio::test]
async fn run_one_tick_no_match_is_noop() {
    let pool = Arc::new(test_pool().await);
    let work = test_work("wrk_daemon_idle");
    works::create_work(&pool, &work).await.unwrap();
    // `0 3 * * *` — fires only at 03:00; unlikely to match the test's `now`.
    let blob = serde_json::json!({
        "tz": "UTC",
        "roles": {
            "brainstorm": {"cron": "0 3 * * *", "enabled": true},
            "write": {"cron": "0 4 * * *", "enabled": true}
        }
    })
    .to_string();
    works::set_schedule_json(&pool, "wrk_daemon_idle", &blob, "2026-06-18T10:00:00Z")
        .await
        .unwrap();

    let supervisor = Arc::new(ScheduleSupervisor::new(pool.clone()));
    cron_supervisor::run_one_tick(&pool, &supervisor).await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM creator_schedules WHERE work_id = 'wrk_daemon_idle'",
    )
    .fetch_one(&*pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "non-matching cron must not enqueue");
}
