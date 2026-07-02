//! V1.82 per-World SOUL narrative migration + DAO tests.
//!
//! Verifies:
//! - The composite-key migration preserves existing V1.81 Creator-level rows
//!   as `world_id = NULL`.
//! - The partial UNIQUE index blocks duplicate Creator-level rows.
//! - Per-World stats are distinct from the Creator-level whole.

#![allow(clippy::unwrap_used)]

use nexus_local_db::{memory_fragment, open_pool, run_migrations, soul_narrative_fragment_stats};

/// Create the pre-V1.81 `memory_soul_narratives` table (as created by migration
/// 20260701) and seed one Creator-level narrative row. Running the full
/// migration chain from this state exercises the 20260702 stats-cache add,
/// the 20260703 nullable-narrative recreation, and the 20260704 composite-key
/// recreation, proving the V1.81 row survives all of them.
async fn seed_v181_schema(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only schema setup for migration survival verification.
    sqlx::query(
        "CREATE TABLE memory_soul_narratives (
            creator_id TEXT NOT NULL PRIMARY KEY,
            narrative TEXT NOT NULL,
            generated_at TEXT NOT NULL,
            fragment_count_at_generation INTEGER NOT NULL,
            max_fragment_created_at_at_generation TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO memory_soul_narratives
         (creator_id, narrative, generated_at, fragment_count_at_generation,
          max_fragment_created_at_at_generation, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ctr_v181_survivor")
    .bind("V1.81 creator narrative text")
    .bind("2026-07-01T00:00:00Z")
    .bind(15_i64)
    .bind("2026-07-01T00:00:00Z")
    .bind("2026-07-01T00:00:00Z")
    .bind("2026-07-01T00:00:00Z")
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn v181_creator_narrative_survives_as_null_world_id() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = open_pool(&db_path).await.unwrap();

    seed_v181_schema(&pool).await;
    run_migrations(&pool).await.unwrap();

    // The V1.81 row should now live at (creator_id, world_id=NULL).
    // SAFETY: test-only direct query.
    let row: (Option<String>, String) = sqlx::query_as(
        "SELECT world_id, narrative FROM memory_soul_narratives WHERE creator_id = ?",
    )
    .bind("ctr_v181_survivor")
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(
        row.0.is_none(),
        "V1.81 row must become Creator-level (NULL world_id)"
    );
    assert_eq!(row.1, "V1.81 creator narrative text");
}

#[tokio::test]
async fn partial_unique_index_blocks_duplicate_creator_level_row() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = open_pool(&db_path).await.unwrap();

    seed_v181_schema(&pool).await;
    run_migrations(&pool).await.unwrap();

    // A second NULL-world_id row for the same creator must fail.
    let result = sqlx::query(
        "INSERT INTO memory_soul_narratives
         (creator_id, world_id, narrative, generated_at, fragment_count_at_generation,
          max_fragment_created_at_at_generation, distinct_keyword_count_cache,
          stats_fingerprint, created_at, updated_at)
         VALUES (?, NULL, ?, NULL, 0, NULL, 0, NULL, ?, ?)",
    )
    .bind("ctr_v181_survivor")
    .bind("duplicate creator narrative")
    .bind("2026-07-02T00:00:00Z")
    .bind("2026-07-02T00:00:00Z")
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "partial UNIQUE index must reject duplicate Creator-level row"
    );

    // A world-bearing row for the same creator is allowed.
    let result = sqlx::query(
        "INSERT INTO memory_soul_narratives
         (creator_id, world_id, narrative, generated_at, fragment_count_at_generation,
          max_fragment_created_at_at_generation, distinct_keyword_count_cache,
          stats_fingerprint, created_at, updated_at)
         VALUES (?, ?, ?, NULL, 0, NULL, 0, NULL, ?, ?)",
    )
    .bind("ctr_v181_survivor")
    .bind("wld_alpha")
    .bind("world alpha narrative")
    .bind("2026-07-02T00:00:00Z")
    .bind("2026-07-02T00:00:00Z")
    .execute(&pool)
    .await;

    assert!(
        result.is_ok(),
        "per-World row must be allowed alongside Creator-level row"
    );
}

async fn insert_fragment(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    world_id: Option<&str>,
    keywords: &[&str],
    idx: usize,
) {
    let keywords_json = serde_json::to_string(&keywords).unwrap();
    let record = memory_fragment::MemoryFragmentRecord {
        fragment_id: format!("frag_{creator_id}_{idx:04}"),
        session_id: format!("sess_{creator_id}_{idx:04}"),
        creator_id: creator_id.to_string(),
        keywords: keywords_json,
        summary: format!("summary {idx}"),
        created_at: chrono::Utc::now().to_rfc3339(),
        ttl: None,
        world_id: world_id.map(std::string::ToString::to_string),
    };
    memory_fragment::create_fragment(pool, &record)
        .await
        .unwrap();
}

#[tokio::test]
async fn per_world_stats_are_distinct_from_creator_whole() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = open_pool(&db_path).await.unwrap();
    run_migrations(&pool).await.unwrap();

    let creator_id = "ctr_world_stats";
    let world_a = "wld_a";
    let world_b = "wld_b";

    // World A: 12 fragments, 12 distinct keywords.
    for i in 0..12 {
        let kw = format!("a_kw_{i}");
        insert_fragment(&pool, creator_id, Some(world_a), &[&kw], i).await;
    }

    // World B: 8 fragments, 8 distinct keywords.
    for i in 0..8 {
        let kw = format!("b_kw_{i}");
        insert_fragment(&pool, creator_id, Some(world_b), &[&kw], i + 100).await;
    }

    // 5 Creator-core fragments (no world_id), 5 distinct keywords.
    for i in 0..5 {
        let kw = format!("core_kw_{i}");
        insert_fragment(&pool, creator_id, None, &[&kw], i + 200).await;
    }

    // Creator whole: 25 fragments, threshold-saturated count (20).
    let (whole_stats, _) = soul_narrative_fragment_stats(&pool, creator_id, None)
        .await
        .unwrap();
    assert_eq!(whole_stats.fragment_count, 25);
    assert_eq!(whole_stats.distinct_keyword_count, 20);

    // World A subset: 12 fragments, 12 distinct keywords (below threshold → exact).
    let (a_stats, _) = soul_narrative_fragment_stats(&pool, creator_id, Some(world_a))
        .await
        .unwrap();
    assert_eq!(a_stats.fragment_count, 12);
    assert_eq!(a_stats.distinct_keyword_count, 12);

    // World B subset: 8 fragments, 8 distinct keywords (below threshold → exact).
    let (b_stats, _) = soul_narrative_fragment_stats(&pool, creator_id, Some(world_b))
        .await
        .unwrap();
    assert_eq!(b_stats.fragment_count, 8);
    assert_eq!(b_stats.distinct_keyword_count, 8);
}
