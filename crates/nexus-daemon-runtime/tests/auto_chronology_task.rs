//! Hermetic test for the daemon-side auto-chronology task wiring (V1.50 T-A P3).
//!
//! Verifies `auto_chronology::run_one_tick` (the daemon wrapper) threads the
//! workspace path into the orchestration advance, and that
//! `parse_interval_secs` honors the env override value. The orchestration
//! finish-detection + advance logic is covered by
//! `nexus-orchestration::auto_chronology_tick`.

use nexus_daemon_runtime::auto_chronology::{
    parse_interval_secs, run_one_tick, DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS,
};
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

/// `parse_interval_secs` honors the env override value without touching the
/// process-global env (R-V150P3AUTOCHRONO-05 — V1.49 R-V149P1-02 flake pattern).
///
/// The parsing logic is a pure function over `Option<&str>`; tests pass values
/// directly instead of mutating `NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN`, so the
/// test is safe under parallel `cargo test --all` and cannot race with any
/// sibling test that calls `AutoChronologyConfig::from_env`.
#[test]
fn parse_interval_secs_handles_env_values() {
    // Default when unset (None).
    assert_eq!(
        parse_interval_secs(None),
        DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS,
        "default is 5 minutes"
    );

    // Override in minutes.
    assert_eq!(parse_interval_secs(Some("1")), 60, "1 minute = 60 seconds");

    // Invalid value falls back to default.
    assert_eq!(
        parse_interval_secs(Some("garbage")),
        DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS,
        "invalid env falls back to default"
    );

    // Zero is rejected (would busy-loop) → default.
    assert_eq!(
        parse_interval_secs(Some("0")),
        DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS,
        "zero interval falls back to default"
    );
}

/// `AutoChronologyConfig::from_env` reflects the actual process env (production
/// path). This test does NOT mutate the env — it only reads whatever the
/// process inherited (expected unset in the test harness) and asserts the
/// default, proving the production constructor stays wired to `parse_interval_secs`
/// without introducing a global-env-mutation flake.
#[test]
fn from_env_uses_default_when_unset() {
    // Do NOT set/remove the env var here — that would mutate process-global
    // state (R-V150P3AUTOCHRONO-05). We only assert the no-override default.
    let cfg = nexus_daemon_runtime::auto_chronology::AutoChronologyConfig::from_env();
    assert!(
        cfg.interval.as_secs() == DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS
            || std::env::var("NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN").is_ok(),
        "from_env must default to {DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS}s when the env is unset \
         (got {}s; if the env IS set in the harness, that override is respected)",
        cfg.interval.as_secs()
    );
}
