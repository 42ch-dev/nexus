//! V1.51 T-A P2 — finalize-time missing-KB detection integration tests.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.51-missing-kb-detection.md`
//! Spec: `.mstar/knowledge/specs/novel-writing/quality-loop.md` §5.5
//!
//! Covers:
//! - AC1: `novel-writing` schedule completion triggers missing-KB detection.
//! - AC3: `--missing-only` lists missing candidates from the advisory log.
//! - AC4: missing candidates are **not** written to `kb_extract_jobs`.
//! - AC6: existing confirmed `KeyBlock` rows filter out known entities.
//!
//! Run with: cargo test -p nexus-orchestration --test missing_kb_detection

#![allow(clippy::unwrap_used)]

use nexus_kb::key_block::KeyBlock;
use nexus_kb::KbStore;
use nexus_local_db::kb_extract_job::list_pending_for_world;
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::quality_loop;
use sqlx::SqlitePool;

const CREATOR: &str = "ctr_missing_test";
const WORLD: &str = "wld_missing_test";

fn novel_work(work_id: &str, chapter: i32) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: CREATOR.to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "Missing Test Novel".to_string(),
        long_term_goal: "Test missing-KB detection".to_string(),
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
        current_stage: "persist".to_string(),
        stage_status: "complete".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some("missing-test".to_string()),
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
        .prefix("missing_kb_test_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    std::mem::forget(db);

    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    pool
}

async fn seed_world(pool: &SqlitePool) {
    nexus_local_db::kb_store::seed::world(
        pool,
        WORLD,
        CREATOR,
        "Missing Test World",
        "missing-test",
        "private",
        "manual",
    )
    .await;
}

async fn insert_novel_writing_schedule(pool: &SqlitePool, schedule_id: &str, work_id: &str) {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version,
            label, created_at, updated_at, work_id)
           VALUES (?, ?, 'novel-writing', 1, 'running', 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(schedule_id)
    .bind(CREATOR)
    .bind(format!("missing-{work_id}"))
    .bind(now)
    .bind(now)
    .bind(work_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_chapter_with_body(
    pool: &SqlitePool,
    work_id: &str,
    chapter: i32,
    body_rel_path: &str,
) {
    let now = "2026-06-18T10:00:00Z";
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

fn write_workspace_with_chapter(body_text: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let body_rel = "Works/missing-test/Stories/ch02-ch02.md";
    let body_full = dir.path().join(body_rel);
    std::fs::create_dir_all(body_full.parent().unwrap()).unwrap();
    std::fs::write(&body_full, body_text).unwrap();
    (dir, body_rel.to_string())
}

// ── AC1: finalize-time detection writes advisory log ─────────────────────────

#[tokio::test]
async fn ac1_finalize_detection_writes_missing_log() {
    let pool = test_pool().await;
    seed_world(&pool).await;

    let work = novel_work("wrk_ac1", 2);
    works::create_work(&pool, &work).await.unwrap();

    let (ws_dir, body_rel) = write_workspace_with_chapter(
        "Lin Xia walked into the tavern. Marcus Vale waved at Lin Xia.",
    );
    seed_chapter_with_body(&pool, "wrk_ac1", 2, &body_rel).await;
    insert_novel_writing_schedule(&pool, "sch_ac1", "wrk_ac1").await;

    let count =
        quality_loop::detect_missing_kb_on_finalize(&pool, "sch_ac1", Some(ws_dir.path()), None)
            .await
            .unwrap();

    assert!(count >= 2, "expected ≥2 missing candidates, got {count}");

    // Advisory log written under Works/<work_ref>/Logs/kb/missing/.
    let log_dir = ws_dir
        .path()
        .join("Works")
        .join("missing-test")
        .join("Logs")
        .join("kb")
        .join("missing");
    assert!(log_dir.is_dir(), "missing-KB log dir should exist");
    let entries: Vec<_> = std::fs::read_dir(&log_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!entries.is_empty(), "expected at least one log file");

    // Log file name contains chapter.
    let log_path = &entries[0].path();
    let name = log_path.file_name().unwrap().to_string_lossy();
    assert!(
        name.contains("ch2"),
        "log filename should contain chapter, got {name}"
    );

    let body = std::fs::read_to_string(log_path).unwrap();
    assert!(
        body.contains("Lin Xia"),
        "log should mention 'Lin Xia': {body}"
    );
    assert!(
        body.contains("Marcus Vale"),
        "log should mention 'Marcus Vale': {body}"
    );
}

// ── AC4: missing candidates are NOT inserted into kb_extract_jobs ───────────

#[tokio::test]
async fn ac4_missing_candidates_not_written_to_extract_jobs() {
    let pool = test_pool().await;
    seed_world(&pool).await;

    let work = novel_work("wrk_ac4", 2);
    works::create_work(&pool, &work).await.unwrap();

    let (ws_dir, body_rel) =
        write_workspace_with_chapter("Aria Stormblade appeared. Bran met Aria Stormblade.");
    seed_chapter_with_body(&pool, "wrk_ac4", 2, &body_rel).await;
    insert_novel_writing_schedule(&pool, "sch_ac4", "wrk_ac4").await;

    quality_loop::detect_missing_kb_on_finalize(&pool, "sch_ac4", Some(ws_dir.path()), None)
        .await
        .unwrap();

    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert!(
        pending.is_empty(),
        "missing candidates must not become pending rows: {pending:?}"
    );
}

// ── AC6: existing confirmed KeyBlock filters out known entity ────────────────

#[tokio::test]
async fn ac6_existing_key_block_filters_known_entity() {
    let pool = test_pool().await;
    seed_world(&pool).await;

    // Confirm a KeyBlock for "Aria Stormblade" before finalize.
    let store = SqliteKbStore::new(pool.clone());
    let mut kb = KeyBlock::new(
        WORLD,
        nexus_contracts::BlockType::Character,
        "Aria Stormblade",
    );
    kb.status = "confirmed".to_string();
    kb.created_at = chrono::Utc::now().to_rfc3339();
    store.insert_key_block(kb).await.unwrap();

    let work = novel_work("wrk_ac6", 2);
    works::create_work(&pool, &work).await.unwrap();

    let (ws_dir, body_rel) =
        write_workspace_with_chapter("Aria Stormblade appeared. Bran Vale nodded.");
    seed_chapter_with_body(&pool, "wrk_ac6", 2, &body_rel).await;
    insert_novel_writing_schedule(&pool, "sch_ac6", "wrk_ac6").await;

    let count =
        quality_loop::detect_missing_kb_on_finalize(&pool, "sch_ac6", Some(ws_dir.path()), None)
            .await
            .unwrap();

    // "Bran Vale" should be missing; "Aria Stormblade" should be filtered.
    assert!(count >= 1, "expected ≥1 missing candidate, got {count}");

    let log_dir = ws_dir
        .path()
        .join("Works")
        .join("missing-test")
        .join("Logs")
        .join("kb")
        .join("missing");
    let body = std::fs::read_dir(&log_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .next()
        .unwrap();

    assert!(
        body.contains("Bran Vale"),
        "log should mention missing 'Bran Vale': {body}"
    );
    assert!(
        !body.contains("Aria Stormblade"),
        "log should NOT mention confirmed 'Aria Stormblade': {body}"
    );
}

// ── Non-novel-writing schedules are a no-op ─────────────────────────────────

#[tokio::test]
async fn non_novel_writing_schedule_is_noop() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_noop", 1);
    works::create_work(&pool, &work).await.unwrap();

    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version,
            label, created_at, updated_at, work_id)
           VALUES (?, ?, 'novel-review-master', 1, 'running', 'serial', 0, ?, ?, ?, ?)",
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

    let count =
        quality_loop::detect_missing_kb_on_finalize(&pool, "sch_noop", Some(ws_dir.path()), None)
            .await
            .unwrap();
    assert_eq!(count, 0, "non-novel-writing schedule must be a no-op");
}

// ── Missing workspace_dir is a safe no-op ────────────────────────────────────

#[tokio::test]
async fn missing_workspace_dir_is_noop() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_nows", 1);
    works::create_work(&pool, &work).await.unwrap();
    insert_novel_writing_schedule(&pool, "sch_nows", "wrk_nows").await;

    let count = quality_loop::detect_missing_kb_on_finalize(&pool, "sch_nows", None, None)
        .await
        .unwrap();
    assert_eq!(count, 0, "missing workspace_dir must be a safe no-op");
}
