//! `memory_soul_narratives` persistence DAO (V1.81).
//!
//! Caches the on-demand Creator-SOUL LLM narrative with stale-invalidation
//! snapshot columns (`fragment_count_at_generation`,
//! `max_fragment_created_at_at_generation`) and a fingerprint-cached
//! distinct-keyword count (`distinct_keyword_count_cache`,
//! `stats_fingerprint`) that avoids streaming keyword JSON on every
//! cached read/poll.

use futures_util::TryStreamExt;
use sqlx::SqlitePool;

use crate::error::LocalDbError;

/// A row from the `memory_soul_narratives` table.
#[derive(Debug, Clone)]
pub struct SoulNarrativeRecord {
    pub creator_id: String,
    pub narrative: String,
    pub generated_at: String,
    pub fragment_count_at_generation: i64,
    pub max_fragment_created_at_at_generation: Option<String>,
    pub distinct_keyword_count_cache: i64,
    pub stats_fingerprint: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Fragment statistics for a creator, used for stale-detection and the
/// insufficient-data gate.
#[derive(Debug, Clone, Default)]
pub struct SoulNarrativeFragmentStats {
    pub fragment_count: i64,
    pub distinct_keyword_count: usize,
    pub max_created_at: Option<String>,
}

/// Read the cached narrative for a creator (if any).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_soul_narrative(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<Option<SoulNarrativeRecord>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT creator_id as "creator_id!", narrative as "narrative!",
                  generated_at as "generated_at!",
                  fragment_count_at_generation as "fragment_count_at_generation!",
                  max_fragment_created_at_at_generation,
                  distinct_keyword_count_cache as "distinct_keyword_count_cache!",
                  stats_fingerprint,
                  created_at as "created_at!", updated_at as "updated_at!"
           FROM memory_soul_narratives WHERE creator_id = ?"#,
        creator_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| SoulNarrativeRecord {
        creator_id: r.creator_id,
        narrative: r.narrative,
        generated_at: r.generated_at,
        fragment_count_at_generation: r.fragment_count_at_generation,
        max_fragment_created_at_at_generation: r.max_fragment_created_at_at_generation,
        distinct_keyword_count_cache: r.distinct_keyword_count_cache,
        stats_fingerprint: r.stats_fingerprint,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

/// Insert or update the cached narrative for a creator.
///
/// Uses `INSERT OR REPLACE` so the handler can call this unconditionally
/// after synthesis.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn upsert_soul_narrative(
    pool: &SqlitePool,
    record: &SoulNarrativeRecord,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        "INSERT OR REPLACE INTO memory_soul_narratives
         (creator_id, narrative, generated_at, fragment_count_at_generation,
          max_fragment_created_at_at_generation,
          distinct_keyword_count_cache, stats_fingerprint,
          created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        record.creator_id,
        record.narrative,
        record.generated_at,
        record.fragment_count_at_generation,
        record.max_fragment_created_at_at_generation,
        record.distinct_keyword_count_cache,
        record.stats_fingerprint,
        record.created_at,
        record.updated_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Build a stats fingerprint from cheap SQL aggregates.
///
/// Format: `"{fragment_count}:{max_created_at}"`. The fingerprint is
/// stable as long as no fragments are added or removed (or their
/// `created_at` changes, which is immutable in practice).
#[must_use]
pub fn build_stats_fingerprint(fragment_count: i64, max_created_at: Option<&str>) -> String {
    format!("{fragment_count}:{}", max_created_at.unwrap_or(""))
}

/// Update only the stats-cache columns on an existing narrative row.
///
/// Uses a targeted UPDATE so the narrative text and generation metadata
/// are not touched.
async fn update_stats_cache(
    pool: &SqlitePool,
    creator_id: &str,
    distinct_keyword_count: i64,
    fingerprint: &str,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        "UPDATE memory_soul_narratives
         SET distinct_keyword_count_cache = ?, stats_fingerprint = ?
         WHERE creator_id = ?",
        distinct_keyword_count,
        fingerprint,
        creator_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Compute the distinct keyword count via early-exit streaming scan.
///
/// Streams keyword rows one at a time, decoding JSON, accumulating into
/// a `HashSet`. Early-exits at the gate threshold (20 distinct) then
/// drains remaining rows for an exact count. This is sound — no
/// under-count — but proportional to total fragments, so it should only
/// run when fragments have actually changed.
async fn compute_distinct_keyword_count(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<usize, LocalDbError> {
    const DISTINCT_KEYWORD_THRESHOLD: usize = 20;

    let mut stream = sqlx::query!(
        r#"SELECT keywords as "keywords!: String" FROM memory_fragments WHERE creator_id = ? ORDER BY created_at DESC"#,
        creator_id
    )
    .fetch(pool);

    let mut distinct: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(row) = stream.try_next().await? {
        if let Ok(keywords) = serde_json::from_str::<Vec<String>>(&row.keywords) {
            for kw in keywords {
                distinct.insert(kw);
            }
        }
        if distinct.len() >= DISTINCT_KEYWORD_THRESHOLD {
            // Drain remaining rows for exact count.
            while let Some(row) = stream.try_next().await? {
                if let Ok(keywords) = serde_json::from_str::<Vec<String>>(&row.keywords) {
                    for kw in keywords {
                        distinct.insert(kw);
                    }
                }
            }
            break;
        }
    }

    Ok(distinct.len())
}

/// Compute fragment statistics for stale-detection and the insufficient-data gate.
///
/// Uses SQL aggregates for `fragment_count` and `max_created_at` to avoid
/// materializing all rows. The `distinct_keyword_count` is served from a
/// fingerprint cache on `memory_soul_narratives`:
///
/// - Builds a fingerprint from the cheap aggregates (`"{count}:{max_created_at}"`).
/// - If the fingerprint matches the cached `stats_fingerprint`, returns the
///   cached `distinct_keyword_count_cache` immediately — **no keyword JSON
///   decode, no streaming scan**.
/// - If the fingerprint differs (fragments changed) or no cache row exists,
///   computes the distinct count soundly via early-exit streaming, then
///   updates the cache.
///
/// This resolves both W-QC3-001 (cached reads pay only 2 SQL aggregates)
/// and W-QC3-003 (when the count IS computed, it's the sound early-exit
/// streaming — no under-count).
///
/// Returns `SoulNarrativeFragmentStats` with the computed statistics.
///
/// # Errors
///
/// Returns `LocalDbError` if any database query fails.
pub async fn soul_narrative_fragment_stats(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<SoulNarrativeFragmentStats, LocalDbError> {
    // 1. Cheap SQL aggregates — always O(1) index scan, no row materialization.
    let fragment_count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "count!: i64" FROM memory_fragments WHERE creator_id = ?"#,
        creator_id
    )
    .fetch_one(pool)
    .await?;

    let max_created_at: Option<String> = sqlx::query_scalar!(
        r#"SELECT MAX(created_at) as "max_created_at?: String" FROM memory_fragments WHERE creator_id = ?"#,
        creator_id
    )
    .fetch_one(pool)
    .await?;

    // 2. Build fingerprint from cheap aggregates.
    let fingerprint = build_stats_fingerprint(fragment_count, max_created_at.as_deref());

    // 3. Check the fingerprint cache.
    let cached = get_soul_narrative(pool, creator_id).await?;

    if let Some(ref c) = cached {
        if c.stats_fingerprint.as_deref() == Some(&fingerprint) {
            // Fingerprint match → fragments unchanged since last compute.
            // Return cached distinct count — NO keyword streaming/decode.
            return Ok(SoulNarrativeFragmentStats {
                fragment_count,
                distinct_keyword_count: usize::try_from(c.distinct_keyword_count_cache)
                    .unwrap_or(0),
                max_created_at,
            });
        }
    }

    // 4. Fingerprint mismatch or no cache row → compute soundly.
    let distinct_keyword_count = compute_distinct_keyword_count(pool, creator_id).await?;

    // 5. Update cache if a narrative row exists.
    if cached.is_some() {
        update_stats_cache(
            pool,
            creator_id,
            i64::try_from(distinct_keyword_count).unwrap_or(0),
            &fingerprint,
        )
        .await?;
    }

    Ok(SoulNarrativeFragmentStats {
        fragment_count,
        distinct_keyword_count,
        max_created_at,
    })
}
