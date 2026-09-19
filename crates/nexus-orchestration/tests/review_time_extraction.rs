//! V1.50 T-B P1 — review-time KB candidate extraction integration tests.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-kb-auto-promotion.md`
//! Spec: `.mstar/specs/novel-writing/cron-staggering.md` §4.4,
//!       `.mstar/specs/entity-scope-model.md` §5.5
//!
//! Covers:
//! - AC2 (#2): review-time extraction hook fires on `novel-review-master`
//!   schedule completion and inserts `kb_extract_jobs` pending rows.
//! - AC6 (#6): idempotency — re-running the hook on the same chapter does not
//!   duplicate pending rows.
//! - Pure heuristic unit tests live in `quality_loop.rs` (`#[cfg(test)]`).
//!
//! Run with: cargo test -p nexus-orchestration --test `review_time_extraction`

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

    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();
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
    // v1.191 P1 T13: the schedule's stored production run identity is what the
    // review-time extraction is admitted under (`current_session_id` FK), so the
    // run row is seeded with it.
    let run_id = format!("run_{schedule_id}");
    // SAFETY: test-only INSERT into orchestration_sessions (the FK target of
    // `creator_schedules.current_session_id`).
    sqlx::query(
        r"INSERT INTO orchestration_sessions
           (session_id, creator_id, preset_id, preset_version, status,
            context_json, created_at, updated_at)
           VALUES (?, ?, 'novel-review-master', 1, 'running', '{}', ?, ?)",
    )
    .bind(&run_id)
    .bind(CREATOR)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    // SAFETY: test-only — DML helper for schedule row insertion.
    sqlx::query(
        r"INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, current_session_id,
            label, created_at, updated_at, work_id)
           VALUES (?, ?, 'novel-review-master', 1, 'running', 'serial', 0, ?, ?, ?, ?, ?)",
    )
    .bind(schedule_id)
    .bind(CREATOR)
    // v1.191 P1 T13: the review-time extraction run is admitted under the
    // schedule's stored production run identity; extraction refuses an empty
    // one instead of inventing a default session.
    .bind(&run_id)
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

// ═══════════════════════════════════════════════════════════════════════════
// v1.191 P1 T13 — the review-time caller runs through the adapter wrapper
// ═══════════════════════════════════════════════════════════════════════════

/// An executor that reports the run identity it was invoked with and answers a
/// fixed extraction payload.
struct MockReviewExtract {
    response: String,
    run_id: std::sync::Mutex<String>,
    /// When set, the executor never answers (a cancelled run).
    pending: bool,
}

#[async_trait::async_trait]
impl nexus_orchestration::capability::PromptExecutor for MockReviewExtract {
    async fn execute(
        &self,
        request: nexus_orchestration::capability::PromptRequest,
    ) -> Result<
        nexus_orchestration::capability::PromptResult,
        nexus_orchestration::capability::CapabilityError,
    > {
        *self.run_id.lock().expect("run id lock") = request.run_id;
        if self.pending {
            std::future::pending::<()>().await;
        }
        Ok(nexus_orchestration::capability::PromptResult {
            full_text: self.response.clone(),
            host_session_id: "host-sess".to_string(),
            operation_id: "op-1".to_string(),
        })
    }
}

/// A registry whose `nexus.llm.extract` runs on the mock executor, with the
/// review hook's stored run identity (`run_<schedule_id>`) registered as a
/// cancellable run.
fn review_registry(schedule_id: &str, executor: std::sync::Arc<dyn nexus_orchestration::capability::PromptExecutor>) -> nexus_orchestration::capability::CapabilityRegistry {
    let mut cancels = std::collections::HashMap::new();
    cancels.insert(
        format!("run_{schedule_id}"),
        tokio_util::sync::CancellationToken::new(),
    );
    let deps = nexus_orchestration::capability::CapabilityRuntimeDeps {
        pool: None,
        prompt_executor: Some(executor),
        session_cancels: std::sync::Arc::new(std::sync::RwLock::new(cancels)),
        daemon_tool_dispatch: None,
        cdn_config: None,
        workspace_executor: None,
    };
    nexus_orchestration::capability::CapabilityRegistry::with_runtime_deps(&deps)
}

async fn pending_rows(pool: &SqlitePool) -> Vec<nexus_local_db::kb_extract_job::KbExtractPromotion> {
    list_pending_for_world(pool, WORLD, None).await.unwrap()
}

async fn relationship_rows(
    pool: &SqlitePool,
) -> Vec<nexus_local_db::kb_relationships::KbRelationshipRow> {
    // `include_suggested` keeps the extraction suggestions visible.
    nexus_local_db::kb_relationships::list_relationships_for_world(pool, WORLD, true, 100)
        .await
        .unwrap()
}

/// The headline T13 case: the review hook extracts through the real caller
/// path (hook → `nexus.llm.extract` → adapter wrapper → `orchestrate_extract`),
/// persists the model's candidates as pending rows and its relationship
/// candidates as suggestions against the entities that exist.
#[tokio::test]
async fn v1191_extract_review_hook_retains_candidate_and_relationship_ids() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_t13_ok", 3);
    works::create_work(&pool, &work).await.unwrap();
    let (ws_dir, body_rel) = write_workspace_with_chapter(
        "Lin Xia drew her blade. Aria and Kael fought together at the Azure Gate.",
    );
    seed_chapter_with_body(&pool, "wrk_t13_ok", 3, &body_rel).await;
    insert_review_master_schedule(&pool, "sch_t13_ok", "wrk_t13_ok").await;

    // The relationship endpoints must already exist as KB rows (the
    // entity-existence prerequisite).
    nexus_local_db::kb_store::seed::knowledge_entry(
        &pool, "kb_t13_aria", WORLD, "character", "Aria", "confirmed",
    )
    .await;
    nexus_local_db::kb_store::seed::knowledge_entry(
        &pool, "kb_t13_kael", WORLD, "character", "Kael", "confirmed",
    )
    .await;

    let executor = std::sync::Arc::new(MockReviewExtract {
        response: serde_json::json!({
            "candidates": [
                {"canonical_name": "Lin Xia", "block_type": "character", "summary": "A warrior", "confidence": 0.93, "source_quote": "Lin Xia drew her blade."},
                {"canonical_name": "Azure Gate", "block_type": "scene", "summary": "The gate", "confidence": 0.8, "source_quote": "at the Azure Gate"}
            ],
            "relationships": [
                {"source_canonical_name": "Aria", "target_canonical_name": "Kael", "relation_type": "allied_with", "symmetric": true, "confidence": 0.7, "source_quote": "Aria and Kael fought together"}
            ]
        })
        .to_string(),
        run_id: std::sync::Mutex::new(String::new()),
        pending: false,
    });
    let registry = review_registry("sch_t13_ok", executor.clone());

    let inserted = quality_loop::extract_kb_candidates_for_review(
        &pool,
        "sch_t13_ok",
        Some(ws_dir.path()),
        Some(&registry),
    )
    .await
    .unwrap();
    assert_eq!(inserted, 2, "both LLM candidates are persisted as pending");

    // The model ran on the real production run identity, not an empty session.
    assert_eq!(
        executor.run_id.lock().expect("run id lock").as_str(),
        "run_sch_t13_ok"
    );

    let pending = pending_rows(&pool).await;
    let names: Vec<&str> = pending
        .iter()
        .filter_map(|c| c.canonical_name_guess.as_deref())
        .collect();
    assert!(names.contains(&"Lin Xia"), "rows: {names:?}");
    assert!(names.contains(&"Azure Gate"), "rows: {names:?}");
    // Candidate ids are retained and the LLM metadata rode along.
    assert!(pending.iter().all(|c| !c.job_id.is_empty()));
    let lin_xia = pending
        .iter()
        .find(|c| c.canonical_name_guess.as_deref() == Some("Lin Xia"))
        .expect("Lin Xia row");
    assert_eq!(lin_xia.block_type_guess.as_deref(), Some("character"));
    assert_eq!(
        lin_xia.llm_source_quote.as_deref(),
        Some("Lin Xia drew her blade.")
    );

    // The relationship sidecar survived SPOKE's KE-only success arm and was
    // resolved against exactly those entities.
    let relationships = relationship_rows(&pool).await;
    assert_eq!(relationships.len(), 1, "one suggestion: {relationships:?}");
    assert_eq!(relationships[0].source_entity_id, "kb_t13_aria");
    assert_eq!(relationships[0].target_entity_id, "kb_t13_kael");
    assert_eq!(relationships[0].relation_type, "allied_with");
    assert!(!relationships[0].relationship_id.is_empty());
}

/// A schedule with no stored run identity extracts nothing: no empty/default
/// session is invented, and neither candidates nor relationships are written.
#[tokio::test]
async fn v1191_extract_review_hook_without_a_run_identity_writes_nothing() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_t13_norun", 1);
    works::create_work(&pool, &work).await.unwrap();
    let (ws_dir, body_rel) = write_workspace_with_chapter("Lin Xia was here.");
    seed_chapter_with_body(&pool, "wrk_t13_norun", 1, &body_rel).await;
    insert_review_master_schedule(&pool, "sch_t13_norun", "wrk_t13_norun").await;
    sqlx::query("UPDATE creator_schedules SET current_session_id = NULL WHERE schedule_id = ?")
        .bind("sch_t13_norun")
        .execute(&pool)
        .await
        .unwrap();

    let executor = std::sync::Arc::new(MockReviewExtract {
        response: "{\"candidates\":[]}".to_string(),
        run_id: std::sync::Mutex::new(String::new()),
        pending: false,
    });
    let registry = review_registry("sch_t13_norun", executor.clone());

    let inserted = quality_loop::extract_kb_candidates_for_review(
        &pool,
        "sch_t13_norun",
        Some(ws_dir.path()),
        Some(&registry),
    )
    .await
    .unwrap();

    assert_eq!(inserted, 0);
    assert!(pending_rows(&pool).await.is_empty());
    assert!(relationship_rows(&pool).await.is_empty());
    assert_eq!(
        executor.run_id.lock().expect("run id lock").as_str(),
        "",
        "the model never ran"
    );
}

/// An incomplete/invalid model terminal is not a worker outage: it writes
/// nothing and never becomes heuristic persistence.
#[tokio::test]
async fn v1191_extract_review_hook_invalid_terminal_writes_nothing() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_t13_bad", 1);
    works::create_work(&pool, &work).await.unwrap();
    let (ws_dir, body_rel) = write_workspace_with_chapter("Lin Xia walked into the tavern.");
    seed_chapter_with_body(&pool, "wrk_t13_bad", 1, &body_rel).await;
    insert_review_master_schedule(&pool, "sch_t13_bad", "wrk_t13_bad").await;

    let executor = std::sync::Arc::new(MockReviewExtract {
        response: "{\"candidates\":[{\"canonical_name\":".to_string(),
        run_id: std::sync::Mutex::new(String::new()),
        pending: false,
    });
    let registry = review_registry("sch_t13_bad", executor);

    let inserted = quality_loop::extract_kb_candidates_for_review(
        &pool,
        "sch_t13_bad",
        Some(ws_dir.path()),
        Some(&registry),
    )
    .await
    .unwrap();

    assert_eq!(inserted, 0);
    assert!(
        pending_rows(&pool).await.is_empty(),
        "an incomplete terminal must not fall back to heuristic candidates"
    );
    assert!(relationship_rows(&pool).await.is_empty());
}

/// A stale target (the Work's World is no longer owned by the stored Creator)
/// refuses before the model runs.
#[tokio::test]
async fn v1191_extract_review_hook_stale_target_writes_nothing() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_t13_stale", 1);
    works::create_work(&pool, &work).await.unwrap();
    let (ws_dir, body_rel) = write_workspace_with_chapter("Lin Xia was here.");
    seed_chapter_with_body(&pool, "wrk_t13_stale", 1, &body_rel).await;
    insert_review_master_schedule(&pool, "sch_t13_stale", "wrk_t13_stale").await;
    // The stored World is now owned by a different creator (the FK target is
    // seeded first), so the hook's stored-state recheck must refuse.
    let other_world = "wld_t13_other";
    nexus_local_db::kb_store::seed::world(
        &pool,
        other_world,
        "ctr_other",
        "Other World",
        "t13-other",
        "private",
        "manual",
    )
    .await;
    sqlx::query("UPDATE narrative_worlds SET owner_creator_id = 'ctr_other' WHERE world_id = ?")
        .bind(WORLD)
        .execute(&pool)
        .await
        .unwrap();

    let executor = std::sync::Arc::new(MockReviewExtract {
        response: "{\"candidates\":[]}".to_string(),
        run_id: std::sync::Mutex::new(String::new()),
        pending: false,
    });
    let registry = review_registry("sch_t13_stale", executor.clone());

    let inserted = quality_loop::extract_kb_candidates_for_review(
        &pool,
        "sch_t13_stale",
        Some(ws_dir.path()),
        Some(&registry),
    )
    .await
    .unwrap();

    assert_eq!(inserted, 0);
    assert!(pending_rows(&pool).await.is_empty());
    assert!(relationship_rows(&pool).await.is_empty());
    assert_eq!(
        executor.run_id.lock().expect("run id lock").as_str(),
        "",
        "a refused target never reaches the model"
    );
}

/// Cancellation before commit: dropping the hook future mid-run writes neither
/// candidates nor relationships.
#[tokio::test]
async fn v1191_extract_review_hook_cancelled_run_writes_nothing() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_t13_cancel", 1);
    works::create_work(&pool, &work).await.unwrap();
    let (ws_dir, body_rel) = write_workspace_with_chapter("Lin Xia was here.");
    seed_chapter_with_body(&pool, "wrk_t13_cancel", 1, &body_rel).await;
    insert_review_master_schedule(&pool, "sch_t13_cancel", "wrk_t13_cancel").await;

    let executor = std::sync::Arc::new(MockReviewExtract {
        response: "{\"candidates\":[]}".to_string(),
        run_id: std::sync::Mutex::new(String::new()),
        pending: true,
    });
    let registry = review_registry("sch_t13_cancel", executor.clone());

    let cancelled = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        quality_loop::extract_kb_candidates_for_review(
            &pool,
            "sch_t13_cancel",
            Some(ws_dir.path()),
            Some(&registry),
        ),
    )
    .await;

    assert!(cancelled.is_err(), "the pending run must be cancelled");
    assert_eq!(
        executor.run_id.lock().expect("run id lock").as_str(),
        "run_sch_t13_cancel",
        "the model run had started"
    );
    assert!(
        pending_rows(&pool).await.is_empty(),
        "a cancelled run writes no candidate"
    );
    assert!(
        relationship_rows(&pool).await.is_empty(),
        "a cancelled run writes no relationship"
    );
}
