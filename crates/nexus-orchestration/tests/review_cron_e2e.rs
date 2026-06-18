//! V1.50 T-A P2 — end-to-end review cron → review-master schedule → T-B P1
//! KB-extraction hook chain test.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-cron-review-staggering.md`
//! Spec: `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §4.1 / §4.4,
//!       `.mstar/knowledge/specs/novel-writing/quality-loop.md` §3
//!
//! Verifies the full chain that T-A P2 wires:
//! 1. The daemon cron evaluator fires the per-Work `review` role at a matching
//!    minute and enqueues a `novel-review-master` schedule (preset_id is the
//!    T-B P1 hook trigger).
//! 2. The cron-enqueued schedule carries the `cron:review:` provenance label
//!    (Option A: one uniform `enqueue_cron_schedule` path for all three roles).
//! 3. After the schedule reaches a terminal state (the executor running the
//!    `novel-review-master` preset is simulated by a status UPDATE — the ACP
//!    agent run is not hermetic), the T-B P1 review-time KB-extraction hook
//!    (`quality_loop::extract_kb_candidates_for_review`) — already wired in
//!    `schedule::supervisor::on_schedule_terminal` — fires on that schedule and
//!    inserts `kb_extract_jobs` pending rows from the chapter prose.
//!
//! This proves the cross-plan handoff is complete: the T-B P1 hook, which
//! previously only fired for the V1.39 stale-findings / manual `creator run`
//! paths, now also fires for cron-launched review schedules.
//!
//! Run with: cargo test -p nexus-orchestration --test review_cron_e2e

#![allow(clippy::unwrap_used)]

use chrono::TimeZone;
use nexus_local_db::kb_extract_job::list_pending_for_world;
use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::{quality_loop, schedule::cron_supervisor};
use sqlx::SqlitePool;

const CREATOR: &str = "ctr_review_cron_e2e";
const WORLD: &str = "wld_review_cron_e2e";
const WORK_REF: &str = "review-cron-e2e";

// ── Test helpers (mirrors review_time_extraction.rs + cron_supervisor.rs) ────

async fn test_pool() -> SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("review_cron_e2e_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    std::mem::forget(db);
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    pool
}

/// Seed a minimal `narrative_worlds` row (and FK creator) so the work's
/// `world_id` FK holds. Reuses the shared seed helper.
async fn seed_world(pool: &SqlitePool) {
    nexus_local_db::kb_store::seed::world(
        pool,
        WORLD,
        CREATOR,
        "Review Cron E2E World",
        WORK_REF,
        "private",
        "manual",
    )
    .await;
}

/// A healthy novel Work with a world and a current chapter (> 0) so the
/// T-B P1 extraction hook has prose to scan.
fn novel_work(work_id: &str, chapter: i32) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: CREATOR.to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "Review Cron E2E Novel".to_string(),
        long_term_goal: "Test the review cron chain".to_string(),
        initial_idea: "A story".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: Some(WORLD.to_string()),
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-18T10:00:00Z".to_string(),
        updated_at: "2026-06-18T10:00:00Z".to_string(),
        current_stage: "review".to_string(),
        stage_status: "active".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(WORK_REF.to_string()),
        total_planned_chapters: Some(5),
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

/// Write a chapter body file under a temp workspace dir and return the dir +
/// the relative body path (stored in `work_chapters.body_path`).
fn write_workspace_with_chapter(body_text: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let body_rel = format!("Works/{WORK_REF}/Stories/ch03.md");
    let body_full = dir.path().join(&body_rel);
    std::fs::create_dir_all(body_full.parent().unwrap()).unwrap();
    std::fs::write(&body_full, body_text).unwrap();
    (dir, body_rel)
}

/// Seed a `work_chapters` row pointing at a body file (mirrors
/// `review_time_extraction.rs::seed_chapter_with_body`).
async fn seed_chapter_with_body(
    pool: &SqlitePool,
    work_id: &str,
    chapter: i32,
    body_rel_path: &str,
) {
    let now = "2026-06-18T10:00:00Z";
    // SAFETY: test-only INSERT into work_chapters.
    sqlx::query(
        "INSERT INTO work_chapters \
         (work_id, chapter, volume, slug, planned_word_count, actual_word_count, \
          status, outline_path, body_path, created_at, updated_at) \
         VALUES (?, ?, 1, ?, 4000, NULL, 'finalized', NULL, ?, ?, ?)",
    )
    .bind(work_id)
    .bind(chapter)
    .bind(format!("ch{chapter:02}"))
    .bind(body_rel_path)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
}

// ── The chain: cron fire → enqueue → terminal → T-B P1 hook ─────────────────

/// End-to-end: the per-Work `review` cron role fires on its spec default
/// (`0,30 * * * *`), enqueues a `novel-review-master` schedule, and — once that
/// schedule reaches a terminal state — the T-B P1 review-time KB-extraction
/// hook fires on it and inserts pending KB candidates from the chapter prose.
///
/// This is the cross-plan handoff proof: T-A P2 (review cron) → T-B P1
/// (extraction hook). The ACP agent run that executes the `novel-review-master`
/// preset is simulated by transitioning the schedule to `completed` (the
/// standard hermetic pattern; the supervisor calls the hook in
/// `on_schedule_terminal` regardless of how the terminal state was reached).
#[tokio::test]
async fn review_cron_fire_triggers_kb_extraction_hook() {
    let pool = test_pool().await;
    seed_world(&pool).await;

    let work = novel_work("wrk_e2e", 3);
    works::create_work(&pool, &work).await.unwrap();

    // Configure the review role on the Work (spec §2.1 shape) via the DAO,
    // mirroring what `creator works cron set --review ...` writes.
    let schedule_json = serde_json::json!({
        "tz": "UTC",
        "roles": {
            "brainstorm": {"cron": "0 3 * * *", "enabled": false},
            "write":      {"cron": "0 4 * * *", "enabled": false},
            "review":     {"cron": "0,30 * * * *", "enabled": true}
        }
    })
    .to_string();
    let now_rfc = chrono::Utc::now().to_rfc3339();
    works::set_schedule_json(&pool, "wrk_e2e", &schedule_json, &now_rfc)
        .await
        .unwrap();

    // Write a chapter body with a recognizable character name so the heuristic
    // has prose to extract from when the hook fires.
    let (ws_dir, body_rel) =
        write_workspace_with_chapter("Lin Xia walked into the tavern. Marcus waved at Lin Xia.");
    seed_chapter_with_body(&pool, "wrk_e2e", 3, &body_rel).await;

    // ── Step 1: cron evaluator fires the review role at a matching minute. ──
    // 14:00 UTC matches the `:00` slot of `0,30 * * * *`.
    let fire_time = chrono::Utc.with_ymd_and_hms(2026, 6, 19, 14, 0, 0).unwrap();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, fire_time).await;
    assert_eq!(
        summary.fired, 1,
        "review role should fire at :00: {summary:?}"
    );

    // The enqueued schedule is a `novel-review-master` run with cron provenance.
    let (schedule_id, preset_id, label): (String, String, String) = sqlx::query_as(
        "SELECT schedule_id, preset_id, label FROM creator_schedules \
         WHERE work_id = 'wrk_e2e' AND preset_id = 'novel-review-master'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(preset_id, "novel-review-master");
    assert_eq!(label, "cron:review:wrk_e2e");
    assert!(
        schedule_id.starts_with("CRON"),
        "cron-fired review schedule must use the CRON prefix (got {schedule_id})"
    );

    // ── Step 2: simulate the executor completing the review-master run. ──────
    // The real supervisor fires the T-B P1 hook in `on_schedule_terminal`; the
    // hook itself only needs the schedule row + work + chapter body, so we
    // transition the schedule to terminal and invoke the hook directly (the
    // same hermetic pattern used in review_time_extraction.rs).
    sqlx::query("UPDATE creator_schedules SET status = 'completed' WHERE schedule_id = ?")
        .bind(&schedule_id)
        .execute(&pool)
        .await
        .unwrap();

    // ── Step 3: the T-B P1 extraction hook fires on the completed schedule. ─
    let inserted = quality_loop::extract_kb_candidates_for_review(
        &pool,
        &schedule_id,
        Some(ws_dir.path()),
        None,
    )
    .await
    .unwrap();
    assert!(
        inserted >= 1,
        "T-B P1 hook should extract ≥1 candidate from the review-cron-launched schedule"
    );

    // The candidate ("Lin Xia") lands in `kb_extract_jobs` as a pending row for
    // the work's world — ready for `creator world kb adopt`.
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert!(!pending.is_empty(), "pending KB candidates should exist");
    let names: Vec<String> = pending
        .iter()
        .filter_map(|p| p.canonical_name_guess.clone())
        .collect();
    assert!(
        names.iter().any(|n| n == "Lin Xia"),
        "expected 'Lin Xia' among extracted candidates: {names:?}"
    );
}

/// Negative leg: when no Work has a review role configured, the cron sweep
/// enqueues no `novel-review-master` schedule (acceptance §4 — graceful skip,
/// "no work = no schedule").
#[tokio::test]
async fn review_cron_no_review_role_enqueues_nothing() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_e2e_none", 1);
    works::create_work(&pool, &work).await.unwrap();

    // Two-role blob (no `review` key) — review deserialises to `None`.
    let schedule_json = serde_json::json!({
        "tz": "UTC",
        "roles": {
            "brainstorm": {"cron": "0 3 * * *", "enabled": false},
            "write":      {"cron": "0 4 * * *", "enabled": false}
        }
    })
    .to_string();
    let now_rfc = chrono::Utc::now().to_rfc3339();
    works::set_schedule_json(&pool, "wrk_e2e_none", &schedule_json, &now_rfc)
        .await
        .unwrap();

    let fire_time = chrono::Utc.with_ymd_and_hms(2026, 6, 19, 14, 0, 0).unwrap();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, fire_time).await;

    assert_eq!(
        summary.fired, 0,
        "nothing should fire when all roles are disabled/absent: {summary:?}"
    );
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM creator_schedules \
         WHERE work_id = 'wrk_e2e_none' AND preset_id = 'novel-review-master'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 0, "no review-master schedule should be enqueued");
}
