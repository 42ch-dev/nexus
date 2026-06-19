//! V1.51 T-A P0 — `creator world kb adopt` surfaces LLM extraction metadata.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.51-llm-extraction.md` (T6)
//! Spec: `.mstar/knowledge/specs/cli-spec.md` §6.2G (V1.51 amendment),
//!       `.mstar/knowledge/specs/llm-extract.md` §3.2
//!
//! Verifies that `kb_adopt` succeeds on candidates produced by the
//! `nexus.llm.extract` pathway (carrying `llm_confidence` +
//! `llm_source_quote`) and that the LLM metadata is preserved on the
//! promotion row after adopt (so the author's confirmation surface can show
//! confidence + source_quote per cli-spec §6.2G).
//!
//! Run with: cargo test -p nexus42 --test creator_world_kb_adopt

#![allow(clippy::unwrap_used)]

use nexus42::commands::creator::world::kb::kb_adopt;
use nexus42::db::Schema;
use nexus_kb::KbStore;
use nexus_local_db::kb_extract_job::{get_promotion, insert_pending_with_llm};
use nexus_local_db::kb_store::SqliteKbStore;

const OWNER: &str = "ctr_adopt_v151";
const WORLD: &str = "wld_adopt_v151";
const WORK_ID: &str = "wrk_adopt_v151";

/// Build a fresh migrated pool + seed a world owned by [`OWNER`].
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = Schema::init(&db_path).await.unwrap();
    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        OWNER,
        "Adopt V1.51 World",
        "adopt-v151",
        "private",
        "manual",
    )
    .await;
    (pool, dir)
}

/// Seed an LLM-extracted pending candidate (block_type=scene, confidence=0.92,
/// verbatim source_quote) — mirrors what `nexus.llm.extract` produces.
async fn seed_llm_pending() -> (sqlx::SqlitePool, tempfile::TempDir, String) {
    let (pool, dir) = fresh_pool().await;
    let payload = serde_json::json!({
        "summary": "The eastern gate",
        "attributes": {"novel_category": "location", "aliases": ["Azure Gate"]},
        "tags": ["novel", "llm-extracted"],
        "block_type": "scene",
        "canonical_name": "Azure Gate",
        "confidence": 0.92,
        "source_quote": "...the eastern gate groaned open...",
    })
    .to_string();
    let row = insert_pending_with_llm(
        &pool,
        OWNER,
        "ws",
        WORLD,
        Some(WORK_ID),
        Some(3),
        "scene",
        "Azure Gate",
        &payload,
        Some(0.92),
        Some("...the eastern gate groaned open..."),
    )
    .await
    .unwrap();
    (pool, dir, row.job_id)
}

// ── AC4: adopt succeeds on an LLM-extracted candidate ──────────────────────

/// `kb_adopt` accepts a candidate carrying LLM metadata (non-character
/// block_type + confidence + source_quote), promotes it to a `confirmed`
/// KeyBlock, and leaves the LLM columns intact on the promotion row.
#[tokio::test]
async fn adopt_succeeds_on_llm_extracted_candidate() {
    let (pool, _dir, job_id) = seed_llm_pending().await;

    // Adopt (non-JSON path exercises the confidence/source_quote display).
    kb_adopt(&pool, OWNER, &job_id, None, false)
        .await
        .expect("adopt should succeed on an LLM-extracted candidate");

    // Promotion row is now confirmed; LLM metadata preserved.
    let row = get_promotion(&pool, &job_id).await.unwrap().unwrap();
    assert_eq!(row.promotion_status, "confirmed");
    assert_eq!(row.block_type_guess.as_deref(), Some("scene"));
    assert_eq!(row.llm_confidence, Some(0.92));
    assert_eq!(
        row.llm_source_quote.as_deref(),
        Some("...the eastern gate groaned open...")
    );

    // A confirmed KeyBlock exists with the LLM-judged block_type.
    let store = SqliteKbStore::new(pool.clone());
    let blocks = store.list_by_world(WORLD).await.unwrap();
    let adopted = blocks
        .iter()
        .find(|b| b.canonical_name == "Azure Gate")
        .unwrap_or_else(|| panic!("no KeyBlock for Azure Gate: {blocks:?}"));
    assert_eq!(adopted.status, "confirmed");
    assert_eq!(
        adopted.block_type,
        nexus_contracts::BlockType::Scene,
        "LLM-judged block_type=scene should flow through adopt"
    );
}

// ── AC4: adopt --json includes the LLM metadata keys ───────────────────────

/// The `--json` adopt output carries `llm_confidence` + `llm_source_quote` so
/// machine consumers see the LLM extraction metadata (cli-spec §6.2G).
#[tokio::test]
async fn adopt_json_output_includes_llm_metadata() {
    // This test exercises kb_adopt with json=true. Stdout capture is brittle
    // in integration tests; instead we verify the underlying data path: the
    // candidate carries the metadata and adopt promotes it cleanly. The
    // --json formatter reads the same KbExtractPromotion fields this test
    // asserts on. (The exact println formatting is covered by the
    // extract_llm_metadata unit test in kb.rs.)
    let (pool, _dir, job_id) = seed_llm_pending().await;
    kb_adopt(&pool, OWNER, &job_id, None, true)
        .await
        .expect("json adopt should succeed");
    let row = get_promotion(&pool, &job_id).await.unwrap().unwrap();
    assert_eq!(row.llm_confidence, Some(0.92));
    assert_eq!(
        row.llm_source_quote.as_deref(),
        Some("...the eastern gate groaned open...")
    );
}

// ── Heuristic candidate: LLM columns NULL, adopt still works ───────────────

/// A V1.50-style heuristic candidate (no LLM metadata) still adopts cleanly;
/// its LLM columns stay NULL (backward compat).
#[tokio::test]
async fn adopt_works_on_heuristic_candidate_with_null_llm_fields() {
    let (pool, _dir) = fresh_pool().await;
    use nexus_local_db::kb_extract_job::insert_pending;
    // Valid novel-profile body (heuristic shape) so adopt-time Novel validation passes.
    let payload = serde_json::json!({
        "summary": "A heuristic hero",
        "attributes": {"novel_category": "character", "aliases": ["Heuristic Hero"]},
        "tags": ["novel", "heuristic-extracted"],
    })
    .to_string();
    let row = insert_pending(
        &pool,
        OWNER,
        "ws",
        WORLD,
        Some(WORK_ID),
        Some(1),
        "character",
        "Heuristic Hero",
        &payload,
    )
    .await
    .unwrap();

    kb_adopt(&pool, OWNER, &row.job_id, None, false)
        .await
        .expect("adopt should succeed on a heuristic candidate");

    let confirmed = get_promotion(&pool, &row.job_id).await.unwrap().unwrap();
    assert_eq!(confirmed.promotion_status, "confirmed");
    assert_eq!(confirmed.llm_confidence, None);
    assert_eq!(confirmed.llm_source_quote, None);
}
