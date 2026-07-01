//! Regression tests for R-V181P0-QC3-W003: sound distinct-keyword count,
//! and fingerprint-cache correctness for W-QC3-001 (no keyword JSON decode
//! on cached reads with unchanged fragments).
//!
//! Verifies:
//! - Sound distinct-keyword count (early-exit streaming, no under-count).
//! - Fingerprint cache: cached reads with unchanged fragments return the
//!   cached count without streaming keyword JSON.
//! - Fingerprint cache: when fragments change, the count is recomputed
//!   soundly and the cache is updated.

#![allow(clippy::unwrap_used)]

use nexus_local_db::{
    build_stats_fingerprint, init_pool, memory_fragment, soul_narrative_fragment_stats,
};

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

// ── Fingerprint cache regression tests ──────────────────────────────

/// Seed a narrative cache row with a known distinct-keyword count and
/// fingerprint, so the fingerprint-cache read path can be tested.
async fn seed_narrative_cache(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    distinct_keyword_count_cache: i64,
    stats_fingerprint: &str,
) {
    let now = chrono::Utc::now().to_rfc3339();
    // SAFETY: test-only direct insert — compile-time macro not applicable
    // because this is a test helper, not a production query.
    sqlx::query(
        "INSERT OR REPLACE INTO memory_soul_narratives
         (creator_id, narrative, generated_at, fragment_count_at_generation,
          max_fragment_created_at_at_generation,
          distinct_keyword_count_cache, stats_fingerprint,
          created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(creator_id)
    .bind("cached narrative text")
    .bind(&now)
    .bind(0_i64) // fragment_count_at_generation — doesn't matter for stats cache test
    .bind(None::<String>) // max_fragment_created_at_at_generation
    .bind(distinct_keyword_count_cache)
    .bind(stats_fingerprint)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();
}

/// Cached read with UNCHANGED fragments: the fingerprint matches, so
/// `soul_narrative_fragment_stats` returns the cached distinct count
/// without scanning keyword JSON.
#[tokio::test]
async fn fingerprint_cache_unchanged_fragments_returns_cached_count() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_pool(&db_path).await.unwrap();
    let creator_id = "ctr_cache_hit";

    // Insert 25 fragments (25 distinct keywords).
    for i in 0..25 {
        let kw = format!("kw_{i}");
        insert_fragment(&pool, creator_id, &[&kw], i).await;
    }

    // Query the actual max_created_at from the DB so the fingerprint matches.
    let max_created_at: Option<String> =
        sqlx::query_scalar("SELECT MAX(created_at) FROM memory_fragments WHERE creator_id = ?")
            .bind(creator_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // Seed a cache row with a known count (999) and matching fingerprint.
    let fingerprint = build_stats_fingerprint(25, max_created_at.as_deref());
    seed_narrative_cache(&pool, creator_id, 999, &fingerprint).await;

    let stats = soul_narrative_fragment_stats(&pool, creator_id)
        .await
        .unwrap();

    // Fingerprint matches → should return the CACHED count (999), not 25.
    assert_eq!(stats.fragment_count, 25);
    assert_eq!(stats.distinct_keyword_count, 999);
    assert!(stats.distinct_keyword_count >= 20);
}

/// Cached read AFTER a new fragment (fingerprint changes) → recomputes
/// soundly and updates the cache with the new count.
#[tokio::test]
async fn fingerprint_cache_changed_fragments_recomputes() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_pool(&db_path).await.unwrap();
    let creator_id = "ctr_cache_miss";

    // Insert 25 fragments (25 distinct keywords).
    for i in 0..25 {
        let kw = format!("kw_{i}");
        insert_fragment(&pool, creator_id, &[&kw], i).await;
    }

    // Seed a cache row with an OLD fingerprint (fragment_count=10).
    let old_fingerprint = build_stats_fingerprint(10, None);
    seed_narrative_cache(&pool, creator_id, 999, &old_fingerprint).await;

    let stats = soul_narrative_fragment_stats(&pool, creator_id)
        .await
        .unwrap();

    // Fingerprint mismatch → should recompute, returning the real count (25).
    assert_eq!(stats.fragment_count, 25);
    assert_eq!(stats.distinct_keyword_count, 25);
    assert!(stats.distinct_keyword_count >= 20);

    // Second call with unchanged fragments → should now return cached 25.
    let stats2 = soul_narrative_fragment_stats(&pool, creator_id)
        .await
        .unwrap();
    assert_eq!(stats2.distinct_keyword_count, 25);
}

/// No cache row exists (ungenerated case) → computes soundly, does not
/// create a cache row (only upsert_soul_narrative creates rows).
#[tokio::test]
async fn fingerprint_cache_no_row_computes_soundly() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_pool(&db_path).await.unwrap();
    let creator_id = "ctr_no_cache";

    // Insert 22 fragments (22 distinct keywords) — no narrative row.
    for i in 0..22 {
        let kw = format!("kw_{i}");
        insert_fragment(&pool, creator_id, &[&kw], i).await;
    }

    let stats = soul_narrative_fragment_stats(&pool, creator_id)
        .await
        .unwrap();

    // Should compute from scratch — no cache to serve from.
    assert_eq!(stats.fragment_count, 22);
    assert_eq!(stats.distinct_keyword_count, 22);
    assert!(stats.distinct_keyword_count >= 20);
}

/// Cache with zero fragments: fingerprint "0:" matches → returns cached 0.
#[tokio::test]
async fn fingerprint_cache_zero_fragments_returns_cached_zero() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = init_pool(&db_path).await.unwrap();
    let creator_id = "ctr_cache_zero";

    // No fragments inserted.
    let fingerprint = build_stats_fingerprint(0, None);
    seed_narrative_cache(&pool, creator_id, 0, &fingerprint).await;

    let stats = soul_narrative_fragment_stats(&pool, creator_id)
        .await
        .unwrap();

    assert_eq!(stats.fragment_count, 0);
    assert_eq!(stats.distinct_keyword_count, 0);
    assert!(stats.distinct_keyword_count < 20);
}
