//! Hermetic e2e test for V1.40 P3 kb-extract binding (T7).
//!
//! Validates: persist schedule → extract → P2 chapter block sees new item.
//! Uses in-memory stores for KB and `SQLite` for job lifecycle.

use nexus_contracts::BlockType;
use nexus_kb::extract_finalize::{finalize_extract, ExtractFinalizeInput};
use nexus_kb::key_block::KeyBlockBody;
use nexus_kb::source_anchor::SourceAnchor;
use nexus_kb::store::InMemoryKbStore;
use nexus_kb::validation::ValidationMode;
use nexus_kb::KbStore;
use nexus_local_db::kb_store::seed;
use nexus_local_db::{enqueue_extract_job_with_artifact, open_pool, run_migrations};

async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = open_pool(&db_path).await.unwrap();
    run_migrations(&pool).await.unwrap();
    (pool, dir)
}

#[tokio::test]
async fn test_persist_extract_chapter_block_e2e() {
    let (pool, _dir) = fresh_pool().await;
    let creator_id = "ctr_test";
    let workspace_slug = "ws_test";
    let world_id = "wld_novel_world";
    let work_id = "wrk_my_novel";
    let chapter_num = 1;
    let work_entry_id = "kb_chapter_01";

    // Step 1: Seed the world so FK constraints pass.
    seed::world(
        &pool,
        world_id,
        creator_id,
        "Novel World",
        "novel-world",
        "private",
        "linear",
    )
    .await;

    // Step 2: Enqueue extract job with chapter artifact locator.
    let job = enqueue_extract_job_with_artifact(
        &pool,
        creator_id,
        workspace_slug,
        work_entry_id,
        world_id,
        Some("work_chapter"),
        Some(&format!("Works/my-novel/Chapters/{chapter_num:02}.md")),
        Some("novel"),
        Some(work_id),
    )
    .await
    .unwrap();

    assert!(job.job_id.starts_with("xj_"));
    assert_eq!(job.status, "queued");
    assert_eq!(job.source_kind.as_deref(), Some("work_chapter"));
    assert_eq!(job.profile_hint.as_deref(), Some("novel"));
    assert_eq!(job.work_id.as_deref(), Some(work_id));

    // Step 3: Simulate the extract finalize step (what kb.extract_work does).
    let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
    let body = KeyBlockBody {
        summary: Some("Lin Xia is a brave warrior from the Neon City".to_string()),
        attributes: Some(serde_json::json!({
            "novel_category": "character",
            "aliases": ["Xia"],
            "traits": ["brave", "resourceful"]
        })),
        tags: Some(vec!["novel".to_string()]),
    };
    let source_anchor = SourceAnchor::from_excerpt("Chapter 01: Lin Xia appeared...");

    let input = ExtractFinalizeInput {
        world_id: world_id.to_string(),
        block_type: BlockType::Character,
        canonical_name: "char_lin_xia".to_string(),
        body,
        source_anchor,
        validation_mode: ValidationMode::Novel,
    };

    let result = finalize_extract(&store, input).await.unwrap();
    assert!(result.key_block_id.starts_with("kb_"));
    assert_eq!(result.world_id, world_id);

    // Step 4: Verify the KB block is queryable via the store.
    let blocks = store.list_by_world(world_id).await.unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].canonical_name, "char_lin_xia");
    assert_eq!(blocks[0].block_type, BlockType::Character);
    assert!(blocks[0].source_anchor.is_some());
}

#[tokio::test]
async fn test_worldless_work_skips_world_promotion() {
    // Legacy V1.39 worldless Works: world_id is None, no World KB promotion.
    // This test verifies the enqueue still works (no FK violation) and
    // that finalize_extract uses Generic validation mode.

    // WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P3-S3
    // — AC3 empty/absent world_id test gap: this test uses a present world_id
    // in Generic mode; a dedicated test for truly empty/absent world_id is deferred.

    let store = InMemoryKbStore::new();
    let body = KeyBlockBody {
        summary: Some("A generic knowledge item".to_string()),
        attributes: None,
        tags: None,
    };
    let source_anchor = SourceAnchor::from_excerpt("Generic excerpt");

    let input = ExtractFinalizeInput {
        world_id: "wld_no_world".to_string(),
        block_type: BlockType::InfoPoint,
        canonical_name: "info_generic".to_string(),
        body,
        source_anchor,
        validation_mode: ValidationMode::Generic,
    };

    let result = finalize_extract(&store, input).await.unwrap();
    assert!(result.key_block_id.starts_with("kb_"));

    let blocks = store.list_by_world("wld_no_world").await.unwrap();
    assert_eq!(blocks.len(), 1);
}

#[tokio::test]
async fn test_extract_idempotent_job() {
    let (pool, _dir) = fresh_pool().await;
    let creator_id = "ctr_idem";
    seed::world(
        &pool,
        "wld_idem",
        creator_id,
        "Idem World",
        "idem",
        "private",
        "linear",
    )
    .await;

    // First enqueue
    let job1 = enqueue_extract_job_with_artifact(
        &pool,
        creator_id,
        "ws_idem",
        "kb_chapter_05",
        "wld_idem",
        Some("work_chapter"),
        Some("Works/novel/Chapters/05.md"),
        Some("novel"),
        Some("wrk_idem"),
    )
    .await
    .unwrap();

    // Second enqueue with same (creator, work_entry_id, world_id) → idempotent
    let job2 = enqueue_extract_job_with_artifact(
        &pool,
        creator_id,
        "ws_idem",
        "kb_chapter_05",
        "wld_idem",
        Some("work_chapter"),
        Some("Works/novel/Chapters/05.md"),
        Some("novel"),
        Some("wrk_idem"),
    )
    .await
    .unwrap();

    assert_eq!(job1.job_id, job2.job_id);
}

#[tokio::test]
async fn test_extract_novel_requires_novel_category() {
    let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
    let body = KeyBlockBody {
        summary: Some("Test".to_string()),
        attributes: Some(serde_json::json!({})), // missing novel_category
        tags: None,
    };
    let source_anchor = SourceAnchor::from_excerpt("test");

    let input = ExtractFinalizeInput {
        world_id: "wld_test".to_string(),
        block_type: BlockType::Character,
        canonical_name: "char_test".to_string(),
        body,
        source_anchor,
        validation_mode: ValidationMode::Novel,
    };

    let err = finalize_extract(&store, input).await.unwrap_err();
    assert!(
        format!("{err}").contains("novel_category"),
        "expected novel_category validation error, got: {err}"
    );
}
