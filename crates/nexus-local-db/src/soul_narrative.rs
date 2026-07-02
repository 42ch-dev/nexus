//! `memory_soul_narratives` persistence DAO (V1.81).
//!
//! Caches the on-demand Creator-SOUL LLM narrative with stale-invalidation
//! snapshot columns (`fragment_count_at_generation`,
//! `max_fragment_created_at_at_generation`) and a fingerprint-cached
//! distinct-keyword count (`distinct_keyword_count_cache`,
//! `stats_fingerprint`) that avoids streaming keyword JSON on every
//! cached read/poll.

use futures_util::TryStreamExt;
use sqlx::{Row, SqlitePool};

use crate::error::LocalDbError;

/// A row from the `memory_soul_narratives` table.
///
/// `narrative` and `generated_at` are `None` for stats-only rows (fingerprint
/// cache for above-gate ungenerated creators).
#[derive(Debug, Clone)]
pub struct SoulNarrativeRecord {
    pub creator_id: String,
    /// `None` = Creator-level (whole) narrative; `Some(w)` = per-World narrative.
    pub world_id: Option<String>,
    pub narrative: Option<String>,
    pub generated_at: Option<String>,
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

/// Read the cached narrative for a `(creator_id, world_id)` scope.
///
/// `world_id = None` reads the Creator-level (whole) narrative; `Some(w)` reads
/// the per-World narrative for that world's fragment subset.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_soul_narrative(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: Option<&str>,
) -> Result<Option<SoulNarrativeRecord>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT creator_id as "creator_id!", world_id, narrative, generated_at,
                  fragment_count_at_generation as "fragment_count_at_generation!",
                  max_fragment_created_at_at_generation,
                  distinct_keyword_count_cache as "distinct_keyword_count_cache!",
                  stats_fingerprint,
                  created_at as "created_at!", updated_at as "updated_at!"
           FROM memory_soul_narratives WHERE creator_id = ? AND world_id IS ?"#,
        creator_id,
        world_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| SoulNarrativeRecord {
        creator_id: r.creator_id,
        world_id: r.world_id,
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

/// Insert or update the cached narrative for a `(creator_id, world_id)` scope.
///
/// Uses `INSERT OR REPLACE` so the handler can call this unconditionally
/// after synthesis. `world_id` is bound consistently (NULL for Creator-level)
/// so the composite PK / partial UNIQUE index conflict target fires correctly.
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
         (creator_id, world_id, narrative, generated_at, fragment_count_at_generation,
          max_fragment_created_at_at_generation,
          distinct_keyword_count_cache, stats_fingerprint,
          created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        record.creator_id,
        record.world_id,
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

/// Persist stats cache columns, creating a stats-only row if none exists.
///
/// When `cached` is `Some`, does a targeted UPDATE that leaves narrative and
/// generation metadata untouched. When `cached` is `None` (above-gate
/// ungenerated creator), inserts a stats-only row with `narrative = NULL`
/// and `generated_at = NULL` so the next poll hits the fingerprint cache
/// instead of re-scanning keyword JSON.
async fn update_stats_cache(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: Option<&str>,
    distinct_keyword_count: i64,
    fingerprint: &str,
    cached: Option<&SoulNarrativeRecord>,
) -> Result<(), LocalDbError> {
    if cached.is_some() {
        // Targeted UPDATE — don't touch narrative/generation metadata.
        sqlx::query!(
            "UPDATE memory_soul_narratives
             SET distinct_keyword_count_cache = ?, stats_fingerprint = ?
             WHERE creator_id = ? AND world_id IS ?",
            distinct_keyword_count,
            fingerprint,
            creator_id,
            world_id
        )
        .execute(pool)
        .await?;
    } else {
        // Stats-only INSERT — narrative/generated_at are NULL.
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query!(
            "INSERT OR REPLACE INTO memory_soul_narratives
             (creator_id, world_id, narrative, generated_at, fragment_count_at_generation,
              max_fragment_created_at_at_generation,
              distinct_keyword_count_cache, stats_fingerprint,
              created_at, updated_at)
             VALUES (?, ?, NULL, NULL, 0, NULL, ?, ?, ?, ?)",
            creator_id,
            world_id,
            distinct_keyword_count,
            fingerprint,
            now,
            now
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Compute the distinct keyword count via early-exit streaming scan.
///
/// Streams keyword rows one at a time, decoding JSON, accumulating into
/// a `HashSet`. Stops as soon as the gate threshold (20 distinct) is
/// reached and returns the threshold as a **saturated count**.
///
/// # Threshold-saturated semantics
///
/// The SOUL insufficient-data gate only needs to know whether the count
/// is `>= 20`. When the streaming distinct set reaches 20 we stop decoding
/// and return 20. This bounds the recompute cost by the number of rows
/// needed to observe 20 distinct keywords, not the total fragment count.
/// Counts below the threshold scan to EOF and return the exact value.
/// Callers must treat `20` as "at least 20" rather than an exact count.
async fn compute_distinct_keyword_count(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: Option<&str>,
) -> Result<usize, LocalDbError> {
    const DISTINCT_KEYWORD_THRESHOLD: usize = 20;

    // SAFETY: dynamic SQL — the optional world_id filter produces two static
    // WHERE-clause variants. All values are parameterized with .bind() to
    // prevent injection. A compile-time macro cannot be used because the two
    // variants yield distinct anonymous record types, which cannot unify in
    // a single stream variable.
    let mut stream = world_id.map_or_else(
        || {
            sqlx::query(
                "SELECT keywords FROM memory_fragments \
                 WHERE creator_id = ? ORDER BY created_at DESC",
            )
            .bind(creator_id)
            .fetch(pool)
        },
        |wid| {
            sqlx::query(
                "SELECT keywords FROM memory_fragments \
                 WHERE creator_id = ? AND world_id = ? ORDER BY created_at DESC",
            )
            .bind(creator_id)
            .bind(wid)
            .fetch(pool)
        },
    );

    let mut distinct: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(row) = stream.try_next().await? {
        let keywords_json: String = row.get("keywords");
        if let Ok(keywords) = serde_json::from_str::<Vec<String>>(&keywords_json) {
            for kw in keywords {
                distinct.insert(kw);
            }
        }
        if distinct.len() >= DISTINCT_KEYWORD_THRESHOLD {
            // Threshold-saturated count: the gate only needs `>= 20`, so stop
            // decoding as soon as we have 20 distinct keywords.
            return Ok(DISTINCT_KEYWORD_THRESHOLD);
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
/// The cache row is keyed per `(creator_id, world_id)`. `world_id = None`
/// computes over the Creator-level whole; `Some(w)` computes over that world's
/// fragment subset.
///
/// Returns the computed statistics plus the cached narrative record (if any),
/// so callers can avoid a redundant `get_soul_narrative` call.
///
/// # Errors
///
/// Returns `LocalDbError` if any database query fails.
pub async fn soul_narrative_fragment_stats(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: Option<&str>,
) -> Result<(SoulNarrativeFragmentStats, Option<SoulNarrativeRecord>), LocalDbError> {
    // 1. Cheap SQL aggregates — always O(1) index scan, no row materialization.
    let fragment_count = if let Some(wid) = world_id {
        sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!: i64" FROM memory_fragments WHERE creator_id = ? AND world_id = ?"#,
            creator_id,
            wid
        )
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!: i64" FROM memory_fragments WHERE creator_id = ?"#,
            creator_id
        )
        .fetch_one(pool)
        .await?
    };

    let max_created_at: Option<String> = if let Some(wid) = world_id {
        sqlx::query_scalar!(
            r#"SELECT MAX(created_at) as "max_created_at?: String" FROM memory_fragments WHERE creator_id = ? AND world_id = ?"#,
            creator_id,
            wid
        )
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_scalar!(
            r#"SELECT MAX(created_at) as "max_created_at?: String" FROM memory_fragments WHERE creator_id = ?"#,
            creator_id
        )
        .fetch_one(pool)
        .await?
    };

    // 2. Build fingerprint from cheap aggregates.
    let fingerprint = build_stats_fingerprint(fragment_count, max_created_at.as_deref());

    // 3. Check the fingerprint cache for this (creator, world) scope.
    let cached = get_soul_narrative(pool, creator_id, world_id).await?;

    if let Some(ref c) = cached {
        if c.stats_fingerprint.as_deref() == Some(&fingerprint) {
            // Fingerprint match → fragments unchanged since last compute.
            // Return cached distinct count — NO keyword streaming/decode.
            return Ok((
                SoulNarrativeFragmentStats {
                    fragment_count,
                    distinct_keyword_count: usize::try_from(c.distinct_keyword_count_cache)
                        .unwrap_or(0),
                    max_created_at,
                },
                cached,
            ));
        }
    }

    // 4. Fingerprint mismatch or no cache row → compute soundly.
    let distinct_keyword_count = compute_distinct_keyword_count(pool, creator_id, world_id).await?;

    // 5. Persist stats cache (inserts stats-only row if none exists — G3 fix).
    update_stats_cache(
        pool,
        creator_id,
        world_id,
        i64::try_from(distinct_keyword_count).unwrap_or(0),
        &fingerprint,
        cached.as_ref(),
    )
    .await?;

    // Re-read cache after possible stats-only insert so callers get the
    // freshly-persisted row (G3: fingerprint cache hit on next poll).
    let cached = if cached.is_some() {
        cached
    } else {
        get_soul_narrative(pool, creator_id, world_id).await?
    };

    Ok((
        SoulNarrativeFragmentStats {
            fragment_count,
            distinct_keyword_count,
            max_created_at,
        },
        cached,
    ))
}
