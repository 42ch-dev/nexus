//! V1.50 T-B P1 — review-time KB candidate extraction integration tests.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-kb-auto-promotion.md`
//! Spec: `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §4.4,
//!       `.mstar/knowledge/specs/entity-scope-model.md` §5.5
//!
//! Covers:
//! - AC2 (#2): review-time extraction hook fires on `novel-review-master`
//!   schedule completion and inserts `kb_extract_jobs` pending rows.
//! - AC6 (#6): idempotency — re-running the hook on the same chapter does not
//!   duplicate pending rows.
//! - Pure heuristic unit tests live in `quality_loop.rs` (`#[cfg(test)]`).
//!
//! Run with: cargo test -p nexus-orchestration --test review_time_extraction

#![allow(clippy::unwrap_used)]

use nexus_local_db::kb_extract_job::list_pending_for_world;
use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::quality_loop;
use sqlx::SqlitePool;

const CREATOR: &str = "ctr_extract_test";
const WORLD: &str = "wld_extract_test";

fn novel_work(work_id: &str, chapter: i32) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: CREATOR.to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "Extract Test Novel".to_string(),
        long_term_goal: "Test extraction".to_string(),
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
        work_ref: Some("extract-test".to_string()),
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

async fn test_pool() -> SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("review_extract_test_")
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
        "Extract Test World",
        "extract-test",
        "private",
        "manual",
    )
    .await;
}

/// Seed a `creator_schedules` row for a `novel-review-master` run.
async fn insert_review_master_schedule(pool: &SqlitePool, schedule_id: &str, work_id: &str) {
    let now = chrono::Utc::now().timestamp();
    // SAFETY: test-only — DML helper for schedule row insertion.
    sqlx::query(
        r"INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version,
            label, created_at, updated_at, work_id)
           VALUES (?, ?, 'novel-review-master', 1, 'running', 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(schedule_id)
    .bind(CREATOR)
    .bind(format!("kb-extract-{work_id}"))
    .bind(now)
    .bind(now)
    .bind(work_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a `work_chapters` row pointing at a body file written under `ws_dir`.
async fn seed_chapter_with_body(
    pool: &SqlitePool,
    work_id: &str,
    chapter: i32,
    body_rel_path: &str,
) {
    // `work_chapters.created_at`/`updated_at` are declared INTEGER but seeded
    // with ISO-8601 strings in production (SQLite type affinity stores them as
    // TEXT); match that so the DAO's String decode succeeds.
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

/// Write a chapter body file under `ws_dir` and return the workspace dir.
fn write_workspace_with_chapter(body_text: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let body_rel = "Works/extract-test/Stories/ch03-ch03.md";
    let body_full = dir.path().join(body_rel);
    std::fs::create_dir_all(body_full.parent().unwrap()).unwrap();
    std::fs::write(&body_full, body_text).unwrap();
    (dir, body_rel.to_string())
}

// ── AC2: review-time extraction fires on novel-review-master completion ─────

#[tokio::test]
async fn ac2_extraction_inserts_pending_candidates() {
    let pool = test_pool().await;
    seed_world(&pool).await;

    let work = novel_work("wrk_ac2", 3);
    works::create_work(&pool, &work).await.unwrap();

    let (ws_dir, body_rel) =
        write_workspace_with_chapter("Lin Xia walked into the tavern. Marcus waved at Lin Xia.");
    seed_chapter_with_body(&pool, "wrk_ac2", 3, &body_rel).await;
    insert_review_master_schedule(&pool, "sch_ac2", "wrk_ac2").await;

    let count =
        quality_loop::extract_kb_candidates_for_review(&pool, "sch_ac2", Some(ws_dir.path()), None)
            .await
            .unwrap();

    // "Lin Xia" and "Marcus" should be extracted.
    assert!(count >= 1, "expected ≥1 candidate, got {count}");

    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert!(!pending.is_empty());
    let names: Vec<String> = pending
        .iter()
        .filter_map(|p| p.canonical_name_guess.clone())
        .collect();
    assert!(
        names.iter().any(|n| n == "Lin Xia"),
        "expected 'Lin Xia' in {names:?}"
    );
}

// ── AC6: idempotency — re-running does not duplicate ─────────────────────────

#[tokio::test]
async fn ac6_rerun_does_not_duplicate_pending() {
    let pool = test_pool().await;
    seed_world(&pool).await;

    let work = novel_work("wrk_ac6", 1);
    works::create_work(&pool, &work).await.unwrap();

    let (ws_dir, body_rel) =
        write_workspace_with_chapter("Aria Stormblade appeared. Aria Stormblade spoke.");
    seed_chapter_with_body(&pool, "wrk_ac6", 1, &body_rel).await;
    insert_review_master_schedule(&pool, "sch_ac6", "wrk_ac6").await;

    // First run.
    let count1 =
        quality_loop::extract_kb_candidates_for_review(&pool, "sch_ac6", Some(ws_dir.path()), None)
            .await
            .unwrap();
    assert!(count1 >= 1, "first run should extract ≥1 candidate");

    let after_first = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    let n_first = after_first.len();

    // Second run on the same schedule (simulates re-fire) — no duplicates.
    let count2 =
        quality_loop::extract_kb_candidates_for_review(&pool, "sch_ac6", Some(ws_dir.path()), None)
            .await
            .unwrap();
    assert_eq!(count2, 0, "second run should insert 0 (idempotent)");

    let after_second = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert_eq!(
        after_second.len(),
        n_first,
        "pending count must not change on re-run"
    );
}

// ── Non-review-master schedules are a no-op ─────────────────────────────────

#[test]
fn pure_heuristic_extracts_character_names() {
    let candidates =
        quality_loop::extract_candidates_from_text("Lin Xia met Captain Holdo at the docks.");
    let names: Vec<String> = candidates
        .into_iter()
        .map(|c| c.canonical_name_guess)
        .collect();
    assert!(
        names.iter().any(|n| n == "Lin Xia"),
        "expected 'Lin Xia' in {names:?}"
    );
    assert!(
        names.iter().any(|n| n.contains("Holdo")),
        "expected a Holdo match in {names:?}"
    );
}

#[tokio::test]
async fn non_review_master_schedule_is_noop() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_noop", 1);
    works::create_work(&pool, &work).await.unwrap();

    // Insert a non-review-master schedule.
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version,
            label, created_at, updated_at, work_id)
           VALUES (?, ?, 'novel-writing', 1, 'running', 'serial', 0, ?, ?, ?, ?)",
    )
    .bind("sch_noop")
    .bind(CREATOR)
    .bind("noop")
    .bind(now)
    .bind(now)
    .bind("wrk_noop")
    .execute(&pool)
    .await
    .unwrap();

    let (ws_dir, body_rel) = write_workspace_with_chapter("Lin Xia was here.");
    seed_chapter_with_body(&pool, "wrk_noop", 1, &body_rel).await;

    let count = quality_loop::extract_kb_candidates_for_review(
        &pool,
        "sch_noop",
        Some(ws_dir.path()),
        None,
    )
    .await
    .unwrap();
    assert_eq!(count, 0, "non-review-master schedule must be a no-op");
}

// ── Missing workspace_dir is a safe no-op ───────────────────────────────────

#[tokio::test]
async fn missing_workspace_dir_is_noop() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_nows", 1);
    works::create_work(&pool, &work).await.unwrap();
    insert_review_master_schedule(&pool, "sch_nows", "wrk_nows").await;

    let count = quality_loop::extract_kb_candidates_for_review(&pool, "sch_nows", None, None)
        .await
        .unwrap();
    assert_eq!(count, 0, "missing workspace_dir must be a safe no-op");
}
