//! `memory_soul_narratives` persistence DAO (V1.81).
//!
//! Caches the on-demand Creator-SOUL LLM narrative with stale-invalidation
//! snapshot columns (`fragment_count_at_generation`,
//! `max_fragment_created_at_at_generation`).

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
          max_fragment_created_at_at_generation, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        record.creator_id,
        record.narrative,
        record.generated_at,
        record.fragment_count_at_generation,
        record.max_fragment_created_at_at_generation,
        record.created_at,
        record.updated_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Compute fragment statistics for stale-detection and the insufficient-data gate.
///
/// Uses SQL aggregates for `fragment_count` and `max_created_at` to avoid
/// materializing all rows. The `distinct_keyword_count` uses an early-exit
/// streaming scan: it streams keyword rows one at a time, accumulating distinct
/// keywords into a `HashSet`. The scan stops as soon as 20 distinct keywords are
/// found (the insufficient-data gate threshold), but continues to EOF for an
/// exact count for the response field. This is sound — no under-count, no
/// bounded LIMIT — and proportional: creators with ≥20 distinct keywords stop
/// early; creators with <20 distinct have few fragments in the local
/// small-queue model.
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
    /// Insufficient-data gate threshold for distinct keywords (≤20).
    const DISTINCT_KEYWORD_THRESHOLD: usize = 20;

    // fragment_count: SQL aggregate — no row materialization.
    let fragment_count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "count!: i64" FROM memory_fragments WHERE creator_id = ?"#,
        creator_id
    )
    .fetch_one(pool)
    .await?;

    // max_created_at: SQL aggregate — no row materialization.
    let max_created_at: Option<String> = sqlx::query_scalar!(
        r#"SELECT MAX(created_at) as "max_created_at?: String" FROM memory_fragments WHERE creator_id = ?"#,
        creator_id
    )
    .fetch_one(pool)
    .await?;

    // distinct_keyword_count: early-exit streaming scan.
    // Stream rows one at a time with `.fetch()` (NOT `.fetch_all()`); stop
    // scanning as soon as the distinct-keyword set reaches the gate threshold.
    // Then drain remaining rows for an exact count for the response field.
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
        // Early exit: the gate threshold is 20 distinct keywords.
        // Once we reach it, the gate result is authoritative (passes).
        // Drain remaining rows for exact count for the response field.
        if distinct.len() >= DISTINCT_KEYWORD_THRESHOLD {
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

    Ok(SoulNarrativeFragmentStats {
        fragment_count,
        distinct_keyword_count: distinct.len(),
        max_created_at,
    })
}
