//! V1.51 T-A P0 — `novel-review-master` review-time LLM extraction E2E.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.51-llm-extraction.md` (T7)
//! Spec: `.mstar/knowledge/specs/llm-extract.md` §5,
//!       `.mstar/knowledge/specs/entity-scope-model.md` §5.5.6
//!
//! Exercises the full review-time pathway for `novel-review-master`:
//! chapter prose → `nexus.llm.extract` (mock worker) → `kb_extract_jobs`
//! pending rows whose `proposed_payload` JSON carries the LLM-extracted
//! `block_type` + `canonical_name` + `confidence` + `source_quote`, and whose
//! dedicated `llm_confidence` / `llm_source_quote` columns are populated.
//!
//! Also covers the heuristic fallback: when no registry is threaded, the hook
//! uses the V1.50 heuristic and the LLM columns stay NULL.
//!
//! Run with: cargo test -p nexus-orchestration --test novel_review_master

#![allow(clippy::unwrap_used)]

use nexus_local_db::kb_extract_job::list_pending_for_world;
use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::capability::{CapabilityRegistry, CapabilityRuntimeDeps};
use nexus_orchestration::quality_loop;
use sqlx::SqlitePool;

const CREATOR: &str = "ctr_v151_e2e";
const WORLD: &str = "wld_v151_e2e";

fn novel_work(work_id: &str, chapter: i32) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: CREATOR.to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "V1.51 LLM Extract Novel".to_string(),
        long_term_goal: "Test LLM extraction".to_string(),
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
        work_ref: Some("v151-extract".to_string()),
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
        .prefix("novel_review_master_v151_")
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
        "V1.51 E2E World",
        "v151-extract",
        "private",
        "manual",
    )
    .await;
}

async fn insert_review_master_schedule(pool: &SqlitePool, schedule_id: &str, work_id: &str) {
    let now = chrono::Utc::now().timestamp();
    // SAFETY: test-only — DML helper for schedule row insertion.
    sqlx::query(
        r"INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version,
            label, created_at, updated_at, work_id)
           VALUES (?, ?, 'novel-review-master', 3, 'running', 'serial', 0, ?, ?, ?, ?)",
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

fn write_workspace_with_chapter(body_text: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let body_rel = "Works/v151-extract/Stories/ch03.md";
    let body_full = dir.path().join(body_rel);
    std::fs::create_dir_all(body_full.parent().unwrap()).unwrap();
    std::fs::write(&body_full, body_text).unwrap();
    (dir, body_rel.to_string())
}

/// Mock worker that returns a fixed LLM extraction payload with a non-character
/// block_type (scene/location) so the test proves the LLM pathway — not the
/// `character`-defaulting heuristic — produced the candidate.
struct MockLlmExtractWorker;

#[async_trait::async_trait]
impl nexus_orchestration::capability::WorkerHandleProvider for MockLlmExtractWorker {
    async fn call_acp_prompt(
        &self,
        _creator_id: &str,
        _session_id: &str,
        _prompt: String,
        _tool_policy: &str,
    ) -> Result<serde_json::Value, nexus_orchestration::capability::CapabilityError> {
        Ok(serde_json::json!({
            "full_text": "{\"candidates\":[
                {\"canonical_name\":\"Lin Xia\",\"block_type\":\"character\",\"summary\":\"A warrior\",\"confidence\":0.95,\"source_quote\":\"Lin Xia drew her blade at the Azure Gate.\"},
                {\"canonical_name\":\"Azure Gate\",\"block_type\":\"scene\",\"summary\":\"The eastern gate\",\"confidence\":0.88,\"source_quote\":\"the Azure Gate groaned open\"}
            ]}"
        }))
    }
}

fn registry_with_mock_worker() -> CapabilityRegistry {
    let deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: Some(std::sync::Arc::new(MockLlmExtractWorker)),
        daemon_tool_dispatch: None,
    };
    CapabilityRegistry::with_runtime_deps(&deps)
}

// ── AC3: novel-review-master LLM pathway writes LLM-extracted payload ──────

/// The headline E2E: review-time extraction via `nexus.llm.extract` produces
/// `kb_extract_jobs` rows whose `proposed_payload` JSON carries all four
/// LLM-extracted keys AND whose dedicated `llm_confidence` / `llm_source_quote`
/// columns are populated. The LLM returned a non-character `block_type`
/// (`scene`), proving the heuristic `character` default was NOT used.
#[tokio::test]
async fn review_master_llm_path_writes_llm_payload() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_v151_llm", 3);
    works::create_work(&pool, &work).await.unwrap();

    let (ws_dir, body_rel) = write_workspace_with_chapter(
        "Lin Xia drew her blade at the Azure Gate. The Azure Gate groaned open.",
    );
    seed_chapter_with_body(&pool, "wrk_v151_llm", 3, &body_rel).await;
    insert_review_master_schedule(&pool, "sch_v151_llm", "wrk_v151_llm").await;

    let registry = registry_with_mock_worker();
    let inserted = quality_loop::extract_kb_candidates_for_review(
        &pool,
        "sch_v151_llm",
        Some(ws_dir.path()),
        Some(&registry),
    )
    .await
    .unwrap();
    assert!(
        inserted >= 2,
        "LLM pathway should insert ≥2 candidates, got {inserted}"
    );

    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();

    // Find the Azure Gate candidate (the non-character one — proves LLM pathway).
    let azure = pending
        .iter()
        .find(|p| p.canonical_name_guess.as_deref() == Some("Azure Gate"))
        .unwrap_or_else(|| panic!("expected 'Azure Gate' candidate in {pending:?}"));

    // Dedicated columns carry the LLM metadata.
    assert_eq!(azure.block_type_guess.as_deref(), Some("scene"));
    assert_eq!(azure.llm_confidence, Some(0.88));
    assert_eq!(
        azure.llm_source_quote.as_deref(),
        Some("the Azure Gate groaned open")
    );

    // proposed_payload JSON carries all four LLM-extracted keys.
    let payload: serde_json::Value =
        serde_json::from_str(azure.proposed_payload.as_deref().unwrap_or("{}"))
            .unwrap();
    assert_eq!(payload["block_type"], "scene", "payload block_type");
    assert_eq!(payload["canonical_name"], "Azure Gate");
    assert_eq!(payload["confidence"], 0.88);
    assert_eq!(payload["source_quote"], "the Azure Gate groaned open");
    assert_eq!(payload["tags"][1], "llm-extracted");
    // novel_category derived from block_type=scene → location.
    assert_eq!(payload["attributes"]["novel_category"], "location");

    // Lin Xia candidate carries the character block_type + its confidence.
    let lin = pending
        .iter()
        .find(|p| p.canonical_name_guess.as_deref() == Some("Lin Xia"))
        .unwrap_or_else(|| panic!("expected 'Lin Xia' candidate in {pending:?}"));
    assert_eq!(lin.block_type_guess.as_deref(), Some("character"));
    assert_eq!(lin.llm_confidence, Some(0.95));
}

// ── Fallback: no registry → heuristic → LLM columns NULL ───────────────────

/// When no registry is threaded, the hook falls back to the V1.50 heuristic:
/// every candidate gets `block_type_guess='character'` and the LLM columns
/// stay NULL. This preserves the V1.50 no-worker behavior.
#[tokio::test]
async fn review_master_no_registry_falls_back_to_heuristic() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_v151_fb", 1);
    works::create_work(&pool, &work).await.unwrap();

    let (ws_dir, body_rel) =
        write_workspace_with_chapter("Lin Xia walked into the tavern.");
    seed_chapter_with_body(&pool, "wrk_v151_fb", 1, &body_rel).await;
    insert_review_master_schedule(&pool, "sch_v151_fb", "wrk_v151_fb").await;

    // No registry → heuristic fallback.
    let inserted = quality_loop::extract_kb_candidates_for_review(
        &pool,
        "sch_v151_fb",
        Some(ws_dir.path()),
        None,
    )
    .await
    .unwrap();
    assert!(inserted >= 1, "heuristic should extract ≥1 candidate");

    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    let lin = pending
        .iter()
        .find(|p| p.canonical_name_guess.as_deref() == Some("Lin Xia"))
        .unwrap();
    // Heuristic defaults: character + NULL LLM columns.
    assert_eq!(lin.block_type_guess.as_deref(), Some("character"));
    assert_eq!(lin.llm_confidence, None, "heuristic → NULL llm_confidence");
    assert_eq!(
        lin.llm_source_quote, None,
        "heuristic → NULL llm_source_quote"
    );
}

// ── Idempotency: re-running the LLM pathway does not duplicate ─────────────

#[tokio::test]
async fn review_master_llm_path_is_idempotent() {
    let pool = test_pool().await;
    seed_world(&pool).await;
    let work = novel_work("wrk_v151_idem", 2);
    works::create_work(&pool, &work).await.unwrap();

    let (ws_dir, body_rel) = write_workspace_with_chapter(
        "Lin Xia drew her blade at the Azure Gate. The Azure Gate groaned open.",
    );
    seed_chapter_with_body(&pool, "wrk_v151_idem", 2, &body_rel).await;
    insert_review_master_schedule(&pool, "sch_v151_idem", "wrk_v151_idem").await;

    let registry = registry_with_mock_worker();

    let count1 = quality_loop::extract_kb_candidates_for_review(
        &pool,
        "sch_v151_idem",
        Some(ws_dir.path()),
        Some(&registry),
    )
    .await
    .unwrap();
    assert!(count1 >= 2, "first run should insert ≥2 candidates");

    let after_first = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    let n_first = after_first.len();

    // Second run — no duplicates (idempotency guard keyed on canonical_name).
    let count2 = quality_loop::extract_kb_candidates_for_review(
        &pool,
        "sch_v151_idem",
        Some(ws_dir.path()),
        Some(&registry),
    )
    .await
    .unwrap();
    assert_eq!(count2, 0, "second run should insert 0 (idempotent)");

    let after_second = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert_eq!(after_second.len(), n_first, "pending count must not change");
}
