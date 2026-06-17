//! Hermetic test for the daemon-side auto-chronology task wiring (V1.50 T-A P3).
//!
//! Verifies `auto_chronology::run_one_tick` (the daemon wrapper) threads the
//! workspace path into the orchestration advance, and that
//! `AutoChronologyConfig::from_env` honors the env override. The orchestration
//! finish-detection + advance logic is covered by
//! `nexus-orchestration::auto_chronology_tick`.

use nexus_daemon_runtime::auto_chronology::{run_one_tick, AutoChronologyConfig};
use nexus_local_db::works::{self, WorkRecord};

async fn test_pool() -> sqlx::SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("daemon_chrono_test_")
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
        title: "Daemon Chrono Test".to_string(),
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
        current_stage: "produce".to_string(),
        stage_status: "complete".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(format!("dchron-{work_id}")),
        total_planned_chapters: Some(2),
        current_chapter: 2,
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

/// `run_one_tick` advances an eligible opted-in Work, writing the outline under
/// the workspace dir passed by the daemon wrapper.
#[tokio::test]
async fn daemon_run_one_tick_advances_eligible_work() {
    let pool = test_pool().await;
    let ws = tempfile::tempdir().unwrap();
    let work = test_work("wrk_dchron");
    works::create_work(&pool, &work).await.unwrap();
    // Seed + finalize volume 1.
    for ch in 1..=2 {
        let slug = format!("v01-ch{ch:02}");
        let params = nexus_local_db::work_chapters::InsertChapterParams {
            work_id: "wrk_dchron",
            chapter: ch,
            volume: Some(1),
            slug: Some(&slug),
            planned_word_count: 4000,
            outline_path: None,
            body_path: None,
            now: "2026-06-18T10:00:00Z",
        };
        nexus_local_db::work_chapters::insert_chapter(&pool, &params)
            .await
            .unwrap();
        nexus_local_db::work_chapters::update_status(
            &pool,
            "wrk_dchron",
            ch,
            1,
            "finalized",
            Some(4000),
            "2026-06-18T10:30:00Z",
        )
        .await
        .unwrap();
    }
    works::set_auto_chronology(&pool, "wrk_dchron", true, "2026-06-18T10:00:00Z")
        .await
        .unwrap();

    run_one_tick(&pool, Some(ws.path())).await;

    let outline = ws
        .path()
        .join("Works")
        .join("dchron-wrk_dchron")
        .join("Outlines")
        .join("volume-2-outline.md");
    assert!(
        outline.exists(),
        "daemon tick must create the volume-2 outline under the workspace"
    );
}

/// `AutoChronologyConfig::from_env` honors `NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN`.
#[test]
fn config_env_override_in_minutes() {
    // Default when unset.
    std::env::remove_var("NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN");
    let cfg = AutoChronologyConfig::from_env();
    assert_eq!(cfg.interval.as_secs(), 5 * 60, "default is 5 minutes");

    // Override in minutes.
    std::env::set_var("NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN", "1");
    let cfg = AutoChronologyConfig::from_env();
    assert_eq!(cfg.interval.as_secs(), 60, "1 minute = 60 seconds");
    std::env::remove_var("NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN");

    // Invalid value falls back to default.
    std::env::set_var("NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN", "garbage");
    let cfg = AutoChronologyConfig::from_env();
    assert_eq!(cfg.interval.as_secs(), 5 * 60, "invalid env falls back to default");
    std::env::remove_var("NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN");

    // Zero is rejected (would busy-loop) → default.
    std::env::set_var("NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN", "0");
    let cfg = AutoChronologyConfig::from_env();
    assert_eq!(cfg.interval.as_secs(), 5 * 60, "zero interval falls back to default");
    std::env::remove_var("NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN");
}
