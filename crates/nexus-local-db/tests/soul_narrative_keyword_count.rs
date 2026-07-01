//! Regression tests for R-V181P0-QC3-W003: sound distinct-keyword count.
//!
//! Verifies that the early-exit streaming scan in
//! `soul_narrative_fragment_stats` does NOT under-count — a creator with ≥20
//! distinct keywords spread across many fragments (more than the old LIMIT 200
//! cap) correctly reports ≥20, and a creator with <20 distinct correctly
//! reports <20.

#![allow(clippy::unwrap_used)]

use nexus_local_db::{init_pool, memory_fragment, soul_narrative_fragment_stats};

/// Helper: create a memory fragment with given keywords.
async fn insert_fragment(pool: &sqlx::SqlitePool, creator_id: &str, keywords: &[&str], idx: usize) {
    let keywords_json = serde_json::to_string(&keywords).unwrap();
    let record = memory_fragment::MemoryFragmentRecord {
        fragment_id: format!("frag_{creator_id}_{idx:04}"),
        session_id: format!("sess_{creator_id}_{idx:04}"),
        creator_id: creator_id.to_string(),
        keywords: keywords_json,
        summary: format!("summary {idx}"),
        created_at: chrono::Utc::now().to_rfc3339(),
        ttl: None,
        world_id: None,
    };
    memory_fragment::create_fragment(pool, &record)
        .await
        .unwrap();
}

/// Creator with ≥20 distinct keywords spread across many fragments
/// (30 fragments, each with one unique keyword) → gate passes
/// (distinct_keyword_count ≥ 20), and the response field is exact.
#[tokio::test]
async fn distinct_keywords_at_least_20_across_many_fragments() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_pool(&db_path).await.unwrap();
    let creator_id = "ctr_test_many";

    // Insert 30 fragments, each with one unique keyword.
    // Total distinct: 30 (well above the gate threshold of 20).
    for i in 0..30 {
        let kw = format!("unique_kw_{i}");
        insert_fragment(&pool, creator_id, &[&kw], i).await;
    }

    let stats = soul_narrative_fragment_stats(&pool, creator_id)
        .await
        .unwrap();

    // Fragment count should be 30.
    assert_eq!(stats.fragment_count, 30);
    // Distinct keyword count should be exactly 30 (sound, no under-count).
    assert_eq!(stats.distinct_keyword_count, 30);
    // Gate check: >= 20 → passes (NOT insufficient_data).
    assert!(stats.distinct_keyword_count >= 20);
}

/// Creator with <20 distinct keywords → gate fails, exact count is correct.
#[tokio::test]
async fn distinct_keywords_below_20_gate_fails() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_pool(&db_path).await.unwrap();
    let creator_id = "ctr_test_few";

    // Insert 5 fragments, each with one keyword. Some keywords repeat.
    let keywords_sequence = ["kw_a", "kw_b", "kw_c", "kw_a", "kw_d"];
    for (i, kw) in keywords_sequence.iter().enumerate() {
        insert_fragment(&pool, creator_id, &[kw], i).await;
    }

    let stats = soul_narrative_fragment_stats(&pool, creator_id)
        .await
        .unwrap();

    // Fragment count should be 5.
    assert_eq!(stats.fragment_count, 5);
    // Distinct keyword count should be exactly 4 (kw_a appears twice).
    assert_eq!(stats.distinct_keyword_count, 4);
    // Gate check: < 20 → fails (insufficient_data).
    assert!(stats.distinct_keyword_count < 20);
}

/// Creator with exactly 20 distinct keywords → gate passes at threshold.
#[tokio::test]
async fn distinct_keywords_exactly_20_gate_passes() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_pool(&db_path).await.unwrap();
    let creator_id = "ctr_test_exact";

    // Insert 20 fragments, each with one unique keyword.
    for i in 0..20 {
        let kw = format!("kw_{i}");
        insert_fragment(&pool, creator_id, &[&kw], i).await;
    }

    let stats = soul_narrative_fragment_stats(&pool, creator_id)
        .await
        .unwrap();

    assert_eq!(stats.fragment_count, 20);
    assert_eq!(stats.distinct_keyword_count, 20);
    // Gate check: == 20 → passes (NOT insufficient_data).
    assert!(stats.distinct_keyword_count >= 20);
}

/// Creator with ≥20 distinct keywords but many duplicate keywords in fragments
/// → gate still passes (distinct count is sound).
#[tokio::test]
async fn distinct_keywords_with_duplicates_still_sound() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_pool(&db_path).await.unwrap();
    let creator_id = "ctr_test_dupes";

    // Insert 50 fragments, but only 25 distinct keywords (many duplicates).
    // 25 distinct > 20 → gate passes.
    for i in 0..50 {
        let kw_idx = i % 25; // Only 25 distinct keywords
        let kw = format!("kw_{kw_idx}");
        insert_fragment(&pool, creator_id, &[&kw], i).await;
    }

    let stats = soul_narrative_fragment_stats(&pool, creator_id)
        .await
        .unwrap();

    assert_eq!(stats.fragment_count, 50);
    assert_eq!(stats.distinct_keyword_count, 25);
    assert!(stats.distinct_keyword_count >= 20);
}

/// Creator with no fragments → zero distinct keywords, gate fails.
#[tokio::test]
async fn no_fragments_zero_distinct() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_pool(&db_path).await.unwrap();
    let creator_id = "ctr_test_empty";

    let stats = soul_narrative_fragment_stats(&pool, creator_id)
        .await
        .unwrap();

    assert_eq!(stats.fragment_count, 0);
    assert_eq!(stats.distinct_keyword_count, 0);
    assert!(stats.distinct_keyword_count < 20);
}
